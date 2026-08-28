# Direct Foreign Execution Bridge & Mathematical Chaining Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Design spec:** `docs/superpowers/specs/2026-08-27-phs-foreign-bridge-design.md`

**Goal:** Enable external host languages and services to directly load `.phs` repositories at runtime, introspect function signatures, dynamically execute calculations with dimensional coercion, and chain/compose mathematical formulas with end-to-end unit and uncertainty safety.

**Architecture:**
1. **Core (`physure-script`)**: Add `PhsModule` and `PhsProject` for symbol introspection, dynamic function invocation with dimensional coercion, and mathematical chaining ($f \circ g$ and multi-step computational pipelines).
2. **Tier 1 (In-Process)**: PyO3 wrapper and pure-Python proxy exposing `physure.load_phs()` / `physure.load_dir()` with dynamic keyword arguments and fluent composition.
3. **Tier 2 (Package)**: `phs.toml` manifest support and `phs pack` CLI bundling.
4. **Tier 3 (Service)**: `phs serve` lightweight server exposing REST/JSON endpoints for catalog, function invocation, and multi-step pipeline execution.

**Tech Stack:** Rust (`physure-script`, `physure-core`, `physure-cli`), PyO3 (`physure-python`), Python 3.11+.

---

## File Structure & Module Map

| File | Subproject | Responsibility |
| :--- | :--- | :--- |
| `physure-script/src/module.rs` | `physure-script` | `ParamInfo`, `FunctionSignature`, `PhsModule`, signature extraction, dynamic invocation with unit coercion |
| `physure-script/src/pipeline.rs` | `physure-script` | Declarative DAG execution for multi-step chained formulas with uncertainty propagation |
| `physure-script/src/lib.rs` | `physure-script` | Re-export `PhsModule`, `PhsProject`, `PhsPipeline` |
| `physure-python/src/lib.rs` | `physure-python` | PyO3 bindings for `PyPhsModule` and `PyPhsProject` |
| `physure-python/physure/module.py` | `physure-python` | Python proxy with `__getattr__`, keyword argument dispatch, `.compose()`, and `Pipeline` |
| `physure-python/physure/__init__.py` | `physure-python` | Public exports: `load_phs`, `load_dir`, `Pipeline` |
| `physure-cli/src/package.rs` | `physure-cli` | `phs.toml` manifest parser and bundle generator (`phs pack`) |
| `physure-cli/src/server.rs` | `physure-cli` | `phs serve` HTTP REST / JSON-RPC server with catalog and pipeline endpoints |
| `physure-cli/src/main.rs` | `physure-cli` | CLI subcommands `pack` and `serve` wiring |

---

## Task 1: Module and Function Signature Introspection in `physure-script`

**Files:**
- Create: `physure-script/src/module.rs`
- Modify: `physure-script/src/lib.rs`
- Test: `physure-script/src/module.rs` (inline unit tests)

- [x] **Step 1: Write the failing unit tests for signature extraction**

```rust
// In physure-script/src/module.rs (bottom test module)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function_signatures_from_phs() {
        let code = r#"
/// Computes kinetic energy in Joules
/// @param m Mass of the body in kg
/// @param v Velocity in m/s
/// @returns Energy in Joules
fn E_k(m: kg, v: m/s) = 0.5 * m * v^2

# Mathematical shorthand
P(x, y) = 100.0 kPa * sin(x / 1.0 m)
"#;
        let module = PhsModule::from_source("physics", code).expect("Module parsing failed");
        assert_eq!(module.name, "physics");
        assert!(module.functions.contains_key("E_k"));
        
        let ek_sig = &module.functions["E_k"];
        assert_eq!(ek_sig.name, "E_k");
        assert_eq!(ek_sig.params.len(), 2);
        assert_eq!(ek_sig.params[0].name, "m");
        assert_eq!(ek_sig.params[0].expected_unit.as_deref(), Some("kg"));
        assert_eq!(ek_sig.params[1].name, "v");
        assert_eq!(ek_sig.params[1].expected_unit.as_deref(), Some("m/s"));
    }
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p physure-script test_extract_function_signatures_from_phs`
Expected: FAIL (module does not exist yet).

- [x] **Step 3: Implement `ParamInfo`, `FunctionSignature`, and `PhsModule`**

```rust
// In physure-script/src/module.rs
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use physure_core::error::{PhysureError, PhysureResult};
use crate::{PhsInterpreter, PhsValue, Statement};

#[derive(Clone, Debug, PartialEq)]
pub struct ParamInfo {
    pub name: String,
    pub expected_unit: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionSignature {
    pub name: String,
    pub docstring: Option<String>,
    pub params: Vec<ParamInfo>,
}

#[derive(Clone)]
pub struct PhsModule {
    pub name: String,
    pub path: Option<PathBuf>,
    pub functions: HashMap<String, FunctionSignature>,
    pub interpreter: PhsInterpreter,
}

impl PhsModule {
    pub fn from_source(name: &str, source: &str) -> PhysureResult<Self> {
        let program = crate::parser::parse_phs(source)?;
        let mut interpreter = PhsInterpreter::default();
        let mut functions = HashMap::new();

        for stmt in &program.statements {
            if let Statement::FunctionDef(f) = stmt {
                // `params: Vec<String>` and `param_units: Vec<Option<String>>` are aligned by
                // *index*, not a map keyed by name -- `param_units.get(p)` with `p: &String`
                // wouldn't even compile (`Vec::get` takes a `usize`). `.doc` (not `.docstring`)
                // is the real field on `FunctionDefNode` (see `physure-script/src/ast.rs`).
                let params = f.params.iter().zip(&f.param_units).map(|(p, u)| {
                    ParamInfo {
                        name: p.clone(),
                        expected_unit: u.clone(),
                    }
                }).collect();

                functions.insert(f.name.clone(), FunctionSignature {
                    name: f.name.clone(),
                    docstring: f.doc.clone(),
                    params,
                });
            }
            interpreter.eval_statement(stmt)?;
        }

        Ok(PhsModule {
            name: name.to_string(),
            path: None,
            functions,
            interpreter,
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> PhysureResult<Self> {
        let p = path.as_ref();
        let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("module");
        let content = std::fs::read_to_string(p)
            .map_err(|e| PhysureError::Generic(format!("Failed to read {}: {}", p.display(), e)))?;
        let mut m = Self::from_source(name, &content)?;
        m.path = Some(p.to_path_buf());
        Ok(m)
    }
}
```

- [x] **Step 4: Re-export in `physure-script/src/lib.rs` and verify test passes**

Run: `cargo test -p physure-script test_extract_function_signatures_from_phs`
Expected: PASS.

---

## Task 2: Dynamic Invocations with Dimensional Coercion in `physure-script`

**Files:**
- Modify: `physure-script/src/module.rs`
- Test: `physure-script/src/module.rs` (inline unit tests)

- [x] **Step 1: Write tests for dynamic invocation and dimensional validation**

```rust
// In physure-script/src/module.rs tests
#[test]
fn test_invoke_with_quantity_coercion() {
    let code = "fn E_k(m: kg, v: m/s) = 0.5 * m * v^2";
    let mut module = PhsModule::from_source("ke", code).unwrap();
    
    // Pass as Quantity values (10 kg and 5 m/s)
    let m_val = PhsValue::Quantity(physure_core::Quantity::new(10.0, "kg").unwrap());
    let v_val = PhsValue::Quantity(physure_core::Quantity::new(5.0, "m/s").unwrap());
    
    let res = module.invoke("E_k", &[m_val, v_val]).unwrap();
    if let PhsValue::Quantity(q) = res {
        assert_eq!(q.value.mean(), 125.0);
        assert_eq!(q.unit.__repr__(), "J");
    } else {
        panic!("Expected Quantity result");
    }
}

#[test]
fn test_invoke_rejects_incompatible_dimensions() {
    let code = "fn E_k(m: kg, v: m/s) = 0.5 * m * v^2";
    let mut module = PhsModule::from_source("ke", code).unwrap();
    
    // Passing seconds instead of velocity
    let m_val = PhsValue::Quantity(physure_core::Quantity::new(10.0, "kg").unwrap());
    let invalid_v = PhsValue::Quantity(physure_core::Quantity::new(5.0, "s").unwrap());
    
    let err = module.invoke("E_k", &[m_val, invalid_v]).unwrap_err();
    assert!(err.to_string().contains("Dimension mismatch") || err.to_string().contains("incompatible"));
}
```

- [x] **Step 2: Run test to verify failure**

Run: `cargo test -p physure-script test_invoke_with_quantity_coercion`
Expected: FAIL (`invoke` not implemented).

- [x] **Step 3: Implement `PhsModule::invoke`**

> **Deviation from the snippet below:** both `parse_expression_atomic` (never resolves against the
> unit registry, so a real `10 N` quantity would never dimension-match an atomically-parsed `"N"`)
> and the `Display`-stringify-and-`eval_str`-reparse mechanism (not round-trippable for `Vector`s
> over 4 elements, `Function`, `Plot`) turned out to be unsound. The shipped `invoke()` instead
> reuses the interpreter's own internal `call_function_node`/`bind_param_value` path — the same
> one native PHS-to-PHS calls use, which already does registry-aware coercion and enforces
> `@requires`/`@ensures`. See the doc comment on `PhsModule` in `physure-script/src/module.rs` for
> the full rationale. `invoke()` takes `&self`, not `&mut self` (loosened for Task 3's benefit).

```rust
// In physure-script/src/module.rs
impl PhsModule {
    pub fn invoke(&mut self, fn_name: &str, args: &[PhsValue]) -> PhysureResult<PhsValue> {
        let sig = self.functions.get(fn_name)
            .ok_or_else(|| PhysureError::Generic(format!("Function '{}' not found in module '{}'", fn_name, self.name)))?
            .clone();

        if args.len() != sig.params.len() {
            return Err(PhysureError::Generic(format!(
                "Function '{}' expects {} arguments, received {}",
                fn_name, sig.params.len(), args.len()
            )));
        }

        let mut coerced_args = Vec::with_capacity(args.len());
        for (arg, param) in args.iter().zip(&sig.params) {
            let val = match (arg, &param.expected_unit) {
                (PhsValue::Quantity(q), Some(expected_unit_str)) => {
                    let expected_unit = physure_core::units::Parser::parse_expression_atomic(expected_unit_str)?;
                    if !q.unit.same_dimensions(&expected_unit) {
                        return Err(PhysureError::Generic(format!(
                            "Dimension mismatch for parameter '{}': expected unit compatible with '{}', got '{}'",
                            param.name, expected_unit_str, q.unit.__repr__()
                        )));
                    }
                    let converted = q.convert_to(&expected_unit)?;
                    PhsValue::Quantity(converted)
                }
                (other, _) => other.clone(),
            };
            coerced_args.push(val);
        }

        // Call via inner interpreter
        let args_str: Vec<String> = coerced_args.iter().map(|a| a.to_string()).collect();
        let call_stmt = format!("{}({})", fn_name, args_str.join(", "));
        let results = self.interpreter.eval_str(&call_stmt)?;
        results.into_iter().last().ok_or_else(|| PhysureError::Generic("Empty function call result".into()))
    }
}
```

- [x] **Step 4: Run tests and verify pass**

Run: `cargo test -p physure-script test_invoke_`
Expected: PASS.

---

## Task 3: Mathematical Chaining & Function Composition Engine

**Files:**
- Create: `physure-script/src/pipeline.rs`
- Modify: `physure-script/src/module.rs`
- Modify: `physure-script/src/lib.rs`
- Test: `physure-script/src/pipeline.rs` (inline unit tests)

- [x] **Step 1: Write tests for function composition and multi-step pipeline chaining**

> **Deviation from the snippet below:** `parse_expression_atomic("N")` never resolves against the
> unit registry (same unsoundness Task 2 already found and documented for `invoke`), so it parses
> to the fake opaque dimension `{"N": 1}` rather than `kg*m*s^-2` — a real computed force would
> never `same_dimensions`-match it, and `convert_to` would fail. The shipped test uses
> `parse_expression` (the registry-resolving variant `Quantity::new` itself uses) instead; the
> physics/numbers were already right. `geom`/`hydr` are plain (non-`mut`) bindings in the shipped
> test — `compose_with` and `ComposedFunction::call` only ever need `&self`, so no `mut` is
> required anywhere in this task, mirroring Task 2's `invoke` deviation. See
> `physure-script/src/pipeline.rs`'s `test_symbolic_composition_between_modules` for the corrected
> version.

```rust
// In physure-script/src/pipeline.rs tests
#[test]
fn test_symbolic_composition_between_modules() {
    let geom_code = "fn area_tubo(d: mm) = 3.1415926535 * (d / 2)^2";
    let hydr_code = "fn fuerza_empuje(P: bar, A: mm^2) = P * A";
    
    let mut geom = PhsModule::from_source("geom", geom_code).unwrap();
    let mut hydr = PhsModule::from_source("hydr", hydr_code).unwrap();
    
    let composite_fn = hydr.compose_with(&geom, "fuerza_empuje", "area_tubo", "A").unwrap();
    
    // Evaluate composite directly: P = 5 bar, d = 50 mm
    let res = composite_fn.call(&[
        PhsValue::Quantity(physure_core::Quantity::new(5.0, "bar").unwrap()),
        PhsValue::Quantity(physure_core::Quantity::new(50.0, "mm").unwrap()),
    ]).unwrap();
    
    if let PhsValue::Quantity(q) = res {
        let n_unit = physure_core::units::Parser::parse_expression_atomic("N").unwrap();
        let in_newtons = q.convert_to(&n_unit).unwrap();
        assert!((in_newtons.value.mean() - 981.747).abs() < 0.1);
    } else {
        panic!("Expected Quantity");
    }
}
```

- [x] **Step 2: Implement `compose_with` and `PhsPipeline` DAG execution**

> **Deviation from the snippet below:** this plan gave no implementation for `compose_with`
> itself, and the design spec's `compose(&self, outer_fn, inner_fn, bind_param) -> PhysureResult<PhsFunction>`
> is same-module (single `&self`, no second module, an undefined `PhsFunction` return type) —
> superseded by this task's own test, which needs *cross-module* composition. The shipped
> `PhsModule::compose_with<'a>(&'a self, inner: &'a PhsModule, outer_fn, inner_fn, bind_param) ->
> PhysureResult<ComposedFunction<'a>>` (in `module.rs`, alongside `invoke`) borrows both modules
> for `'a` rather than cloning them (`PhsModule` deliberately isn't `Clone`) — a plain borrow is
> enough since both `invoke` calls it makes only need `&self`. `ComposedFunction::call`'s argument
> order is outer's params minus `bind_param`, in order, followed by inner's params, in order.
> Validity (both functions exist, `bind_param` names a real outer parameter) is checked eagerly in
> `compose_with`, not deferred to `call()`.
>
> `PhsPipeline::execute` takes `&self`, not `&mut self` as sketched below — it originally needed
> `&mut self` only because it called `self.modules.get_mut(..)` for the now-obsolete reason
> `invoke` used to need `&mut self`; since Task 2 loosened `invoke` to `&self`, `execute` uses
> `self.modules.get(..)` and needs no exclusive borrow either. See `physure-script/src/module.rs`
> (`compose_with`/`ComposedFunction`) and `physure-script/src/pipeline.rs` (`PhsPipeline`) for the
> shipped code and full rationale in the doc comments.

```rust
// In physure-script/src/pipeline.rs
use std::collections::HashMap;
use physure_core::error::{PhysureError, PhysureResult};
use crate::{PhsModule, PhsValue};

pub struct PipelineStep {
    pub module_name: String,
    pub function_name: String,
    pub inputs: HashMap<String, PipelineArg>,
    pub output_alias: String,
}

pub enum PipelineArg {
    Literal(PhsValue),
    Reference(String), // points to a previous step's output_alias
}

pub struct PhsPipeline {
    pub modules: HashMap<String, PhsModule>,
    pub steps: Vec<PipelineStep>,
}

impl PhsPipeline {
    pub fn new() -> Self {
        Self { modules: HashMap::new(), steps: Vec::new() }
    }

    pub fn add_module(&mut self, module: PhsModule) {
        self.modules.insert(module.name.clone(), module);
    }

    pub fn add_step(&mut self, step: PipelineStep) {
        self.steps.push(step);
    }

    pub fn execute(&mut self) -> PhysureResult<HashMap<String, PhsValue>> {
        let mut scope: HashMap<String, PhsValue> = HashMap::new();

        for step in &self.steps {
            let module = self.modules.get_mut(&step.module_name)
                .ok_or_else(|| PhysureError::Generic(format!("Module '{}' not found in pipeline", step.module_name)))?;
            
            let sig = module.functions.get(&step.function_name)
                .ok_or_else(|| PhysureError::Generic(format!("Function '{}' not found", step.function_name)))?
                .clone();

            let mut call_args = Vec::with_capacity(sig.params.len());
            for param in &sig.params {
                let arg_spec = step.inputs.get(&param.name)
                    .ok_or_else(|| PhysureError::Generic(format!("Missing input '{}' for step '{}'", param.name, step.output_alias)))?;
                
                let val = match arg_spec {
                    PipelineArg::Literal(v) => v.clone(),
                    PipelineArg::Reference(ref_name) => {
                        scope.get(ref_name)
                            .cloned()
                            .ok_or_else(|| PhysureError::Generic(format!("Unresolved reference '{}'", ref_name)))?
                    }
                };
                call_args.push(val);
            }

            let result = module.invoke(&step.function_name, &call_args)?;
            scope.insert(step.output_alias.clone(), result);
        }

        Ok(scope)
    }
}
```

- [x] **Step 3: Run pipeline tests and verify pass**

Run: `cargo test -p physure-script pipeline`
Expected: PASS.

---

## Task 4: PyO3 Rust Wrapper in `physure-python`

**Files:**
- Modify: `physure-python/src/lib.rs`
- Test: `physure-python/tests/core/test_module_bridge.py`

- [x] **Step 1: Expose `PyPhsModule` and `PyPhsProject` in `physure-python/src/lib.rs`**

> **Deviations from the snippet below:** `PyPhsProject` was skipped — no `PhsProject` type was
> ever built in Tasks 1-3 (it's a phantom from an earlier design draft), so there's nothing to
> wrap. `invoke()` uses `&self` (matches `PhsModule::invoke`'s real signature after Task 2's
> loosening) and wraps the call in `py.allow_threads(...)`, matching `PyInterpreter::evaluate`'s
> existing convention, so a long-running invocation doesn't block other Python threads. `from_file`
> checks `Path::is_file()` first and raises `PyFileNotFoundError` (not a generic `PyValueError`
> with an OS-locale-dependent message) when the file doesn't exist. See `physure-python/src/lib.rs`
> (`PyPhsModule`) for the shipped code.

```rust
// In physure-python/src/lib.rs
#[pyclass(name = "PhsModuleCore")]
pub struct PyPhsModule {
    inner: ::physure_script::PhsModule,
}

#[pymethods]
impl PyPhsModule {
    #[staticmethod]
    fn from_file(path: &str) -> PyResult<Self> {
        let inner = ::physure_script::PhsModule::from_file(path)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(PyPhsModule { inner })
    }

    #[staticmethod]
    fn from_source(name: &str, source: &str) -> PyResult<Self> {
        let inner = ::physure_script::PhsModule::from_source(name, source)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(PyPhsModule { inner })
    }

    fn list_functions(&self) -> Vec<String> {
        self.inner.functions.keys().cloned().collect()
    }

    fn get_params(&self, fn_name: &str) -> PyResult<Vec<String>> {
        self.inner.functions.get(fn_name)
            .map(|f| f.params.iter().map(|p| p.name.clone()).collect())
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(format!("Function '{}' not found", fn_name)))
    }

    fn invoke(&mut self, py: Python<'_>, fn_name: &str, args: Vec<Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let mut rust_args = Vec::with_capacity(args.len());
        for a in args {
            rust_args.push(py_to_phs_value(&a)?);
        }
        let res = self.inner.invoke(fn_name, &rust_args)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        phs_value_to_py(py, res)
    }
}
```

- [x] **Step 2: Register `PhsModuleCore` in `_core` module register function**

```rust
m.add_class::<PyPhsModule>()?;
```

- [x] **Step 3: Compile extension**

> **The documented command below is wrong and does not work** — `physure-core` has no
> `pyproject.toml` and is a pure `rlib` with an explicit "MUST NOT depend on pyo3" rule; there is
> nothing there for maturin to build. The actual maturin project is `physure-python/` (its own
> `pyproject.toml` has `module-name = "physure._core"`). The real command is
> `cd physure-python && maturin develop`. (This same wrong command is also in root `CLAUDE.md`'s
> Commands section — worth fixing there too next time that file is touched.) Also watch out:
> running this from a shell whose `$VIRTUAL_ENV` still points at a *different* checkout's `.venv`
> silently installs into the wrong one — verify with
> `python -c "import physure; print(physure._core.__file__)"` after building.

Run: ~~`cd physure-core && maturin develop && cd ..`~~ → `cd physure-python && maturin develop`
Expected: Build succeeds and module loads.

---

## Task 5: Python Ergonomic Interface (`physure.load_phs` & `physure.load_dir`)

**Files:**
- Create: `physure-python/physure/module.py`
- Modify: `physure-python/physure/__init__.py`
- Test: `physure-python/tests/test_foreign_bridge.py`

- [x] **Step 1: Write Python unit tests for direct loading and kwargs invocation**

> **Deviations from the snippet below (both confirmed bugs in the plan text, not style
> choices):**
> 1. `fuerza_empuje`'s first parameter is declared `P`, not `presion` — the snippet's kwarg
>    name doesn't match the function signature and would raise `ValueError: Missing required
>    parameter 'P'`. Fixed to `P="5 bar"`.
> 2. `res.unit`/the PHS-result type is the bare `physure._core.Quantity` (what
>    `phs_value_to_py` returns), which has no `.convert_to()` — and no conversion method at
>    all. `PhsFunctionWrapper.__call__` promotes results into the richer domain
>    `physure.domain.measurement.quantity.Quantity` instead (via
>    `CompoundUnit.from_rational_unit`, the library's own existing Rust-`RationalUnit`-to-
>    domain-unit bridge), so callers get the same type `Q_()` returns, with a real `.to()`.
>    Fixed `convert_to("N")` → `to("N")`; fixed `str(res.unit) == "J"` → `str(res) == "125.0
>    J"` (the domain unit's own `str()` shows the expanded base-unit form; the alias lookup
>    that renders "J" lives on `Quantity.__str__`, not `CompoundUnit.__str__`).
>
> A third, deeper issue surfaced investigating deviation 2: a domain `Quantity` (what
> `10 * kg` actually returns) is a separate, composition-based class that is NOT a
> `physure._core.Quantity` instance and defines `__float__` — passed straight into
> `PhsModuleCore.invoke()`, `py_to_phs_value` silently extracts it as a bare `f64`, dropping
> the unit (and any dimension check) instead of raising. Confirmed by hand: invoking a
> `kg`-declared parameter with a `Quantity` built in meters returned a bogus dimensionless
> result rather than failing. `PhsFunctionWrapper.__call__` now bridges every domain
> `Quantity` argument into a real `physure._core.Quantity` first (`_to_core_quantity` in
> `physure/module.py`, reusing the same magnitude/unit/std_dev construction
> `Quantity._maybe_wrap_in_rust_core` already uses) so the dimension check stays intact.
>
> Test 2 (chaining) also had to move off the plan's literal `bar`/`mm` units onto plain base-
> SI compositions (`kg`, `m`, `s`) after finding a fourth, separate bug — this one in
> `physure-script`, not this task's binding layer: `PhsModule::bind_param_value`
> (`physure-script/src/interpreter/expressions.rs`) parses a function's *declared* unit with
> the registry-*expanding* parser, while every foreign-facing constructor
> (`parse_unit_expression`, `UnitRegistry.get_unit`, this task's own bridge) uses the
> *atomic* one, and `RationalUnit::same_dimensions` (`physure-core/src/units/rational.rs`)
> compares raw symbol keys rather than reduced dimensions — so a foreign `Quantity` built in
> a named/derived/prefixed unit ("N", "Pa", "bar", "mm", "km", "g", ...) is rejected as
> dimensionally incompatible with a declared parameter of that very same unit. Confirmed with
> a minimal repro using only `physure._core` (no domain layer at all): `RationalUnit`s with
> identical `id`s still fail `PhsModule::invoke`'s coercion. A base-SI-only pre-conversion
> workaround was tried and rejected — it dodges the parser mismatch but produces a silently
> wrong final answer once the result is promoted back to a domain `Quantity` (`CompoundUnit
> .from_rational_unit` doesn't reconstruct the Rust-side unit's `scale`), which is worse than
> the honest `ValueError` this task ships instead. Out of scope for Task 5 (`physure-script`
> is off-limits here beyond the FFI-boundary fix above); pinned as an `xfail(strict=True)`
> regression test (`test_named_unit_parameter_rejects_matching_foreign_quantity`) so a future
> `physure-script` fix is noticed rather than silently masked.
>
> Additional tests beyond the plan's two happy-path snippets: missing function
> (`AttributeError` via `__getattr__`), missing required kwarg (`ValueError`), wrong-dimension
> argument (`ValueError`, proving `_to_core_quantity` actually prevents the silent-unit-drop
> above end-to-end), missing file (`FileNotFoundError`), `load_dir`, and `__dir__`. See
> `physure-python/tests/test_foreign_bridge.py` for the shipped tests.

```python
# In physure-python/tests/test_foreign_bridge.py
import pytest
import physure
from physure.units import kg, m, s

def test_load_phs_and_invoke(tmp_path):
    phs_file = tmp_path / "kinetics.phs"
    phs_file.write_text("fn E_k(m: kg, v: m/s) = 0.5 * m * v^2\n")

    mod = physure.load_phs(phs_file)
    res = mod.E_k(m=10.0 * kg, v=5.0 * m / s)
    
    assert res.magnitude == 125.0
    assert str(res.unit) == "J"

def test_load_phs_with_chaining(tmp_path):
    geom_file = tmp_path / "geom.phs"
    geom_file.write_text("fn area_tubo(d: mm) = 3.1415926535 * (d / 2)^2\n")
    hydr_file = tmp_path / "hydr.phs"
    hydr_file.write_text("fn fuerza_empuje(P: bar, A: mm^2) = P * A\n")

    geom = physure.load_phs(geom_file)
    hydr = physure.load_phs(hydr_file)

    # Chaining output of geom into hydr
    fuerza = hydr.fuerza_empuje(presion="5 bar", A=geom.area_tubo(d="50 mm"))
    assert fuerza.convert_to("N").magnitude == pytest.approx(981.74, rel=1e-3)
```

- [x] **Step 2: Implement `PhsModuleWrapper` and `load_phs`**

> **Deviations:** Also added `load_dir(path)` (one-line convenience: `load_phs` over every
> `.phs` file directly in a directory, keyed by filename stem) since the plan's own title
> names it, even though no sample code was given — no `PhsProject`-style cross-module
> composition was added, that stays out of scope. `PhsFunctionWrapper.__call__` routes every
> argument through `_coerce_scalar` (the plan's string-splitting logic, unchanged) and then
> `_to_core_quantity` (new — see Step 1's deviation notes), and its result through
> `_to_domain_value` (new) before returning. `load_phs`/`load_dir` are exported from
> `physure/__init__.py` lazily, via the same `_ATTR_LOADERS`/`__getattr__` pattern every other
> deferred export already uses, so accessing them doesn't pull in `physure.module`,
> `physure.domain`, torch, or scipy at `import physure` time (measured: `import physure`
> ~11ms, first `Q_()` eval ~170ms, well inside the ~0.5s budget; `physure.module` only enters
> `sys.modules` on first `load_phs`/`load_dir` access). See `physure-python/physure/module.py`
> for the shipped code.

```python
# In physure-python/physure/module.py
from pathlib import Path
from typing import Any
from physure._core import PhsModuleCore
from physure import Q_

class PhsFunctionWrapper:
    def __init__(self, module_core: PhsModuleCore, name: str, params: list[str]):
        self._core = module_core
        self._name = name
        self._params = params

    def __call__(self, *args, **kwargs) -> Any:
        ordered_args = []
        if kwargs:
            for p in self._params:
                if p in kwargs:
                    ordered_args.append(kwargs[p])
                else:
                    raise ValueError(f"Missing required parameter '{p}' for {self._name}()")
        else:
            ordered_args = list(args)

        # Convert string arguments or numbers to Quantities if needed. There is no
        # `Quantity.parse(str)` in physure-python -- the real factory is `Q_(magnitude,
        # unit)`, taking the two apart rather than one combined string. This split-on-
        # first-space approach mirrors physure-java's `Quantity.parse` (see
        # `physure-java/src/main/java/com/physure/Quantity.java`), which faced the same
        # "one string in, Quantity out" problem.
        # TODO: this does not understand uncertainty syntax ("50 mm +/- 0.1 mm", used in
        # this design's own Pipeline examples) -- that needs PHS's own quantity-literal
        # grammar, not a plain string split. Reusing the real PHS parser (exposed from
        # `physure-script` through PyO3) for this coercion path is follow-up work, not
        # covered by this task.
        coerced = []
        for arg in ordered_args:
            if isinstance(arg, str):
                if " " in arg:
                    mag_str, unit_str = arg.split(" ", 1)
                    coerced.append(Q_(float(mag_str), unit_str))
                else:
                    coerced.append(float(arg))
            else:
                coerced.append(arg)

        return self._core.invoke(self._name, coerced)

class PhsModuleWrapper:
    def __init__(self, core: PhsModuleCore):
        self._core = core
        self._fn_cache = {}
        for fn in self._core.list_functions():
            params = self._core.get_params(fn)
            self._fn_cache[fn] = PhsFunctionWrapper(self._core, fn, params)

    def __getattr__(self, item: str) -> PhsFunctionWrapper:
        if item in self._fn_cache:
            return self._fn_cache[item]
        raise AttributeError(f"Module has no function '{item}'")

    def __dir__(self):
        return list(super().__dir__()) + list(self._fn_cache.keys())

def load_phs(path: str | Path) -> PhsModuleWrapper:
    core = PhsModuleCore.from_file(str(path))
    return PhsModuleWrapper(core)
```

- [x] **Step 3: Run pytest and verify green**

Run: `uv run pytest physure-python/tests/test_foreign_bridge.py -v`
Expected: PASS. (9 passed, 1 xfailed — see Step 1's deviation notes for the `xfail`. Full
`physure-python` suite also verified green: 896 passed, 1 xfailed.)

---

## Task 6: Packaging & Manifest (`phs.toml` & `phs pack`) in `physure-cli`

> **TODO (architecture decision, not yet made):** no `toml` crate exists anywhere in this
> workspace today (checked the root `Cargo.toml`'s `[workspace.dependencies]` and every
> crate's own `Cargo.toml`) -- parsing `phs.toml` means adding one. `physure-cli` is a leaf
> binary crate, so this doesn't collide with `physure-python`'s "zero runtime dependencies"
> policy (that policy is about `pyproject.toml`'s wheel dependencies specifically), but it's
> still a new dependency and should be a conscious choice, not a side effect of running this
> task. Confirm before Task 6 starts.

**Files:**
- Create: `physure-cli/src/package.rs`
- Modify: `physure-cli/src/main.rs`
- Test: `physure-cli/src/package.rs` (inline unit tests)

- [ ] **Step 1: Write manifest parsing and verification tests**

```rust
#[test]
fn test_parse_phs_manifest() {
    let toml = r#"
[package]
name = "fluid-models"
version = "1.0.0"
entry = "main.phs"

[exports]
fluidos = "models/fluidos.phs"
"#;
    let manifest = Manifest::from_str(toml).unwrap();
    assert_eq!(manifest.package.name, "fluid-models");
    assert_eq!(manifest.exports.get("fluidos").unwrap(), "models/fluidos.phs");
}
```

- [ ] **Step 2: Implement manifest data structures and `phs pack` logic**

---

## Task 7: Model Server Runner (`phs serve`) in `physure-cli`

> **TODO (security, not yet decided):** the design spec has no authentication, access-control,
> or bind-address story for `phs serve` at all -- as written this is a REST server that
> executes arbitrary loaded `.phs` formulas for any caller who can reach the port, with no
> mention of an API key, localhost-only default, or rate limiting. Decide the auth/exposure
> model explicitly before implementing the routes below; do not ship a default-open server.
>
> **Lower implementation risk than the design spec's prose suggests:** `physure-cli` already
> depends on `tiny_http = "0.12"` (see `physure-cli/Cargo.toml`) and already has a working
> HTTP server in `physure-cli/src/web.rs` (used today by `phs script.phs --html`'s local plot
> viewer). This task should reuse that dependency and its existing request/response patterns
> rather than adding a new HTTP crate or building server plumbing from scratch -- read
> `web.rs` before starting Step 1.

**Files:**
- Create: `physure-cli/src/server.rs`
- Modify: `physure-cli/src/main.rs`

- [ ] **Step 1: Implement HTTP REST handler using embedded TCP/HTTP server**
- [ ] **Step 2: Add `/api/v1/catalog`, `/api/v1/:module/:function`, and `/api/v1/pipeline` routes**
- [ ] **Step 3: Wire `phs serve <dir> --port <port>` into `physure-cli/src/main.rs`**

---

## Plan Review & Verification Checklist

1. **Spec Coverage**: All 3 tiers, signature introspection, dimensional validation, and chaining/pipeline execution mapped to concrete tasks.
2. **Quality Gates**:
   - `cargo test --all` passes with zero warnings.
   - `uv run ruff check .` and `uv run pytest` pass with coverage $\ge 80\%$.
