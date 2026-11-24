//! Characterization of the `limit` builtin (sonar S3776 cleanup): what each kind of limit answers or refuses, as it
//! was before `limit` and its sampling of one side were split into helpers. Unlike the other limit tests, the
//! expected values here ARE the engine's output (exact f64 and exact messages): they pin the behaviour of a refactor,
//! not the mathematics, which `limit_semantics.rs` and the other limit suites check against closed forms.

use physure_script::interpreter::eval_phs;
use physure_script::value::PhsValue;

/// What `program` (run after `use * from calc`) gives: its last value exactly, or its error.
fn outcome(program: &str) -> String {
    match eval_phs(&format!("use * from calc\n{program}")) {
        Ok(values) => match values.last().expect("a value") {
            PhsValue::Number(n) => format!("number {n:?}"),
            PhsValue::Quantity(q) => {
                format!("quantity {:?} +/- {:?} {}", q.value.mean(), q.value.std_dev(), q.unit.__repr__())
            }
            other => format!("value {other}"),
        },
        Err(error) => format!("error {error}"),
    }
}

/// Each program and its outcome, as the engine gave them before the refactor.
const EXPECTED: &[(&str, &str)] = &[
    ("limit(\"sin(x)/x\", \"x\", 0)", "number 1.0000000000000004"),
    ("limit(\"(1 + 1/x)^x\", \"x\", inf)", "number 2.718281828459045"),
    ("limit(\"1/x\", \"x\", 0, \"+\")", "number inf"),
    ("limit(\"1/x\", \"x\", 0, \"-\")", "number -inf"),
    ("limit(\"1/x\", \"x\", 0)", "error limit: the limit at x = 0.0 does not exist: from the left the expression tends to -inf, from the right to inf"),
    ("limit(\"1/x\", \"x\", inf)", "number 0.0"),
    ("limit(\"exp(-x)\", \"x\", inf)", "number -1.603810890548638e-28"),
    ("limit(\"x^2\", \"x\", inf)", "number inf"),
    ("limit(\"x\", \"x\", -inf)", "number -inf"),
    ("limit(\"sqrt(-x)\", \"x\", inf)", "error limit: the expression is undefined towards +inf"),
    ("limit(\"sqrt(x - 100)\", \"x\", inf)", "error limit: the expression is undefined farther from the point than x = 128.0, where it is defined"),
    ("limit(\"sqrt(100 - x)\", \"x\", inf)", "error limit: the expression is undefined at x = 128.0 although it is defined farther from the point"),
    ("limit(\"sin(x)\", \"x\", inf)", "error limit: the extrapolation towards +inf did not settle: best estimate -0.49124577601471175, error estimate 9.46605219138435e-2 (the last step was reached); the expression may oscillate, jump, or change on a scale smaller than the smallest step; rewrite it so that the limit is a value of a continuous expression, or give the limit in closed form"),
    ("limit(\"x\", \"x\", inf, \"+\")", "error limit: the point +inf is approached from one side only; leave out the side (\"+\" or \"-\" is for a finite point)"),
    ("limit(\"x\", \"x\", -inf, \"-\")", "error limit: the point -inf is approached from one side only; leave out the side (\"+\" or \"-\" is for a finite point)"),
    ("limit(\"x^2\", \"x\", 3)", "number 9.0"),
    ("limit(\"x^2\", \"x\", 3 m)", "quantity 9.0 +/- 0.0 m^2"),
    ("limit(\"x^2\", \"x\", 3 +/- 0.1)", "quantity 9.0 +/- 0.6000000000000001 "),
    ("limit(\"sin(x)/x\", \"x\", 0 +/- 0.1)", "error limit: the point 0.00 ± 0.10 is uncertain, and the limit there is extrapolated (the expression is not evaluated at the point itself): an extrapolated limit has no derivative with respect to its point to propagate the uncertainty with"),
    ("limit(\"sin(x)/x\", \"x\", 0 m)", "error limit: the expression cannot be evaluated at x = -0.125 m: sin expects an angle or a dimensionless value, got 'm'"),
    ("limit(\"sqrt(x)\", \"x\", 0)", "error limit: the expression is undefined for x < 0.0 next to the point (at every sample from x = -0.125 to x = -1.1920928955078125e-7), so the two-sided limit at x = 0.0 does not exist; use side \"+\" for the limit from the right"),
    ("limit(\"sqrt(x)\", \"x\", 0, \"+\")", "number 0.0"),
    ("limit(\"sqrt(-x)\", \"x\", 0, \"-\")", "number 0.0"),
    ("limit(\"sqrt(x)\", \"x\", -1)", "error limit: the expression is undefined on both sides of x = -1.0"),
    ("limit(\"x\", \"x\", 0, \"up\")", "error limit: the side must be \"+\" (from the right) or \"-\" (from the left), not \"up\"; leave it out for the two-sided limit"),
    ("limit(\"x\", \"x\", 0, 1)", "error limit: the side must be \"+\" (from the right) or \"-\" (from the left), not 1.0 (a Quantity); leave it out for the two-sided limit"),
    ("limit(\"sin(1/x)\", \"x\", 0)", "error limit: the extrapolation from the left of x = 0.0 did not settle: best estimate 0.49124577601471175, error estimate 9.46605219138435e-2 (the last step was reached); the expression may oscillate, jump, or change on a scale smaller than the smallest step; rewrite it so that the limit is a value of a continuous expression, or give the limit in closed form"),
    ("limit(\"(x^2 - 1)/(x - 1)\", \"x\", 1)", "number 2.0"),
    ("limit(\"(x^2 - 1)/(x - 1)\", \"x\", 1, \"-\")", "number 2.0"),
    ("limit(\"ln(x)\", \"x\", 0, \"+\")", "number -inf"),
    ("limit(\"sqrt(x^2 * (x^2 - 1e-4))\", \"x\", 0)", "error limit: the expression is undefined at x = 0.0078125 although it is defined farther from the point"),
    ("limit(\"1/(x - 1e-9)\", \"x\", 0, \"+\")", "number -999999999.9999999"),
    ("limit(\"1/x^2\", \"x\", 0)", "number inf"),
    ("limit(\"x/abs(x)\", \"x\", 0)", "error limit: the limit at x = 0.0 does not exist: from the left the expression tends to -1.0, from the right to 1.0"),
    ("a = 3\nlimit(\"a*x\", \"x\", 2)", "number 6.0"),
    ("a = 3 +/- 0.5\nlimit(\"a*sin(x)/x\", \"x\", 0)", "quantity 3.0000000000000018 +/- 0.5000000000000002 "),
    ("limit(\"x\", \"y\", 0)", "error limit: the expression uses 'x', which is neither a variable of the program, pi, a constant of the catalog nor a unit"),
    ("limit(\"1/x\", \"x\", 1e-300)", "number 9.999999999999999e299"),
    ("limit(\"x\", \"x\", 1e300)", "number 1e300"),
    ("limit(\"(exp(x) - 1)/x\", \"x\", 0 s)", "error limit: the expression cannot be evaluated at x = -0.125 s: exp expects a dimensionless value, got 's'"),
    ("limit(\"(exp(x/s) - 1)/x\", \"x\", 0 s)", "quantity 1.000000000000004 +/- 0.0 Hz"),
    ("limit(\"x*sin(1/x)\", \"x\", 0)", "error limit: the extrapolation from the left of x = 0.0 did not settle: best estimate -0.00015090198510364012, error estimate 3.915768845574668e-6 (the last step was reached); the expression may oscillate, jump, or change on a scale smaller than the smallest step; rewrite it so that the limit is a value of a continuous expression, or give the limit in closed form"),
    ("limit(\"1/x\", \"x\", 1e-200, \"+\")", "number 1e200"),
    ("limit(\"x^x\", \"x\", 0, \"+\")", "error limit: the extrapolation from the right of x = 0.0 did not settle: best estimate 0.9999576494812102, error estimate 4.243188799346598e-5 (the last step was reached); the expression may oscillate, jump, or change on a scale smaller than the smallest step; rewrite it so that the limit is a value of a continuous expression, or give the limit in closed form"),
    ("limit(\"x\", \"x\", \"zero\")", "error limit: the point uses 'zero', which is neither a variable of the program, pi, a constant of the catalog nor a unit"),
    ("limit(\"1/0\", \"x\", 1)", "error limit: the expression is undefined on both sides of x = 1.0"),
    ("limit(\"(1 + x)^(1/x)\", \"x\", 0)", "number 2.7182818284590455"),
    ("limit(\"tan(x)\", \"x\", 1.5707963267948966)", "number 1.633123935319537e16"),
    ("limit(\"exp(x)\", \"x\", 800, \"+\")", "error limit: the extrapolation from the right of x = 800.0 did not settle: best estimate NaN, error estimate inf (a sample overflowed); the expression may oscillate, jump, or change on a scale smaller than the smallest step; rewrite it so that the limit is a value of a continuous expression, or give the limit in closed form"),
    // KNOWN WRONG (reported with the S3776 cleanup, not fixed by it): the limit of this step function at +inf is
    // 0 (1 with the `+ 1`); the table pins what the engine answers today. Update it when the engine is fixed.
    ("limit(\"floor(100/x)\", \"x\", inf)", "number -1.0"),
    // KNOWN WRONG (reported with the S3776 cleanup, not fixed by it): the limit of this step function at +inf is
    // 0 (1 with the `+ 1`); the table pins what the engine answers today. Update it when the engine is fixed.
    ("limit(\"floor(100/x) + 1\", \"x\", inf)", "number 0.0"),
    ("limit(\"x/abs(x)\", \"x\", 1e-320, \"+\")", "number 1.0"),
    ("limit(\"sin(x - 1e-320)/(x - 1e-320)\", \"x\", 1e-320, \"+\")", "number 1.0"),
    ("limit(\"floor(x)\", \"x\", 1e-320, \"-\")", "number -1.0"),
    ("limit(\"sign(x)\", \"x\", 0)", "error limit: the expression cannot be evaluated at x = -0.125: Undefined function 'sign'"),
];

#[test]
fn the_outcomes_of_limits_are_pinned() {
    for (program, expected) in EXPECTED {
        assert_eq!(outcome(program), *expected, "{program:?}");
    }
}
