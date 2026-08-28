//! Introspection over a parsed `.phs` source: exposes the functions a script defines,
//! their declared parameter units, and their doc comments, without requiring a caller to
//! walk the AST directly.
//!
//! This is the foundation for the foreign-execution bridge (see
//! `docs/superpowers/plans/2026-08-27-phs-foreign-bridge.md`): a host language loads a
//! `PhsModule` to discover what it can call and with what units, before invoking anything.
//! Dynamic invocation and formula composition are later stages built on top of this and are
//! deliberately not implemented here.

use crate::{PhsInterpreter, Statement};
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
/// `interpreter` is deliberately private: Task 2's `invoke()` will consult `functions` for
/// unit-coercion metadata before calling into the interpreter, and a caller mutating
/// `interpreter`'s environment directly (adding/removing bindings) could desync it from the
/// cached signature map with no way to detect it. Exposing a narrow accessor is a decision
/// for whichever task first needs read access from outside this module.
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
    // Not read anywhere yet -- Task 2's `invoke()` is the first consumer. Kept private (see
    // the struct doc above) rather than `pub` to keep it in sync with `functions`.
    #[allow(dead_code)]
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
}
