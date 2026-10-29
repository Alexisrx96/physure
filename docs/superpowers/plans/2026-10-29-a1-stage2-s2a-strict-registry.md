# A1 stage 2, step 2a — the core takes a system's definitions strictly — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a core `UnitRegistry` can take the definitions a unit system is made of (units, prefixes, levels of a quantity, units of a new dimension such as money) from text, all or nothing, with every problem reported, and the extension exposes it as `UnitRegistry.extend_conf`. Nothing in Python consumes it yet (step 2b).

**Architecture:** `parse_physure_conf` becomes a thin lenient wrapper of `parse_conf`, which reports what it cannot read; `extend_registry` is the strict reading (a refused text leaves the registry as it was, `[Settings]` is not applied). The registry remembers the base unit of each dimension symbol so that a later text starts from it, and `$` is a dimension symbol. A level may have the dimensions of a quantity (`dBW`).

**Tech Stack:** Rust (`physure-core`, `physure-python/src/lib.rs`, PyO3 0.25), pytest.

**Design:** `docs/superpowers/specs/2026-10-29-a1-stage2-design.md` ("Step 2, refined"). **Replay:** this plan has not been replayed mechanically yet; doing it on a clean branch is the first thing to do once the design is confirmed.

**Conventions of the repository for every task:** work in a worktree branch, never on `main`; in a Windows shell `unset VIRTUAL_ENV` and use `uv run --no-sync`; rebuild the extension with `uv run --no-sync maturin develop --release` from `physure-python/` before a Python test sees a change in Rust; `physure-core/src/units/conf.rs`, `registry.rs`, `tests.rs` and `physure-core/src/quantity.rs` use CRLF line endings, so edit them with a script that reads bytes, normalises to LF, edits, and writes back with the file's own EOL; end commit messages with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; `ruff check .` and `ruff format --check .` from `physure-python/` must pass.

---

## File structure

| File | Change |
|---|---|
| `physure-core/src/units/registry.rs` | `dimension_base`; `add_level`; `factor_between` refuses a level |
| `physure-core/src/units/conf.rs` | `parse_conf`, `extend_registry`, strict `build_registry_with`, `$`, levels of a quantity |
| `physure-core/src/quantity.rs` | `combine_levels` and `base_display` for a level of a quantity |
| `physure-core/src/units/tests.rs` | the tests |
| `physure-python/src/lib.rs`, `physure-python/physure/_core.pyi` | `extend_conf`, strict `from_conf_text` |
| `physure-python/tests/core/test_registry_extend.py` | new |
| `CHANGELOG.md`, the stage 2 design | the entries |

---

### Task 1: a later text starts from the base units of the first; `$` is a dimension symbol

**Files:** `physure-core/src/units/registry.rs`, `physure-core/src/units/conf.rs`, `physure-core/src/units/tests.rs`.

- [ ] **Step 1: Write the failing test.** Append to `physure-core/src/units/tests.rs`:

```rust
/// A text parsed after another starts from the base units of the first: money is a dimension
/// (`$`) whose base unit is the first unit at scale 1, a second unit of it is scaled from that
/// one, and a second unit at scale 1 on a dimension that has a base unit is a name for it.
#[test]
fn a_later_text_starts_from_the_base_units_of_the_first() {
    use crate::units::parser::Parser;
    let (mut reg, mut constants) = conf::build_registry_from_conf();
    conf::parse_physure_conf("[Units]\nUSD = 1.0, $, noprefix, [USD, dollar]\n", &mut reg, &mut constants);
    conf::parse_physure_conf("[Units]\nEUR = 1.08, $, noprefix, [EUR, euro]\nsmoot = 1.0, L, [smoot]\n", &mut reg, &mut constants);
    let atomic = |text: &str| Parser::parse_expression_atomic(text).unwrap();
    assert_eq!(reg.factor_between(&atomic("EUR"), &atomic("USD")).unwrap(), 1.08);
    assert_eq!(reg.facts("EUR").unwrap().dimension, vec![("$".to_string(), (1, 1))]);
    assert_eq!(reg.factor_between(&atomic("smoot"), &atomic("m")).unwrap(), 1.0);
    assert_eq!(reg.dimension_base.get("L").map(String::as_str), Some("m"));
    assert_eq!(reg.dimension_base.get("$").map(String::as_str), Some("USD"));
}
```

- [ ] **Step 2: See it fail.** From `physure-core/`: `cargo test -p physure --lib a_later_text_starts_from`. Expected: FAIL to compile (`no field dimension_base`).

- [ ] **Step 3: Implement.**

In `registry.rs`, in `pub struct UnitRegistry`, after the field `pub base_dimension: HashMap<String, String>,` add:

```rust
    /// The base unit of each dimension symbol (`L` is `m`, `$` is `USD`): the first one declared,
    /// kept so that a text parsed later can declare units of a dimension without declaring its base
    /// unit again, and cannot make a second base unit of it by accident.
    pub dimension_base: HashMap<String, String>,
```

and in `UnitRegistry::new()`, after `base_dimension: HashMap::new(),` add `            dimension_base: HashMap::new(),`.

In `conf.rs`:

Find: `    let mut dim_to_base: HashMap<String, String> = HashMap::new();`
Replace with: `    let mut dim_to_base: HashMap<String, String> = registry.dimension_base.clone();`

Find (the line that opens pass 2):
```rust
    // Pass 2: Parse and register all units using the dynamic dimension map
```
Replace with:
```rust
    registry.dimension_base = dim_to_base.clone();

    // Pass 2: Parse and register all units using the dynamic dimension map
```

Find: `    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')`
Replace with: `    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')`

- [ ] **Step 4: See it pass.** `cargo test -p physure` (all green).

- [ ] **Step 5: Commit.**

```bash
git add physure-core/src/units/registry.rs physure-core/src/units/conf.rs physure-core/src/units/tests.rs
git commit -m "feat(core): a text parsed later starts from the base units of the first

The registry keeps the base unit of each dimension symbol, and the
parser starts from it: a second text can declare units of a dimension
(money, \`\$\`, is now a dimension symbol) without declaring its base
unit again, and a second unit at scale 1 on a dimension that has one is
a name for it, not a base unit of its own.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: reading a system's definitions strictly

**Files:** `physure-core/src/units/conf.rs`, `physure-core/src/units/tests.rs`.

- [ ] **Step 1: Write the failing tests.** Append to `physure-core/src/units/tests.rs`:

```rust
/// What a system is made of is read all or nothing: a line the parser cannot read is a problem,
/// nothing of a refused text is kept, and the embedded catalog has no line the parser refuses.
#[test]
fn extending_a_registry_is_all_or_nothing_and_the_catalog_has_no_unreadable_line() {
    use std::collections::HashMap;
    let (mut catalog, mut catalog_constants) = (UnitRegistry::new(), HashMap::new());
    let outcome = conf::extend_registry(&mut catalog, &mut catalog_constants, conf::DEFAULT_PHYSURE_CONF);
    assert!(outcome.is_ok(), "{:?}", outcome.err());

    let (mut reg, mut constants) = conf::build_registry_from_conf();
    let before = reg.clone();
    let text = "[Units]\nfurlong = 201.168, L, [fur, furlong]\nleague = lots, L, [lg]\nbad line\n[Prefixes]\nzorro = zz, many\n";
    let problems = conf::extend_registry(&mut reg, &mut constants, text).unwrap_err();
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems.iter().any(|p| p.contains("league") && p.contains("is not a number")), "{problems:?}");
    assert!(problems.iter().any(|p| p.contains("bad line")), "{problems:?}");
    assert!(problems.iter().any(|p| p.contains("zorro")), "{problems:?}");
    assert!(reg == before, "nothing of a refused text is kept, not even its good line");

    let accepted = conf::extend_registry(&mut reg, &mut constants, "[Units]\nfurlong = 201.168, L, [fur, furlong]\n");
    assert!(accepted.is_ok());
    assert!(reg.facts("fur").is_ok());

    // a unit whose dimension cannot be read is a problem too, and its names are not registered
    let unreadable = conf::extend_registry(&mut reg, &mut constants, "[Units]\nsmurf = 2.0, Q, [smf]\n");
    assert!(unreadable.is_err());
    assert!(reg.facts("smf").is_err());
}

/// The settings of a conf are process-wide, and a registry does not own them: extending one does
/// not apply a `[Settings]` section.
#[test]
fn extending_a_registry_does_not_apply_its_settings() {
    let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (mut reg, mut constants) = conf::build_registry_from_conf();
    let before = crate::settings::max_loop_iterations();
    conf::extend_registry(&mut reg, &mut constants, "[Settings]\nmax_loop_iterations = 7\n").unwrap();
    assert_eq!(crate::settings::max_loop_iterations(), before);
}
```

Also in the existing test `a_registry_can_be_built_from_the_catalog_plus_extra_text`, make the two calls of `conf::build_registry_with` unwrap the result:

Find: `let (reg, _) = conf::build_registry_with(extra);`
Replace with: `let (reg, _) = conf::build_registry_with(extra).unwrap();`

Find: `let (redefined, _) = conf::build_registry_with("[Units]\nfoot = 0.3, L, [ft, foot]\n");`
Replace with: `let (redefined, _) = conf::build_registry_with("[Units]\nfoot = 0.3, L, [ft, foot]\n").unwrap();`

- [ ] **Step 2: See them fail.** `cargo test -p physure --lib extending_a_registry`. Expected: FAIL to compile (`cannot find function extend_registry`).

- [ ] **Step 3: Implement** in `physure-core/src/units/conf.rs`. Each replacement below is an exact find / replace.

(a) Find:
```rust
pub fn parse_physure_conf(
    conf_str: &str,
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
) {
    let mut current_section = "";
```
Replace with:
```rust
pub fn parse_physure_conf(
    conf_str: &str,
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
) {
    // A line the parser cannot read is skipped, as it always was: this is the lenient reading of
    // the catalog and of the `physure.conf` of the working directory.
    parse_conf(conf_str, registry, constants, true, &mut Vec::new());
}

/// Adds the units, prefixes, constants and categories of `conf_str` to `registry`, all or nothing:
/// a line the parser cannot read is a problem, and if there is one the registry and the constants
/// are left as they were and every problem is returned. `[Settings]` is not read: the settings of a
/// conf are process-wide and a registry does not own them. This is the reading to use for the
/// definitions a unit system is made of.
pub fn extend_registry(
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
    conf_str: &str,
) -> Result<(), Vec<String>> {
    let backup = (registry.clone(), constants.clone());
    let mut problems = Vec::new();
    parse_conf(conf_str, registry, constants, false, &mut problems);
    if problems.is_empty() {
        return Ok(());
    }
    (*registry, *constants) = backup;
    Err(problems)
}

fn parse_conf(
    conf_str: &str,
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
    apply_settings: bool,
    problems: &mut Vec<String>,
) {
    let mut current_section = "";
```

(b) Find: `            "Settings" => {`
Replace with: `            "Settings" if apply_settings => {`

(c) Find:
```rust
                        if let Ok(factor) = parts[1].parse::<f64>() {
                            registry.add_prefix_exact(symbol.to_string(), factor, crate::units::exact::ExactScale::parse(parts[1]));
                            registry.add_prefix_name(name.trim().to_string(), symbol.to_string());
                        }
                    }
```
Replace with:
```rust
                        if let Ok(factor) = parts[1].parse::<f64>() {
                            registry.add_prefix_exact(symbol.to_string(), factor, crate::units::exact::ExactScale::parse(parts[1]));
                            registry.add_prefix_name(name.trim().to_string(), symbol.to_string());
                        } else {
                            problems.push(format!("{line}: the factor of a prefix is a number, not '{}'", parts[1]));
                        }
                    } else {
                        problems.push(format!("{line}: a prefix is `name = symbol, factor`"));
                    }
```

(d) Find:
```rust
    for line in unit_lines {
        parse_unit_line(line, registry, &dim_to_base);
    }
```
Replace with:
```rust
    for line in unit_lines {
        if let Err(reason) = parse_unit_line(line, registry, &dim_to_base) {
            problems.push(format!("{line}: {reason}"));
        }
    }
```

(e) Find: `fn parse_unit_line(line: &str, registry: &mut UnitRegistry, dim_to_base: &HashMap<String, String>) {`
Replace with: `fn parse_unit_line(line: &str, registry: &mut UnitRegistry, dim_to_base: &HashMap<String, String>) -> Result<(), String> {`

(f) Find:
```rust
        None => return,
    };

    let mut aliases: Vec<String> = if !aliases_str.is_empty() {
```
Replace with:
```rust
        None => return Err("a unit line is `name = factor, dimension, [names]`".to_string()),
    };

    let mut aliases: Vec<String> = if !aliases_str.is_empty() {
```

(g) Find:
```rust
    if parts.is_empty() {
        return;
    }
```
Replace with:
```rust
    if parts.is_empty() {
        return Err("the line has no value".to_string());
    }
```

(h) Find:
```rust
            registry.add_log_unit(symbol.clone(), factor, reference);
            register_aliases(registry, key, &symbol, &aliases, &names);
        }
        return;
    }
```
Replace with:
```rust
            registry.add_log_unit(symbol.clone(), factor, reference);
            register_aliases(registry, key, &symbol, &aliases, &names);
            return Ok(());
        }
        return Err("a level is `Log, factor, reference, dimension`".to_string());
    }
```

(i) Find:
```rust
        Some(parsed) => parsed,
        None => return,
    };
```
Replace with:
```rust
        Some(parsed) => parsed,
        None => return Err(format!("'{}' is not a number", parts[0])),
    };
```

(j) Find:
```rust
            registry.add_derived_unit(symbol.clone(), u);
        }
    }

    register_aliases(registry, key, &symbol, &aliases, &names);
}
```
Replace with:
```rust
            registry.add_derived_unit(symbol.clone(), u);
        } else {
            return Err(format!("cannot read the dimension '{dim_str}' of {symbol}, or its recipe"));
        }
    }

    register_aliases(registry, key, &symbol, &aliases, &names);
    Ok(())
}
```

(k) Replace the whole of `build_registry_with` (its doc comment included, from `/// The embedded catalog plus \`extra\`` to the closing `}` of the function):
```rust
/// The embedded catalog plus `extra`, the text of a system file or of a user conf, read strictly
/// (`extend_registry`) after it, so that a later definition wins. The `physure.conf` of the working
/// directory is not read: the caller decides what a system is made of. A line the parser cannot
/// read is an error that lists every such line, and `[Settings]` is not applied.
pub fn build_registry_with(extra: &str) -> Result<(UnitRegistry, HashMap<String, String>), Vec<String>> {
    let (mut reg, mut constants) = embedded_catalog().clone();
    extend_registry(&mut reg, &mut constants, extra)?;
    Ok((reg, constants))
}
```

- [ ] **Step 4: Keep the extension building.** `build_registry_with` returns a `Result` now and `physure-python/src/lib.rs` calls it, so the extension (a workspace member) would not compile at this commit: make `from_conf_text` strict too. In `physure-python/src/lib.rs`:

Find (the helper added in step 1):
```rust
fn value_error(e: PhysureError) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(e.to_string())
}
```
Replace with:
```rust
fn value_error(e: PhysureError) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(e.to_string())
}

/// The lines of a conf that the core could not read, one per line of the message.
fn conf_problems(problems: Vec<String>) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(problems.join("\n"))
}
```

Find:
```rust
    fn from_conf_text(extra: &str) -> Self {
        let (reg, _constants) = ::physure_core::units::conf::build_registry_with(extra);
        PyUnitRegistry(reg)
    }
```
Replace with:
```rust
    fn from_conf_text(extra: &str) -> PyResult<Self> {
        let (reg, _constants) = ::physure_core::units::conf::build_registry_with(extra).map_err(conf_problems)?;
        Ok(PyUnitRegistry(reg))
    }
```

- [ ] **Step 5: See them pass.** `cargo test -p physure` (all green, including the three settings tests and the step 1 test) and, from the repository root, `cargo check -p physure-python` (builds).

- [ ] **Step 6: Commit.**

```bash
git add physure-core/src/units/conf.rs physure-core/src/units/tests.rs physure-python/src/lib.rs
git commit -m "feat(core): read what a system is made of all or nothing

parse_physure_conf is the lenient reading, as it was. extend_registry is
the strict one: a line the parser cannot read (a factor that is not a
number, a dimension or recipe that does not parse, a malformed prefix)
is a problem; if there is one the registry is left as it was and every
problem is returned. A unit line that fails no longer registers its
names. [Settings] is not applied by it, since the settings of a conf are
process-wide and a registry does not own them. The embedded catalog has
no line the parser refuses, and a test says so. build_registry_with is
the strict one, and so is from_conf_text of the extension, which raises
ValueError listing every such line.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: a level can be a level of a quantity

**Files:** `physure-core/src/units/registry.rs`, `physure-core/src/units/conf.rs`, `physure-core/src/quantity.rs`, `physure-core/src/units/tests.rs`.

- [ ] **Step 1: Write the failing test.** Append to `physure-core/src/units/tests.rs`:

```rust
/// A level can be the level of a quantity: `dBW` is 10 * log10(P / 1 W) and has the dimensions of a
/// power, `dBm` is the same against a milliwatt.
#[test]
fn a_level_can_have_a_dimension() {
    use crate::quantity::Quantity;
    use crate::units::parser::Parser;
    let (mut reg, mut constants) = conf::build_registry_from_conf();
    let text = "[Units]\ndBW = Log, 10, 1.0, M*L^2*T^-3, [dBW]\ndBm = Log, 10, 0.001, M*L^2*T^-3, [dBm]\n";
    conf::extend_registry(&mut reg, &mut constants, text).unwrap();
    let (dbw, dbm, watt) = (reg.get_unit("dBW").unwrap(), reg.get_unit("dBm").unwrap(), reg.get_unit("W").unwrap());
    assert!(dbw.is_logarithmic() && dbm.is_logarithmic());
    assert!(dbw.same_dimensions(&watt));
    let q = |value: f64, unit: &RationalUnit| Quantity::new_scalar(value, 0.0, unit.clone(), None, None);
    let mean = |quantity: Quantity| quantity.value.mean();
    assert!((mean(q(30.0, &dbw).convert_to(&watt).unwrap()) - 1000.0).abs() < 1e-9);
    assert!((mean(q(0.0, &dbm).convert_to(&watt).unwrap()) - 0.001).abs() < 1e-15);
    assert!(mean(q(30.0, &dbm).convert_to(&dbw).unwrap()).abs() < 1e-9);
    assert!((mean(q(1000.0, &watt).convert_to(&dbw).unwrap()) - 30.0).abs() < 1e-9);
    // levels of a power add as the powers: 10 dBW + 10 dBW is 20 W
    let sum = q(10.0, &dbw).add(&q(10.0, &dbw)).unwrap();
    assert!(sum.unit.is_logarithmic());
    assert!((mean(sum) - 10.0 * 20f64.log10()).abs() < 1e-9);
    // not a plain quantity, not another dimension, and no factor between units
    assert!(q(10.0, &dbw).add(&q(1.0, &watt)).is_err());
    assert!(q(1.0, &dbw).convert_to(&reg.get_unit("m").unwrap()).is_err());
    let atomic = |text: &str| Parser::parse_expression_atomic(text).unwrap();
    assert!(reg.factor_between(&atomic("dBW"), &atomic("W")).is_err());
    let facts = reg.facts("dBW").unwrap();
    let expected: Vec<(String, (i64, i64))> = vec![("L".into(), (2, 1)), ("M".into(), (1, 1)), ("T".into(), (-3, 1))];
    assert_eq!(facts.dimension, expected);
    assert!(!facts.is_linear);
}
```

- [ ] **Step 2: See it fail.** `cargo test -p physure --lib a_level_can_have_a_dimension`. Expected: FAIL at the `extend_registry(...).unwrap()` ("a level is `Log, factor, reference, dimension`"): the dimension has to be `1` today.

- [ ] **Step 3: Implement.**

In `registry.rs`, find the whole of `add_log_unit`:
```rust
    /// Registers a level: `add_log_unit("dB", 10.0, 1.0)` is `10 * log10(ratio / 1)`.
    pub fn add_log_unit(&mut self, name: String, factor: f64, reference: f64) {
        let mut u = RationalUnit::dimensionless().with_log(LogScale { factor, reference });
        u.display_name = Some(name.clone());
        self.derived_units.insert(name, u);
    }
```
Replace with:
```rust
    /// Registers a level of a plain ratio: `add_log_unit("dB", 10.0, 1.0)` is `10 * log10(ratio / 1)`.
    pub fn add_log_unit(&mut self, name: String, factor: f64, reference: f64) {
        self.add_level(name, RationalUnit::dimensionless(), factor, reference);
    }

    /// Registers a level of a quantity: `dBW` is `10 * log10(P / 1 W)`, a level with the dimensions
    /// of `quantity` (its scale and zero point are ignored) whose reference is in base units.
    pub fn add_level(&mut self, name: String, quantity: RationalUnit, factor: f64, reference: f64) {
        let mut u = quantity.with_unit_scale().with_offset(0.0).with_log(LogScale { factor, reference });
        u.display_name = Some(name.clone());
        self.derived_units.insert(name, u);
    }
```

In `registry.rs`, in `factor_between`, find:
```rust
        let (from, to) = (self.expand(from)?, self.expand(to)?);
        if !from.same_dimensions(&to) {
```
Replace with:
```rust
        let (from, to) = (self.expand(from)?, self.expand(to)?);
        if from.is_logarithmic() || to.is_logarithmic() {
            return Err(PhysureError::Generic(format!(
                "a level ('{}' to '{}') has no factor: it converts through the logarithm of a ratio",
                from.__repr__(),
                to.__repr__()
            )));
        }
        if !from.same_dimensions(&to) {
```

In `conf.rs`, find the whole level block of `parse_unit_line` (as left by Task 2 (h)), its comment included:
```rust
    // `pH = Log, -1, 1.0, 1, noprefix`: a level is `Log, factor, reference, dimension`, and
    // its dimension is the dimensionless "1", because it is the logarithm of a ratio.
    if parts[0].eq_ignore_ascii_case("log") {
        let number = |index: usize| parts.get(index).and_then(|text| text.parse::<f64>().ok());
        if let (Some(factor), Some(reference), Some("1")) = (number(1), number(2), parts.get(3).copied()) {
            registry.add_log_unit(symbol.clone(), factor, reference);
            register_aliases(registry, key, &symbol, &aliases, &names);
            return Ok(());
        }
        return Err("a level is `Log, factor, reference, dimension`".to_string());
    }
```
Replace with:
```rust
    // `pH = Log, -1, 1.0, 1, noprefix`: a level is `Log, factor, reference, dimension`. Its dimension
    // is "1" when it is the logarithm of a ratio, or the dimensions of a quantity (`dBW`).
    if parts[0].eq_ignore_ascii_case("log") {
        let number = |index: usize| parts.get(index).and_then(|text| text.parse::<f64>().ok());
        let (Some(factor), Some(reference), Some(dimension)) = (number(1), number(2), parts.get(3).copied()) else {
            return Err("a level is `Log, factor, reference, dimension`".to_string());
        };
        // `1` is a level of a plain ratio; any other dimension is a level of a quantity (`dBW`)
        let quantity = if dimension == "1" {
            RationalUnit::dimensionless()
        } else {
            let normalized = normalize_dimension_symbols(dimension, dim_to_base);
            crate::units::parser::Parser::parse_expression_with_registry(&normalized, registry)
                .map_err(|_| format!("cannot read the dimension '{dimension}' of the level {symbol}"))?
        };
        registry.add_level(symbol.clone(), quantity, factor, reference);
        register_aliases(registry, key, &symbol, &aliases, &names);
        return Ok(());
    }
```

In `quantity.rs`, in `combine_levels`, find:
```rust
        let ratios = RationalUnit::dimensionless();
        let (a, b) = (self.convert_to(&ratios)?, other.convert_to(&ratios)?);
```
Replace with:
```rust
        if !self.unit.same_dimensions(&other.unit) {
            return Err(PhysureError::UnitMismatch {
                expected: self.unit.__repr__(),
                actual: other.unit.__repr__(),
            });
        }
        // the plain quantity both stand for: a ratio for a level of a ratio, a power for a level of one
        let plain = RationalUnit::new_from_dimensions(self.unit.dimensions.iter().cloned());
        let (a, b) = (self.convert_to(&plain)?, other.convert_to(&plain)?);
```

In `quantity.rs`, in `base_display`, find:
```rust
            if let Ok(ratio) = self.convert_to(&RationalUnit::dimensionless()) {
                return ratio.base_display();
            }
```
Replace with:
```rust
            let plain = RationalUnit::new_from_dimensions(self.unit.dimensions.iter().cloned());
            if let Ok(value) = self.convert_to(&plain) {
                return value.base_display();
            }
```

- [ ] **Step 4: See it pass.** `cargo test -p physure` (all green, including the existing tests of `dB` and `pH`).

- [ ] **Step 5: Commit.**

```bash
git add physure-core/src/units/registry.rs physure-core/src/units/conf.rs physure-core/src/quantity.rs physure-core/src/units/tests.rs
git commit -m "feat(core): a level can be the level of a quantity

\`dBW = Log, 10, 1.0, M*L^2*T^-3\` is 10 * log10(P / 1 W): a level with the
dimensions of a power whose reference is in base units. It converts to
and from watts through the logarithm, with the uncertainty of the
derivative, and levels of a power add as the powers. Python registers
such a unit (the tests register dBW on the dimension of a power); until
now the core could only hold levels of a plain ratio (dB, pH).

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: the extension takes the definitions of a system

**Files:** `physure-python/src/lib.rs`, `physure-python/physure/_core.pyi`, `physure-python/tests/core/test_registry_extend.py`.

- [ ] **Step 1: Write the tests.** Create `physure-python/tests/core/test_registry_extend.py`:

```python
"""A1 stage 2, step 2a: a registry takes the definitions of a system, strictly."""

import pytest

from physure._core import UnitRegistry

MONEY = (
    "[Units]\n"
    "USD = 1.0, $, noprefix, [USD, dollar, dollars]\n"
    "EUR = 1.08, $, noprefix, [EUR, euro, euros]\n"
)


def _extended(text):
    registry = UnitRegistry.from_conf()
    registry.extend_conf(text)
    return registry


def test_money_is_a_dimension_the_registry_can_scale():
    registry = _extended(MONEY)
    assert registry.facts("EUR")["dimension"] == {"$": 1}
    factor = registry.factor_between(
        registry.parse_atomic("EUR"), registry.parse_atomic("USD")
    )
    assert factor == 1.08
    assert registry.alias_of(registry.parse_atomic("euros")) == "euros"


def test_a_level_of_a_power_has_the_dimension_of_a_power():
    registry = _extended("[Units]\ndBW = Log, 10, 1.0, M*L^2*T^-3, [dBW]\n")
    facts = registry.facts("dBW")
    assert facts["dimension"] == {"L": 2, "M": 1, "T": -3}
    assert facts["is_linear"] is False


def test_a_line_the_core_cannot_read_is_an_error_and_keeps_nothing():
    registry = UnitRegistry.from_conf()
    text = "[Units]\nfurlong = 201.168, L, [fur]\nleague = lots, L, [lg]\n"
    with pytest.raises(ValueError, match="is not a number"):
        registry.extend_conf(text)
    with pytest.raises(ValueError, match="Unknown unit"):
        registry.facts("fur")


def test_from_conf_text_is_strict_too():
    with pytest.raises(ValueError, match="is not a number"):
        UnitRegistry.from_conf_text("[Units]\nleague = lots, L, [lg]\n")
    registry = UnitRegistry.from_conf_text(MONEY)
    assert registry.facts("euro")["dimension"] == {"$": 1}


def test_a_registry_of_its_own_knows_what_it_is_given_and_the_si_base_units():
    registry = UnitRegistry()
    with pytest.raises(ValueError, match="Unknown unit"):
        registry.facts("ft")
    registry.extend_conf(
        "[Units]\nmetre = 1.0, L, [m, metre]\nfoot = 0.3048, L, [ft]\n"
    )
    assert registry.factor_between(
        registry.parse_atomic("ft"), registry.parse_atomic("m")
    ) == pytest.approx(0.3048)
    # any text parsed also registers the eight SI base units, whether or not it mentions them
    assert registry.facts("kg")["dimension"] == {"M": 1}
    with pytest.raises(ValueError, match="Unknown unit"):
        registry.facts("lb")
```

- [ ] **Step 2: Add the extension method.** In `physure-python/src/lib.rs` (`from_conf_text` is already strict, from Task 2):

Find:
```rust
    fn from_conf_text(extra: &str) -> PyResult<Self> {
        let (reg, _constants) = ::physure_core::units::conf::build_registry_with(extra).map_err(conf_problems)?;
        Ok(PyUnitRegistry(reg))
    }
```
Replace with:
```rust
    fn from_conf_text(extra: &str) -> PyResult<Self> {
        let (reg, _constants) = ::physure_core::units::conf::build_registry_with(extra).map_err(conf_problems)?;
        Ok(PyUnitRegistry(reg))
    }

    /// Adds the units, prefixes and levels of conf text to this registry, all or nothing: a line the
    /// core cannot read raises ValueError listing each, and the registry is left as it was.
    fn extend_conf(&mut self, text: &str) -> PyResult<()> {
        let mut constants = HashMap::new();
        ::physure_core::units::conf::extend_registry(&mut self.0, &mut constants, text).map_err(conf_problems)
    }
```

In `physure-python/physure/_core.pyi`, find `    def from_conf_text(extra: str) -> UnitRegistry: ...` and add after it:
```python
    def extend_conf(self, text: str) -> None: ...
```

- [ ] **Step 3: Build and run.** From `physure-python/` (`unset VIRTUAL_ENV`): `uv run --no-sync maturin develop --release`, check that `uv run --no-sync python -X utf8 -c "import physure._core as c; print(c.__file__)"` prints a path inside this worktree, then `uv run --no-sync pytest tests/core/test_registry_extend.py tests/core/test_registry_facts.py -q -p no:cacheprovider`. Expected: all pass (5 new tests, the 44 of step 1). Then `uv run --no-sync ruff format tests/core/test_registry_extend.py`, `uv run --no-sync ruff check .`, `uv run --no-sync ruff format --check .`: clean.

- [ ] **Step 4: Commit.**

```bash
git add physure-python/src/lib.rs physure-python/physure/_core.pyi physure-python/tests/core/test_registry_extend.py
git commit -m "feat: a registry of the extension takes the definitions of a system

UnitRegistry.extend_conf adds the units, prefixes and levels of conf text
all or nothing, raising ValueError with every line the core could not
read, as from_conf_text has since the core reads strictly. Tests of both,
of money as a dimension and of dBW through the extension.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: changelog, design pointer and the full verification

**Files:** `CHANGELOG.md`, `docs/superpowers/specs/2026-10-29-a1-stage2-design.md`.

- [ ] **Step 1: Changelog.** Under `## [Unreleased]` / `### Changed`, as the last bullet before `### Fixed`:

```markdown
- **The core takes a system's definitions strictly.** `UnitRegistry.extend_conf(text)` (and `from_conf_text`) adds units, prefixes and levels all or nothing and raises with every line the core cannot read; the embedded catalog has none. A text parsed after another starts from the base units of the first, so money (`$`) can be a dimension of its own (`EUR = 1.08, $` is scaled from `USD`) and a second unit at scale 1 on a dimension that has a base unit is a name for it, not a new base unit. `[Settings]` is not applied by it. A level can be the level of a quantity: `dBW = Log, 10, 1.0, M*L^2*T^-3` converts to watts through the logarithm and levels of a power add as the powers.
```

Under `### Fixed`, as the last bullet before `## [0.2.4]`:

```markdown
- **A unit line the core could not read no longer registers its names.** A line whose dimension and recipe did not parse was dropped, but its aliases were registered pointing at nothing.
```

- [ ] **Step 2: Design pointer.** Append to `docs/superpowers/specs/2026-10-29-a1-stage2-design.md`:

```markdown

## Outcome of step 2a (the core takes a system's definitions strictly)

- `extend_registry` is the strict reading of what a system is made of and `parse_physure_conf` stays the lenient one (the catalog and the `physure.conf` of the working directory). The embedded catalog has no line the parser refuses, and a test says so, which is what lets the strict reading be used for the system file as well.
- The registry keeps the base unit of each dimension symbol (`dimension_base`) and a text parsed later starts from it; `$` is a dimension symbol, so `enable_currency` is expressible as text.
- A level can have the dimensions of a quantity (`dBW`, `dBm`), converts to the plain unit of those dimensions through the logarithm, and has no conversion factor between units.
- Any text parsed, strict or lenient, also registers the eight SI base units (`m`, `s`, `kg`, `A`, `K`, `mol`, `cd`, `rad`) and the default dimension symbols of them (`L`, `M`, `T`, `I`, `O`, `N`, `J`, `A`), whether or not the text mentions them. A registry of a bare system therefore knows `kg`; step 2b must not use the registry's membership to answer `in` for a system, which stays the system's own until step 7.
- Step 2b (Python) is planned from here.
```

- [ ] **Step 3: Verify everything.** From the repository root of the worktree: `cargo test --workspace --exclude physure-python --no-fail-fast` (all `test result` lines ok). From `physure-python/`: `uv run --no-sync ruff check .`, `uv run --no-sync ruff format --check .`, `uv run --no-sync pytest -q -p no:cacheprovider` (all green, the same five xfails; 1145 passed), `uv run --no-sync mkdocs build --strict`. The benchmark is not affected (nothing in Python changes), so it is not run here.

- [ ] **Step 4: Commit.**

```bash
git add CHANGELOG.md docs/superpowers/specs/2026-10-29-a1-stage2-design.md
git commit -m "docs: changelog and outcome of step 2a (the core takes a system's definitions strictly)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-review against the design

- **Coverage:** the five findings of "Step 2, refined" that belong to the core: bases that persist and `$` (Task 1), strict all-or-nothing reading with problems and the catalog guard (Task 2), levels of a quantity (Task 3), the extension's `extend_conf` (Task 4). The mirror's faults and the lookup path are removed in step 2b, which is Python.
- **Placeholders:** none; every edit is an exact find and replace, and each says which task it follows (Task 3's find is the text Task 2 leaves).
- **Type consistency:** `extend_registry` returns `Result<(), Vec<String>>`; `build_registry_with` returns `Result<(UnitRegistry, HashMap<String, String>), Vec<String>>` and its two callers (the step 1 test, `from_conf_text`) unwrap or map it; `add_level(name, quantity: RationalUnit, factor, reference)` is what the level block calls.
- **Out of scope here:** the translator from `register_unit` to text, `UnitSystem` owning its registry, pickling, the differential test of a system (step 2b).
