//! How deep an expression tree may be. A flat chain like `1 + 1 + ... + 1` is no nesting of brackets, but it parses
//! to a left-nested tree as deep as the chain is long, and the checker, the evaluator, `Clone` and `Drop` all recurse
//! over it: 200,000 terms (400 KB of text) ended `phs conf verify --run` in a stack overflow no caller can catch. The
//! parser refuses a tree deeper than `MAX_EXPRESSION_DEPTH`, and text that would make the parser itself recurse
//! without brackets (an `else if` chain, function bodies nested by indentation) counts against `MAX_NESTING_DEPTH`.
//!
//! Every case runs on a thread with a 1 MiB stack (a Windows main thread): an overflow aborts the test binary, loudly.

use physure_core::units::conf::with_conf_registry;
use physure_script::parser::{MAX_EXPRESSION_DEPTH, MAX_NESTING_DEPTH};
use physure_script::report::{evaluate, DEFAULT_SEED};
use physure_script::{check_program, parse_phs, Limits};
use std::time::Duration;

const ONE_MIB: usize = 1 << 20;

fn on_one_mib<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    std::thread::Builder::new().stack_size(ONE_MIB).spawn(f).unwrap().join().expect("the thread panicked")
}

/// Parses `source`, checks it, clones and drops it, all on a 1 MiB stack. `Err` is the parse error.
fn parse_check_drop(source: String) -> Result<usize, String> {
    on_one_mib(move || {
        let program = parse_phs(&source).map_err(|e| e.to_string())?;
        let copy = program.clone();
        drop(copy);
        let findings = with_conf_registry(|registry| check_program(&program, registry));
        drop(program);
        Ok(findings.len())
    })
}

fn too_deep(error: &str) -> bool {
    error.contains("expression too deeply nested") && error.contains(&format!("at most {MAX_EXPRESSION_DEPTH}"))
}

/// A chain with `ops` operators: a tree `ops + 1` levels deep.
fn chain(first: &str, op: &str, ops: usize) -> String {
    format!("x = {first}{}", op.repeat(ops))
}

/// `ops` operators make a tree `ops + 1` deep: the most the limit allows is `MAX_EXPRESSION_DEPTH - 1` of them.
const AT_LIMIT: usize = MAX_EXPRESSION_DEPTH - 1;

#[test]
fn a_long_sum_at_the_limit_parses_and_one_more_term_is_refused() {
    parse_check_drop(chain("1", " + 1", AT_LIMIT)).unwrap();
    let error = parse_check_drop(chain("1", " + 1", AT_LIMIT + 1)).unwrap_err();
    assert!(too_deep(&error), "{error}");
}

#[test]
fn the_reviewers_chain_of_200000_terms_is_refused_not_a_stack_overflow() {
    let error = parse_check_drop(chain("1", "+1", 200_000)).unwrap_err();
    assert!(too_deep(&error), "{}", &error[..error.len().min(300)]);
}

#[test]
fn products_quotients_and_implicit_products_are_chains_too() {
    for op in [" * 1", " / 1", " 1"] {
        parse_check_drop(chain("2", op, AT_LIMIT)).unwrap();
        let error = parse_check_drop(chain("2", op, AT_LIMIT + 1)).unwrap_err();
        assert!(too_deep(&error), "{op}: {error}");
        let error = parse_check_drop(chain("2", op, 100_000)).unwrap_err();
        assert!(too_deep(&error), "{op}: {}", &error[..error.len().min(300)]);
    }
}

#[test]
fn comparisons_and_conversions_are_chains_too() {
    for op in [" < 2", " => m"] {
        let first = if op.contains("=>") { "1 m" } else { "1" };
        parse_check_drop(chain(first, op, AT_LIMIT)).unwrap();
        let error = parse_check_drop(chain(first, op, AT_LIMIT + 1)).unwrap_err();
        assert!(too_deep(&error), "{op}: {error}");
        let error = parse_check_drop(chain(first, op, 100_000)).unwrap_err();
        assert!(too_deep(&error), "{op}: {}", &error[..error.len().min(300)]);
    }
}

#[test]
fn a_long_run_of_not_and_or_is_refused_at_the_limit() {
    // `not` is the only unary prefix that repeats: `- - 1` and `!x` are not PHS
    let nots = |count: usize| format!("x = {}True", "not ".repeat(count));
    parse_check_drop(nots(AT_LIMIT)).unwrap();
    assert!(too_deep(&parse_check_drop(nots(AT_LIMIT + 1)).unwrap_err()));
    assert!(too_deep(&parse_check_drop(nots(100_000)).unwrap_err()));
    for op in [" and True", " or True"] {
        parse_check_drop(chain("True", op, AT_LIMIT)).unwrap();
        assert!(too_deep(&parse_check_drop(chain("True", op, AT_LIMIT + 1)).unwrap_err()), "{op}");
        assert!(too_deep(&parse_check_drop(chain("True", op, 100_000)).unwrap_err()), "{op}");
    }
}

#[test]
fn a_chain_of_method_calls_is_refused_at_the_limit() {
    parse_check_drop(chain("2", ".abs()", AT_LIMIT)).unwrap();
    assert!(too_deep(&parse_check_drop(chain("2", ".abs()", AT_LIMIT + 1)).unwrap_err()));
    assert!(too_deep(&parse_check_drop(chain("2", ".abs()", 100_000)).unwrap_err()));
}

#[test]
fn powers_and_unary_minus_nest_only_through_brackets() {
    // `2^2^2` is not PHS: a power chain is refused by the grammar, however long
    assert!(parse_check_drop(chain("2", "^2", 100_000)).is_err());
    assert!(parse_check_drop(format!("x = {}1", "- ".repeat(100_000))).is_err());
    // through brackets they nest as deep as the bracket limit allows
    let powers = format!("x = {}2{}", "2^(".repeat(MAX_NESTING_DEPTH), ")".repeat(MAX_NESTING_DEPTH));
    let minus = format!("x = {}1{}", "-(".repeat(MAX_NESTING_DEPTH), ")".repeat(MAX_NESTING_DEPTH));
    // the parser spends more stack per bracket than 1 MiB holds in a debug build (see `tests/nesting.rs`)
    for source in [powers, minus] {
        std::thread::Builder::new()
            .stack_size(64 << 20)
            .spawn(move || parse_phs(&source).map(drop).unwrap())
            .unwrap()
            .join()
            .unwrap();
    }
}

#[test]
fn calls_nested_to_the_bracket_limit_parse_and_chains_inside_brackets_add_up() {
    let calls = format!("x = {}1{}", "abs(".repeat(MAX_NESTING_DEPTH), ")".repeat(MAX_NESTING_DEPTH));
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || parse_phs(&calls).map(drop).unwrap())
        .unwrap()
        .join()
        .unwrap();
    // 40 brackets, each the first term of a chain of 9 more: no bracket limit is passed, but the tree is 360 deep
    let nested = format!("x = {}1{}", "(".repeat(40), " + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1)".repeat(40));
    let error = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || parse_phs(&nested).unwrap_err().to_string())
        .unwrap()
        .join()
        .unwrap();
    assert!(too_deep(&error), "{error}");
}

#[test]
fn where_bindings_are_a_chain_too() {
    let bindings = |count: usize| format!("x = a0 where {}", (0..count).map(|i| format!("a{i} = 1")).collect::<Vec<_>>().join(", "));
    parse_check_drop(bindings(AT_LIMIT)).unwrap();
    assert!(too_deep(&parse_check_drop(bindings(AT_LIMIT + 1)).unwrap_err()));
    assert!(too_deep(&parse_check_drop(bindings(50_000)).unwrap_err()));
}

#[test]
fn an_else_if_chain_counts_against_the_nesting_limit() {
    let chain = |count: usize| format!("x = {}3", "if True then 1 else ".repeat(count));
    parse_check_drop(chain(MAX_NESTING_DEPTH)).unwrap();
    let error = parse_check_drop(chain(MAX_NESTING_DEPTH + 1)).unwrap_err();
    assert!(error.contains("nested more than"), "{error}");
    // written one branch to a line
    let lines = format!("x = {}3", "if True\nthen 1\nelse ".repeat(100_000));
    assert!(parse_check_drop(lines).unwrap_err().contains("nested more than"));
    // separate statements are not nested
    parse_check_drop("y = if True then 1 else 2\n".repeat(500)).unwrap();
}

#[test]
fn function_bodies_nested_by_indentation_count_against_the_nesting_limit() {
    let nested = |count: usize| {
        let mut source = String::new();
        for level in 0..count {
            source.push_str(if level == 0 { "" } else { " " });
            source.push_str(&format!("fn f{level}(x) =\n"));
        }
        source + " x\n"
    };
    parse_check_drop(nested(MAX_NESTING_DEPTH / 2)).unwrap();
    let error = parse_check_drop(nested(100_000)).unwrap_err();
    assert!(error.contains("nested more than"), "{error}");
    // functions side by side, each with a block body, are not nested
    parse_check_drop("fn g(x) =\n  y = x\n  y\n".repeat(500)).unwrap();
}

#[test]
fn the_sandboxed_evaluator_runs_a_chain_at_the_limit_and_reports_one_past_it_as_a_syntax_error() {
    let at_limit = evaluate(&chain("1", " + 1", AT_LIMIT), Limits::sandboxed(Duration::from_secs(60)), DEFAULT_SEED);
    assert!(at_limit.ok, "{at_limit:?}");
    assert_eq!(at_limit.results[0].value.value, Some(MAX_EXPRESSION_DEPTH as f64));
    let past = evaluate(&chain("1", "+1", 200_000), Limits::sandboxed(Duration::from_secs(60)), DEFAULT_SEED);
    assert!(!past.ok);
    assert_eq!(past.diagnostics[0].code, "syntax");
    assert!(too_deep(&past.diagnostics[0].message));
}

#[test]
fn ordinary_formulas_are_far_from_the_limit() {
    parse_check_drop("fn ke(mass, v) = 1/2 * mass * v^2\nx = ke(2 kg, 3 m/s) => J\ny = (1 + 2) * (3 + 4) / 5 - 6".to_string()).unwrap();
    // a polynomial of a hundred terms
    let poly = (0..100).map(|i| format!("{i} * t^2")).collect::<Vec<_>>().join(" + ");
    parse_check_drop(format!("t = 2\nx = {poly}")).unwrap();
}

#[test]
fn a_line_at_column_0_that_continues_a_statement_closes_no_function_body() {
    // each of these keeps the parser inside the body above it, so none of them may reset the count
    let continuations = [" y where\na =\n1\n", " if True\nthen 1\nelse 2\n", " @stable\n", " /// doc\n"];
    for continuation in continuations {
        let mut source = String::new();
        for level in 0..200 {
            source.push_str(if level == 0 { "" } else { " " });
            source.push_str(&format!("fn f{level}(x) =\n{continuation}"));
        }
        source.push_str(" x\n");
        let error = parse_check_drop(source).unwrap_err();
        assert!(error.contains("nested more than"), "{continuation:?}: {error}");
    }
}

#[test]
fn a_quote_inside_a_raw_block_hides_no_bracket() {
    let source = format!("```\"```\nx = {}1{}\ny = \"\"", "(".repeat(100_000), ")".repeat(100_000));
    let error = parse_check_drop(source).unwrap_err();
    assert!(error.contains("brackets are nested more than"), "{error}");
}
