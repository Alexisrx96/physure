//! One-sided limits, `limit(expr, var, point, side)` with `side` `"+"` (from the right) or `"-"` (from the left),
//! positional or as the keyword `side=`; the strict two-sided limit (both sides must be defined); and the keyword
//! arguments of the calc builtins, which are refused when unknown instead of being dropped.
//!
//! Every program runs through `report::evaluate`, the entry `phs eval` uses. Every expected value is a closed form
//! written here, never a copy of the engine's output.

use physure_script::report::{evaluate, Report, DEFAULT_SEED};
use physure_script::Limits;
use std::time::Duration;

fn run(program: &str) -> Report {
    evaluate(&format!("use * from calc\n{program}"), Limits::sandboxed(Duration::from_secs(60)), DEFAULT_SEED)
}

/// The message with which `program` is refused.
fn refused(program: &str) -> String {
    let report = run(program);
    match report.diagnostics.iter().find(|d| d.severity == "error") {
        Some(d) => d.message.clone(),
        None => panic!("{program:?} should be refused, it gave {:?}", report.results.last().map(|r| &r.value.text)),
    }
}

/// The last value of `program`: (text, value, uncertainty, unit).
fn answered(program: &str) -> (String, Option<f64>, Option<f64>, Option<String>) {
    let report = run(program);
    assert!(report.ok, "{program:?} failed: {:?}", report.diagnostics);
    let item = report.results.last().unwrap_or_else(|| panic!("{program:?} gave no value"));
    (item.value.text.clone(), item.value.value, item.value.uncertainty, item.value.unit.clone())
}

// ---- keyword arguments of the calc builtins ----

#[test]
fn an_unknown_keyword_argument_of_a_calc_builtin_is_refused() {
    // before: integral(..., foo=2) was 0.5 and series(..., foo=2) the series: the keyword was dropped
    for program in [
        "integral(\"x\", \"x\", 0, 1, foo=2)",
        "integral(\"x\", \"x\", foo: 2)",
        "series(\"sin(x)\", \"x\", foo=2)",
        "limit(\"sin(x)/x\", \"x\", 0, foo=2)",
        "deriv(\"x^2\", \"x\", order=2)",
        "simplify(\"x + x\", foo=1)",
    ] {
        let message = refused(program);
        assert!(message.contains("keyword") && message.contains('`'), "{program}: {message}");
    }
    assert!(refused("integral(\"x\", \"x\", 0, 1, foo=2)").contains("`foo`"));
    assert!(refused("integral(\"x\", \"x\", 0, 1, foo=2, foo=3)").contains("`foo`"));
    // without keywords the builtins answer as before
    assert_eq!(answered("integral(\"x\", \"x\", 0, 1)").1, Some(0.5));
}
