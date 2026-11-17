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
