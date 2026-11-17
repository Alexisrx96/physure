//! Reserved names (spec `docs/superpowers/specs/2026-11-16-reserved-names-design.md`): the statement words, the
//! literals and the functions of the core can never be bound, in any form that binds, and the names that are not
//! reserved still bind.

use physure_script::interpreter::eval_phs;
use physure_script::reserved::{CORE_FUNCTIONS, LITERALS, STATEMENT_WORDS};
use physure_script::{parse_phs, PhsInterpreter, PhsValue};

/// The message with which `program` is refused.
fn refused(program: &str) -> String {
    match eval_phs(program) {
        Ok(values) => panic!("{program:?} should be refused, it gave {:?}", values.last().map(|v| v.to_string())),
        Err(e) => e.to_string(),
    }
}

/// The text of the value of the last statement of `program`.
fn value(program: &str) -> String {
    let values = eval_phs(program).unwrap_or_else(|e| panic!("{program:?} failed: {e}"));
    values.last().expect("a value").to_string()
}

/// Every way a program binds a name, with `name`.
fn bindings_of(name: &str) -> Vec<String> {
    vec![
        format!("{name} = 3"),
        format!("f({name}) = {name} * 2"),
        format!("fn g({name}) = {name}"),
        format!("for {name} in 0..3 {{ 1 }}"),
        format!("x = 2 * y where y = 1, {name} = 3"),
        format!("{name}(x) = x"),
        // inside a body that never runs
        format!("fn h(x) = x * a where {name} = 1, a = 2"),
        format!("fn k(x) = for {name} in 0..3 {{ x }}"),
        format!("use solve as {name} from calc"),
    ]
}

#[test]
fn a_statement_word_a_literal_or_a_core_function_cannot_be_bound_in_any_form() {
    for name in STATEMENT_WORDS.iter().chain(&LITERALS).chain(&CORE_FUNCTIONS) {
        for program in bindings_of(name) {
            let message = refused(&program);
            assert!(
                message.contains(&format!("`{name}` is")) && message.contains("cannot be bound as") && message.contains("choose another name"),
                "{program:?}: {message}"
            );
        }
    }
}

#[test]
fn the_message_names_the_class() {
    assert!(refused("max = 3").contains("`max` is a function of PHS and cannot be bound as a variable"));
    assert!(refused("use = 3").contains("`use` is a word of the language and cannot be bound as a variable"));
    assert!(refused("inf = 3").contains("`inf` is a literal of the language and cannot be bound as a variable"));
    assert!(refused("fn f(round) = round").contains("cannot be bound as a parameter of `f`"));
}

#[test]
fn a_name_that_is_not_reserved_still_binds() {
    // unit names are the variables of physics; `e`, the words of a format and a function of a module are free
    // until the module is imported
    for name in ["m", "s", "kg", "e", "base", "frac", "x", "limit", "NaN"] {
        assert_eq!(value(&format!("{name} = 3\n{name}")), "3.0", "{name}");
    }
}

#[test]
fn a_value_a_host_gives_cannot_be_a_reserved_name() {
    let program = parse_phs("1").expect("a program");
    let error = PhsInterpreter::default()
        .run_program_with(&program, vec![("max".to_string(), PhsValue::Number(1.0))])
        .unwrap_err()
        .to_string();
    assert!(error.contains("`max` is a function of PHS and cannot be bound as a value given by the host"), "{error}");
}

#[test]
fn an_equation_call_may_name_a_variable_after_a_core_function() {
    // the variables of an equation call are local to it, and a call reaches the builtin first
    assert_eq!(value("eq = \"F = sigma * A\"\neq(sigma=5, A=2)"), "10.0");
    assert_eq!(value("eq = \"y = max * x\"\neq(max=3, x=2)"), "6.0");
    // pi and the constants of the catalog are still refused
    assert!(refused("eq = \"y = 2 * x\"\neq(pi=3)").contains("`pi` is the constant"));
}

#[test]
fn a_function_of_a_module_is_free_until_the_module_is_imported() {
    assert_eq!(value("limit = 5\nlimit"), "5.0");
    assert_eq!(value("fn series(a) = a + 1\nseries(1)"), "2.0");
}

#[test]
fn a_function_of_an_imported_module_cannot_be_bound() {
    for program in [
        "use * from calc\nlimit = 5",
        "use * from calc\nfn series(a) = a",
        "use * from calc\nfn f(solve) = solve",
        "use * from calc\nfor deriv in 0..3 { 1 }",
        "use * from array\nnorm = 1",
        "use * from plot\nplot = 1",
    ] {
        let message = refused(program);
        assert!(message.contains("is a function of `") && message.contains("which this program imports"), "{program:?}: {message}");
    }
}

#[test]
fn only_what_is_imported_is_reserved() {
    // `use solve from calc` brings `solve` and nothing else of calc
    assert_eq!(value("use solve from calc\nlimit = 5\nlimit"), "5.0");
    assert!(refused("use solve from calc\nsolve = 1").contains("`solve` is a function of `calc`"));
    // an alias reserves the alias and leaves the name free
    assert_eq!(value("use solve as sv from calc\nsolve = 2\nsolve"), "2.0");
    assert!(refused("use solve as sv from calc\nsv = 1").contains("`sv` is a function of `calc`"));
}

#[test]
fn importing_a_module_twice_is_not_a_binding() {
    assert_eq!(value("use * from calc\nuse * from calc\n1"), "1.0");
    assert_eq!(value("use solve from calc\nuse solve from calc\n1"), "1.0");
}

#[test]
fn an_import_nested_in_a_body_is_not_a_binding_either() {
    // a body can hold an import (the grammar's `stmt`); running it again binds nothing new
    assert_eq!(value("use solve from calc
while False {
  use solve from calc
}
1"), "1.0");
    assert_eq!(value("use solve from calc
fn f(x) =
  use solve from calc
  x
f(2)"), "2.0");
}

#[test]
fn a_nested_binding_that_is_not_an_import_is_still_refused() {
    let message = refused("use * from calc
fn g(x) =
  limit = 1
  x");
    assert!(message.contains("is a function of `calc`"), "{message}");
}

#[test]
fn an_import_cannot_replace_a_name_the_program_already_holds() {
    for program in [
        "limit = 5
use * from calc",
        "fn series(a) = a
use * from calc",
        "solve = 1
use solve from calc",
        "sv = 1
use solve as sv from calc",
    ] {
        let message = refused(program);
        assert!(message.contains("is a function of `calc`") && message.contains("already bound in the program"), "{program:?}: {message}");
    }
    // a name the module does not bring stays free, before and after
    assert_eq!(value("x = 1
use solve from calc
x"), "1.0");
}

#[test]
fn an_import_cannot_remap_a_module_function() {
    for program in [
        "use * from calc
use solve as limit from calc",
        "use deriv from calc
use solve as deriv from calc",
        "use solve as limit from calc
use * from calc",
    ] {
        let message = refused(program);
        assert!(message.contains("is already a function of `calc`"), "{program:?}: {message}");
    }
    // the same thing twice is not a remap
    assert_eq!(value("use solve as sv from calc
use solve as sv from calc
1"), "1.0");
    // two wildcards that share names keep working: the later one wins, as before
    assert_eq!(value("use * from calc
use * from array
1"), "1.0");
}

#[test]
fn a_named_import_that_fails_unlocks_nothing() {
    let interpreter = PhsInterpreter::default();
    let first = parse_phs("limit = 1
use solve, limit from calc").expect("a program");
    assert!(interpreter.run_program_with(&first, vec![]).is_err());
    let second = parse_phs("solve = 4
solve").expect("a program");
    assert!(interpreter.run_program_with(&second, vec![]).is_ok());
}

#[test]
fn a_value_a_host_gives_cannot_be_a_function_of_an_imported_module() {
    let interpreter = PhsInterpreter::default();
    interpreter.run_program_with(&parse_phs("use * from calc
1").expect("a program"), vec![]).expect("the import runs");
    let error = interpreter
        .run_program_with(&parse_phs("1").expect("a program"), vec![("limit".to_string(), PhsValue::Number(1.0))])
        .unwrap_err()
        .to_string();
    assert!(error.contains("`limit` is a function of `calc`") && error.contains("a value given by the host"), "{error}");
}
