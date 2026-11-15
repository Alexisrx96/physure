//! A module is evaluated in a scope of its own: once, in the order it defines its names, where the names of whoever
//! imports it are not visible. An imported value is the value the module computed; an imported function sees the
//! names of its module, never those of the importer, and takes only its arguments from the call.
//!
//! Before this file a module's definitions were evaluated in the importer's environment, and its functions ran
//! there too. With `lib.phs` = `k = 2; r = 3*k; f(x) = k*x`:
//!
//! - `use r from "lib"` failed with "Unknown unit 'k'";
//! - `k = 10; use r from "lib"; r` gave 30 (it is 6);
//! - `use f from "lib"; f(1)` failed;
//! - `k = 10; use f from "lib"; f(1)` gave 10 (it is 2).
//!
//! Every expected value is worked out here from the module's text, never copied from the engine's output.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use physure_script::interpreter::PhsInterpreter;
use physure_script::resolver::{MemoryModuleResolver, ModuleExport};
use physure_script::value::PhsValue;
use physure_script::{parse_phs, Expr, Statement};

/// A directory of its own for one test, holding `files`.
struct Modules(PathBuf);

impl Modules {
    fn new(test: &str, files: &[(&str, &str)]) -> Self {
        let dir = std::env::temp_dir().join(format!("physure_module_scope_{test}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let modules = Modules(dir);
        for (name, text) in files {
            modules.write(name, text);
        }
        modules
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.0.join(name), text).unwrap();
    }

    fn interpreter(&self) -> PhsInterpreter {
        PhsInterpreter::with_base_dir(&self.0)
    }

    /// The values of the statements of `program`, run in a new interpreter that imports from this directory.
    fn run(&self, program: &str) -> Vec<PhsValue> {
        self.interpreter().eval_str(program).unwrap_or_else(|e| panic!("{program:?} failed: {e}"))
    }

    /// The message with which `program` is refused.
    fn refused(&self, program: &str) -> String {
        match self.interpreter().eval_str(program) {
            Ok(values) => panic!("{program:?} should be refused, it gave {:?}", values.last().map(|v| v.to_string())),
            Err(e) => e.to_string(),
        }
    }
}

impl Drop for Modules {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const LIB: (&str, &str) = ("lib.phs", "k = 2\nr = 3*k\nf(x) = k*x\ng(x) = f(x) + k\n");

/// The mean of a value that is a number or a quantity.
fn mean(value: &PhsValue) -> f64 {
    match value {
        PhsValue::Number(n) => *n,
        PhsValue::Quantity(q) => q.value.mean(),
        other => panic!("expected a number or a quantity, got {other:?}"),
    }
}

fn last(values: &[PhsValue]) -> f64 {
    mean(values.last().expect("a value"))
}

// --- the four measured cases ---

#[test]
fn an_imported_value_is_computed_in_its_module_alone() {
    let modules = Modules::new("value_alone", &[LIB]);
    assert_eq!(last(&modules.run("use r from \"lib\"\nr")), 6.0);
}

#[test]
fn the_importers_names_do_not_reach_an_imported_value() {
    let modules = Modules::new("value_leak", &[LIB]);
    assert_eq!(last(&modules.run("k = 10\nuse r from \"lib\"\nr")), 6.0);
}

#[test]
fn an_imported_function_sees_the_names_of_its_module() {
    let modules = Modules::new("function_alone", &[LIB]);
    assert_eq!(last(&modules.run("use f from \"lib\"\nf(1)")), 2.0);
}

#[test]
fn the_importers_names_do_not_reach_an_imported_function() {
    let modules = Modules::new("function_leak", &[LIB]);
    assert_eq!(last(&modules.run("k = 10\nuse f from \"lib\"\nf(1)")), 2.0);
    // nor a name the importer binds after the import
    assert_eq!(last(&modules.run("use f from \"lib\"\nk = 10\nf(1)")), 2.0);
}

// --- the forms of `use` ---

#[test]
fn a_wildcard_import_binds_the_values_the_module_computed() {
    let modules = Modules::new("wildcard", &[LIB]);
    let values = modules.run("k = 10\nuse * from \"lib\"\nr\nf(1)\nk");
    let tail: Vec<f64> = values[values.len() - 3..].iter().map(mean).collect();
    // the module's `k` is imported with the rest, and replaces the importer's (as any import of a name does)
    assert_eq!(tail, [6.0, 2.0, 2.0]);
    // the importer's later `k` is its own; the module's function keeps the module's
    assert_eq!(last(&modules.run("use * from \"lib\"\nk = 10\nf(1)")), 2.0);
}

#[test]
fn an_aliased_import_is_the_same_value_under_another_name() {
    let modules = Modules::new("alias", &[LIB]);
    let values = modules.run("use f as ff, r as rr from \"lib\"\nk = 10\nff(1)\nrr");
    let tail: Vec<f64> = values[values.len() - 2..].iter().map(mean).collect();
    assert_eq!(tail, [2.0, 6.0]);
}

#[test]
fn a_function_of_a_module_calls_the_functions_of_its_module() {
    let modules = Modules::new("calls", &[LIB]);
    // `g(1)` is `f(1) + k` with the module's `f` and `k`: 2*1 + 2
    assert_eq!(last(&modules.run("k = 10\nf(x) = 100*x\nuse g from \"lib\"\ng(1)")), 4.0);
}

#[test]
fn a_renamed_export_is_computed_under_the_names_of_its_module() {
    let modules = Modules::new("renamed", &[("hidden.phs", "p = 2 m\nq = 3 * p\nexport p as P\nexport q as Q\n")]);
    let values = modules.run("use Q, P from \"hidden\"\nQ\nP");
    assert_eq!(mean(&values[1]), 6.0);
    assert!(values[1].to_string().ends_with(" m"), "{}", values[1]);
    assert_eq!(mean(&values[2]), 2.0);
    // what is not exported is not imported
    let message = modules.refused("use q from \"hidden\"");
    assert!(message.contains('q') && message.contains("hidden"), "{message}");
}

#[test]
fn a_module_imports_from_another_in_its_own_scope() {
    let modules = Modules::new(
        "nested",
        &[LIB, ("nested.phs", "use r from \"lib\"\nbase = r + 1\nh(x) = f2(x) * base\nf2(x) = 10 * x\n")],
    );
    // `base` is lib's `r` + 1 = 7, `h(1)` is 10*1*7; the importer's `r`, `k` and `f2` are not the module's
    let values = modules.run("r = 100\nk = 10\nf2(x) = x\nuse h, base from \"nested\"\nbase\nh(1)");
    let tail: Vec<f64> = values[values.len() - 2..].iter().map(mean).collect();
    assert_eq!(tail, [7.0, 70.0]);
    // what the module imported is its own: it does not appear in the importer
    let mut interpreter = modules.interpreter();
    interpreter.eval_str("use h from \"nested\"").unwrap();
    assert!(interpreter.get_var("r").is_none() && interpreter.get_var("base").is_none());
}

#[test]
fn a_name_the_module_does_not_define_is_refused_at_the_import() {
    let modules = Modules::new("missing", &[LIB]);
    let message = modules.refused("use nope from \"lib\"");
    assert!(message.contains("nope") && message.contains("lib"), "{message}");
    // a name a module exports but does not define is refused naming both, not reported as a missing module
    modules.write("bad_export.phs", "a = 1\nexport b\n");
    let message = modules.refused("use a from \"bad_export\"");
    assert!(message.contains("bad_export") && message.contains('b') && !message.contains("No such module"), "{message}");
}

#[test]
fn a_module_that_cannot_be_parsed_is_reported_as_such() {
    let modules = Modules::new("broken", &[("broken.phs", "x = (1 +\n")]);
    let message = modules.refused("use x from \"broken\"");
    assert!(message.contains("broken") && !message.contains("No such module"), "{message}");
}

#[test]
fn a_cyclic_import_is_refused_with_its_cycle() {
    let modules = Modules::new(
        "cycle",
        &[("cyc_a.phs", "use b from \"cyc_b\"\na = 1\n"), ("cyc_b.phs", "use a from \"cyc_a\"\nb = 2\n"), ("self.phs", "use s from \"self\"\ns = 1\n")],
    );
    let message = modules.refused("use a from \"cyc_a\"\na");
    assert!(message.contains("cyc_a -> cyc_b -> cyc_a"), "{message}");
    let message = modules.refused("use s from \"self\"");
    assert!(message.contains("self -> self"), "{message}");
}

#[test]
fn a_decorated_function_of_a_module_checks_its_contract_in_its_module() {
    let modules = Modules::new("decorated", &[("deco.phs", "k = 2\n@requires(x > 0, \"x must be positive\")\nsafe(x) = k * x\n")]);
    assert_eq!(last(&modules.run("k = 10\nuse safe from \"deco\"\nsafe(3)")), 6.0);
    let message = modules.refused("use safe from \"deco\"\nsafe(-1)");
    assert!(message.contains("x must be positive"), "{message}");
}

#[test]
fn a_module_is_evaluated_once_so_its_measurements_are_the_same_ones() {
    let modules = Modules::new("once", &[("unc.phs", "a = 2 ± 0.1\nb = 2 * a\n")]);
    // `b - 2a` is exactly zero only when both are the module's one measurement `a`
    let values = modules.run("use a as a1 from \"unc\"\nuse a, b from \"unc\"\nb - 2*a1");
    let PhsValue::Quantity(d) = values.last().unwrap() else { panic!("{values:?}") };
    assert_eq!(d.value.mean(), 0.0);
    assert!(d.value.std_dev().abs() < 1e-12, "{}", d.value.std_dev());
}

#[test]
fn a_module_changed_on_disk_is_evaluated_again() {
    let modules = Modules::new("changed", &[LIB]);
    let mut interpreter = modules.interpreter();
    assert_eq!(last(&interpreter.eval_str("use r from \"lib\"\nr").unwrap()), 6.0);
    modules.write("lib.phs", "r = 7\n");
    assert_eq!(last(&interpreter.eval_str("use r from \"lib\"\nr").unwrap()), 7.0);
}

#[test]
fn the_top_level_loops_of_a_module_run() {
    let modules = Modules::new("loop", &[("loopy.phs", "n = 1\nwhile n < 10 {\n  n = n * 2\n}\n")]);
    assert_eq!(last(&modules.run("use n from \"loopy\"\nn")), 16.0);
}

#[test]
fn an_imported_name_can_be_rebound_by_the_importer() {
    let modules = Modules::new("rebind", &[LIB]);
    assert_eq!(last(&modules.run("use r from \"lib\"\nr = 1\nr")), 1.0);
}

#[test]
fn a_module_or_an_import_cannot_rebind_a_constant() {
    let modules = Modules::new("constant", &[LIB, ("pi_mod.phs", "pi = 3\nfine = 1\n")]);
    let message = modules.refused("use fine from \"pi_mod\"");
    assert!(message.contains("`pi`"), "{message}");
    let message = modules.refused("use r from \"lib\"\npi = 3");
    assert!(message.contains("`pi`"), "{message}");
}

#[test]
fn a_function_given_to_a_module_function_sees_the_names_of_the_caller() {
    let modules = Modules::new("higher", &[("lib2.phs", "k = 2\nc = 100\napply(g, x) = g(x) * k\n")]);
    // `apply(sq, 1)` is `sq(1) * k` with the caller's `c` in `sq` and the module's `k`: 5*1*2
    assert_eq!(last(&modules.run("c = 5\nsq(x) = c * x\nuse apply from \"lib2\"\napply(sq, 1)")), 10.0);
}

#[test]
fn a_function_a_module_function_returns_keeps_its_module() {
    let modules = Modules::new("returned", &[LIB, ("lib3.phs", "k = 2\nf(x) = k*x\nmake(y) = f\n")]);
    assert_eq!(last(&modules.run("k = 10\nuse make from \"lib3\"\nh = make(0)\nh(1)")), 2.0);
}

#[test]
fn the_derivative_of_an_imported_function_is_taken_in_its_module() {
    let modules = Modules::new("prime", &[LIB]);
    // d/dx (k*x) = k = 2
    assert_eq!(last(&modules.run("k = 10\nuse f from \"lib\"\nf'(1)")), 2.0);
}

#[test]
fn an_imported_function_using_its_module_is_not_inlined_into_a_symbolic_expression() {
    let modules = Modules::new("inline", &[LIB, ("sq.phs", "sq(x) = x^2\n")]);
    // `k` would be read where the expression is evaluated, the importer's 10: refused, naming it
    let message = modules.refused("use * from calc\nk = 10\nuse f from \"lib\"\nderiv(\"f(t)\", \"t\")");
    assert!(message.contains("'k'") && message.contains("lib"), "{message}");
    // a function that uses only its parameters is inlined as before: d/dt t^2 at t = 3 is 6
    let values = modules.run("use * from calc\nuse sq from \"sq\"\nd = deriv(\"sq(t)\", \"t\")\nd");
    let text = values.last().unwrap().to_string();
    assert!(text.contains('t') && text.contains('2'), "{text}");
}

// --- a module given in memory (the sandbox's resolver) ---

fn expression(text: &str) -> Expr {
    match parse_phs(text).unwrap().statements.remove(0) {
        Statement::Expr(e) => e,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_module_given_in_memory_is_evaluated_in_its_own_scope() {
    let mut export = ModuleExport { symbols: Vec::new(), functions: HashMap::new() };
    export.define("k".into(), expression("2"));
    export.define("r".into(), expression("3*k"));
    let Statement::FunctionDef(f) = parse_phs("f(x) = k*x").unwrap().statements.remove(0) else { unreachable!() };
    export.functions.insert("f".into(), f);
    let mut resolver = MemoryModuleResolver::new();
    resolver.add_module("lib".into(), export);
    let mut interpreter = PhsInterpreter::new(Arc::new(resolver));
    let values = interpreter.eval_str("k = 10\nuse r, f from \"lib\"\nr\nf(1)").unwrap();
    let tail: Vec<f64> = values[values.len() - 2..].iter().map(mean).collect();
    assert_eq!(tail, [6.0, 2.0]);
}
