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

// helpers kept for the verify --run tests below
#[allow(dead_code)]
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
