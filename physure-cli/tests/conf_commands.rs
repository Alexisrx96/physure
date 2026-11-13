//! `phs conf`: the commands that inspect, freeze and verify a frozen configuration (conf freeze Phase 2). The real
//! binary runs in a temporary directory of its own, with `PHYSURE_CONF` and `PHYSURE_REQUIRE_FROZEN` removed unless a
//! test sets them. The test build names no commit, so files are frozen with `--engine-policy release` unless the test is
//! about the policy.

use physure_core::frozen::{
    engine_pairs, freeze, now_unix, plan_seal, write_atomic, EngineIdentity, EnginePolicy, EntryInput, FreezeOptions, FrozenFile,
    FrozenParts, Source, EMPTY_OVERRIDES,
};
use physure_core::units::conf::DEFAULT_PHYSURE_CONF;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const FROZEN_FILE_NAME: &str = "physure.frozen.conf";
const R_VALUE: &str = "= 8.31446261815324";

/// A unique temporary directory, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let path = std::env::temp_dir().join(format!("physure_cli_conf_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Dir(path.canonicalize().unwrap())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, text: &str) {
        fs::write(self.path(name), text).unwrap();
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.path(name)).unwrap()
    }

    fn phs(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_phs"));
        command.args(args).current_dir(&self.0).env_remove("PHYSURE_CONF").env_remove("PHYSURE_REQUIRE_FROZEN");
        for (name, value) in env {
            command.env(name, value);
        }
        command.output().expect("phs runs")
    }

    /// `phs conf <args>`.
    fn conf(&self, args: &[&str]) -> Output {
        let mut all = vec!["conf"];
        all.extend_from_slice(args);
        self.phs(&all, &[])
    }

    /// Freezes the directory under `release` with the core's own `freeze` (the command is tested on its own).
    fn freeze(&self) {
        let options = FreezeOptions {
            cwd: &self.0,
            out: None,
            reason: "first freeze",
            policy: EnginePolicy::Release,
            engine_file: None,
            time_unix: now_unix(),
            actor: "t@h",
        };
        let (file, out) = freeze(&options).unwrap();
        write_atomic(&out, &file.render().unwrap(), false).unwrap();
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

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Both streams, for assertion messages and for checks that do not care where a line went.
fn text(output: &Output) -> String {
    format!("{}{}", stdout(output), stderr(output))
}

fn json(output: &Output) -> Value {
    serde_json::from_str(&stdout(output)).unwrap_or_else(|e| panic!("not one JSON document ({e}): {}", text(output)))
}

/// A frozen file pinned (policy `release`) to a version that is not the running one.
fn file_for_another_engine() -> String {
    file_for_another_engine_with(DEFAULT_PHYSURE_CONF, "")
}

/// [`file_for_another_engine`] with these sources 1 and 2.
fn file_for_another_engine_with(embedded: &str, overrides: &str) -> String {
    let identity = EngineIdentity { version: "0.0.1".into(), ..EngineIdentity::current() };
    FrozenFile::new(
        "2026-11-08T10:00:00Z",
        FrozenParts {
            engine: engine_pairs(&identity, EnginePolicy::Release, None).unwrap(),
            sources: vec![Source::new("embedded", embedded).unwrap(), Source::new("overrides", overrides).unwrap()],
            ..FrozenParts::default()
        },
        EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: "test", actor: "t@h", reason: "test", diff: "d" },
    )
    .unwrap()
    .render()
    .unwrap()
}

/// The 64-hexadecimal seal printed on a line of `output`.
fn seal_in(output: &str) -> String {
    output
        .split(|c: char| !c.is_ascii_hexdigit())
        .find(|word| word.len() == 64)
        .unwrap_or_else(|| panic!("no seal in: {output}"))
        .to_string()
}

// ---- Task 5: dispatch, status, history ----

#[test]
fn status_without_a_file_says_unfrozen_exit_0() {
    let dir = Dir::new("status_unfrozen");
    for args in [&["status"][..], &[][..]] {
        let output = dir.conf(args);
        assert_eq!(output.status.code(), Some(0), "{}", text(&output));
        assert!(stdout(&output).contains("no frozen configuration in force"), "{}", text(&output));
    }
    dir.write("physure.conf", "[Units]\nzz_unit = 2.0, L, [zz_unit]\n");
    let output = dir.conf(&["status"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert!(stdout(&output).contains("physure.conf") && stdout(&output).contains("overlay"), "{}", text(&output));
}

#[test]
fn status_of_a_sound_file_exit_0_prints_seal_and_edition() {
    let dir = Dir::new("status_sound");
    dir.freeze();
    let seal = FrozenFile::parse(&dir.read(FROZEN_FILE_NAME)).unwrap().seal_sha256().to_string();
    let output = dir.conf(&["status"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let out = stdout(&output);
    assert!(out.contains(&seal), "{out}");
    assert!(out.contains("standard-2024"), "{out}");
    assert!(out.contains("discovery"), "{out}");
}

#[test]
fn status_of_an_edited_file_exit_1_and_names_the_problem() {
    let dir = Dir::new("status_edited");
    dir.freeze();
    dir.write(FROZEN_FILE_NAME, &dir.read(FROZEN_FILE_NAME).replacen(R_VALUE, "= 8.31446261815325", 1));
    let output = dir.conf(&["status"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(stdout(&output).contains("source 1"), "{}", text(&output));
}

#[test]
fn status_of_a_file_for_another_engine_exit_3() {
    let dir = Dir::new("status_engine");
    dir.write(FROZEN_FILE_NAME, &file_for_another_engine());
    let output = dir.conf(&["status"]);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(stdout(&output).contains("0.0.1"), "{}", text(&output));
    // --allow-engine-mismatch lets it be inspected; it never changes the exit code
    let output = dir.conf(&["status", "--allow-engine-mismatch", "--json"]);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert_eq!(json(&output)["engine_matches"], Value::Bool(false));
}

#[test]
fn status_with_require_frozen_and_no_file_exit_2() {
    let dir = Dir::new("status_required");
    let output = dir.phs(&["conf", "status"], &[("PHYSURE_REQUIRE_FROZEN", "1")]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("PHYSURE_REQUIRE_FROZEN"), "{}", text(&output));
    // an explicit --conf that names nothing is resolution too
    let output = dir.conf(&["status", "--conf", "missing.conf"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(text(&output).contains("missing.conf"), "{}", text(&output));
}

#[test]
fn status_json_has_schema_1_and_command_status() {
    let dir = Dir::new("status_json");
    let unfrozen = json(&dir.conf(&["status", "--json"]));
    assert_eq!((unfrozen["schema"].clone(), unfrozen["command"].clone()), (Value::from(1), Value::from("status")));
    assert_eq!(unfrozen["frozen"], Value::Bool(false));
    dir.freeze();
    let output = dir.conf(&["--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let frozen = json(&output);
    assert_eq!((frozen["schema"].clone(), frozen["command"].clone()), (Value::from(1), Value::from("status")));
    assert_eq!(frozen["frozen"], Value::Bool(true));
    assert_eq!(frozen["engine_matches"], Value::Bool(true));
    assert_eq!(frozen["history_entries"], Value::from(1));
    assert_eq!(frozen["why"], Value::from("discovery"));
    assert!(frozen["path"].as_str().unwrap().ends_with(FROZEN_FILE_NAME));
    assert!(frozen["atomic_weight_edition"].as_str().unwrap().starts_with("standard-2024"));
}

#[test]
fn a_broken_file_does_not_stop_conf_status_but_stops_other_commands() {
    let dir = Dir::new("broken");
    dir.write(FROZEN_FILE_NAME, "[Units]\nm = 1.0, L\n");
    assert_eq!(dir.phs(&["1 m => ft"], &[]).status.code(), Some(1));
    let output = dir.conf(&["status"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(!stderr(&output).contains("refusing to run"), "{}", text(&output));
    assert!(stdout(&output).contains(FROZEN_FILE_NAME), "{}", text(&output));
}

#[test]
fn a_physure_conf_beside_a_frozen_file_is_a_warning() {
    let dir = Dir::new("status_ignored");
    dir.freeze();
    dir.write("physure.conf", "[Units]\nzz_unit = 2.0, L, [zz_unit]\n");
    let output = dir.conf(&["status", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let warnings = json(&output)["warnings"].to_string();
    assert!(warnings.contains("physure.conf") && warnings.contains("ignored"), "{warnings}");
}

#[test]
fn history_lists_entries_in_order() {
    let dir = Dir::new("history");
    dir.freeze();
    let path = dir.path(FROZEN_FILE_NAME);
    let edited = dir.read(FROZEN_FILE_NAME).replace(EMPTY_OVERRIDES.trim_end(), "[Units]\nzz_unit = 2.0, L, [zz_unit]");
    dir.write(FROZEN_FILE_NAME, &edited);
    let plan = plan_seal(&path, "my unit", now_unix() + 1, "t@h").unwrap().unwrap();
    write_atomic(&path, &plan.file.render().unwrap(), true).unwrap();
    let output = dir.conf(&["history"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let out = stdout(&output);
    let (freeze, seal) = (out.find("freeze").expect("freeze row"), out.find("seal").expect("seal row"));
    assert!(freeze < seal, "{out}");
    assert!(out.contains("first freeze") && out.contains("my unit"), "{out}");
    let rows = json(&dir.conf(&["history", "--json"]));
    assert_eq!(rows["command"], Value::from("history"));
    let rows = rows["rows"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[0]["action"].clone(), rows[1]["action"].clone()), (Value::from("freeze"), Value::from("seal")));
    assert_eq!(rows[1]["unverified"], Value::Bool(false));
}

#[test]
fn history_of_a_tampered_chain_exits_1_and_prints_the_problems_first() {
    let dir = Dir::new("history_tampered");
    dir.freeze();
    dir.write(FROZEN_FILE_NAME, &dir.read(FROZEN_FILE_NAME).replacen("reason = first freeze", "reason = other freeze", 1));
    let output = dir.conf(&["history"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    let out = stdout(&output);
    let problems = out.find("does not verify").unwrap_or_else(|| panic!("{out}"));
    assert!(problems < out.find("other freeze").unwrap(), "{out}");
    assert!(out.contains("unverified"), "{out}");
}

#[test]
fn unknown_flag_is_exit_2_and_names_the_flags() {
    let dir = Dir::new("unknown_flag");
    let output = dir.conf(&["status", "--frobnicate"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    let err = stderr(&output);
    assert!(err.contains("--frobnicate") && err.contains("--json") && err.contains("--conf"), "{err}");
    // a flag another command takes is misuse here
    let output = dir.conf(&["status", "--reason", "x"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    // a flag without its value
    let output = dir.conf(&["status", "--conf"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
}

#[test]
fn unknown_commands_are_exit_2_and_name_the_commands() {
    let dir = Dir::new("unknown_command");
    let output = dir.conf(&["frobnicate"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    let err = stderr(&output);
    for command in ["status", "show", "diff", "update", "seal", "history", "freeze", "verify"] {
        assert!(err.contains(command), "{command}: {err}");
    }
}

#[test]
fn conf_help_prints_the_commands() {
    let dir = Dir::new("help");
    for args in [&["--help"][..], &["freeze", "-h"][..]] {
        let output = dir.conf(args);
        assert_eq!(output.status.code(), Some(0), "{}", text(&output));
        let out = stdout(&output);
        assert!(out.contains("phs conf freeze") && out.contains("--engine-policy") && out.contains("--expect"), "{out}");
    }
}

/// C1: `conf` is dispatched before anything else looks at the arguments, so a word another command would claim
/// (`help`, `doc`, `repl`, `daemon`, `-V`) never runs that command.
#[test]
fn conf_is_dispatched_before_every_other_command() {
    let dir = Dir::new("dispatch");
    for args in [&["show", "doc"][..], &["repl"][..], &["daemon"][..], &["-V"][..], &["--version"][..], &["status", "doc"][..]] {
        let output = dir.conf(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {}", text(&output));
        let all = text(&output);
        assert!(!all.contains("USAGE") && !all.contains("REPL") && !all.contains("phs 0.") && !all.contains("# "), "{args:?}: {all}");
    }
    // the pre-command check still guards every other command: a broken file beside conf's own success
    dir.write(FROZEN_FILE_NAME, "garbage");
    assert_eq!(dir.phs(&["--version"], &[]).status.code(), Some(1));
}

// ---- Task 6: freeze and verify ----

/// `phs conf freeze --reason R --engine-policy release` plus `extra`.
fn freeze_cli(dir: &Dir, extra: &[&str]) -> Output {
    let mut args = vec!["freeze", "--reason", "first freeze", "--engine-policy", "release"];
    args.extend_from_slice(extra);
    dir.conf(&args)
}

#[test]
fn freeze_with_no_physure_conf_writes_a_file_that_verifies() {
    let dir = Dir::new("freeze_plain");
    let output = freeze_cli(&dir, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let file = FrozenFile::parse(&dir.read(FROZEN_FILE_NAME)).unwrap();
    assert!(file.verify().is_empty());
    assert_eq!(file.engine().get("policy"), Some("release"));
    assert_eq!(file.history()[0].reason(), "first freeze");
    let out = stdout(&output);
    assert!(out.contains(FROZEN_FILE_NAME) && out.contains("record this seal outside this directory"), "{out}");
    assert!(out.contains("release"), "{out}");
    let verified = dir.conf(&["verify"]);
    assert_eq!(verified.status.code(), Some(0), "{}", text(&verified));
    // --out writes elsewhere, relative to the working directory
    let other = Dir::new("freeze_out");
    let output = freeze_cli(&other, &["--out", "audit.frozen.conf", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let doc = json(&output);
    assert_eq!((doc["schema"].clone(), doc["command"].clone()), (Value::from(1), Value::from("freeze")));
    assert!(doc["path"].as_str().unwrap().ends_with("audit.frozen.conf"));
    assert_eq!(doc["seal"].as_str().unwrap().len(), 64);
    assert_eq!(doc["policy"], Value::from("release"));
    assert!(other.path("audit.frozen.conf").is_file() && !other.path(FROZEN_FILE_NAME).exists());
}

#[test]
fn freeze_reason_help_freezes_and_prints_no_help() {
    let dir = Dir::new("freeze_help_reason");
    let output = dir.conf(&["freeze", "--reason", "help", "--engine-policy", "release"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert!(!stdout(&output).contains("USAGE"), "{}", text(&output));
    assert_eq!(FrozenFile::parse(&dir.read(FROZEN_FILE_NAME)).unwrap().history()[0].reason(), "help");
}

#[test]
fn freeze_twice_is_exit_2_and_names_update() {
    let dir = Dir::new("freeze_twice");
    freeze_cli(&dir, &[]);
    let before = dir.read(FROZEN_FILE_NAME);
    let output = freeze_cli(&dir, &[]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(stderr(&output).contains("phs conf update"), "{}", text(&output));
    assert_eq!(dir.read(FROZEN_FILE_NAME), before);
    // an output that exists is not overwritten
    let other = Dir::new("freeze_taken");
    other.write("taken.conf", "x");
    let output = freeze_cli(&other, &["--out", "taken.conf"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert_eq!(other.read("taken.conf"), "x");
}

#[test]
fn freeze_without_reason_is_exit_2_before_reading_anything() {
    let dir = Dir::new("freeze_no_reason");
    // a physure.conf that would be refused (exit 1) if it were read
    dir.write("physure.conf", "[AtomicWeights]\nFe = abc, 0.1\n");
    for args in [&["freeze", "--engine-policy", "release"][..], &["freeze", "--reason", "  ", "--engine-policy", "release"][..]] {
        let output = dir.conf(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {}", text(&output));
        assert!(stderr(&output).contains("--reason"), "{}", text(&output));
    }
    assert!(!dir.path(FROZEN_FILE_NAME).exists());
    // with a reason the file is read, and a physure.conf that would never load is a discrepancy: exit 1
    let output = freeze_cli(&dir, &[]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(stderr(&output).contains("Fe = abc"), "{}", text(&output));
    assert!(!dir.path(FROZEN_FILE_NAME).exists());
}

#[test]
fn freeze_with_conf_is_exit_2() {
    let dir = Dir::new("freeze_conf");
    let output = freeze_cli(&dir, &["--conf", "x.conf"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(stderr(&output).contains("--out"), "{}", text(&output));
    assert!(!dir.path(FROZEN_FILE_NAME).exists());
}

#[test]
fn freeze_prints_the_seal_that_verify_expects() {
    let dir = Dir::new("freeze_seal");
    let seal = seal_in(&stdout(&freeze_cli(&dir, &[])));
    assert_eq!(seal, FrozenFile::parse(&dir.read(FROZEN_FILE_NAME)).unwrap().seal_sha256());
    let output = dir.conf(&["verify", "--expect", &seal.to_uppercase()]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let last = if seal.ends_with('0') { "1" } else { "0" };
    let wrong = format!("{}{last}", &seal[..63]);
    let output = dir.conf(&["verify", "--expect", &wrong]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(stdout(&output).contains("does not match"), "{}", text(&output));
}

#[test]
fn freeze_build_policy_on_a_build_without_a_commit_is_exit_2_and_mentions_release() {
    let dir = Dir::new("freeze_build");
    let output = dir.conf(&["freeze", "--reason", "default policy"]);
    if EngineIdentity::current().commit == physure_core::frozen::engine::UNKNOWN {
        assert_eq!(output.status.code(), Some(2), "{}", text(&output));
        assert!(stderr(&output).contains("--engine-policy release"), "{}", text(&output));
        assert!(!dir.path(FROZEN_FILE_NAME).exists());
    } else {
        assert_eq!(output.status.code(), Some(0), "{}", text(&output));
        assert_eq!(FrozenFile::parse(&dir.read(FROZEN_FILE_NAME)).unwrap().engine().get("policy"), Some("build"));
    }
}

#[test]
fn freeze_with_overrides_keeps_physure_conf_as_source_2() {
    let dir = Dir::new("freeze_overrides");
    let local = "[Constants]\nmolar_gas_constant = 8.0 J*mol^-1*K^-1\n[Units]\nzz_unit = 2.0, L, [zz_unit]\n";
    dir.write("physure.conf", local);
    let output = freeze_cli(&dir, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let file = FrozenFile::parse(&dir.read(FROZEN_FILE_NAME)).unwrap();
    assert_eq!((file.sources()[1].origin(), file.sources()[1].text()), ("overrides", local.trim_end()));
    // the frozen file is now the catalog of phs
    let output = dir.phs(&["3 zz_unit => m"], &[]);
    assert!(output.status.success() && stdout(&output).contains('6'), "{}", text(&output));
}

#[test]
fn verify_names_the_modified_value() {
    let dir = Dir::new("verify_modified");
    freeze_cli(&dir, &[]);
    dir.write(FROZEN_FILE_NAME, &dir.read(FROZEN_FILE_NAME).replacen(R_VALUE, "= 8.31446261815325", 1));
    let output = dir.conf(&["verify"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(stdout(&output).contains("source 1") && stdout(&output).contains("does not verify"), "{}", text(&output));
    // the same file by name, from elsewhere
    let elsewhere = Dir::new("verify_elsewhere");
    let path = dir.path(FROZEN_FILE_NAME);
    let output = elsewhere.conf(&["verify", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
}

/// Review finding 4: an edited `[Engine]` block breaks the seal; its state against this build is not reported, so a
/// tampered block can never read as "matches".
#[test]
fn verify_of_a_tampered_engine_block_does_not_report_the_engine_as_matching() {
    let dir = Dir::new("verify_engine_tampered");
    freeze_cli(&dir, &[]);
    let frozen = dir.read(FROZEN_FILE_NAME);
    let edited = frozen.replacen("policy = release", "policy = record", 1);
    assert_ne!(edited, frozen, "the [Engine] block was not found");
    dir.write(FROZEN_FILE_NAME, &edited);
    let output = dir.conf(&["verify", "--json"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    let doc = json(&output);
    assert_eq!(doc["verified"], Value::Bool(false));
    assert_eq!(doc["engine_matches"], Value::Bool(false));
    assert_eq!(doc["engine"], Value::Null);
    let output = dir.conf(&["status"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(stdout(&output).contains("engine: not checked (the file does not verify)"), "{}", text(&output));
}

#[test]
fn verify_json_schema() {
    let dir = Dir::new("verify_json");
    let seal = seal_in(&stdout(&freeze_cli(&dir, &[])));
    let output = dir.conf(&["verify", "--json", "--expect", &seal]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let doc = json(&output);
    assert_eq!((doc["schema"].clone(), doc["command"].clone()), (Value::from(1), Value::from("verify")));
    assert_eq!(doc["seal"], Value::from(seal.as_str()));
    assert_eq!(doc["verified"], Value::Bool(true));
    assert_eq!(doc["engine_matches"], Value::Bool(true));
    assert_eq!(doc["seal_matches"], Value::Bool(true));
    assert_eq!(doc["exit_code"], Value::from(0));
    assert!(doc["problems"].as_array().unwrap().is_empty());
    let sources = doc["sources"].as_array().unwrap();
    assert_eq!((sources[0]["origin"].clone(), sources[1]["origin"].clone()), (Value::from("embedded"), Value::from("overrides")));
    // without --expect there is no seal_matches to report
    assert_eq!(json(&dir.conf(&["verify", "--json"]))["seal_matches"], Value::Null);
}

/// C4: a hand edit keeps the stored seal, so the seal alone must not vouch for it.
#[test]
fn verify_expect_after_editing_a_source_is_a_mismatch() {
    let dir = Dir::new("verify_expect_edit");
    let seal = seal_in(&stdout(&freeze_cli(&dir, &[])));
    let edited = dir.read(FROZEN_FILE_NAME).replace(EMPTY_OVERRIDES.trim_end(), "[Units]\nzz_unit = 2.0, L, [zz_unit]");
    dir.write(FROZEN_FILE_NAME, &edited);
    let output = dir.conf(&["verify", "--json", "--expect", &seal]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert_eq!(json(&output)["seal_matches"], Value::Bool(false));
}

#[test]
fn verify_expect_that_is_not_64_hex_is_exit_2() {
    let dir = Dir::new("verify_expect_bad");
    freeze_cli(&dir, &[]);
    let (non_hex, short) = ("g".repeat(64), "a".repeat(63));
    for bad in ["abc", non_hex.as_str(), short.as_str()] {
        let output = dir.conf(&["verify", "--expect", bad]);
        assert_eq!(output.status.code(), Some(2), "{bad}: {}", text(&output));
        assert!(stderr(&output).contains("64"), "{}", text(&output));
    }
}

#[test]
fn verify_of_a_missing_file_is_1_and_without_a_file_in_force_is_2() {
    let dir = Dir::new("verify_missing");
    let output = dir.conf(&["verify", "missing.frozen.conf"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(text(&output).contains("missing.frozen.conf"), "{}", text(&output));
    let output = dir.conf(&["verify"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    let output = dir.conf(&["verify", "--conf", "missing.frozen.conf"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
}

#[test]
fn verify_of_a_file_for_another_engine_stays_3_with_allow_engine_mismatch() {
    let dir = Dir::new("verify_engine");
    dir.write(FROZEN_FILE_NAME, &file_for_another_engine());
    for extra in [&[][..], &["--allow-engine-mismatch"][..]] {
        let mut args = vec!["verify", "--json"];
        args.extend_from_slice(extra);
        let output = dir.conf(&args);
        assert_eq!(output.status.code(), Some(3), "{extra:?}: {}", text(&output));
        let doc = json(&output);
        assert_eq!(doc["engine_matches"], Value::Bool(false));
        assert_eq!(doc["verified"], Value::Bool(true));
    }
}

// ---- Task 7: show ----

/// The non-empty lines of stdout.
fn lines(output: &Output) -> Vec<String> {
    stdout(output).lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect()
}

#[test]
fn show_constants_lists_only_constants() {
    let dir = Dir::new("show_constants");
    freeze_cli(&dir, &[]);
    let output = dir.conf(&["show", "constants"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let shown = lines(&output);
    assert!(shown.len() > 10, "{shown:?}");
    assert!(shown.iter().all(|l| l.starts_with("constants.")), "{shown:?}");
    assert!(shown.iter().any(|l| l.starts_with("constants.molar_gas_constant = 8.31446261815324")), "{shown:?}");
    let mut sorted = shown.clone();
    sorted.sort();
    assert_eq!(shown, sorted, "sorted");
    // the whole catalog has the other sections too
    let all = stdout(&dir.conf(&["show"]));
    assert!(all.contains("
units.m ") && all.contains("
settings.angle_mode "), "the whole catalog");
}

#[test]
fn show_one_key() {
    let dir = Dir::new("show_key");
    freeze_cli(&dir, &[]);
    let output = dir.conf(&["show", "constants.molar_gas_constant"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert_eq!(lines(&output), vec!["constants.molar_gas_constant = 8.31446261815324 J*mol^-1*K^-1".to_string()]);
}

#[test]
fn show_origin_marks_overrides() {
    let dir = Dir::new("show_origin");
    dir.write(
        "physure.conf",
        "[Constants]
molar_gas_constant = 8.0 J*mol^-1*K^-1
speed_of_light_in_vacuum = 299792458.0 m*s^-1
",
    );
    freeze_cli(&dir, &[]);
    let output = dir.conf(&["show", "constants", "--origin"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let shown = lines(&output);
    let line = |key: &str| shown.iter().find(|l| l.starts_with(&format!("constants.{key} "))).unwrap_or_else(|| panic!("{key}: {shown:?}")).clone();
    assert!(line("molar_gas_constant").ends_with("[overrides]"), "{shown:?}");
    assert!(line("avogadro_constant").ends_with("[embedded]"), "{shown:?}");
    assert!(line("speed_of_light_in_vacuum").ends_with("[embedded (restated by overrides)]"), "{shown:?}");
}

#[test]
fn show_unknown_selector_is_exit_2() {
    let dir = Dir::new("show_unknown");
    freeze_cli(&dir, &[]);
    let output = dir.conf(&["show", "nonsense"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    let err = stderr(&output);
    assert!(err.contains("nonsense") && err.contains("constants") && err.contains("units") && err.contains("settings"), "{err}");
    assert_eq!(dir.conf(&["show", "constants.nonsense"]).status.code(), Some(2));
}

#[test]
fn show_json() {
    let dir = Dir::new("show_json");
    freeze_cli(&dir, &[]);
    let output = dir.conf(&["show", "constants.molar_gas_constant", "--json", "--origin"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let doc = json(&output);
    assert_eq!((doc["schema"].clone(), doc["command"].clone()), (Value::from(1), Value::from("show")));
    assert_eq!(doc["frozen"], Value::Bool(true));
    let entries = doc["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!((entry["section"].clone(), entry["key"].clone(), entry["kind"].clone()), ("constants".into(), "molar_gas_constant".into(), "constant".into()));
    assert_eq!(entry["value"], Value::from("8.31446261815324 J*mol^-1*K^-1"));
    assert_eq!(entry["origin"], Value::from("embedded"));
}

#[test]
fn show_with_a_broken_file_is_exit_1() {
    let dir = Dir::new("show_broken");
    freeze_cli(&dir, &[]);
    dir.write(FROZEN_FILE_NAME, &dir.read(FROZEN_FILE_NAME).replacen(R_VALUE, "= 8.31446261815325", 1));
    let output = dir.conf(&["show", "constants"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(stderr(&output).contains(FROZEN_FILE_NAME) && stdout(&output).is_empty(), "{}", text(&output));
    // a file for another engine is not the configuration in force either: 3
    let other = Dir::new("show_engine");
    other.write(FROZEN_FILE_NAME, &file_for_another_engine());
    assert_eq!(other.conf(&["show", "constants"]).status.code(), Some(3));
}

#[test]
fn show_unfrozen_reads_physure_conf_as_the_runtime_does() {
    let dir = Dir::new("show_unfrozen");
    let output = dir.conf(&["show", "constants.molar_gas_constant", "--origin"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert_eq!(lines(&output), vec!["constants.molar_gas_constant = 8.31446261815324 J*mol^-1*K^-1  [embedded]".to_string()]);
    // a physure.conf: its values, with their origin; a line the runtime skips is a warning
    dir.write("physure.conf", "[Constants]
molar_gas_constant = 8.0 J*mol^-1*K^-1
this is not a line
");
    let output = dir.conf(&["show", "constants.molar_gas_constant", "--origin"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert_eq!(lines(&output), vec!["constants.molar_gas_constant = 8.0 J*mol^-1*K^-1  [physure.conf]".to_string()]);
    assert!(stderr(&output).contains("warning") && stderr(&output).contains("this is not a line"), "{}", text(&output));
    let doc = json(&dir.conf(&["show", "constants", "--json"]));
    assert_eq!(doc["frozen"], Value::Bool(false));
    assert!(doc["warnings"].to_string().contains("this is not a line"), "{doc}");
}

// ---- Task 8: diff ----

#[test]
fn diff_of_a_fresh_freeze_is_identical_exit_0() {
    let dir = Dir::new("diff_fresh");
    freeze_cli(&dir, &[]);
    let output = dir.conf(&["diff"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert!(stdout(&output).contains("identical"), "{}", text(&output));
    // --allow-engine-mismatch is accepted and changes nothing
    assert_eq!(dir.conf(&["diff", "--allow-engine-mismatch"]).status.code(), Some(0));
}

#[test]
fn diff_after_editing_physure_conf_exit_1_and_names_the_unit() {
    let dir = Dir::new("diff_drift");
    freeze_cli(&dir, &[]);
    dir.write("physure.conf", "[Units]
zz_unit = 2.0, L, [zz_unit]
");
    let output = dir.conf(&["diff"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    let out = stdout(&output);
    assert!(out.contains("+ zz_unit"), "{out}");
    // the default compares with the working directory: an upgrade and a drifted physure.conf both show here
    assert!(out.contains("library upgrade") && out.contains("physure.conf"), "{out}");
    assert!(out.contains("unit +1 -0 ~0"), "the summary: {out}");
}

#[test]
fn diff_two_files() {
    let a = Dir::new("diff_a");
    let b = Dir::new("diff_b");
    freeze_cli(&a, &[]);
    b.write("physure.conf", "[Constants]
molar_gas_constant = 8.0 J*mol^-1*K^-1
");
    freeze_cli(&b, &[]);
    let (fa, fb) = (a.path(FROZEN_FILE_NAME), b.path(FROZEN_FILE_NAME));
    let output = a.conf(&["diff", fa.to_str().unwrap(), fb.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    let out = stdout(&output);
    assert!(out.contains("~ molar_gas_constant: 8.31446261815324 J*mol^-1*K^-1 -> 8.0 J*mol^-1*K^-1"), "{out}");
    assert!(!out.contains("library upgrade"), "the note is for the default only: {out}");
    assert_eq!(a.conf(&["diff", fa.to_str().unwrap(), fa.to_str().unwrap()]).status.code(), Some(0));
    // one argument: that file against what this build gives unfrozen here; `unfrozen` names that side
    assert_eq!(a.conf(&["diff", FROZEN_FILE_NAME]).status.code(), Some(0));
    assert_eq!(a.conf(&["diff", "unfrozen", FROZEN_FILE_NAME]).status.code(), Some(0));
    assert_eq!(a.conf(&["diff", fb.to_str().unwrap()]).status.code(), Some(1));
}

/// Review finding 2: `[Environment]` is outside the seal and informational everywhere, so it is listed but does not
/// make two configurations different.
#[test]
fn diff_of_files_that_differ_only_in_the_environment_is_exit_0() {
    let dir = Dir::new("diff_environment");
    freeze_cli(&dir, &[]);
    let frozen = dir.read(FROZEN_FILE_NAME);
    let cpus = FrozenFile::parse(&frozen).unwrap().environment().get("cpus").unwrap().to_string();
    let edited = frozen.replacen(&format!("cpus = {cpus}"), "cpus = 99999", 1);
    assert_ne!(edited, frozen, "the [Environment] block was not found");
    dir.write("other.frozen.conf", &edited);
    let output = dir.conf(&["diff", FROZEN_FILE_NAME, "other.frozen.conf"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let out = stdout(&output);
    assert!(out.contains("cpus") && out.contains("informational") && out.contains("identical"), "{out}");
    let output = dir.conf(&["diff", FROZEN_FILE_NAME, "other.frozen.conf", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let doc = json(&output);
    assert_eq!(doc["identical"], Value::Bool(true));
    assert_eq!(doc["environment_identical"], Value::Bool(false));
    assert!(doc["environment"].as_array().unwrap().iter().any(|c| c["key"] == "cpus"), "{doc}");
    // the default compares with this machine: an edited environment alone is not a difference either
    dir.write(FROZEN_FILE_NAME, &edited);
    assert_eq!(dir.conf(&["diff"]).status.code(), Some(0));
}

#[test]
fn diff_without_a_frozen_file_is_exit_2() {
    let dir = Dir::new("diff_none");
    let output = dir.conf(&["diff"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(stderr(&output).contains("nothing to compare"), "{}", text(&output));
    assert_eq!(dir.conf(&["diff", "a", "b", "c"]).status.code(), Some(2), "three arguments");
}

#[test]
fn diff_of_a_file_that_does_not_verify_or_does_not_exist_is_exit_1() {
    let dir = Dir::new("diff_broken");
    freeze_cli(&dir, &[]);
    dir.write(FROZEN_FILE_NAME, &dir.read(FROZEN_FILE_NAME).replacen(R_VALUE, "= 8.31446261815325", 1));
    let output = dir.conf(&["diff", "--json"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    let doc = json(&output);
    assert_eq!(doc["identical"], Value::Bool(false));
    assert!(!doc["a_problems"].as_array().unwrap().is_empty(), "{doc}");
    let output = dir.conf(&["diff", "missing.frozen.conf"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(stderr(&output).contains("missing.frozen.conf"), "{}", text(&output));
}

#[test]
fn diff_of_a_file_for_another_engine_lists_the_engine_and_stays_1() {
    let dir = Dir::new("diff_engine");
    dir.write(FROZEN_FILE_NAME, &file_for_another_engine());
    for extra in [&[][..], &["--allow-engine-mismatch"][..]] {
        let mut args = vec!["diff", "--json"];
        args.extend_from_slice(extra);
        let output = dir.conf(&args);
        assert_eq!(output.status.code(), Some(1), "{extra:?}: {}", text(&output));
        let engine = json(&output)["engine"].as_array().unwrap().clone();
        assert!(engine.iter().any(|c| c["key"] == "version" && c["old"] == "0.0.1"), "{engine:?}");
    }
}

#[test]
fn diff_json_schema_is_stable() {
    let dir = Dir::new("diff_json");
    freeze_cli(&dir, &[]);
    dir.write("physure.conf", "[Units]
zz_unit = 2.0, L, [zz_unit]
[Constants]
molar_gas_constant = 8.0 J*mol^-1*K^-1
");
    let output = dir.conf(&["diff", "--json"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    let doc = json(&output);
    for key in ["schema", "command", "a", "b", "identical", "data", "engine", "environment", "summary", "warnings"] {
        assert!(doc.get(key).is_some(), "{key}: {doc}");
    }
    assert_eq!((doc["schema"].clone(), doc["command"].clone()), (Value::from(1), Value::from("diff")));
    assert_eq!(doc["identical"], Value::Bool(false));
    let data = doc["data"].as_array().unwrap();
    let mut keys: Vec<&str> = data[0].as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, vec!["change", "kind", "name", "new", "old"]);
    let changed = data.iter().find(|c| c["name"] == "molar_gas_constant" && c["kind"] == "constant").expect("the constant");
    assert_eq!((changed["change"].clone(), changed["old"].clone(), changed["new"].clone()),
        ("changed".into(), "8.31446261815324 J*mol^-1*K^-1".into(), "8.0 J*mol^-1*K^-1".into()));
    let added = data.iter().find(|c| c["name"] == "zz_unit" && c["kind"] == "unit_names").expect("the unit");
    assert_eq!((added["change"].clone(), added["old"].clone()), ("added".into(), Value::Null));
}

#[test]
fn diff_does_not_modify_anything() {
    let dir = Dir::new("diff_readonly");
    freeze_cli(&dir, &[]);
    dir.write("physure.conf", "[Units]
zz_unit = 2.0, L, [zz_unit]
");
    let (frozen, local) = (fs::read(dir.path(FROZEN_FILE_NAME)).unwrap(), fs::read(dir.path("physure.conf")).unwrap());
    dir.conf(&["diff"]);
    dir.conf(&["diff", "--json"]);
    assert_eq!(fs::read(dir.path(FROZEN_FILE_NAME)).unwrap(), frozen);
    assert_eq!(fs::read(dir.path("physure.conf")).unwrap(), local);
    let mut names: Vec<String> = fs::read_dir(&dir.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, vec!["physure.conf".to_string(), FROZEN_FILE_NAME.to_string()]);
}

// ---- Task 9: update and seal ----

/// The file's bytes, to prove nothing was written.
fn bytes(dir: &Dir) -> Vec<u8> {
    fs::read(dir.path(FROZEN_FILE_NAME)).unwrap()
}

#[test]
fn update_and_seal_without_reason_exit_2() {
    let dir = Dir::new("update_no_reason");
    freeze_cli(&dir, &[]);
    let before = bytes(&dir);
    for command in ["update", "seal"] {
        for args in [&[command, "--yes"][..], &[command, "--reason", " ", "--yes"][..]] {
            let output = dir.conf(args);
            assert_eq!(output.status.code(), Some(2), "{args:?}: {}", text(&output));
            assert!(stderr(&output).contains("--reason"), "{args:?}: {}", text(&output));
        }
    }
    assert_eq!(bytes(&dir), before);
    // nothing in force: nothing to update or seal
    let empty = Dir::new("update_none");
    for command in ["update", "seal"] {
        let output = empty.conf(&[command, "--reason", "r", "--yes"]);
        assert_eq!(output.status.code(), Some(2), "{command}: {}", text(&output));
    }
}

#[test]
fn update_with_nothing_new_says_so_exit_0() {
    let dir = Dir::new("update_nothing");
    freeze_cli(&dir, &[]);
    let before = bytes(&dir);
    let output = dir.conf(&["update", "--reason", "nothing"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert!(stdout(&output).contains("nothing to update"), "{}", text(&output));
    assert_eq!(bytes(&dir), before);
}

#[test]
fn update_without_yes_and_no_terminal_is_exit_2_and_writes_nothing() {
    let dir = Dir::new("update_no_terminal");
    // source 1 as an older library shipped it: one constant with another value (sealed so, so the file verifies)
    let r = "molar_gas_constant = 8.31446261815324 J*mol^-1*K^-1";
    assert!(DEFAULT_PHYSURE_CONF.contains(r), "the test's anchor is gone");
    let older = DEFAULT_PHYSURE_CONF.replacen(r, "molar_gas_constant = 8.0 J*mol^-1*K^-1", 1);
    dir.write(FROZEN_FILE_NAME, &file_for_another_engine_with(&older, ""));
    let before = bytes(&dir);
    let output = dir.conf(&["update", "--reason", "adopt this build"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    // what it would write is shown first: the data the update changes, and the engine pairs that differ
    let out = stdout(&output);
    assert!(out.contains("~ molar_gas_constant: 8.0 J*mol^-1*K^-1 -> 8.31446261815324 J*mol^-1*K^-1"), "{out}");
    assert!(out.contains("version: 0.0.1 -> "), "{out}");
    assert!(stderr(&output).contains("--yes") && stderr(&output).contains("terminal"), "{}", text(&output));
    assert_eq!(bytes(&dir), before, "nothing written");
    // --json: one clean document on stdout, the refusal in it
    let output = dir.conf(&["update", "--reason", "adopt this build", "--json"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    let doc = json(&output);
    assert_eq!(doc["command"], Value::from("update"));
    assert!(doc["error"].as_str().unwrap().contains("--yes"), "{doc}");
    assert!(doc["engine"].as_array().unwrap().iter().any(|c| c["key"] == "version"), "{doc}");
    let data = doc["data"].as_array().unwrap_or_else(|| panic!("{doc}"));
    assert!(
        data.iter().any(|c| c["name"] == "molar_gas_constant" && c["old"] == "8.0 J*mol^-1*K^-1" && c["new"] == "8.31446261815324 J*mol^-1*K^-1"),
        "{doc}"
    );
    assert_eq!(bytes(&dir), before, "nothing written");
}

#[test]
fn update_with_yes_writes_records_and_verifies() {
    let dir = Dir::new("update_yes");
    dir.write(FROZEN_FILE_NAME, &file_for_another_engine());
    let output = dir.conf(&["update", "--reason", "adopt this build", "--yes"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let file = FrozenFile::parse(&dir.read(FROZEN_FILE_NAME)).unwrap();
    let out = stdout(&output);
    assert!(out.contains(file.seal_sha256()) && out.contains("record this seal outside this directory"), "{out}");
    assert_eq!(file.engine().get("version"), Some(EngineIdentity::current().version.as_str()));
    assert_eq!(file.engine().get("policy"), Some("release"), "the file's policy is kept");
    let verified = dir.conf(&["verify"]);
    assert_eq!(verified.status.code(), Some(0), "{}", text(&verified));
    let rows = json(&dir.conf(&["history", "--json"]))["rows"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 2);
    assert_eq!((rows[1]["action"].clone(), rows[1]["reason"].clone()), (Value::from("update"), Value::from("adopt this build")));
    // and now there is nothing to update
    let again = dir.conf(&["update", "--reason", "again", "--yes"]);
    assert!(again.status.success() && stdout(&again).contains("nothing to update"), "{}", text(&again));
}

#[test]
fn update_of_a_modified_file_is_refused_exit_2() {
    let dir = Dir::new("update_modified");
    dir.write(FROZEN_FILE_NAME, &file_for_another_engine().replace("[Constants]", "[Constants]\nzz_c = 3 m"));
    let before = bytes(&dir);
    let output = dir.conf(&["update", "--reason", "x", "--yes"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(stderr(&output).contains("seal it or restore it"), "{}", text(&output));
    assert_eq!(bytes(&dir), before);
}

/// The frozen file of `dir` with its overrides (source 2) replaced by `overrides`.
fn edit_overrides(dir: &Dir, overrides: &str) {
    let edited = dir.read(FROZEN_FILE_NAME).replace(EMPTY_OVERRIDES.trim_end(), overrides);
    dir.write(FROZEN_FILE_NAME, &edited);
}

#[test]
fn seal_after_editing_overrides() {
    let dir = Dir::new("seal_edit");
    freeze_cli(&dir, &[]);
    edit_overrides(&dir, "[Units]\nzz_unit = 2.0, L, [zz_unit]\n# my unit");
    assert_eq!(dir.conf(&["verify"]).status.code(), Some(1));
    // without --yes and no terminal: shown, refused, nothing written
    let before = bytes(&dir);
    let output = dir.conf(&["seal", "--reason", "my unit"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert_eq!(bytes(&dir), before);
    let output = dir.conf(&["seal", "--reason", "my unit", "--yes"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let out = stdout(&output);
    // (a) the canonical text of the new overrides (no comment), (b) what they contribute
    assert!(out.contains("zz_unit = 2.0, L, [zz_unit]") && !out.contains("# my unit"), "{out}");
    assert!(out.contains("+ zz_unit"), "{out}");
    let file = FrozenFile::parse(&dir.read(FROZEN_FILE_NAME)).unwrap();
    assert!(out.contains(file.seal_sha256()) && out.contains("record this seal"), "{out}");
    assert_eq!(dir.conf(&["verify"]).status.code(), Some(0));
    let rows = json(&dir.conf(&["history", "--json"]))["rows"].as_array().unwrap().clone();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["action"], Value::from("seal"));
    let output = dir.phs(&["3 zz_unit => m"], &[]);
    assert!(output.status.success() && stdout(&output).contains('6'), "{}", text(&output));
}

#[test]
fn seal_json_carries_the_overrides_and_their_contribution() {
    let dir = Dir::new("seal_json");
    freeze_cli(&dir, &[]);
    edit_overrides(&dir, "[Constants]\nmolar_gas_constant = 8.0 J*mol^-1*K^-1");
    let output = dir.conf(&["seal", "--reason", "r", "--yes", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    let doc = json(&output);
    assert_eq!(doc["command"], Value::from("seal"));
    assert_eq!(doc["overrides"], Value::from("[Constants]\nmolar_gas_constant = 8.0 J*mol^-1*K^-1"));
    let data = doc["data"].as_array().unwrap();
    assert!(data.iter().any(|c| c["change"] == "changed" && c["name"] == "molar_gas_constant" && c["new"] == "8.0 J*mol^-1*K^-1"), "{doc}");
    assert_eq!(doc["seal"].as_str().unwrap(), FrozenFile::parse(&dir.read(FROZEN_FILE_NAME)).unwrap().seal_sha256());
}

#[test]
fn seal_refuses_an_edit_of_source_1_exit_2() {
    let dir = Dir::new("seal_source_1");
    freeze_cli(&dir, &[]);
    dir.write(FROZEN_FILE_NAME, &dir.read(FROZEN_FILE_NAME).replacen(R_VALUE, "= 8.31446261815325", 1));
    let before = bytes(&dir);
    let output = dir.conf(&["seal", "--reason", "x", "--yes"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(stderr(&output).contains("overrides block only"), "{}", text(&output));
    assert_eq!(bytes(&dir), before);
}

/// Review finding 1: a hand edit of the overrides of a file pinned to another engine is not sealed under this one;
/// the only way onto another engine is `update` (spec §7).
#[test]
fn seal_under_an_engine_the_file_does_not_pin_is_exit_3_and_names_update() {
    let dir = Dir::new("seal_other_engine");
    dir.write(FROZEN_FILE_NAME, &file_for_another_engine_with(DEFAULT_PHYSURE_CONF, EMPTY_OVERRIDES));
    edit_overrides(&dir, "[Units]\nzz_unit = 2.0, L, [zz_unit]");
    let before = bytes(&dir);
    let output = dir.conf(&["seal", "--reason", "x", "--yes"]);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert!(stderr(&output).contains("phs conf update") && stderr(&output).contains("0.0.1"), "{}", text(&output));
    assert_eq!(bytes(&dir), before, "nothing written");
    let output = dir.conf(&["seal", "--reason", "x", "--yes", "--json"]);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    let doc = json(&output);
    assert!(doc["error"].as_str().unwrap().contains("phs conf update"), "{doc}");
    assert_eq!(doc["written"], Value::Bool(false));
    assert_eq!(bytes(&dir), before, "nothing written");
}

#[test]
fn seal_on_a_sound_file_is_exit_0_nothing_to_do() {
    let dir = Dir::new("seal_sound");
    freeze_cli(&dir, &[]);
    let before = bytes(&dir);
    let output = dir.conf(&["seal", "--reason", "x"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert!(stdout(&output).contains("already verifies"), "{}", text(&output));
    assert_eq!(bytes(&dir), before);
}
