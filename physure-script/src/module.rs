//! Introspection over, and dynamic invocation of, a parsed `.phs` source: exposes the
//! functions a script defines, their declared parameter units, and their doc comments,
//! without requiring a caller to walk the AST directly, plus a way to call them with
//! host-supplied argument values.
//!
//! This is the foundation for the foreign-execution bridge (see
//! `docs/superpowers/plans/2026-08-27-phs-foreign-bridge.md`): a host language loads a
//! `PhsModule` to discover what it can call and with what units, then invokes it via
//! [`PhsModule::invoke`]. Formula composition/pipelines is a later stage built on top of this
//! and is deliberately not implemented here.

use crate::{PhsInterpreter, PhsValue, Statement};
use physure_core::error::{PhysureError, PhysureResult};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One function parameter's name and its declared unit constraint, if any.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamInfo {
    pub name: String,
    /// The unit expression exactly as written in the `.phs` source (e.g. `"kg"`,
    /// `"m/s"`), or `None` if the parameter has no declared unit.
    pub expected_unit: Option<String>,
}

/// A function's callable shape: name, parameters (with units), and doc comment.
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionSignature {
    pub name: String,
    /// The function's `///` doc comment, if it has one. See
    /// [`crate::ast::FunctionDefNode::doc`] for the exact extraction rules.
    pub docstring: Option<String>,
    pub params: Vec<ParamInfo>,
}

/// A parsed `.phs` source, with every top-level function's signature extracted and a live
/// [`PhsInterpreter`] whose environment already has every top-level statement evaluated
/// (functions registered, module-level assignments bound) — ready for a caller to invoke
/// functions against.
///
/// `interpreter` is deliberately private: a caller mutating `interpreter`'s environment
/// directly (adding/removing bindings) could desync it from the cached signature map with no
/// way to detect it. Exposing a narrow accessor is a decision for whichever task first needs
/// read access from outside this module.
///
/// [`PhsModule::invoke`] (Task 2) is the first consumer, and it deliberately does *not* use
/// `functions` for dimensional coercion. The original plan called for re-parsing each
/// parameter's declared-unit string with `physure_core::units::Parser::parse_expression_atomic`
/// and comparing `RationalUnit::same_dimensions` by hand, then formatting the coerced
/// arguments back into a PHS call expression string and re-parsing it through
/// `PhsInterpreter::eval_str`. Both halves of that turned out to be unsound:
///
/// - `parse_expression_atomic` treats every unit token as its own atomic dimension with no
///   registry lookup (that is its documented purpose), so it never expands a named derived
///   unit or a prefix -- `"N"` parses to the single dimension `{"N": 1}`, not
///   `{"kg": 1, "m": 1, "s": -2}`, and `"km"` parses to `{"km": 1}`, not `{"m": 1}` scaled by
///   1000. Meanwhile every real `Quantity` (via `Quantity::new`, which calls
///   `Parser::parse_expression`) carries the registry-resolved, base-SI-decomposed form.
///   Comparing the two with `same_dimensions` would reject perfectly valid calls for any
///   function whose declared unit is a named/prefixed unit -- i.e. most physics.
/// - `PhsValue`'s `Display` is not round-trippable through the PHS parser for every shape:
///   `Vector`s over 4 elements print as `"[a, b, c, ... (N items)]"`, `Function`/`Plot` print
///   human-readable summaries, not expressions, and none of that is valid PHS syntax to
///   re-parse.
///
/// Instead, `invoke()` looks the target function up as a `PhsValue::Function` in
/// `interpreter.env` and calls `PhsInterpreter::call_function_node` directly -- the same
/// internal path a native PHS-to-PHS call site uses. That function's own parameter-binding
/// step (`bind_param_value`) already does correct, registry-aware dimensional coercion via
/// `UnitParser::parse_expression` (not the atomic variant) and `Quantity::convert_to`, and
/// already enforces `@requires`/`@ensures`. Reusing it means foreign callers get identical
/// semantics to PHS-to-PHS calls, with no second, easier-to-drift implementation of unit
/// coercion living in this file.
///
/// One consequence of executing against live `env` bindings rather than `functions` is worth
/// calling out: a source that reassigns a function's name to a non-function value after
/// defining it (`"fn add(a, b) = a + b\nadd = 5.0"`), or that aliases a function under a new
/// name (`"fn add(a, b) = a + b\nalias = add"`), can make `env` and `functions` disagree about
/// what is callable -- `env` would happily call through an alias `functions` never heard of,
/// and would no longer be able to call a name `functions` still lists. `invoke()` resolves
/// that disagreement by treating `functions` (Task 1's introspection map, and the one a
/// foreign caller inspects before deciding what to invoke) as the sole authority on which
/// *names* are callable: it rejects any `fn_name` not in `functions` before ever consulting
/// `env`, rather than trying to keep the two structures in sync by some other mechanism.
pub struct PhsModule {
    pub name: String,
    /// The path this module was loaded from, or `None` for a module built from an
    /// in-memory source string via [`PhsModule::from_source`].
    pub path: Option<PathBuf>,
    /// Every top-level function's signature, keyed by name. If the source defines the same
    /// function name twice, the later definition wins — matching
    /// `PhsInterpreter::eval_statement`'s own `HashMap::insert` on `Statement::FunctionDef`,
    /// which the same source drives when populating `interpreter`'s environment below.
    pub functions: HashMap<String, FunctionSignature>,
    // Holds the live, callable AST (`PhsValue::Function` bindings in `env`) that
    // `invoke()` calls into. Kept private -- see the struct doc above.
    interpreter: PhsInterpreter,
}

impl PhsModule {
    /// Parses `source` as PHS, extracts every top-level function's signature, and evaluates
    /// every top-level statement into a fresh interpreter so the module is immediately ready
    /// to have its functions invoked.
    ///
    /// # Errors
    ///
    /// Returns an error if `source` fails to parse, or if evaluating any top-level statement
    /// fails (e.g. a module-level assignment referencing an unknown unit).
    pub fn from_source(name: &str, source: &str) -> PhysureResult<Self> {
        let program = crate::parser::parse_phs(source)?;
        let mut interpreter = PhsInterpreter::default();
        let mut functions = HashMap::new();

        for stmt in &program.statements {
            if let Statement::FunctionDef(f) = stmt {
                let params = f
                    .params
                    .iter()
                    .enumerate()
                    .map(|(i, p)| ParamInfo {
                        name: p.clone(),
                        expected_unit: f.param_units.get(i).cloned().flatten(),
                    })
                    .collect();

                functions.insert(
                    f.name.clone(),
                    FunctionSignature {
                        name: f.name.clone(),
                        docstring: f.doc.clone(),
                        params,
                    },
                );
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

    /// Reads `.phs` source from `path` and builds a module from it, named after the file's
    /// stem (e.g. `physics.phs` -> module name `"physics"`).
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, or for any reason [`Self::from_source`]
    /// would fail.
    pub fn from_file(path: impl AsRef<Path>) -> PhysureResult<Self> {
        let p = path.as_ref();
        let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("module");
        let content = std::fs::read_to_string(p)
            .map_err(|e| PhysureError::Generic(format!("Failed to read {}: {}", p.display(), e)))?;
        let mut m = Self::from_source(name, &content)?;
        m.path = Some(p.to_path_buf());
        Ok(m)
    }

    /// Calls the top-level function named `fn_name` with `args`, coercing any `Quantity`
    /// argument to its parameter's declared unit (raising on a dimensionally incompatible
    /// one) exactly as a native PHS-to-PHS call would.
    ///
    /// This delegates to [`PhsInterpreter::call_function_node`] against the module's own
    /// `PhsValue::Function` binding rather than re-implementing unit coercion in this file --
    /// see the struct-level doc comment above for why. A consequence of that is `@requires`/
    /// `@ensures` contracts on the target function are enforced here too, not skipped.
    ///
    /// `fn_name` is checked against `self.functions` -- Task 1's introspection map -- before
    /// anything else, so a name invisible to introspection (an alias, or any other binding
    /// that isn't a top-level `fn` definition) is never callable through here even if it
    /// happens to resolve to a `PhsValue::Function` in `env`. See the struct-level doc comment
    /// above for why that gate exists.
    ///
    /// # Errors
    ///
    /// Returns an error if `fn_name` is not a key in `self.functions` (i.e. not a function
    /// this module's introspection lists as callable), if `args.len()` doesn't match the
    /// function's parameter count, if a `Quantity` argument's unit is dimensionally
    /// incompatible with its parameter's declared unit, or if evaluating the function body
    /// itself fails (e.g. a `@requires`/`@ensures` contract violation).
    pub fn invoke(&self, fn_name: &str, args: &[PhsValue]) -> PhysureResult<PhsValue> {
        if !self.functions.contains_key(fn_name) {
            return Err(PhysureError::Generic(format!(
                "'{}' is not a function this module ('{}') exports",
                fn_name, self.name
            )));
        }
        let Some(PhsValue::Function(func)) = self.interpreter.env.get(fn_name) else {
            return Err(PhysureError::Generic(format!(
                "'{}' is listed as a function in module '{}' but its binding was reassigned \
                 to a non-function value after definition",
                fn_name, self.name
            )));
        };
        self.interpreter
            .call_function_node(func, args.to_vec(), &self.interpreter.env)
    }

    /// Composes `outer_fn` (a function of `self`) with `inner_fn` (a function of `inner`),
    /// producing a single callable that first evaluates `inner_fn` and feeds its result into
    /// `outer_fn`'s `bind_param` parameter.
    ///
    /// This is cross-module composition — `self` and `inner` may be (and in the motivating
    /// use case, are) two different `.phs` files loaded independently, e.g. a hydraulics
    /// module's `fuerza_empuje(P, A)` composed with a geometry module's `area_tubo(d)` bound
    /// to `fuerza_empuje`'s `A` parameter. The design spec's `compose(&self, outer_fn,
    /// inner_fn, bind_param) -> PhysureResult<PhsFunction>` (same-module, single `&self`,
    /// undefined `PhsFunction` return type) does not cover this — it was superseded before
    /// this method existed by the plan's own test, which requires composing across two
    /// independently-loaded modules. See `docs/superpowers/plans/2026-08-27-phs-foreign-bridge.md`
    /// Task 3 for the full history; the narrower same-module form isn't implemented since
    /// nothing in this crate currently needs it.
    ///
    /// The returned [`ComposedFunction`] borrows both modules (`'a`) rather than owning cloned
    /// copies of them: `PhsModule` deliberately doesn't derive `Clone` (see the struct doc
    /// comment above), and there's no other cheap way to keep two live, independently-invokable
    /// modules around after this call returns. A plain borrow is sufficient because `invoke`
    /// only needs `&self`, so nothing about calling the composed function later requires
    /// mutable or owned access to either module — the caller just needs to keep both `self`
    /// and `inner` alive for as long as it holds onto the `ComposedFunction`, which a pipeline
    /// or any other in-process caller naturally does. If a future caller needs a composed
    /// function to outlive the modules it was built from (e.g. handing it across an FFI
    /// boundary that can't express a borrow), that's a job for `Arc<PhsModule>` at that call
    /// site — not a reason to make every composition here pay for shared ownership it doesn't
    /// need.
    ///
    /// `ComposedFunction::call`'s expected argument order is: `outer_fn`'s parameters, in their
    /// declared order, *skipping* `bind_param` (which is supplied by `inner_fn`'s result
    /// instead of the caller), followed by `inner_fn`'s parameters in their declared order.
    /// E.g. for `fuerza_empuje(P, A)` composed with `area_tubo(d)` bound to `"A"`, the composed
    /// function takes `[P, d]` — `A` is never supplied directly.
    ///
    /// # Errors
    ///
    /// Returns an error if `outer_fn` is not a function `self` exports, if `inner_fn` is not a
    /// function `inner` exports, or if `bind_param` does not name a parameter of `outer_fn`.
    /// These are all checked eagerly, here, rather than deferred to the first `call()` — a
    /// caller that successfully builds a `ComposedFunction` can trust it's actually callable.
    pub fn compose_with<'a>(
        &'a self,
        inner: &'a PhsModule,
        outer_fn: &str,
        inner_fn: &str,
        bind_param: &str,
    ) -> PhysureResult<ComposedFunction<'a>> {
        let outer_sig = self.functions.get(outer_fn).ok_or_else(|| {
            PhysureError::Generic(format!(
                "'{}' is not a function this module ('{}') exports",
                outer_fn, self.name
            ))
        })?;
        let inner_sig = inner.functions.get(inner_fn).ok_or_else(|| {
            PhysureError::Generic(format!(
                "'{}' is not a function the inner module ('{}') exports",
                inner_fn, inner.name
            ))
        })?;
        let bind_index = outer_sig
            .params
            .iter()
            .position(|p| p.name == bind_param)
            .ok_or_else(|| {
                PhysureError::Generic(format!(
                    "outer function '{}' has no parameter named '{}' to bind '{}''s result to",
                    outer_fn, bind_param, inner_fn
                ))
            })?;

        Ok(ComposedFunction {
            outer: self,
            inner,
            outer_fn: outer_fn.to_string(),
            inner_fn: inner_fn.to_string(),
            outer_arity: outer_sig.params.len(),
            inner_arity: inner_sig.params.len(),
            bind_index,
        })
    }
}

/// A callable produced by [`PhsModule::compose_with`]: calling it evaluates the inner
/// function first, then splices its result into the outer function's `bind_param` position
/// and evaluates the outer function.
///
/// Borrows both modules for `'a` rather than owning them — see the doc comment on
/// `compose_with` for why a borrow (and not `Arc`) is the right call here.
///
/// `Debug` is derived manually (rather than via `#[derive(Debug)]`) because `PhsModule` itself
/// has no `Debug` impl (its private `interpreter` field holds live `Arc<Mutex<..>>` state that
/// isn't meaningfully printable) -- printing a `ComposedFunction` shows the composition's
/// shape (which functions, which modules by name, arities, bind position) without trying to
/// print either module's full contents.
pub struct ComposedFunction<'a> {
    outer: &'a PhsModule,
    inner: &'a PhsModule,
    outer_fn: String,
    inner_fn: String,
    /// `outer_fn`'s total parameter count, captured at `compose_with` time (immutable
    /// thereafter — a `PhsModule`'s `functions` map never changes after construction).
    outer_arity: usize,
    inner_arity: usize,
    /// Index of `bind_param` within `outer_fn`'s parameter list.
    bind_index: usize,
}

impl std::fmt::Debug for ComposedFunction<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComposedFunction")
            .field("outer_module", &self.outer.name)
            .field("outer_fn", &self.outer_fn)
            .field("inner_module", &self.inner.name)
            .field("inner_fn", &self.inner_fn)
            .field("outer_arity", &self.outer_arity)
            .field("inner_arity", &self.inner_arity)
            .field("bind_index", &self.bind_index)
            .finish()
    }
}

impl<'a> ComposedFunction<'a> {
    /// Calls the composed function. `args` must supply exactly `outer_fn`'s parameters minus
    /// `bind_param`, followed by all of `inner_fn`'s parameters — see the ordering documented
    /// on [`PhsModule::compose_with`].
    ///
    /// Both the inner and outer calls go through [`PhsModule::invoke`], so both stages get
    /// full registry-aware unit coercion and `@requires`/`@ensures` enforcement exactly as if
    /// each were called directly — composition adds no second, easier-to-drift code path for
    /// dimensional correctness.
    ///
    /// # Errors
    ///
    /// Returns an error if `args.len()` doesn't match the expected arity (outer arity - 1 +
    /// inner arity), or if either the inner or outer `invoke()` call fails (dimension
    /// mismatch, contract violation, etc.).
    pub fn call(&self, args: &[PhsValue]) -> PhysureResult<PhsValue> {
        // `bind_index < outer_arity` is guaranteed by `compose_with` (it comes from
        // `position()` over `outer_fn`'s own params), so this subtraction never underflows.
        let outer_partial_arity = self.outer_arity - 1;
        let expected_total = outer_partial_arity + self.inner_arity;
        if args.len() != expected_total {
            return Err(PhysureError::Generic(format!(
                "composed function ('{}' \u{2218} '{}') expects {} argument(s) \
                 ({} for the outer call, {} for the inner call), got {}",
                self.outer_fn,
                self.inner_fn,
                expected_total,
                outer_partial_arity,
                self.inner_arity,
                args.len()
            )));
        }

        let (outer_partial, inner_args) = args.split_at(outer_partial_arity);
        let inner_result = self.inner.invoke(&self.inner_fn, inner_args)?;

        // `outer_partial` already holds every outer argument *except* `bind_param`, in the
        // declared order they appear once `bind_param`'s slot is skipped -- so re-inserting
        // the inner result at `bind_index` recovers the full, correctly-ordered argument
        // list. `Vec::insert` handles `bind_index` being any valid position, including the
        // last one, with no manual index bookkeeping.
        let mut full_outer_args = outer_partial.to_vec();
        full_outer_args.insert(self.bind_index, inner_result);

        self.outer.invoke(&self.outer_fn, &full_outer_args)
    }
}

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
        assert_eq!(
            ek_sig.docstring.as_deref(),
            Some(
                "Computes kinetic energy in Joules\n@param m Mass of the body in kg\n\
                 @param v Velocity in m/s\n@returns Energy in Joules"
            )
        );
        assert_eq!(ek_sig.params.len(), 2);
        assert_eq!(ek_sig.params[0].name, "m");
        assert_eq!(ek_sig.params[0].expected_unit.as_deref(), Some("kg"));
        assert_eq!(ek_sig.params[1].name, "v");
        assert_eq!(ek_sig.params[1].expected_unit.as_deref(), Some("m/s"));
    }

    #[test]
    fn docstring_is_none_when_function_has_no_doc_comment() {
        let code = "fn add(a: m, b: m) = a + b";
        let module = PhsModule::from_source("mathy", code).expect("Module parsing failed");
        let sig = &module.functions["add"];
        assert_eq!(sig.docstring, None);
    }

    #[test]
    fn params_have_no_unit_when_none_declared() {
        // `P(x, y) = ...` style function definitions carry no `: unit` annotations at all,
        // so every param's `expected_unit` should come back `None` rather than panicking on
        // a short/missing `param_units` entry.
        let code = "P(x, y) = 100.0 kPa * x / y";
        let module = PhsModule::from_source("shorthand", code).expect("Module parsing failed");
        let sig = &module.functions["P"];
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.params[0].expected_unit, None);
        assert_eq!(sig.params[1].expected_unit, None);
    }

    #[test]
    fn from_file_reads_source_sets_path_and_name_from_file_stem() {
        let dir = std::env::temp_dir();
        let stem = format!("physure_module_test_{}", std::process::id());
        let file_path = dir.join(format!("{stem}.phs"));
        std::fs::write(&file_path, "fn double(x: m) = 2.0 * x").expect("failed to write temp file");

        let result = PhsModule::from_file(&file_path);
        std::fs::remove_file(&file_path).ok();

        let module = result.expect("Module parsing failed");
        assert_eq!(module.name, stem);
        assert_eq!(module.path.as_deref(), Some(file_path.as_path()));
        assert!(module.functions.contains_key("double"));
    }

    #[test]
    fn from_file_errors_when_file_does_not_exist() {
        let missing = std::env::temp_dir().join("physure_module_test_definitely_missing.phs");
        let result = PhsModule::from_file(&missing);
        assert!(result.is_err());
    }

    #[test]
    fn from_source_propagates_parse_errors() {
        let result = PhsModule::from_source("broken", "fn (((( invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_invoke_with_quantity_coercion() {
        let code = "fn E_k(m: kg, v: m/s) = 0.5 * m * v^2";
        let module = PhsModule::from_source("ke", code).unwrap();

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
        let module = PhsModule::from_source("ke", code).unwrap();

        // Passing seconds instead of velocity
        let m_val = PhsValue::Quantity(physure_core::Quantity::new(10.0, "kg").unwrap());
        let invalid_v = PhsValue::Quantity(physure_core::Quantity::new(5.0, "s").unwrap());

        let err = module.invoke("E_k", &[m_val, invalid_v]).unwrap_err();
        let msg = err.to_string();
        // `bind_param_value`'s wrapper text always contains "incompatible", so asserting on
        // that alone can't tell a well-formed message from a malformed one -- pin down the
        // parameter name too, so this only passes if the error actually names what went wrong.
        assert!(
            msg.contains("parameter 'v'")
                && (msg.contains("Dimension mismatch") || msg.contains("incompatible")),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn test_invoke_coerces_a_differently_scaled_but_compatible_unit() {
        // Real dimensional coercion, not just a same-unit no-op: 5000 g into a `kg` parameter
        // must convert (and produce the same physical answer as passing 5 kg directly), per
        // `bind_param_value`'s documented "5 cm passed to a (r: m) parameter" contract.
        let code = "fn double_mass(m: kg) = 2.0 * m";
        let module = PhsModule::from_source("massy", code).unwrap();

        let grams = PhsValue::Quantity(physure_core::Quantity::new(5000.0, "g").unwrap());
        let res = module.invoke("double_mass", &[grams]).unwrap();
        let PhsValue::Quantity(q) = res else {
            panic!("Expected Quantity result")
        };
        assert_eq!(q.unit.__repr__(), "kg");
        assert_eq!(q.value.mean(), 10.0);
    }

    #[test]
    fn test_invoke_passes_unitless_number_through_when_param_has_no_declared_unit() {
        let code = "scale(x, y) = x * y";
        let module = PhsModule::from_source("shorthand", code).unwrap();

        let res = module
            .invoke("scale", &[PhsValue::Number(3.0), PhsValue::Number(4.0)])
            .unwrap();
        match res {
            PhsValue::Number(n) => assert_eq!(n, 12.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 12.0),
            other => panic!("Expected a numeric result, got {other:?}"),
        }
    }

    #[test]
    fn test_invoke_zero_arg_function() {
        let code = "fn answer() = 42.0";
        let module = PhsModule::from_source("consts", code).unwrap();
        let res = module.invoke("answer", &[]).unwrap();
        match res {
            PhsValue::Number(n) => assert_eq!(n, 42.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 42.0),
            other => panic!("Expected a numeric result, got {other:?}"),
        }
    }

    #[test]
    fn test_invoke_errors_on_argument_count_mismatch() {
        let code = "fn add(a: m, b: m) = a + b";
        let module = PhsModule::from_source("mathy", code).unwrap();
        let only_arg = PhsValue::Quantity(physure_core::Quantity::new(1.0, "m").unwrap());
        let err = module.invoke("add", &[only_arg]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains('2') && msg.contains('1'),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn test_invoke_errors_when_function_not_found() {
        let code = "fn add(a: m, b: m) = a + b";
        let module = PhsModule::from_source("mathy", code).unwrap();
        let err = module.invoke("subtract", &[]).unwrap_err();
        assert!(err.to_string().contains("is not a function this module"));
    }

    #[test]
    fn test_invoke_rejects_an_alias_invisible_to_introspection() {
        // `alias` resolves to a real, callable `PhsValue::Function` in the interpreter's
        // `env` (PHS lets you bind any value, including a function, to a new name), but
        // Task 1's `functions` map -- built only from `Statement::FunctionDef` nodes -- never
        // heard of it. `invoke()` must side with `functions`: a foreign caller who inspected
        // `module.functions` first (the documented way to discover what's callable) never
        // saw "alias" listed, so calling it anyway would let it invoke something invisible to
        // introspection, silently defeating the "discover before invoking" contract.
        let code = "fn add(a: m, b: m) = a + b\nalias = add";
        let module = PhsModule::from_source("mathy", code).unwrap();
        assert!(!module.functions.contains_key("alias"));

        let err = module.invoke("alias", &[]).unwrap_err();
        assert!(err.to_string().contains("is not a function this module"));
    }

    #[test]
    fn test_invoke_reports_a_clear_error_when_a_function_name_is_reassigned() {
        // The mirror-image gap: `functions` still lists `add` as a 2-param function (it was
        // captured once, at definition time, and never re-scanned), but the source rebinds
        // the name to a plain quantity afterwards, so `env` no longer has a function there.
        // This must not panic or silently coerce the quantity into a call -- it should fail
        // with a message that points at the real cause instead of a bare "not found".
        let code = "fn add(a: m, b: m) = a + b\nadd = 5.0 m";
        let module = PhsModule::from_source("mathy", code).unwrap();
        assert!(module.functions.contains_key("add"));

        let err = module.invoke("add", &[]).unwrap_err();
        assert!(err.to_string().contains("reassigned"));
    }

    #[test]
    fn test_invoke_enforces_requires_contract() {
        // The struct-level doc comment claims @requires/@ensures are enforced "for free" for
        // foreign callers because invoke() reuses call_function_node -- this is the test that
        // actually demonstrates it through the invoke() entry point, rather than only through
        // eval_str (already covered in interpreter::tests).
        let code = "@requires(m > 0.0, \"mass must be positive\")\nfn double_mass(m) = m * 2.0";
        let module = PhsModule::from_source("guarded", code).unwrap();

        let err = module
            .invoke("double_mass", &[PhsValue::Number(-1.0)])
            .unwrap_err();
        assert!(
            matches!(err, PhysureError::ContractViolation { ref decorator, .. } if decorator == "requires"),
            "expected a requires ContractViolation, got {err:?}"
        );

        let ok = module
            .invoke("double_mass", &[PhsValue::Number(2.0)])
            .unwrap();
        match ok {
            PhsValue::Number(n) => assert_eq!(n, 4.0),
            PhsValue::Quantity(q) => assert_eq!(q.value.mean(), 4.0),
            other => panic!("Expected a numeric result, got {other:?}"),
        }
    }

    #[test]
    fn test_invoke_enforces_ensures_contract() {
        let code = "@ensures(result > 100.0, \"result must exceed 100\")\nfn small(m) = m";
        let module = PhsModule::from_source("guarded", code).unwrap();

        let err = module
            .invoke("small", &[PhsValue::Number(1.0)])
            .unwrap_err();
        assert!(
            matches!(err, PhysureError::ContractViolation { ref decorator, .. } if decorator == "ensures"),
            "expected an ensures ContractViolation, got {err:?}"
        );
    }

    // -- compose_with -------------------------------------------------------------------
    //
    // The main "does composition actually chain unit coercion end-to-end across two real
    // modules" test lives in `pipeline.rs`, matching where the original plan asked for it
    // (`compose_with` and `PhsPipeline` share that test file's intent). These tests instead
    // cover `compose_with`'s own error paths, which are specific to `PhsModule` and belong
    // next to `invoke`'s equivalent error-path tests above.

    #[test]
    fn test_compose_with_errors_when_outer_fn_missing() {
        let outer = PhsModule::from_source("outer", "fn f(a: m) = a").unwrap();
        let inner = PhsModule::from_source("inner", "fn g(b: m) = b").unwrap();

        let err = outer
            .compose_with(&inner, "nonexistent", "g", "a")
            .unwrap_err();
        assert!(err.to_string().contains("is not a function this module"));
    }

    #[test]
    fn test_compose_with_errors_when_inner_fn_missing() {
        let outer = PhsModule::from_source("outer", "fn f(a: m) = a").unwrap();
        let inner = PhsModule::from_source("inner", "fn g(b: m) = b").unwrap();

        let err = outer
            .compose_with(&inner, "f", "nonexistent", "a")
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("is not a function the inner module"));
    }

    #[test]
    fn test_compose_with_errors_when_bind_param_not_a_param_of_outer_fn() {
        let outer = PhsModule::from_source("outer", "fn f(a: m) = a").unwrap();
        let inner = PhsModule::from_source("inner", "fn g(b: m) = b").unwrap();

        let err = outer
            .compose_with(&inner, "f", "g", "not_a_param")
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("no parameter named 'not_a_param'"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn test_composed_function_splices_bind_param_result_into_the_middle_of_outer_args() {
        // The plan's own composition test only exercises binding the *last* outer parameter
        // (`fuerza_empuje(P, A)` bound on `A`), which never has to prove the splice actually
        // reinserts the inner result at the right position -- pushing it last would look
        // identical. Use a 3-param outer function bound on its *middle* parameter, with each
        // param declared in a genuinely *different* unit (m / s / kg), so a bug that always
        // appended the inner result instead of inserting it at `bind_index` can't hide behind
        // a coincidentally-correct number the way a same-unit sum would: it would misalign
        // every later positional argument against a differently-dimensioned parameter, and
        // `invoke`'s own `bind_param_value` would reject it with a real dimension-mismatch
        // error rather than silently returning a wrong-but-plausible answer.
        let outer = PhsModule::from_source("outer", "fn f(a: m, b: s, c: kg) = b * 2.0").unwrap();
        let inner = PhsModule::from_source("inner", "fn g(x: s) = x * 3.0").unwrap();
        let composed = outer.compose_with(&inner, "f", "g", "b").unwrap();

        let a = PhsValue::Quantity(physure_core::Quantity::new(2.0, "m").unwrap());
        let c = PhsValue::Quantity(physure_core::Quantity::new(4.0, "kg").unwrap());
        let x = PhsValue::Quantity(physure_core::Quantity::new(5.0, "s").unwrap());
        // Composed argument order: outer's params minus "b" (i.e. [a, c]), then inner's
        // params (i.e. [x]) -- so [a=2m, c=4kg, x=5s]. A correct splice binds b = g(5s) =
        // 15s, giving f(2m, 15s, 4kg) = 30s. An append-instead-of-insert bug would instead
        // bind b <- c's value (4 kg) -- dimensionally incompatible with b's declared unit
        // `s` -- and this call would fail with a dimension mismatch instead of quietly
        // returning a wrong number.
        let res = composed.call(&[a, c, x]).unwrap();

        let PhsValue::Quantity(q) = res else {
            panic!("Expected Quantity result");
        };
        assert_eq!(q.unit.__repr__(), "s");
        assert_eq!(q.value.mean(), 30.0);
    }

    #[test]
    fn test_composed_function_call_errors_on_arity_mismatch() {
        // f(a, c) composed on "c" expects [a, d] (2 args: 1 outer partial + 1 inner) -- passing
        // 3 must fail with a clear arity error rather than panicking on the split_at/zip.
        let outer = PhsModule::from_source("outer", "fn f(a: m, c: m) = a + c").unwrap();
        let inner = PhsModule::from_source("inner", "fn g(d: m) = d").unwrap();
        let composed = outer.compose_with(&inner, "f", "g", "c").unwrap();

        let one_m = PhsValue::Quantity(physure_core::Quantity::new(1.0, "m").unwrap());
        let err = composed
            .call(&[one_m.clone(), one_m.clone(), one_m])
            .unwrap_err();
        assert!(err.to_string().contains("expects 2 argument"));
    }
}
