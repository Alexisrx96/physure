# A1 stage 1a: exact rational scales in the core — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `physure-core` convert between units with exact rational scales and a single rounding, so that it gives the same `f64`, bit for bit, as Python does over the whole catalog. This closes the `last-bit-difference` class of the stage 0 divergence report (47 symbols and one compound case) and is the first of four stage-1 pull requests (1a exact scales, 1b the three missing capabilities, 1c the compound-unit API, 1d the `5 * m` spike). Design: `docs/superpowers/specs/2026-10-27-a1-rust-owned-unit-algebra-design.md`, "Requirement: numerical exactness".

**Architecture:** Four commits. (1) The core's benchmark file does not compile (it predates the crate rename and the move of the symbolic engine); it is repaired, benchmarks for prefixed units are added, and a baseline is taken on the unchanged code. (2) A new type `ExactScale` (`physure-core/src/units/exact.rs`): a positive rational held as scale 1, a reduced `u128` fraction, or a `BigRational`, always in the smallest that fits. (3) `RationalUnit` carries `exact: Option<ExactScale>` next to its `scale: f64`; the product, quotient and integer power of units, prefixes, and the catalog parser all keep it, and a conversion is `source.exact / target.exact` rounded once (`RationalUnit::ratio_to`). Converting out of an affine scale divides by its scale, as Python's converter does. The stage 0 parity test then asserts that Python and Rust agree exactly. (4) Documentation and the measured cost.

**Tech Stack:** Rust (`physure-core`, tests in `physure-script`), `num-bigint` / `num-rational` (already compiled: `num-bigint` was a dependency of `num-rational`), criterion, Python 3.11+ and pytest for the parity test. The extension must be rebuilt after each Rust change.

## What was measured while planning (use it to check your results)

The change was implemented and verified on a prototype branch before this plan was written.

| Measure | Before | After |
|---|---|---|
| Symbols for which Python and Rust differ in the last bit | 47 (`KNOWN_LAST_BIT`) | 0 |
| Compound cases that differ | 1 (`g/cm^3` to `kg/m^3`: 999.9999999999999) | 0 |
| `1 ppm => percent`, `1 ft => inch`, `1 g/cm^3 => kg/m^3` in PHS | 9.999999999999999e-05, 12.000000000000002, 999.9999999999999 | 1e-4, 12.0, 1000.0 |
| Rust core tests (`cargo test -p physure`, the lib) | 184 passed | 197 passed (184, 10 for `ExactScale`, 3 new) |
| Python suite | 1021 passed, 6 xfailed | 1021 passed, 6 xfailed (the same tests; two of them stricter) |
| Cost of `quantity_convert_prefixed` (km to mile) | about 156 ns | about 233 ns, +31 % (about 770 ns with `BigRational` alone) |
| Cost of `unit_mul_prefixed` / `unit_div_prefixed` | about 117 ns / 118 ns | about 147 ns / 218 ns |
| Cost of unit parsing (`parse_compound_unit`) | about 0.53 ms | about 0.67 ms, +28 % (each call rebuilds the registry from the catalog, now with an exact scale per unit) |
| Cost for units of scale 1 (`unit_mul`, `quantity_add_scalar`, ...) | 120 to 150 ns | +5 % to +15 % |

The benchmarks are noisy on a loaded machine: in earlier runs those the change cannot touch moved by up to 60 %. The numbers above are from a quiet run; read them as orders of magnitude.

## Decisions taken while planning

- **Representation: three variants** (spec open item "width of the exact rational type"). `i128` alone cannot hold quecto (10^-30) times a scale written with 15 digits, and the catalog contains such units, so a fallback to `f64` would break "bit for bit"; `BigRational` alone made a prefixed conversion four times slower. So: `One` (every coherent SI unit, no data), `Small(u128, u128)` (a reduced fraction, plain integer arithmetic with overflow checks) and `Big(Arc<BigRational>)`. A value is always in the smallest representation that fits, so equal values compare equal.
- **`scale: f64` stays**, as the single rounding of the exact scale; every binding keeps working unchanged. `exact` is `None` only for a scale that arrived as a bare `f64` (`with_scale`) or from a fractional power (where Python also falls back to a float), and then conversions divide the doubles as before. The invariant, "if `exact` is `Some(e)` then `scale` is `e.to_f64()`", is tested for every unit of the catalog.
- **Where the exactness is used:** `RationalUnit::ratio_to` (a conversion, and the rescaling of the right operand of `+` and `-`), and the product, quotient and integer power of units. Nothing else in the core reads the exact scale.
- **The affine formula follows Python.** Python converts out of an affine scale with `(value - offset) / scale`; the core multiplied by `1 / scale`, which differs in the last bit for Fahrenheit. It now divides. The parity test found this after the exact scales were in.
- **Out of scope, found on the way** (recorded in the design document by Task 5): converting between affine scales is not exact either (`100 degC => degF` is `211.99999999999997` in Python and in Rust, because the Fahrenheit zero point is a double); `UnitDefinition` and `UnitConverter`, the second unit model that the Python binding exposes, still hold `f64` scales; the other four classes of the divergence report belong to stages 1b and 2.

## Conventions (apply to every task)

- Work in the existing worktree `D:\Projects\physure\.claude\worktrees\single-conf`, which has a built environment, on a new branch `feat/a1-exact-scales` created from local `main` (Task 1). Never commit on `main`.
- **VENV SAFETY (critical):** the maintainer's shell exports `VIRTUAL_ENV` pointing at the main checkout's venv. Run `unset VIRTUAL_ENV` before every `uv` command, run uv from this worktree's `physure-python/` (or with `--project physure-python`), and use `uv run --no-sync ...`. Never run `uv sync`. The one command that builds the extension is `uv run --no-sync maturin develop --release` from `physure-python/`; after it, `uv run --no-sync python -X utf8 -c "import physure._core as c; print(c.__file__)"` must print a path inside `D:\Projects\physure\.claude\worktrees\single-conf\physure-python\physure\`. If it prints any other path, stop and report.
- `git add <explicit paths>`; never `-A` or `-a`; check `git diff --cached --name-status` before each commit. `Cargo.lock` is staged only in the commit of Task 3, and only after `git diff Cargo.lock` shows nothing but the new `num-bigint` entry in the dependencies of the `physure` package.
- Every commit ends with the trailer: `-m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"` as a second `-m`.
- Python line length is 79 (ruff). Use `python -X utf8` for scripts that read or write files. Rust is formatted as the surrounding code is (the crates are not rustfmt-clean; do not reformat existing code).
- The core edits are made by one script, `apply_exact.py`, in parts; `exact.rs` is extracted from this plan by `plan_block.py` (Task 1). Do not edit those files by hand and do not retype the code.
- Cargo commands run from the worktree root. The core crate's package name is `physure` (`cargo test -p physure`); the language crate's is `physure-script`. Several commands are slow (a first `cargo bench` compile can take up to 10 minutes, the extension rebuild 1 to 2, the Python suite 2 to 4): run each in the foreground with the longest timeout the tool allows, one at a time. If a command is killed by the timeout, run it again: cargo resumes where it stopped.
- **The Python suite rewrites three generated files** (it adds `mock_unit` back to `_generated_types.py`, `registry.pyi` and `units/__init__.pyi`; the leak is already in `main`). After every run of the full suite, restore them: `git checkout -- physure-python/physure/_generated_types.py physure-python/physure/core/registry.pyi physure-python/physure/units/__init__.pyi`.
- **Rebuild** (after every Rust change that Python tests must see; the Python side reads the compiled extension):

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync maturin develop --release 2>&1 | tail -2
uv run --no-sync python -X utf8 -c "import physure._core as c; print(c.__file__)"
```

---

### Task 1: Create the branch and the helper scripts

**Files:** none tracked

- [ ] **Step 1: Start from a clean tree at the tip of `main`**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
git status -sb | head -3
git log --oneline -1
git log --oneline -1 main
```

Expected: no changed files (a modified `Cargo.lock` alone is fine); the two `log` lines show the same commit. If the worktree is on another branch with no uncommitted changes, `git switch --detach main` first. Any other uncommitted change: stop and report.

- [ ] **Step 2: Create the branch**

```bash
git switch -c feat/a1-exact-scales main
git status -sb | head -1
```

Expected: `## feat/a1-exact-scales`.

- [ ] **Step 3: Rebuild the extension at the state of `main`**

Run the **Rebuild** block of the Conventions. The parity test of Task 4 must fail for the right reason, which needs an extension built from the unchanged core.

Expected: `Installed physure-0.2.4` and a `_core.pyd` path inside the worktree.

- [ ] **Step 4: Write `plan_block.py`, which extracts a fenced block from this plan**

```bash
cat > /c/Users/Irvin/AppData/Local/Temp/plan_block.py <<'EOF'
"""Writes the fenced block that follows an anchor line of the plan to a file.

Usage: python -X utf8 plan_block.py "<anchor line>" <language> <destination>
"""
import pathlib
import re
import sys

PLAN = pathlib.Path(
    r"D:\Projects\physure\.claude\worktrees\a2-spec\docs\superpowers\plans\2026-10-27-a1-stage1a-exact-scales.md"
)
FENCE = chr(96) * 3
anchor, lang, dest = sys.argv[1:4]
text = PLAN.read_text(encoding="utf-8").replace("\r\n", "\n")
m = re.search(
    re.escape(anchor) + r"\n\n" + FENCE + lang + r"\n(.*?)\n" + FENCE + r"\n",
    text,
    re.S,
)
assert m, f"block not found after: {anchor}"
out = pathlib.Path(dest)
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(m.group(1) + "\n", encoding="utf-8", newline="\n")
print("written", dest, "|", m.group(1).count("\n") + 1, "lines")
EOF
```

Expected: no output.

- [ ] **Step 5: Extract `apply_exact.py` (defined at the end of this plan)**

```bash
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `apply_exact.py` with exactly this content:' python /c/Users/Irvin/AppData/Local/Temp/apply_exact.py
```

Expected: `written ... | 378 lines`. An `AssertionError: block not found` means the plan file is not at the path inside `plan_block.py`: stop and report.

---

### Task 2: Repair the benchmarks and take the baseline

**Files:**
- Modify: `physure-core/benches/core_benchmarks.rs`

- [ ] **Step 1: Apply the `bench` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_exact.py bench
cargo bench -p physure --bench core_benchmarks --no-run 2>&1 | tail -3
```

Expected: `edited physure-core\benches\core_benchmarks.rs`, then a `Finished` line and the path of the benchmark executable. The first compile of the dependencies in the bench profile is slow; if the tool kills it, run the same command again.

- [ ] **Step 2: Take the baseline on the unchanged code**

```bash
cargo bench -p physure --bench core_benchmarks -- --save-baseline before "unit|quantity|parse" 2>&1 | grep -E "time:" -B1 | grep -v "^--"
```

Expected: one `time:` line for each of `unit_mul`, `unit_div`, `unit_mul_prefixed`, `unit_div_prefixed`, `quantity_convert_prefixed`, `parse_compound_unit`, `parse_prefixed_compound_unit`, `quantity_add_scalar` and `quantity_mul_scalar`, in the neighbourhood of: 115 to 400 ns for the unit products and quotients and for the conversion, 0.5 to 1.3 ms for the two parses, 140 to 170 ns for the quantity operations (a loaded machine gives the larger values). Write the nine lines down for the report; they are the baseline that Task 5 compares with.

- [ ] **Step 3: Commit**

```bash
git add physure-core/benches/core_benchmarks.rs
git diff --cached --name-status
git commit -m "fix(core): the benchmarks compile again, and cover prefixed units" -m "core_benchmarks.rs imported the crate by its old name and a symbolic engine that moved to physure-script, so it had not compiled for a long time. The symbolic benchmark is gone (the engine is not in this crate) and benchmarks for units whose scale is not 1, a prefixed conversion and the parsing of compound units are added: the existing ones only used scale-1 units, which no change to the representation of scales can slow down." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: one `M`.

---

### Task 3: The exact scale type

**Files:**
- Create: `physure-core/src/units/exact.rs`
- Modify: `physure-core/src/units/mod.rs`
- Modify: `physure-core/Cargo.toml`, `Cargo.toml`, `Cargo.lock`

- [ ] **Step 1: Write the type and its tests**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/plan_block.py 'Create `physure-core/src/units/exact.rs` with exactly this content:' rust physure-core/src/units/exact.rs
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_exact.py type
cargo test -p physure --lib units::exact 2>&1 | grep -E "^test |^test result|^error|panicked"
```

The block written by the first command is:

Create `physure-core/src/units/exact.rs` with exactly this content:

```rust
//! Exact rational scale factors for units.
//!
//! A unit's `scale` is an `f64`: the nearest double to the decimal that `physure.conf`
//! writes. A conversion divides two such scales, and every rounding on the way shows in the
//! last digit (`1 ppm => percent` used to be `9.999999999999999e-05`). `ExactScale` keeps the
//! value as the rational the catalog states, so a ratio is computed exactly and rounded once,
//! which is what Python's `Fraction` arithmetic does.
//!
//! Three representations, so that the common cases cost almost nothing:
//!
//! * `One`, every coherent SI unit: no allocation, no arithmetic.
//! * `Small`, a reduced fraction of two `u128`: `km`, `mile`, `lb`, `g/cm^3`, nearly every unit
//!   a person writes. Plain integer arithmetic with overflow checks.
//! * `Big`, an unbounded `BigRational`: the rest. A prefix such as quecto (10^-30) times a scale
//!   written with 15 digits already needs a denominator beyond 128 bits, and the catalog
//!   contains such units, so the fallback is exact too instead of dropping to `f64`.
//!
//! A value is always held in the smallest representation that fits, so equal values compare
//! equal.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::sync::Arc;

/// Largest power of ten, and largest integer exponent, an exact scale will build. Beyond this
/// the result is "not exact" (`None`) and callers fall back to `f64`, rather than allocate
/// integers of unbounded size on behalf of a hostile `1e999999999`.
const MAX_EXPONENT: i64 = 400;

/// Integers up to this size convert to `f64` without rounding, so dividing two of them with the
/// FPU is already the correctly rounded quotient.
const EXACT_IN_F64: u128 = 1 << 53;

/// A positive rational number: the value of a unit in the coherent base units of its dimension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactScale(Repr);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Repr {
    One,
    /// numerator, denominator: reduced, both non-zero, not both 1.
    Small(u128, u128),
    Big(Arc<BigRational>),
}

impl ExactScale {
    pub fn one() -> Self {
        ExactScale(Repr::One)
    }

    pub fn is_one(&self) -> bool {
        matches!(self.0, Repr::One)
    }

    fn small(numer: u128, denom: u128) -> Self {
        if numer == 1 && denom == 1 {
            Self::one()
        } else {
            ExactScale(Repr::Small(numer, denom))
        }
    }

    /// The smallest representation that holds `ratio` (which is already reduced).
    fn from_ratio(ratio: BigRational) -> Self {
        match (ratio.numer().to_u128(), ratio.denom().to_u128()) {
            (Some(numer), Some(denom)) => Self::small(numer, denom),
            _ => ExactScale(Repr::Big(Arc::new(ratio))),
        }
    }

    fn to_big(&self) -> BigRational {
        match &self.0 {
            Repr::One => BigRational::one(),
            Repr::Small(numer, denom) => BigRational::new(BigInt::from(*numer), BigInt::from(*denom)),
            Repr::Big(ratio) => ratio.as_ref().clone(),
        }
    }

    /// Reads a decimal such as `1e-5`, `0.0254`, `6.02214076e+23` or a fraction `3/7` as
    /// Python's `Fraction(text)` does. `None` for zero, negative values, and anything that is
    /// not a finite decimal, so the caller keeps the `f64` it already has.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let ratio = match text.split_once('/') {
            Some((numer, denom)) => {
                let numer: BigInt = numer.trim().parse().ok()?;
                let denom: BigInt = denom.trim().parse().ok()?;
                if denom.is_zero() {
                    return None;
                }
                BigRational::new(numer, denom)
            }
            None => parse_decimal(text)?,
        };
        ratio.is_positive().then(|| Self::from_ratio(ratio))
    }

    /// The exact value of a double's shortest round-trip decimal (`0.001` for `1e-3`), for
    /// scales that reach the registry as `f64` rather than as the text of the catalog.
    pub fn from_f64(value: f64) -> Option<Self> {
        if !value.is_finite() || value <= 0.0 {
            return None;
        }
        Self::parse(&format!("{value:e}"))
    }

    pub fn mul(&self, other: &Self) -> Self {
        match (&self.0, &other.0) {
            (Repr::One, _) => other.clone(),
            (_, Repr::One) => self.clone(),
            (Repr::Small(an, ad), Repr::Small(bn, bd)) => match mul_small((*an, *ad), (*bn, *bd)) {
                Some((numer, denom)) => Self::small(numer, denom),
                None => Self::from_ratio(self.to_big() * other.to_big()),
            },
            _ => Self::from_ratio(self.to_big() * other.to_big()),
        }
    }

    /// `self / other`; `other` is never zero, because `parse` refuses zero.
    pub fn div(&self, other: &Self) -> Self {
        match (&self.0, &other.0) {
            (_, Repr::One) => self.clone(),
            (Repr::One, Repr::Small(numer, denom)) => Self::small(*denom, *numer),
            (Repr::Small(an, ad), Repr::Small(bn, bd)) => match mul_small((*an, *ad), (*bd, *bn)) {
                Some((numer, denom)) => Self::small(numer, denom),
                None => Self::from_ratio(self.to_big() / other.to_big()),
            },
            _ => Self::from_ratio(self.to_big() / other.to_big()),
        }
    }

    /// `self` raised to an integer power; `None` when the exponent is too large to build.
    pub fn powi(&self, exponent: i64) -> Option<Self> {
        if exponent.abs() > MAX_EXPONENT {
            return None;
        }
        let power = exponent.unsigned_abs();
        match &self.0 {
            Repr::One => Some(Self::one()),
            Repr::Small(numer, denom) => {
                let (numer, denom) = if exponent < 0 { (*denom, *numer) } else { (*numer, *denom) };
                match (u32::try_from(power).ok().and_then(|p| numer.checked_pow(p)), u32::try_from(power).ok().and_then(|p| denom.checked_pow(p))) {
                    (Some(n), Some(d)) => Some(Self::small(n, d)),
                    _ => Some(Self::big_pow(self, exponent)),
                }
            }
            Repr::Big(_) => Some(Self::big_pow(self, exponent)),
        }
    }

    fn big_pow(&self, exponent: i64) -> Self {
        let base = self.to_big();
        let base = if exponent < 0 { base.recip() } else { base };
        Self::from_ratio(num_traits::pow(base, exponent.unsigned_abs() as usize))
    }

    /// The nearest `f64`, rounded once (ties to even).
    pub fn to_f64(&self) -> f64 {
        match &self.0 {
            Repr::One => 1.0,
            Repr::Small(numer, denom) if *numer <= EXACT_IN_F64 && *denom <= EXACT_IN_F64 => {
                *numer as f64 / *denom as f64
            }
            Repr::Small(..) | Repr::Big(_) => self.to_big().to_f64().unwrap_or(f64::NAN),
        }
    }
}

/// The product of two reduced fractions, reduced, or `None` if it does not fit in `u128`.
/// Cross-reducing before multiplying keeps the result reduced without a final gcd.
fn mul_small((an, ad): (u128, u128), (bn, bd): (u128, u128)) -> Option<(u128, u128)> {
    let g1 = gcd(an, bd);
    let g2 = gcd(bn, ad);
    let numer = (an / g1).checked_mul(bn / g2)?;
    let denom = (ad / g2).checked_mul(bd / g1)?;
    Some((numer, denom))
}

/// Binary gcd of two non-zero integers.
fn gcd(mut a: u128, mut b: u128) -> u128 {
    let shift = (a | b).trailing_zeros();
    a >>= a.trailing_zeros();
    loop {
        b >>= b.trailing_zeros();
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        b -= a;
        if b == 0 {
            return a << shift;
        }
    }
}

fn parse_decimal(text: &str) -> Option<BigRational> {
    let (mantissa, exponent) = match text.find(|c| c == 'e' || c == 'E') {
        Some(at) => (&text[..at], text[at + 1..].parse::<i64>().ok()?),
        None => (text, 0),
    };
    let (negative, mantissa) = match mantissa.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, mantissa.strip_prefix('+').unwrap_or(mantissa)),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.bytes().chain(frac_part.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let power_of_ten = exponent.checked_sub(frac_part.len() as i64)?;
    if power_of_ten.abs() > MAX_EXPONENT {
        return None;
    }
    let mut numer: BigInt = format!("{int_part}{frac_part}").parse().ok()?;
    if negative {
        numer = -numer;
    }
    let ten_power = num_traits::pow(BigInt::from(10), power_of_ten.unsigned_abs() as usize);
    Some(if power_of_ten >= 0 {
        BigRational::from_integer(numer * ten_power)
    } else {
        BigRational::new(numer, ten_power)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact(text: &str) -> ExactScale {
        ExactScale::parse(text).unwrap_or_else(|| panic!("{text:?} should parse"))
    }

    fn is_small(scale: &ExactScale) -> bool {
        matches!(scale.0, Repr::Small(..))
    }

    fn is_big(scale: &ExactScale) -> bool {
        matches!(scale.0, Repr::Big(_))
    }

    #[test]
    fn parses_decimals_exponents_and_fractions() {
        assert!(exact("1").is_one());
        assert!(exact("1.0").is_one());
        assert!(exact("1e0").is_one());
        assert_eq!(exact("0.5"), exact("1/2"));
        assert_eq!(exact("1e3"), exact("1000"));
        assert_eq!(exact("1E-3"), exact("1/1000"));
        assert_eq!(exact("+2.5e+1"), exact("25"));
        assert_eq!(exact(" 0.0254 "), exact("127/5000"));
    }

    #[test]
    fn refuses_what_is_not_a_positive_finite_decimal() {
        for text in ["", "e5", ".", "abc", "0", "0.0", "-1", "1/0", "nan", "inf", "1e", "1.2.3", "0x10", "1e999999"] {
            assert!(ExactScale::parse(text).is_none(), "{text:?} should not parse");
        }
    }

    #[test]
    fn from_f64_reads_the_shortest_decimal_of_the_double() {
        assert_eq!(ExactScale::from_f64(0.001).unwrap(), exact("1/1000"));
        assert_eq!(ExactScale::from_f64(1.0).unwrap(), ExactScale::one());
        assert_eq!(ExactScale::from_f64(0.0174532925).unwrap(), exact("0.0174532925"));
        assert!(ExactScale::from_f64(0.0).is_none());
        assert!(ExactScale::from_f64(f64::NAN).is_none());
        assert!(ExactScale::from_f64(-2.0).is_none());
    }

    #[test]
    fn arithmetic_is_exact() {
        let kilo = exact("1e3");
        let milli = exact("1e-3");
        assert!(kilo.mul(&milli).is_one());
        assert!(kilo.div(&kilo).is_one());
        assert_eq!(milli.div(&kilo), exact("1e-6"));
        assert_eq!(kilo.powi(2).unwrap(), exact("1e6"));
        assert_eq!(kilo.powi(-2).unwrap(), exact("1e-6"));
        assert!(kilo.powi(0).unwrap().is_one());
        assert!(ExactScale::one().powi(7).unwrap().is_one());
        assert!(kilo.powi(401).is_none());
    }

    /// `1 ppm => percent`: the doubles 1e-6 and 0.01 divide to `9.999999999999999e-05`, the
    /// rationals the catalog states divide to exactly `1e-4`.
    #[test]
    fn a_ratio_is_rounded_once() {
        let ppm = exact("1e-6");
        let percent = exact("0.01");
        assert_ne!(1e-6_f64 / 0.01_f64, 1e-4);
        assert_eq!(ppm.div(&percent).to_f64(), 1e-4);
        assert_eq!(exact("0.1").div(&exact("0.01")).to_f64(), 10.0);
    }

    #[test]
    fn to_f64_rounds_to_nearest_even_beyond_what_i128_can_hold() {
        // quecto (1e-30) times a 15-digit scale: the denominator is 10^50.
        let tiny = exact("1e-30").mul(&exact("4.84813681109536e-06"));
        assert!(is_big(&tiny));
        assert_eq!(tiny.to_f64(), 4.84813681109536e-36);
        // 4.8e24 still fits in u128, but not in 53 bits: rounded through the exact division
        let big = exact("1e30").mul(&exact("4.84813681109536e-06"));
        assert!(is_small(&big));
        assert_eq!(big.to_f64(), 4.84813681109536e24);
    }

    #[test]
    fn a_value_is_held_in_the_smallest_representation_that_fits() {
        assert!(is_small(&exact("1e3")));
        assert!(is_small(&exact("0.0254")));
        assert!(is_big(&exact("1e50")));
        assert!(is_big(&exact("1e-50")));
        // leaving the u128 range and coming back normalises, so equal values compare equal
        let there = exact("1e50").mul(&exact("1e3"));
        assert!(is_big(&there));
        let back = there.div(&exact("1e50"));
        assert!(is_small(&back));
        assert_eq!(back, exact("1e3"));
        assert!(exact("1e50").div(&exact("1e50")).is_one());
    }

    #[test]
    fn small_arithmetic_that_overflows_falls_back_to_big_and_stays_exact() {
        let a = exact("1e20");
        let b = exact("1e20");
        let product = a.mul(&b);
        assert!(is_big(&product));
        assert_eq!(product, exact("1e40"));
        assert_eq!(a.powi(3).unwrap(), exact("1e60"));
        assert_eq!(a.powi(-3).unwrap(), exact("1e-60"));
        assert_eq!(exact("1e-20").div(&exact("1e20")), exact("1e-40"));
    }

    #[test]
    fn a_fraction_is_reduced_across_the_operands() {
        let third = exact("1/3");
        let nine = exact("9");
        assert_eq!(third.mul(&nine), exact("3"));
        assert_eq!(nine.div(&third), exact("27"));
        assert_eq!(exact("2/3").mul(&exact("3/2")), ExactScale::one());
    }

    #[test]
    fn to_f64_of_large_small_values_goes_through_the_exact_division() {
        // numerator and denominator both above 2^53, so the FPU quotient would round twice
        let scale = exact("0.30480000000000001");
        assert!(is_small(&scale));
        assert_eq!(scale.to_f64(), 0.30480000000000001);
    }
}
```

Expected: `written ... | 353 lines`; `edited` lines for `Cargo.toml` (twice) and `mod.rs` (twice); ten `... ok` lines and `test result: ok. 10 passed`.

- [ ] **Step 2: Check the lock file**

```bash
git diff Cargo.lock
```

Expected: the only change is one added line `"num-bigint",` in the `dependencies` of the package `name = "physure"`. If the diff contains anything else, stop and report.

- [ ] **Step 3: Commit**

```bash
git add physure-core/src/units/exact.rs physure-core/src/units/mod.rs physure-core/Cargo.toml Cargo.toml Cargo.lock
git diff --cached --name-status
git commit -m "feat(core): ExactScale, the exact rational value of a unit's scale" -m "A new type for the scale factors of units: a positive rational held as scale 1 (no data), a reduced fraction of two u128 (almost every unit) or a BigRational (quecto times a 15-digit scale needs a denominator of 10^50), always in the smallest representation that fits so that equal values compare equal. It reads a catalog literal as Python's Fraction(text) does, multiplies, divides and raises to integer powers exactly, and rounds to f64 once. num-bigint was already compiled as a dependency of num-rational; it becomes a direct dependency." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: three `M` and one `A` (`exact.rs`) and `M Cargo.lock`, five lines in all.

---

### Task 4: Convert with the exact scales

**Files:**
- Modify: `physure-core/src/units/rational.rs`, `registry.rs`, `conf.rs`, `tests.rs`
- Modify: `physure-core/src/quantity.rs`
- Modify: `physure-script/tests/unit_literal_regression.rs`
- Modify: `physure-python/tests/core/test_unit_algebra_parity.py`

- [ ] **Step 1: Tests first, and watch them fail**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_exact.py tests
cargo test -p physure --lib units::tests 2>&1 | grep -E "^error" | sort | uniq -c
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync pytest tests/core/test_unit_algebra_parity.py -q -p no:cacheprovider 2>&1 | tail -1
```

Expected: `edited` lines for `tests.rs`, `unit_literal_regression.rs` and `test_unit_algebra_parity.py`; the Rust build fails with `error[E0599]: no method named \`ratio_to\`` and with `error[E0609]: no field \`exact\` on type ...` (the new tests use what the next step adds); ruff clean; `2 failed, 4 passed, 5 xfailed` for the parity tests (the last-bit test and the compound test now demand no difference, and the extension still comes from the unchanged core).

- [ ] **Step 2: Apply the `wire` part**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_exact.py wire
cargo test -p physure 2>&1 | grep -E "^test result|FAILED|panicked|^error"
cargo test -p physure-script 2>&1 | grep -E "^test result|FAILED|panicked|^error"
```

Expected: `edited` lines for `rational.rs`, `registry.rs`, `conf.rs` and `quantity.rs`; the core crate prints three `test result: ok` lines (`197 passed`, `10 passed`, `3 passed`); the language crate prints only `ok` lines, none failed.

- [ ] **Step 3: The other crates still compile**

```bash
cargo check -p physure-cli -p physure-lsp -p physure-java -p physure-wasm --tests 2>&1 | grep -E "^error|Finished"
```

Expected: `Finished`, no `error`.

- [ ] **Step 4: Rebuild, then the parity test and the whole Python suite**

Run the **Rebuild** block of the Conventions, then:

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync pytest tests/core/test_unit_algebra_parity.py -q -p no:cacheprovider 2>&1 | tail -1
uv run --no-sync pytest -q -p no:cacheprovider 2>&1 | tail -1
cd ..
git checkout -- physure-python/physure/_generated_types.py physure-python/physure/core/registry.pyi physure-python/physure/units/__init__.pyi
git status --short | grep -v Cargo.lock
```

Expected: `6 passed, 5 xfailed`; `1021 passed, 6 xfailed`; the last `git status` lists only the modified files of this task (nothing else).

- [ ] **Step 5: Commit**

```bash
git add physure-core/src/units/rational.rs physure-core/src/units/registry.rs physure-core/src/units/conf.rs physure-core/src/units/tests.rs physure-core/src/quantity.rs physure-script/tests/unit_literal_regression.rs physure-python/tests/core/test_unit_algebra_parity.py
git diff --cached --name-status
git commit -m "feat(core): convert between units with exact rational scales" -m "RationalUnit carries its scale as an ExactScale next to the f64, which is now the single rounding of it. The catalog parser, the prefixes, and the product, quotient and integer power of units keep it, and a conversion (and the rescaling of the right operand of + and -) is source / target in the rationals, rounded once, as Python's Fraction arithmetic does: 1 ppm => percent was 9.999999999999999e-05, 1 ft => inch 12.000000000000002 and 1 g/cm^3 => kg/m^3 999.9999999999999. Converting out of an affine scale divides by its scale instead of multiplying by the reciprocal, which differs in the last bit for Fahrenheit and is what Python's converter does. The parity test, which recorded 47 symbols and one compound case that differed in the last bit, now asserts that none does." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: seven `M`.

---

### Task 5: Measure the cost and document

**Files:**
- Modify: `structure.md`, `CHANGELOG.md`
- Modify: `docs/superpowers/specs/2026-10-27-a1-rust-owned-unit-algebra-design.md`
- Modify: `docs/superpowers/specs/2026-10-27-a1-stage0-divergence-report.md`

- [ ] **Step 1: Measure against the baseline**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf
cargo bench -p physure --bench core_benchmarks -- --baseline before "unit|quantity|parse" 2>&1 | grep -E "time:|change:" | sed 's/\s\+/ /g'
```

Expected: the same nine benchmarks, each with a `change:` line. Within the noise described above: `unit_mul_prefixed` between +10 % and +60 %, `unit_div_prefixed` between +40 % and +120 %, `quantity_convert_prefixed` between +20 % and +100 % (about 75 ns more), the two parses between +10 % and +50 % (the registry they rebuild now parses an exact scale per unit), the benchmarks of units of scale 1 within about ±20 % on a quiet machine (±60 % on a busy one). A prefixed benchmark more than 3 times slower than the baseline: stop and report.

- [ ] **Step 2: Apply the `docs` part and commit**

```bash
python -X utf8 /c/Users/Irvin/AppData/Local/Temp/apply_exact.py docs
git add structure.md CHANGELOG.md docs/superpowers/specs/2026-10-27-a1-rust-owned-unit-algebra-design.md docs/superpowers/specs/2026-10-27-a1-stage0-divergence-report.md
git diff --cached --name-status
git commit -m "docs: the outcome of A1 stage 1a" -m "Records the representation chosen for the exact scale (the open item of the design), its measured cost, the stage 0 class that is closed, and what was found on the way: affine conversions are not exact (100 degC is 211.99999999999997 degF), the second unit model still holds f64 scales, and the benchmarks did not compile. CHANGELOG entries for the exact conversions and the repaired benchmarks." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Expected `--name-status`: four `M`.

---

### Task 6: Verify the whole project

**Files:** none

- [ ] **Step 1: Lint, format, and the strict docs build**

```bash
cd /d/Projects/physure/.claude/worktrees/single-conf/physure-python
unset VIRTUAL_ENV
uv run --no-sync ruff check . && uv run --no-sync ruff format --check .
uv run --no-sync mkdocs build --strict 2>&1 | tail -1
```

Expected: `All checks passed!` and every file formatted; `INFO    -  Documentation built in ... seconds`.

- [ ] **Step 2: The full suites, as CI runs them**

```bash
uv run --no-sync pytest -q -p no:cacheprovider 2>&1 | tail -1
cd ..
git checkout -- physure-python/physure/_generated_types.py physure-python/physure/core/registry.pyi physure-python/physure/units/__init__.pyi
cargo test -p physure 2>&1 | grep -E "^test result"
cargo test -p physure-script 2>&1 | grep -E "^test result"
```

Expected: `1021 passed, 6 xfailed`; the core crate three `ok` lines (`197`, `10`, `3` passed); the language crate only `ok` lines. If any other test fails, run it alone and report; do not change anything else.

- [ ] **Step 3: The catalog audit still holds, and first use stays fast**

```bash
cd physure-python
unset VIRTUAL_ENV
uv run --no-sync python -X utf8 ../docs/superpowers/specs/catalog-audit/audit_identities.py 2>&1 | grep -E "identities off"
uv run --no-sync python -m physure "500 N / 2 m^2 => kPa" > /dev/null
time uv run --no-sync python -m physure "500 N / 2 m^2 => kPa"
cd ..
git status --short | grep -v Cargo.lock
git log --oneline -6
```

Expected: `0 of 27 identities off by more than 4e-16`; `0.25 kPa` in about 0.1 to 0.2 s on the second run (0.3 s at most when the machine is busy); the `git status` prints nothing; four new commits on `feat/a1-exact-scales` on top of `main`.

- [ ] **Step 4: Hand-off**

Report the branch name, the four commit SHAs and the measured numbers (they should match the table above, within the noise of the benchmarks). Nothing is pushed. Integrating the branch is the maintainer's decision. Next: A1 stage 1b (long prefix names, the logarithmic units `dB`/`decibel`/`pH`, and the delta-temperature rule).

---

## Reference: the script this plan embeds

Create `apply_exact.py` with exactly this content:

```python
"""A1 stage 1a: exact rational scales in the core.

Usage (from the repository root of the worktree):
    python -X utf8 apply_exact.py bench | type | tests | wire | docs

Every replacement asserts that its anchor occurs exactly as many times as
expected, and every file keeps its own line endings.
"""

import pathlib
import re
import sys

CORE = pathlib.Path("physure-core")
UNITS = CORE / "src/units"
TESTS_PY = pathlib.Path("physure-python/tests")


def load(path):
    raw = path.read_bytes().decode("utf-8")
    eol = "\r\n" if "\r\n" in raw else "\n"
    return raw.replace(eol, "\n"), eol


def save(path, text, eol):
    path.write_bytes(text.replace("\n", eol).encode("utf-8"))
    print("edited", path)


def edit(path, replacements):
    text, eol = load(path)
    for old, new in replacements:
        assert text.count(old) == 1, f"{path}: {old[:70]!r} found {text.count(old)}, expected 1"
        text = text.replace(old, new)
    save(path, text, eol)


def append(path, addition):
    text, eol = load(path)
    save(path, text.rstrip("\n") + "\n" + addition, eol)


part = sys.argv[1]

if part == "bench":
    # The benchmark file predates two refactors, so it does not compile: the crate is
    # `physure_core` (package `physure`), and the symbolic engine moved to physure-script.
    # Repair it and add the benchmarks that exercise a scale other than 1.
    path = CORE / "benches/core_benchmarks.rs"
    text, eol = load(path)
    assert "bench_prefixed_units" not in text
    old = "use physure::{Quantity, RationalUnit, CovarianceStore, PruningConfig, symbolic::Expr};\n"
    assert text.count(old) == 1
    text = text.replace(
        old,
        "use physure_core::{Quantity, RationalUnit, CovarianceStore, PruningConfig};\n"
        "use physure_core::units::Parser;\n",
    )
    text, n = re.subn(r"fn bench_symbolic_eval\(c: &mut Criterion\) \{.*?\n\}\n\n", "", text, flags=re.S)
    assert n == 1
    old = "criterion_group!(benches, bench_units, bench_quantity_arithmetic, bench_covariance_propagation, bench_symbolic_eval);"
    assert text.count(old) == 1
    text = text.replace(
        old,
        "criterion_group!(benches, bench_units, bench_prefixed_units, bench_quantity_arithmetic, bench_covariance_propagation);",
    )
    old = "fn bench_quantity_arithmetic(c: &mut Criterion) {"
    assert text.count(old) == 1
    text = text.replace(
        old,
        "/// Units whose scale is not 1, which is where an exact scale costs something: the scale-1 units\n"
        "/// of `bench_units` take the fast path whatever the representation.\n"
        "fn bench_prefixed_units(c: &mut Criterion) {\n"
        "    let km = Parser::parse_expression(\"km\").unwrap();\n"
        "    let ms = Parser::parse_expression(\"ms\").unwrap();\n"
        "    let mile = Parser::parse_expression(\"mile\").unwrap();\n"
        "    let q = Quantity::with_uncertainty(5.0, 0.0, \"km\").unwrap();\n\n"
        "    c.bench_function(\"unit_mul_prefixed\", |b| b.iter(|| km.mul(&ms)));\n"
        "    c.bench_function(\"unit_div_prefixed\", |b| b.iter(|| km.div(&ms)));\n"
        "    c.bench_function(\"quantity_convert_prefixed\", |b| b.iter(|| q.convert_to(&mile).unwrap()));\n"
        "    c.bench_function(\"parse_compound_unit\", |b| b.iter(|| Parser::parse_expression(\"kg*m/s^2\").unwrap()));\n"
        "    c.bench_function(\"parse_prefixed_compound_unit\", |b| b.iter(|| Parser::parse_expression(\"km/h\").unwrap()));\n"
        "}\n\n"
        "fn bench_quantity_arithmetic(c: &mut Criterion) {",
    )
    save(path, text, eol)

elif part == "type":
    edit(pathlib.Path("Cargo.toml"), [
        ('num-rational  = "0.4"\n', 'num-bigint    = "0.4"\nnum-rational  = "0.4"\n'),
    ])
    edit(CORE / "Cargo.toml", [
        ("num-rational.workspace = true\n", "num-bigint.workspace   = true\nnum-rational.workspace = true\n"),
    ])
    edit(UNITS / "mod.rs", [
        ("pub mod dimension;\n", "pub mod dimension;\npub mod exact;\n"),
        ("pub use dimension::{dim_index, to_superscript, DimVector, SI_ORDER};\n",
         "pub use dimension::{dim_index, to_superscript, DimVector, SI_ORDER};\npub use exact::ExactScale;\n"),
    ])

elif part == "tests":
    # tests first: they do not compile until the `wire` part, which is what makes them red
    append(UNITS / "tests.rs", '''
/// The doubles 1e-6 and 0.01 divide to 9.999999999999999e-05; the rationals the catalog states
/// divide to exactly 1e-4. A conversion is rounded once, from the exact scales.
#[test]
fn a_conversion_is_rounded_once_from_the_exact_scales() {
    let (reg, _) = conf::build_registry_from_conf();
    let ratio = |from: &str, to: &str| {
        let from = Parser::parse_expression_with_registry(from, &reg).unwrap();
        let to = Parser::parse_expression_with_registry(to, &reg).unwrap();
        from.ratio_to(&to)
    };
    assert_eq!(ratio("ppm", "percent"), 1e-4);
    assert_eq!(ratio("g/cm^3", "kg/m^3"), 1000.0);
    assert_eq!(ratio("ft", "inch"), 12.0);
    assert_eq!(ratio("mile", "ft"), 5280.0);
    assert_eq!(ratio("lb", "oz"), 16.0);
    assert_eq!(ratio("gal", "inch^3"), 231.0);
    assert_eq!(ratio("acre", "ft^2"), 43560.0);
}

/// `scale` is the exact scale rounded once, for every unit of the catalog and for the units
/// that prefixes and products build from them.
#[test]
fn the_scale_is_the_exact_scale_rounded_once() {
    let (reg, _) = conf::build_registry_from_conf();
    for (name, unit) in reg.base_units.iter().chain(reg.derived_units.iter()) {
        let exact = unit.exact.as_ref().unwrap_or_else(|| panic!("{name} has no exact scale"));
        assert_eq!(unit.scale.to_bits(), exact.to_f64().to_bits(), "{name}");
    }
    let tiny = reg.get_unit("qarcsec").unwrap();
    assert_eq!(tiny.scale, 4.84813681109536e-36);
    assert!(tiny.exact.is_some());
    let product = reg.get_unit("km").unwrap().mul(&reg.get_unit("ms").unwrap());
    assert_eq!(product.scale, 1.0);
    assert!(product.exact.as_ref().is_some_and(ExactScale::is_one));
}

/// A fractional power cannot be held exactly (Python falls back to a float there too), except
/// that 1 to any power is 1. A scale set from a bare `f64` is not exact either.
#[test]
fn what_cannot_be_exact_falls_back_to_the_double() {
    let (reg, _) = conf::build_registry_from_conf();
    let root = reg.get_unit("km").unwrap().pow(Rational64::new(1, 2));
    assert!(root.exact.is_none());
    assert_eq!(root.scale, 1000f64.powf(0.5));
    let root_of_one = reg.get_unit("m").unwrap().pow(Rational64::new(1, 2));
    assert!(root_of_one.exact.as_ref().is_some_and(ExactScale::is_one));
    let loose = length().with_scale(0.5);
    assert!(loose.exact.is_none());
    assert_eq!(loose.ratio_to(&length()), 0.5);
    let sure = length().with_exact_scale(ExactScale::parse("1/3").unwrap());
    assert_eq!(sure.scale, 1.0 / 3.0);
    assert_eq!(sure.ratio_to(&length()), 1.0 / 3.0);
}
''')
    append(pathlib.Path("physure-script/tests/unit_literal_regression.rs"), '''
/// A conversion is rounded once from the exact scales of the catalog, so a relation that holds
/// by definition comes out as written: `1 ppm => percent` was `9.999999999999999e-05` and
/// `1 ft => inch` was `12.000000000000002`.
#[test]
fn conversions_are_rounded_once_from_exact_scales() {
    let cases = [
        ("1 ppm => percent", 1e-4),
        ("1 g/cm^3 => kg/m^3", 1000.0),
        ("1 ft => inch", 12.0),
        ("1 mile => ft", 5280.0),
        ("1 lb => oz", 16.0),
        ("1 gal => inch^3", 231.0),
        ("1 acre => ft^2", 43560.0),
        ("1 ft + 1 inch => inch", 13.0),
    ];
    for (src, expected) in cases {
        assert_eq!(eval_quantity(src).value.mean(), expected, "{src}");
    }
}
''')
    # Python and Rust must now agree to the last bit: the recorded exceptions go
    parity = TESTS_PY / "core/test_unit_algebra_parity.py"
    text, eol = load(parity)
    text, n = re.subn(
        r"# Symbols whose conversion differs from Rust in the last bit\..*?\n\)\n\n", "", text, flags=re.S
    )
    assert n == 1
    old = 'KNOWN_COMPOUND_LAST_BIT = (("g/cm^3", "kg/m^3"),)\n'
    assert text.count(old) == 1
    text = text.replace(old, "")
    start = text.index("def test_last_bit_differences_are_the_known_ones(comparison):")
    end = text.index("@pytest.mark.xfail(", start)
    text = (
        text[:start]
        + "def test_python_and_rust_agree_to_the_last_bit(comparison):\n"
        + "    found = _of_kind(comparison, \"last-bit-difference\")\n"
        + "    assert not found, _show(found)\n\n\n"
        + text[end:]
    )
    start = text.index("    assert tuple(sorted(differing)) == tuple(")
    end = text.index("def _plain(number):")
    text = (
        text[:start]
        + "    assert not differing, f\"compound conversions that differ: {differing}\"\n\n\n"
        + text[end:]
    )
    save(parity, text, eol)

elif part == "wire":
    edit(UNITS / "rational.rs", [
        ("use smallvec::SmallVec;\n", "use smallvec::SmallVec;\nuse crate::units::exact::ExactScale;\n"),
        ("    pub scale: f64,\n    /// Additive term",
         "    pub scale: f64,\n"
         "    /// The same factor as an exact rational, when it is known. Invariant: if this is `Some(e)`,\n"
         "    /// `scale` is `e` rounded once to the nearest double. A conversion divides the exact\n"
         "    /// scales and rounds the quotient once; only a unit whose scale came from a bare `f64`\n"
         "    /// (or from a fractional power) has `None` and falls back to dividing the doubles.\n"
         "    pub exact: Option<ExactScale>,\n"
         "    /// Additive term"),
        ("RationalUnit { dimensions, scale: 1.0, offset: 0.0, id, display_name: None }\n    }\n\n    /// Returns a copy of this unit with a different scale factor (same dimensions/id).\n    pub fn with_scale(mut self, scale: f64) -> Self {\n        self.scale = scale;\n        self\n    }\n",
         "RationalUnit { dimensions, scale: 1.0, exact: Some(ExactScale::one()), offset: 0.0, id, display_name: None }\n    }\n\n"
         "    /// Returns a copy of this unit with a different scale factor (same dimensions/id). The\n"
         "    /// factor is a bare `f64`, so the exact scale is no longer known.\n"
         "    pub fn with_scale(mut self, scale: f64) -> Self {\n        self.scale = scale;\n        self.exact = None;\n        self\n    }\n\n"
         "    /// The same dimensions measured in the coherent base units: scale 1, exactly.\n"
         "    pub fn with_unit_scale(mut self) -> Self {\n        self.scale = 1.0;\n        self.exact = Some(ExactScale::one());\n        self\n    }\n\n"
         "    /// Returns a copy of this unit with an exact scale factor; `scale` is its single rounding.\n"
         "    pub fn with_exact_scale(mut self, exact: ExactScale) -> Self {\n        self.scale = exact.to_f64();\n        self.exact = Some(exact);\n        self\n    }\n\n"
         "    /// The factor that turns a magnitude in this unit into one in `target`, which must have the\n"
         "    /// same dimensions. Both exact scales are divided and the quotient rounded once; if either\n"
         "    /// is unknown the doubles are divided instead.\n"
         "    pub fn ratio_to(&self, target: &Self) -> f64 {\n"
         "        match (&self.exact, &target.exact) {\n"
         "            (Some(from), Some(to)) => from.div(to).to_f64(),\n"
         "            _ => self.scale / target.scale,\n"
         "        }\n    }\n"),
        ("            dimensions: DimVec::new(),\n            scale: 1.0,\n            offset: 0.0,",
         "            dimensions: DimVec::new(),\n            scale: 1.0,\n            exact: Some(ExactScale::one()),\n            offset: 0.0,"),
        ("        // No offset on a product: `degC * m` has no zero point to speak of. Callers that hold\n        // an affine operand must `to_delta` it (and shift its magnitude) before multiplying.\n        RationalUnit { dimensions: new_dims, scale: self.scale * other.scale, offset: 0.0, id, display_name }",
         "        let exact = match (&self.exact, &other.exact) {\n            (Some(a), Some(b)) => Some(a.mul(b)),\n            _ => None,\n        };\n"
         "        let scale = exact.as_ref().map_or(self.scale * other.scale, ExactScale::to_f64);\n"
         "        // No offset on a product: `degC * m` has no zero point to speak of. Callers that hold\n        // an affine operand must `to_delta` it (and shift its magnitude) before multiplying.\n        RationalUnit { dimensions: new_dims, scale, exact, offset: 0.0, id, display_name }"),
        ("        RationalUnit { dimensions: new_dims, scale: self.scale / other.scale, offset: 0.0, id, display_name }",
         "        let exact = match (&self.exact, &other.exact) {\n            (Some(a), Some(b)) => Some(a.div(b)),\n            _ => None,\n        };\n"
         "        let scale = exact.as_ref().map_or(self.scale / other.scale, ExactScale::to_f64);\n"
         "        RationalUnit { dimensions: new_dims, scale, exact, offset: 0.0, id, display_name }"),
        ("        let exp_f = *exp_r.numer() as f64 / *exp_r.denom() as f64;\n        RationalUnit { dimensions: new_dims, scale: self.scale.powf(exp_f), offset: 0.0, id, display_name: None }",
         "        let exp_f = *exp_r.numer() as f64 / *exp_r.denom() as f64;\n"
         "        // A whole power of an exact scale stays exact. A fractional one does not (Python falls\n        // back to a float there too), except that 1 to any power is 1.\n"
         "        let exact = match &self.exact {\n"
         "            Some(e) if e.is_one() => Some(ExactScale::one()),\n"
         "            Some(e) if *exp_r.denom() == 1 => e.powi(*exp_r.numer()),\n"
         "            _ => None,\n"
         "        };\n"
         "        let scale = exact.as_ref().map_or(self.scale.powf(exp_f), ExactScale::to_f64);\n"
         "        RationalUnit { dimensions: new_dims, scale, exact, offset: 0.0, id, display_name: None }"),
    ])
    edit(UNITS / "registry.rs", [
        ("use crate::units::rational::RationalUnit;\n",
         "use crate::units::exact::ExactScale;\nuse crate::units::rational::RationalUnit;\n"),
        ("    pub prefixes: HashMap<String, f64>,\n",
         "    pub prefixes: HashMap<String, f64>,\n"
         "    /// The exact value of each prefix factor, for the prefixes whose factor is known exactly.\n"
         "    pub prefix_exact: HashMap<String, ExactScale>,\n"),
        ("            prefixes: HashMap::new(),\n",
         "            prefixes: HashMap::new(),\n            prefix_exact: HashMap::new(),\n"),
        ("    pub fn add_prefix(&mut self, symbol: String, factor: f64) {\n        self.prefixes.insert(symbol, factor);\n    }\n",
         "    pub fn add_prefix(&mut self, symbol: String, factor: f64) {\n"
         "        self.add_prefix_exact(symbol, factor, ExactScale::from_f64(factor));\n    }\n\n"
         "    /// Registers a prefix with its factor as the double and, when known, as the exact value\n"
         "    /// that the catalog states.\n"
         "    pub fn add_prefix_exact(&mut self, symbol: String, factor: f64, exact: Option<ExactScale>) {\n"
         "        match exact {\n"
         "            Some(e) => self.prefix_exact.insert(symbol.clone(), e),\n"
         "            None => self.prefix_exact.remove(&symbol),\n"
         "        };\n"
         "        self.prefixes.insert(symbol, factor);\n    }\n"),
        ("    pub fn add_scaled_unit(&mut self, name: String, base: &str, scale: f64) {\n        let base_unit = self.get_unit(base).expect(\"base unit must be registered first\");\n        let new_scale = base_unit.scale * scale;\n        let mut u = base_unit.with_scale(new_scale);\n        u.display_name = Some(name.clone());\n        self.derived_units.insert(name, u);\n    }\n",
         "    pub fn add_scaled_unit(&mut self, name: String, base: &str, scale: f64) {\n"
         "        self.add_scaled_unit_exact(name, base, scale, ExactScale::from_f64(scale));\n    }\n\n"
         "    /// Like `add_scaled_unit`, with the factor also given as the exact value the catalog states.\n"
         "    pub fn add_scaled_unit_exact(&mut self, name: String, base: &str, scale: f64, exact: Option<ExactScale>) {\n"
         "        let base_unit = self.get_unit(base).expect(\"base unit must be registered first\");\n"
         "        let mut u = Self::scaled(base_unit, scale, exact);\n"
         "        u.display_name = Some(name.clone());\n        self.derived_units.insert(name, u);\n    }\n\n"
         "    /// `base` rescaled by `factor`: exact when both the base and the factor are, otherwise the\n"
         "    /// product of the doubles.\n"
         "    fn scaled(base: RationalUnit, factor: f64, exact: Option<ExactScale>) -> RationalUnit {\n"
         "        match (&base.exact, exact) {\n"
         "            (Some(b), Some(e)) => {\n                let combined = b.mul(&e);\n                base.with_exact_scale(combined)\n            }\n"
         "            _ => {\n                let product = base.scale * factor;\n                base.with_scale(product)\n            }\n"
         "        }\n    }\n"),
        ("    pub fn add_affine_unit(&mut self, name: String, base: &str, scale: f64, offset: f64) {\n        let base_unit = self.get_unit(base).expect(\"base unit must be registered first\");\n        let new_scale = base_unit.scale * scale;\n        let mut u = base_unit.with_scale(new_scale).with_offset(offset);\n        u.display_name = Some(name.clone());\n        self.derived_units.insert(name, u);\n    }\n",
         "    pub fn add_affine_unit(&mut self, name: String, base: &str, scale: f64, offset: f64) {\n"
         "        self.add_affine_unit_exact(name, base, scale, ExactScale::from_f64(scale), offset);\n    }\n\n"
         "    /// Like `add_affine_unit`, with the scale also given as the exact value the catalog states.\n"
         "    pub fn add_affine_unit_exact(&mut self, name: String, base: &str, scale: f64, exact: Option<ExactScale>, offset: f64) {\n"
         "        let base_unit = self.get_unit(base).expect(\"base unit must be registered first\");\n"
         "        let mut u = Self::scaled(base_unit, scale, exact).with_offset(offset);\n"
         "        u.display_name = Some(name.clone());\n        self.derived_units.insert(name, u);\n    }\n"),
        ("            self.split_prefix(name).map(|(_p_sym, p_factor, base_u)| {\n                let new_scale = base_u.scale * p_factor;\n                let mut prefixed = base_u.with_scale(new_scale);\n",
         "            self.split_prefix(name).map(|(p_sym, p_factor, base_u)| {\n                let mut prefixed = Self::scaled(base_u, p_factor, self.prefix_exact.get(&p_sym).cloned());\n"),
    ])
    edit(UNITS / "conf.rs", [
        ("                        if let Ok(factor) = parts[1].parse::<f64>() {\n                            registry.add_prefix(symbol.to_string(), factor);\n                        }\n",
         "                        if let Ok(factor) = parts[1].parse::<f64>() {\n"
         "                            registry.add_prefix_exact(symbol.to_string(), factor, crate::units::exact::ExactScale::parse(parts[1]));\n                        }\n"),
        ("    let factor: f64 = match parts[0].parse() {\n        Ok(f) => f,\n        Err(_) => return,\n    };\n",
         "    let factor: f64 = match parts[0].parse() {\n        Ok(f) => f,\n        Err(_) => return,\n    };\n"
         "    // The same factor as the exact rational the catalog writes: conversions use it so that a\n"
         "    // ratio is rounded once. A literal that is not a plain decimal stays a double.\n"
         "    let exact = crate::units::exact::ExactScale::parse(parts[0]);\n"),
        ("        let mut u = RationalUnit::new_from_dimensions([]).with_scale(factor);\n",
         "        let mut u = match &exact {\n            Some(e) => RationalUnit::new_from_dimensions([]).with_exact_scale(e.clone()),\n            None => RationalUnit::new_from_dimensions([]).with_scale(factor),\n        };\n"),
        ("            registry.add_affine_unit(symbol.clone(), &base_symbol, factor, offset);\n",
         "            registry.add_affine_unit_exact(symbol.clone(), &base_symbol, factor, exact, offset);\n"),
        ("            registry.add_scaled_unit(symbol.clone(), &base_symbol, factor);\n",
         "            registry.add_scaled_unit_exact(symbol.clone(), &base_symbol, factor, exact);\n"),
        ("            let new_scale = parsed_unit.scale * factor;\n            // Carry the zero point through here too: dropping it would silently turn an\n            // affine definition into a purely multiplicative one.\n            let mut u = parsed_unit.with_scale(new_scale).with_offset(offset);\n",
         "            let rescaled = match (&parsed_unit.exact, &exact) {\n                (Some(p), Some(e)) => {\n                    let combined = p.mul(e);\n                    parsed_unit.with_exact_scale(combined)\n                }\n                _ => {\n                    let product = parsed_unit.scale * factor;\n                    parsed_unit.with_scale(product)\n                }\n            };\n"
         "            // Carry the zero point through here too: dropping it would silently turn an\n            // affine definition into a purely multiplicative one.\n            let mut u = rescaled.with_offset(offset);\n"),
    ])
    edit(CORE / "src/quantity.rs", [
        # a conversion is rounded once from the exact scales
        ("        let other_value = if self.unit.scale != other.unit.scale {\n            Self::scale_value(&other.value, other.unit.scale / self.unit.scale)?\n        } else {\n            other.value.clone()\n        };\n        let new_value = self.value.propagate_add(&other_value)?;",
         "        let other_value = if self.unit.scale != other.unit.scale {\n            Self::scale_value(&other.value, other.unit.ratio_to(&self.unit))?\n        } else {\n            other.value.clone()\n        };\n        let new_value = self.value.propagate_add(&other_value)?;"),
        ("        let other_value = if self.unit.scale != other.unit.scale {\n            Self::scale_value(&other.value, other.unit.scale / self.unit.scale)?\n        } else {\n            other.value.clone()\n        };\n        let new_value = self.value.propagate_sub(&other_value)?;",
         "        let other_value = if self.unit.scale != other.unit.scale {\n            Self::scale_value(&other.value, other.unit.ratio_to(&self.unit))?\n        } else {\n            other.value.clone()\n        };\n        let new_value = self.value.propagate_sub(&other_value)?;"),
        ("        let ratio = self.unit.scale / target.scale;\n        let new_value = Self::scale_value(&self.value, ratio)?;",
         "        let ratio = self.unit.ratio_to(target);\n        let new_value = Self::scale_value(&self.value, ratio)?;"),
        # resets of the scale keep the exact scale in step with it
        ("            let mut canonical_unit = self.unit.clone();\n            canonical_unit.scale = 1.0;\n",
         "            let canonical_unit = self.unit.clone().with_unit_scale();\n"),
        ("        let mut base_unit = self.unit.clone();\n        base_unit.scale = 1.0;\n        base_unit.display_name = None;\n",
         "        let mut base_unit = self.unit.clone().with_unit_scale();\n        base_unit.display_name = None;\n"),
        ("        let mut base = self.unit.to_delta().with_scale(1.0);\n",
         "        let mut base = self.unit.to_delta().with_unit_scale();\n"),
        # converting out of an affine scale divides by the scale, as the Python converter does
        ("    /// Shifts an uncertainty value by an exact additive constant.",
         "    /// Divides an uncertainty value by an exact constant, the inverse of `scale_value`. It is\n"
         "    /// not the same as multiplying by the reciprocal: `x / 0.5555555555555556` and\n"
         "    /// `x * (1.0 / 0.5555555555555556)` differ in the last bit, and the Python side divides.\n"
         "    fn unscale_value(value: &UncertaintyValue, divisor: f64) -> PhysureResult<UncertaintyValue> {\n"
         "        if divisor == 1.0 {\n            return Ok(value.clone());\n        }\n"
         "        value.propagate_div(&UncertaintyValue::Gaussian(GaussianBackend::exact(divisor)))\n    }\n\n"
         "    /// Shifts an uncertainty value by an exact additive constant."),
        ("                Self::scale_value(&Self::shift_value(&base, -target.offset)?, 1.0 / target.scale)?;",
         "                Self::unscale_value(&Self::shift_value(&base, -target.offset)?, target.scale)?;"),
    ])

elif part == "docs":
    edit(pathlib.Path("structure.md"), [
        ("node_units[\"units/<br/>RationalUnit, UnitRegistry, .conf parsing<br/>[units/mod.rs]\"]",
         "node_units[\"units/<br/>RationalUnit with exact rational scales, UnitRegistry, .conf parsing<br/>[units/mod.rs]\"]"),
    ])
    edit(pathlib.Path("CHANGELOG.md"), [(
        "\n\n### Fixed\n- **`SymbolicQuantity`,",
        "\n- **The Rust core converts between units with exact rational scales.** A unit's scale is kept as the rational that `physure.conf` writes (`ExactScale`, in `physure-core/src/units/exact.rs`) and a conversion divides two of them and rounds once, as Python's `Fraction` arithmetic does. `1 ppm => percent` was `9.999999999999999e-05`, `1 ft => inch` was `12.000000000000002` and `1 g/cm^3 => kg/m^3` was `999.9999999999999`; they are `1e-4`, `12.0` and `1000.0`, and Python and Rust now agree to the last bit over the whole catalog. Converting out of Celsius or Fahrenheit divides by the scale instead of multiplying by its reciprocal, as Python does. The `scale` field of `RationalUnit` stays an `f64`, now the single rounding of the exact one.\n\n### Fixed\n- **`SymbolicQuantity`,",
    )])
    edit(pathlib.Path("CHANGELOG.md"), [(
        "\n\n## [0.2.4] - 2026-08-28\n",
        "\n- **The `physure` core benchmarks compile again.** `benches/core_benchmarks.rs` still imported the crate by its old name and a symbolic engine that moved to `physure-script`; the symbolic benchmark is gone and benchmarks for prefixed units, conversions and unit parsing are added.\n\n## [0.2.4] - 2026-08-28\n",
    )])
    append(pathlib.Path("docs/superpowers/specs/2026-10-27-a1-stage0-divergence-report.md"), '''
## Update — A1 stage 1a

The `last-bit-difference` class is closed. The core keeps each scale as the exact rational the catalog writes and converts with one rounding, so `KNOWN_LAST_BIT` and `KNOWN_COMPOUND_LAST_BIT` are gone and the tests assert that Python and Rust agree to the last bit over the whole catalog and over the compound cases. The other four classes remain, for stages 1b and 2.
''')
    append(pathlib.Path("docs/superpowers/specs/2026-10-27-a1-rust-owned-unit-algebra-design.md"), '''
## Outcome of stage 1a (exact scales)

- **Width of the exact type: decided.** `i128` is not enough (quecto times a scale written with 15 digits already needs a denominator of 10^50, and the catalog has such units), `f64` is not exact, so the exact scale has three representations: scale 1 (a variant with no data), a reduced fraction of two `u128` for almost every unit, and `BigRational` for the rest. A value is always held in the smallest one that fits. No new crate is compiled: `num-bigint` was already a dependency of `num-rational`.
- **The `f64` stays.** `RationalUnit.scale` is the exact scale rounded once; the exact scale is an `Option` next to it that is `None` only for a scale that arrived as a bare `f64` or from a fractional power, which is where Python falls back to a float as well. No binding changes.
- **Exit criterion met for this class:** with the exact scales, Python and Rust differ in the last bit for no symbol of the catalog and for no compound case (47 symbols and one compound case before).
- **Cost, measured with the repaired benchmarks** (Windows, a quiet run; on a busy machine the benchmarks that the change cannot touch moved by up to 60 %, so read these as orders of magnitude): a conversion between prefixed units went from about 156 ns to about 233 ns (+31 %), a product of prefixed units from about 117 ns to 147 ns and a quotient from 118 ns to 218 ns, units of scale 1 by +5 % to +15 %, and parsing a unit expression from about 0.53 ms to 0.67 ms (+28 %). The parse is slow to begin with because `Parser::parse_expression` rebuilds the whole registry from the catalog on every call; the extra 0.14 ms is the exact scale built for each of its units. Caching that registry is the remedy; it is not part of this work. With `BigRational` alone the same conversion cost about 770 ns, which is why the `u128` representation exists.
- **Found on the way, not fixed here:** (1) converting between affine scales is not exact either: `100 degC => degF` is `211.99999999999997` in Python and in Rust, because the zero point of Fahrenheit is a double; it needs exact affine arithmetic. (2) `UnitDefinition` / `UnitConverter`, the second unit model that the Python binding exposes, still holds `f64` scales. (3) `core_benchmarks.rs` did not compile; it does now.
''')

else:
    raise SystemExit(f"unknown part: {part}")
```
