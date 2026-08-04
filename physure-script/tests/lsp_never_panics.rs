//! The LSP re-analyses the buffer on every keystroke, so it sees every intermediate prefix of
//! whatever the user is typing — and it both parses *and* interprets it. A panic there is not a
//! diagnostic: it kills the server process (exit 101), the client restarts a few times and then
//! gives up, so the user loses diagnostics for the rest of the session. Errors are fine; panics
//! are not.
//!
//! Regression: `deriv("x^2")` with a single argument passed an `args.is_empty()` guard and then
//! indexed `args[1]`, panicking with "index out of bounds: len is 1 but index is 1".

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Realistic documents whose every prefix must be survivable.
const DOCS: &[&str] = &[
    "cap = 4.18 J / (g * K)\neq = \"Q = m * {cap} * (T2 - T1)\"\neq(m = 100.0 g, T1 = 20.0 degC, T2 = 120.0 degF) => J\n5 degC - 25 degC\n",
    "use deriv, integral from calc\nd = deriv(\"x^2\", \"x\")\ni = integral(\"x^2\", \"x\", 0, 1)\n",
    "f(x) = x^2 + 2 * x - 1\ny = f(3)\nz = 2 m + 3 m => cm\n",
    "v = [1, 2, 3]\nw = v * 2\nm = [[1, 2], [3, 4]]\n",
    "if x > 0 then 1 else -1\nr = 5 kg * 9.81 m/s^2 => N\n",
    "use solve from calc\ns = solve(\"R = V / I\", \"R\")\n",
    "x = 3 where y = 2\nq = a > b ? 1 : 2\nt = 5 m => ft => in\n",
    "def g(a, b = 2 m):\n    return a + b\ng(a = 1 m, b = 3 m)\n",
    "u = 2 Ω => kΩ\np = 10 % of 50\nc = 1.5e-3 F\n",
    "n = -4 ** 2\nk = (1 + 2) * (3 - 4) / 5\nmat = [[1, 2], [3, 4]] * 2\n",
    "export f\nuse * from array\nsum([1, 2, 3])\n",
];

/// Fragments that are not prefixes of anything valid — trailing operators, unbalanced
/// delimiters, and half-typed calls, all of which occur mid-keystroke.
const FRAGMENTS: &[&str] = &[
    "2 /", "2 *", "2 +", "2 -", "2 ^", "x =", "x = (", "x = [", "x = \"",
    "f(", "f(a,", "f(a =", "if", "if x", "if x then", "if x then 1 else",
    "=> J", "2 m =>", "use", "use deriv", "use deriv from", "return",
    "def", "def f(", "def f(a):", "x where", "x where y =", "a ? b :",
    "5 degC -", "[[1, 2], [3,", "\"unterminated", "# comment only",
    "()", "[]", "{}", "((((", "]]]]", "1..2", "..", "e", "1e", "1e+",
];

/// Under-applied calls into every domain-gated builtin family. Each of these must report an
/// arity error, never index past the end of `args`.
const UNDER_APPLIED: &[&str] = &[
    "use deriv from calc\nderiv(\"x^2\")\n",
    "use diff from calc\ndiff(\"x^2\")\n",
    "use grad from calc\ngrad(\"x^2\")\n",
    "use div from calc\ndiv([\"x\"])\n",
    "use curl from calc\ncurl([\"x\"])\n",
    "use laplacian from calc\nlaplacian(\"x^2\")\n",
    "use integral from calc\nintegral(\"x^2\")\n",
    "use integral from calc\nintegral(\"x^2\", \"x\")\n",
    "use simplify from calc\nsimplify()\n",
    "use solve from calc\nsolve(\"x = 1\")\n",
];

fn cases() -> Vec<String> {
    let mut cases: Vec<String> = FRAGMENTS
        .iter()
        .chain(UNDER_APPLIED.iter())
        .map(|s| s.to_string())
        .collect();
    for doc in DOCS {
        let chars: Vec<char> = doc.chars().collect();
        for len in 1..=chars.len() {
            cases.push(chars[..len].iter().collect());
        }
    }
    cases
}

/// Runs `f` over every case, returning the ones that panicked.
fn offenders(f: impl Fn(&str)) -> Vec<(String, String)> {
    // Keep the output readable: each panicking case would otherwise print its own backtrace.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut bad = Vec::new();
    for case in cases() {
        if let Err(e) = catch_unwind(AssertUnwindSafe(|| f(&case))) {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string());
            bad.push((case, msg));
        }
    }

    std::panic::set_hook(prev);
    bad
}

fn report(bad: &[(String, String)], phase: &str) {
    assert!(
        bad.is_empty(),
        "{} case(s) panicked during {} instead of returning an error; first few:\n{}",
        bad.len(),
        phase,
        bad.iter()
            .take(8)
            .map(|(c, m)| format!("  {:?}\n    -> {}", c, m))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn parsing_a_partial_document_never_panics() {
    let bad = offenders(|src| {
        let _ = physure_script::parser::parse_phs_with_lines(src);
    });
    report(&bad, "parsing");
}

#[test]
fn interpreting_a_partial_document_never_panics() {
    let bad = offenders(|src| {
        // Mirrors what the LSP's `analyze` does: parse, then run each statement, ignoring errors.
        if let Ok(statements) = physure_script::parser::parse_phs_with_lines(src) {
            let mut interp = physure_script::interpreter::PhsInterpreter::default();
            for (_, stmt) in statements {
                let _ = interp.run_statement(&stmt);
            }
        }
    });
    report(&bad, "interpretation");
}

#[test]
fn deriv_with_a_single_argument_is_an_error_not_a_panic() {
    let statements = physure_script::parser::parse_phs_with_lines(
        "use deriv from calc\nderiv(\"x^2\")\n",
    )
    .expect("this parses fine; the old failure was at evaluation time");

    let mut interp = physure_script::interpreter::PhsInterpreter::default();
    let mut last = Ok(());
    for (_, stmt) in statements {
        last = interp.run_statement(&stmt).map(|_| ());
    }
    let err = last.expect_err("deriv with one argument must be rejected").to_string();
    assert!(
        err.contains("deriv expects"),
        "expected an arity error mentioning the signature, got: {err}"
    );
}
