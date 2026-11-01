//! The semantics of PHS operators and builtins that no other test pins: equations and vectors as operands, functions
//! that combine, ranges, conversions, booleans, formats, assertions, and what a differential equation solver
//! answers and refuses.

use physure_script::interpreter::eval_phs;

/// The value of the last statement of `program`, as text.
fn run(program: &str) -> String {
    let values = eval_phs(program).unwrap_or_else(|e| panic!("{program:?} failed: {e}"));
    values.last().expect("a value").to_string()
}

/// The message with which `program` is refused.
fn refused(program: &str) -> String {
    match eval_phs(program) {
        Ok(values) => panic!("{program:?} should be refused, it gave {:?}", values.last().map(|v| v.to_string())),
        Err(e) => e.to_string(),
    }
}

fn check(table: &[(&str, &str)]) {
    for (program, expected) in table {
        assert_eq!(&run(program), expected, "{program}");
    }
}

fn calc(call: &str) -> String {
    run(&format!("use * from calc\n{call}"))
}

#[test]
fn a_differential_equation_of_the_second_order_is_solved_by_its_roots() {
    for (equation, solution) in [
        ("y'' + 2*y' + y = 0", "y = (C2 * x + C1) * exp(-1 * x)"),
        ("y'' + 2*y' + 5*y = 0", "y = (cos(2 * x) * C1 + sin(2 * x) * C2) * exp(-1 * x)"),
        ("y'' - 4*y = 0", "y = exp(-2 * x) * C2 + exp(2 * x) * C1"),
        ("y'' = 0", "y = C2 * x + C1"),
        ("y' + y = x", "y = (exp(x) * x - exp(x) + C1)/exp(x)"),
        ("y' = sin(x)", "y = -1 * cos(x) + C1"),
    ] {
        assert_eq!(calc(&format!(r#"dsolve("{equation}", "y", "x")"#)), solution, "{equation}");
    }
}

#[test]
fn a_constant_forcing_term_adds_its_particular_solution() {
    // y'' + y = 2 has the particular solution y = 2
    let solution = calc(r#"dsolve("y'' + y = 2", "y", "x")"#);
    assert!(solution.contains("cos(x) * C1") && solution.contains("sin(x) * C2") && solution.contains("2"), "{solution}");
}

#[test]
fn a_differential_equation_that_cannot_be_solved_is_refused_not_half_solved() {
    // the homogeneous solution alone does not satisfy y'' + y = x: it must not be returned as if it did
    let message = refused("use * from calc\ndsolve(\"y'' + y = x\", \"y\", \"x\")");
    assert!(message.contains("right-hand side"), "{message}");
    let message = refused("use * from calc\ndsolve(\"y' = y^2\", \"y\", \"x\")");
    assert!(message.contains("integration"), "{message}");
}

#[test]
fn an_integral_that_has_no_closed_form_here_stays_an_integral() {
    check(&[
        ("use * from calc\nintegral(\"sqrt(x)\", \"x\")", "integral(sqrt(x), x)"),
        ("use * from calc\nintegral(\"exp(x)*sin(x)\", \"x\")", "integral(exp(x) * sin(x), x)"),
        ("use * from calc\nintegral(\"abs(x)\", \"x\")", "integral(abs(x), x)"),
        ("use * from calc\nintegral(\"x^x\", \"x\")", "integral(x^x, x)"),
    ]);
    assert!(refused("use * from calc\nintegral(\"exp(x^2)\", \"x\")").contains("Non-linear argument"));
    assert!(refused("use * from calc\nintegral(\"sin(x^2)\", \"x\")").contains("Non-linear argument"));
}

#[test]
fn integration_by_parts_and_a_constant_are_found() {
    check(&[
        ("use * from calc\nintegral(\"x^2 * exp(x)\", \"x\")", "exp(x) * x^2 - 2 * (exp(x) * x - exp(x))"),
        ("use * from calc\nintegral(\"x * cos(x)\", \"x\")", "sin(x) * x - cos(x) * -1"),
        ("use * from calc\nintegral(\"sin(x) + cos(x) - 4\", \"x\")", "-1 * cos(x) + sin(x) - 4 * x"),
        ("use * from calc\nintegral(\"-x\", \"x\")", "(-1 * x^2)/2"),
        ("use * from calc\nintegral(\"y\", \"x\")", "x * y"),
        ("use * from calc\nintegral(\"x^0.5\", \"x\")", "0.6666666666666666 * x^1.5"),
        ("use * from calc\nintegral(\"a^x\", \"x\")", "a^x/ln(a)"),
        ("use * from calc\nintegral(\"2*x*exp(x^2)\", \"x\")", "exp(x^2)"),
    ]);
}

#[test]
fn trigonometric_and_hyperbolic_integrals_of_a_linear_argument_are_scaled() {
    check(&[
        ("use * from calc\nintegral(\"tan(2*x)\", \"x\")", "ln(abs(sec(2 * x)))/2"),
        ("use * from calc\nintegral(\"cot(2*x)\", \"x\")", "ln(abs(sin(2 * x)))/2"),
        ("use * from calc\nintegral(\"sec(2*x)\", \"x\")", "ln(abs(sec(2 * x) + tan(2 * x)))/2"),
        ("use * from calc\nintegral(\"csc(2*x)\", \"x\")", "(ln(abs(cot(2 * x) + csc(2 * x))) * -1)/2"),
        ("use * from calc\nintegral(\"sinh(2*x)\", \"x\")", "cosh(2 * x)/2"),
        ("use * from calc\nintegral(\"cosh(2*x)\", \"x\")", "sinh(2 * x)/2"),
        ("use * from calc\nintegral(\"tanh(2*x)\", \"x\")", "ln(cosh(2 * x))/2"),
        ("use * from calc\nintegral(\"(4+x^2)^-1\", \"x\")", "atan(x/2)/2"),
        ("use * from calc\nintegral(\"cos(x)^-2\", \"x\")", "tan(x)"),
        ("use * from calc\nintegral(\"sin(3*x)^-2\", \"x\")", "(cot(3 * x) * -1)/3"),
        ("use * from calc\nintegral(\"sec(3*x)^2\", \"x\")", "tan(3 * x)/3"),
    ]);
}

#[test]
fn an_equation_is_a_value_that_takes_part_in_arithmetic() {
    check(&[
        ("a = \"x + 1 = 3\"\nb = \"x + 2 = 5\"\na + b", "2 * x + 3 = 8"),
        ("a = \"x + 1 = 3\"\na * 2", "(1 + x) * 2 = 6"),
        ("a = \"x + 1 = 3\"\na - 1 m", "1 + x - m = 3 - m"),
    ]);
}

#[test]
fn vectors_combine_item_by_item_and_with_a_scalar() {
    check(&[
        ("a = vector(1, 2, 3)\nb = vector(4, 5, 6)\na + b", "[5.0, 7.0, 9.0]"),
        ("a = vector(1, 2, 3)\na * 2", "[2.0, 4.0, 6.0]"),
        ("a = vector(1, 2, 3)\n2 * a", "[2.0, 4.0, 6.0]"),
    ]);
    assert!(refused("a = vector(1, 2, 3)\nb = vector(1, 2)\na + b").contains("Vector length mismatch"));
}

#[test]
fn functions_combine_into_functions() {
    check(&[
        ("f(x) = x + 1\ng(x) = x * 2\nf + g", "fn f_add_g"),
        ("f(x) = x + 1\ng(x) = x * 2\nf * g", "fn f_mul_g"),
        ("f(x) = x + 1\ng(x) = x * 2\nf(g)", "fn f_g"),
    ]);
    // and the combined function computes the combination
    assert_eq!(run("f(x) = x + 1\ng(x) = x * 2\nh = f + g\nh(3)"), "10.0");
    assert_eq!(run("f(x) = x + 1\ng(x) = x * 2\nh = f(g)\nh(3)"), "7.0");
}

#[test]
fn arithmetic_keeps_the_units_and_refuses_what_has_no_meaning() {
    check(&[
        ("5 m / s", "5.0 m / s"),
        ("3 m ^ 2", "3.0 m ^ 2"),
        ("2 m ^ 2 m", "2.0 m^3"),
        ("2 ^ 0.5", "1.4142135623731"),
        ("x = 5 m\nx => cm", "500.0 cm"),
        ("5 +/- 0.1 m", "5.00 ± 0.10 m"),
        ("x = 5 +/- 0.1 m\nx * x", "25.0 ± 1.0 m^2"),
    ]);
    assert!(refused("5 foobar").contains("Unknown unit 'foobar'"));
    assert!(refused("(-8) ^ 0.5").contains("negative base"));
    assert!(refused("4 / 0").contains("Division by zero"));
    assert!(refused("4 m / 0 s").contains("Division by zero"));
    assert!(refused("3 m + 4").contains("Unit mismatch"));
    assert!(refused("5 kg => m").contains("Unit mismatch"));
    assert!(refused("3 m > 1 s").contains("Unit mismatch"));
}

#[test]
fn a_range_is_its_two_ends_and_can_be_scaled_or_converted() {
    check(&[
        ("0 m .. 100 m", "0.0 m .. 100.0 m"),
        ("(0 m .. 100 m) => km", "0.0 km .. 0.1 km"),
        ("(1 .. 4) m", "1.0 m .. 4.0 m"),
        ("for i in (1 .. 4) { i * i }", "[1.0, 4.0, 9.0]"),
    ]);
}

#[test]
fn comparisons_are_booleans_and_logic_wants_booleans() {
    check(&[("1 < 2", "True"), ("2 m == 200 cm", "True"), ("ternary(1 < 2, 10, 20)", "10.0")]);
    assert!(refused("true and false").contains("expects Bool operands"));
    assert!(refused("not true").contains("expects a Bool operand"));
}

#[test]
fn the_math_builtins_agree_with_the_mathematics() {
    check(&[
        ("format(3.14159, \".2f\")", "3.14"),
        ("format(1.5, \"frac\")", "3/2"),
        ("format(2 m, \"base\")", "2.0 m"),
        ("min(3, 1 m)", "1.0 m"),
        ("max(2, 5, 4)", "5.0"),
        ("round(3.14159, 2)", "3.14"),
        ("floor(2.7) + ceil(2.1)", "5.0"),
        ("log(100)", "2.0"),
        ("sigma(5 +/- 0.1 m)", "0.1 m"),
        ("uncertainty(5 m)", "0.0 m"),
        ("vector(1, 2, 3)", "[1.0, 2.0, 3.0]"),
        ("\"hello {1 + 2}\"", "hello 3.0"),
    ]);
    assert!(refused("sqrt(-4)").contains("sqrt of a negative value"));
    assert!(refused("ln(0)").contains("ln of a non-positive value"));
    assert!(refused("parallel_map(f, vector(1, 2))").contains("parallel_map expects (fn, vector)"));
}

#[test]
fn assertions_pass_in_any_unit_and_say_what_they_compared() {
    assert!(eval_phs("assert(1 m, 100 cm)").is_ok());
    assert!(refused("assert(1 m, 2 m)").contains("assert failed: 1.0 m != 2.0 m"));
    assert!(refused("exact_assert(1 m, 100 cm)").contains("exact_assert failed"));
}
