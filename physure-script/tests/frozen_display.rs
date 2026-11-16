//! The display rules of a frozen file (`[Settings] display_significant_digits`, `display_scientific_below`,
//! `display_scientific_from`, `uncertainty_digits`) change the text PHS prints, and a file without them prints what
//! the library always printed. A binary of its own: the file's rules apply process-wide, and every other test binary
//! of this crate asserts printed text.
//!
//! These tests change the current directory and the cache of the configuration in force: they serialise on one
//! lock and put everything back when they end.

use physure_core::frozen::active::{reset_cache, ENV_CONF, ENV_REQUIRE, FROZEN_FILE_NAME};
use physure_core::frozen::{engine_pairs, EngineIdentity, EnginePolicy, EntryInput, FrozenFile, FrozenParts, Source};
use physure_core::units::conf::DEFAULT_PHYSURE_CONF;
use physure_script::interpreter::eval_phs;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());

/// A temporary current directory; the previous one and the cache are restored on drop.
struct Dir {
    path: PathBuf,
    previous: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl Dir {
    fn new(name: &str) -> Dir {
        let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for variable in [ENV_CONF, ENV_REQUIRE] {
            assert!(std::env::var_os(variable).is_none(), "{variable} is set in the environment of the tests: unset it");
        }
        let path = std::env::temp_dir().join(format!("physure_phs_frozen_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(&path).unwrap();
        reset_cache();
        Dir { path, previous, _lock: lock }
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous);
        reset_cache();
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// A valid frozen file: the embedded catalog, and `overrides` as source 2.
fn frozen(overrides: &str) -> String {
    FrozenFile::new(
        "2026-11-08T10:00:00Z",
        FrozenParts {
            engine: engine_pairs(&EngineIdentity::current(), EnginePolicy::Release, None).unwrap(),
            sources: vec![Source::new("embedded", DEFAULT_PHYSURE_CONF).unwrap(), Source::new("overrides", overrides).unwrap()],
            ..FrozenParts::default()
        },
        EntryInput { time: "2026-11-08T10:00:00Z", action: "freeze", engine: "test", actor: "t@h", reason: "test", diff: "d" },
    )
    .unwrap()
    .render()
    .unwrap()
}

/// The text PHS prints for the last statement of `source`.
fn printed(source: &str) -> String {
    eval_phs(source).unwrap_or_else(|e| panic!("{source}: {e}")).into_iter().last().map(|v| v.to_string()).unwrap()
}

#[test]
fn the_display_rules_of_a_frozen_file_change_the_printed_text() {
    let dir = Dir::new("display");
    let program = "x = 1/3
x";
    let measured = "e = 625 +/- 40.0195264839553 J
e";
    let tiny = "t = 0.005 m
t";
    assert_eq!(printed(program), "0.333333333333333", "today's text");
    assert_eq!(printed(measured), "630 ± 40 J");
    assert_eq!(printed(tiny), "0.005 m");
    fs::write(
        dir.path.join(FROZEN_FILE_NAME),
        frozen("[Settings]
display_significant_digits = 6
display_scientific_below = 0.01
uncertainty_digits = 2
"),
    )
    .unwrap();
    reset_cache();
    assert_eq!(printed(program), "0.333333");
    assert_eq!(printed(measured), "625 ± 40 J");
    assert_eq!(printed(tiny), "5e-3 m");
    assert_eq!(printed("x = 1/3
x: base"), "0.333333", "x: base follows the digits");
    fs::write(dir.path.join(FROZEN_FILE_NAME), frozen("[Settings]
display_scientific_from = 1000
")).unwrap();
    reset_cache();
    assert_eq!(printed("x = 12345.0
x"), "1.2345e4");
    assert_eq!(printed(program), "0.333333333333333", "a key the file does not set is the library's");
    fs::remove_file(dir.path.join(FROZEN_FILE_NAME)).unwrap();
    reset_cache();
    assert_eq!(printed(program), "0.333333333333333", "the library's rules are back");
    assert_eq!(printed(measured), "630 ± 40 J");
}
