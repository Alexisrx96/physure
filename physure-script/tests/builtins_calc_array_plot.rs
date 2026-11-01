//! The `calc`, `array` and `plot` builtins of PHS, run as programs: what each one answers, and what it refuses.
//!
//! The expected text of a symbolic answer is the engine's own spelling of it (`3 * x^2`); each one was checked
//! by hand against the mathematics before it was written here, so a change of spelling is a conscious update of
//! this table and a change of value is a failure.

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

fn calc(call: &str) -> String {
    run(&format!("use * from calc\n{call}"))
}

fn calc_refused(call: &str) -> String {
    refused(&format!("use * from calc\n{call}"))
}

fn array(call: &str) -> String {
    run(&format!("use * from array\n{call}"))
}

fn array_refused(call: &str) -> String {
    refused(&format!("use * from array\n{call}"))
}

fn plot(call: &str) -> String {
    run(&format!("use * from plot\n{call}"))
}

fn plot_refused(call: &str) -> String {
    refused(&format!("use * from plot\n{call}"))
}

/// The number at the start of a printed value.
fn number(text: &str) -> f64 {
    text.split_whitespace().next().unwrap().parse().unwrap_or_else(|_| panic!("{text:?} does not start with a number"))
}

fn assert_table(table: &[(&str, &str)], evaluate: fn(&str) -> String) {
    for (call, expected) in table {
        assert_eq!(&evaluate(call), expected, "{call}");
    }
}

#[test]
fn derivatives_follow_the_rules_of_calculus() {
    assert_table(
        &[
            (r#"deriv("x^3", "x")"#, "3 * x^2"),
            (r#"deriv("sin(x)*x", "x")"#, "cos(x) * x + sin(x)"),
            (r#"deriv("x^4", "x", 2)"#, "12 * x^2"),
            (r#"deriv("sin(x)^2", "x")"#, "cos(x) * 2 * sin(x)"),
            (r#"deriv("exp(2*x)*ln(x)", "x")"#, "exp(2 * x)/x + 2 * exp(2 * x) * ln(x)"),
            (r#"deriv("x^x", "x")"#, "(ln(x) + 1) * x^x"),
            (r#"deriv("x/(x+1)", "x")"#, "(1 + x - x)/(1 + x)^2"),
            (r#"deriv("sqrt(x) + abs(x)", "x")"#, "1/(2 * x^0.5) + x/abs(x)"),
            (
                r#"deriv("tan(x) + cot(x) + sec(x) + csc(x)", "x")"#,
                "-1 * cot(x) * csc(x) - 1 * csc(x)^2 + sec(x) * tan(x) + sec(x)^2",
            ),
            (
                r#"deriv("arcsin(x) + arccos(x) + arctan(x)", "x")"#,
                "-1/(1 - x^2)^0.5 + 1/(1 + x^2) + 1/(1 - x^2)^0.5",
            ),
            (r#"deriv("sinh(x)*cosh(x) + tanh(x)", "x")"#, "cosh(x)^2 + sech(x)^2 + sinh(x)^2"),
        ],
        calc,
    );
}

#[test]
fn a_derivative_may_be_written_the_way_leibniz_wrote_it() {
    assert_table(
        &[
            (r#"deriv("dy/dx")"#, "y'"),
            (r#"deriv("d2y/dx2")"#, "y''"),
            (r#"deriv("y = x^2", "x")"#, "y' = 2 * x"),
        ],
        calc,
    );
}

#[test]
fn a_derivative_with_the_wrong_arguments_says_what_it_expects() {
    assert!(calc_refused("deriv()").contains("deriv expects 1, 2, or 3 arguments"));
    assert!(calc_refused("deriv(3)").contains("expression string"));
    assert!(calc_refused(r#"deriv("x^2", 3)"#).contains("variable string"));
    assert!(calc_refused(r#"deriv("not a leibniz string")"#).contains("Leibniz"));
}

#[test]
fn vector_calculus_works_component_by_component() {
    assert_table(
        &[
            (r#"grad("x^2 + y^2", vector("x", "y"))"#, "[2 * x, 2 * y]"),
            (r#"div(vector("x", "y"), vector("x", "y"))"#, "2"),
            (r#"curl(vector("y", "-x", "0"), vector("x", "y", "z"))"#, "[0, 0, -2]"),
            (r#"laplacian("x^2 + y^2", vector("x", "y"))"#, "4"),
        ],
        calc,
    );
    assert!(calc_refused(r#"grad("x")"#).contains("grad expects"));
    assert!(calc_refused(r#"div(vector("x"), vector("x", "y"))"#).contains("dimension mismatch"));
    assert!(calc_refused(r#"curl(vector("x", "y"), vector("x", "y"))"#).contains("3D"));
    assert!(calc_refused(r#"laplacian("x")"#).contains("laplacian expects"));
}

#[test]
fn a_series_keeps_the_terms_up_to_the_order_asked() {
    assert_table(
        &[
            (r#"series("exp(x)", "x")"#, "x^2/2 + x^3/6 + x^4/24 + x^5/120 + x^6/720 + 1 + x"),
            (r#"series("sin(x)", "x", 0, 5)"#, "(-1 * x^3)/6 + x^5/120 + x"),
            (r#"series("cos(x)", "x")"#, "(-1 * x^2)/2 + (-1 * x^6)/720 + x^4/24 + 1"),
            (r#"series("1/(1-x)", "x", 0, 4)"#, "1 + x^2 + x^3 + x^4 + x"),
            (r#"series("ln(x)", "x", 1, 4)"#, "(-1 * (x - 1)^2)/2 + (-1 * (x - 1)^4)/4 + (x - 1)^3/3 + x - 1"),
            (r#"simplify("x + x")"#, "2 * x"),
        ],
        calc,
    );
    assert!(calc_refused(r#"series("x")"#).contains("series expects"));
    assert!(calc_refused(r#"series("x", "x", [1])"#).contains("expansion point"));
    assert!(calc_refused(r#"series("x", "x", 0, [1])"#).contains("order"));
    assert!(calc_refused("simplify()").contains("simplify expects"));
}

#[test]
fn indefinite_integrals_are_antiderivatives() {
    assert_table(
        &[
            (r#"integral("x^2", "x")"#, "x^3/3"),
            (r#"integral("sin(x)", "x")"#, "cos(x) * -1"),
            (r#"integral("sin(x) dx")"#, "cos(x) * -1"),
            (r#"integral("cos(2*x)", "x")"#, "sin(2 * x)/2"),
            (r#"integral("exp(3*x)", "x")"#, "exp(3 * x)/3"),
            (r#"integral("ln(x)", "x")"#, "ln(x) * x - x"),
            (r#"integral("1/x", "x")"#, "ln(x)"),
            (r#"integral("1/(3*x+1)", "x")"#, "ln(3 * x + 1)/3"),
            (r#"integral("x^-2", "x")"#, "-1/x"),
            (r#"integral("2^x", "x")"#, "2^x/ln(2)"),
            (r#"integral("tan(x)", "x")"#, "ln(abs(sec(x)))"),
            (r#"integral("cot(x)", "x")"#, "ln(abs(sin(x)))"),
            (r#"integral("sec(x)", "x")"#, "ln(abs(sec(x) + tan(x)))"),
            (r#"integral("csc(x)", "x")"#, "ln(abs(cot(x) + csc(x))) * -1"),
            (r#"integral("sinh(x)", "x")"#, "cosh(x)"),
            (r#"integral("cosh(x)", "x")"#, "sinh(x)"),
            (r#"integral("tanh(x)", "x")"#, "ln(cosh(x))"),
            (r#"integral("sec(x)^2", "x")"#, "tan(x)"),
            (r#"integral("csc(2*x)^2", "x")"#, "(cot(2 * x) * -1)/2"),
            (r#"integral("1/(x^2+4)", "x")"#, "atan(x/2)/2"),
            (r#"integral("1/(x^2+1)", "x")"#, "atan(x)"),
            (r#"integral("x*exp(x)", "x")"#, "exp(x) * x - exp(x)"),
            (r#"integral("x*sin(x)", "x")"#, "cos(x) * -1 * x - -1 * sin(x)"),
            (r#"integral("x*ln(x)", "x")"#, "(ln(x) * x^2)/2 - x^2/4"),
            (r#"integral("2*x*cos(x^2)", "x")"#, "sin(x^2)"),
            (r#"integral("3", "x")"#, "3 * x"),
            (r#"integral("5*x + 2", "x")"#, "2 * x + 2.5 * x^2"),
            (r#"integral("(2*x)/(x^2+1)", "x")"#, "ln(abs(1 + x^2))"),
        ],
        calc,
    );
}

#[test]
fn an_integral_that_is_not_known_is_refused_not_guessed() {
    assert!(calc_refused(r#"integral("x/(x+1)", "x")"#).contains("Unsupported integration pattern"));
    assert!(calc_refused(r#"integral("1/cos(x)^2", "x")"#).contains("Unsupported integration pattern"));
    assert!(calc_refused("integral()").contains("integral expects"));
    // one argument must say which variable it integrates by: `"f(x) dx"`
    assert!(calc_refused(r#"integral("x + 1")"#).contains("integral expects"));
}

#[test]
fn a_definite_integral_is_the_area_under_the_curve() {
    // symbolic antiderivative at both ends
    assert_eq!(calc(r#"integral("x", "x", 0, 2)"#), "2.0");
    // no antiderivative: numerical, exact to the digits shown
    let gaussian_unit = number(&calc(r#"integral("exp(-x^2)", "x", 0, 1)"#));
    assert!((gaussian_unit - 0.746_824_132_812_427).abs() < 1e-9, "{gaussian_unit}");
}

#[test]
fn an_improper_integral_converges_to_its_value() {
    // the integral of exp(-x) from 0 to infinity is 1, and the Gaussian integral is sqrt(pi)
    let decay = number(&calc(r#"integral("exp(-x)", "x", 0, "inf")"#));
    assert!((decay - 1.0).abs() < 1e-9, "{decay}");
    let gaussian = number(&calc(r#"integral("exp(-x^2)", "x", "-inf", "inf")"#));
    assert!((gaussian - std::f64::consts::PI.sqrt()).abs() < 1e-9, "{gaussian}");
    let lorentzian = number(&calc(r#"integral("1/(1+x^2)", "x", "-inf", "inf")"#));
    assert!((lorentzian - std::f64::consts::PI).abs() < 1e-4, "{lorentzian}");
}

#[test]
fn limits_are_read_next_to_the_point_and_far_out() {
    assert!((number(&calc(r#"limit("x^2", "x", 2)"#)) - 4.0).abs() < 1e-6);
    assert_eq!(calc(r#"limit("sin(x)/x", "x", 0)"#), "1.0");
    assert_eq!(calc(r#"limit("(x^2-1)/(x-1)", "x", 1)"#), "2.0");
    assert_eq!(calc(r#"limit("1/x", "x", "inf")"#), "0.0");
    assert_eq!(calc(r#"limit("exp(x)", "x", "inf")"#), "inf");
    assert!(calc_refused(r#"limit("x", "x")"#).contains("limit expects"));
}

#[test]
fn an_equation_is_solved_for_one_unknown() {
    assert_table(
        &[
            (r#"solve("2*x + 3 = 7", "x")"#, "2.0"),
            (r#"solve("x/3 = 2", "x")"#, "6.0"),
            (r#"solve("3/x = 6", "x")"#, "0.5"),
            (r#"solve("F = m*a", "a")"#, "a = F/m"),
            (r#"substitute("x + y", "x", "2")"#, "2 + y"),
            (r#"substitute("x^2 + 1 = y", "x", "3")"#, "10 = y"),
        ],
        calc,
    );
    assert!(calc_refused(r#"solve("x^2 = 4", "x")"#).contains("Cannot solve equation symbolically"));
    assert!(calc_refused(r#"solve("a*x + b = 0", "x")"#).contains("Cannot solve equation symbolically"));
    assert!(calc_refused(r#"solve("x")"#).contains("solve expects"));
    assert!(calc_refused(r#"substitute("x")"#).contains("substitute expects"));
}

#[test]
fn laplace_transforms_go_both_ways() {
    assert_table(
        &[
            (r#"laplace("t", "t", "s")"#, "1/s^2"),
            (r#"laplace("5", "t", "s")"#, "5/s"),
            (r#"laplace("t^3", "t", "s")"#, "6/s^4"),
            (r#"laplace("3*t + 2", "t", "s")"#, "2/s + 3/s^2"),
            (r#"laplace("exp(-2*t)", "t", "s")"#, "1/(s - -2)"),
            (r#"laplace("sin(2*t)", "t", "s")"#, "2/(4 + s^2)"),
            (r#"laplace("cos(3*t)", "t", "s")"#, "s/(9 + s^2)"),
            (r#"laplace("sinh(2*t)", "t", "s")"#, "2/(s^2 - 4)"),
            (r#"laplace("cosh(2*t)", "t", "s")"#, "s/(s^2 - 4)"),
            (r#"inv_laplace("1/s", "s", "t")"#, "1"),
            (r#"inv_laplace("1/s^2", "s", "t")"#, "t"),
            (r#"inv_laplace("1/(s-3)", "s", "t")"#, "exp(3 * t)"),
            (r#"inv_laplace("s/(s^2+4)", "s", "t")"#, "cos(2 * t)"),
            (r#"inv_laplace("2/(s^2+4)", "s", "t")"#, "sin(2 * t)"),
            (r#"inv_laplace("1/(s^2+9)", "s", "t")"#, "sin(3 * t)/3"),
            (r#"inv_laplace("1/s + 1/s^2", "s", "t")"#, "1 + t"),
        ],
        calc,
    );
    assert!(calc_refused(r#"laplace("tan(t)", "t", "s")"#).contains("not implemented"));
    assert!(calc_refused(r#"inv_laplace("s^3", "s", "t")"#).contains("not implemented"));
    assert!(calc_refused(r#"laplace("t")"#).contains("laplace expects"));
    assert!(calc_refused(r#"inv_laplace("1/s")"#).contains("inv_laplace expects"));
}

#[test]
fn a_differential_equation_is_solved_with_its_constants() {
    assert_table(
        &[
            (r#"dsolve("y' = y", "y", "x")"#, "y = C1/exp(-1 * x)"),
            (r#"dsolve("y' + 2*y = 0", "y", "x")"#, "y = C1/exp(2 * x)"),
            (r#"dsolve("y' = x*y", "y", "x")"#, "y = C1/exp((-1 * x^2)/2)"),
            (r#"dsolve("y' = 3", "y", "x")"#, "y = 3 * x + C1"),
            (r#"dsolve("y'' + y = 0", "y", "x")"#, "y = cos(x) * C1 + sin(x) * C2"),
            (r#"dsolve("y'' - 3*y' + 2*y = 0", "y", "x")"#, "y = exp(2 * x) * C1 + exp(x) * C2"),
        ],
        calc,
    );
    assert!(calc_refused(r#"dsolve("y' = y")"#).contains("dsolve expects"));
}

#[test]
fn symbolic_matrices_have_a_determinant_a_trace_and_eigenvalues() {
    assert_table(
        &[
            (r#"sym_det("[[a, b], [c, d]]")"#, "a * d - b * c"),
            (r#"sym_trace("[[a, b], [c, d]]")"#, "a + d"),
            (r#"sym_charpoly("[[a, b], [c, d]]")"#, "(a - lambda) * (d - lambda) - b * c"),
            (r#"sym_eigenvalues("[[2, 0], [0, 3]]")"#, "[3, 2]"),
            (r#"sym_transpose("[[a, b], [c, d]]")"#, "[[a, c], [b, d]]"),
        ],
        calc,
    );
    assert!(calc_refused("sym_det(3)").contains("symbolic matrix"));
    assert!(calc_refused("sym_charpoly()").contains("sym_charpoly expects"));
}

#[test]
fn linspace_gives_the_number_of_points_asked() {
    assert_eq!(array("linspace(0, 1, 5)"), "[0.0, 0.25, 0.5, ... (5 items)]");
    assert_eq!(array("linspace(0 m, 4 m, 3)"), "[0.0 m, 2.0 m, 4.0 m]");
    // fifty points when no count is given
    assert!(array("linspace(0, 1)").ends_with("(50 items)]"));
    assert!(array_refused("linspace(1)").contains("linspace expects start and stop"));
}

#[test]
fn numerical_calculus_on_samples() {
    assert_eq!(array("gradient(vector(0, 1, 4, 9), vector(0, 1, 2, 3))"), "[1.0, 2.0, 4.0, 5.0]");
    assert_eq!(array("trapz(vector(0, 1, 4), vector(0, 1, 2))"), "3.0");
    // the unit of the area is the product of the units
    assert_eq!(array("trapz(vector(1 m, 2 m), vector(0 s, 1 s))"), "1.5 m * s");
    assert!(array_refused("gradient(vector(0, 1), vector(0, 1, 2))").contains("equal length"));
    assert!(array_refused("gradient(vector(0, 1))").contains("gradient expects y and x vectors"));
    assert!(array_refused("trapz(3, vector(0, 1))").contains("trapz expects y vector"));
}

#[test]
fn linear_algebra_on_vectors_and_matrices() {
    assert_table(
        &[
            ("dot(vector(1, 2, 3), vector(4, 5, 6))", "32.0"),
            ("cross(vector(1, 0, 0), vector(0, 1, 0))", "[0.0, 0.0, 1.0]"),
            ("norm(vector(3, 4))", "5.0"),
            ("transpose(vector(vector(1, 2, 3), vector(4, 5, 6)))", "[[1.0, 4.0], [2.0, 5.0], [3.0, 6.0]]"),
            ("matmul(vector(vector(1, 2), vector(3, 4)), vector(vector(5, 6), vector(7, 8)))", "[[19.0, 22.0], [43.0, 50.0]]"),
            ("det(vector(vector(1, 2), vector(3, 4)))", "-2.0"),
        ],
        array,
    );
}

#[test]
fn linear_algebra_refuses_shapes_it_cannot_use() {
    assert!(array_refused("dot(vector(1, 2), vector(4, 5, 6))").contains("equal non-empty vector lengths"));
    assert!(array_refused("dot(1, 2)").contains("dot expects 2 vectors"));
    assert!(array_refused("cross(vector(1, 0), vector(0, 1))").contains("3D"));
    assert!(array_refused("norm(vector())").contains("non-empty"));
    assert!(array_refused("norm(3)").contains("norm expects vector"));
    assert!(array_refused("matmul(vector(vector(1, 2)), vector(vector(5, 6)))").contains("dimension mismatch"));
    assert!(array_refused("matmul(1, 2)").contains("2D vector matrix"));
    assert!(array_refused("transpose(3)").contains("2D vector matrix"));
    assert!(array_refused("det(vector(vector(1, 2, 3), vector(3, 4, 5), vector(1, 1, 1)))").contains("2x2"));
}

#[test]
fn a_plot_is_drawn_with_the_units_of_its_data() {
    let curve = plot("plot(vector(1, 2, 3), vector(1, 4, 9))");
    assert!(curve.contains("Physure Live Plot") && curve.contains("9.000e0"), "{curve}");
    let titled = plot(r#"plot(vector(1, 2, 3), vector(1, 4, 9), "Squares")"#);
    assert!(titled.contains("Squares"), "{titled}");
    let in_metres = plot("plot(vector(1 m, 2 m, 3 m))");
    assert!(in_metres.contains("3.000e0 m"), "{in_metres}");
    assert!(plot_refused("plot()").contains("plot expects at least 1 argument"));
}

#[test]
fn a_surface_a_field_and_a_parallel_plot_are_described() {
    assert!(plot(r#"plot3d("x*y")"#).contains("3D Surface Plot"));
    assert!(plot("f(x, y) = x * y\nplot3d(f)").contains("3D Surface: fn f(x, y)"));
    assert!(plot(r#"plot3d("x*y", "My surface")"#).contains("My surface"));
    assert!(plot(r#"plot_field("y", "-x")"#).contains("Vector Field Plot: F = (y, -x)"));
    assert!(plot("plot_nd(vector(vector(1, 2, 3), vector(4, 5, 6)))").contains("3 dimensions, 2 samples"));
}

#[test]
fn a_plot_that_is_not_given_what_it_needs_says_so() {
    assert!(plot_refused("plot3d(3)").contains("expects a function"));
    assert!(plot_refused(r#"plot_field("y")"#).contains("plot_field expects v(x, y)"));
    assert!(plot_refused("plot_field(1, 2)").contains("plot_field expects u(x, y)"));
    assert!(plot_refused("plot_nd()").contains("2D matrix"));
    assert!(plot_refused("plot_nd(3)").contains("2D matrix"));
}
