# physure-wasm Implementation Plan

**For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a new `physure-wasm` Cargo workspace crate that exposes `Quantity`, `UnitRegistry`, and `PhyFunction` to JS/TS via wasm-bindgen, publishing to npm as `physure`, matching `physure-java`'s public API surface.

**Architecture:** Thin `#[wasm_bindgen]` wrapper structs around `physure_core::Quantity`/`UnitRegistry` and hand-written PHS-source-string composition (the same technique `physure-java`'s JNI layer and Java's `PhyFunction`/`UnitRegistry` use) — no JNI-style handle boxing, since wasm-bindgen manages object lifetimes itself. `UnitRegistry` and every `PhyFunction` built from it share one `Rc<RefCell<RegistryState>>` so registering a function mutates the same interpreter `.evaluate()` reads from, exactly like Java's `PhyFunction` holding a reference to its `UnitRegistry`.

**Tech Stack:** Rust, wasm-bindgen 0.2, js-sys 0.3, console_error_panic_hook 0.1, wasm-bindgen-test 0.3, wasm-pack, Node.js (test runner).

**Design note (deviation from spec wording):** the spec's `.call(...args: (Quantity | string)[]): Quantity` is implemented as `#[wasm_bindgen(variadic)] pub fn call(&self, args: Vec<JsValue>)`. wasm-bindgen has no clean way to express a `(Quantity | string)[]` variadic parameter type in the generated `.d.ts` for a class method (that machinery exists for free functions via `typescript_custom_section`, not for overriding one method's signature inside an auto-generated `class` block) — the trade-off is the emitted `.d.ts` shows `call(...args: any[]): Quantity`. Runtime behavior matches the spec exactly (each arg may be a `Quantity` instance or a string); only the static typing precision is reduced. Flagging this now rather than silently shipping a narrower spec than approved.

**Key finding from codebase research:** `physure-script/src/function.rs` defines a `PhyFunction` struct, but `physure-script/src/lib.rs` never declares `pub mod function;` — it is dead code, not part of the compiled public API, and it has a latent bug (`interp.get_user_fn(...)` — no such method exists on `PhsInterpreter`; it silently never causes a build failure only because the module isn't compiled). This plan does **not** depend on or fix that file — `physure-wasm`'s `PhyFunction` is a fresh implementation modeled on `physure-java`'s `PhyFunction.java`/`UnitRegistry.java` (which share one live interpreter per registry, not a clone-per-function), verified against `physure-script/src/function.rs`'s test-proven output values (`test_phy_function_parity_rust`, `test_phy_function_trig_and_exp_calculus`) for the exact naming convention (`d_{name}_d_{var}`, `i_{name}_d_{var}`, `solve_{name}_for_{var}`) and numeric results.

---

### Task 1: Crate scaffold + toolchain

**Files:**
- Modify: `Cargo.toml:9-16` (add `"physure-wasm"` to `[workspace] members`)
- Create: `physure-wasm/Cargo.toml`
- Create: `physure-wasm/src/lib.rs`

- [ ] **Step 1: Install the wasm target and wasm-pack**

Run:
```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
```
Expected: both commands succeed (`wasm-pack --version` then prints a version).

- [ ] **Step 2: Add the workspace member**

Modify `Cargo.toml`:
```toml
[workspace]
resolver = "2"
members  = [
    "physure-core",
    "physure-script",
    "physure-cli",
    "physure-python",
    "physure-lsp",
    "physure-java",
    "physure-wasm",
]
```

- [ ] **Step 3: Create the crate**

Create `physure-wasm/Cargo.toml`:
```toml
[package]
name = "physure-wasm"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
description = "WebAssembly/JS/TS bindings for the physure core physics engine."

[lib]
name = "physure_wasm"
crate-type = ["cdylib", "rlib"]

[dependencies]
physure-core = { workspace = true }
physure-script = { workspace = true }
wasm-bindgen = "0.2"
js-sys = "0.3"
console_error_panic_hook = "0.1"

[dev-dependencies]
wasm-bindgen-test = "0.3"
```

Create `physure-wasm/src/lib.rs`:
```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[cfg(test)]
mod tests {
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node);

    #[wasm_bindgen_test]
    fn wasm_pack_test_harness_runs() {
        assert_eq!(2 + 2, 4);
    }
}
```

- [ ] **Step 4: Verify the crate builds and the test harness runs**

Run:
```bash
cargo check -p physure-wasm
cd physure-wasm && wasm-pack test --node && cd ..
```
Expected: `cargo check` succeeds; `wasm-pack test --node` reports `test wasm_pack_test_harness_runs ... ok`, `1 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml physure-wasm/Cargo.toml physure-wasm/src/lib.rs
git commit -m "feat(wasm): scaffold physure-wasm crate"
```

---

### Task 2: Error conversion (`PhysureError` -> `JsValue`)

**Files:**
- Create: `physure-wasm/src/error.rs`
- Modify: `physure-wasm/src/lib.rs`

- [ ] **Step 1: Write failing test**

Create `physure-wasm/src/error.rs`:
```rust
#[cfg(test)]
mod tests {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node);

    #[wasm_bindgen_test]
    fn converts_a_physure_error_into_a_js_error_with_the_same_message() {
        let err = physure_core::error::PhysureError::DivisionByZero("velocity".to_string());
        let js_err = super::to_js_error(err);
        let js_error = js_err
            .dyn_into::<js_sys::Error>()
            .expect("should convert into a JS Error instance");
        assert_eq!(String::from(js_error.message()), "Division by zero: velocity");
    }
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: FAIL to compile — `cannot find function 'to_js_error' in module 'error'` (or similar E0425/E0433).

- [ ] **Step 3: Write minimal implementation**

Add above the `#[cfg(test)]` block in `physure-wasm/src/error.rs`:
```rust
use physure_core::error::PhysureError;
use wasm_bindgen::JsValue;

pub fn to_js_error(err: PhysureError) -> JsValue {
    js_sys::Error::new(&err.to_string()).into()
}
```

Modify `physure-wasm/src/lib.rs` (add before the existing `use wasm_bindgen::prelude::*;` line):
```rust
mod error;

use wasm_bindgen::prelude::*;
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: `test error::tests::converts_a_physure_error_into_a_js_error_with_the_same_message ... ok`.

- [ ] **Step 5: Commit**

```bash
git add physure-wasm/src/error.rs physure-wasm/src/lib.rs
git commit -m "feat(wasm): add PhysureError -> JsValue conversion"
```

---

### Task 3: `Quantity` — construction, getters, `toString`

**Files:**
- Create: `physure-wasm/src/quantity.rs`
- Modify: `physure-wasm/src/lib.rs`

- [ ] **Step 1: Write failing test**

Create `physure-wasm/src/quantity.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node);

    #[wasm_bindgen_test]
    fn constructs_a_quantity_and_exposes_its_getters() {
        let q = Quantity::new(75.0, "kg").unwrap();
        assert_eq!(q.value(), 75.0);
        assert_eq!(q.uncertainty(), 0.0);
        assert_eq!(q.unit(), "kg");
    }

    #[wasm_bindgen_test]
    fn constructs_a_quantity_with_uncertainty() {
        let q = Quantity::with_uncertainty(75.0, 0.5, "kg").unwrap();
        assert_eq!(q.value(), 75.0);
        assert_eq!(q.uncertainty(), 0.5);
    }

    #[wasm_bindgen_test]
    fn to_js_string_renders_value_and_unit() {
        let q = Quantity::new(5.0, "m/s").unwrap();
        assert_eq!(q.to_js_string(), "5 m/s");
    }

    #[wasm_bindgen_test]
    fn rejects_an_unknown_unit_with_a_catchable_error() {
        let err = Quantity::new(1.0, "not_a_real_unit").unwrap_err();
        let js_error = err
            .dyn_into::<js_sys::Error>()
            .expect("should be a JS Error");
        assert!(String::from(js_error.message()).contains("Unknown unit"));
    }
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: FAIL to compile — `cannot find struct 'Quantity' in this scope` (or similar E0433).

- [ ] **Step 3: Write minimal implementation**

Add above the `#[cfg(test)]` block in `physure-wasm/src/quantity.rs`:
```rust
use crate::error::to_js_error;
use physure_core::quantity::Quantity as CoreQuantity;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Quantity {
    pub(crate) inner: CoreQuantity,
}

#[wasm_bindgen]
impl Quantity {
    #[wasm_bindgen(constructor)]
    pub fn new(value: f64, unit: &str) -> Result<Quantity, JsValue> {
        CoreQuantity::new(value, unit)
            .map(Quantity::from_core)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = withUncertainty)]
    pub fn with_uncertainty(value: f64, uncertainty: f64, unit: &str) -> Result<Quantity, JsValue> {
        CoreQuantity::with_uncertainty(value, uncertainty, unit)
            .map(Quantity::from_core)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn to_js_string(&self) -> String {
        self.inner.to_string()
    }

    #[wasm_bindgen(getter)]
    pub fn value(&self) -> f64 {
        self.inner.value.mean()
    }

    #[wasm_bindgen(getter)]
    pub fn uncertainty(&self) -> f64 {
        self.inner.value.std_dev()
    }

    #[wasm_bindgen(getter)]
    pub fn unit(&self) -> String {
        self.inner.unit.__repr__()
    }
}

impl Quantity {
    pub(crate) fn from_core(inner: CoreQuantity) -> Self {
        Quantity { inner }
    }
}
```

Modify `physure-wasm/src/lib.rs`:
```rust
mod error;
mod quantity;

use wasm_bindgen::prelude::*;

pub use quantity::Quantity;
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: all 4 new tests pass, plus the 2 from earlier tasks (6 total).

- [ ] **Step 5: Commit**

```bash
git add physure-wasm/src/quantity.rs physure-wasm/src/lib.rs
git commit -m "feat(wasm): add Quantity construction, getters, toString"
```

---

### Task 4: `Quantity` — arithmetic, `pow`, `convertTo`

**Files:**
- Modify: `physure-wasm/src/quantity.rs`

- [ ] **Step 1: Write failing test**

Add to the `mod tests` block in `physure-wasm/src/quantity.rs`:
```rust
    #[wasm_bindgen_test]
    fn arithmetic_and_conversion_between_compatible_units() {
        let d = Quantity::new(10.0, "m").unwrap();
        let t = Quantity::new(2.0, "s").unwrap();
        let v = d.divide(&t).unwrap();
        assert_eq!(v.value(), 5.0);
        assert_eq!(v.unit(), "m/s");

        let v_kmh = v.convert_to("km/h").unwrap();
        assert!((v_kmh.value() - 18.0).abs() < 1e-9);

        let doubled = v.multiply(&Quantity::new(2.0, "").unwrap()).unwrap();
        assert_eq!(doubled.value(), 10.0);

        let squared = v.pow(2.0).unwrap();
        assert_eq!(squared.value(), 25.0);
        assert_eq!(squared.unit(), "m^2/s^2");
    }

    #[wasm_bindgen_test]
    fn add_and_subtract_require_compatible_dimensions() {
        let m = Quantity::new(5.0, "m").unwrap();
        let s = Quantity::new(5.0, "s").unwrap();
        let err = m.add(&s).unwrap_err();
        let js_error = err
            .dyn_into::<js_sys::Error>()
            .expect("should be a JS Error");
        assert!(String::from(js_error.message()).contains("Incompatible dimensions"));
    }
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: FAIL to compile — `no method named 'divide' found for struct 'Quantity'` (or similar E0599).

- [ ] **Step 3: Write minimal implementation**

Add inside the `#[wasm_bindgen] impl Quantity` block in `physure-wasm/src/quantity.rs` (after `unit()`):
```rust
    pub fn add(&self, other: &Quantity) -> Result<Quantity, JsValue> {
        self.inner.add(&other.inner).map(Quantity::from_core).map_err(to_js_error)
    }

    pub fn subtract(&self, other: &Quantity) -> Result<Quantity, JsValue> {
        self.inner.sub(&other.inner).map(Quantity::from_core).map_err(to_js_error)
    }

    pub fn multiply(&self, other: &Quantity) -> Result<Quantity, JsValue> {
        self.inner.mul(&other.inner).map(Quantity::from_core).map_err(to_js_error)
    }

    pub fn divide(&self, other: &Quantity) -> Result<Quantity, JsValue> {
        self.inner.div(&other.inner).map(Quantity::from_core).map_err(to_js_error)
    }

    pub fn pow(&self, exponent: f64) -> Result<Quantity, JsValue> {
        self.inner.pow(exponent).map(Quantity::from_core).map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = convertTo)]
    pub fn convert_to(&self, unit: &str) -> Result<Quantity, JsValue> {
        self.inner.to(unit).map(Quantity::from_core).map_err(to_js_error)
    }
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: all tests pass (8 total).

- [ ] **Step 5: Commit**

```bash
git add physure-wasm/src/quantity.rs
git commit -m "feat(wasm): add Quantity arithmetic, pow, convertTo"
```

---

### Task 5: `UnitRegistry` — construction + unit introspection

**Files:**
- Create: `physure-wasm/src/registry.rs`
- Modify: `physure-wasm/src/lib.rs`

- [ ] **Step 1: Write failing test**

Create `physure-wasm/src/registry.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node);

    #[wasm_bindgen_test]
    fn get_unit_exponents_returns_the_base_dimensions() {
        let reg = UnitRegistry::new();
        let map = reg.get_unit_exponents("kg*m/s^2").unwrap();
        assert_eq!(map.get(&"kg".into()).as_f64(), Some(1.0));
        assert_eq!(map.get(&"m".into()).as_f64(), Some(1.0));
        assert_eq!(map.get(&"s".into()).as_f64(), Some(-2.0));
    }

    #[wasm_bindgen_test]
    fn get_unit_scale_returns_the_si_scale_factor() {
        let reg = UnitRegistry::new();
        assert!((reg.get_unit_scale("cm").unwrap() - 0.01).abs() < 1e-9);
    }

    #[wasm_bindgen_test]
    fn get_categories_lists_known_categories() {
        let reg = UnitRegistry::new();
        let categories = reg.get_categories();
        assert!(categories.get(&"length".into()).is_array());
    }

    #[wasm_bindgen_test]
    fn from_content_applies_an_override_on_top_of_the_default_config() {
        let reg = UnitRegistry::from_content("[Units]\nfurlong = 201.168 m\n");
        let scale = reg.get_unit_scale("furlong").unwrap();
        assert!((scale - 201.168).abs() < 1e-6);
    }
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: FAIL to compile — `cannot find struct 'UnitRegistry' in this scope`.

- [ ] **Step 3: Write minimal implementation**

Add above the `#[cfg(test)]` block in `physure-wasm/src/registry.rs`:
```rust
use crate::error::to_js_error;
use physure_core::units::conf::{parse_physure_conf, DEFAULT_PHYSURE_CONF};
use physure_core::units::parser::Parser;
use physure_core::UnitRegistry as CoreUnitRegistry;
use physure_script::PhsInterpreter;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

pub(crate) struct RegistryState {
    pub(crate) registry: CoreUnitRegistry,
    pub(crate) interpreter: PhsInterpreter,
}

fn load_default() -> CoreUnitRegistry {
    let mut registry = CoreUnitRegistry::new();
    let mut constants = HashMap::new();
    parse_physure_conf(DEFAULT_PHYSURE_CONF, &mut registry, &mut constants);
    registry
}

#[wasm_bindgen]
pub struct UnitRegistry {
    pub(crate) state: Rc<RefCell<RegistryState>>,
}

#[wasm_bindgen]
impl UnitRegistry {
    #[wasm_bindgen(constructor)]
    pub fn new() -> UnitRegistry {
        UnitRegistry {
            state: Rc::new(RefCell::new(RegistryState {
                registry: load_default(),
                interpreter: PhsInterpreter::default(),
            })),
        }
    }

    #[wasm_bindgen(js_name = fromContent)]
    pub fn from_content(content: &str) -> UnitRegistry {
        let mut registry = load_default();
        let mut constants = HashMap::new();
        parse_physure_conf(content, &mut registry, &mut constants);
        UnitRegistry {
            state: Rc::new(RefCell::new(RegistryState {
                registry,
                interpreter: PhsInterpreter::default(),
            })),
        }
    }

    #[wasm_bindgen(js_name = getUnitExponents)]
    pub fn get_unit_exponents(&self, expr: &str) -> Result<js_sys::Map, JsValue> {
        let state = self.state.borrow();
        let unit = Parser::parse_expression_with_registry(expr, &state.registry).map_err(to_js_error)?;
        let map = js_sys::Map::new();
        for (symbol, (num, _den)) in &unit.dimensions {
            map.set(&JsValue::from_str(symbol), &JsValue::from_f64(*num as f64));
        }
        Ok(map)
    }

    #[wasm_bindgen(js_name = getUnitScale)]
    pub fn get_unit_scale(&self, expr: &str) -> Result<f64, JsValue> {
        let state = self.state.borrow();
        Parser::parse_expression_with_registry(expr, &state.registry)
            .map(|u| u.scale)
            .map_err(to_js_error)
    }

    #[wasm_bindgen(js_name = getCategories)]
    pub fn get_categories(&self) -> js_sys::Map {
        let state = self.state.borrow();
        let map = js_sys::Map::new();
        for (category, units) in &state.registry.categories {
            let array = js_sys::Array::new();
            for unit in units {
                array.push(&JsValue::from_str(unit));
            }
            map.set(&JsValue::from_str(category), &array);
        }
        map
    }
}

impl Default for UnitRegistry {
    fn default() -> Self {
        UnitRegistry::new()
    }
}
```

Modify `physure-wasm/src/lib.rs`:
```rust
mod error;
mod quantity;
mod registry;

use wasm_bindgen::prelude::*;

pub use quantity::Quantity;
pub use registry::UnitRegistry;
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: all tests pass (12 total).

- [ ] **Step 5: Commit**

```bash
git add physure-wasm/src/registry.rs physure-wasm/src/lib.rs
git commit -m "feat(wasm): add UnitRegistry construction and unit introspection"
```

---

### Task 6: `UnitRegistry` — `evaluate`/`deriv`/`integral`/`solve`

**Files:**
- Modify: `physure-wasm/src/registry.rs`

- [ ] **Step 1: Write failing test**

Add to the `mod tests` block in `physure-wasm/src/registry.rs`:
```rust
    #[wasm_bindgen_test]
    fn evaluate_runs_statements_and_returns_the_result_as_a_string() {
        let mut reg = UnitRegistry::new();
        let result = reg.evaluate("a = 15 m; b = 5 m; a * b").unwrap();
        assert_eq!(result, "75 m^2");
    }

    #[wasm_bindgen_test]
    fn deriv_computes_a_symbolic_derivative() {
        let mut reg = UnitRegistry::new();
        let result = reg.deriv("v0 * t + 0.5 * a * t^2", "t").unwrap();
        assert_eq!(result, "v0 + a * t");
    }

    #[wasm_bindgen_test]
    fn integral_computes_a_symbolic_integral() {
        let mut reg = UnitRegistry::new();
        let result = reg.integral("3 * t^2", "t").unwrap();
        assert_eq!(result, "t^3");
    }

    #[wasm_bindgen_test]
    fn solve_isolates_the_requested_variable() {
        let mut reg = UnitRegistry::new();
        let result = reg.solve("P * V = n * R * T", "T").unwrap();
        assert_eq!(result, "P * V / (n * R)");
    }
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: FAIL to compile — `no method named 'evaluate' found for struct 'UnitRegistry'`.

- [ ] **Step 3: Write minimal implementation**

Add these imports to the top of `physure-wasm/src/registry.rs`:
```rust
use physure_script::{parse_phs, PhsValue, Statement};
```

Add inside the `#[wasm_bindgen] impl UnitRegistry` block (after `get_categories`):
```rust
    pub fn evaluate(&self, source: &str) -> Result<String, JsValue> {
        let program = parse_phs(source).map_err(to_js_error)?;
        let mut state = self.state.borrow_mut();
        let mut result = String::new();
        for (idx, stmt) in program.statements.iter().enumerate() {
            if idx > 0 {
                result.push('\n');
            }
            let value = state.interpreter.run_statement(stmt).map_err(to_js_error)?;
            match value {
                PhsValue::None => match stmt {
                    Statement::Assignment(node) => match state.interpreter.get_var(&node.name) {
                        Some(v) => result.push_str(&v.to_string()),
                        None => result.push_str("None"),
                    },
                    _ => result.push_str("None"),
                },
                other => result.push_str(&other.to_string()),
            }
        }
        Ok(result)
    }

    pub fn deriv(&self, expr: &str, wrt: &str) -> Result<String, JsValue> {
        self.evaluate(&format!("deriv(\"{}\", \"{}\")", expr, wrt))
    }

    pub fn integral(&self, expr: &str, wrt: &str) -> Result<String, JsValue> {
        self.evaluate(&format!("integral(\"{}\", \"{}\")", expr, wrt))
    }

    pub fn solve(&self, equation: &str, wrt: &str) -> Result<String, JsValue> {
        self.evaluate(&format!("solve(\"{}\", \"{}\")", equation, wrt))
    }
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: all tests pass (16 total).

- [ ] **Step 5: Commit**

```bash
git add physure-wasm/src/registry.rs
git commit -m "feat(wasm): add UnitRegistry evaluate, deriv, integral, solve"
```

---

### Task 7: `PhyFunction` — construction, `getParams`/`getName`, `call` (string args)

**Files:**
- Create: `physure-wasm/src/phy_function.rs`
- Modify: `physure-wasm/src/lib.rs`

- [ ] **Step 1: Write failing test**

Create `physure-wasm/src/phy_function.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::UnitRegistry;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node);

    #[wasm_bindgen_test]
    fn constructs_registers_and_reports_params_and_name() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        assert_eq!(ke.get_params(), vec!["m".to_string(), "v".to_string()]);
        assert_eq!(ke.get_name(), "kinetic_energy");
    }

    #[wasm_bindgen_test]
    fn call_with_phs_literal_string_arguments() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let result = ke.call(vec![JsValue::from_str("10 kg"), JsValue::from_str("5 m/s")]).unwrap();
        assert_eq!(result.value(), 125.0);
        assert_eq!(result.unit(), "J");
    }
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: FAIL to compile — `cannot find struct 'PhyFunction' in this scope`.

- [ ] **Step 3: Write minimal implementation**

Add above the `#[cfg(test)]` block in `physure-wasm/src/phy_function.rs`:
```rust
use crate::error::to_js_error;
use crate::quantity::Quantity;
use crate::registry::{RegistryState, UnitRegistry};
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::Quantity as CoreQuantity;
use physure_script::{parse_phs, PhsInterpreter, PhsValue};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

fn register_body(interpreter: &mut PhsInterpreter, body: &str) -> PhysureResult<()> {
    let statements = parse_phs(body)?;
    let stmt = statements
        .statements
        .first()
        .ok_or_else(|| PhysureError::Generic("Empty function body".into()))?;
    interpreter.run_statement(stmt)?;
    Ok(())
}

#[wasm_bindgen]
pub struct PhyFunction {
    state: Rc<RefCell<RegistryState>>,
    name: String,
}

#[wasm_bindgen]
impl PhyFunction {
    #[wasm_bindgen(constructor)]
    pub fn new(registry: &UnitRegistry, name: &str, body: &str) -> Result<PhyFunction, JsValue> {
        {
            let mut state = registry.state.borrow_mut();
            register_body(&mut state.interpreter, body).map_err(to_js_error)?;
        }
        Ok(PhyFunction {
            state: registry.state.clone(),
            name: name.to_string(),
        })
    }

    #[wasm_bindgen(js_name = getName)]
    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    #[wasm_bindgen(js_name = getParams)]
    pub fn get_params(&self) -> Vec<String> {
        self.state
            .borrow()
            .interpreter
            .get_fn_params(&self.name)
            .unwrap_or_default()
    }

    #[wasm_bindgen(variadic)]
    pub fn call(&self, args: Vec<JsValue>) -> Result<Quantity, JsValue> {
        let mut formatted = Vec::with_capacity(args.len());
        for arg in &args {
            formatted.push(format_call_arg(arg)?);
        }
        let call_expr = format!("{}({})", self.name, formatted.join(", "));
        let statements = parse_phs(&call_expr).map_err(to_js_error)?;
        let stmt = statements
            .statements
            .first()
            .ok_or_else(|| to_js_error(PhysureError::Generic("Empty call expression".into())))?;
        let mut state = self.state.borrow_mut();
        match state.interpreter.run_statement(stmt).map_err(to_js_error)? {
            PhsValue::Quantity(q) => Ok(Quantity::from_core(q)),
            PhsValue::Number(n) => Ok(Quantity::from_core(CoreQuantity::new_scalar(
                n,
                0.0,
                physure_core::units::RationalUnit::dimensionless(),
                None,
                None,
            ))),
            other => Err(to_js_error(PhysureError::Generic(format!(
                "Call did not return a quantity: {:?}",
                other
            )))),
        }
    }
}

fn format_call_arg(arg: &JsValue) -> Result<String, JsValue> {
    if let Some(s) = arg.as_string() {
        return Ok(s);
    }
    Err(js_sys::Error::new("PhyFunction.call: each argument must be a Quantity or a string").into())
}
```

Modify `physure-wasm/src/lib.rs`:
```rust
mod error;
mod phy_function;
mod quantity;
mod registry;

use wasm_bindgen::prelude::*;

pub use phy_function::PhyFunction;
pub use quantity::Quantity;
pub use registry::UnitRegistry;
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: all tests pass (18 total).

- [ ] **Step 5: Commit**

```bash
git add physure-wasm/src/phy_function.rs physure-wasm/src/lib.rs
git commit -m "feat(wasm): add PhyFunction construction and string-arg call"
```

---

### Task 8: `PhyFunction.call` with `Quantity` arguments

**Files:**
- Modify: `physure-wasm/src/phy_function.rs`

- [ ] **Step 1: Write failing test**

Add to the `mod tests` block in `physure-wasm/src/phy_function.rs`:
```rust
    #[wasm_bindgen_test]
    fn call_with_quantity_arguments() {
        use crate::quantity::Quantity;

        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let mass = Quantity::new(10.0, "kg").unwrap();
        let speed = Quantity::new(5.0, "m/s").unwrap();
        let result = ke.call(vec![JsValue::from(mass), JsValue::from(speed)]).unwrap();
        assert_eq!(result.value(), 125.0);
        assert_eq!(result.unit(), "J");
    }

    #[wasm_bindgen_test]
    fn call_rejects_an_argument_that_is_neither_a_quantity_nor_a_string() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let err = ke.call(vec![JsValue::from_f64(10.0)]).unwrap_err();
        use wasm_bindgen::JsCast;
        let js_error = err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_error.message()).contains("must be a Quantity or a string"));
    }
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: FAIL — `call_with_quantity_arguments` fails at runtime with the "must be a Quantity or a string" error (the `Quantity` branch in `format_call_arg` doesn't exist yet), while `call_rejects_...` already passes.

- [ ] **Step 3: Write minimal implementation**

Replace `format_call_arg` in `physure-wasm/src/phy_function.rs`:
```rust
fn format_call_arg(arg: &JsValue) -> Result<String, JsValue> {
    use wasm_bindgen::JsCast;

    if let Some(s) = arg.as_string() {
        return Ok(s);
    }
    if let Ok(q) = arg.clone().dyn_into::<Quantity>() {
        return Ok(format_quantity_literal(q.inner_ref()));
    }
    Err(js_sys::Error::new("PhyFunction.call: each argument must be a Quantity or a string").into())
}

fn format_quantity_literal(q: &CoreQuantity) -> String {
    let mut unit_str = q.unit.__repr__();
    unit_str = unit_str.replace('·', " * ");
    unit_str = unit_str.replace('⁻', "^-");
    unit_str = unit_str.replace('⁰', "^0");
    unit_str = unit_str.replace('¹', "^1");
    unit_str = unit_str.replace('²', "^2");
    unit_str = unit_str.replace('³', "^3");
    unit_str = unit_str.replace('⁴', "^4");
    unit_str = unit_str.replace('⁵', "^5");
    unit_str = unit_str.replace('⁶', "^6");
    unit_str = unit_str.replace('⁷', "^7");
    unit_str = unit_str.replace('⁸', "^8");
    unit_str = unit_str.replace('⁹', "^9");
    unit_str = unit_str.replace("^-^-", "^-");
    unit_str = unit_str.replace("^^", "^");
    format!("{} {}", q.value.mean(), unit_str)
}
```

Add a package-visible accessor to `physure-wasm/src/quantity.rs`, inside the plain (non-`#[wasm_bindgen]`) `impl Quantity` block:
```rust
    pub(crate) fn inner_ref(&self) -> &CoreQuantity {
        &self.inner
    }
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: all tests pass (20 total).

- [ ] **Step 5: Commit**

```bash
git add physure-wasm/src/phy_function.rs physure-wasm/src/quantity.rs
git commit -m "feat(wasm): support Quantity arguments in PhyFunction.call"
```

---

### Task 9: `PhyFunction` — `deriv`/`integral`/`solve`

**Files:**
- Modify: `physure-wasm/src/phy_function.rs`

- [ ] **Step 1: Write failing test**

Add to the `mod tests` block in `physure-wasm/src/phy_function.rs`:
```rust
    #[wasm_bindgen_test]
    fn deriv_returns_a_new_callable_phy_function() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let dke_dv = ke.deriv("v").unwrap();
        assert_eq!(dke_dv.get_params(), vec!["m".to_string(), "v".to_string()]);
        let result = dke_dv
            .call(vec![JsValue::from_str("10 kg"), JsValue::from_str("5 m/s")])
            .unwrap();
        assert_eq!(result.value(), 50.0);
    }

    #[wasm_bindgen_test]
    fn integral_returns_a_new_callable_phy_function() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let ike_dv = ke.integral("v").unwrap();
        let result = ike_dv
            .call(vec![JsValue::from_str("10 kg"), JsValue::from_str("5 m/s")])
            .unwrap();
        assert!((result.value() - 208.33333333333334).abs() < 1e-9);
    }

    #[wasm_bindgen_test]
    fn solve_returns_a_phy_function_with_target_as_the_first_parameter() {
        let registry = UnitRegistry::new();
        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let solve_for_v = ke.solve("v").unwrap();
        assert_eq!(solve_for_v.get_params(), vec!["target".to_string(), "m".to_string()]);
        let result = solve_for_v
            .call(vec![JsValue::from_str("125 J"), JsValue::from_str("10 kg")])
            .unwrap();
        assert_eq!(result.value(), 5.0);
    }
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: FAIL to compile — `no method named 'deriv' found for struct 'PhyFunction'`.

- [ ] **Step 3: Write minimal implementation**

Add inside the `#[wasm_bindgen] impl PhyFunction` block (after `call`):
```rust
    pub fn deriv(&self, wrt: &str) -> Result<PhyFunction, JsValue> {
        let params = self.get_params();
        if params.is_empty() {
            return Err(to_js_error(PhysureError::Generic(
                "Cannot differentiate a function with no parameters".into(),
            )));
        }
        let params_joined = params.join(", ");
        let call_expr = format!("{}({})", self.name, params_joined);
        let deriv_result = self.evaluate_string(&format!("deriv(\"{}\", \"{}\")", call_expr, wrt))?;
        let new_name = format!("d_{}_d_{}", self.name, wrt);
        let new_body = format!("{}({}) = {}", new_name, params_joined, deriv_result);
        self.define(&new_name, &new_body)
    }

    pub fn integral(&self, wrt: &str) -> Result<PhyFunction, JsValue> {
        let params = self.get_params();
        if params.is_empty() {
            return Err(to_js_error(PhysureError::Generic(
                "Cannot integrate a function with no parameters".into(),
            )));
        }
        let params_joined = params.join(", ");
        let call_expr = format!("{}({})", self.name, params_joined);
        let integral_result = self.evaluate_string(&format!("integral(\"{}\", \"{}\")", call_expr, wrt))?;
        let new_name = format!("i_{}_d_{}", self.name, wrt);
        let new_body = format!("{}({}) = {}", new_name, params_joined, integral_result);
        self.define(&new_name, &new_body)
    }

    pub fn solve(&self, wrt: &str) -> Result<PhyFunction, JsValue> {
        let params = self.get_params();
        if params.is_empty() {
            return Err(to_js_error(PhysureError::Generic(
                "Cannot solve a function with no parameters".into(),
            )));
        }
        let params_joined = params.join(", ");
        let call_expr = format!("{}({})", self.name, params_joined);
        let solve_result =
            self.evaluate_string(&format!("solve(\"{} = target\", \"{}\")", call_expr, wrt))?;
        let new_name = format!("solve_{}_for_{}", self.name, wrt);
        let other_params: Vec<String> = params.into_iter().filter(|p| p != wrt).collect();
        let mut new_params = vec!["target".to_string()];
        new_params.extend(other_params);
        let new_params_joined = new_params.join(", ");
        let new_body = format!("{}({}) = {}", new_name, new_params_joined, solve_result);
        self.define(&new_name, &new_body)
    }
```

Add a plain (non-`#[wasm_bindgen]`) `impl PhyFunction` block at the bottom of `physure-wasm/src/phy_function.rs`, before the `format_call_arg` function:
```rust
impl PhyFunction {
    fn evaluate_string(&self, expr: &str) -> Result<String, JsValue> {
        let statements = parse_phs(expr).map_err(to_js_error)?;
        let stmt = statements
            .statements
            .first()
            .ok_or_else(|| to_js_error(PhysureError::Generic("Empty expression".into())))?;
        let mut state = self.state.borrow_mut();
        match state.interpreter.run_statement(stmt).map_err(to_js_error)? {
            PhsValue::String(s) => Ok(s),
            PhsValue::Equation(_, rhs) => Ok(rhs.to_phs_string()),
            other => Err(to_js_error(PhysureError::Generic(format!(
                "Expected a string or equation result, got: {:?}",
                other
            )))),
        }
    }

    fn define(&self, name: &str, body: &str) -> Result<PhyFunction, JsValue> {
        register_body(&mut self.state.borrow_mut().interpreter, body).map_err(to_js_error)?;
        Ok(PhyFunction {
            state: self.state.clone(),
            name: name.to_string(),
        })
    }
}
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: all tests pass (23 total).

- [ ] **Step 5: Commit**

```bash
git add physure-wasm/src/phy_function.rs
git commit -m "feat(wasm): add PhyFunction deriv, integral, solve"
```

---

### Task 10: `PhyFunction` — arithmetic, `compose`, registry-mismatch guard

**Files:**
- Modify: `physure-wasm/src/phy_function.rs`

- [ ] **Step 1: Write failing test**

Add to the `mod tests` block in `physure-wasm/src/phy_function.rs`:
```rust
    #[wasm_bindgen_test]
    fn arithmetic_combines_two_phy_functions() {
        let registry = UnitRegistry::new();
        let f = PhyFunction::new(&registry, "f", "f(x) = 10 * x").unwrap();
        let g = PhyFunction::new(&registry, "g", "g(x) = 2 * x").unwrap();

        let diff = f.subtract(&g).unwrap();
        let prod = f.multiply(&g).unwrap();
        let quot = f.divide(&g).unwrap();

        assert_eq!(diff.call(vec![JsValue::from_str("3")]).unwrap().value(), 24.0);
        assert_eq!(prod.call(vec![JsValue::from_str("3")]).unwrap().value(), 180.0);
        assert_eq!(quot.call(vec![JsValue::from_str("3")]).unwrap().value(), 5.0);
    }

    #[wasm_bindgen_test]
    fn compose_nests_the_inner_function_call() {
        let registry = UnitRegistry::new();
        let f = PhyFunction::new(&registry, "f", "f(x) = 2 * x").unwrap();
        let g = PhyFunction::new(&registry, "g", "g(x) = 3 * x").unwrap();
        let composed = f.compose(&g).unwrap();
        let result = composed.call(vec![JsValue::from_str("2")]).unwrap();
        assert_eq!(result.value(), 12.0);
    }

    #[wasm_bindgen_test]
    fn operations_across_different_registries_are_rejected() {
        let registry_a = UnitRegistry::new();
        let registry_b = UnitRegistry::new();
        let f = PhyFunction::new(&registry_a, "f", "f(x) = 2 * x").unwrap();
        let g = PhyFunction::new(&registry_b, "g", "g(x) = 3 * x").unwrap();
        let err = f.add(&g).unwrap_err();
        use wasm_bindgen::JsCast;
        let js_error = err.dyn_into::<js_sys::Error>().expect("should be a JS Error");
        assert!(String::from(js_error.message()).contains("same UnitRegistry"));
    }
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: FAIL to compile — `no method named 'subtract' found for struct 'PhyFunction'`.

- [ ] **Step 3: Write minimal implementation**

Add inside the `#[wasm_bindgen] impl PhyFunction` block (after `solve`):
```rust
    pub fn add(&self, other: &PhyFunction) -> Result<PhyFunction, JsValue> {
        self.binary_op(other, "+", "add")
    }

    pub fn subtract(&self, other: &PhyFunction) -> Result<PhyFunction, JsValue> {
        self.binary_op(other, "-", "subtract")
    }

    pub fn multiply(&self, other: &PhyFunction) -> Result<PhyFunction, JsValue> {
        self.binary_op(other, "*", "multiply")
    }

    pub fn divide(&self, other: &PhyFunction) -> Result<PhyFunction, JsValue> {
        self.binary_op(other, "/", "divide")
    }

    pub fn compose(&self, inner: &PhyFunction) -> Result<PhyFunction, JsValue> {
        self.require_same_registry(inner)?;
        let params_f = self.get_params();
        let params_g = inner.get_params();
        if params_f.is_empty() {
            return Err(to_js_error(PhysureError::Generic(
                "Outer function must have at least one parameter.".into(),
            )));
        }
        let mut combined = params_g.clone();
        for p in &params_f[1..] {
            if !combined.contains(p) {
                combined.push(p.clone());
            }
        }
        let combined_joined = combined.join(", ");
        let call_g = format!("{}({})", inner.name, params_g.join(", "));
        let mut call_f_args = vec![call_g];
        call_f_args.extend(params_f[1..].to_vec());
        let call_f = format!("{}({})", self.name, call_f_args.join(", "));
        let new_name = format!("compose_{}_{}", self.name, inner.name);
        let new_body = format!("{}({}) = {}", new_name, combined_joined, call_f);
        self.define(&new_name, &new_body)
    }
```

Add to the plain `impl PhyFunction` block:
```rust
    fn require_same_registry(&self, other: &PhyFunction) -> Result<(), JsValue> {
        if !Rc::ptr_eq(&self.state, &other.state) {
            return Err(js_sys::Error::new(
                "PhyFunction operations require both functions to share the same UnitRegistry",
            )
            .into());
        }
        Ok(())
    }

    fn binary_op(&self, other: &PhyFunction, op_symbol: &str, op_name: &str) -> Result<PhyFunction, JsValue> {
        self.require_same_registry(other)?;
        let params1 = self.get_params();
        let params2 = other.get_params();
        let mut combined = params1.clone();
        for p in &params2 {
            if !combined.contains(p) {
                combined.push(p.clone());
            }
        }
        let combined_joined = combined.join(", ");
        let new_name = format!("{}_{}_{}", op_name, self.name, other.name);
        let new_body = format!(
            "{}({}) = {}({}) {} {}({})",
            new_name,
            combined_joined,
            self.name,
            params1.join(", "),
            op_symbol,
            other.name,
            params2.join(", ")
        );
        self.define(&new_name, &new_body)
    }
```

- [ ] **Step 4: Run test, verify it passes**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: all tests pass (26 total).

- [ ] **Step 5: Commit**

```bash
git add physure-wasm/src/phy_function.rs
git commit -m "feat(wasm): add PhyFunction arithmetic, compose, and registry guard"
```

---

### Task 11: Packaging — `package.json` + dual build verification

**Files:**
- Create: `physure-wasm/package.json`
- Create: `physure-wasm/.gitignore`

- [ ] **Step 1: Create the package manifest**

Create `physure-wasm/package.json`:
```json
{
  "name": "physure",
  "version": "0.2.3",
  "description": "Unit-aware, dimension-correct physics computing — WebAssembly bindings for the physure core physics engine.",
  "license": "MIT",
  "repository": "https://github.com/Alexisrx96/physure",
  "files": [
    "pkg-bundler",
    "pkg-node"
  ],
  "exports": {
    ".": {
      "node": "./pkg-node/physure_wasm.js",
      "import": "./pkg-bundler/physure_wasm.js",
      "types": "./pkg-bundler/physure_wasm.d.ts"
    }
  }
}
```

Create `physure-wasm/.gitignore` (build output is generated by CI/local builds, never committed):
```
/pkg-bundler
/pkg-node
/target
```

- [ ] **Step 2: Build both targets**

Run:
```bash
cd physure-wasm
wasm-pack build --release --target bundler --out-dir pkg-bundler
wasm-pack build --release --target nodejs --out-dir pkg-node
cd ..
```
Expected: both commands succeed and produce `pkg-bundler/physure_wasm.js` + `.d.ts`, and `pkg-node/physure_wasm.js` + `.d.ts`.

- [ ] **Step 3: Exercise the published entry point from plain Node**

Run:
```bash
node -e "
const { UnitRegistry, Quantity, PhyFunction } = require('./physure-wasm/pkg-node/physure_wasm.js');
const q = new Quantity(75, 'kg');
console.log(q.toString());
const registry = new UnitRegistry();
const ke = new PhyFunction(registry, 'kinetic_energy', 'kinetic_energy(m, v) = 0.5 * m * v^2');
console.log(ke.call('10 kg', '5 m/s').toString());
"
```
Expected output:
```
75 kg
125 J
```

- [ ] **Step 4: Commit**

```bash
git add physure-wasm/package.json physure-wasm/.gitignore
git commit -m "feat(wasm): assemble dual bundler/nodejs npm package"
```

---

### Task 12: CI — `wasm-release.yml`

**Files:**
- Create: `.github/workflows/wasm-release.yml`

- [ ] **Step 1: Write the workflow**

Create `.github/workflows/wasm-release.yml`:
```yaml
# Release pipeline: push a tag `wasm-vX.Y.Z` to publish the `physure-wasm`
# bindings (physure-wasm/) to npm as the `physure` package.
#
# physure-wasm is versioned independently from the Rust core, Python wrapper,
# and Java bindings (see core-release.yml / release.yml / java-release.yml).
#
# One-time setup (repo owner):
#   1. Create an npm account/org and verify it can publish the `physure` name.
#   2. Generate an npm "Automation" access token (npmjs.com -> Access Tokens).
#   3. GitHub: create the `npm` environment (Settings -> Environments) and add
#      the token as the NPM_TOKEN secret in that environment.
name: WASM Release

on:
  push:
    tags: ["wasm-v*"]

permissions:
  contents: read

jobs:
  check-version:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4
      - name: Tag must match physure-wasm/Cargo.toml version
        run: |
          tag="${GITHUB_REF_NAME#wasm-v}"
          version="$(grep -m1 -oP '(?<=^version = ")[^"]+' physure-wasm/Cargo.toml || true)"
          if [ -z "$version" ]; then
            version="$(grep -m1 -oP '(?<=^version.workspace = )true' Cargo.toml > /dev/null && grep -m1 -oP '(?<=^version *= *")[^"]+' Cargo.toml)"
          fi
          if [ "$tag" != "$version" ]; then
            echo "Tag $GITHUB_REF_NAME (parsed $tag) does not match Cargo.toml version $version" >&2
            exit 1
          fi

  publish:
    needs: check-version
    runs-on: ubuntu-latest
    environment: npm
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown
      - uses: actions/setup-node@v4
        with:
          node-version: "20"
          registry-url: "https://registry.npmjs.org"
      - name: Install wasm-pack
        run: cargo install wasm-pack --locked
      - name: Build bundler target
        working-directory: physure-wasm
        run: wasm-pack build --release --target bundler --out-dir pkg-bundler
      - name: Build nodejs target
        working-directory: physure-wasm
        run: wasm-pack build --release --target nodejs --out-dir pkg-node
      - name: Publish to npm
        working-directory: physure-wasm
        run: npm publish --access public
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

- [ ] **Step 2: Verify the workflow is syntactically valid**

Run: `python -c "import yaml, sys; yaml.safe_load(open('.github/workflows/wasm-release.yml'))"`
Expected: no output, exit code 0 (valid YAML).

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/wasm-release.yml
git commit -m "ci(wasm): add npm release workflow for physure-wasm"
```

---

### Task 13: End-to-end parity test

**Files:**
- Modify: `physure-wasm/src/phy_function.rs`

- [ ] **Step 1: Write failing test**

Add to the `mod tests` block in `physure-wasm/src/phy_function.rs` (this mirrors `physure-java/README.md`'s usage example end to end, and the already-passing `test_phy_function_parity_rust` in `physure-script/src/function.rs`):
```rust
    #[wasm_bindgen_test]
    fn kinetic_energy_end_to_end_matches_the_java_readme_example() {
        use crate::quantity::Quantity;

        let registry = UnitRegistry::new();

        let exponents = registry.get_unit_exponents("kg*m/s^2").unwrap();
        assert_eq!(exponents.get(&"kg".into()).as_f64(), Some(1.0));

        let eval_result = registry.evaluate("a = 15 m; b = 5 m; a * b").unwrap();
        assert_eq!(eval_result, "75 m^2");

        let q1 = Quantity::new(10.0, "m").unwrap();
        let q2 = Quantity::new(2.0, "s").unwrap();
        let velocity = q1.divide(&q2).unwrap();
        let velocity_kmh = velocity.convert_to("km/h").unwrap();
        assert!((velocity_kmh.value() - 18.0).abs() < 1e-9);

        let ke = PhyFunction::new(&registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
        let energy = ke
            .call(vec![JsValue::from_str("10 kg"), JsValue::from_str("5 m/s")])
            .unwrap();
        assert_eq!(energy.value(), 125.0);
        assert_eq!(energy.unit(), "J");

        let dke_dv = ke.deriv("v").unwrap();
        let deriv_result = dke_dv
            .call(vec![JsValue::from_str("10 kg"), JsValue::from_str("5 m/s")])
            .unwrap();
        assert_eq!(deriv_result.value(), 50.0);

        let solve_for_m = ke.solve("m").unwrap();
        let target_energy = Quantity::new(125.0, "J").unwrap();
        let solved_mass = solve_for_m
            .call(vec![JsValue::from(target_energy), JsValue::from_str("5 m/s")])
            .unwrap();
        assert_eq!(solved_mass.value(), 10.0);
    }
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: PASS immediately — every API this test exercises was already implemented in Tasks 3-9. This step exists to confirm no regression was introduced by any earlier task; if it fails, the failure points at whichever earlier task's implementation drifted from the others (e.g. a units/`convertTo` mismatch), which must be fixed before continuing.

- [ ] **Step 3: n/a — no new implementation needed**

- [ ] **Step 4: Run the full test suite one more time**

Run: `cd physure-wasm && wasm-pack test --node && cd ..`
Expected: all 27 tests pass.

- [ ] **Step 5: Commit**

```bash
git add physure-wasm/src/phy_function.rs
git commit -m "test(wasm): add end-to-end kinetic_energy parity test"
```
