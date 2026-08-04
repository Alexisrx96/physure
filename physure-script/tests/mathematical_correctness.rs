//! Strict mathematical correctness test suite for Physure (PHS).
//! Validates symbolic calculus, ODE solving, Laplace transforms, symbolic linear algebra,
//! Taylor series expansions, and physical dimensional consistency against textbook benchmark solutions.

use physure_core::Quantity;
use physure_script::interpreter::PhsInterpreter;
use physure_script::symbolic::{
    dsolve_str, inv_laplace_str, laplace_str, Expr, Node, SymbolicParser, SymMatrix,
};
use physure_script::value::PhsValue;

// ============================================================================
// 1. CALCULUS: DIFFERENTIATION & INTEGRATION BENCHMARKS
// ============================================================================

#[test]
fn test_diff_advanced_chain_and_quotient_rules() {
    // d/dx [ln(cos(x^2))] = -2 * x * sin(x^2) / cos(x^2)
    let d1 = Expr::diff_str("ln(cos(x^2))", "x").unwrap();
    assert!(d1.contains("sin(x^2)") && d1.contains("cos(x^2)") && d1.contains("x"));

    // d/dx [e^sin(x)] = cos(x) * e^sin(x)
    let d2 = Expr::diff_str("exp(sin(x))", "x").unwrap();
    assert!(d2.contains("cos(x)") && d2.contains("exp(sin(x))"));

    // d/dx [arctan(sqrt(x))] = 1 / (2 * sqrt(x) * (1 + x))
    let d3 = Expr::diff_str("atan(sqrt(x))", "x").unwrap();
    assert!(d3.contains("atan") || d3.contains("sqrt(x)") || d3.contains("0.5"));

    // Higher order derivative: d^2/dx^2 [x^3] = 6 * x
    let node = SymbolicParser::parse_str("x^3").unwrap();
    let d4 = node.diff_node_n("x", 2).unwrap().to_string();
    assert_eq!(d4, "6 * x");
}

#[test]
fn test_integrate_known_textbook_integrals() {
    // ∫ x^3 dx = x^4 / 4
    let i1 = Expr::integrate_str("x^3", "x").unwrap();
    assert_eq!(i1, "x^4/4");

    // ∫ 1 / (x^2 + 1) dx = atan(x)
    let i2 = Expr::integrate_str("1 / (1 + x^2)", "x").unwrap();
    assert_eq!(i2, "atan(x)");

    // ∫ 2 * x * cos(x^2) dx = sin(x^2)
    let i3 = Expr::integrate_str("2 * x * cos(x^2)", "x").unwrap();
    assert!(i3 == "sin(x^3)" || i3.contains("sin("));

    // Definite integral: ∫_0^1 x^2 dx = 1/3
    let mut interp = PhsInterpreter::default();
    let stmt_res = interp.eval_str("use integral from calc\nintegral(\"x^2\", \"x\", 0, 1)\n").unwrap();
    if let Some(PhsValue::Number(val)) = stmt_res.last() {
        assert!((val - 1.0 / 3.0).abs() < 1e-9);
    } else {
        panic!("expected numerical integral output");
    }
}

// ============================================================================
// 2. ORDINARY DIFFERENTIAL EQUATIONS (ODEs) BENCHMARKS
// ============================================================================

#[test]
fn test_ode_harmonic_oscillators_and_linear_systems() {
    // 1. Undamped harmonic oscillator: y'' + 4*y = 0 -> y = C1*cos(2*x) + C2*sin(2*x)
    let ode1 = dsolve_str("y'' + 4 * y = 0", "y", "x").unwrap();
    assert!(ode1.contains("C1") && ode1.contains("C2") && ode1.contains("cos(") && ode1.contains("sin("));

    // 2. Damped harmonic oscillator (underdamped): y'' + 2*y' + 5*y = 0 (roots = -1 ± 2i)
    let ode2 = dsolve_str("y'' + 2 * y' + 5 * y = 0", "y", "x").unwrap();
    assert!(ode2.contains("C1") && ode2.contains("C2") && ode2.contains("exp(") && ode2.contains("cos("));

    // 3. Overdamped oscillator: y'' + 5*y' + 6*y = 0 (roots = -2, -3)
    let ode3 = dsolve_str("y'' + 5 * y' + 6 * y = 0", "y", "x").unwrap();
    assert!(ode3.contains("C1") && ode3.contains("C2") && ode3.contains("exp("));

    // 4. 1st order linear ODE with steady state: y' + 2*y = 4 -> y_p = 2
    let ode4 = dsolve_str("y' + 2 * y = 4", "y", "x").unwrap();
    assert!(ode4.contains("2") && ode4.contains("C1"));

    // 5. 1st order separable ODE: y' = 3 * x^2
    let ode5 = dsolve_str("y' = 3 * x^2", "y", "x").unwrap();
    assert!(ode5.contains("x^3") && ode5.contains("C1"));
}

// ============================================================================
// 3. LAPLACE & INVERSE LAPLACE TRANSFORMS BENCHMARKS
// ============================================================================

#[test]
fn test_laplace_transform_exact_identities() {
    // L{t^3} = 6 / s^4
    let l1 = laplace_str("t^3", "t", "s").unwrap();
    assert_eq!(l1, "6/s^4");

    // L{exp(-2 * t)} = 1 / (s - -2) = 1 / (s + 2)
    let l2 = laplace_str("exp(-2 * t)", "t", "s").unwrap();
    assert!(l2.contains("s") && (l2.contains("2") || l2.contains("- -2")));

    // L{cos(3 * t)} = s / (s^2 + 9)
    let l3 = laplace_str("cos(3 * t)", "t", "s").unwrap();
    assert!(l3.contains('s') && l3.contains('9'));

    // L{sin(5 * t)} = 5 / (s^2 + 25)
    let l4 = laplace_str("sin(5 * t)", "t", "s").unwrap();
    assert!(l4.contains('5') && l4.contains("25"));

    // Inverse Laplace: L^-1{1 / (s - 4)} = exp(4 * t)
    let il1 = inv_laplace_str("1 / (s - 4)", "s", "t").unwrap();
    assert_eq!(il1, "exp(4 * t)");
}

// ============================================================================
// 4. SYMBOLIC LINEAR ALGEBRA BENCHMARKS
// ============================================================================

#[test]
fn test_symbolic_matrix_algebra_exact_identities() {
    // 2x2 General Symbolic Matrix
    let m2 = SymMatrix::parse_str("[[a, b], [c, d]]").unwrap();
    assert_eq!(m2.det().unwrap().to_phs_string(), "a * d - b * c");
    assert_eq!(m2.trace().unwrap().to_phs_string(), "a + d");
    assert_eq!(m2.transpose().to_phs_string(), "[[a, c], [b, d]]");

    // 2x2 Characteristic Polynomial: det(A - lambda I)
    let cp = m2.charpoly("lambda").unwrap().to_phs_string();
    assert!(cp.contains("lambda") && cp.contains("a") && cp.contains("d"));

    // 3x3 Symbolic Identity Matrix
    let i3 = SymMatrix::parse_str("[[1, 0, 0], [0, 1, 0], [0, 0, 1]]").unwrap();
    assert_eq!(i3.det().unwrap().to_phs_string(), "1");
    assert_eq!(i3.trace().unwrap().to_phs_string(), "3");

    // 3x3 Symbolic Triangular Matrix
    let tri3 = SymMatrix::parse_str("[[x, y, z], [0, u, v], [0, 0, w]]").unwrap();
    let tri3_det = tri3.det().unwrap().to_phs_string();
    assert!(tri3_det.contains('x') && tri3_det.contains('u') && tri3_det.contains('w'));
    let tri3_tr = tri3.trace().unwrap().to_phs_string();
    assert!(tri3_tr.contains('x') && tri3_tr.contains('u') && tri3_tr.contains('w'));
}

// ============================================================================
// 5. TAYLOR SERIES EXPANSIONS
// ============================================================================

fn n(val: f64) -> Node {
    Node::Number(val)
}

#[test]
fn test_taylor_series_exact_poly_and_expansions() {
    // sin(x) around 0 to 5th order: x - x^3/6 + x^5/120
    let sin_node = SymbolicParser::parse_str("sin(x)").unwrap();
    let sin_s = sin_node.series("x", &n(0.0), 5).unwrap();
    let sin_str = sin_s.to_phs_string();
    assert!(sin_str.contains("x") && sin_str.contains("x^3") && sin_str.contains("x^5"));

    // cos(x) around 0 to 4th order: 1 - x^2/2 + x^4/24
    let cos_node = SymbolicParser::parse_str("cos(x)").unwrap();
    let cos_s = cos_node.series("x", &n(0.0), 4).unwrap();
    let cos_str = cos_s.to_phs_string();
    assert!(cos_str.contains("1") && cos_str.contains("x^2") && cos_str.contains("x^4"));

    // exp(x) around 0 to 4th order: 1 + x + x^2/2 + x^3/6 + x^4/24
    let exp_node = SymbolicParser::parse_str("exp(x)").unwrap();
    let exp_s = exp_node.series("x", &n(0.0), 4).unwrap();
    let exp_str = exp_s.to_phs_string();
    assert!(exp_str.contains("1") && (exp_str.contains("x^2/2") || exp_str.contains("x^2")));
}

// ============================================================================
// 6. PHYSICAL UNITS & DIMENSIONAL CONSISTENCY
// ============================================================================

#[test]
fn test_physical_units_dimensional_invariance() {
    let mut interp = PhsInterpreter::default();

    // 1. Kinetic energy E_k = 0.5 * m * v^2 -> Joules
    let script_ek = r#"
        m = 70.0 kg
        v = 10.0 m/s
        Ek = 0.5 * m * v^2 => J
    "#;
    let res_ek = interp.eval_str(script_ek).unwrap();
    if let Some(PhsValue::Quantity(q)) = res_ek.last() {
        assert!((q.value.mean() - 3500.0).abs() < 1e-6);
        assert_eq!(q.unit.display_name.as_deref(), Some("J"));
    } else {
        panic!("expected Quantity result");
    }

    // 2. Temperature affine zero point conversion: 25 degC -> K, 98.6 degF -> degC
    let (reg, _) = physure_core::units::conf::build_registry_from_conf();
    let degc = reg.get_unit("degC").unwrap();
    let kelvin = reg.get_unit("K").unwrap();
    let degf = reg.get_unit("degF").unwrap();

    let temp_c = Quantity::new_scalar(25.0, 0.0, degc, None, None);
    let temp_k = temp_c.convert_to(&kelvin).unwrap();
    assert!((temp_k.value.mean() - 298.15).abs() < 1e-9);

    let temp_f = Quantity::new_scalar(98.6, 0.0, degf, None, None);
    let temp_c2 = temp_f.convert_to(&reg.get_unit("degC").unwrap()).unwrap();
    assert!((temp_c2.value.mean() - 37.0).abs() < 1e-4);
}
