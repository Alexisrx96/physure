//! `phs` checks the configuration in force before it runs anything, and says what is wrong with an exit code:
//! 1 for the integrity of the frozen file, 2 for resolution, 3 for the engine. The real binary runs in a temporary
//! directory of its own, with `PHYSURE_CONF` and `PHYSURE_REQUIRE_FROZEN` set only where a test sets them.

use physure_core::frozen::{engine_pairs, EngineIdentity, EnginePolicy, EntryInput, FrozenFile, FrozenParts, Pairs, Source};
use physure_core::units::conf::DEFAULT_PHYSURE_CONF;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const FROZEN_FILE_NAME: &str = "physure.frozen.conf";

/// A unique temporary directory, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let path = std::env::temp_dir().join(format!("physure_cli_frozen_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Dir(path)
    }

    fn frozen(&self, text: &str) {
        fs::write(self.0.join(FROZEN_FILE_NAME), text).unwrap();
    }

    fn phs(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_phs"));
        command.args(args).current_dir(&self.0).env_remove("PHYSURE_CONF").env_remove("PHYSURE_REQUIRE_FROZEN");
        for (name, value) in env {
            command.env(name, value);
        }
        command.output().expect("phs runs")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn file(engine: Pairs, overrides: &str) -> String {
    FrozenFile::new(
        "2026-11-08T10:00:00Z",
        FrozenParts {
            engine,
            sources: vec![Source::new("embedded", DEFAULT_PHYSURE_CONF).unwrap(), Source::new("overrides", overrides).unwrap()],
            ..FrozenParts::default()
        },
        EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: "test", actor: "t@h", reason: "test", diff: "d" },
    )
    .unwrap()
    .render()
    .unwrap()
}

fn release_engine() -> Pairs {
    engine_pairs(&EngineIdentity::current(), EnginePolicy::Release, None).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_valid_frozen_file_is_the_catalog_of_phs() {
    let dir = Dir::new("valid");
    dir.frozen(&file(release_engine(), "[Units]\nzz_unit = 2.0, L, [zz_unit]\n"));
    let output = dir.phs(&["3 zz_unit => m"], &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(String::from_utf8_lossy(&output.stdout).contains('6'), "{}", String::from_utf8_lossy(&output.stdout));
}

#[test]
fn a_damaged_frozen_file_exits_with_1() {
    let dir = Dir::new("damaged");
    let text = file(release_engine(), "").replacen("avogadro_constant = 6.02214076e+23", "avogadro_constant = 6.02214077e+23", 1);
    dir.frozen(&text);
    let output = dir.phs(&["1 m => ft"], &[]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("refusing to run") && stderr(&output).contains(FROZEN_FILE_NAME), "{}", stderr(&output));
    // not a frozen file at all: the same kind of problem
    dir.frozen("[Units]\nm = 1.0, L\n");
    assert_eq!(dir.phs(&["--version"], &[]).status.code(), Some(1));
}

#[test]
fn a_required_frozen_file_that_is_missing_exits_with_2() {
    let dir = Dir::new("required");
    let output = dir.phs(&["1 m => ft"], &[("PHYSURE_REQUIRE_FROZEN", "1")]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("PHYSURE_REQUIRE_FROZEN"), "{}", stderr(&output));
    let output = dir.phs(&["--version"], &[("PHYSURE_CONF", "missing.conf")]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
}

#[test]
fn another_engine_exits_with_3() {
    let dir = Dir::new("engine");
    let identity = EngineIdentity { version: "9.9.9".into(), ..EngineIdentity::current() };
    dir.frozen(&file(engine_pairs(&identity, EnginePolicy::Release, None).unwrap(), ""));
    let output = dir.phs(&["1 m => ft"], &[]);
    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    assert!(stderr(&output).contains("9.9.9"), "{}", stderr(&output));
}
