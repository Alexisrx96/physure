# A1 stage 2, step 1 — the registry answers — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `physure._core.UnitRegistry` answers, as methods, the questions that Python's `CompoundUnit` answers with the `UnitSystem` (dimension, conversion factor, kind, linearity, alias, simplified form, parsing), for the default catalog and for the catalog plus extra conf text. Nothing in Python consumes it yet.

**Architecture:** the core already has the answers (`UnitRegistry::facts`, `simplify`, `dimension_of`, `kind_of`, `is_linear_of`, `alias_of`, from stage 1c). This step adds the two it lacks (`expand` and `factor_between`, the conversion factor between two units written as symbols), a way to build a registry from the catalog plus extra text, and exposes all of it on the PyO3 `UnitRegistry` object so that a system can own one (step 2). Python tests compare the registry's answers with Python's own over the catalog, so the claim "the registry can stand in for the system" is measured before anything depends on it.

**Tech Stack:** Rust (`physure-core`, `physure-python/src/lib.rs` with PyO3 0.25), Python tests with pytest, the benchmark in `physure-python/benchmarks/`.

**Design:** `docs/superpowers/specs/2026-10-29-a1-stage2-design.md` (step 1). **Replay:** this plan has not been replayed mechanically yet; doing that on a clean branch is the first thing to do once the design is approved (the earlier plans of A1 were each replayed before they were executed).

**Conventions of the repository for every task:** work in a worktree branch, never on `main`; in a Windows shell `unset VIRTUAL_ENV` and use `uv run --no-sync`; rebuild the extension with `uv run --no-sync maturin develop --release` from `physure-python/` before a Python test sees a change in Rust; end commit messages with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; `ruff check .` and `ruff format --check .` from `physure-python/` must pass.

---

## File structure

| File | Change |
|---|---|
| `physure-core/src/units/registry.rs` | `expand`, `factor_between` |
| `physure-core/src/units/conf.rs` | `embedded_catalog()` (the cached parse, extracted) and `build_registry_with(extra)` |
| `physure-core/src/units/tests.rs` | tests of the three |
| `physure-python/src/lib.rs` | the methods of `PyUnitRegistry`; two helpers shared with `unit_facts` |
| `physure-python/physure/_core.pyi` | the stubs |
| `physure-python/tests/core/test_registry_facts.py` | new: the registry against Python |
| `physure-python/benchmarks/bench_unit_ops.py` | new: the microbenchmark that the gates of the design cite |
| `CHANGELOG.md`, the stage 2 design | the entry and the pointer |

---

### Task 1: `expand` and `factor_between` in the core

**Files:**
- Modify: `physure-core/src/units/registry.rs` (after `facts`)
- Test: `physure-core/src/units/tests.rs`

- [ ] **Step 1: Write the failing test**

Append to `physure-core/src/units/tests.rs`:

```rust
/// A unit written as symbols stands for what the registry says each symbol is, with exact scales:
/// `km/h` is 5/18 m/s, and the factor between two of them is rounded once.
#[test]
fn a_unit_written_as_symbols_expands_and_converts_by_the_registry() {
    use crate::units::parser::Parser;
    let (reg, _) = conf::build_registry_from_conf();
    let atomic = |text: &str| Parser::parse_expression_atomic(text).unwrap();

    let kmh = reg.expand(&atomic("km/h")).unwrap();
    assert!(kmh.same_dimensions(&reg.expand(&atomic("m/s")).unwrap()));
    assert_eq!(kmh.scale, 1000.0 / 3600.0);

    assert_eq!(reg.factor_between(&atomic("km/h"), &atomic("m/s")).unwrap(), 1000.0 / 3600.0);
    assert_eq!(reg.factor_between(&atomic("kWh"), &atomic("J")).unwrap(), 3.6e6);
    assert_eq!(reg.factor_between(&atomic("kg*m/s^2"), &atomic("N")).unwrap(), 1.0);
    // the offset of a temperature scale is not part of a factor
    assert_eq!(reg.factor_between(&atomic("degC"), &atomic("K")).unwrap(), 1.0);

    assert!(reg.factor_between(&atomic("m"), &atomic("s")).is_err());
    assert!(reg.expand(&atomic("furlongs_per_fortnight")).is_err());
    assert!(reg.expand(&atomic("dB/km")).is_err());
}
```

- [ ] **Step 2: Run it to see it fail**

Run (from `physure-core/`): `cargo test -p physure --lib a_unit_written_as_symbols_expands`
Expected: FAIL to compile, "no method named `expand` found for struct `UnitRegistry`".

- [ ] **Step 3: Implement**

In `physure-core/src/units/registry.rs`, after the method `facts` (the one ending with `simplified: unit.symbolic_repr(),\n        })\n    }`), add:

```rust

    /// The unit that a unit written as symbols stands for: each symbol replaced by what the
    /// registry says it is, with its exact scale (`km/h` is 5/18 m/s). `atomic` is a unit parsed
    /// with `Parser::parse_expression_atomic`. A symbol the registry does not know is an error, and
    /// so is a level inside a compound (`RationalUnit::refuse_logarithmic_compound`).
    pub fn expand(&self, atomic: &RationalUnit) -> PhysureResult<RationalUnit> {
        let mut expanded = RationalUnit::dimensionless();
        for (symbol, (n, d)) in &atomic.dimensions {
            let unit = self.get_unit(symbol).ok_or_else(|| PhysureError::UnknownUnit {
                suggestion: self.closest_symbol(symbol),
                symbol: symbol.clone(),
            })?;
            let power = unit.pow(Rational64::new(*n, *d));
            let product = expanded.mul(&power);
            RationalUnit::refuse_logarithmic_compound(&[&expanded, &power], &product)?;
            expanded = product;
        }
        Ok(expanded)
    }

    /// The factor that turns a magnitude in `from` into one in `to`, two units written as symbols
    /// of the same dimension: the ratio of their exact scales, rounded once. The zero point of a
    /// temperature scale is not part of a factor.
    pub fn factor_between(&self, from: &RationalUnit, to: &RationalUnit) -> PhysureResult<f64> {
        let (from, to) = (self.expand(from)?, self.expand(to)?);
        if !from.same_dimensions(&to) {
            return Err(PhysureError::UnitMismatch { expected: from.__repr__(), actual: to.__repr__() });
        }
        Ok(from.ratio_to(&to))
    }
```

- [ ] **Step 4: Run the test**

Run: `cargo test -p physure --lib a_unit_written_as_symbols_expands`
Expected: PASS. Then `cargo test -p physure` (all green).

- [ ] **Step 5: Commit**

```bash
git add physure-core/src/units/registry.rs physure-core/src/units/tests.rs
git commit -m "feat(core): expand a unit written as symbols and convert between two

UnitRegistry::expand replaces each symbol of a unit by what the registry
says it is, with its exact scale, and factor_between is the ratio of two
of them rounded once. They are what Python's conversion_factor_to does
with a UnitSystem.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: a registry from the catalog plus extra text

**Files:**
- Modify: `physure-core/src/units/conf.rs`
- Test: `physure-core/src/units/tests.rs`

- [ ] **Step 1: Write the failing test**

Append to `physure-core/src/units/tests.rs`:

```rust
/// A system is the catalog plus what it adds: a registry built with extra conf text knows the
/// extra units, a later definition wins, and the default registry is not touched.
#[test]
fn a_registry_can_be_built_from_the_catalog_plus_extra_text() {
    use crate::units::parser::Parser;
    let extra = "[Units]\nfurlong = 201.168, L, [fur, furlong]\n";
    let (reg, _) = conf::build_registry_with(extra);
    let atomic = |text: &str| Parser::parse_expression_atomic(text).unwrap();
    assert_eq!(reg.factor_between(&atomic("fur"), &atomic("m")).unwrap(), 201.168);
    assert_eq!(reg.facts("furlong").unwrap().dimension, vec![("L".to_string(), (1, 1))]);
    // the metre is still the metre
    assert_eq!(reg.factor_between(&atomic("km"), &atomic("m")).unwrap(), 1000.0);

    let (default, _) = conf::build_registry_from_conf();
    assert!(default.facts("furlong").is_err());

    // a later definition wins
    let (redefined, _) = conf::build_registry_with("[Units]\nfoot = 0.3, L, [ft, foot]\n");
    assert_eq!(redefined.factor_between(&atomic("ft"), &atomic("m")).unwrap(), 0.3);
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p physure --lib a_registry_can_be_built_from_the_catalog_plus_extra_text`
Expected: FAIL to compile, "cannot find function `build_registry_with` in module `conf`".

- [ ] **Step 3: Implement**

In `physure-core/src/units/conf.rs`, replace the head of `build_registry_from_conf` (from the `pub fn` line to the `.clone();` that ends the `EMBEDDED` block, both included) so that the cached parse is a function of its own and the new function uses it:

```rust
/// The embedded catalog, parsed once per process.
fn embedded_catalog() -> &'static (UnitRegistry, HashMap<String, String>) {
    static EMBEDDED: OnceLock<(UnitRegistry, HashMap<String, String>)> = OnceLock::new();
    EMBEDDED.get_or_init(|| {
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        parse_physure_conf(DEFAULT_PHYSURE_CONF, &mut reg, &mut constants);
        (reg, constants)
    })
}

/// The embedded catalog plus `extra`, the text of a system file or of a user conf, parsed after it
/// so that a later definition wins. The `physure.conf` of the working directory is not read: the
/// caller decides what a system is made of. A `[Settings]` section in `extra` writes the
/// process-wide settings, as it does in any conf.
pub fn build_registry_with(extra: &str) -> (UnitRegistry, HashMap<String, String>) {
    let (mut reg, mut constants) = embedded_catalog().clone();
    parse_physure_conf(extra, &mut reg, &mut constants);
    (reg, constants)
}

pub fn build_registry_from_conf() -> (UnitRegistry, HashMap<String, String>) {
    // 1. Load embedded default master physure.conf
    let (mut reg, mut constants) = embedded_catalog().clone();
```

(The doc comment of `build_registry_from_conf` that precedes it stays above it.)

- [ ] **Step 4: Run the tests**

Run: `cargo test -p physure` (all green, including the new test and the three settings tests).

- [ ] **Step 5: Commit**

```bash
git add physure-core/src/units/conf.rs physure-core/src/units/tests.rs
git commit -m "feat(core): build a registry from the catalog plus extra conf text

A unit system is the catalog plus what it adds. build_registry_with
parses extra text after the embedded catalog (a later definition wins)
and does not read the physure.conf of the working directory.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: the methods on the PyO3 registry

**Files:**
- Modify: `physure-python/src/lib.rs`
- Modify: `physure-python/physure/_core.pyi`

- [ ] **Step 1: Add the helpers and the methods**

In `physure-python/src/lib.rs`, add the two helpers just before `// ── PyUnitRegistry ──` (they hold the code that `unit_facts` has inline, which then calls them):

```rust
fn value_error(e: PhysureError) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(e.to_string())
}

/// A dimension as a Python dict: the dimension symbol and its exponent, an int when it is one.
fn dimension_dict<'py>(py: Python<'py>, dimension: &[(String, (i64, i64))]) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    for (symbol, (n, d)) in dimension {
        if *d == 1 {
            out.set_item(symbol, *n)?;
        } else {
            out.set_item(symbol, *n as f64 / *d as f64)?;
        }
    }
    Ok(out)
}

fn kind_name(kind: UnitKind) -> &'static str {
    if kind == UnitKind::Point { "absolute" } else { "delta" }
}
```

Replace the body of `unit_facts` (the whole function, keep its doc comment and `#[pyfunction]`) with:

```rust
fn unit_facts<'py>(py: Python<'py>, expression: &str) -> PyResult<Bound<'py, PyDict>> {
    let (registry, _) = ::physure_core::units::conf::build_registry_from_conf();
    facts_dict(py, &registry, expression)
}

fn facts_dict<'py>(py: Python<'py>, registry: &UnitRegistry, expression: &str) -> PyResult<Bound<'py, PyDict>> {
    let facts = registry.facts(expression).map_err(value_error)?;
    let out = PyDict::new(py);
    out.set_item("dimension", dimension_dict(py, &facts.dimension)?)?;
    out.set_item("kind", kind_name(facts.kind))?;
    out.set_item("is_linear", facts.is_linear)?;
    out.set_item("alias", facts.alias)?;
    out.set_item("simplified", facts.simplified)?;
    Ok(out)
}
```

In `impl PyUnitRegistry`, after `from_conf`, add:

```rust
    /// The catalog plus extra conf text (the text of a system file, or units of the user's).
    #[staticmethod]
    fn from_conf_text(extra: &str) -> Self {
        let (reg, _constants) = ::physure_core::units::conf::build_registry_with(extra);
        PyUnitRegistry(reg)
    }

    /// A unit written as symbols (`km/h`), not resolved: its dimensions are the symbols written.
    fn parse_atomic(&self, py: Python<'_>, expression: &str) -> PyResult<PyObject> {
        let unit = ::physure_core::units::parser::Parser::parse_expression_atomic(expression).map_err(value_error)?;
        get_cached_unit(py, unit)
    }

    /// The unit with every recipe unit (`N`, `J`, `Hz`...) replaced by its base symbols.
    fn simplify(&self, py: Python<'_>, unit: &PyRationalUnit) -> PyResult<PyObject> {
        get_cached_unit(py, self.0.simplify(&unit.0))
    }

    /// What a unit written as symbols stands for: each symbol replaced by what the registry says it is.
    fn expand(&self, py: Python<'_>, unit: &PyRationalUnit) -> PyResult<PyObject> {
        get_cached_unit(py, self.0.expand(&unit.0).map_err(value_error)?)
    }

    /// The physical dimension of a unit written as symbols, by dimension symbol.
    fn dimension_of<'py>(&self, py: Python<'py>, unit: &PyRationalUnit) -> PyResult<Bound<'py, PyDict>> {
        dimension_dict(py, &self.0.dimension_of(&unit.0).map_err(value_error)?)
    }

    /// `"absolute"` for a temperature scale (degC, degF), `"delta"` for anything else.
    fn kind_of(&self, unit: &PyRationalUnit) -> &'static str {
        kind_name(self.0.kind_of(&unit.0))
    }

    /// False when the unit is an offset or a logarithmic unit.
    fn is_linear_of(&self, unit: &PyRationalUnit) -> bool {
        self.0.is_linear_of(&unit.0)
    }

    /// The name the unit is shown by (`kg·m²/s³` is `W`).
    fn alias_of(&self, unit: &PyRationalUnit) -> String {
        self.0.alias_of(&unit.0)
    }

    /// The factor that turns a magnitude in `source` into one in `target`, rounded once.
    fn factor_between(&self, source: &PyRationalUnit, target: &PyRationalUnit) -> PyResult<f64> {
        self.0.factor_between(&source.0, &target.0).map_err(value_error)
    }

    /// Dimension, kind, linearity, alias and simplified form of a unit expression.
    fn facts<'py>(&self, py: Python<'py>, expression: &str) -> PyResult<Bound<'py, PyDict>> {
        facts_dict(py, &self.0, expression)
    }
```

- [ ] **Step 2: Add the stubs**

In `physure-python/physure/_core.pyi`, find `class UnitRegistry:` and add after its existing methods:

```python
    @staticmethod
    def from_conf_text(extra: str) -> UnitRegistry: ...
    def parse_atomic(self, expression: str) -> RationalUnit: ...
    def simplify(self, unit: RationalUnit) -> RationalUnit: ...
    def expand(self, unit: RationalUnit) -> RationalUnit: ...
    def dimension_of(self, unit: RationalUnit) -> dict[str, int | float]: ...
    def kind_of(self, unit: RationalUnit) -> str: ...
    def is_linear_of(self, unit: RationalUnit) -> bool: ...
    def alias_of(self, unit: RationalUnit) -> str: ...
    def factor_between(self, source: RationalUnit, target: RationalUnit) -> float: ...
    def facts(self, expression: str) -> dict[str, Any]: ...
```

- [ ] **Step 3: Build**

Run (from `physure-python/`, `unset VIRTUAL_ENV` first): `uv run --no-sync maturin develop --release`, then `uv run --no-sync python -X utf8 -c "import physure._core as c; r = c.UnitRegistry.from_conf(); print(r.facts('J/s'))"`
Expected: the extension builds; the print shows `{'dimension': {'L': 2, 'M': 1, 'T': -3}, 'kind': 'delta', 'is_linear': True, 'alias': 'W', 'simplified': 'kg·m²/s³'}`, and `c.__file__` is inside this worktree.

- [ ] **Step 4: Commit**

```bash
git add physure-python/src/lib.rs physure-python/physure/_core.pyi
git commit -m "feat: the registry of the extension answers what a unit is

UnitRegistry gets parse_atomic, simplify, expand, dimension_of, kind_of,
is_linear_of, alias_of, factor_between and facts, and from_conf_text to
build one from the catalog plus extra text. unit_facts shares its dict
builders with them.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: the registry against Python

**Files:**
- Create: `physure-python/tests/core/test_registry_facts.py`

- [ ] **Step 1: Write the tests**

```python
"""A1 stage 2, step 1: the registry object answers what Python answers.

A ``UnitSystem`` will own one of these (step 2). Before anything depends on it,
its answers are compared with the system's, over the units of the catalog.
"""

import json
from pathlib import Path

import pytest

from physure import Q_
from physure._core import UnitRegistry
from physure.application.context import get_current_system
from physure.domain.exceptions import UnknownUnitError
from physure.units._index import UNIT_INDEX

GOLDEN_FILE = Path(__file__).parent / "golden" / "unit_semantics.json"
FACT_FIELDS = ("dimension", "kind", "is_linear", "alias", "simplified")


def _plain(number):
    value = float(number)
    return int(value) if value.is_integer() else value


def _python_facts(expression):
    system = get_current_system()
    unit = Q_(1.0, expression).unit
    return {
        "dimension": {
            key: _plain(exp)
            for key, exp in sorted(unit.dimension(system).exponents.items())
        },
        "kind": unit.kind(system),
        "is_linear": unit.is_linear(system),
        "alias": unit.to_string(system, use_alias=True),
        "simplified": str(unit.simplify(system)),
    }


@pytest.fixture(scope="module")
def registry():
    return UnitRegistry.from_conf()


@pytest.mark.parametrize(
    "expression",
    sorted(json.loads(GOLDEN_FILE.read_text(encoding="utf-8"))),
)
def test_the_registry_reproduces_the_frozen_unit_semantics(
    registry, expression
):
    frozen = json.loads(GOLDEN_FILE.read_text(encoding="utf-8"))[expression]
    facts = registry.facts(expression)
    for field in FACT_FIELDS:
        assert facts[field] == frozen[field], field


def test_the_registry_agrees_with_python_on_every_unit_it_resolves(registry):
    checked = 0
    for name in sorted(UNIT_INDEX):
        try:
            python = _python_facts(name)
        except UnknownUnitError:
            continue
        rust = registry.facts(name)
        for field in FACT_FIELDS:
            assert rust[field] == python[field], (name, field)
        checked += 1
    assert checked > 2000


def test_the_pieces_of_a_unit_answer_like_the_facts(registry):
    unit = registry.parse_atomic("J/s")
    simplified = registry.simplify(unit)
    assert registry.dimension_of(simplified) == {"L": 2, "M": 1, "T": -3}
    assert registry.alias_of(simplified) == "W"
    assert registry.kind_of(registry.parse_atomic("degC")) == "absolute"
    assert registry.is_linear_of(registry.parse_atomic("dB")) is False
    assert registry.is_linear_of(unit) is True


@pytest.mark.parametrize(
    ("source", "target"),
    [
        ("km/h", "m/s"),
        ("kWh", "J"),
        ("psi", "Pa"),
        ("g/cm^3", "kg/m^3"),
        ("L/min", "m^3/s"),
        ("BTU/h", "W"),
        ("ft*lbf", "J"),
        ("mol/L", "mol/m^3"),
    ],
)
def test_the_factor_between_two_units_is_pythons(registry, source, target):
    system = get_current_system()
    python = Q_(1.0, source).unit.conversion_factor_to(
        Q_(1.0, target).unit, system
    )
    rust = registry.factor_between(
        registry.parse_atomic(source), registry.parse_atomic(target)
    )
    assert rust == python


def test_incompatible_units_have_no_factor(registry):
    with pytest.raises(ValueError, match="mismatch"):
        registry.factor_between(
            registry.parse_atomic("m"), registry.parse_atomic("s")
        )


def test_extra_conf_text_adds_units_to_that_registry_only():
    extra = "[Units]\nfurlong = 201.168, L, [fur, furlong]\n"
    registry = UnitRegistry.from_conf_text(extra)
    assert registry.facts("fur")["dimension"] == {"L": 1}
    assert registry.factor_between(
        registry.parse_atomic("fur"), registry.parse_atomic("m")
    ) == pytest.approx(201.168)
    with pytest.raises(ValueError, match="Unknown unit"):
        UnitRegistry.from_conf().facts("fur")
```

- [ ] **Step 2: Run them**

Run (from `physure-python/`): `uv run --no-sync pytest tests/core/test_registry_facts.py -q -p no:cacheprovider`
Expected: all pass (about 45 tests). If `conversion_factor_to` has another signature than `(target, system)` in the checkout, read `units.py` and adapt the one call; the comparison itself must stay exact equality.

- [ ] **Step 3: Format and lint**

Run: `uv run --no-sync ruff format tests/core/test_registry_facts.py` and `uv run --no-sync ruff check .`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add physure-python/tests/core/test_registry_facts.py
git commit -m "test: the registry object answers what Python answers

The facts of the 32 expressions frozen in stage 0 and of every unit of
the catalog that Python resolves, the factor between eight compound
units (exact equality with conversion_factor_to), and a registry built
with extra conf text.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: the microbenchmark that the gates cite

**Files:**
- Create: `physure-python/benchmarks/bench_unit_ops.py`

- [ ] **Step 1: Write it**

```python
"""Microbenchmark of the unit and quantity operations (A1 stage 1d onwards).

Run from ``physure-python/``: ``uv run --no-sync python benchmarks/bench_unit_ops.py [out.json]``.
The gate of every step of A1 stage 2 is that no row is more than 10 % slower
than the numbers recorded in the stage 1d section of the design, measured on
the same machine.
"""

import json
import sys
import timeit

import numpy as np

import physure
from physure import Q_, units

m, s = units.m, units.s
q1, q2 = Q_(5.0, "m"), Q_(3.0, "m")
q_speed = Q_(5.0, "m") / Q_(2.0, "s")
array = Q_(np.ones(1000), "m")


def measure(label, fn, results):
    timer = timeit.Timer(fn)
    number, _ = timer.autorange()
    best = min(timer.repeat(repeat=5, number=number)) / number
    results[label] = best * 1e9
    print(f"{label:<42} {best * 1e9:>12,.0f} ns")


def main():
    results = {}
    cases = [
        ("Q_(5.0, 'm')  [string unit]", lambda: Q_(5.0, "m")),
        ("Q_(5.0, units.m)  [unit object]", lambda: Q_(5.0, m)),
        ("5 * units.m  [the sugar]", lambda: 5 * m),
        ("units.m * units.s", lambda: m * s),
        ("units.m / units.s", lambda: m / s),
        ("units.m ** 2", lambda: m**2),
        ("q1 + q2", lambda: q1 + q2),
        ("q1 * q2", lambda: q1 * q2),
        ("q1.to('km')", lambda: q1.to("km")),
        ("q_speed.to('km/h')", lambda: q_speed.to("km/h")),
        ("array(1000).to('km')", lambda: array.to("km")),
    ]
    for label, fn in cases:
        measure(label, fn, results)
    if len(sys.argv) > 1:
        with open(sys.argv[1], "w", encoding="utf-8") as out:
            json.dump(results, out, indent=1)


if __name__ == "__main__":
    print(f"physure {physure.__version__}")
    main()
```

- [ ] **Step 2: Run it**

Run: `uv run --no-sync python benchmarks/bench_unit_ops.py`
Expected: eleven rows, each within about 10 % of the stage 1d table (`Q_(5.0, 'm')` about 15 000 ns, a unit product about 1 000 ns).

- [ ] **Step 3: Format, lint, commit**

```bash
uv run --no-sync ruff format benchmarks/bench_unit_ops.py && uv run --no-sync ruff check .
git add physure-python/benchmarks/bench_unit_ops.py
git commit -m "bench: the unit and quantity operations that the stage 2 gates cite

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: changelog, design pointer and the full verification

**Files:**
- Modify: `CHANGELOG.md`, `docs/superpowers/specs/2026-10-29-a1-stage2-design.md`

- [ ] **Step 1: Changelog**

In `CHANGELOG.md`, under `## [Unreleased]` / `### Changed`, before the line `### Fixed`, add one bullet:

```markdown
- **The registry of the extension answers what a unit is.** `physure._core.UnitRegistry` has `parse_atomic`, `simplify`, `expand`, `dimension_of`, `kind_of`, `is_linear_of`, `alias_of`, `factor_between` and `facts`, and `UnitRegistry.from_conf_text(extra)` builds one from the catalog plus extra conf text. They are what `CompoundUnit` answers with a `UnitSystem`; nothing in Python calls them yet.
```

- [ ] **Step 2: Design pointer**

Append to `docs/superpowers/specs/2026-10-29-a1-stage2-design.md`:

```markdown

## Outcome of step 1 (the registry answers)

- The registry object answers the facts of every unit of the catalog that Python resolves with the same values, and the conversion factor between compound units with exact equality, as the new `tests/core/test_registry_facts.py` shows.
- `UnitRegistry::expand` and `factor_between` are new in the core; a registry can be built from the catalog plus extra text (`build_registry_with`), which a system file or a user conf will use in step 2. `[Settings]` of an extra conf still writes process-wide settings; whether a registry owns its settings is a question of step 2.
- `physure-python/benchmarks/bench_unit_ops.py` is the benchmark the gates cite.
```

- [ ] **Step 3: Verify everything**

Run from the repository root of the worktree:
- `cargo test --workspace --exclude physure-python --no-fail-fast` (all ok)
- from `physure-python/`: `uv run --no-sync ruff check .`, `uv run --no-sync ruff format --check .`, `uv run --no-sync pytest -q -p no:cacheprovider` (all green, the same five xfails as before), `uv run --no-sync mkdocs build --strict`
- `uv run --no-sync python benchmarks/bench_unit_ops.py` once more: no row slower than 10 % against the previous run.

- [ ] **Step 4: Commit**

```bash
git add CHANGELOG.md docs/superpowers/specs/2026-10-29-a1-stage2-design.md
git commit -m "docs: changelog and outcome of the registry API (A1 stage 2, step 1)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-review against the design

- **Coverage:** step 1 of the design asks for `parse_atomic`, `simplify`, `expand`, `dimension_of`, `kind_of`, `is_linear_of`, `alias_of`, `factor_between`, `facts` and a registry from the catalog plus extra text (Tasks 1–3), the acceptance that the facts through the registry equal Python's (Task 4) and the benchmark of the gates (Task 5).
- **Placeholders:** none; the one place that depends on the checkout (the signature of `conversion_factor_to`, Task 4 Step 2) says how to adapt it.
- **Type consistency:** `expand` and `factor_between` take and return `RationalUnit`; the PyO3 methods name them `unit`, `source` and `target`; the stubs match them; `dimension_dict` returns the int-or-float dict that `unit_facts` returned before.
- **Out of scope here, in the design:** the system owning its registry (step 2), settings in the registry, the seam, the class swap.
