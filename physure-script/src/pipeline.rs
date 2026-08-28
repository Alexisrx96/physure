//! Multi-step formula pipelines built on top of [`PhsModule::invoke`]: a [`PhsPipeline`] wires
//! together calls into one or more modules where a later step's argument can reference an
//! earlier step's result by name. Steps run in the fixed order they were added -- see
//! [`PhsPipeline`]'s own doc comment for exactly what "DAG" does and doesn't mean here.
//!
//! This is a thin orchestration layer over `PhsModule::invoke` and `PhsModule::compose_with`
//! (see `module.rs`) -- it adds no second unit-coercion path of its own. Every step's
//! arguments, whether literal values or resolved references to an earlier step's output, go
//! through `invoke()` exactly as a direct call would, so values threaded between steps keep
//! full dimensional correctness and `@requires`/`@ensures` enforcement automatically.

use crate::{PhsModule, PhsValue};
use physure_core::error::{PhysureError, PhysureResult};
use std::collections::HashMap;

/// One step in a [`PhsPipeline`]: call `function_name` in the module named `module_name` with
/// `inputs`, and make the result available to later steps under `output_alias`.
#[derive(Debug, Clone)]
pub struct PipelineStep {
    pub module_name: String,
    pub function_name: String,
    /// Maps each of the target function's parameter names to where its value comes from.
    /// A parameter with no entry here is a missing-input error at `execute()` time, not a
    /// default -- pipeline steps are not allowed to silently under-supply arguments.
    pub inputs: HashMap<String, PipelineArg>,
    pub output_alias: String,
}

/// Where a single [`PipelineStep`] argument's value comes from.
// `PhsValue` is ~496 bytes (it carries e.g. `Vector`/`Matrix` variants inline), so this enum
// trips `clippy::large_enum_variant`. Deliberately not boxed: `PipelineArg` lives in a
// config-time `HashMap` built once per pipeline, not a hot loop, so the extra stack space
// isn't a real cost, and boxing would add an indirection every call site has to match on.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineArg {
    /// A value supplied directly, independent of any other step.
    Literal(PhsValue),
    /// Another step's `output_alias`, resolved from the pipeline's running scope when this
    /// step executes. Referencing an alias no earlier step has produced yet (including one
    /// that belongs to a *later* step, or one that was never added) is an `execute()`-time
    /// error -- see [`PhsPipeline::execute`].
    Reference(String),
}

/// A named collection of modules plus an ordered list of steps to run against them, threading
/// each step's result forward by name so later steps can consume earlier results.
///
/// Steps run in the order they were added. There is no independent dependency-resolution or
/// topological-sort pass here -- "DAG" describes the shape of the data flow (a step can only
/// consume outputs of steps that ran before it), not a scheduler that reorders steps for you.
/// A step referencing an alias defined by a later step, or one never added at all, fails at
/// `execute()` with a clear "unresolved reference" error rather than being silently reordered
/// or defaulted.
#[derive(Default)]
pub struct PhsPipeline {
    pub modules: HashMap<String, PhsModule>,
    pub steps: Vec<PipelineStep>,
}

impl PhsPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `module` under its own `module.name`. Adding a second module with a name
    /// already in use silently replaces the first one (plain `HashMap::insert` semantics) --
    /// there's no separate "already registered" error today.
    pub fn add_module(&mut self, module: PhsModule) {
        self.modules.insert(module.name.clone(), module);
    }

    /// Appends `step` to the run order. Note that `execute()`'s `scope.insert` has the same
    /// silent-overwrite behavior for a repeated `output_alias`: a later step's result simply
    /// replaces an earlier step's under the same alias, rather than erroring.
    pub fn add_step(&mut self, step: PipelineStep) {
        self.steps.push(step);
    }

    /// Runs every step in order, resolving [`PipelineArg::Reference`] inputs against earlier
    /// steps' outputs, and returns every step's result keyed by its `output_alias`.
    ///
    /// Takes `&self`, not `&mut self`. The plan this was built from sketched `execute` as
    /// `&mut self` using `self.modules.get_mut(..)`, but that predates `PhsModule::invoke`
    /// being loosened from `&mut self` to `&self` (Task 2, done before this file existed).
    /// With `invoke` needing only a shared borrow, nothing in `execute` needs to mutate a
    /// module or the pipeline itself -- only `add_module`/`add_step` do that, and only before
    /// execution starts. Keeping `execute` immutable lets a caller run the same pipeline more
    /// than once without an artificial exclusive borrow (e.g. from multiple threads, or just
    /// twice in a row), matching the same reasoning Task 2 used for `invoke`.
    ///
    /// # Errors
    ///
    /// Returns an error -- and stops at that step, discarding no-longer-relevant partial
    /// results rather than returning them -- if a step names a module or function that isn't
    /// registered, an input is missing for one of the target function's parameters, a
    /// `Reference` input names an alias no earlier step produced, or the underlying `invoke()`
    /// call itself fails (dimension mismatch, wrong argument count, `@requires`/`@ensures`
    /// violation, etc.).
    pub fn execute(&self) -> PhysureResult<HashMap<String, PhsValue>> {
        let mut scope: HashMap<String, PhsValue> = HashMap::new();

        for step in &self.steps {
            let module = self.modules.get(&step.module_name).ok_or_else(|| {
                PhysureError::Generic(format!(
                    "Module '{}' not found in pipeline (step '{}')",
                    step.module_name, step.output_alias
                ))
            })?;

            let sig = module.functions.get(&step.function_name).ok_or_else(|| {
                PhysureError::Generic(format!(
                    "Function '{}' not found in module '{}' (step '{}')",
                    step.function_name, step.module_name, step.output_alias
                ))
            })?;

            let mut call_args = Vec::with_capacity(sig.params.len());
            for param in &sig.params {
                let arg_spec = step.inputs.get(&param.name).ok_or_else(|| {
                    PhysureError::Generic(format!(
                        "Missing input '{}' for step '{}'",
                        param.name, step.output_alias
                    ))
                })?;

                let val = match arg_spec {
                    PipelineArg::Literal(v) => v.clone(),
                    PipelineArg::Reference(ref_name) => {
                        scope.get(ref_name).cloned().ok_or_else(|| {
                            PhysureError::Generic(format!(
                                "Unresolved reference '{}' in step '{}'",
                                ref_name, step.output_alias
                            ))
                        })?
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PhsModule;

    /// Corrected version of the plan's Task 3 composition test. The plan's own text used
    /// `physure_core::units::Parser::parse_expression_atomic("N")` to build the comparison
    /// unit; that variant treats every symbol as its own opaque, unregistered dimension (its
    /// documented purpose), so a real, registry-resolved computed force would never compare
    /// equal to it via `same_dimensions` -- `convert_to` would fail with a dimension mismatch
    /// for essentially any named/prefixed unit. `parse_expression` is the registry-resolving
    /// variant (the same one `Quantity::new` itself uses internally), which is what makes
    /// `"N"` actually expand to `kg*m*s^-2` and compare correctly against the computed
    /// quantity. The physics/numbers in the plan's test were already right (verified by hand:
    /// P = 5 bar = 500000 Pa, A = area_tubo(50mm) = pi*(25mm)^2 = 1963.495 mm^2 =
    /// 0.001963495 m^2, F = P*A = 981.75 N) -- only this parser call was wrong.
    #[test]
    fn test_symbolic_composition_between_modules() {
        let geom_code = "fn area_tubo(d: mm) = 3.1415926535 * (d / 2)^2";
        let hydr_code = "fn fuerza_empuje(P: bar, A: mm^2) = P * A";

        let geom = PhsModule::from_source("geom", geom_code).unwrap();
        let hydr = PhsModule::from_source("hydr", hydr_code).unwrap();

        let composite_fn = hydr
            .compose_with(&geom, "fuerza_empuje", "area_tubo", "A")
            .unwrap();

        // Evaluate composite directly: P = 5 bar, d = 50 mm.
        let res = composite_fn
            .call(&[
                PhsValue::Quantity(physure_core::Quantity::new(5.0, "bar").unwrap()),
                PhsValue::Quantity(physure_core::Quantity::new(50.0, "mm").unwrap()),
            ])
            .unwrap();

        if let PhsValue::Quantity(q) = res {
            let n_unit = physure_core::units::Parser::parse_expression("N").unwrap();
            let in_newtons = q.convert_to(&n_unit).unwrap();
            assert!(
                (in_newtons.value.mean() - 981.747).abs() < 0.1,
                "expected ~981.747 N, got {}",
                in_newtons.value.mean()
            );
        } else {
            panic!("Expected Quantity");
        }
    }

    fn geom_module() -> PhsModule {
        PhsModule::from_source("geom", "fn area_tubo(d: mm) = 3.1415926535 * (d / 2)^2").unwrap()
    }

    fn hydr_module() -> PhsModule {
        PhsModule::from_source("hydr", "fn fuerza_empuje(P: bar, A: mm^2) = P * A").unwrap()
    }

    #[test]
    fn test_pipeline_two_step_execution_resolves_reference_between_steps() {
        let mut pipeline = PhsPipeline::new();
        pipeline.add_module(geom_module());
        pipeline.add_module(hydr_module());

        pipeline.add_step(PipelineStep {
            module_name: "geom".to_string(),
            function_name: "area_tubo".to_string(),
            inputs: HashMap::from([(
                "d".to_string(),
                PipelineArg::Literal(PhsValue::Quantity(
                    physure_core::Quantity::new(50.0, "mm").unwrap(),
                )),
            )]),
            output_alias: "area".to_string(),
        });
        pipeline.add_step(PipelineStep {
            module_name: "hydr".to_string(),
            function_name: "fuerza_empuje".to_string(),
            inputs: HashMap::from([
                (
                    "P".to_string(),
                    PipelineArg::Literal(PhsValue::Quantity(
                        physure_core::Quantity::new(5.0, "bar").unwrap(),
                    )),
                ),
                ("A".to_string(), PipelineArg::Reference("area".to_string())),
            ]),
            output_alias: "force".to_string(),
        });

        let scope = pipeline.execute().unwrap();
        assert_eq!(scope.len(), 2);

        let PhsValue::Quantity(force) = scope.get("force").expect("force output missing") else {
            panic!("expected force to be a Quantity");
        };
        let n_unit = physure_core::units::Parser::parse_expression("N").unwrap();
        let in_newtons = force.convert_to(&n_unit).unwrap();
        assert!(
            (in_newtons.value.mean() - 981.747).abs() < 0.1,
            "expected ~981.747 N, got {}",
            in_newtons.value.mean()
        );

        // The intermediate step's own output is also in scope, independently inspectable.
        let PhsValue::Quantity(area) = scope.get("area").expect("area output missing") else {
            panic!("expected area to be a Quantity");
        };
        assert!((area.value.mean() - 1963.495).abs() < 0.01);
    }

    #[test]
    fn test_pipeline_unresolved_reference_produces_clear_error() {
        let mut pipeline = PhsPipeline::new();
        pipeline.add_module(hydr_module());

        pipeline.add_step(PipelineStep {
            module_name: "hydr".to_string(),
            function_name: "fuerza_empuje".to_string(),
            inputs: HashMap::from([
                (
                    "P".to_string(),
                    PipelineArg::Literal(PhsValue::Quantity(
                        physure_core::Quantity::new(5.0, "bar").unwrap(),
                    )),
                ),
                // No earlier step ever produced "area" -- this must fail, not panic or
                // silently substitute a default.
                ("A".to_string(), PipelineArg::Reference("area".to_string())),
            ]),
            output_alias: "force".to_string(),
        });

        let err = pipeline.execute().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Unresolved reference") && msg.contains("area"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn test_pipeline_missing_module_produces_clear_error() {
        let mut pipeline = PhsPipeline::new();
        // Note: "hydr" module is deliberately never added.
        pipeline.add_step(PipelineStep {
            module_name: "hydr".to_string(),
            function_name: "fuerza_empuje".to_string(),
            inputs: HashMap::new(),
            output_alias: "force".to_string(),
        });

        let err = pipeline.execute().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Module 'hydr' not found"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn test_pipeline_missing_function_produces_clear_error() {
        let mut pipeline = PhsPipeline::new();
        pipeline.add_module(hydr_module());
        pipeline.add_step(PipelineStep {
            module_name: "hydr".to_string(),
            function_name: "not_a_real_function".to_string(),
            inputs: HashMap::new(),
            output_alias: "force".to_string(),
        });

        let err = pipeline.execute().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Function 'not_a_real_function' not found"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn test_pipeline_missing_input_for_a_required_parameter_produces_clear_error() {
        let mut pipeline = PhsPipeline::new();
        pipeline.add_module(geom_module());
        pipeline.add_step(PipelineStep {
            module_name: "geom".to_string(),
            function_name: "area_tubo".to_string(),
            // "d" is area_tubo's only parameter, and it's never supplied.
            inputs: HashMap::new(),
            output_alias: "area".to_string(),
        });

        let err = pipeline.execute().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Missing input 'd'") && msg.contains("step 'area'"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn test_pipeline_propagates_dimension_mismatch_from_invoke() {
        // A pipeline step's arguments still go through invoke()'s real unit coercion --
        // passing a dimensionally incompatible quantity must surface invoke()'s own error,
        // not be silently accepted or produce a bogus result.
        let mut pipeline = PhsPipeline::new();
        pipeline.add_module(geom_module());
        pipeline.add_step(PipelineStep {
            module_name: "geom".to_string(),
            function_name: "area_tubo".to_string(),
            inputs: HashMap::from([(
                "d".to_string(),
                PipelineArg::Literal(PhsValue::Quantity(
                    physure_core::Quantity::new(5.0, "s").unwrap(),
                )),
            )]),
            output_alias: "area".to_string(),
        });

        let err = pipeline.execute().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("parameter 'd'")
                && (msg.contains("Dimension mismatch") || msg.contains("incompatible")),
            "unexpected message: {msg}"
        );
    }
}
