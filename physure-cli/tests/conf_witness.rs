//! `phs conf freeze --witness` and `phs conf verify --run` (conf freeze Phase 3, spec §8): the real binary in a
//! temporary directory of its own, with `PHYSURE_CONF` and `PHYSURE_REQUIRE_FROZEN` removed unless a test sets them.
//! Files are frozen with `--engine-policy release` (the test build names no commit).

use physure_core::frozen::sha256::sha256_hex;
use physure_core::frozen::{
    engine_pairs, parse_value, EngineIdentity, EnginePolicy, EntryInput, FrozenFile, FrozenParts, Pairs, Source, WitnessScript,
    WitnessValue,
};
use physure_core::units::conf::DEFAULT_PHYSURE_CONF;
use physure_script::report::{canonical_witness, evaluate_with_ceilings, DEFAULT_SEED};
use physure_script::Limits;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const FROZEN_FILE_NAME: &str = "physure.frozen.conf";

/// A unique temporary directory, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let path = std::env::temp_dir().join(format!("physure_cli_witness_it_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Dir(path.canonicalize().unwrap())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.path(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, text).unwrap();
        path
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.path(name)).unwrap()
    }

    fn frozen(&self) -> FrozenFile {
        FrozenFile::parse(&self.read(FROZEN_FILE_NAME)).unwrap()
    }

    fn phs(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_phs"));
        command.args(args).current_dir(&self.0).env_remove("PHYSURE_CONF").env_remove("PHYSURE_REQUIRE_FROZEN");
        for (name, value) in env {
            command.env(name, value);
        }
        command.output().expect("phs runs")
    }

    fn conf(&self, args: &[&str]) -> Output {
        let mut all = vec!["conf"];
        all.extend_from_slice(args);
        self.phs(&all, &[])
    }

    /// `phs conf freeze --reason R --engine-policy release` plus `extra`.
    fn freeze(&self, extra: &[&str]) -> Output {
        let mut args = vec!["freeze", "--reason", "witnessed", "--engine-policy", "release"];
        args.extend_from_slice(extra);
        self.conf(&args)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn text(output: &Output) -> String {
    format!("{}{}", stdout(output), String::from_utf8_lossy(&output.stderr))
}

fn json(output: &Output) -> Value {
    serde_json::from_str(&stdout(output)).unwrap_or_else(|e| panic!("not one JSON document ({e}): {}", text(output)))
}

/// The keys of `[Witness]` that start with `prefix`.
fn keys(file: &FrozenFile, prefix: &str) -> Vec<String> {
    file.witnesses().iter().map(|(k, _)| k.to_string()).filter(|k| k.starts_with(prefix)).collect()
}

/// The hash `freeze` records for `text`, computed here as the CLI does (seed 0, the sandbox, the fixed ceilings).
fn expected_hash(text: &str) -> String {
    let report = evaluate_with_ceilings(text, Limits::sandboxed(Duration::from_secs(60)), DEFAULT_SEED, 1_000_000, 1_000);
    assert!(report.ok, "{:?}", report.diagnostics);
    sha256_hex(canonical_witness(&report).as_bytes())
}

// ---- B2: freeze --witness ----

#[test]
fn freeze_embeds_the_battery_and_a_user_witness_and_verify_passes() {
    let dir = Dir::new("freeze_embeds");
    dir.write("validation.phs", "d = 120 m\nt = 8 s\nv = d / t\n");
    let output = dir.freeze(&["--witness", "validation.phs"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let file = dir.frozen();
    assert!(file.verify().is_empty(), "{:?}", file.verify());
    let builtin = keys(&file, "builtin.");
    assert!(builtin.len() >= 38, "{builtin:?}");
    assert_eq!(keys(&file, "user."), ["user.validation"]);
    // the battery first, in id order, then the user witnesses
    let all: Vec<&str> = file.witnesses().iter().map(|(k, _)| k).collect();
    assert_eq!(all.last(), Some(&"user.validation"));
    let mut sorted = builtin.clone();
    sorted.sort();
    assert_eq!(builtin, sorted);
    assert_eq!(
        file.witnesses().get("user.validation"),
        Some(format!("sha256:{}", expected_hash("d = 120 m\nt = 8 s\nv = d / t")).as_str())
    );
    let out = stdout(&output);
    assert!(out.contains(&format!("witnesses: {} built-in, 1 user", builtin.len())), "{out}");
    assert!(out.contains("record this seal outside this directory"), "{out}");
    let verified = dir.conf(&["verify"]);
    assert_eq!(verified.status.code(), Some(0), "{}", text(&verified));
}

#[test]
fn the_battery_is_recorded_with_values_and_without_errors() {
    let dir = Dir::new("freeze_battery");
    let output = dir.freeze(&["--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let file = dir.frozen();
    let builtin = keys(&file, "builtin.");
    assert!(builtin.len() >= 38, "{builtin:?}");
    assert!(keys(&file, "user.").is_empty() && file.witness_scripts().is_empty());
    for (key, value) in file.witnesses().iter() {
        let parsed = parse_value(value).unwrap_or_else(|e| panic!("{key}: {e}"));
        assert!(!matches!(parsed, WitnessValue::Failed(_)), "{key} failed under the embedded catalog: {value}");
    }
    let doc = json(&output);
    assert_eq!(doc["witnesses"]["builtin"], Value::from(builtin.len()));
    assert_eq!(doc["witnesses"]["user"], Value::from(0));
    let history = file.history();
    assert_eq!(history.len(), 1);
    assert!(history[0].diff().ends_with(&format!("; witnesses: {} built-in, 0 user", builtin.len())), "{}", history[0].diff());
}

/// Correction C1: with `PHYSURE_REQUIRE_FROZEN=1` and no frozen file the first file can still be made, and its
/// witnesses are computed under it (not errors of "a frozen file is required").
#[test]
fn freeze_works_under_require_frozen_with_no_file() {
    let dir = Dir::new("freeze_require");
    dir.write("check.phs", "x = 3 m\nx * 2");
    let output = dir.phs(
        &["conf", "freeze", "--reason", "first", "--engine-policy", "release", "--witness", "check.phs"],
        &[("PHYSURE_REQUIRE_FROZEN", "1")],
    );
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let file = dir.frozen();
    for (key, value) in file.witnesses().iter() {
        assert!(!value.starts_with("failed="), "{key} = {value}");
    }
    assert_eq!(file.witnesses().get("user.check"), Some(format!("sha256:{}", expected_hash("x = 3 m\nx * 2")).as_str()));
}

/// Correction C13: what is evaluated is the normalised text, the one the file keeps.
#[test]
fn the_script_is_kept_normalised_and_the_normalised_text_is_what_is_evaluated() {
    let dir = Dir::new("freeze_crlf");
    let raw = "s = \"first line\r\nsecond line\"\r\nx = 2 m\r\n\r\n";
    dir.write("crlf.phs", raw);
    let output = dir.freeze(&["--witness", "crlf.phs"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let file = dir.frozen();
    let normalised = WitnessScript::new("crlf", raw).unwrap();
    assert_eq!(file.witness_scripts(), [normalised.clone()]);
    assert_eq!(normalised.text(), "s = \"first line\nsecond line\"\nx = 2 m");
    let recorded = file.witnesses().get("user.crlf").unwrap().to_string();
    assert_eq!(recorded, format!("sha256:{}", expected_hash(normalised.text())));
    assert_ne!(recorded, format!("sha256:{}", expected_hash(raw)), "the raw text gives another result: the test means something");
}

#[test]
fn repeated_witnesses_are_kept_in_the_order_given() {
    let dir = Dir::new("freeze_two");
    dir.write("b_second.phs", "2 m");
    dir.write("a_first.phs", "1 m");
    let output = dir.freeze(&["--witness", "b_second.phs", "--witness", "a_first.phs", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let file = dir.frozen();
    assert_eq!(keys(&file, "user."), ["user.b_second", "user.a_first"]);
    let names: Vec<&str> = file.witness_scripts().iter().map(|s| s.name()).collect();
    assert_eq!(names, ["b_second", "a_first"]);
    assert_eq!(json(&output)["witnesses"]["user"], Value::from(2));
}

#[test]
fn a_script_with_an_error_is_refused_exit_2_with_the_findings() {
    let dir = Dir::new("freeze_error");
    dir.write("wrong.phs", "x = 1 m\ny = x + 1 s\n");
    let output = dir.freeze(&["--witness", "wrong.phs"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    let all = text(&output);
    assert!(all.contains("wrong") && all.contains("dimension-mismatch") && all.contains("line 2"), "{all}");
    assert!(!dir.path(FROZEN_FILE_NAME).exists(), "nothing written");
}

#[test]
fn unusable_scripts_are_refused_exit_2_before_anything_is_written() {
    let dir = Dir::new("freeze_unusable");
    dir.write("ok.phs", "1 m");
    dir.write("bad name.phs", "1 m");
    dir.write("one/same.phs", "1 m");
    dir.write("two/same.phs", "2 m");
    fs::write(dir.path("binary.phs"), [0xff, 0xfe, 0x00]).unwrap();
    fs::write(dir.path("large.phs"), vec![b'1'; (1 << 20) + 1]).unwrap();
    dir.write("framing.phs", "x = 1\n@@ witness other\n");
    let cases: [(&[&str], &str); 6] = [
        (&["--witness", "bad name.phs"], "file stem"),
        (&["--witness", "one/same.phs", "--witness", "two/same.phs"], "same"),
        (&["--witness", "missing.phs"], "missing.phs"),
        (&["--witness", "binary.phs"], "UTF-8"),
        (&["--witness", "large.phs"], "at most"),
        (&["--witness", "ok.phs", "--witness", "framing.phs"], "@@"),
    ];
    for (extra, expected) in cases {
        let output = dir.freeze(extra);
        assert_eq!(output.status.code(), Some(2), "{extra:?}: {}", text(&output));
        assert!(text(&output).contains(expected), "{extra:?}: {}", text(&output));
        assert!(!dir.path(FROZEN_FILE_NAME).exists(), "{extra:?}: nothing written");
    }
    // the same script twice is the same name twice
    let output = dir.freeze(&["--witness", "ok.phs", "--witness", "ok.phs"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
}

#[test]
fn an_existing_file_is_refused_before_any_script_is_evaluated() {
    let dir = Dir::new("freeze_existing");
    assert_eq!(dir.freeze(&[]).status.code(), Some(0));
    let before = dir.read(FROZEN_FILE_NAME);
    // a script that would take the whole default timeout if it were evaluated
    dir.write("slow.phs", "i = 0\nwhile i < 1 {\nj = 0\nwhile j < 9999 {\nj = j + 1\n}\ni = i * 1\n}");
    let started = Instant::now();
    let output = dir.freeze(&["--witness", "slow.phs"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("phs conf update"), "{}", text(&output));
    assert!(started.elapsed() < Duration::from_secs(30), "{:?}", started.elapsed());
    assert_eq!(dir.read(FROZEN_FILE_NAME), before);
}

#[test]
fn witness_and_the_verify_flags_are_refused_where_they_do_not_apply() {
    let dir = Dir::new("flags_misuse");
    dir.write("ok.phs", "1 m");
    for args in [
        &["verify", "--witness", "ok.phs"][..],
        &["status", "--run"][..],
        &["freeze", "--reason", "r", "--run"][..],
        &["freeze", "--reason", "r", "--timeout-ms", "10"][..],
        &["verify", "--timeout-ms", "10"][..],
        &["verify", "--run", "--timeout-ms", "0"][..],
        &["verify", "--run", "--timeout-ms", "soon"][..],
        &["verify", "--run", "--timeout-ms", "5", "--timeout-ms", "6"][..],
        &["freeze", "--reason", "r", "--witness"][..],
    ] {
        let output = dir.conf(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {}", text(&output));
    }
    assert!(!dir.path(FROZEN_FILE_NAME).exists());
}

#[test]
fn status_shows_the_witness_counts() {
    let dir = Dir::new("status_counts");
    dir.write("mine.phs", "1 m");
    assert_eq!(dir.freeze(&["--witness", "mine.phs"]).status.code(), Some(0));
    let builtin = keys(&dir.frozen(), "builtin.").len();
    let output = dir.conf(&["status"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert!(stdout(&output).contains(&format!("witnesses: {builtin} built-in, 1 user recorded")), "{}", text(&output));
    let doc = json(&dir.conf(&["status", "--json"]));
    assert_eq!(doc["witnesses_recorded"]["builtin"], Value::from(builtin));
    assert_eq!(doc["witnesses_recorded"]["user"], Value::from(1));
}

// ---- B3: verify --run ----

/// A file for another engine (version 0.0.1) that records one built-in witness.
fn file_for_another_engine() -> String {
    let identity = EngineIdentity { version: "0.0.1".into(), ..EngineIdentity::current() };
    FrozenFile::new(
        "2026-11-08T10:00:00Z",
        FrozenParts {
            engine: engine_pairs(&identity, EnginePolicy::Release, None).unwrap(),
            witnesses: Pairs::new(&[("builtin.units.foot_to_metre", "3fd381d7dbf487fd:0000000000000000:m")]).unwrap(),
            sources: vec![Source::new("embedded", DEFAULT_PHYSURE_CONF).unwrap(), Source::new("overrides", "").unwrap()],
            ..FrozenParts::default()
        },
        EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: "test", actor: "t@h", reason: "test", diff: "d" },
    )
    .unwrap()
    .render()
    .unwrap()
}

/// Rewrites the frozen file of `dir` with `edit` applied to its `[Witness]` pairs and its scripts, re-sealed with
/// the core API (a seal entry), so the file verifies and only the witnesses changed.
fn reseal(dir: &Dir, edit: impl FnOnce(&mut Vec<(String, String)>, &mut Vec<WitnessScript>)) {
    let mut file = dir.frozen();
    let mut pairs = file.witnesses().as_slice().to_vec();
    let mut scripts = file.witness_scripts().to_vec();
    edit(&mut pairs, &mut scripts);
    file.set_witnesses(Pairs::from_vec(pairs).unwrap());
    file.set_witness_scripts(scripts);
    file.reseal(EntryInput { time: "2099-01-01T00:00:00Z", action: "seal", engine: "test", actor: "t@h", reason: "test edit", diff: "d" })
        .unwrap();
    assert!(file.verify().is_empty(), "{:?}", file.verify());
    dir.write(FROZEN_FILE_NAME, &file.render().unwrap());
}

/// Adds a user witness `name` with `text` and a recorded hash of `hash` (64 hex digits).
fn add_user_witness(dir: &Dir, name: &str, text: &str, hash: &str) {
    reseal(dir, |pairs, scripts| {
        pairs.push((format!("user.{name}"), format!("sha256:{hash}")));
        scripts.push(WitnessScript::new(name, text).unwrap());
    });
}

fn verify_run(dir: &Dir, extra: &[&str]) -> Output {
    let mut args = vec!["verify", "--run"];
    args.extend_from_slice(extra);
    dir.conf(&args)
}

#[test]
fn a_fresh_file_matches_every_recorded_witness_exit_0() {
    let dir = Dir::new("run_fresh");
    dir.write("mine.phs", "x = 2 m\nx / 4 s");
    assert_eq!(dir.freeze(&["--witness", "mine.phs"]).status.code(), Some(0));
    let recorded = dir.frozen().witnesses().len();
    let builtin = keys(&dir.frozen(), "builtin.").len();
    let output = verify_run(&dir, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let out = stdout(&output);
    assert!(out.contains(&format!("{recorded} matched")), "{out}");
    assert!(out.contains(&format!("checked {builtin} built-in, 1 user")), "{out}");
    let doc = json(&verify_run(&dir, &["--json"]));
    let witnesses = &doc["witnesses"];
    assert_eq!(witnesses["checked"], Value::from(recorded));
    assert_eq!(witnesses["match"], Value::from(recorded));
    for list in ["differs", "missing", "new", "errors"] {
        assert_eq!(witnesses[list], Value::Array(Vec::new()), "{list}: {doc}");
    }
    assert_eq!(doc["witnesses_ok"], Value::Bool(true));
    assert_eq!(doc["exit_code"], Value::from(0));
    // without --run nothing is evaluated: no witness section
    let doc = json(&dir.conf(&["verify", "--json"]));
    assert!(doc.get("witnesses").is_none() && doc.get("witnesses_ok").is_none(), "{doc}");
}

#[test]
fn a_changed_builtin_value_is_a_difference_with_both_bit_patterns_exit_1() {
    let dir = Dir::new("run_builtin_differs");
    assert_eq!(dir.freeze(&[]).status.code(), Some(0));
    let key = "builtin.units.foot_to_metre";
    let original = dir.frozen().witnesses().get(key).expect("the case is recorded").to_string();
    let WitnessValue::Number { mean, uncertainty, unit } = parse_value(&original).unwrap() else { panic!("{original}") };
    let edited = physure_core::frozen::format_value(&WitnessValue::Number { mean: mean + 1, uncertainty, unit });
    reseal(&dir, |pairs, _| pairs.iter_mut().find(|(k, _)| k == key).unwrap().1 = edited.clone());
    let output = verify_run(&dir, &[]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    let out = stdout(&output);
    assert!(out.contains(key), "{out}");
    assert!(out.contains(&format!("{mean:016x}")) && out.contains(&format!("{:016x}", mean + 1)), "{out}");
    assert!(out.contains("1 ulp"), "{out}");
    assert!(out.contains(&format!("{:?}", f64::from_bits(mean))), "the decimal value: {out}");
    // the differing rows come before the count of matches
    assert!(out.find(key).unwrap() < out.find(" matched").unwrap(), "{out}");
    let doc = json(&verify_run(&dir, &["--json"]));
    assert_eq!(doc["witnesses_ok"], Value::Bool(false));
    let differs = doc["witnesses"]["differs"].as_array().unwrap();
    assert_eq!(differs.len(), 1, "{doc}");
    assert_eq!(differs[0]["key"], Value::from(key));
    assert_eq!(differs[0]["recorded"], Value::from(edited.as_str()));
    assert_eq!(differs[0]["running"], Value::from(original.as_str()));
    assert_eq!(differs[0]["ulps"], Value::from(1));
}

#[test]
fn a_changed_user_hash_is_a_difference_showing_the_running_canonical_text_exit_1() {
    let dir = Dir::new("run_user_differs");
    dir.write("mine.phs", "x = 2 m");
    assert_eq!(dir.freeze(&["--witness", "mine.phs"]).status.code(), Some(0));
    reseal(&dir, |pairs, _| pairs.iter_mut().find(|(k, _)| k == "user.mine").unwrap().1 = format!("sha256:{}", "0".repeat(64)));
    let output = verify_run(&dir, &[]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    let out = stdout(&output);
    assert!(out.contains("user.mine") && out.contains("only that") && out.contains("phs-witness-1"), "{out}");
    let doc = json(&verify_run(&dir, &["--json"]));
    let differs = &doc["witnesses"]["differs"][0];
    assert_eq!(differs["key"], Value::from("user.mine"));
    assert!(differs["running_canonical"].as_str().unwrap().starts_with("phs-witness-1\n"), "{doc}");
}

#[test]
fn a_recorded_case_this_build_lacks_is_missing_exit_1() {
    let dir = Dir::new("run_missing");
    assert_eq!(dir.freeze(&[]).status.code(), Some(0));
    reseal(&dir, |pairs, _| pairs.push(("builtin.zz_removed".into(), "3ff0000000000000:0000000000000000:m".into())));
    let output = verify_run(&dir, &[]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(stdout(&output).contains("builtin.zz_removed"), "{}", text(&output));
    let doc = json(&verify_run(&dir, &["--json"]));
    assert_eq!(doc["witnesses"]["missing"], serde_json::json!(["builtin.zz_removed"]));
}

#[test]
fn a_case_the_file_lacks_is_new_and_informational_exit_0() {
    let dir = Dir::new("run_new");
    assert_eq!(dir.freeze(&[]).status.code(), Some(0));
    let key = "builtin.units.foot_to_metre";
    reseal(&dir, |pairs, _| pairs.retain(|(k, _)| k != key));
    let output = verify_run(&dir, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert!(stdout(&output).contains(key), "listed: {}", text(&output));
    let doc = json(&verify_run(&dir, &["--json"]));
    assert_eq!(doc["witnesses"]["new"], serde_json::json!([key]));
    assert_eq!(doc["witnesses_ok"], Value::Bool(true));
}

#[test]
fn a_file_for_another_engine_is_exit_3_and_nothing_is_evaluated() {
    let dir = Dir::new("run_engine");
    dir.write(FROZEN_FILE_NAME, &file_for_another_engine());
    let output = verify_run(&dir, &[]);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    let out = stdout(&output);
    assert!(out.contains("not run") && out.contains("engine") && out.contains("nothing was evaluated"), "{out}");
    let doc = json(&verify_run(&dir, &["--json"]));
    assert_eq!(doc["witnesses"], Value::Null);
    assert_eq!(doc["witnesses_ok"], Value::Bool(false));
}

#[test]
fn a_tampered_file_is_exit_1_and_nothing_is_evaluated() {
    let dir = Dir::new("run_tampered");
    assert_eq!(dir.freeze(&[]).status.code(), Some(0));
    let edited = dir.read(FROZEN_FILE_NAME).replacen("= 8.31446261815324", "= 8.31446261815325", 1);
    dir.write(FROZEN_FILE_NAME, &edited);
    let output = verify_run(&dir, &[]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(stdout(&output).contains("not run") && stdout(&output).contains("does not verify"), "{}", text(&output));
    assert_eq!(json(&verify_run(&dir, &["--json"]))["witnesses"], Value::Null);
}

/// Correction C5: a file that records no built-in witness (every Phase 2 file) does not pass vacuously.
#[test]
fn a_file_without_the_battery_is_exit_1_nothing_is_witnessed() {
    let dir = Dir::new("run_vacuous");
    assert_eq!(dir.freeze(&[]).status.code(), Some(0));
    reseal(&dir, |pairs, _| pairs.clear());
    let output = verify_run(&dir, &[]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    let out = stdout(&output);
    assert!(out.contains("nothing is witnessed") && out.contains("phs conf update --accept-witness-changes all"), "{out}");
    let doc = json(&verify_run(&dir, &["--json"]));
    assert_eq!(doc["witnesses_ok"], Value::Bool(false));
}

/// Decision 3: `verify --run FILE` evaluates under FILE, not under what is in force where it runs.
#[test]
fn verify_run_of_a_file_elsewhere_evaluates_under_that_file() {
    let dir = Dir::new("run_elsewhere_file");
    dir.write("physure.conf", "[Constants]\nmolar_gas_constant = 8.0 J*mol^-1*K^-1\n");
    assert_eq!(dir.freeze(&[]).status.code(), Some(0));
    let key = "builtin.const.molar_gas_constant";
    let recorded = parse_value(dir.frozen().witnesses().get(key).unwrap()).unwrap();
    let WitnessValue::Number { mean, .. } = recorded else { panic!("{recorded:?}") };
    assert_eq!(f64::from_bits(mean), 8.0, "frozen under the override");
    // another directory, whose own physure.conf says something else
    let other = Dir::new("run_elsewhere_cwd");
    other.write("physure.conf", "[Constants]\nmolar_gas_constant = 9.0 J*mol^-1*K^-1\n");
    let file = dir.path(FROZEN_FILE_NAME);
    let output = other.conf(&["verify", file.to_str().unwrap(), "--run"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    // the same file by a relative path
    let relative = format!("../{}/{FROZEN_FILE_NAME}", dir.0.file_name().unwrap().to_str().unwrap());
    let output = other.conf(&["verify", &relative, "--run"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
}

/// Correction C3: a witness that does not end is a limit, not a difference, and the process ends promptly.
#[test]
fn a_user_witness_that_loops_forever_is_a_limit_and_the_process_ends_promptly() {
    let dir = Dir::new("run_forever");
    assert_eq!(dir.freeze(&[]).status.code(), Some(0));
    let forever = "i = 0\nwhile i < 1 {\nj = 0\nwhile j < 1 {\nk = 0\nwhile k < 9999 {\nk = k + 1\n}\nj = j * 1\n}\ni = i * 1\n}";
    add_user_witness(&dir, "forever", forever, &"a".repeat(64));
    add_user_witness(&dir, "after", "1 m", &"b".repeat(64));
    let started = Instant::now();
    let output = verify_run(&dir, &["--timeout-ms", "300", "--json"]);
    assert!(started.elapsed() < Duration::from_secs(30), "{:?}", started.elapsed());
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    let doc = json(&output);
    let errors = doc["witnesses"]["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{doc}");
    assert_eq!(errors[0]["key"], Value::from("user.forever"));
    assert!(errors[0]["message"].as_str().unwrap().starts_with("limit: "), "{doc}");
    // the next script ran (the first one stopped at its deadline) and differs from its made-up hash
    let differs = doc["witnesses"]["differs"].as_array().unwrap();
    assert_eq!(differs.len(), 1, "{doc}");
    assert_eq!(differs[0]["key"], Value::from("user.after"));
    let output = verify_run(&dir, &["--timeout-ms", "300"]);
    assert!(stdout(&output).contains("user.forever") && stdout(&output).contains("limit"), "{}", text(&output));
}

/// Correction C3: the file's `[Settings]` cannot raise the ceilings its own witnesses run under.
#[test]
fn the_file_s_settings_do_not_raise_the_witness_ceiling() {
    let dir = Dir::new("run_ceiling");
    dir.write("physure.conf", "[Settings]\nmax_loop_iterations = 1000000000\n");
    assert_eq!(dir.freeze(&[]).status.code(), Some(0), "the battery runs under the file's settings");
    add_user_witness(&dir, "big", "x = for i in 0..2000000 { i }", &"c".repeat(64));
    let doc = json(&verify_run(&dir, &["--json"]));
    assert_eq!(doc["exit_code"], Value::from(1), "{doc}");
    let errors = doc["witnesses"]["errors"].as_array().unwrap();
    assert_eq!(errors[0]["key"], Value::from("user.big"), "{doc}");
    let message = errors[0]["message"].as_str().unwrap();
    assert!(message.starts_with("limit: ") && message.contains("max_loop_iterations ceiling of 1000000"), "{message}");
}
