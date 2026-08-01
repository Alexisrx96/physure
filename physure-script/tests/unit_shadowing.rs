//! When a quantity literal's unit chain names a variable the script bound.
//!
//! `unit_expr` is greedy — it absorbs `* ident` / `/ ident` for as long as the identifiers
//! name registered units — so with `g = 9.81 m/s^2` in scope, `f = 10.0 kg * g` parses as a
//! single literal whose unit is `kg * g`: a mass squared, where the author plainly meant
//! 98.1 N. Nothing in the output said a choice had been made, and because every codegen
//! target consumes the same AST, `transpile --target python` emitted `Q_(10.0, "kg * g")`.
//!
//! The repo's rule is that a wrong answer with confident units is worse than an exception,
//! so the parser now refuses to guess and names the offending token instead. These tests pin
//! both the refusal and — just as important — the cases that must keep parsing.

use physure_script::parser::parse_phs;
use physure_script::{transpile, Target};

fn expect_ambiguity(src: &str, token: &str) -> String {
    let err = parse_phs(src)
        .err()
        .unwrap_or_else(|| panic!("{src:?} parsed cleanly; expected an ambiguity error"));
    let text = format!("{err:?}");
    assert!(
        text.contains(&format!("'{token}'")),
        "{src:?} errored without naming {token:?}: {text}",
    );
    text
}

fn expect_ok(src: &str) {
    if let Err(e) = parse_phs(src) {
        panic!("{src:?} must still parse, but was rejected: {e:?}");
    }
}

/// The two reported spellings of the defect: a bound `g` read as gram, a bound `t` as tonne.
#[test]
fn a_bound_name_in_the_unit_chain_is_rejected() {
    expect_ambiguity("g = 9.81 m / s ^ 2\nf = 10.0 kg * g\n", "g");
    expect_ambiguity("t = 3.0 s\nx = 5 m / t\n", "t");
}

/// The message has to be actionable, so it must carry both escape hatches: parenthesising the
/// number ends the unit chain (the word then reaches the expression parser as a variable),
/// and quoting the word keeps it a unit (a string operand resolves against the registry).
#[test]
fn the_message_offers_both_disambiguations() {
    let text = expect_ambiguity("g = 9.81 m / s ^ 2\nf = 10.0 kg * g\n", "g");
    assert!(text.contains("(10 kg) * g"), "no variable rewrite offered: {text}");
    assert!(text.contains("kg * "), "no unit rewrite offered: {text}");
    assert!(text.contains("multiply by the variable"), "{text}");
    assert!(text.contains("keep the unit"), "{text}");
}

/// Both suggested rewrites must actually be accepted by the parser they are printed from.
#[test]
fn the_suggested_rewrites_parse() {
    expect_ok("g = 9.81 m / s ^ 2\nf = (10.0 kg) * g\n");
    expect_ok("g = 9.81 m / s ^ 2\nf = 10.0 kg * \"g\"\n");
    expect_ok("t = 3.0 s\nx = (5 m) / t\n");
    expect_ok("t = 3.0 s\nx = 5 m / \"t\"\n");
}

/// A binding takes effect only after its own right-hand side has been read, so the classic
/// `g = 9.81 m / s ^ 2` must not flag the `g` it is in the middle of defining. The same holds
/// for any use site that precedes the binding.
#[test]
fn a_name_bound_later_does_not_shadow_an_earlier_use() {
    expect_ok("g = 9.81 m / s ^ 2\n");
    expect_ok("t = 60 s / min\n");
    expect_ok("x = 5 m / t\nt = 3.0 s\n");
}

/// Juxtaposition is never multiplication in PhysureScript: `3 m` is a quantity literal by
/// grammar, with no operator to be ambiguous about. A script that binds `m` must therefore
/// keep parsing every later `3 m`.
#[test]
fn the_first_token_after_the_number_is_never_ambiguous() {
    expect_ok("m = 5.0 kg\nd = 3 m\n");
    expect_ok("s = 2.0\nd = 10 s\n");
    expect_ok("t = 1.0\nmass = 4 t\n");
}

/// Parameters shadow inside their own function body and nowhere else — a `t` parameter must
/// not make `5 m / t` ambiguous in the statements that follow the definition.
#[test]
fn function_parameters_scope_to_their_body() {
    expect_ambiguity("fn speed(t) = 5 m / t\n", "t");
    expect_ok("fn speed(t) = (5 m) / t\nd = 5 m / t\n");
}

/// A function's locals are equally confined to it.
#[test]
fn function_locals_scope_to_their_body() {
    expect_ambiguity("fn f(x) =\n    t = 2.0 s\n    5 m / t\n", "t");
    expect_ok("fn f(x) =\n    t = 2.0 s\n    x\nd = 5 m / t\n");
}

/// `where` desugars to nested `let` bindings whose scope is the body, so a `where`-bound name
/// shadows in the body but not in its own value expression.
#[test]
fn where_bindings_scope_to_the_body() {
    expect_ambiguity("e = 10.0 kg * g where g = 9.81 m / s ^ 2\n", "g");
    expect_ok("e = (10.0 kg) * g where g = 9.81 m / s ^ 2\n");
}

/// The check runs before codegen, not inside the interpreter, so a transpile of an ambiguous
/// script fails instead of emitting `Q_(10.0, "kg * g")` and friends.
#[test]
fn transpiling_an_ambiguous_script_fails_for_every_target() {
    let src = "g = 9.81 m / s ^ 2\nf = 10.0 kg * g\n";
    for target in [Target::Python, Target::Rust, Target::Java] {
        let parsed = parse_phs(src);
        assert!(parsed.is_err(), "{target:?}: the ambiguous script reached codegen");
        // And nothing downstream can revive it: there is no program to hand to `transpile`.
        let clean = parse_phs("f = 10.0 kg\n").expect("the unambiguous form must parse");
        assert!(transpile(&clean, target).is_ok());
    }
}

/// The scripts the rule must never touch: the guide's headline examples, uncertainty
/// literals, and constants whose units legitimately chain several symbols.
#[test]
fn unambiguous_scripts_are_untouched() {
    expect_ok("500 N / 2 m^2 => kPa\n");
    expect_ok("10.0 kg\n");
    expect_ok("9.81 +/- 0.05 m / s ^ 2\n");
    expect_ok("k = 8.99e9 N * m ^ 2 / C ^ 2\n");
    expect_ok("E = 0.5 * m * v ^ 2 where m = 2.0 kg, v = 3.0 m / s\n");
}

/// The rewrites are cut out of the user's own chain, so a parenthesised group must come back
/// balanced — `kg * (g * m)` once suggested the unparseable `(10 kg) * g * m)`.
#[test]
fn a_parenthesised_group_is_suggested_back_whole() {
    let text = expect_ambiguity("g = 9.81 m / s ^ 2\ne = 10 kg * (g * m)\n", "g");
    assert!(text.contains("(10 kg) * (g * m)"), "unbalanced rewrite: {text}");
    expect_ok("g = 9.81 m / s ^ 2\ne = (10 kg) * (g * m)\n");
}

/// An exponent digit is not a name, and a variable named like a unit only matters where it
/// actually appears — a chain whose every word is a plain unit must stay silent.
#[test]
fn exponents_and_unrelated_bindings_are_ignored() {
    expect_ok("two = 2\nk = 8.99e9 N * m ^ 2 / C ^ 2\n");
    expect_ok("N = 5.0\nk = 8.99e9 J * m ^ 2 / C ^ 2\n");
}
