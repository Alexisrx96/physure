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
    let identity = EngineIdentity { version: "0.0.1".into(), ..EngineIdentity::current() };
    FrozenFile::new(
        "2026-11-08T10:00:00Z",
        FrozenParts {
            engine: engine_pairs(&identity, EnginePolicy::Release, None).unwrap(),
            sources: vec![Source::new("embedded", DEFAULT_PHYSURE_CONF).unwrap(), Source::new("overrides", "").unwrap()],
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
fn unknown_and_unbuilt_commands_are_exit_2() {
    let dir = Dir::new("unknown_command");
    let output = dir.conf(&["frobnicate"]);
    assert_eq!(output.status.code(), Some(2), "{}", text(&output));
    assert!(stderr(&output).contains("status") && stderr(&output).contains("verify"), "{}", text(&output));
    for planned in ["show", "diff", "update", "seal"] {
        let output = dir.conf(&[planned]);
        assert_eq!(output.status.code(), Some(2), "{planned}: {}", text(&output));
        assert!(stderr(&output).contains("not available in this build"), "{planned}: {}", text(&output));
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
