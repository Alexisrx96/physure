# Atomic Weights from `physure.conf` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The chemistry module takes its atomic weights from a `[AtomicWeights]` section of `physure.conf` (CIAAW 2024 abridged table, already committed as `32e24da`) instead of a table in Rust, so a user's conf or a unit system can override or add an element, and the default is the current IUPAC standard.

**Architecture:** `UnitRegistry` gets an `atomic_weights` map filled by the conf parser (strict, all-or-nothing, later definition wins, exactly like `[Constants]` and `[Units]`). `chemistry::elements::atomic_weight` reads it from the catalog in scope (`with_current_catalog`, falling back to `with_conf_registry`), the same way `gas_constant` already does. The 118-row Rust table is deleted.

**Tech Stack:** Rust (`physure-core`), Python only for the golden generator.

**Follows:** `docs/superpowers/plans/2026-11-07-chemistry-core.md` (all tasks done on this branch). **Decisions made with the user (2026-11-07):** default = CIAAW *abridged* 2024 table; elements with no standard weight keep a mass number with uncertainty 0 (a convention of the conf, commented there). **Open assumption, stated in the conf comment and the CHANGELOG:** the "±" CIAAW prints is treated as a standard uncertainty (k = 1), as the Python package did; CIAAW's page does not say, and the primary report could not be read.

## What changes for callers

- `Species::molar_mass_parts()` returns `PhysureResult<(f64, f64)>` (an element can be missing from the catalog in scope, a panic is not acceptable).
- `parse_formula`, `Species::new`, `Reaction::from_string` validate element symbols against the catalog in scope, so `D2O` parses when a conf defines `D = 2.01410177812, 0.00000000012` and is refused when none does.
- Molar masses move in the 3rd–5th digit versus the Python package (e.g. Na 22.98976928 → 22.990, Be 9.0121831 → 9.0122) and some uncertainties grow (single-isotope elements). The molar-mass golden file is regenerated from the conf (Task 11); the reactions golden is unchanged.

All commands run from the worktree root `D:/Projects/physure/.claude/worktrees/chemistry-core`. Commit trailer on every commit: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

---

### Task 9: The conf section is parsed into the registry

**Files:**
- Modify: `physure-core/src/units/registry.rs` (field + `new()`)
- Modify: `physure-core/src/units/conf.rs` (section arm + `read_atomic_weight`)
- Create: `physure-core/tests/atomic_weights_conf.rs`

- [ ] **Step 1: Write the failing tests**

`physure-core/tests/atomic_weights_conf.rs`:

```rust
//! The `[AtomicWeights]` section of physure.conf: `symbol = weight, standard uncertainty` in g/mol.

use physure_core::units::conf::{build_registry_from_conf, extend_registry};

#[test]
fn the_embedded_catalog_has_all_118_elements() {
    let (registry, _) = build_registry_from_conf();
    assert_eq!(registry.atomic_weights.len(), 118);
    assert_eq!(registry.atomic_weights["H"], (1.008, 0.0002));
    assert_eq!(registry.atomic_weights["Fe"], (55.845, 0.002));
    assert_eq!(registry.atomic_weights["Pb"], (207.2, 1.1));
    // no standard weight: the mass number and an uncertainty of 0
    assert_eq!(registry.atomic_weights["Tc"], (98.0, 0.0));
    assert_eq!(registry.atomic_weights["Og"], (294.0, 0.0));
}

#[test]
fn a_later_definition_replaces_and_a_new_symbol_is_added() {
    let (mut registry, mut constants) = build_registry_from_conf();
    extend_registry(&mut registry, &mut constants, "[AtomicWeights]\nFe = 56.0, 0.5\nD = 2.01410177812, 0.00000000012\n")
        .unwrap();
    assert_eq!(registry.atomic_weights["Fe"], (56.0, 0.5));
    assert_eq!(registry.atomic_weights["D"], (2.01410177812, 0.00000000012));
    assert_eq!(registry.atomic_weights.len(), 119);
    assert_eq!(registry.atomic_weights["H"], (1.008, 0.0002), "what is not redefined stays");
}

#[test]
fn a_line_that_does_not_read_is_reported_and_nothing_is_applied() {
    for bad in [
        "Fe = abc, 0.1",   // weight is not a number
        "Fe = 55.8",       // no uncertainty
        "Fe = 55.8, x",    // uncertainty is not a number
        "fe = 55.8, 0.1",  // not an element symbol
        "Fe2 = 55.8, 0.1", // not an element symbol
        "Fe = -1, 0",      // a weight must be positive
        "Fe = 0, 0",
        "Fe = 55.8, -0.1", // an uncertainty cannot be negative
        "Fe = 55.8, nan",
        "Fe = inf, 0",
    ] {
        let (mut registry, mut constants) = build_registry_from_conf();
        let before = registry.atomic_weights.clone();
        let text = format!("[AtomicWeights]\nNa = 23.0, 0.1\n{bad}\n");
        let problems = extend_registry(&mut registry, &mut constants, &text).unwrap_err();
        assert_eq!(problems.len(), 1, "{bad:?}: {problems:?}");
        assert!(problems[0].contains(bad), "{bad:?} not named in {problems:?}");
        assert_eq!(registry.atomic_weights, before, "{bad:?} left a trace");
    }
}
```

Run: `cargo test -p physure --test atomic_weights_conf`
Expected: FAIL to compile (`atomic_weights` is not a field).

- [ ] **Step 2: The registry field**

In `physure-core/src/units/registry.rs`, add to `pub struct UnitRegistry` after `constants_meta`:

```rust
    /// Standard atomic weights from `[AtomicWeights]`: symbol to `(weight in g/mol, standard uncertainty)`.
    pub atomic_weights: HashMap<String, (f64, f64)>,
```

and in `UnitRegistry::new()` after `constants_meta: HashMap::new(),`:

```rust
            atomic_weights: HashMap::new(),
```

If any other place builds `UnitRegistry { .. }` as a struct literal the compiler names it; add the field there too.

- [ ] **Step 3: The parser**

In `physure-core/src/units/conf.rs`, in the `match current_section` of `parse_conf` add the arm after `"Constants"`:

```rust
            "AtomicWeights" => read_atomic_weight(line, registry, problems),
```

and add next to `read_constant`:

```rust
/// One line of `[AtomicWeights]`: `symbol = weight, standard uncertainty`, both in g/mol. A weight is
/// positive and finite and an uncertainty is finite and not negative; anything else is a problem, because
/// a molar mass built from it would be a confident wrong answer.
fn read_atomic_weight(line: &str, registry: &mut UnitRegistry, problems: &mut Vec<String>) {
    let parsed = (|| {
        let (symbol, rest) = line.split_once('=')?;
        let (weight, std) = rest.split_once(',')?;
        let symbol = symbol.trim();
        let mut chars = symbol.chars();
        let is_symbol = chars.next().is_some_and(|c| c.is_ascii_uppercase())
            && symbol.len() <= 3
            && chars.all(|c| c.is_ascii_lowercase());
        let weight: f64 = weight.trim().parse().ok()?;
        let std: f64 = std.trim().parse().ok()?;
        (is_symbol && weight.is_finite() && weight > 0.0 && std.is_finite() && std >= 0.0)
            .then(|| (symbol.to_string(), (weight, std)))
    })();
    match parsed {
        Some((symbol, value)) => {
            registry.atomic_weights.insert(symbol, value);
        }
        None => problems.push(format!(
            "{line}: expected `symbol = weight, standard uncertainty` with a positive weight and a non-negative uncertainty"
        )),
    }
}
```

- [ ] **Step 4: Run, then the whole crate's unit tests**

Run: `cargo test -p physure --test atomic_weights_conf` → 3 pass.
Run: `cargo test -p physure` → green (nothing else reads the section yet; `Python`'s configparser is checked in Task 11).

- [ ] **Step 5: Commit**

```bash
git add physure-core/src/units physure-core/tests/atomic_weights_conf.rs
git commit -m "feat(core): read [AtomicWeights] from the conf, strictly, into the registry

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 10: The chemistry module reads the weights from the catalog in scope

**Files:**
- Modify: `physure-core/src/chemistry/mod.rs` (shared scope helper)
- Modify: `physure-core/src/chemistry/elements.rs` (table deleted)
- Modify: `physure-core/src/chemistry/formula.rs`, `species.rs`, `thermo.rs`
- Test: the `tests` modules of those files

Design (follow it exactly):

1. `mod.rs` gets one helper that every reader uses, so the catalog is resolved **once per operation** (`with_conf_registry` builds a merged registry on every call when a local `physure.conf` exists, which must not happen once per element):

```rust
use crate::units::{conf::with_conf_registry, with_current_catalog, UnitRegistry};

/// Runs `f` against the catalog in scope, or against the embedded one plus the working directory's conf.
pub(crate) fn with_catalog_in_scope<R>(f: impl FnOnce(&UnitRegistry) -> R) -> R {
    with_current_catalog(|scoped| match scoped {
        Some(registry) => f(registry),
        None => with_conf_registry(f),
    })
}
```

   `thermo::gas_constant` becomes `with_catalog_in_scope(read_gas_constant)`; delete its own scope code and the now-unused imports.

2. `elements.rs` loses `ATOMIC_WEIGHTS` and the table tests; it keeps one function and its doc:

```rust
//! Standard atomic weights, read from the `[AtomicWeights]` section of `physure.conf` (CIAAW abridged
//! 2024 by default; a conf of your own or a unit system can replace or add an element).

/// `(weight in g/mol, standard uncertainty)` of an element by symbol, from the catalog in scope.
pub fn atomic_weight(symbol: &str) -> Option<(f64, f64)> {
    super::with_catalog_in_scope(|registry| registry.atomic_weights.get(symbol).copied())
}
```

   Tests for it: `H` is `(1.008, 0.0002)`; `Xx`, `h` and `""` are `None`; inside `crate::units::with_catalog(&registry, || ...)` (see `units/scope.rs` for the signature) with a registry from `build_registry_from_conf()` extended by `D = 2.01410177812, 0.00000000012`, `atomic_weight("D")` is that pair, and outside the scope it is `None`.

3. `formula.rs`: `parse_formula` resolves the catalog once and passes the map down. `parse_group(part, formula, weights: &HashMap<String, (f64, f64)>)` checks `weights.contains_key(symbol)` where it called `atomic_weight(symbol).is_none()`; `parse_formula` wraps the body in `super::with_catalog_in_scope(|registry| ...)` passing `&registry.atomic_weights`. Behaviour and messages are otherwise unchanged. Add a test: `D2O` is refused by default and parsed inside a scope whose registry defines `D`.

4. `species.rs`: replace `molar_mass_parts` and the source-building code with one internal function that resolves the catalog once and returns the per-element terms:

```rust
/// For each element: `(symbol, count * weight, count * standard uncertainty)`, from the catalog in scope.
fn terms(&self) -> PhysureResult<Vec<(String, f64, f64)>> {
    super::with_catalog_in_scope(|registry| {
        self.composition
            .iter()
            .map(|(element, &count)| {
                let (weight, std) = registry.atomic_weights.get(element).copied().ok_or_else(|| {
                    PhysureError::DomainError(format!("no atomic weight for {element:?} in the catalog"))
                })?;
                Ok((element.clone(), f64::from(count) * weight, f64::from(count) * std))
            })
            .collect()
    })
}
```

   `molar_mass_parts()` returns `PhysureResult<(f64, f64)>`: the sum of the second field and the square root of the sum of squares of the third. `molar_mass()` builds the quantity from the same terms with the stable per-element source ids it already uses (mean = sum of the second field, one `(id, third field)` source per element). Update every caller (`mass_to_moles`, tests, `tests/chemistry_golden.rs`) for the `Result`.

   Tests to add: inside a scope whose registry says `H = 2.0, 0`, `Species::new("H2")` has `molar_mass_parts() == Ok((4.0, 0.0))`; a `Species` made by default, then asked for its molar mass inside a scope whose registry has no `Zr`, returns an `Err` naming `Zr` (not a panic) — build that registry by cloning `build_registry_from_conf()` and removing the key.

5. Existing tests whose expected numbers came from the old Python table must be updated **from the new conf values, written down by hand as literals, never read back from the code under test**: C is now `12.011 ± 0.002` (was `0.001`), the rest of the weights used by the existing tests (H 1.008 ± 0.0002, N 14.007 ± 0.001, O 15.999 ± 0.001, Ca 40.078 ± 0.004, Cu 63.546 ± 0.003, S 32.06 ± 0.02, Fe 55.845 ± 0.002) are unchanged. `tests/chemistry_golden.rs` is handled in Task 11. Do not change a tolerance.

- [ ] **Step 1:** Make the changes above, tests first where a test can fail before the change (the `D2O` and scoped-`H = 2.0` tests do).
- [ ] **Step 2:** `cargo test -p physure --lib chemistry` → green. `cargo test -p physure --test chemistry_golden` is expected to FAIL on molar-mass rows only until Task 11; the reactions test must pass.
- [ ] **Step 3:** `cargo clippy -p physure --lib --tests -- -D warnings` → nothing from `chemistry/` (the ~26 older errors elsewhere are not ours).
- [ ] **Step 4: Commit**

```bash
git add physure-core/src/chemistry
git commit -m "feat(core): the chemistry module reads its atomic weights from the catalog in scope

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Golden file, docs and changelog

**Files:**
- Modify: `scripts/gen_chemistry_golden.py`, `tests/golden/chemistry_molar_mass.tsv`, `physure-core/tests/chemistry_golden.rs`
- Modify: `CHANGELOG.md`, `docs/chemistry_roadmap.md`, and the doc that describes the sections of `physure.conf` (find it with `grep -rln "\[Constants\]" docs README.md physure-python/docs 2>/dev/null`; add a short `[AtomicWeights]` description there, no relative links to source files)

- [ ] **Step 1: The generator no longer reads Python's table.** In `gen_chemistry_golden.py` replace `molar_mass_rows` so the *weights* come from `physure.conf` and the *formula parsing* from the Python implementation (still independent of the Rust code):

```python
CONF = Path(__file__).resolve().parent.parent / "physure-core" / "src" / "units" / "physure.conf"


def atomic_weights() -> dict[str, tuple[float, float]]:
    """symbol -> (weight, standard uncertainty) from the [AtomicWeights] section of physure.conf."""
    weights: dict[str, tuple[float, float]] = {}
    section = None
    for raw in CONF.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith(("#", ";")):
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1]
        elif section == "AtomicWeights":
            symbol, rest = line.split("=", 1)
            weight, std = rest.split(",", 1)
            weights[symbol.strip()] = (float(weight), float(std))
    return weights


def molar_mass_rows() -> list[str]:
    weights = atomic_weights()
    rows = []
    for formula in [*weights, *COMPOUNDS]:
        mass = variance = 0.0
        for element, count in parse_formula(formula).items():
            weight, std = weights[element]
            mass += count * weight
            variance += (count * std) ** 2
        rows.append(f"{formula}\t{mass!r}\t{variance**0.5!r}")
    return rows
```

   with `from physure.ext.chemistry.species import parse_formula` instead of the `ATOMIC_WEIGHTS`/`Species` import (the reactions part still uses `Reaction`). Update the module docstring and the TSV header: the molar masses are the CIAAW weights of `physure.conf` combined by the formula parser, no longer "what Python answered"; the reactions file still is. Regenerate with `PYTHONUTF8=1 /d/Projects/physure/physure-python/.venv/Scripts/python.exe scripts/gen_chemistry_golden.py` (run from the worktree root; do not use `uv`). The row count stays 139 lines. In `chemistry_golden.rs` update the module comment and the `?` for `molar_mass_parts`.

- [ ] **Step 2:** `cargo test -p physure --test chemistry_golden` → 2 pass.

- [ ] **Step 3: Python's own reading of the conf.** Python (`physure-python/physure/application/startup.py`) parses this same file with `configparser`. Read how it builds its parser (around lines 150–260) and prove with a throwaway script run by the venv python (not committed) that the same options parse the new file without raising; report the result. Do not edit Python.

- [ ] **Step 4: CHANGELOG.** Edit the existing "Chemistry in the Rust core" entry (do not add a second one): replace the sentence that says the table is IUPAC 2013 with what is true now — the atomic weights are the `[AtomicWeights]` section of `physure.conf` (CIAAW abridged 2024, the table of the 2021 report with the 2024 revisions of Gd, Lu and Zr), overridable per element in a conf or a unit system, strict like the rest of the conf; elements without a standard weight carry a mass number with uncertainty 0 by convention of the file; the "±" is taken as a standard uncertainty (CIAAW's page does not say); molar masses therefore differ from the Python package in the 3rd–5th digit (Na 22.990 instead of 22.98976928) and single-isotope elements get larger uncertainties; `molar_mass_parts` returns a `Result`; and the molar-mass golden file is computed from the conf by the generator while the reactions golden is still what Python answered. Keep the list of deliberate differences.
  In `docs/chemistry_roadmap.md` fix the Phase 5 status note: the weights are in `physure.conf`, not in a Rust table, and 5.2 is "periodic table in the conf".

- [ ] **Step 5: Gates.** `cargo test -p physure` (everything), the strict mkdocs build (`D:/Projects/physure/physure-python/.venv/Scripts/python.exe -m mkdocs build --strict -f physure-python/mkdocs.yml -d <tmp>`; delete any `.cache/` it leaves in the worktree), and `cargo llvm-cov -p physure --summary-only` for the per-file coverage of `chemistry/*.rs` and `units/conf.rs` (chemistry files ≥ 90 %).

- [ ] **Step 6: Commit**

```bash
git add -A scripts tests physure-core CHANGELOG.md docs
git commit -m "docs+test(core): atomic weights come from the conf; the molar-mass golden is computed from it

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-review

- **Spec coverage:** "most accepted" → CIAAW abridged 2024 in the conf (done, `32e24da`); "configurable on conf" → Tasks 9–10 (section, strict parse, later wins, catalog in scope, unit-system override via `with_catalog`); user decisions on abridged vs full precision and on elements without a weight → in the data and the conf comment; open assumption on "±" → conf comment + CHANGELOG (Task 11).
- **Placeholders:** none. The conf block was generated from the CIAAW page (84 values) and, for the 34 elements the page prints no weight for, from the Python package's mass numbers.
- **Types:** `atomic_weights: HashMap<String, (f64, f64)>` in `UnitRegistry` (Task 9) is what `elements::atomic_weight`, `formula::parse_group` and `Species::terms` read (Task 10) and what the generator parses (Task 11); `molar_mass_parts` is `PhysureResult<(f64, f64)>` everywhere after Task 10.

---

### Task 12: The "±" of CIAAW is an interval half-width, read as a rectangular distribution (u = U/√3)

**Why (verified against the primary sources, 2026-11-07).** The number after "±" in the CIAAW tables is neither a standard uncertainty nor a GUM expanded uncertainty:

- Prohaska et al., *Standard atomic weights of the elements 2021*, Pure Appl. Chem. 94(5), 573–600 (2022), §6–7: the uncertainty U[Ar°(E)] is such that atomic weights of normal materials "are expected to lie between Ar°(E) − U and Ar°(E) + U with great certitude"; the old `192.217(2)` format was abandoned because it "suggests that the stated uncertainty is a standard uncertainty"; "the uniform distribution on these intervals … can be used as a simple and practically useful model".
- Possolo, van der Veen, Meija, Hibbert, Pure Appl. Chem. 90(2), 395–424 (2018), doi:10.1515/pac-2016-0402: it is a "decisional expanded uncertainty" that "differs essentially from the expanded uncertainty defined in the GUM".
- IUPAC *Guidelines for the use of atomic weights* (van der Veen et al.; read as the peer-review manuscript PAC-REC-16-04-01; published as *Interpretation and use of standard atomic weights*, doi:10.1515/pac-2017-1002): `u(Ar) = (b − a)/√12`, so "the half-width of the interval is 0.01 and hence the standard uncertainty is 0.01/√3 = 0.006"; for a molecular weight `u²(Mr) = Σ_E ν_E² u²(Ar(E))` (an element contributes its stoichiometric coefficient times its standard uncertainty; different elements are uncorrelated).

The code already combines per element in that way (one source per element). What is missing is the division by √3: until now the half-width was used as if it were a standard uncertainty, which overstates every molar-mass uncertainty by √3.

**Design.** The conf keeps the number CIAAW publishes (auditable against the page); the registry keeps what the rest of the library calls an uncertainty, a standard uncertainty.

- `symbol = weight, half_width` → the registry stores `(weight, half_width / √3)` (rectangular model).
- `symbol = weight, u, standard` → the number already is a standard uncertainty and is stored as given (for an isotope mass with its own `u`, e.g. `D = 2.01410177812, 0.00000000012, standard`).
- `symbol = weight, half_width, interval` is the same as the first form.
- Anything else in the third field is a problem (strict, as before). The weight is positive and finite, the number finite and not negative (as before).

Downstream (`atomic_weight`, `Species`, `molar_mass_parts`) is unchanged: it reads standard uncertainties.

**Files:**
- Modify: `physure-core/src/units/conf.rs` (`read_atomic_weight`), `physure-core/src/units/physure.conf` (the comment of the section only, no data line), `physure-core/tests/atomic_weights_conf.rs`
- Modify: the expected numbers in the `tests` modules of `physure-core/src/chemistry/{species,thermo,elements,formula}.rs` that use atomic-weight uncertainties
- Modify: `scripts/gen_chemistry_golden.py`, `tests/golden/chemistry_molar_mass.tsv` (regenerated), `physure-core/tests/chemistry_golden.rs` (comment only)
- Modify: `CHANGELOG.md` (the existing chemistry entry), `docs/chemistry_roadmap.md`, `.claude/skills/add-unit/SKILL.md`

- [ ] **Step 1: Tests first** in `tests/atomic_weights_conf.rs`: (a) the embedded catalog: `H` is `(1.008, 0.0002 / 3f64.sqrt())`, `Pb` is `(207.2, 1.1 / 3f64.sqrt())`, `Fe` is `(55.845, 0.002 / 3f64.sqrt())`, `Tc` is `(98.0, 0.0)` (write the expected values with that exact expression so the floats are bit-identical); (b) extending with `X = 2.0, 0.3` stores `0.3 / 3f64.sqrt()`, with `X = 2.0, 0.3, standard` stores `0.3`, with `X = 2.0, 0.3, interval` stores `0.3 / 3f64.sqrt()`; (c) refused lines (one problem each, registry untouched): `X = 2.0, 0.3, expanded`, `X = 2.0, 0.3, standard, extra`, `X = 2.0, 0.3,` (empty third field). Update the existing assertions in that file that compared the stored uncertainty to the raw number. Run → FAIL.
- [ ] **Step 2: Parser.** In `read_atomic_weight` split the part after `=` at the first comma into the weight and the rest; split the rest at an optional second comma into the number and a kind (`"interval"` when there is none, otherwise the trimmed text, which must be `"interval"` or `"standard"`); `std = number / 3f64.sqrt()` for `interval`, `number` for `standard`; keep the existing validity checks and the problem message, extended to mention the optional `, standard`.
- [ ] **Step 3: The conf comment.** Replace the paragraph of the `[AtomicWeights]` header that says the "+-" is taken as a standard uncertainty (k = 1) with what is true now: the second column is the half-width of the interval in which the atomic weight of normal materials lies (the "±" CIAAW prints; a decisional uncertainty, not a GUM expanded uncertainty, no coverage factor), read as a uniform distribution and stored as the standard uncertainty `half-width / sqrt(3)`; a third field `, standard` marks a number that already is a standard uncertainty; the uncertainty of a molar mass combines per element, `u² = Σ ν_E² u²(Ar(E))`, so repeated atoms of one element are one quantity; cite the three sources above. Update the first line of the header (`symbol = weight, half-width [, standard]`). Do not change any data line.
- [ ] **Step 4: Chemistry tests.** Run `cargo test -p physure`; every failing chemistry test is a hand-written expected uncertainty built from the old numbers. Recompute each expected value by hand from the CIAAW half-widths divided by √3 (H 0.0002/√3, O 0.001/√3, C 0.002/√3, and so on), written as literal expressions in the test, never read back from the code under test. Do not change a tolerance and do not touch the means. Add one test in `species.rs`: `Species::new("H2O")` has `molar_mass_parts().1 == sqrt((2 · 0.0002/√3)² + (0.001/√3)²)` within 1e-12 relative, with a comment naming the IUPAC formula.
- [ ] **Step 5: Golden.** In `gen_chemistry_golden.py`'s `atomic_weights()` apply the same rule when reading the conf (`/ math.sqrt(3)` unless the third field is `standard`) so the generator stays an independent implementation of the documented rule; update its docstring and the TSV header; regenerate (`PYTHONUTF8=1 /d/Projects/physure/physure-python/.venv/Scripts/python.exe scripts/gen_chemistry_golden.py`, no `uv`); the reactions TSV must come out byte-identical; the 1e-12 tolerances in the Rust test stay.
- [ ] **Step 6: Docs.** `CHANGELOG.md`: edit the existing "Chemistry in the Rust core" entry in place (keep its line endings): remove the "open assumption … standard uncertainty (k = 1)" sentence and say what is true: the second column is the half-width CIAAW publishes, read as a uniform distribution and converted to a standard uncertainty `U/√3` as IUPAC recommends (cite the three sources), `, standard` for a number that already is one, a molar mass combines per element; add to the list of deliberate differences that the Python package used the half-width as if it were a standard uncertainty (√3 too large). `docs/chemistry_roadmap.md` and `.claude/skills/add-unit/SKILL.md`: fix the `[AtomicWeights]` descriptions the same way (example `D = 2.01410177812, 0.00000000012, standard`).
- [ ] **Step 7: Gates and commit.** `cargo test -p physure --no-fail-fast` all green, clippy shows nothing new in the files touched, the mkdocs strict build passes (output outside the repo; delete any `.cache/` it leaves), `git status` shows only the intended files. One commit: `fix(core): the ± of the atomic weights is an interval half-width, read as uniform (u = U/√3)`.
