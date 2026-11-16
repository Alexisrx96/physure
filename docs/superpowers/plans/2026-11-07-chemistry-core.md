# Chemistry in the Rust Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port formula parsing, molar mass, mass↔mole conversion, reaction balancing and the three thermo/kinetics functions from `physure-python/physure/ext/chemistry/` into a new `physure-core/src/chemistry/` module, held to the Python results by a golden file.

**Architecture:** Five small files under `physure-core/src/chemistry/` (`elements`, `formula`, `species`, `reaction`, `thermo`). Exact integer arithmetic (`Rational64`, checked) for balancing; `Quantity` for everything with a unit, so the dimension checks, the uncertainty propagation and the gas constant come from the core. No new dependency. `tests/golden/chemistry_*.tsv`, produced by the Python implementation, freeze what the port must reproduce.

**Tech Stack:** Rust 2021, `num-rational` / `num-traits` (already in `physure-core`), Python (stdlib + the existing `physure` install) for the golden generator only.

**Scope.** This is plan 1 of 3 for Part A of `docs/implementation_plan.md`. It covers A1, A2 and A3 and ends with a green `cargo test -p physure`. Not here, and written after this lands (they need the final core API):

- **Plan 2 — PHS `chem` domain (A4):** `domain_members`, `eval_chem_builtin`, `PhsValue::Species/Reaction`, and the `use dot from matrix` panic. The file paths in `implementation_plan.md` for this part are stale (`physure-script/src/builtins.rs` and `interpreter.rs` no longer exist at those paths after the parser split) and must be re-read first.
- **Plan 3 — Python delegates (A5):** PyO3 bindings in `physure-python/src/lib.rs`, and the Python chemistry package calling `physure._core`. `implementation_plan.md` A5 says to keep the pure-Python implementation as a fallback; that contradicts the project rule (CLAUDE.md "The Rust core comes first", and "Python has no independent implementation"). Plan 3 deletes the Python algorithms instead of keeping them.

## Decisions fixed here (so nobody re-litigates them mid-task)

- **Rust wins where it differs from Python, and the difference goes in CHANGELOG** (Task 8). The deliberate differences:
  1. A hydrate dot is accepted (`CuSO4·5H2O`, `CuSO4*5H2O`); Python rejects it.
  2. A count of zero is refused (`H0`); Python accepted it and produced a species with no atoms.
  3. `arrhenius` keeps the uncertainty of `Ea`, `T` and `R`; Python took `math.exp(-magnitude)` and dropped it. Dropping an uncertainty is the invariant CLAUDE.md forbids.
  4. Overflow in the balancing arithmetic is an error; Python integers do not overflow.
- **No `phf`, no `ChemistryError`, no new crate dependency** (`implementation_plan.md` §"Out of scope").
- **`standard_enthalpy` / `standard_entropy` stay in Python.** They are lookup tables nothing in PHS needs yet.
- **`Reaction.calculate` (limiting reactant) is not ported.** It is a loop over `mass_to_moles`, which is ported; Python keeps the loop in plan 3.
- **Errors:** `PhysureError::ParseError` for text that is not a formula or an equation, `PhysureError::DomainError` for a reaction that cannot be balanced or overflows. No new variant.

## File structure

| File | Responsibility |
|---|---|
| Create `physure-core/src/chemistry/mod.rs` | module list and re-exports |
| Create `physure-core/src/chemistry/elements.rs` | the 118 IUPAC weights, `atomic_weight(symbol)` |
| Create `physure-core/src/chemistry/formula.rs` | `Composition`, `parse_formula`, `subscripts_to_ascii` |
| Create `physure-core/src/chemistry/species.rs` | `Species`, molar mass, `mass_to_moles`, `moles_to_mass` |
| Create `physure-core/src/chemistry/reaction.rs` | exact RREF, `balance`, `Reaction`, equation parser |
| Create `physure-core/src/chemistry/thermo.rs` | `gas_constant`, `arrhenius`, `clausius_clapeyron`, `gibbs_free_energy` |
| Modify `physure-core/src/lib.rs` | `pub mod chemistry;` |
| Create `scripts/gen_chemistry_golden.py` | writes the two golden files from the Python implementation |
| Create `tests/golden/chemistry_molar_mass.tsv`, `tests/golden/chemistry_reactions.tsv` | frozen Python results |
| Create `physure-core/tests/chemistry_golden.rs` | holds the core to both files |
| Modify `CHANGELOG.md`, `structure.md`, `docs/chemistry_roadmap.md` | Task 8 |

Unit tests live in `#[cfg(test)] mod tests` at the bottom of each source file (the functions they reach are private).

All commands run from the worktree root `D:/Projects/physure/.claude/worktrees/chemistry-core` unless stated. The first `cargo` command compiles the whole workspace and is slow; later ones are incremental.

---

### Task 1: Module scaffold and the element table

**Files:**
- Create: `physure-core/src/chemistry/mod.rs`
- Create: `physure-core/src/chemistry/elements.rs`
- Modify: `physure-core/src/lib.rs` (add `pub mod chemistry;` after `pub mod error;`)

- [ ] **Step 1: Create the module file**

`physure-core/src/chemistry/mod.rs`:

```rust
//! Chemistry: formulas, molar masses, reaction balancing, kinetics and thermodynamics.
//!
//! The Python package `physure.ext.chemistry` began as the specification of this module; the golden
//! files in `tests/golden/chemistry_*.tsv` hold the two to the same answers.

pub mod elements;
```

Add `pub mod chemistry;` to `physure-core/src/lib.rs` directly below `pub mod error;`.

- [ ] **Step 2: Write `elements.rs` with a marker where the table goes**

`physure-core/src/chemistry/elements.rs`:

```rust
//! Standard atomic weights, IUPAC 2021: symbol, weight in g/mol, absolute uncertainty.
//!
//! An element with no stable isotope has the mass of its longest-lived known isotope and an
//! uncertainty of 0.0. The table was generated from `ATOMIC_WEIGHTS` in
//! `physure-python/physure/ext/chemistry/species.py`; from here on this is the one copy.
//! Sorted by symbol (byte order) so that `atomic_weight` can binary-search it.
//!
//! Reference: Meija, J. et al. (2016), Pure and Applied Chemistry 88(3), 265-291,
//! <https://doi.org/10.1515/pac-2015-0305>.

static ATOMIC_WEIGHTS: &[(&str, f64, f64)] = &[
    // @TABLE@
];

/// `(weight in g/mol, absolute uncertainty)` of an element, by its symbol (`"Fe"`).
pub fn atomic_weight(symbol: &str) -> Option<(f64, f64)> {
    ATOMIC_WEIGHTS
        .binary_search_by_key(&symbol, |&(s, _, _)| s)
        .ok()
        .map(|i| (ATOMIC_WEIGHTS[i].1, ATOMIC_WEIGHTS[i].2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_118_elements_in_strictly_increasing_order() {
        assert_eq!(ATOMIC_WEIGHTS.len(), 118);
        assert!(ATOMIC_WEIGHTS.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn known_weights() {
        assert_eq!(atomic_weight("H"), Some((1.008, 0.0002)));
        assert_eq!(atomic_weight("Fe"), Some((55.845, 0.002)));
        assert_eq!(atomic_weight("Og"), Some((294.0, 0.0)));
    }

    #[test]
    fn a_symbol_that_is_not_an_element_is_none() {
        assert_eq!(atomic_weight("Xx"), None);
        assert_eq!(atomic_weight("h"), None);
        assert_eq!(atomic_weight(""), None);
    }
}
```

- [ ] **Step 3: Fill the table from the Python source (no retyping)**

```bash
PY=/d/Projects/physure/physure-python/.venv/Scripts/python.exe
$PY - <<'EOF'
import ast, pathlib
src = pathlib.Path("physure-python/physure/ext/chemistry/species.py").read_text(encoding="utf-8")
node = next(n for n in ast.parse(src).body
            if isinstance(n, ast.AnnAssign) and n.target.id == "ATOMIC_WEIGHTS")
weights = ast.literal_eval(node.value)
assert len(weights) == 118
rows = "\n".join(f'    ("{s}", {m!r}, {u!r}),' for s, (m, u) in sorted(weights.items()))
target = pathlib.Path("physure-core/src/chemistry/elements.rs")
text = target.read_text(encoding="utf-8")
assert "    // @TABLE@" in text
target.write_text(text.replace("    // @TABLE@", rows), encoding="utf-8")
EOF
grep -c '^    ("' physure-core/src/chemistry/elements.rs
```

Expected: `118`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p physure --lib chemistry::elements`
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add physure-core/src/chemistry physure-core/src/lib.rs
git commit -m "feat(core): the IUPAC atomic weights as a sorted table

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: The formula parser

**Files:**
- Create: `physure-core/src/chemistry/formula.rs`
- Modify: `physure-core/src/chemistry/mod.rs`

- [ ] **Step 1: Register the module**

Append to `mod.rs`:

```rust
pub mod formula;

pub use formula::{parse_formula, Composition};
```

- [ ] **Step 2: Write the tests first** (create `formula.rs` containing only this, so it fails to compile and then fails)

`physure-core/src/chemistry/formula.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn comp(pairs: &[(&str, u32)]) -> Composition {
        pairs.iter().map(|&(e, n)| (e.to_string(), n)).collect()
    }

    #[test]
    fn plain_formulas() {
        assert_eq!(parse_formula("H2O").unwrap(), comp(&[("H", 2), ("O", 1)]));
        assert_eq!(parse_formula("NaCl").unwrap(), comp(&[("Cl", 1), ("Na", 1)]));
        assert_eq!(parse_formula("C6H12O6").unwrap(), comp(&[("C", 6), ("H", 12), ("O", 6)]));
    }

    #[test]
    fn groups_multiply_and_nest() {
        assert_eq!(parse_formula("Ca(NO3)2").unwrap(), comp(&[("Ca", 1), ("N", 2), ("O", 6)]));
        assert_eq!(
            parse_formula("Mg3(Fe(CN)6)2").unwrap(),
            comp(&[("C", 12), ("Fe", 2), ("Mg", 3), ("N", 12)])
        );
    }

    #[test]
    fn unicode_subscripts_are_digits() {
        assert_eq!(parse_formula("H₂O").unwrap(), parse_formula("H2O").unwrap());
        assert_eq!(parse_formula("C₆H₁₂O₆").unwrap(), parse_formula("C6H12O6").unwrap());
    }

    #[test]
    fn a_hydrate_adds_its_water_times_the_number_in_front() {
        let expected = comp(&[("Cu", 1), ("H", 10), ("O", 9), ("S", 1)]);
        assert_eq!(parse_formula("CuSO4·5H2O").unwrap(), expected);
        assert_eq!(parse_formula("CuSO4*5H2O").unwrap(), expected);
        // no number in front means one
        assert_eq!(parse_formula("CuSO4·H2O").unwrap(), comp(&[("Cu", 1), ("H", 2), ("O", 5), ("S", 1)]));
    }

    fn message(formula: &str) -> String {
        parse_formula(formula).unwrap_err().to_string()
    }

    #[test]
    fn what_is_not_a_formula_is_refused() {
        for bad in ["", "H2O)", "(H2O", "()", "Xx3", "h2o", "H2 O", "2H2O", "H2O·", "·H2O"] {
            assert!(parse_formula(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn a_count_of_zero_is_refused_not_kept_as_an_empty_species() {
        assert!(message("H0").contains("zero"));
        assert!(message("(OH)0").contains("zero"));
    }

    #[test]
    fn charges_and_aggregation_states_are_named_not_dropped() {
        assert!(message("SO4^2-").contains("'^'"));
        assert!(message("NaCl(aq)").contains("(aq)"));
        assert!(message("H2O(l)").contains("(l)"));
    }

    #[test]
    fn an_unknown_element_is_named() {
        assert!(message("Xx3").contains("Xx"));
    }

    #[test]
    fn a_count_that_does_not_fit_is_an_error_not_a_wrap() {
        assert!(parse_formula("H4294967295H").is_err());
        assert!(parse_formula("(H4294967295)2").is_err());
    }
}
```

Run: `cargo test -p physure --lib chemistry::formula`
Expected: FAIL to compile (`parse_formula` not found).

- [ ] **Step 3: Write the implementation** (put it above the `tests` module)

```rust
//! Chemical formulas: `"Ca(NO3)2"` is `{Ca: 1, N: 2, O: 6}`.
//!
//! Ionic charges (`SO4^2-`) and aggregation states (`(aq)`) are not formulas of a compound but of
//! a species in a medium; they are refused by name, because dropping them would give a wrong
//! composition and so a wrong molar mass.

use std::collections::BTreeMap;

use crate::chemistry::elements::atomic_weight;
use crate::error::{PhysureError, PhysureResult};

/// Element symbol to the number of its atoms. Ordered by symbol.
pub type Composition = BTreeMap<String, u32>;

fn bad(formula: &str, why: &str) -> PhysureError {
    PhysureError::ParseError(format!("invalid formula {formula:?}: {why}"))
}

/// Unicode subscript digits (`₀`..`₉`) as ASCII digits.
pub fn subscripts_to_ascii(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '₀'..='₉' => char::from(b'0' + (c as u32 - '₀' as u32) as u8),
            other => other,
        })
        .collect()
}

/// The number at `at` in `bytes` and the position after it; `(1, at)` when there is none.
fn read_count(bytes: &[u8], at: usize, formula: &str) -> PhysureResult<(u32, usize)> {
    let digits = bytes[at..].iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return Ok((1, at));
    }
    let text = std::str::from_utf8(&bytes[at..at + digits]).expect("ASCII digits are UTF-8");
    let count: u32 = text.parse().map_err(|_| bad(formula, "a count is too large"))?;
    if count == 0 {
        return Err(bad(formula, "a count of zero"));
    }
    Ok((count, at + digits))
}

fn add(into: &mut Composition, element: String, n: u32, formula: &str) -> PhysureResult<()> {
    let slot = into.entry(element).or_insert(0);
    *slot = slot.checked_add(n).ok_or_else(|| bad(formula, "a count is too large"))?;
    Ok(())
}

/// One part of a formula, without a hydrate dot: element symbols, counts and parentheses.
fn parse_group(part: &str, formula: &str) -> PhysureResult<Composition> {
    if part.is_empty() {
        return Err(bad(formula, "nothing between the separators"));
    }
    if let Some(c) = part.chars().find(|c| !c.is_ascii()) {
        return Err(bad(formula, &format!("unsupported character {c:?}")));
    }
    let bytes = part.as_bytes();
    let mut stack = vec![Composition::new()];
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => {
                stack.push(Composition::new());
                i += 1;
            }
            b')' => {
                let (times, next) = read_count(bytes, i + 1, formula)?;
                i = next;
                if stack.len() < 2 {
                    return Err(bad(formula, "unbalanced parentheses"));
                }
                let inner = stack.pop().expect("two or more groups");
                if inner.is_empty() {
                    return Err(bad(formula, "empty parentheses"));
                }
                let outer = stack.last_mut().expect("the outer group");
                for (element, n) in inner {
                    let n = n.checked_mul(times).ok_or_else(|| bad(formula, "a count is too large"))?;
                    add(outer, element, n, formula)?;
                }
            }
            b'A'..=b'Z' => {
                let start = i;
                i += 1;
                if i < bytes.len() && bytes[i].is_ascii_lowercase() {
                    i += 1;
                }
                let symbol = &part[start..i];
                if atomic_weight(symbol).is_none() {
                    return Err(bad(formula, &format!("unknown element {symbol:?}")));
                }
                let (n, next) = read_count(bytes, i, formula)?;
                i = next;
                add(stack.last_mut().expect("the outer group"), symbol.to_string(), n, formula)?;
            }
            other => {
                return Err(bad(formula, &format!("unsupported character '{}'", other as char)));
            }
        }
    }
    if stack.len() != 1 {
        return Err(bad(formula, "unbalanced parentheses"));
    }
    Ok(stack.pop().expect("the outer group"))
}

/// The atoms of a formula by element. `·` or `*` separates a hydrate, with an optional number in
/// front of the part after it (`CuSO4·5H2O`).
pub fn parse_formula(formula: &str) -> PhysureResult<Composition> {
    let text = subscripts_to_ascii(formula);
    for state in ["(aq)", "(s)", "(l)", "(g)"] {
        if text.contains(state) {
            return Err(bad(formula, &format!("the aggregation state {state} is not supported")));
        }
    }
    let mut total = Composition::new();
    for (index, part) in text.split(['·', '*']).enumerate() {
        let digits = part.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && index == 0 {
            return Err(bad(formula, "a formula cannot start with a number"));
        }
        let (times, _) = read_count(part.as_bytes(), 0, formula)?;
        for (element, n) in parse_group(&part[digits..], formula)? {
            let n = n.checked_mul(times).ok_or_else(|| bad(formula, "a count is too large"))?;
            add(&mut total, element, n, formula)?;
        }
    }
    Ok(total)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p physure --lib chemistry::formula`
Expected: 9 tests pass.

- [ ] **Step 5: Clippy**

Run: `cargo clippy -p physure --lib -- -D warnings`
Expected: no warnings from `chemistry/`. (If clippy reports pre-existing warnings elsewhere, report them; do not fix them here.)

- [ ] **Step 6: Commit**

```bash
git add physure-core/src/chemistry
git commit -m "feat(core): chemical formula parser with hydrates, Unicode subscripts and named refusals

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Species and molar mass

**Files:**
- Create: `physure-core/src/chemistry/species.rs`
- Modify: `physure-core/src/chemistry/mod.rs`

- [ ] **Step 1: Register the module**

Append to `mod.rs`:

```rust
pub mod species;

pub use species::Species;
```

- [ ] **Step 2: Write the failing tests** (`species.rs` contains only this)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1.0)
    }

    #[test]
    fn water_is_18_015_with_its_atoms_uncertainties_in_quadrature() {
        let (mass, std) = Species::new("H2O").unwrap().molar_mass_parts();
        assert!(close(mass, 2.0 * 1.008 + 15.999));
        // sqrt((2 * 0.0002)^2 + 0.001^2)
        assert!(close(std, (0.0004_f64.powi(2) + 0.001_f64.powi(2)).sqrt()));
    }

    #[test]
    fn calcium_nitrate_and_copper_sulfate_pentahydrate() {
        let (mass, _) = Species::new("Ca(NO3)2").unwrap().molar_mass_parts();
        assert!(close(mass, 40.078 + 2.0 * 14.007 + 6.0 * 15.999));
        let (mass, _) = Species::new("CuSO4·5H2O").unwrap().molar_mass_parts();
        assert!(close(mass, 63.546 + 32.06 + 9.0 * 15.999 + 10.0 * 1.008));
    }

    #[test]
    fn the_molar_mass_is_a_quantity_in_g_per_mol() {
        let q = Species::new("CO2").unwrap().molar_mass().unwrap();
        assert!(close(q.value.mean(), 12.011 + 2.0 * 15.999));
        assert!(close(q.value.std_dev(), (0.001_f64.powi(2) + 0.002_f64.powi(2)).sqrt()));
        assert!(close(q.to("kg/mol").unwrap().value.mean(), 0.044009));
    }

    #[test]
    fn a_species_keeps_the_formula_it_was_given() {
        let s = Species::new("H₂O").unwrap();
        assert_eq!(s.formula, "H₂O");
        assert_eq!(s.composition()["H"], 2);
    }

    #[test]
    fn a_bad_formula_is_an_error() {
        assert!(Species::new("H2O)").is_err());
    }
}
```

Run: `cargo test -p physure --lib chemistry::species`
Expected: FAIL to compile.

- [ ] **Step 3: Implement** (above the tests)

```rust
//! A chemical species and its molar mass.
//!
//! The uncertainty of the molar mass combines the tabulated uncertainty of each element in
//! quadrature, weighted by the atom count (JCGM 100:2008 §5.1; the atoms are independent).

use crate::chemistry::elements::atomic_weight;
use crate::chemistry::formula::{parse_formula, Composition};
use crate::error::PhysureResult;
use crate::quantity::Quantity;

/// A compound or an element, read from a formula.
#[derive(Debug, Clone, PartialEq)]
pub struct Species {
    /// The formula as it was written (`"H₂O"` stays `"H₂O"`).
    pub formula: String,
    composition: Composition,
}

impl Species {
    pub fn new(formula: &str) -> PhysureResult<Self> {
        Ok(Species { formula: formula.to_string(), composition: parse_formula(formula)? })
    }

    /// Atoms by element symbol.
    pub fn composition(&self) -> &Composition {
        &self.composition
    }

    /// `(molar mass in g/mol, its standard uncertainty)`.
    pub fn molar_mass_parts(&self) -> (f64, f64) {
        let (mut mass, mut variance) = (0.0, 0.0);
        for (element, &count) in &self.composition {
            // the composition is only built by `parse_formula`, which checks every symbol
            let (weight, std) = atomic_weight(element).expect("a parsed element has a weight");
            mass += f64::from(count) * weight;
            variance += (f64::from(count) * std).powi(2);
        }
        (mass, variance.sqrt())
    }

    /// The molar mass as a quantity in g/mol, with its uncertainty.
    pub fn molar_mass(&self) -> PhysureResult<Quantity> {
        let (mass, std) = self.molar_mass_parts();
        Quantity::with_uncertainty(mass, std, "g/mol")
    }
}
```

- [ ] **Step 4: Run, then commit**

Run: `cargo test -p physure --lib chemistry::species`
Expected: 5 tests pass.

```bash
git add physure-core/src/chemistry
git commit -m "feat(core): Species and its molar mass with the atoms' uncertainties in quadrature

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Mass to moles and moles to mass

**Files:**
- Modify: `physure-core/src/chemistry/species.rs`
- Modify: `physure-core/src/chemistry/mod.rs` (extend the `species` re-export)

The Python version converts through an equivalency and then rebuilds the uncertainty as `|n| * sqrt(rel(m)² + rel(M)²)`. The core gets the same number by dividing the two independent quantities: the mass measured and the molar mass, each its own source.

- [ ] **Step 1: Add the failing tests** to the `tests` module of `species.rs`

```rust
    #[test]
    fn fifty_grams_of_co2_with_both_uncertainties() {
        let co2 = Species::new("CO2").unwrap();
        let mass = Quantity::with_uncertainty(50.0, 0.1, "g").unwrap();
        let n = mass_to_moles(&mass, &co2).unwrap();
        let m_co2 = 12.011 + 2.0 * 15.999;
        let u_m = (0.001_f64.powi(2) + 0.002_f64.powi(2)).sqrt();
        let expected = 50.0 / m_co2;
        let rel = ((0.1_f64 / 50.0).powi(2) + (u_m / m_co2).powi(2)).sqrt();
        assert!(close(n.value.mean(), expected));
        assert!(close(n.value.std_dev(), expected * rel));
        assert!(close(n.to("mol").unwrap().value.mean(), expected));
    }

    #[test]
    fn other_mass_units_are_converted() {
        let co2 = Species::new("CO2").unwrap();
        let kilo = Quantity::new(0.05, "kg").unwrap();
        let n = mass_to_moles(&kilo, &co2).unwrap();
        assert!(close(n.value.mean(), 50.0 / (12.011 + 2.0 * 15.999)));
    }

    #[test]
    fn a_quantity_that_is_not_a_mass_is_refused() {
        let co2 = Species::new("CO2").unwrap();
        assert!(mass_to_moles(&Quantity::new(1.0, "s").unwrap(), &co2).is_err());
        assert!(moles_to_mass(&Quantity::new(1.0, "g").unwrap(), &co2).is_err());
    }

    #[test]
    fn moles_and_back() {
        let water = Species::new("H2O").unwrap();
        let n = Quantity::with_uncertainty(2.0, 0.01, "mol").unwrap();
        let mass = moles_to_mass(&n, &water).unwrap();
        let m = 2.0 * 1.008 + 15.999;
        assert!(close(mass.value.mean(), 2.0 * m));
        assert!(close(mass.to("kg").unwrap().value.mean(), 2.0 * m / 1000.0));
        let (_, u_m) = water.molar_mass_parts();
        let rel = ((0.01_f64 / 2.0).powi(2) + (u_m / m).powi(2)).sqrt();
        assert!(close(mass.value.std_dev(), 2.0 * m * rel));
    }
```

Run: `cargo test -p physure --lib chemistry::species`
Expected: FAIL to compile (`mass_to_moles` not found).

- [ ] **Step 2: Implement** (below `impl Species`)

```rust
/// A mass as an amount of substance. The molar mass is measured too, and its uncertainty is in
/// the result. A quantity that is not a mass is a unit mismatch.
pub fn mass_to_moles(mass: &Quantity, species: &Species) -> PhysureResult<Quantity> {
    mass.to("g")?.div(&species.molar_mass()?)?.to("mol")
}

/// An amount of substance as a mass in grams, with the uncertainty of the molar mass in it.
pub fn moles_to_mass(moles: &Quantity, species: &Species) -> PhysureResult<Quantity> {
    moles.to("mol")?.mul(&species.molar_mass()?)?.to("g")
}
```

In `mod.rs` change the re-export to `pub use species::{mass_to_moles, moles_to_mass, Species};`.

- [ ] **Step 3: Run**

Run: `cargo test -p physure --lib chemistry::species`
Expected: 9 tests pass. If `fifty_grams_of_co2_with_both_uncertainties` fails on the uncertainty only, the propagation mode is not Gaussian-with-lineage in this build: print `n.value.std_dev()` and the expected value, and report before changing the formula.

- [ ] **Step 4: Commit**

```bash
git add physure-core/src/chemistry
git commit -m "feat(core): mass_to_moles and moles_to_mass carry the molar mass uncertainty

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Reaction balancing

**Files:**
- Create: `physure-core/src/chemistry/reaction.rs`
- Modify: `physure-core/src/chemistry/mod.rs`

- [ ] **Step 1: Register the module**

Append to `mod.rs`:

```rust
pub mod reaction;

pub use reaction::Reaction;
```

- [ ] **Step 2: Write the failing tests** (`reaction.rs` contains only this)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn coeffs(equation: &str) -> (Vec<i64>, Vec<i64>) {
        let r = Reaction::from_string(equation).unwrap();
        (r.reactant_coeffs, r.product_coeffs)
    }

    #[test]
    fn water() {
        assert_eq!(coeffs("H2 + O2 -> H2O"), (vec![2, 1], vec![2]));
    }

    #[test]
    fn octane_combustion_is_exact() {
        assert_eq!(coeffs("C8H18 + O2 -> CO2 + H2O"), (vec![2, 25], vec![16, 18]));
    }

    #[test]
    fn iron_ore_with_carbon() {
        assert_eq!(coeffs("Fe2O3 + C -> Fe + CO2"), (vec![2, 3], vec![4, 3]));
    }

    #[test]
    fn coefficients_already_written_are_rederived() {
        assert_eq!(coeffs("2 H2 + 5 O2 -> 9 H2O"), (vec![2, 1], vec![2]));
    }

    #[test]
    fn a_hydrate_balances() {
        assert_eq!(coeffs("CuSO4·5H2O -> CuSO4 + H2O"), (vec![1], vec![1, 5]));
    }

    #[test]
    fn every_separator_is_read_and_the_reversible_ones_are_flagged() {
        for (arrow, reversible) in [("->", false), ("=", false), ("→", false), ("<=>", true), ("⇌", true)] {
            let r = Reaction::from_string(&format!("N2 + 3 H2 {arrow} 2 NH3")).unwrap();
            assert_eq!(r.reversible, reversible, "{arrow}");
            assert_eq!((r.reactant_coeffs.clone(), r.product_coeffs.clone()), (vec![1, 3], vec![2]), "{arrow}");
        }
    }

    #[test]
    fn it_prints_the_balanced_equation() {
        assert_eq!(Reaction::from_string("H2 + O2 -> H2O").unwrap().to_string(), "2 H2 + O2 -> 2 H2O");
        assert_eq!(Reaction::from_string("N2 + H2 <=> NH3").unwrap().to_string(), "N2 + 3 H2 ⇌ 2 NH3");
    }

    fn message(equation: &str) -> String {
        Reaction::from_string(equation).unwrap_err().to_string()
    }

    #[test]
    fn an_underdetermined_system_is_refused_not_solved_arbitrarily() {
        assert!(message("H2 + O2 -> H2O + H2O2").contains("one degree of freedom"));
    }

    #[test]
    fn a_system_with_no_solution_is_refused() {
        assert!(message("H2 -> O2").contains("one degree of freedom"));
    }

    #[test]
    fn a_solution_that_is_not_positive_is_refused() {
        assert!(message("H2 + H2O -> O2").contains("positive"));
    }

    #[test]
    fn malformed_equations_are_refused() {
        for bad in ["H2 + O2", "H2 -> O2 -> H2O", "H2 + -> H2O", "-> H2O", "H2 -> ", "2 -> H2", "H2 + O 2 -> H2O"] {
            assert!(Reaction::from_string(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn a_bad_formula_inside_an_equation_names_the_formula() {
        assert!(message("H2 + Xx -> H2").contains("Xx"));
    }

    #[test]
    fn overflowing_arithmetic_is_an_error_not_a_wrap() {
        assert!(lcm_checked(i64::MAX, 2).is_err());
        assert_eq!(lcm_checked(4, 6).unwrap(), 12);
        assert_eq!(gcd(12, 18), 6);
    }
}
```

Run: `cargo test -p physure --lib chemistry::reaction`
Expected: FAIL to compile.

- [ ] **Step 3: Implement** (above the tests)

```rust
//! Reactions: parsing an equation and finding the smallest whole stoichiometric coefficients.
//!
//! Balancing solves the element-conservation null space by Gauss-Jordan elimination over exact
//! rationals. Floats mis-round large stoichiometries (octane combustion), and every operation
//! is checked: an overflow is an error, never a wrapped number.
//!
//! Reference: Smith, W.R. & Missen, R.W. (1999), Chemical Reaction Stoichiometry (CRS): A Tutorial,
//! Chemical Engineering Education 33(1).

use std::collections::BTreeSet;
use std::fmt;

use num_rational::Rational64;
use num_traits::{CheckedDiv, CheckedMul, CheckedSub, Zero};

use crate::chemistry::formula::subscripts_to_ascii;
use crate::chemistry::species::Species;
use crate::error::{PhysureError, PhysureResult};

const ARROWS: [&str; 5] = ["<=>", "->", "=", "→", "⇌"];

type Row = Vec<Rational64>;

fn overflow() -> PhysureError {
    PhysureError::DomainError("the stoichiometric coefficients do not fit in 64-bit integers".into())
}

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn lcm_checked(a: i64, b: i64) -> PhysureResult<i64> {
    (a / gcd(a, b)).checked_mul(b).ok_or_else(overflow)
}

/// Reduced row-echelon form of `m` and the columns that hold a pivot.
fn rref(mut m: Vec<Row>) -> PhysureResult<(Vec<Row>, Vec<usize>)> {
    let rows = m.len();
    let cols = m.first().map_or(0, Vec::len);
    let mut pivot_row = 0;
    let mut pivots = Vec::new();
    for col in 0..cols {
        let Some(found) = (pivot_row..rows).find(|&r| !m[r][col].is_zero()) else {
            continue;
        };
        m.swap(pivot_row, found);
        let pivot_value = m[pivot_row][col];
        for v in m[pivot_row].iter_mut() {
            *v = v.checked_div(&pivot_value).ok_or_else(overflow)?;
        }
        let pivot_values = m[pivot_row].clone();
        for (r, row) in m.iter_mut().enumerate() {
            if r == pivot_row || row[col].is_zero() {
                continue;
            }
            let factor = row[col];
            for (cell, p) in row.iter_mut().zip(&pivot_values) {
                let scaled = factor.checked_mul(p).ok_or_else(overflow)?;
                *cell = cell.checked_sub(&scaled).ok_or_else(overflow)?;
            }
        }
        pivots.push(col);
        pivot_row += 1;
        if pivot_row == rows {
            break;
        }
    }
    Ok((m, pivots))
}

/// The smallest positive whole coefficients that conserve every element, as
/// `(reactants, products)`.
pub fn balance(reactants: &[Species], products: &[Species]) -> PhysureResult<(Vec<i64>, Vec<i64>)> {
    let species: Vec<&Species> = reactants.iter().chain(products).collect();
    let n_react = reactants.len();
    let elements: BTreeSet<&str> =
        species.iter().flat_map(|s| s.composition().keys().map(String::as_str)).collect();
    let matrix: Vec<Row> = elements
        .iter()
        .map(|element| {
            species
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let count = i64::from(s.composition().get(*element).copied().unwrap_or(0));
                    Rational64::from_integer(if i < n_react { count } else { -count })
                })
                .collect()
        })
        .collect();

    let (reduced, pivots) = rref(matrix)?;
    let free: Vec<usize> = (0..species.len()).filter(|c| !pivots.contains(c)).collect();
    if free.len() != 1 {
        return Err(PhysureError::DomainError(
            "cannot uniquely balance this reaction (expected exactly one degree of freedom)".into(),
        ));
    }
    let free_col = free[0];

    let zero = Rational64::from_integer(0);
    let mut coeffs = vec![zero; species.len()];
    coeffs[free_col] = Rational64::from_integer(1);
    for (row, &col) in pivots.iter().enumerate() {
        coeffs[col] = -reduced[row][free_col];
    }
    if coeffs.iter().any(|c| *c < zero) {
        for c in &mut coeffs {
            *c = -*c;
        }
    }
    if coeffs.iter().any(|c| *c <= zero) {
        return Err(PhysureError::DomainError(
            "cannot balance this reaction with positive integer coefficients".into(),
        ));
    }

    let mut denominators = 1_i64;
    for c in &coeffs {
        denominators = lcm_checked(denominators, *c.denom())?;
    }
    let mut ints = Vec::with_capacity(coeffs.len());
    for c in &coeffs {
        ints.push(c.numer().checked_mul(denominators / c.denom()).ok_or_else(overflow)?);
    }
    let common = ints.iter().fold(0, |g, &v| gcd(g, v));
    if common > 1 {
        for v in &mut ints {
            *v /= common;
        }
    }
    let products_ints = ints.split_off(n_react);
    Ok((ints, products_ints))
}

/// A balanced chemical reaction.
#[derive(Debug, Clone, PartialEq)]
pub struct Reaction {
    pub reactants: Vec<Species>,
    pub products: Vec<Species>,
    pub reactant_coeffs: Vec<i64>,
    pub product_coeffs: Vec<i64>,
    pub reversible: bool,
}

impl Reaction {
    pub fn new(reactants: Vec<Species>, products: Vec<Species>, reversible: bool) -> PhysureResult<Self> {
        let (reactant_coeffs, product_coeffs) = balance(&reactants, &products)?;
        Ok(Reaction { reactants, products, reactant_coeffs, product_coeffs, reversible })
    }

    /// Reads `"2 H2 + O2 -> 2 H2O"`. Coefficients that are written are re-derived. The
    /// separators are `->`, `=`, `→` (irreversible) and `<=>`, `⇌` (reversible).
    pub fn from_string(equation: &str) -> PhysureResult<Self> {
        let invalid = || PhysureError::ParseError(format!("invalid reaction equation: {equation:?}"));
        let (at, arrow) = find_arrow(equation, 0).ok_or_else(invalid)?;
        let end = at + arrow.len();
        if find_arrow(equation, end).is_some() {
            return Err(invalid());
        }
        let side = |text: &str| text.split('+').map(parse_term).collect::<PhysureResult<Vec<_>>>();
        let reversible = arrow == "<=>" || arrow == "⇌";
        Reaction::new(side(&equation[..at])?, side(&equation[end..])?, reversible)
    }
}

/// The leftmost separator at or after byte `from`, with the longer ones tried first.
fn find_arrow(text: &str, from: usize) -> Option<(usize, &'static str)> {
    text[from..].char_indices().find_map(|(i, _)| {
        ARROWS.iter().find(|arrow| text[from + i..].starts_with(**arrow)).map(|arrow| (from + i, *arrow))
    })
}

/// One term, `"2 H2O"` or `"H2O"`: an optional coefficient (ignored), then a formula.
fn parse_term(term: &str) -> PhysureResult<Species> {
    let text = subscripts_to_ascii(term.trim());
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let formula = text[digits..].trim_start();
    if formula.is_empty() {
        return Err(PhysureError::ParseError(format!("invalid reaction term: {term:?}")));
    }
    Species::new(formula)
}

impl fmt::Display for Reaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn side(f: &mut fmt::Formatter<'_>, species: &[Species], coeffs: &[i64]) -> fmt::Result {
            for (i, (s, c)) in species.iter().zip(coeffs).enumerate() {
                if i > 0 {
                    write!(f, " + ")?;
                }
                if *c != 1 {
                    write!(f, "{c} ")?;
                }
                write!(f, "{}", s.formula)?;
            }
            Ok(())
        }
        side(f, &self.reactants, &self.reactant_coeffs)?;
        write!(f, " {} ", if self.reversible { "⇌" } else { "->" })?;
        side(f, &self.products, &self.product_coeffs)
    }
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p physure --lib chemistry::reaction`
Expected: 13 tests pass.

Likely snags, and what to do:
- `CheckedDiv` / `CheckedSub` / `CheckedMul` "not implemented for Ratio<i64>": the method is on `Ratio<T>` in `num-rational` 0.4 with `T: Clone + Integer + CheckedMul`; run `cargo tree -p physure -i num-rational` and report the version before changing the approach.
- `H2 + O 2 -> H2O` must be refused (the space inside `O 2`): `Species::new("O 2")` rejects the space as an unsupported character. If it does not, fix `parse_term`, not the test.

- [ ] **Step 5: Clippy, then commit**

Run: `cargo clippy -p physure --lib -- -D warnings`
Expected: nothing from `chemistry/`.

```bash
git add physure-core/src/chemistry
git commit -m "feat(core): exact, checked reaction balancing and the equation parser

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Thermodynamics and kinetics

**Files:**
- Create: `physure-core/src/chemistry/thermo.rs`
- Modify: `physure-core/src/chemistry/mod.rs`

- [ ] **Step 1: Confirm what the catalog gives**

Run: `grep -n "pub struct ConstantMetadata" -A6 physure-core/src/units/registry.rs`
Expected: a struct whose `value: String` field is `pub`. If a field is private, stop and report — the lookup in `gas_constant` below needs `value`.

- [ ] **Step 2: Register the module**

Append to `mod.rs`:

```rust
pub mod thermo;

pub use thermo::{arrhenius, clausius_clapeyron, gas_constant, gibbs_free_energy};
```

- [ ] **Step 3: Write the failing tests** (`thermo.rs` contains only this)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1e-300)
    }

    const R: f64 = 8.31446261815324;

    #[test]
    fn the_gas_constant_is_the_catalogs() {
        let r = gas_constant().unwrap();
        assert!(close(r.value.mean(), R));
        assert_eq!(r.value.std_dev(), 0.0);
        assert!(close(r.to("J/(mol*K)").unwrap().value.mean(), R));
    }

    #[test]
    fn arrhenius_against_the_closed_form() {
        let a = Quantity::new(1e13, "s^-1").unwrap();
        let ea = Quantity::new(75.0, "kJ/mol").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        let k = arrhenius(&a, &ea, &t, None).unwrap();
        let expected = 1e13 * (-75_000.0 / (R * 298.15)).exp();
        assert!(close(k.to("s^-1").unwrap().value.mean(), expected));
        assert!((expected - 0.73).abs() < 0.005, "k is 0.73 s^-1, got {expected}");
    }

    #[test]
    fn arrhenius_keeps_the_uncertainty_of_the_activation_energy() {
        let a = Quantity::new(1e13, "s^-1").unwrap();
        let ea = Quantity::with_uncertainty(75.0, 1.0, "kJ/mol").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        let k = arrhenius(&a, &ea, &t, None).unwrap();
        // dk/k = (dEa / (R T)); the uncertainty is not dropped
        let rel = 1000.0 / (R * 298.15);
        assert!(close(k.value.std_dev() / k.value.mean(), rel));
    }

    #[test]
    fn arrhenius_with_a_given_gas_constant() {
        let a = Quantity::new(1.0, "s^-1").unwrap();
        let ea = Quantity::new(1000.0, "J/mol").unwrap();
        let t = Quantity::new(300.0, "K").unwrap();
        let r = Quantity::new(8.0, "J/(mol*K)").unwrap();
        let k = arrhenius(&a, &ea, &t, Some(&r)).unwrap();
        assert!(close(k.value.mean(), (-1000.0_f64 / (8.0 * 300.0)).exp()));
    }

    #[test]
    fn an_activation_energy_in_kelvin_is_a_dimension_error() {
        let a = Quantity::new(1e13, "s^-1").unwrap();
        let ea = Quantity::new(75.0, "K").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        assert!(arrhenius(&a, &ea, &t, None).is_err());
    }

    #[test]
    fn gibbs_free_energy_of_water_formation_is_minus_237_1() {
        let dh = Quantity::new(-285.8, "kJ/mol").unwrap();
        let ds = Quantity::new(-163.2, "J/(mol*K)").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        let g = gibbs_free_energy(&dh, &t, &ds).unwrap();
        let expected = -285.8 + 298.15 * 163.2 / 1000.0;
        assert!(close(g.to("kJ/mol").unwrap().value.mean(), expected));
        assert!((expected + 237.1).abs() < 0.05);
    }

    #[test]
    fn gibbs_refuses_an_entropy_that_is_an_energy() {
        let dh = Quantity::new(-285.8, "kJ/mol").unwrap();
        let t = Quantity::new(298.15, "K").unwrap();
        assert!(gibbs_free_energy(&dh, &t, &dh).is_err());
    }

    #[test]
    fn clausius_clapeyron_for_water_cooling_to_90_c() {
        let dh = Quantity::new(40.7, "kJ/mol").unwrap();
        let t1 = Quantity::new(373.15, "K").unwrap();
        let t2 = Quantity::new(363.15, "K").unwrap();
        let p1 = Quantity::new(101.325, "kPa").unwrap();
        let p2 = clausius_clapeyron(&dh, &t1, &p1, &t2, None).unwrap();
        let expected = 101.325 * (-(40_700.0 / R) * (1.0 / 363.15 - 1.0 / 373.15)).exp();
        assert!(close(p2.to("kPa").unwrap().value.mean(), expected));
        assert!(expected > 69.5 && expected < 71.5, "about 70 kPa, got {expected}");
    }

    #[test]
    fn clausius_clapeyron_round_trips() {
        let dh = Quantity::new(40.7, "kJ/mol").unwrap();
        let t1 = Quantity::new(373.15, "K").unwrap();
        let t2 = Quantity::new(363.15, "K").unwrap();
        let p1 = Quantity::new(101.325, "kPa").unwrap();
        let p2 = clausius_clapeyron(&dh, &t1, &p1, &t2, None).unwrap();
        let back = clausius_clapeyron(&dh, &t2, &p2, &t1, None).unwrap();
        assert!(close(back.to("kPa").unwrap().value.mean(), 101.325));
    }
}
```

Run: `cargo test -p physure --lib chemistry::thermo`
Expected: FAIL to compile.

- [ ] **Step 4: Implement** (above the tests)

```rust
//! Kinetics and thermodynamics: Arrhenius, Clausius-Clapeyron, Gibbs.
//!
//! They work on `Quantity`, so a dimension that does not fit is an error and the uncertainty of
//! every input is in the result. The gas constant is the catalog's `molar_gas_constant` (exact
//! since the 2019 SI), read from the catalog in scope, not written again here.
//!
//! References: Arrhenius, S. (1889), Z. Phys. Chem. 4, 226-248,
//! <https://doi.org/10.1515/zpch-1889-0416>; Gibbs, J. W. (1876), Trans. Conn. Acad. Arts Sci. 3,
//! 108-248, <https://doi.org/10.1080/14786447808639542>.

use crate::error::{PhysureError, PhysureResult};
use crate::quantity::Quantity;
use crate::units::conf::with_conf_registry;
use crate::units::{with_current_catalog, RationalUnit, UnitRegistry};

fn read_gas_constant(registry: &UnitRegistry) -> PhysureResult<Quantity> {
    let missing = || PhysureError::Generic("the catalog defines no molar_gas_constant".into());
    let value = &registry.constants_meta.get("molar_gas_constant").ok_or_else(missing)?.value;
    let (number, unit) = value.split_once(char::is_whitespace).ok_or_else(missing)?;
    let number: f64 = number
        .parse()
        .map_err(|_| PhysureError::Generic(format!("molar_gas_constant is not a number: {value:?}")))?;
    Quantity::new(number, unit.trim())
}

/// The molar gas constant `R` of the catalog in scope, in J/(mol·K).
pub fn gas_constant() -> PhysureResult<Quantity> {
    with_current_catalog(|scoped| match scoped {
        Some(registry) => read_gas_constant(registry),
        None => with_conf_registry(read_gas_constant),
    })
}

fn gas_or(r: Option<&Quantity>) -> PhysureResult<Quantity> {
    match r {
        Some(r) => Ok(r.clone()),
        None => gas_constant(),
    }
}

/// Arrhenius equation: `k = A * exp(-Ea / (R T))`. `r` is `R`, the catalog's when `None`.
pub fn arrhenius(a: &Quantity, ea: &Quantity, t: &Quantity, r: Option<&Quantity>) -> PhysureResult<Quantity> {
    let exponent = ea.div(&gas_or(r)?.mul(t)?)?.convert_to(&RationalUnit::dimensionless())?;
    a.div(&exponent.exp()?)
}

/// Clausius-Clapeyron relation: the pressure `P2` at `t2`, given `p1` at `t1` and the enthalpy of
/// vaporisation. `ln(P2/P1) = -(dH/R) (1/T2 - 1/T1)`.
pub fn clausius_clapeyron(
    delta_h_vap: &Quantity,
    t1: &Quantity,
    p1: &Quantity,
    t2: &Quantity,
    r: Option<&Quantity>,
) -> PhysureResult<Quantity> {
    let one = Quantity::new(1.0, "")?;
    let inverse = |t: &Quantity| one.div(&t.to("K")?);
    let exponent = delta_h_vap
        .div(&gas_or(r)?)?
        .mul(&inverse(t1)?.sub(&inverse(t2)?)?)?
        .convert_to(&RationalUnit::dimensionless())?;
    p1.mul(&exponent.exp()?)
}

/// Gibbs free energy: `dG = dH - T dS`.
pub fn gibbs_free_energy(delta_h: &Quantity, t: &Quantity, delta_s: &Quantity) -> PhysureResult<Quantity> {
    let entropy_term = t.mul(delta_s)?.convert_to(&delta_h.unit)?;
    delta_h.sub(&entropy_term)
}
```

- [ ] **Step 5: Run**

Run: `cargo test -p physure --lib chemistry::thermo`
Expected: 9 tests pass.

If a test fails, read the failure before changing code. The ones that can plausibly fail on API detail, not on logic: (a) `Quantity::new(1.0, "")` must give a dimensionless quantity (it does, `quantity.rs`: an empty unit is dimensionless); (b) `exp()` on a dimensionless `Quantity` — if it refuses because the unit is a *scaled* dimensionless one, convert with `convert_to(&RationalUnit::dimensionless())` (already done); (c) `sub` of two quantities whose units differ — the code above converts first so it does not depend on `sub` converting.

- [ ] **Step 6: Clippy, then commit**

Run: `cargo clippy -p physure --lib -- -D warnings`
Expected: nothing from `chemistry/`.

```bash
git add physure-core/src/chemistry
git commit -m "feat(core): arrhenius, clausius_clapeyron and gibbs_free_energy on quantities

arrhenius keeps the uncertainty of Ea, T and R; the Python version dropped it.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Hold the core to the Python results (golden files)

**Files:**
- Create: `scripts/gen_chemistry_golden.py`
- Create: `tests/golden/chemistry_molar_mass.tsv` (generated)
- Create: `tests/golden/chemistry_reactions.tsv` (generated)
- Create: `physure-core/tests/chemistry_golden.rs`

The golden files come from the *Python* implementation, as it is on this branch. After plan 3 deletes that implementation, they are the frozen record of what it answered.

- [ ] **Step 1: Write the generator**

`scripts/gen_chemistry_golden.py`:

```python
"""Writes tests/golden/chemistry_*.tsv from the Python chemistry package.

Run with an environment that has physure installed (the main checkout's venv works):

    physure-python/.venv/Scripts/python.exe scripts/gen_chemistry_golden.py

The files freeze what `physure.ext.chemistry` answered before the core took it over; the core's
`tests/chemistry_golden.rs` holds the Rust port to them. Formulas and equations on which the
two are meant to differ (hydrates, a zero count) are deliberately not in the lists.
"""

from __future__ import annotations

from pathlib import Path

from physure.ext.chemistry.reaction import Reaction
from physure.ext.chemistry.species import ATOMIC_WEIGHTS, Species

GOLDEN = Path(__file__).resolve().parent.parent / "tests" / "golden"

COMPOUNDS = [
    "H2O", "H₂O", "CO2", "C6H12O6", "C₆H₁₂O₆", "Ca(NO3)2", "NaCl", "Mg3(Fe(CN)6)2",
    "Al2(SO4)3", "C8H18", "NH4NO3", "Fe2O3", "(NH4)2SO4", "CH3(CH2)4CH3",
    "K4Fe(CN)6", "Ca3(PO4)2", "C12H22O11", "NaHCO3", "H2SO4", "UF6",
]

REACTIONS = [
    "H2 + O2 -> H2O",
    "C8H18 + O2 -> CO2 + H2O",
    "Fe2O3 + C -> Fe + CO2",
    "2 H2 + 5 O2 -> 9 H2O",
    "N2 + 3 H2 ⇌ 2 NH3",
    "N2 + H2 <=> NH3",
    "N2 + H2 = NH3",
    "N2 + H2 → NH3",
    "C₂H₆ + O₂ -> CO₂ + H₂O",
    "Al + HCl -> AlCl3 + H2",
    "KMnO4 + HCl -> KCl + MnCl2 + H2O + Cl2",
    "C6H12O6 + O2 -> CO2 + H2O",
    "Ca(OH)2 + H3PO4 -> Ca3(PO4)2 + H2O",
]

REFUSED = [
    "H2 + O2 -> H2O + H2O2",
    "H2 -> O2",
    "H2 + H2O -> O2",
    "H2 + O2",
    "H2 -> O2 -> H2O",
    "H2 + -> H2O",
    "H2 + Xx -> H2",
    "H2O) -> H2",
]


def molar_mass_rows() -> list[str]:
    rows = []
    for formula in [*sorted(ATOMIC_WEIGHTS), *COMPOUNDS]:
        q = Species(formula).molar_mass
        rows.append(f"{formula}\t{q.magnitude!r}\t{q.uncertainty!r}")
    return rows


def reaction_rows() -> list[str]:
    rows = []
    for equation in REACTIONS:
        r = Reaction.from_string(equation)
        react = ",".join(map(str, r.reactant_coeffs))
        prod = ",".join(map(str, r.product_coeffs))
        rows.append(f"{equation}\t{int(r.reversible)}\t{react}\t{prod}")
    for equation in REFUSED:
        try:
            Reaction.from_string(equation)
        except ValueError:
            rows.append(f"{equation}\tERROR\t\t")
        else:
            raise SystemExit(f"{equation!r} was expected to be refused")
    return rows


def write(name: str, header: str, rows: list[str]) -> None:
    text = f"# {header}\n" + "\n".join(rows) + "\n"
    (GOLDEN / name).write_text(text, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    write(
        "chemistry_molar_mass.tsv",
        "formula, molar mass (g/mol), standard uncertainty; from physure.ext.chemistry by scripts/gen_chemistry_golden.py",
        molar_mass_rows(),
    )
    write(
        "chemistry_reactions.tsv",
        "equation, reversible, reactant coefficients, product coefficients (ERROR when refused); by scripts/gen_chemistry_golden.py",
        reaction_rows(),
    )
```

- [ ] **Step 2: Generate**

```bash
PY=/d/Projects/physure/physure-python/.venv/Scripts/python.exe
$PY scripts/gen_chemistry_golden.py
wc -l tests/golden/chemistry_molar_mass.tsv tests/golden/chemistry_reactions.tsv
```

Expected: `139` lines in the molar-mass file (1 header + 118 + 20), `22` in the reactions file (1 header + 13 + 8). If a `REACTIONS` equation is refused by Python or a `REFUSED` one is accepted, the generator stops and names it: remove that line from its list rather than editing Python, and note it in the commit message.

- [ ] **Step 3: Write the test that holds the core to them**

`physure-core/tests/chemistry_golden.rs`:

```rust
//! The core against `tests/golden/chemistry_*.tsv`: what `physure.ext.chemistry` answered for the
//! molar masses of 118 elements and 20 compounds and for the coefficients of 13 reactions (and
//! which equations it refused), written by `scripts/gen_chemistry_golden.py`.

use physure_core::chemistry::{Reaction, Species};

const MOLAR_MASS: &str = include_str!("../../tests/golden/chemistry_molar_mass.tsv");
const REACTIONS: &str = include_str!("../../tests/golden/chemistry_reactions.tsv");

fn rows(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(|l| l.split('\t').collect())
}

fn numbers(list: &str) -> Vec<i64> {
    list.split(',').map(|n| n.parse().unwrap()).collect()
}

#[test]
fn every_molar_mass_is_reproduced() {
    let mut count = 0;
    for row in rows(MOLAR_MASS) {
        let (formula, mass, std): (&str, f64, f64) = (row[0], row[1].parse().unwrap(), row[2].parse().unwrap());
        let (got_mass, got_std) = Species::new(formula).unwrap().molar_mass_parts();
        // the atoms are summed in a different order, so the last bits may differ
        assert!((got_mass - mass).abs() <= 1e-12 * mass.abs().max(1.0), "{formula}: {got_mass} vs {mass}");
        assert!((got_std - std).abs() <= 1e-12 * std.abs().max(1e-12), "{formula}: {got_std} vs {std}");
        count += 1;
    }
    assert_eq!(count, 138);
}

#[test]
fn every_reaction_is_balanced_or_refused_as_in_python() {
    let mut count = 0;
    for row in rows(REACTIONS) {
        let equation = row[0];
        match row[1] {
            "ERROR" => assert!(Reaction::from_string(equation).is_err(), "{equation:?} should be refused"),
            reversible => {
                let r = Reaction::from_string(equation).unwrap_or_else(|e| panic!("{equation:?}: {e}"));
                assert_eq!(r.reversible, reversible == "1", "{equation}");
                assert_eq!(r.reactant_coeffs, numbers(row[2]), "{equation}");
                assert_eq!(r.product_coeffs, numbers(row[3]), "{equation}");
            }
        }
        count += 1;
    }
    assert_eq!(count, 21);
}
```

- [ ] **Step 4: Run**

Run: `cargo test -p physure --test chemistry_golden`
Expected: 2 tests pass.

Any disagreement is a finding, not a test to loosen. Stop and report the row, Python's value and Rust's. (A one-ulp difference in a mass is covered by the 1e-12 tolerance; anything bigger is a real difference.)

- [ ] **Step 5: Commit**

```bash
git add scripts/gen_chemistry_golden.py tests/golden/chemistry_molar_mass.tsv tests/golden/chemistry_reactions.tsv physure-core/tests/chemistry_golden.rs
git commit -m "test(core): hold the chemistry port to what the Python package answered

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Gates, changelog, structure, roadmap

**Files:**
- Modify: `CHANGELOG.md`, `structure.md`, `docs/chemistry_roadmap.md`

- [ ] **Step 1: The whole core crate, with doc tests**

Run: `cargo test -p physure`
Expected: everything passes (existing tests untouched).

Run: `cargo clippy -p physure --all-targets -- -D warnings`
Expected: nothing from `chemistry/` or `tests/chemistry_golden.rs`.

- [ ] **Step 2: Coverage of the new module**

Run (if `cargo-llvm-cov` is installed; `make sonar` uses it): `cargo llvm-cov -p physure --summary-only 2>&1 | grep -E "chemistry|TOTAL"`
Expected: each `chemistry/*.rs` file at ≥ 90 % lines (the "Physure Core Strict" gate). If a file is below, the uncovered lines are almost certainly error branches: add a test that reaches the branch rather than lowering the bar. If `cargo-llvm-cov` is not installed, say so in the report; do not skip silently.

- [ ] **Step 3: CHANGELOG**

Add at the top of `## [Unreleased]` → `### Added` in `CHANGELOG.md`:

```markdown
- **Chemistry in the Rust core** (`physure_core::chemistry`; plan 1 of the chemistry port in `docs/superpowers/plans/2026-11-07-chemistry-core.md`). `parse_formula` reads `Ca(NO3)2`, `H₂O` and hydrates (`CuSO4·5H2O`); `Species::molar_mass` carries the standard uncertainty of each atom in quadrature (the 118 IUPAC 2021 weights are a sorted table, no new dependency); `mass_to_moles` and `moles_to_mass` put the uncertainty of the molar mass into the result; `Reaction::from_string` balances an equation with exact, checked rational arithmetic (`->`, `=`, `→`, `<=>`, `⇌`) and refuses one that has not exactly one solution; `arrhenius`, `clausius_clapeyron` and `gibbs_free_energy` work on `Quantity`, with the gas constant read from the catalog. `tests/golden/chemistry_molar_mass.tsv` and `chemistry_reactions.tsv` hold the core to what the Python package answered. Python and PHS do not use it yet (the next two plans). Where the core differs from Python, the core is right: a hydrate dot is accepted (Python refused it), a count of zero is refused (Python made a species with no atoms), `arrhenius` keeps the uncertainty of `Ea`, `T` and `R` (Python dropped it), and an overflow in the balancing is an error.
```

- [ ] **Step 4: structure.md and the roadmap**

Run the `update-structure` skill (it audits `structure.md` against the tree) and let it add `physure-core/src/chemistry/` with its five files.

In `docs/chemistry_roadmap.md` and `ROADMAP.md` / `docs/ROADMAP.md`, mark only what this plan finished: the Phase 5 items "periodic table", "formula parser" and "Fraction RREF balancer" as done, leaving "PyO3 bindings" unchecked (plan 3). Do not move the subsystem percentage or the Phase 6 items. Keep the docs link policy: no relative links to source files, full `https://github.com/Alexisrx96/physure/blob/main/...` URLs only.

- [ ] **Step 5: Docs build, then commit**

Run (from `physure-python/` of the *main* checkout's venv so no rebuild is needed):

```bash
D:/Projects/physure/physure-python/.venv/Scripts/python.exe -m mkdocs build --strict -f physure-python/mkdocs.yml -d /tmp/mk_chem_check
```

Expected: `Documentation built`, no warnings about broken links.

```bash
git add CHANGELOG.md structure.md docs ROADMAP.md
git commit -m "docs: chemistry in the core — changelog, structure, roadmap

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-review

**Spec coverage** (`implementation_plan.md` Part A, A1–A3):
- A1 `elements.rs`, `formula.rs`, `species.rs`: Tasks 1–3. Sorted-slice binary search (no `phf`): Task 1. Hydrate dots and Unicode subscripts: Task 2. Quadrature uncertainty: Task 3. "Reject, don't guess" for charges and states: Task 2 test `charges_and_aggregation_states_are_named_not_dropped`. Tests named in the spec (`H2O`, `C6H12O6`, `Ca(NO3)2`, `CuSO4·5H2O`, malformed `H2O)`, `Xx3`, empty): Tasks 2–3 and the golden file.
- A2 `reaction.rs`: Task 5. Five separators with the reversible flag, exact arithmetic, overflow as an error, octane, `Fe2O3 + C` → 2/3/4/3, and the "one degree of freedom" failure: all in Task 5's tests.
- A3 `thermo.rs`: Task 6 (`arrhenius`, `gibbs_free_energy`, `clausius_clapeyron`; gas constant from the catalog; `Ea` in kelvin raises; −237.1 kJ/mol).
- `mass_to_moles` / `moles_to_mass` appear in A4's builtin list, not A1–A3; they are in Task 4 because the PHS builtin and the Python delegate both need them in the core.
- A4 and A5 are deliberately not here (see Scope).

**Placeholder scan:** none; every code step has the code. The only generated content is the element table (Task 1, by script from the Python source) and the golden files (Task 7, by script).

**Type consistency:** `Composition` (formula.rs) is used by `Species` (species.rs); `Species::composition()` is the accessor used in `reaction.rs`; `balance` returns `(Vec<i64>, Vec<i64>)` and `Reaction` stores `reactant_coeffs` / `product_coeffs` as `Vec<i64>`; the golden test reads those same fields; `arrhenius(a, ea, t, r: Option<&Quantity>)` has the same shape in the tests as in the implementation; `lcm_checked` and `gcd` are private and tested in the module that defines them.
