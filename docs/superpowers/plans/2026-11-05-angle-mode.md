# Angle mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A conversion (`=>`, `to`, an annotated parameter, the registry's plan) reads the radian and the steradian as 1, as SI Table 4 does, while sums and comparisons keep refusing every angle against a plain number; `angle_mode = dimension` brings back today's behaviour exactly.

**Architecture:** One new setting (`AngleMode`, copied from `PropagationMode`) and one small piece of catalog data (`[Compatibility]`: which markers a conversion may read as 1, and which unit is `periodic`). A new `RationalUnit::convertible_to` runs the strict dimension comparison first and consults the data only when it fails; it is called from exactly three conversion sites. A boolean `periodic` on `RationalUnit` lets the check see the hertz.

**Tech Stack:** Rust (`physure-core` = package `physure`, lib `physure_core`; `physure-script`), PyO3 (`physure-python/src/lib.rs`), Python 3.11+ with pytest, uv, maturin.

**Spec:** `docs/superpowers/specs/2026-11-05-angle-mode-design.md`. Read it first; this plan does not repeat its reasoning.

**Working rules for this repository** (from `CLAUDE.md` and the maintainer's standing instructions):

- Work only in the worktree `D:\Projects\physure\.claude\worktrees\angle-mode` on branch `feat/angle-mode`. Never commit on `main`. Do not push.
- English only: code, comments, commit messages.
- Run each shell command on its own (no `&&` chains, no `$(...)`, no heredocs). Commit with `git commit -m "<subject>" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"`.
- The first `cargo` build in a fresh worktree compiles every dependency and takes minutes: run it with `run_in_background` and a long timeout.
- Core and script tests that touch process-wide state take `crate::settings::TEST_LOCK` (unit tests inside `physure-core`) and restore what they change.
- `physure-core` and `physure-script` are under the strict Sonar gate: every new branch needs a test in the same task.

## File structure

| File | Responsibility |
|---|---|
| `physure-core/src/units/angle_mode.rs` (create) | `AngleMode`, its process-wide value, the per-thread `scoped` override, and the process-wide grants of `[Compatibility]` |
| `physure-core/src/units/rational.rs` (modify) | the `periodic` field; `MarkerKind`; `RationalUnit::convertible_to` |
| `physure-core/src/units/conf.rs` (modify) | read `[Settings] angle_mode`; read `[Compatibility]` (`periodic`, rows) |
| `physure-core/src/units/physure.conf` (modify) | the default setting, the `[Compatibility]` section, the comment above `radian` |
| `physure-core/src/units/registry.rs`, `physure-core/src/quantity.rs` (modify) | the three conversion sites call `convertible_to` |
| `physure-core/src/units/mod.rs`, `physure-core/src/lib.rs` (modify) | export the module and the public names |
| `physure-core/tests/angle_mode.rs` (create) | the rule at the Rust API: flag, `convertible_to` table, conversions, refused sums, mode independence of sums |
| `physure-script/tests/angle_mode.rs` (create) | the same rule through PHS: conversions, refusals, thread capture |
| `physure-script/tests/angle_mode_golden.rs` (create) + `tests/golden/angle_mode_strict.tsv` (create) | the answers of `main` @ `b27d0f8`, pinned for `Dimension` mode |
| `physure-script/src/interpreter/expressions.rs`, `physure-script/src/builtins/core.rs` (modify) | capture the mode into rayon tasks |
| `physure-script/src/check.rs`, `physure-script/tests/check.rs` (modify) | the checker stays silent on a conversion that differs only in the angle |
| `physure-python/src/lib.rs`, `physure-python/physure/_core.pyi` (modify) | `AngleModeScope`, `angle_mode()` |
| `physure-python/physure/application/context.py`, `physure-python/physure/__init__.py` (modify) | `physure.angle_mode(...)`, `physure.get_angle_mode()` |
| `physure-python/tests/core/test_angle_mode.py` (create) | the Python surface |
| `CHANGELOG.md`, `physure-cli/src/main.rs` (the generator of the guide), `PHYSURE_LANGUAGE_GUIDE.md` and `docs/UNITS.md` (both regenerated) | the user-facing record |
| `physure-crucible` (other repo, Task 14) | the wrapper converts before it subtracts; `F-0001` resolved |

---

### Task 0: Baseline

**Files:** none.

- [ ] **Step 1: Confirm the worktree and the branch**

Run: `git branch --show-current`
Expected: `feat/angle-mode`

Run: `git status --short`
Expected: no output.

- [ ] **Step 2: Build and run the Rust tests once, in the background, to learn the baseline**

Run (with `run_in_background: true`, `timeout: 600000`): `cargo test -p physure -p physure-script`
Expected: finishes. Write down any test that already fails; none of them may be blamed on this work later.

- [ ] **Step 3: Note the baseline in the task list.** No commit.

---

### Task 1: Pin today's answers

The claim "`Dimension` mode is today's behaviour" is tested against answers recorded **before** any core change. This task records them.

**Files:**
- Create: `physure-script/tests/angle_mode_golden.rs`
- Create: `tests/golden/angle_mode_strict.tsv` (written by the test in record mode)

- [ ] **Step 1: Write the test**

Create `physure-script/tests/angle_mode_golden.rs`:

```rust
//! A golden file that pins what the angle work must not change.
//!
//! `tests/golden/angle_mode_strict.tsv` (at the repository root) holds, for every expression of `CORPUS`, what PHS
//! answered on `main` @ `b27d0f8`, before the angle mode existed: the verdict and, for a success, the value, and for a
//! refusal, the text of the error. Once the angle mode exists, the strict mode (`Dimension`) must give the same rows,
//! byte for byte: that is the claim that the strict mode is today's behaviour.
//!
//! The file is written once, on a tree that has no angle mode yet. Recording refuses to overwrite an existing file, so
//! delete it by hand before recording again. On Bash:
//!
//!     RECORD_GOLDEN=1 cargo test -p physure-script --test angle_mode_golden
//!
//! On PowerShell, set the variable first and remove it afterwards:
//!
//!     $env:RECORD_GOLDEN = "1"
//!     cargo test -p physure-script --test angle_mode_golden
//!     Remove-Item Env:RECORD_GOLDEN
//!
//! A recorded value depends on the number formatting and on the error text of the core, so the file may only be
//! re-recorded on a tree that has no angle code (an unmodified `b27d0f8`), never on the feature branch after Task 2.

use physure_script::interpreter::eval_phs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

const CORPUS: &[&str] = &[
    // what a conversion may read as 1
    "(4 kg) * (0.5 m)^2 / 4 * (10 rad/s)^2 => J",
    "(10 rad/s) * (0.5 m) => m/s",
    "(2 m) * (0.5 rad) => m",
    "(10 N*m) * (3 rad/s) => W",
    "(377 rad/s) * (0.1 H) => ohm",
    "(2 rad) => unity",
    "(90 deg) => unity",
    "(100 cd) / (2 m)^2 => lx",
    "(5 lm) => cd",
    "(30 W) / (4 sr) / (2 m)^2 => W/m^2",
    "(2 rad => unity) + 1",
    "(25 kg*m^2*rad^2/s^2 => J) - (25 J)",
    // a rate of angle and a frequency
    "(10 rad/s) => Hz",
    "(10 rad/s) => kHz",
    "(10 Hz) => rad/s",
    "(10 rad/s) / (6.283185307179586 rad) => Hz",
    "(6.283185307179586 rad) * (50 Hz) => rad/s",
    // sums and comparisons that must stay refused
    "(2 rad) + 1",
    "1 + (2 rad)",
    "(5 deg) + (50 %)",
    "(1 rad) == 1",
    "(2 m*rad) + (1 m)",
    "(25 kg*m^2*rad^2/s^2) - (25 J)",
    "(1 rad)^2 + (1 rad)",
    // accepted today, and still
    "(1 rad) + (90 deg)",
    "(10 N*m/rad) * (2 rad) => J",
    "(1 m) + (100 cm)",
    "(20 dB) => percent",
    "(5 Hz) => kHz",
    "sin(90 deg)",
    "sin(2 rad)",
    // refused today, and still
    "(1 deg) => sr",
    "(1 rad) + (1 sr)",
    "(1 deg) + (1 sr)",
    "(1 rad) => dB",
    "sin(9.81 m/s^2)",
    "(1 rad/s) => sr/s",
    "(1 sr) => rad",
];

fn answer(source: &str) -> String {
    let text = match eval_phs(source) {
        Ok(values) => match values.last() {
            Some(value) => format!("ok {value}"),
            None => "ok".to_string(),
        },
        Err(e) => format!("error {e}"),
    };
    assert!(
        !text.contains('\n') && !text.contains('\t'),
        "the answer to {source:?} holds a newline or a tab, which would break the rows of the golden file: {text:?}",
    );
    text
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("tests").join("golden").join("angle_mode_strict.tsv")
}

fn render() -> String {
    CORPUS.iter().map(|source| format!("{source}\t{}\n", answer(source))).collect()
}

#[test]
fn the_answers_are_the_ones_recorded() {
    let current = render();
    if std::env::var("RECORD_GOLDEN").as_deref() == Ok("1") {
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(golden_path())
            .expect("the golden exists: delete it by hand to record again");
        file.write_all(current.as_bytes()).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(golden_path())
        .expect("tests/golden/angle_mode_strict.tsv is missing: record it with RECORD_GOLDEN=1");
    let golden = golden.replace("\r\n", "\n");
    let golden_rows: Vec<&str> = golden.lines().collect();
    let current_rows: Vec<&str> = current.lines().collect();
    assert_eq!(
        current_rows.len(),
        golden_rows.len(),
        "the corpus has {} rows and the golden file has {}",
        current_rows.len(),
        golden_rows.len(),
    );
    let disagreements: Vec<String> = current_rows
        .iter()
        .zip(&golden_rows)
        .filter(|(current_row, golden_row)| current_row != golden_row)
        .map(|(current_row, golden_row)| {
            let (source, got) = current_row.split_once('\t').expect("every row is a source and an answer");
            match golden_row.split_once('\t') {
                Some((golden_source, expected)) if golden_source == source => {
                    format!("{source}: expected {expected}, got {got}")
                }
                _ => format!("{source}: expected the row {golden_row:?}, got {got}"),
            }
        })
        .collect();
    assert!(
        disagreements.is_empty(),
        "{} disagreements with the golden file:\n{}",
        disagreements.len(),
        disagreements.join("\n"),
    );
}
```

- [ ] **Step 2: Record the file, on the unmodified core**

`tests/golden/angle_mode_strict.tsv` must not exist yet: recording refuses to overwrite it (delete it by hand only to record again, and only on a tree that has no angle code).

Run on Bash: `RECORD_GOLDEN=1 cargo test -p physure-script --test angle_mode_golden`
Run on PowerShell: set `$env:RECORD_GOLDEN = "1"` first, run `cargo test -p physure-script --test angle_mode_golden`, then `Remove-Item Env:RECORD_GOLDEN`.
Expected: `test result: ok. 1 passed`

- [ ] **Step 3: Read what was recorded**

Open `tests/golden/angle_mode_strict.tsv`. Expected: 38 lines. The lines `(10 rad/s) / (6.283185307179586 rad) => Hz`, `(6.283185307179586 rad) * (50 Hz) => rad/s`, `(1 rad) + (90 deg)`, `(10 N*m/rad) * (2 rad) => J`, `(1 m) + (100 cm)`, `(20 dB) => percent`, `(5 Hz) => kHz`, `sin(90 deg)` and `sin(2 rad)` answer `ok ...` followed by the value; every other line answers `error ` followed by the core's message: a unit mismatch (`Unit mismatch: expected '...', got '...'`), and `sin expects an angle or a dimensionless value, got 'm/s^2'` for the `sin(9.81 m/s^2)` line. If any other line is `ok`, stop: the corpus does not match the spec's table and the spec must be corrected first.

- [ ] **Step 4: Run it without the switch**

Run: `cargo test -p physure-script --test angle_mode_golden`
Expected: `test result: ok. 1 passed`

- [ ] **Step 5: Commit**

```bash
git add physure-script/tests/angle_mode_golden.rs tests/golden/angle_mode_strict.tsv
git commit -m "test: pin the answers PHS gives before the angle mode" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: The mode

**Files:**
- Create: `physure-core/src/units/angle_mode.rs`
- Modify: `physure-core/src/units/mod.rs`, `physure-core/src/lib.rs`

- [ ] **Step 1: Write the module with its tests** (the tests fail to compile until the module exists, which is the red step)

Create `physure-core/src/units/angle_mode.rs`:

```rust
//! How a conversion reads the radian and the steradian: `[Settings] angle_mode` in `physure.conf`, and which of
//! them `[Compatibility]` lets it read as 1.
//!
//! The SI gives both units the expression 1 (Brochure, Table 4) and writes them in an expression only to say what a
//! quantity involves. `Dimensionless`, the default, lets a *conversion* identify them with 1 where the compatibility
//! rows say so; a sum or a comparison never does. `Dimension` makes the angle a dimension like any other, which is
//! what Physure did before this setting existed.
//!
//! The file's setting is process-wide, like `propagation_mode`'s, and a [`scoped`] override sits on top of it per
//! thread. A rayon worker does not inherit that override: whoever starts parallel work reads [`angle_mode`] first and
//! opens a [`scoped`] one inside each task.

use std::cell::Cell;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// How a conversion treats the radian and the steradian.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AngleMode {
    /// A conversion may read them as 1, as the SI does, where `[Compatibility]` says so.
    #[default]
    Dimensionless,
    /// They are base dimensions, compared like any other.
    Dimension,
}

impl AngleMode {
    const fn as_u8(self) -> u8 {
        match self {
            Self::Dimensionless => 0,
            Self::Dimension => 1,
        }
    }

    const fn from_u8(raw: u8) -> Self {
        match raw {
            1 => Self::Dimension,
            _ => Self::Dimensionless,
        }
    }

    /// The name this mode is written with in a conf file.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dimensionless => "dimensionless",
            Self::Dimension => "dimension",
        }
    }
}

impl FromStr for AngleMode {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dimensionless" => Ok(Self::Dimensionless),
            "dimension" => Ok(Self::Dimension),
            other => Err(format!("unknown angle mode '{other}', expected one of: dimensionless, dimension")),
        }
    }
}

static CONFIGURED: AtomicU8 = AtomicU8::new(0);

thread_local! {
    static OVERRIDE: Cell<Option<AngleMode>> = const { Cell::new(None) };
}

/// The mode in force: a [`scoped`] override if this thread has one, otherwise whatever `physure.conf` asked for.
pub fn angle_mode() -> AngleMode {
    OVERRIDE.with(Cell::get).unwrap_or_else(|| AngleMode::from_u8(CONFIGURED.load(Ordering::Relaxed)))
}

/// Sets the process-wide mode, returning the one it replaced. This is what reading a `physure.conf` calls; for a
/// temporary change, use [`scoped`].
pub fn set_angle_mode(mode: AngleMode) -> AngleMode {
    AngleMode::from_u8(CONFIGURED.swap(mode.as_u8(), Ordering::Relaxed))
}

/// Overrides the mode on this thread until the returned guard is dropped; guards nest.
///
/// ```
/// use physure_core::units::angle_mode::{angle_mode, scoped, AngleMode};
/// {
///     let _guard = scoped(AngleMode::Dimension);
///     assert_eq!(angle_mode(), AngleMode::Dimension);
/// }
/// assert_eq!(angle_mode(), AngleMode::Dimensionless);
/// ```
pub fn scoped(mode: AngleMode) -> AngleModeGuard {
    AngleModeGuard(OVERRIDE.with(|slot| slot.replace(Some(mode))))
}

/// Restores the mode that was in force before its [`scoped`] call, including on unwind: a mode left switched on by a
/// panicking test would silently change every later answer.
pub struct AngleModeGuard(Option<AngleMode>);

impl Drop for AngleModeGuard {
    fn drop(&mut self) {
        OVERRIDE.with(|slot| slot.set(self.0));
    }
}

static RADIAN_READ_AS_ONE: AtomicBool = AtomicBool::new(true);
static STERADIAN_READ_AS_ONE: AtomicBool = AtomicBool::new(true);

/// Whether a conversion may read the base unit `marker` (`"rad"` or `"sr"`) as 1, as `[Compatibility]` says.
pub fn marker_converts_to_plain(marker: &str) -> bool {
    match marker {
        "rad" => RADIAN_READ_AS_ONE.load(Ordering::Relaxed),
        "sr" => STERADIAN_READ_AS_ONE.load(Ordering::Relaxed),
        _ => false,
    }
}

/// Sets which markers a conversion may read as 1: what reading a `[Compatibility]` section calls.
pub fn set_marker_grants(radian: bool, steradian: bool) {
    RADIAN_READ_AS_ONE.store(radian, Ordering::Relaxed);
    STERADIAN_READ_AS_ONE.store(steradian, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::TEST_LOCK;

    #[test]
    fn every_spelling_the_conf_may_use_round_trips() {
        for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
            assert_eq!(mode.name().parse::<AngleMode>(), Ok(mode));
        }
        assert_eq!("  Dimension ".parse::<AngleMode>(), Ok(AngleMode::Dimension));
    }

    #[test]
    fn an_unknown_mode_names_the_choices() {
        let why = "degrees".parse::<AngleMode>().unwrap_err();
        assert!(why.contains("'degrees'"), "{why}");
        assert!(why.contains("dimensionless, dimension"), "{why}");
    }

    #[test]
    fn a_scope_nests_and_restores() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = angle_mode();
        {
            let _outer = scoped(AngleMode::Dimension);
            assert_eq!(angle_mode(), AngleMode::Dimension);
            {
                let _inner = scoped(AngleMode::Dimensionless);
                assert_eq!(angle_mode(), AngleMode::Dimensionless);
            }
            assert_eq!(angle_mode(), AngleMode::Dimension);
        }
        assert_eq!(angle_mode(), before);
    }

    #[test]
    fn a_scope_restores_on_unwind() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = angle_mode();
        let result = std::panic::catch_unwind(|| {
            let _guard = scoped(AngleMode::Dimension);
            panic!("a test that leaves a scope open");
        });
        assert!(result.is_err());
        assert_eq!(angle_mode(), before);
    }

    #[test]
    fn a_scope_belongs_to_its_thread() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let process_wide = angle_mode();
        let _guard = scoped(AngleMode::Dimension);
        let seen = std::thread::spawn(angle_mode).join().unwrap();
        assert_eq!(seen, process_wide);
    }

    #[test]
    fn the_process_wide_mode_is_set_and_the_old_one_returned() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = set_angle_mode(AngleMode::Dimension);
        assert_eq!(angle_mode(), AngleMode::Dimension);
        assert_eq!(set_angle_mode(original), AngleMode::Dimension);
    }

    #[test]
    fn only_the_two_markers_can_be_granted() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_marker_grants(true, false);
        assert!(marker_converts_to_plain("rad"));
        assert!(!marker_converts_to_plain("sr"));
        assert!(!marker_converts_to_plain("m"));
        set_marker_grants(true, true);
    }
}
```

- [ ] **Step 2: Export it**

In `physure-core/src/units/mod.rs`, replace `pub mod conf;` with:

```rust
pub mod angle_mode;
pub mod conf;
```

In `physure-core/src/lib.rs`, after the line `pub use units::{RationalUnit, UnitRegistry, DimVector, UnitKind};` add:

```rust
pub use units::angle_mode::{angle_mode, set_angle_mode, AngleMode};
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p physure --lib angle_mode`
Expected: 7 tests pass (the doctest runs under `cargo test -p physure --doc angle_mode`, also green).

- [ ] **Step 4: Commit**

```bash
git add physure-core/src/units/angle_mode.rs physure-core/src/units/mod.rs physure-core/src/lib.rs
git commit -m "feat(core): the angle mode and the grants a conversion reads" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `[Settings] angle_mode`

**Files:**
- Modify: `physure-core/src/units/conf.rs` (`apply_setting`, tests), `physure-core/src/units/physure.conf`

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `physure-core/src/units/conf.rs`, after `parses_max_loop_iterations_from_settings_section`, add:

```rust
    #[test]
    fn the_embedded_conf_sets_the_default_angle_mode() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        assert_eq!(crate::units::angle_mode::angle_mode(), crate::units::angle_mode::AngleMode::Dimensionless);
    }

    #[test]
    fn parses_angle_mode_from_settings_section() {
        use crate::units::angle_mode::{angle_mode, set_angle_mode, AngleMode};
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        let original = set_angle_mode(AngleMode::Dimensionless);
        parse_physure_conf("[Settings]\nangle_mode = dimension\n", &mut reg, &mut constants);
        assert_eq!(angle_mode(), AngleMode::Dimension);
        set_angle_mode(original);
    }

    #[test]
    fn a_bad_angle_mode_keeps_the_one_in_force() {
        use crate::units::angle_mode::{angle_mode, set_angle_mode, AngleMode};
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        let original = set_angle_mode(AngleMode::Dimensionless);
        parse_physure_conf("[Settings]\nangle_mode = degrees\n", &mut reg, &mut constants);
        assert_eq!(angle_mode(), AngleMode::Dimensionless);
        set_angle_mode(original);
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p physure --lib units::conf::tests::parses_angle_mode`
Expected: FAIL (`left: Dimensionless, right: Dimension`): the key is not read yet.

- [ ] **Step 3: Read the key**

In `physure-core/src/units/conf.rs`, change the import on line 1 to:

```rust
use crate::uncertainty::{PropagationMode, set_propagation_mode};
use crate::units::angle_mode::{set_angle_mode, AngleMode};
```

and in `apply_setting`, before the `"parallel_threshold"` arm, add:

```rust
        "angle_mode" => match raw.parse::<AngleMode>() {
            Ok(mode) => {
                set_angle_mode(mode);
            }
            // Same reasoning as `propagation_mode`: a conf is read before there is anywhere to report to, and the
            // mode it names is not worth refusing to start over.
            Err(why) => eprintln!("physure.conf: {why}; keeping the angle mode in force"),
        },
```

- [ ] **Step 4: Write the default into the embedded conf**

In `physure-core/src/units/physure.conf`, replace the line `propagation_mode = correlated` with:

```
propagation_mode = correlated
# How a conversion reads the radian and the steradian. `dimensionless` is the SI (Brochure, Table 4: both are 1 in
# an expression): `=>`, `to` and a unit annotation may identify them with 1 where [Compatibility] says so.
# `dimension` makes the angle a dimension like any other. A sum or a comparison never reads them as 1.
angle_mode = dimensionless
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p physure --lib units::conf`
Expected: all pass, including the three new ones.

- [ ] **Step 6: Commit**

```bash
git add physure-core/src/units/conf.rs physure-core/src/units/physure.conf
git commit -m "feat(core): read [Settings] angle_mode from physure.conf" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The `periodic` flag on a unit

**Files:**
- Modify: `physure-core/src/units/rational.rs`, `physure-core/src/units/conf.rs`, `physure-core/src/units/physure.conf`
- Create: `physure-core/tests/angle_mode.rs`

- [ ] **Step 1: Write the failing test file**

Create `physure-core/tests/angle_mode.rs`:

```rust
//! The angle mode at the Rust API: the `periodic` flag, what a conversion may read as 1, and what nothing may.

use physure_core::units::{Parser, RationalUnit};

fn unit(text: &str) -> RationalUnit {
    Parser::parse_expression(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

#[test]
fn the_hertz_is_periodic_under_every_spelling() {
    for spelling in ["Hz", "hertz", "kHz", "kilohertz", "MHz", "mHz"] {
        assert!(unit(spelling).periodic, "{spelling} should carry the periodic flag");
    }
}

#[test]
fn nothing_else_is_periodic() {
    for text in ["s^-1", "1/s", "Hz*rad", "Hz^2", "Hz/Hz", "rad/s", "m/s", "kg*m/s^2", "rad", "unity"] {
        assert!(!unit(text).periodic, "{text} should not carry the periodic flag");
    }
}

#[test]
fn the_flag_is_not_part_of_equality() {
    assert_eq!(unit("Hz"), unit("s^-1"));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p physure --test angle_mode`
Expected: FAIL to compile: `no field 'periodic' on type RationalUnit`.

- [ ] **Step 3: Add the field and keep it where `display_name` is kept**

In `physure-core/src/units/rational.rs`, after `pub display_name: Option<String>,` in the struct add:

```rust
    /// Set on the hertz, with its prefixes and aliases, by `[Compatibility] periodic`: the SI reserves it for
    /// periodic phenomena (Brochure, Table 4, footnote d), so a conversion never reads a marker as 1 against it.
    /// It is not part of equality or of the hash: `Hz` and `s^-1` are the same unit.
    pub periodic: bool,
```

Then the five struct literals (the plan's spec lists them: `new_from_dimensions`, `dimensionless`, `mul`, `div`, `pow`):

1. In `new_from_dimensions`, replace `id, display_name: None }` at the end of the `RationalUnit { dimensions, scale: 1.0, ... }` line with `id, display_name: None, periodic: false }`.
2. In `dimensionless()`, replace
```rust
            id: 0,
            display_name: None,
        }
```
with
```rust
            id: 0,
            display_name: None,
            periodic: false,
        }
```
3. In `mul`, replace
```rust
        let display_name = if other.dimensions.is_empty() && (other.scale - 1.0).abs() < 1e-9 {
            self.display_name.clone()
        } else if self.dimensions.is_empty() && (self.scale - 1.0).abs() < 1e-9 {
            other.display_name.clone()
        } else {
            None
        };
```
with
```rust
        let (display_name, periodic) = if other.dimensions.is_empty() && (other.scale - 1.0).abs() < 1e-9 {
            (self.display_name.clone(), self.periodic)
        } else if self.dimensions.is_empty() && (self.scale - 1.0).abs() < 1e-9 {
            (other.display_name.clone(), other.periodic)
        } else {
            (None, false)
        };
```
4. In `div`, replace
```rust
        let display_name = if other.dimensions.is_empty() && (other.scale - 1.0).abs() < 1e-9 {
            self.display_name.clone()
        } else {
            None
        };
```
with
```rust
        let (display_name, periodic) = if other.dimensions.is_empty() && (other.scale - 1.0).abs() < 1e-9 {
            (self.display_name.clone(), self.periodic)
        } else {
            (None, false)
        };
```
5. The final literal of `mul` and of `div` is the same line, which appears twice. Replace both occurrences of
`RationalUnit { dimensions: new_dims, scale, exact, offset: 0.0, exact_offset: None, log, id, display_name }`
with
`RationalUnit { dimensions: new_dims, scale, exact, offset: 0.0, exact_offset: None, log, id, display_name, periodic }`.
6. In `pow`, replace `log, id, display_name: None }` with `log, id, display_name: None, periodic: false }`.

Equality and the hash are not touched.

- [ ] **Step 4: Read `periodic` from the conf and flag the stored unit**

In `physure-core/src/units/conf.rs`, in `parse_conf`:

After `let mut unit_lines: Vec<&str> = Vec::new();` add:

```rust
    let mut periodic_names: Vec<String> = Vec::new();
```

In the `match current_section` of pass 1, after the `"Categories"` arm add:

```rust
            "Compatibility" => read_compatibility_line(line, &mut periodic_names),
```

After the pass-2 loop (the `for line in unit_lines { ... }`), add:

```rust
    mark_periodic(registry, &periodic_names);
```

and add these two functions after `read_category`:

```rust
/// One line of `[Compatibility]`. `periodic = hertz` names units that the SI reserves for periodic phenomena; the
/// other lines are rows, read in a later step of the same file.
fn read_compatibility_line(line: &str, periodic: &mut Vec<String>) {
    if let Some((key, names)) = line.split_once('=') {
        if key.trim() == "periodic" {
            periodic.extend(names.split(',').map(str::trim).filter(|name| !name.is_empty()).map(str::to_string));
        }
    }
}

/// Flags the units that `[Compatibility] periodic` names, once they are registered. A name is any spelling of the
/// unit (`hertz`); the flag goes on the unit the registry keeps, so that its prefixes (built from a copy of it) and
/// its aliases carry it.
fn mark_periodic(registry: &mut UnitRegistry, names: &[String]) {
    for name in names {
        let symbol = registry.resolve_symbol(name);
        if let Some(unit) = registry.derived_units.get_mut(&symbol) {
            unit.periodic = true;
        }
    }
}
```

- [ ] **Step 5: Name the unit in the embedded conf**

At the end of `physure-core/src/units/physure.conf`, replace the last line `area = ["m^2", "cm^2", "km^2", "ha", "acre"]` with:

```
area = ["m^2", "cm^2", "km^2", "ha", "acre"]

[Compatibility]
# The SI reserves the hertz for periodic phenomena (Brochure, Table 4, footnote d), and a frequency and an angular
# frequency differ by a factor of 2*pi. A unit named here, with its prefixes and aliases, is not converted into one
# that carries a marker: `rad/s => Hz` is refused, `(rad/s) / (2*pi*rad) => Hz` is not.
periodic = hertz
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p physure --test angle_mode`
Expected: 3 tests pass. If `the_hertz_is_periodic_under_every_spelling` fails for one spelling, read that spelling's path in `Parser::parse_expression` (`parser.rs`, `resolve_symbol`) before changing anything: the flag must come from `get_unit`.

Run: `cargo test -p physure --lib`
Expected: pass (the unit-equality and hash tests prove nothing else moved).

- [ ] **Step 7: Commit**

```bash
git add physure-core/src/units/rational.rs physure-core/src/units/conf.rs physure-core/src/units/physure.conf physure-core/tests/angle_mode.rs
git commit -m "feat(core): flag the hertz as periodic, with its prefixes and aliases" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The rows of `[Compatibility]`

**Files:**
- Modify: `physure-core/src/units/conf.rs`, `physure-core/src/units/physure.conf`

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `conf.rs`, add:

```rust
    fn marker_grants() -> (bool, bool) {
        use crate::units::angle_mode::marker_converts_to_plain;
        (marker_converts_to_plain("rad"), marker_converts_to_plain("sr"))
    }

    fn restore_marker_grants() {
        crate::units::angle_mode::set_marker_grants(true, true);
    }

    #[test]
    fn the_embedded_conf_lets_a_conversion_read_both_markers_as_one() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        assert_eq!(marker_grants(), (true, true));
    }

    #[test]
    fn a_compatibility_section_replaces_the_rows() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        parse_physure_conf("[Compatibility]\nradian, plain = convert\n", &mut reg, &mut constants);
        assert_eq!(marker_grants(), (true, false));
        parse_physure_conf("[Compatibility]\nplain, steradian = convert\n", &mut reg, &mut constants);
        assert_eq!(marker_grants(), (false, true));
        restore_marker_grants();
    }

    #[test]
    fn an_empty_compatibility_section_grants_nothing() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        parse_physure_conf("[Compatibility]\n", &mut reg, &mut constants);
        assert_eq!(marker_grants(), (false, false));
        restore_marker_grants();
    }

    #[test]
    fn a_conf_without_the_section_keeps_the_rows() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        parse_physure_conf("[Settings]\nparallel_threshold = 10000\n", &mut reg, &mut constants);
        assert_eq!(marker_grants(), (true, true));
    }

    #[test]
    fn a_row_that_asks_for_a_sum_or_a_comparison_grants_nothing() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        for row in ["radian, plain = convert add", "radian, plain = compare", "radian, plain = add compare convert"] {
            parse_physure_conf(&format!("[Compatibility]\n{row}\n"), &mut reg, &mut constants);
            assert_eq!(marker_grants(), (false, false), "{row}");
        }
        restore_marker_grants();
    }

    #[test]
    fn two_markers_do_not_convert_into_each_other_and_a_plain_unit_is_not_a_marker() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        for row in ["radian, steradian = convert", "metre, plain = convert", "plain, plain = convert", "radian = convert"] {
            parse_physure_conf(&format!("[Compatibility]\n{row}\n"), &mut reg, &mut constants);
            assert_eq!(marker_grants(), (false, false), "{row}");
        }
        restore_marker_grants();
    }

    #[test]
    fn a_system_conf_does_not_change_the_grants_but_can_name_a_periodic_unit() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let (mut reg, mut constants) = build_registry_from_conf();
        extend_registry(&mut reg, &mut constants, "[Compatibility]\nperiodic = becquerel\n").unwrap();
        assert_eq!(marker_grants(), (true, true));
        extend_registry(&mut reg, &mut constants, "[Compatibility]\n").unwrap();
        assert_eq!(marker_grants(), (true, true));
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p physure --lib units::conf::tests::a_compatibility_section_replaces_the_rows`
Expected: FAIL (`left: (true, true), right: (true, false)`): rows are not read yet.

- [ ] **Step 3: Read the rows**

In `conf.rs`, replace the Task 4 `read_compatibility_line` with this version, and add the three functions after it:

```rust
/// One line of `[Compatibility]`. `periodic = hertz` names units that the SI reserves for periodic phenomena; any
/// other line is a row, `radian, plain = convert`, kept for `apply_compatibility`.
fn read_compatibility_line(line: &str, periodic: &mut Vec<String>, rows: &mut Vec<String>) {
    match line.split_once('=') {
        Some((key, names)) if key.trim() == "periodic" => {
            periodic.extend(names.split(',').map(str::trim).filter(|name| !name.is_empty()).map(str::to_string));
        }
        _ => rows.push(line.to_string()),
    }
}

/// The marker a row of `[Compatibility]` lets a conversion read as 1.
enum Marker {
    Radian,
    Steradian,
}

/// A row is `unit, plain = convert`, with the unit one of the two markers, in either order. Anything else is not
/// supported yet: a sum or a comparison never reads a marker as 1 (Python decides those from `Dimension`, which stays
/// strict), and two markers do not convert into each other.
fn read_compatibility_row(row: &str) -> Result<Marker, String> {
    let Some((kinds, operations)) = row.split_once('=') else {
        return Err(format!("[Compatibility] '{row}' is not `kind, kind = operations`"));
    };
    let operations: Vec<&str> = operations.split_whitespace().collect();
    if operations != ["convert"] {
        return Err(format!(
            "[Compatibility] '{row}': only `convert` is supported, a sum or a comparison never reads a marker as 1"
        ));
    }
    let kinds: Vec<&str> = kinds.split(',').map(str::trim).collect();
    let marker = match kinds.as_slice() {
        [marker, "plain"] | ["plain", marker] => *marker,
        _ => return Err(format!("[Compatibility] '{row}': a row pairs a marker unit with `plain`")),
    };
    match marker {
        "radian" | "rad" => Ok(Marker::Radian),
        "steradian" | "sr" => Ok(Marker::Steradian),
        other => Err(format!("[Compatibility] '{row}': '{other}' is not a marker unit (radian or steradian)")),
    }
}

/// Replaces the process-wide grants by the rows of a `[Compatibility]` section. A row that cannot be read is
/// reported on stderr and grants nothing.
fn apply_compatibility(rows: &[String]) {
    let (mut radian, mut steradian) = (false, false);
    for row in rows {
        match read_compatibility_row(row) {
            Ok(Marker::Radian) => radian = true,
            Ok(Marker::Steradian) => steradian = true,
            Err(why) => eprintln!("physure.conf: {why}; the row is ignored"),
        }
    }
    crate::units::angle_mode::set_marker_grants(radian, steradian);
}
```

In `parse_conf`:

- after `let mut periodic_names: Vec<String> = Vec::new();` add:
```rust
    let mut compatibility_rows: Vec<String> = Vec::new();
    let mut has_compatibility = false;
```
- in the section-header branch, replace
```rust
            current_section = &line[1..line.len() - 1];
            continue;
```
with
```rust
            current_section = &line[1..line.len() - 1];
            has_compatibility |= current_section == "Compatibility";
            continue;
```
- replace the Task 4 arm with
```rust
            "Compatibility" => read_compatibility_line(line, &mut periodic_names, &mut compatibility_rows),
```
- after `mark_periodic(registry, &periodic_names);` add
```rust
    if apply_settings && has_compatibility {
        apply_compatibility(&compatibility_rows);
    }
```

- [ ] **Step 4: Write the rows into the embedded conf**

In `physure.conf`, replace the `[Compatibility]` section of Task 4 with:

```
[Compatibility]
# Which base units a conversion may read as 1 (SI Brochure, Table 4: the expression of the radian and of the
# steradian is 1). `unit, plain = convert` lets `=>`, `to` and a unit annotation identify that unit with 1 when the
# rest of the unit agrees: `m*rad/s => m/s`, `cd/m^2 => lx`. Only `convert` is supported: a sum or a comparison
# never reads a marker as 1. A conf that has this section replaces these rows; one that has none keeps them.
radian, plain = convert
steradian, plain = convert
# The SI reserves the hertz for periodic phenomena (Brochure, Table 4, footnote d), and a frequency and an angular
# frequency differ by a factor of 2*pi. A unit named here, with its prefixes and aliases, is not converted into one
# that carries a marker: `rad/s => Hz` is refused, `(rad/s) / (2*pi*rad) => Hz` is not.
periodic = hertz
```

Also in the same file replace the comment line `# Note: Unity '1' is dimensionless. Radians are Angles (A).` with:

```
# Note: Unity '1' is dimensionless. The radian and the steradian are the base units of the angle dimension (A); the
# SI gives both the expression 1, see [Compatibility].
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p physure --lib units::conf`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add physure-core/src/units/conf.rs physure-core/src/units/physure.conf
git commit -m "feat(core): read the rows of [Compatibility], conversions only" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: `convertible_to`

**Files:**
- Modify: `physure-core/src/units/rational.rs`
- Test: `physure-core/tests/angle_mode.rs`

- [ ] **Step 1: Write the failing table test**

Append to `physure-core/tests/angle_mode.rs`:

```rust
use physure_core::units::angle_mode::{scoped, AngleMode};

/// Conversions that the strict comparison already accepts, or that the SI licenses by reading a marker as 1.
const CONVERTIBLE: &[(&str, &str)] = &[
    // strict: identical base exponents
    ("Hz", "s^-1"),
    ("kHz", "Hz"),
    ("rad", "deg"),
    ("dB", "percent"),
    ("Hz*rad", "rad/s"),
    // a marker read as 1
    ("m*rad/s", "m/s"),
    ("m/s", "m*rad/s"),
    ("rad", "unity"),
    ("deg", "unity"),
    ("unity", "rad"),
    ("kg*m^2*rad^2/s^2", "J"),
    ("cd/m^2", "lx"),
    ("lm", "cd"),
    ("rad/s", "s^-1"),
];

/// Conversions that no mode allows.
const NEVER: &[(&str, &str)] = &[
    // a rate of angle is not a frequency
    ("rad/s", "Hz"),
    ("rad/s", "kHz"),
    ("rad/s", "hertz"),
    ("Hz", "rad/s"),
    // two markers do not convert into each other, whatever they carry
    ("rad", "sr"),
    ("sr", "rad"),
    ("deg", "sr"),
    ("rad/s", "sr/s"),
    ("rad^2", "rad"),
    ("m", "m*rad*sr"),
    // a level is not an angle
    ("rad", "dB"),
    ("sr", "dB"),
    // the physical part differs
    ("m*rad", "m^2"),
];

#[test]
fn the_default_mode_reads_a_marker_as_one_in_a_conversion() {
    let _mode = scoped(AngleMode::Dimensionless);
    for (from, to) in CONVERTIBLE {
        assert!(unit(from).convertible_to(&unit(to)), "{from} => {to} should convert");
    }
}

#[test]
fn no_mode_converts_what_the_si_does_not_license() {
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        for (from, to) in NEVER {
            assert!(!unit(from).convertible_to(&unit(to)), "{from} => {to} should be refused in {mode:?}");
        }
    }
}

#[test]
fn the_strict_mode_converts_only_what_has_identical_exponents() {
    let _mode = scoped(AngleMode::Dimension);
    for (from, to) in CONVERTIBLE {
        let strict = unit(from).same_dimensions(&unit(to));
        assert_eq!(unit(from).convertible_to(&unit(to)), strict, "{from} => {to}");
    }
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p physure --test angle_mode`
Expected: FAIL to compile: `no method named 'convertible_to'`.

- [ ] **Step 3: Implement it**

In `physure-core/src/units/rational.rs`, add to the imports at the top:

```rust
use crate::units::angle_mode::{angle_mode, marker_converts_to_plain, AngleMode};
```

After `pub type DimVec = ...;` add:

```rust
/// The base units that the SI gives the expression 1 (Brochure, Table 4): in an expression they say what a quantity
/// involves, and a conversion may read them as 1.
const MARKERS: [&str; 2] = ["rad", "sr"];

/// What is left of a unit when its physical dimension is set aside: the markers it carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkerKind {
    Plain,
    Radian,
    Steradian,
    Mixed,
}

impl MarkerKind {
    /// Whether a conversion may read this kind as the plain unit, as `[Compatibility]` says.
    fn reads_as_plain(self) -> bool {
        match self {
            Self::Radian => marker_converts_to_plain("rad"),
            Self::Steradian => marker_converts_to_plain("sr"),
            Self::Plain | Self::Mixed => false,
        }
    }
}
```

In `impl RationalUnit`, after `same_dimensions`, add:

```rust
    /// The base units of this unit that are not markers, with their exponents: its physical part.
    fn physical_part(&self) -> impl Iterator<Item = &(String, (i64, i64))> {
        self.dimensions.iter().filter(|(symbol, _)| !MARKERS.contains(&symbol.as_str()))
    }

    fn marker_kind(&self) -> MarkerKind {
        match (self.get_exponent("rad").is_some(), self.get_exponent("sr").is_some()) {
            (false, false) => MarkerKind::Plain,
            (true, false) => MarkerKind::Radian,
            (false, true) => MarkerKind::Steradian,
            (true, true) => MarkerKind::Mixed,
        }
    }

    /// Whether a *conversion* from this unit to `target` is allowed.
    ///
    /// Identical base exponents always convert, as they always did. Otherwise, in `Dimensionless` mode, a marker
    /// (`rad`, `sr`) may be read as 1 when the rest of the unit agrees and `[Compatibility]` grants it: `m·rad/s`
    /// converts to `m/s`, `deg` to `unity`, `cd/m²` to `lx`. It never does for a level, for the `periodic` hertz
    /// (a frequency and an angular frequency differ by 2π), for one marker against another, or for two exponents
    /// of the same marker. A sum or a comparison does not call this: it keeps `same_dimensions`.
    pub fn convertible_to(&self, target: &Self) -> bool {
        if self.same_dimensions(target) {
            return true;
        }
        if angle_mode() == AngleMode::Dimension
            || self.is_logarithmic()
            || target.is_logarithmic()
            || self.periodic
            || target.periodic
            || !self.physical_part().eq(target.physical_part())
        {
            return false;
        }
        match (self.marker_kind(), target.marker_kind()) {
            (MarkerKind::Plain, kind) | (kind, MarkerKind::Plain) => kind.reads_as_plain(),
            _ => false,
        }
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p physure --test angle_mode`
Expected: all pass. A failure names the pair; check it against the table in the spec before changing the code.

- [ ] **Step 5: Commit**

```bash
git add physure-core/src/units/rational.rs physure-core/tests/angle_mode.rs
git commit -m "feat(core): RationalUnit::convertible_to reads a marker as 1 for a conversion" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Call it from the three conversion sites

**Files:**
- Modify: `physure-core/src/quantity.rs` (`convert_to`), `physure-core/src/units/registry.rs` (`factor_between`, `conversion`)
- Test: `physure-core/tests/angle_mode.rs`

- [ ] **Step 1: Write the failing tests**

Append to `physure-core/tests/angle_mode.rs`:

```rust
use physure_core::quantity::Quantity;

fn q(value: f64, unit: &str) -> Quantity {
    Quantity::new(value, unit).unwrap_or_else(|e| panic!("{unit}: {e}"))
}

fn rotating_disk_energy() -> Quantity {
    let (mass, radius, omega, half) = (q(4.0, "kg"), q(0.5, "m"), q(10.0, "rad/s"), q(0.5, ""));
    let inertia = mass.mul(&radius.pow(2.0).unwrap()).unwrap().mul(&half).unwrap();
    inertia.mul(&omega.pow(2.0).unwrap()).unwrap().mul(&half).unwrap()
}

#[test]
fn the_energy_of_a_rotating_disk_converts_to_joules() {
    let _mode = scoped(AngleMode::Dimensionless);
    let joules = rotating_disk_energy().to("J").unwrap();
    assert!((joules.value.mean() - 25.0).abs() < 1e-12, "{}", joules.value.mean());
    assert_eq!(joules.unit.__repr__(), "J");
}

#[test]
fn a_speed_from_an_angular_velocity_converts_to_metres_per_second() {
    let _mode = scoped(AngleMode::Dimensionless);
    let speed = q(10.0, "rad/s").mul(&q(0.5, "m")).unwrap().to("m/s").unwrap();
    assert!((speed.value.mean() - 5.0).abs() < 1e-12);
}

#[test]
fn the_strict_mode_refuses_both() {
    let _mode = scoped(AngleMode::Dimension);
    assert!(rotating_disk_energy().to("J").is_err());
    assert!(q(10.0, "rad/s").mul(&q(0.5, "m")).unwrap().to("m/s").is_err());
}

#[test]
fn a_rate_of_angle_does_not_convert_to_a_frequency() {
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        assert!(q(10.0, "rad/s").to("Hz").is_err(), "{mode:?}");
        assert!(q(10.0, "rad/s").to("kHz").is_err(), "{mode:?}");
        assert!(q(10.0, "Hz").to("rad/s").is_err(), "{mode:?}");
    }
}

#[test]
fn dividing_by_two_pi_radians_is_the_way_to_a_frequency() {
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        let hertz = q(10.0, "rad/s").div(&q(std::f64::consts::TAU, "rad")).unwrap().to("Hz").unwrap();
        assert!((hertz.value.mean() - 10.0 / std::f64::consts::TAU).abs() < 1e-12, "{mode:?}");
    }
}

#[test]
fn a_sum_or_a_comparison_never_reads_a_marker_as_one() {
    let refused = [
        (2.0, "rad", 1.0, ""),
        (5.0, "deg", 50.0, "percent"),
        (2.0, "m*rad", 1.0, "m"),
        (25.0, "kg*m^2*rad^2/s^2", 25.0, "J"),
        (1.0, "rad^2", 1.0, "rad"),
        (1.0, "rad", 1.0, "sr"),
    ];
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        for (a, unit_a, b, unit_b) in refused {
            let (left, right) = (q(a, unit_a), q(b, unit_b));
            assert!(left.add(&right).is_err(), "{unit_a} + {unit_b} in {mode:?}");
            assert!(left.sub(&right).is_err(), "{unit_a} - {unit_b} in {mode:?}");
            assert!(!left.approx_eq(&right, 1e-9, 1e-12), "{unit_a} ~ {unit_b} in {mode:?}");
        }
    }
}

#[test]
fn an_explicit_conversion_first_makes_the_sum_possible() {
    let _mode = scoped(AngleMode::Dimensionless);
    let converted = q(2.0, "rad").to("unity").unwrap();
    let sum = converted.add(&q(1.0, "")).unwrap();
    assert!((sum.value.mean() - 3.0).abs() < 1e-12);
}

const UNITS: &[&str] = &[
    "rad", "deg", "sr", "unity", "percent", "m", "m*rad", "m*sr", "kg*m^2*rad^2/s^2", "J", "Hz", "kHz", "rad/s",
    "s^-1", "cd", "lm", "lx", "cd/m^2", "dB", "rad^2", "N*m", "W", "W/sr",
];

/// What a sum, a difference and a comparison answer, as a comparable value.
fn operators(a: &Quantity, b: &Quantity) -> (Result<(f64, String), String>, Result<(f64, String), String>, bool) {
    let shown = |r: Result<Quantity, physure_core::PhysureError>| {
        r.map(|q| (q.value.mean(), q.unit.__repr__())).map_err(|e| e.to_string())
    };
    (shown(a.add(b)), shown(a.sub(b)), a.approx_eq(b, 1e-9, 1e-12))
}

#[test]
fn sums_and_comparisons_do_not_depend_on_the_angle_mode() {
    for first in UNITS {
        for second in UNITS {
            let (a, b) = (q(2.0, first), q(3.0, second));
            let default = {
                let _mode = scoped(AngleMode::Dimensionless);
                operators(&a, &b)
            };
            let strict = {
                let _mode = scoped(AngleMode::Dimension);
                operators(&a, &b)
            };
            assert_eq!(default, strict, "{first} with {second}");
        }
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p physure --test angle_mode`
Expected: `the_energy_of_a_rotating_disk_converts_to_joules`, `a_speed_from_an_angular_velocity_converts_to_metres_per_second` and `an_explicit_conversion_first_makes_the_sum_possible` FAIL with a unit mismatch; the others pass (the sums and the strict mode are already today's behaviour).

- [ ] **Step 3: Call `convertible_to` at the three sites**

In `physure-core/src/quantity.rs`, in `convert_to`, replace `if !self.unit.same_dimensions(target) {` with `if !self.unit.convertible_to(target) {` and replace the doc comment line `/// Errors if `target` has different physical dimensions.` with:

```rust
    /// Errors if `target` has different physical dimensions. In `Dimensionless` angle mode it may read `rad` and
    /// `sr` as 1 (see `RationalUnit::convertible_to`); a sum or a comparison never does.
```

In `physure-core/src/units/registry.rs`, in `factor_between` replace `if !from.same_dimensions(&to) {` with `if !from.convertible_to(&to) {`, and do the same in `conversion`. (The two lines are identical text; both are replaced. Leave every other `same_dimensions` call in the repository as it is.)

- [ ] **Step 4: Run the tests**

Run: `cargo test -p physure --test angle_mode`
Expected: all pass, including `sums_and_comparisons_do_not_depend_on_the_angle_mode` (it covers 23 × 23 pairs).

Run: `cargo test -p physure`
Expected: no new failure against the Task 0 baseline. A failure in `units/tests.rs` that pins the old refusal of a conversion across `rad` is a test of the old default: move it under `scoped(AngleMode::Dimension)` and say so in the commit message; do not touch a test that pins a sum or a comparison.

- [ ] **Step 5: Commit**

```bash
git add physure-core/src/quantity.rs physure-core/src/units/registry.rs physure-core/tests/angle_mode.rs
git commit -m "feat(core): a conversion reads rad and sr as 1; sums and comparisons do not" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: PHS

**Files:**
- Create: `physure-script/tests/angle_mode.rs`
- Modify: `physure-script/tests/angle_mode_golden.rs`

- [ ] **Step 1: Make the golden test run in `Dimension` mode**

In `physure-script/tests/angle_mode_golden.rs` (the file as Task 1 left it), add this import on the line before `use physure_script::interpreter::eval_phs;`, so that the imports stay in alphabetical order:

```rust
use physure_core::units::angle_mode::{scoped, AngleMode};
```

and make `render` open with the guard that puts the thread in the strict mode, so that it reads:

```rust
fn render() -> String {
    let _strict = scoped(AngleMode::Dimension);
    CORPUS.iter().map(|source| format!("{source}\t{}\n", answer(source))).collect()
}
```

`the_answers_are_the_ones_recorded`, `answer`, `golden_path` and the `CORPUS` stay as they are: the guard lives in `render`, which is the only caller of `answer`, so every row is evaluated in `Dimension` mode.

Also change the sentence of the header comment "The file is written once, on a tree that has no angle mode yet." to "The file was written once, on a tree that had no angle mode yet." The rest of that paragraph (the refusal to overwrite and the Bash and PowerShell forms) stays.

- [ ] **Step 2: Run it**

Run: `cargo test -p physure-script --test angle_mode_golden`
Expected: pass. This is the claim "strict is today, byte for byte".

- [ ] **Step 3: Write the PHS tests**

Create `physure-script/tests/angle_mode.rs`:

```rust
//! The angle mode, seen from PHS: what a conversion may read as 1, and what nothing may.

use physure_core::units::angle_mode::{scoped, AngleMode};
use physure_script::interpreter::eval_phs;
use physure_script::value::PhsValue;

/// The magnitude, in the unit the program asked for, of the quantity its last statement produced.
fn magnitude(source: &str) -> f64 {
    match eval_phs(source).unwrap_or_else(|e| panic!("{source:?} failed: {e:?}")).into_iter().last() {
        Some(PhsValue::Quantity(q)) => q.value.mean(),
        other => panic!("{source:?} produced {other:?}, expected a quantity"),
    }
}

fn assert_magnitude(source: &str, expected: f64) {
    let actual = magnitude(source);
    assert!(
        (actual - expected).abs() <= expected.abs().max(1.0) * 1e-12,
        "{source:?}: expected {expected}, got {actual}",
    );
}

/// What a conversion may read as 1, with its answer in the default mode; the strict mode refuses all of them.
const CONVERSIONS: &[(&str, f64)] = &[
    ("(4 kg) * (0.5 m)^2 / 4 * (10 rad/s)^2 => J", 25.0),
    ("(10 rad/s) * (0.5 m) => m/s", 5.0),
    ("(2 m) * (0.5 rad) => m", 1.0),
    ("(10 N*m) * (3 rad/s) => W", 30.0),
    ("(377 rad/s) * (0.1 H) => ohm", 37.7),
    ("(2 rad) => unity", 2.0),
    ("(90 deg) => unity", std::f64::consts::FRAC_PI_2),
    ("(100 cd) / (2 m)^2 => lx", 25.0),
    ("(5 lm) => cd", 5.0),
    ("(30 W) / (4 sr) / (2 m)^2 => W/m^2", 1.875),
    ("(2 rad => unity) + 1", 3.0),
    ("(25 kg*m^2*rad^2/s^2 => J) - (25 J)", 0.0),
];

/// Accepted in both modes, as they were before the angle mode existed.
const UNCHANGED: &[(&str, f64)] = &[
    ("(10 rad/s) / (6.283185307179586 rad) => Hz", 1.5915494309189535),
    ("(6.283185307179586 rad) * (50 Hz) => rad/s", 314.1592653589793),
    ("(1 rad) + (90 deg)", 2.5707963267948966),
    ("(10 N*m/rad) * (2 rad) => J", 20.0),
    ("(1 m) + (100 cm)", 2.0),
    ("(20 dB) => percent", 10000.0),
];

/// Refused in both modes: a sum or a comparison across a marker, one marker against another, a level against an
/// angle, a rate of angle against a frequency, and a trigonometric function of something that is not an angle.
const REFUSED: &[&str] = &[
    "(2 rad) + 1",
    "1 + (2 rad)",
    "(5 deg) + (50 %)",
    "(1 rad) == 1",
    "(2 m*rad) + (1 m)",
    "(25 kg*m^2*rad^2/s^2) - (25 J)",
    "(1 rad)^2 + (1 rad)",
    "(1 deg) => sr",
    "(1 rad) + (1 sr)",
    "(1 deg) + (1 sr)",
    "(1 rad) => dB",
    "sin(9.81 m/s^2)",
    "(1 rad/s) => sr/s",
    "(1 sr) => rad",
    "(10 rad/s) => Hz",
    "(10 rad/s) => kHz",
    "(10 Hz) => rad/s",
];

#[test]
fn the_default_mode_converts_across_the_markers() {
    let _mode = scoped(AngleMode::Dimensionless);
    for (source, expected) in CONVERSIONS {
        assert_magnitude(source, *expected);
    }
}

#[test]
fn the_strict_mode_refuses_every_such_conversion() {
    let _mode = scoped(AngleMode::Dimension);
    for (source, _) in CONVERSIONS {
        assert!(eval_phs(source).is_err(), "{source:?} should be refused in the strict mode");
    }
}

#[test]
fn what_never_depended_on_the_mode_still_does_not() {
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        for (source, expected) in UNCHANGED {
            assert_magnitude(source, *expected);
        }
        for source in ["sin(90 deg)", "sin(2 rad)"] {
            assert!(eval_phs(source).is_ok(), "{source:?} in {mode:?}");
        }
        for source in REFUSED {
            assert!(eval_phs(source).is_err(), "{source:?} should be refused in {mode:?}");
        }
    }
}

#[test]
fn a_parallel_for_runs_in_the_mode_of_its_caller() {
    let program = "v = for i in 1..9 { (10 rad/s) * (0.5 m) => m/s }";
    let _every_loop_is_parallel = physure_core::settings::scoped(0);
    {
        let _strict = scoped(AngleMode::Dimension);
        assert!(eval_phs(program).is_err(), "the strict mode must reach the rayon workers");
    }
    let _default = scoped(AngleMode::Dimensionless);
    assert!(eval_phs(program).is_ok());
}

#[test]
fn parallel_map_runs_in_the_mode_of_its_caller() {
    let program = "fn speed(w) = w * (0.5 m) => m/s\nres = parallel_map(speed, vector(1 rad/s, 2 rad/s, 3 rad/s))";
    {
        let _strict = scoped(AngleMode::Dimension);
        assert!(eval_phs(program).is_err(), "the strict mode must reach the rayon workers");
    }
    let _default = scoped(AngleMode::Dimensionless);
    assert!(eval_phs(program).is_ok());
}
```

- [ ] **Step 4: Run them**

Run: `cargo test -p physure-script --test angle_mode`
Expected: `the_default_mode_converts_across_the_markers`, `the_strict_mode_refuses_every_such_conversion` and `what_never_depended_on_the_mode_still_does_not` pass. The two parallel tests FAIL on the assertion `the strict mode must reach the rayon workers`: workers run in the process mode, so the strict program succeeds. That is the red state for Task 9. If either parallel test fails for another reason (a parse error), print the error, fix the program text only, and re-run.

- [ ] **Step 5: Commit**

```bash
git add physure-script/tests/angle_mode.rs physure-script/tests/angle_mode_golden.rs
git commit -m "test(script): the angle mode through PHS, and strict equal to the recorded answers" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Parallel work runs in the caller's mode

**Files:**
- Modify: `physure-script/src/interpreter/expressions.rs` (`eval_for`), `physure-script/src/builtins/core.rs` (`parallel_map`)

- [ ] **Step 1: Capture the mode in `eval_for`**

In `physure-script/src/interpreter/expressions.rs`, in `eval_for`, replace

```rust
            use rayon::prelude::*;
            let results: Vec<PhsValue> = items
                .into_par_iter()
                .map(|item| {
                    let mut local_env = env.clone();
```

with

```rust
            use rayon::prelude::*;
            // A scoped angle mode is per thread and a rayon worker is another thread: read it here and open it again
            // inside each task, or the loop body would run in the process mode.
            let angle_mode = physure_core::units::angle_mode::angle_mode();
            let results: Vec<PhsValue> = items
                .into_par_iter()
                .map(|item| {
                    let _angle = physure_core::units::angle_mode::scoped(angle_mode);
                    let mut local_env = env.clone();
```

- [ ] **Step 2: Capture it in `parallel_map`**

In `physure-script/src/builtins/core.rs`, in `parallel_map`, replace

```rust
    let vec = v.clone();
    let call = |(i, item): (usize, PhsValue)| {
        interpreter
```

with

```rust
    let vec = v.clone();
    // See `eval_for`: the angle mode of the caller has to be opened again inside each task.
    let angle_mode = physure_core::units::angle_mode::angle_mode();
    let call = |(i, item): (usize, PhsValue)| {
        let _angle = physure_core::units::angle_mode::scoped(angle_mode);
        interpreter
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p physure-script --test angle_mode`
Expected: all 5 pass.

Run: `cargo test -p physure-script`
Expected: no new failure against the Task 0 baseline.

- [ ] **Step 4: Commit**

```bash
git add physure-script/src/interpreter/expressions.rs physure-script/src/builtins/core.rs
git commit -m "fix(script): parallel loops and parallel_map run in the angle mode of their caller" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 10: The static checker stays silent where the interpreter may accept

**Files:**
- Modify: `physure-script/src/check.rs`, `physure-script/tests/check.rs`

- [ ] **Step 1: Write the failing tests**

Append to `physure-script/tests/check.rs`:

```rust
// ---- the angle in a conversion

#[test]
fn a_conversion_that_differs_only_in_the_angle_is_left_to_the_interpreter() {
    // `facts` gives `rad`, `deg` and `sr` the same dimension symbol `A`, so the checker cannot tell whether a
    // conversion may read the angle as 1; it stays silent and the interpreter decides.
    assert_clean("(10 rad/s) * (0.5 m) => m/s");
    assert_clean("(100 cd) / (2 m)^2 => lx");
    assert_clean("(2 rad) => unity");
    assert!(found("(10 rad/s) => Hz").is_empty(), "silence is sound: the interpreter refuses it");
    assert!(eval_phs("(10 rad/s) => Hz").is_err());
}

#[test]
fn a_conversion_that_differs_in_more_than_the_angle_is_still_an_error() {
    assert_refused("(10 rad/s) * (0.5 m) => m", 1, "dimension-mismatch");
    assert_refused("(2 rad) => m", 1, "dimension-mismatch");
}

#[test]
fn a_sum_across_the_angle_is_still_reported() {
    assert_refused("(2 m*rad) + (1 m)", 1, "dimension-mismatch");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p physure-script --test check a_conversion_that_differs_only_in_the_angle`
Expected: FAIL: `false positive` for `(10 rad/s) * (0.5 m) => m/s` (the checker reports `dimension-mismatch` on a program the interpreter runs).

- [ ] **Step 3: Implement it**

In `physure-script/src/check.rs`, in `impl Dim`, after `same_dimension` add:

```rust
    /// The same dimension once the angle is set aside: what a conversion may read as 1 in `Dimensionless` mode.
    fn same_apart_from_angle(&self, other: &Dim) -> bool {
        let without_angle = |dim: &Dim| -> BTreeMap<String, Rational64> {
            dim.exps.iter().filter(|(symbol, _)| symbol.as_str() != "A").map(|(symbol, power)| (symbol.clone(), *power)).collect()
        };
        without_angle(self) == without_angle(other)
    }
```

and in `binary`, in the `BinaryOp::Convert` branch, replace

```rust
            if let Ty::Qty(dim) = &from {
                if !dim.same_dimension(&target) {
```

with

```rust
            if let Ty::Qty(dim) = &from {
                let angle_only = physure_core::units::angle_mode::angle_mode()
                    == physure_core::units::angle_mode::AngleMode::Dimensionless
                    && dim.same_apart_from_angle(&target);
                if !dim.same_dimension(&target) && !angle_only {
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p physure-script --test check`
Expected: all pass.

Run: `cargo test -p physure-script --test check_corpus`
Expected: pass: no false positive on any program the interpreter runs.

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/check.rs physure-script/tests/check.rs
git commit -m "fix(check): stay silent on a conversion that differs only in the angle" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Python

**Files:**
- Modify: `physure-python/src/lib.rs`, `physure-python/physure/_core.pyi`, `physure-python/physure/application/context.py`, `physure-python/physure/__init__.py`
- Create: `physure-python/tests/core/test_angle_mode.py`

- [ ] **Step 1: Prepare the build, carefully**

Run (from `D:\Projects\physure\.claude\worktrees\angle-mode\physure-python`): `echo $VIRTUAL_ENV`
Expected: empty, or a path inside this worktree. If it names another checkout's `.venv`, `maturin develop` would write the extension there: unset it (`deactivate`, or start a clean shell) before going on.

Run: `uv sync --all-extras --dev`
Expected: creates `physure-python/.venv` inside the worktree.

- [ ] **Step 2: Write the failing tests**

Create `physure-python/tests/core/test_angle_mode.py`:

```python
"""A conversion reads the radian and the steradian as 1; a sum or a comparison never does."""

import pytest

import physure
from physure import Q_


def _speed():
    return Q_(10, "rad/s") * Q_(0.5, "m")


def test_the_default_mode_is_the_si_one():
    assert physure.get_angle_mode() == "dimensionless"


def test_a_conversion_reads_the_radian_as_one():
    assert _speed().to("m/s").magnitude == pytest.approx(5.0)


def test_the_energy_of_a_rotating_disk_converts_to_joules():
    mass, radius, omega = Q_(4, "kg"), Q_(0.5, "m"), Q_(10, "rad/s")
    inertia = 0.5 * mass * radius**2
    energy = 0.5 * inertia * omega**2
    assert energy.to("J").magnitude == pytest.approx(25.0)


def test_a_rate_of_angle_converts_to_per_second_but_not_to_hertz():
    assert Q_(10, "rad/s").to("s^-1").magnitude == pytest.approx(10.0)
    with pytest.raises(physure.PhysureError):
        Q_(10, "rad/s").to("Hz")


def test_the_strict_mode_is_the_old_behaviour():
    with physure.angle_mode("dimension"):
        assert physure.get_angle_mode() == "dimension"
        with pytest.raises(physure.PhysureError):
            _speed().to("m/s")
    assert physure.get_angle_mode() == "dimensionless"


def test_scopes_nest_and_restore_after_an_exception():
    with physure.angle_mode("dimension"):
        with physure.angle_mode("dimensionless"):
            assert physure.get_angle_mode() == "dimensionless"
        assert physure.get_angle_mode() == "dimension"
    with pytest.raises(RuntimeError), physure.angle_mode("dimension"):
        raise RuntimeError
    assert physure.get_angle_mode() == "dimensionless"


def test_an_unknown_mode_is_refused():
    with pytest.raises(ValueError, match="dimensionless, dimension"):
        physure.angle_mode("degrees")


@pytest.mark.parametrize(
    ("left", "right"),
    [
        ("rad", "unity"),
        ("deg", "percent"),
        ("m*rad", "m"),
        ("kg*m^2*rad^2/s^2", "J"),
        ("rad", "sr"),
    ],
)
def test_a_sum_never_reads_a_marker_as_one(left, right):
    for mode in ("dimensionless", "dimension"):
        with physure.angle_mode(mode), pytest.raises(physure.PhysureError):
            Q_(2, left) + Q_(1, right)


def test_ordering_against_a_bare_number_is_not_defined_for_an_angle():
    with pytest.raises(TypeError):
        Q_(1, "rad") < 1  # noqa: B015
```

- [ ] **Step 3: Add the PyO3 scope and getter**

In `physure-python/src/lib.rs`, add after the `use ::physure_core::{ ... };` block at the top:

```rust
use ::physure_core::units::angle_mode::{self as angle, AngleMode, AngleModeGuard};
```

Before the `// ── Module Registration` comment add:

```rust
/// The angle mode in force on this thread: `"dimensionless"` or `"dimension"`.
#[pyfunction]
fn angle_mode() -> &'static str {
    angle::angle_mode().name()
}

/// `with AngleModeScope("dimension"): ...` reads angles by the given mode until the block ends; scopes nest. The
/// mode is that of the thread that made the scope: a guard restores a thread-local, so the object is not `Send`.
#[pyclass(name = "AngleModeScope", module = "physure._core", unsendable)]
struct PyAngleModeScope {
    mode: AngleMode,
    guards: Vec<AngleModeGuard>,
}

#[pymethods]
impl PyAngleModeScope {
    #[new]
    fn new(mode: &str) -> PyResult<Self> {
        let mode = mode.parse::<AngleMode>().map_err(pyo3::exceptions::PyValueError::new_err)?;
        Ok(Self { mode, guards: Vec::new() })
    }

    fn __enter__(mut slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        let guard = angle::scoped(slf.mode);
        slf.guards.push(guard);
        slf
    }

    #[pyo3(signature = (_exc_type=None, _exc_value=None, _traceback=None))]
    fn __exit__(
        &mut self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_value: Option<&Bound<'_, PyAny>>,
        _traceback: Option<&Bound<'_, PyAny>>,
    ) -> bool {
        // dropping the guard restores the mode that was in force before the matching `__enter__`
        self.guards.pop();
        false
    }
}
```

In `_core`, after `m.add_function(wrap_pyfunction!(default_conf, m)?)?;` add:

```rust
    m.add_class::<PyAngleModeScope>()?;
    m.add_function(wrap_pyfunction!(angle_mode, m)?)?;
```

In `physure-python/physure/_core.pyi`, after `def default_conf() -> str: ...` add:

```python
def angle_mode() -> str: ...

class AngleModeScope:
    def __init__(self, mode: str) -> None: ...
    def __enter__(self) -> AngleModeScope: ...
    def __exit__(
        self,
        exc_type: Any = None,
        exc_value: Any = None,
        traceback: Any = None,
    ) -> bool: ...
```

- [ ] **Step 4: The Python names**

In `physure-python/physure/application/context.py`, after the `get_propagation_mode` function, add:

```python
def angle_mode(mode: str) -> Any:
    """Context manager that sets how a conversion reads the radian and the steradian.

    ``"dimensionless"`` (the default, and what the SI does: both are 1 in an
    expression) lets a conversion such as ``(w * r).to("m/s")`` or
    ``energy.to("J")`` identify them with 1 where the compatibility rows of
    ``physure.conf`` allow it. ``"dimension"`` makes the angle a dimension like
    any other. A sum or a comparison never reads them as 1, in either mode. The
    scope belongs to the thread that opens it.

    Args:
        mode: ``"dimensionless"`` or ``"dimension"``.

    Raises:
        ValueError: If the mode is neither.

    Examples:
        >>> import physure
        >>> with physure.angle_mode("dimension"):
        ...     physure.get_angle_mode()
        'dimension'
        >>> physure.get_angle_mode()
        'dimensionless'
    """
    from physure._core import AngleModeScope

    return AngleModeScope(mode)


def get_angle_mode() -> str:
    """Returns the mode a conversion reads angles by on this thread."""
    from physure._core import angle_mode as core_angle_mode

    return core_angle_mode()
```

In `physure-python/physure/__init__.py`:

1. In the `TYPE_CHECKING` block, replace
```python
        uncertainty_model,
    )
    from physure.application.context import (
        get_current_system as get_active_system,
    )
```
with
```python
        uncertainty_model,
    )
    from physure.application.context import (
        angle_mode,
        get_angle_mode,
    )
    from physure.application.context import (
        get_current_system as get_active_system,
    )
```
2. Replace `_CONTEXT_ATTRS = {\n    "get_current_system",` with
```python
_CONTEXT_ATTRS = {
    "angle_mode",
    "get_angle_mode",
    "get_current_system",
```
3. In `_load_context`, replace
```python
        uncertainty_model,
        use_system,
    )

    _map = {
```
with
```python
        uncertainty_model,
        use_system,
    )
    from physure.application.context import angle_mode, get_angle_mode

    _map = {
        "angle_mode": angle_mode,
        "get_angle_mode": get_angle_mode,
```

- [ ] **Step 5: Build the extension and verify which one was built**

Run (from `physure-python`): `uv run maturin develop --release`
Expected: ends with `Installed physure-...`. First build takes minutes: run it in the background.

Run: `uv run python -c "import physure._core as c; print(c.__file__)"`
Expected: a path inside `D:\Projects\physure\.claude\worktrees\angle-mode\physure-python\`. If it is under another checkout, stop and report it: the wrong venv was written.

- [ ] **Step 6: Run the tests**

Run: `uv run pytest tests/core/test_angle_mode.py -v`
Expected: all pass.

Run: `uv run pytest tests/core tests/measurement_tests -q`
Expected: no new failure. A test that pins the old refusal of `rad` in a conversion is a test of the old default: move it under `with physure.angle_mode("dimension"):` and say so in the commit message. Do not edit a test that pins a sum or a comparison; if one fails, the implementation is wrong.

Run: `uv run pytest --doctest-modules physure/application/context.py -q`
Expected: pass.

- [ ] **Step 7: Lint**

Run: `uv run ruff check .`
Expected: `All checks passed!`

Run: `uv run ruff format --check .`
Expected: no file would be reformatted. If one would, run `uv run ruff format physure/application/context.py tests/core/test_angle_mode.py`.

- [ ] **Step 8: Commit**

```bash
git add physure-python/src/lib.rs physure-python/physure/_core.pyi physure-python/physure/application/context.py physure-python/physure/__init__.py physure-python/tests/core/test_angle_mode.py
git commit -m "feat(python): physure.angle_mode and get_angle_mode" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Documents

**Files:**
- Modify: `CHANGELOG.md`, `physure-cli/src/main.rs` (the guide's generator)
- Regenerate: `PHYSURE_LANGUAGE_GUIDE.md`, `docs/UNITS.md`

- [ ] **Step 1: The changelog**

In `CHANGELOG.md`, directly under the first `### Changed` heading of `## [Unreleased]`, add as the first bullet:

```markdown
- **A conversion reads the radian and the steradian as 1, as the SI does.** `½·I·ω² => J` (finding `F-0001` of `physure-crucible`), `(10 rad/s) * (0.5 m) => m/s`, `(377 rad/s) * (0.1 H) => ohm` and `(100 cd) / (2 m)^2 => lx` were unit mismatches, because the catalog made the angle a base dimension. SI Brochure Table 4 gives `rad` and `sr` the expression 1, so `=>`, `to`, the binding of an annotated parameter and the registry's conversion plan now identify them with 1 when the rest of the unit agrees. Nothing else does: a sum or a comparison still refuses an angle against a plain number (`2 rad + 1`, `5 deg + 50 %`, `2 m*rad + 1 m`), `rad` still does not convert to `sr` or to a level, and `10 rad/s => Hz` is still refused, because the SI reserves the hertz for periodic phenomena and a frequency and an angular frequency differ by 2π (`(10 rad/s) / (2π rad) => Hz` is the way to write it). `[Settings] angle_mode = dimension` in `physure.conf`, `with physure.angle_mode("dimension"):` in Python, or `physure_core::units::angle_mode::scoped(AngleMode::Dimension)` in Rust brings back the old behaviour exactly, and `[Compatibility]` in `physure.conf` lists the markers a conversion may read as 1. Design: `docs/superpowers/specs/2026-11-05-angle-mode-design.md`.
- **Parallel loops and `parallel_map` run in the angle mode of the code that started them.** A scoped mode is per thread and a rayon worker is another thread, so the loop body used to run in the process mode.
```

If the file has no `### Changed` heading under `[Unreleased]`, add one after `### Added` section's last bullet.

- [ ] **Step 2: The language guide (it is generated)**

`PHYSURE_LANGUAGE_GUIDE.md` is written by `phs doc --save` from `generate_language_docs_md()` in `physure-cli/src/main.rs`, and a hand edit of the Markdown would be lost the next time it is generated. Edit the generator.

In `physure-cli/src/main.rs`, directly after the two lines

```rust
    md.push_str("v_kmh = 25.0 m / s => km / h     # Converts 25 m/s to km/h\n");
    md.push_str("```\n\n");
```

add:

```rust
    md.push_str("#### Angles in a conversion\n\n");
    md.push_str("The SI gives the radian and the steradian the expression 1, and `=>` follows it: an angle is read as 1 when a conversion needs it, so the formulas of mechanics, circuits and photometry convert without ceremony.\n\n");
    md.push_str("```phs\n");
    md.push_str("energy = 0.5 * (0.5 kg * m^2) * (10 rad/s)^2   # kg * m^2 * rad^2 * s^-2\n");
    md.push_str("e_j = energy => J                              # 25 J\n");
    md.push_str("v = (10 rad/s) * (0.5 m) => m/s                # 5 m/s\n");
    md.push_str("lux = (100 cd) / (2 m)^2 => lx                 # 25 lx\n");
    md.push_str("```\n\n");
    md.push_str("A sum or a comparison never reads an angle as 1: `2 rad + 1` and `2 m*rad + 1 m` are errors, and the way is to convert first, `(2 rad => unity) + 1`. A rate of angle is not a frequency (they differ by 2π), so `10 rad/s => Hz` is an error; divide by the angle of a cycle instead, `(10 rad/s) / (6.283185307179586 rad) => Hz`. `angle_mode = dimension` in `physure.conf` makes the angle a dimension like any other.\n\n");
```

Then regenerate the file from the worktree root:

Run: `cargo run --release -p physure-cli --bin phs -- doc --save`
Expected: `📄 Language reference specification saved to: PHYSURE_LANGUAGE_GUIDE.md`

Run: `git diff --stat PHYSURE_LANGUAGE_GUIDE.md`
Expected: only the section above added. If other lines changed, the committed file had drifted from its generator: keep them (regenerating is the repository's way) and say so in the commit message.

Run: `cargo test -p physure-cli --test cli_commands the_language_guide_is_printed_or_saved`
Expected: pass.

Check the example by running it (the build of Task 13, Step 5 has the new `phs`): `target/release/phs.exe` on the four lines must print `25.0 J`, `5.0 m/s` and `25.0 lx` and no error.

- [ ] **Step 3: Regenerate the units reference**

Run (from `physure-python`): `uv run python scripts/generate_units_readme.py`
Expected: `Successfully generated ...UNITS.md`.

Run: `git diff --stat docs/UNITS.md`
Expected: either no change, or a small change that only reflects the comment and section added to `physure.conf`. If the generator now lists `[Compatibility]` rows as units or fails, stop and read `scripts/generate_units_readme.py` before editing it.

- [ ] **Step 4: Build the docs the way CI does**

Run (from `physure-python`): `uv run mkdocs build --strict`
Expected: ends without a warning. This plan adds no link to `docs/`.

- [ ] **Step 5: Commit**

```bash
git add CHANGELOG.md physure-cli/src/main.rs PHYSURE_LANGUAGE_GUIDE.md docs/UNITS.md
git commit -m "docs: a conversion reads rad and sr as 1" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 13: Verify the whole branch

**Files:** none.

- [ ] **Step 1: All Rust tests**

Run (background, `timeout: 600000`): `cargo test --workspace --exclude physure-python`
Expected: the Task 0 baseline and nothing more. List every failure that is not in the baseline; there should be none.

- [ ] **Step 2: Clippy on what changed**

Run: `cargo clippy -p physure -p physure-script --all-targets`
Expected: no new warning in the files this plan touched.

- [ ] **Step 3: All Python tests, with the coverage bar**

Run (from `physure-python`): `uv run pytest --cov -q`
Expected: pass, coverage at or above 80%.

- [ ] **Step 4: Startup cost**

Run (from `physure-python`): `uv run python -c "import time; t=time.time(); import physure; physure.Q_(500, 'N'); print(round(time.time()-t, 2))"`
Expected: not above the figure on `main` for the same command (the budget is about 0.5 s). The new code adds a conf section and a field, no import.

- [ ] **Step 5: The hot path**

Run: `cargo bench -p physure --bench core_benchmarks -- quantity_convert_prefixed quantity_add_scalar`
Expected: no change above noise against the same command on `main` (run it in the main checkout's build, or on the commit before Task 4). `convert_to` runs the strict comparison first, and `add` does not change.

- [ ] **Step 6: Show the diff to the maintainer**

Run: `git log --oneline b27d0f8..HEAD`
Run: `git diff --stat b27d0f8..HEAD`
Expected: the commits of Tasks 1–12 plus the spec and plan commits. Do not push. Report the two figures of Steps 4 and 5.

---

### Task 14: The crucible (separate repository `D:\Projects\physure-crucible`)

This task changes the crucible's wrapper so that `F-0001` passes on the PHS surface, and resolves the finding. It needs the maintainer's approval of the wrapper change, given on 2026-11-05. It runs after Tasks 1–13, because it needs a candidate built from this branch.

**Files (in `physure-crucible`):**
- Modify: `validator/case.py`, `tests/test_case.py`
- Modify: `findings/F-0001-radians-remain-dimensionful/finding.toml`, `finding.md`
- Create: `reports/history/R-<hash>/` (written by the archive command)

- [ ] **Step 1: Branch in the crucible**

Run (from `D:\Projects\physure-crucible`): `git status --short`
Expected: no output. If there is output, stop: it is not this task's.

Run: `git switch -c fix/f0001-convert-before-compare`

- [ ] **Step 2: Update the test first**

In `tests/test_case.py`, in `test_render_checked_phs_appends_case_id_and_case_tolerance`, replace

```python
    assert "abs(result - _validator_reference)" in rendered
```

with

```python
    assert "abs((result => m) - _validator_reference)" in rendered
```

Run: `.venv/Scripts/python.exe -m pytest tests/test_case.py -q`
Expected: that test FAILS (the wrapper still subtracts without converting); the others pass.

- [ ] **Step 3: Convert the result to the unit the case expects**

In `validator/case.py`, in `render_checked_phs`, replace

```python
    abs_tol = _phs_quantity(expected.abs_tol, expected.unit)
    return (
```

with

```python
    abs_tol = _phs_quantity(expected.abs_tol, expected.unit)
    # The case states the unit it expects, so the result is converted to it, as the Rust consumers do with
    # `to(unit)`: a sum would refuse `kg*m^2*rad^2*s^-2` against `J`, and the question of the case is whether the
    # energy converts to joules.
    result = f"({case.phs_result} => {expected.unit})" if expected.unit else case.phs_result
    return (
```

and replace

```python
        + f"_validator_error = abs({case.phs_result} - _validator_reference)\n"
```

with

```python
        + f"_validator_error = abs({result} - _validator_reference)\n"
```

- [ ] **Step 4: Run the crucible's own tests**

Run: `.venv/Scripts/python.exe -m pytest -q`
Expected: 377 passed.

- [ ] **Step 5: Build a candidate from the branch**

Run (from the physure worktree, background, `timeout: 600000`): `cargo build --release -p physure-cli`
Expected: `target/release/phs.exe` exists in the worktree.

Run (from `D:\Projects\physure-crucible`):
`.venv/Scripts/python.exe scripts/prepare_candidate.py --physure-root D:/Projects/physure/.claude/worktrees/angle-mode --phs D:/Projects/physure/.claude/worktrees/angle-mode/target/release/phs.exe --platform windows-x86_64 --output .artifacts/candidate-angle.toml`
Expected: writes the manifest (the script runs `cargo package -p physure --allow-dirty` with a 120 s limit; if it times out on the first build, run `cargo package -p physure --allow-dirty` once by hand in the worktree and repeat).

- [ ] **Step 6: Run the gate**

Run: `.venv/Scripts/crucible.exe --candidate .artifacts/candidate-angle.toml --cases cases --report .artifacts/angle-report.json`
Expected: `mechanics.rotating-disk-energy` passes on both surfaces; every other conformance case that passed before still passes; the experimental `physics.phs-structured-uncertainty` keeps its documented limitation. If another case fails, read its `stderr` in the report: a unit it expects may not parse after `=>` (a unit starting with a digit); handle it in `render_checked_phs` and add a test, do not weaken the case.

- [ ] **Step 7: Archive the run and resolve the finding**

Run: `.venv/Scripts/crucible.exe findings archive --report .artifacts/angle-report.json --history reports/history`
Expected: prints the run id (`R-...`) and its hash. Write the id down.

In `findings/F-0001-radians-remain-dimensionful/finding.toml` set `status = "resolved"` and `updated_at` to today's date.

In `finding.md`, replace the `## Actual behavior` section's first sentence with "Before the fix, both surfaces retained the dimensionful `rad²` factor and failed the public conversion to `J`." and append:

```markdown
## Resolution
Physure reads the radian and the steradian as 1 in a conversion, as SI Table 4 does (`docs/superpowers/specs/2026-11-05-angle-mode-design.md`, branch `feat/angle-mode`), and the wrapper of the PHS surface converts the result to the expected unit before it subtracts, as the Rust consumers already did. Both surfaces report 25 J. Evidence: run `R-<id from Step 7>`.
```

Run: `.venv/Scripts/crucible.exe findings build --root . --findings findings --history reports/history --cases cases --database .artifacts/findings.sqlite`
Expected: completes; `crucible findings list --database .artifacts/findings.sqlite --status open --format json` no longer lists `F-0001`.

- [ ] **Step 8: Commit in the crucible**

```bash
git add validator/case.py tests/test_case.py findings reports/history
git commit -m "fix: convert the PHS result to the expected unit before comparing, and resolve F-0001" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Review every report for secrets before archiving it, as the crucible's README requires: the archived report keeps raw commands and candidate-local paths. Do not push.

---

## Self-review

**Spec coverage**

| Spec section | Task |
|---|---|
| The mode (`AngleMode`, `scoped`, `[Settings]`) | 2, 3 |
| The rule: only a conversion reads a marker as 1; strict compatibility first | 6, 7 |
| The list: rows, `plain`, markers, `periodic`, conversion-only, rows replace | 4, 5 |
| The hertz refusal and the flag | 4, 6, 7 |
| `dimension_of` and Python's `Dimension` unchanged | no task changes them; pinned by Task 7's mode-independence test and Task 11's sum tests |
| Three call sites | 7 |
| Threads: capture at two rayon sites | 8 (tests), 9 |
| Static checker silence on `=>` | 10 |
| Python scope and getter | 11 |
| Strict is today, byte for byte | 1, 8 |
| Prevented operations pinned, in both modes; differential test over unit pairs | 7, 8 |
| `CHANGELOG`, guide, `UNITS.md`, comment above `radian` | 5 (comment), 12 |
| `F-0001` and the crucible wrapper | 14 |
| Cost / startup | 13 |
| Recording the mode in the crucible manifest | deliberately not in this plan (spec, Surfaces) |

**Placeholders:** none; every code step has its code and every command its expected result.

**Names:** `AngleMode`, `angle_mode()`, `set_angle_mode()`, `scoped()`, `AngleModeGuard`, `marker_converts_to_plain()`, `set_marker_grants()` (Task 2) are the names used in Tasks 3–11; `RationalUnit::periodic` and `convertible_to` (Tasks 4, 6) are the names used in Tasks 7–10; the Python names `AngleModeScope`, `angle_mode`, `get_angle_mode` agree across `lib.rs`, `_core.pyi`, `context.py`, `__init__.py` and the tests.

**Known risks the executor must watch:** the `periodic` flag surviving `Parser::parse_expression` (Task 4, Step 6 says what to read if a spelling fails); the PHS text of the two parallel tests (Task 8, Step 4 says to fix the program, not the assertion); tests that pin the old default (Tasks 7 and 11 say to move them under `Dimension` mode on purpose, never to edit one that pins a sum or a comparison).
