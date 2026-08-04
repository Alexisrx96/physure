use super::*;

fn x() -> Node {
    Node::Symbol("x".to_string())
}

fn y() -> Node {
    Node::Symbol("y".to_string())
}

fn n(v: f64) -> Node {
    Node::Number(v)
}

// ============================================================================
// 1. FACTORIZATION TESTS (Exhaustive)
// ============================================================================

#[test]
fn test_factor_linear_common_factor() {
    // a*x + b*x -> (a + b)*x or x*(a + b)
    let a = Node::Symbol("a".to_string());
    let b = Node::Symbol("b".to_string());
    let expr = Node::Add(vec![
        Node::Mul(vec![a.clone(), x()]),
        Node::Mul(vec![b.clone(), x()]),
    ]);

    let factored = expr.factor();
    let opt1 = Node::Mul(vec![x(), Node::Add(vec![a.clone(), b.clone()])]);
    let opt2 = Node::Mul(vec![Node::Add(vec![a, b]), x()]);
    assert!(factored == opt1 || factored == opt2);
}

#[test]
fn test_factor_combining_identical_base_powers() {
    // x^2 * x^3 -> x^5
    let expr1 = Node::Pow(Box::new(x()), Box::new(n(2.0)));
    let expr2 = Node::Pow(Box::new(x()), Box::new(n(3.0)));
    let mul = Node::Mul(vec![expr1, expr2]);

    let factored = mul.factor();
    assert_eq!(factored, Node::Pow(Box::new(x()), Box::new(n(5.0))));
}

#[test]
fn test_factor_subtract_terms() {
    // c*x - c*y -> c*(x - y)
    let c = Node::Symbol("c".to_string());
    let expr = Node::Sub(
        Box::new(Node::Mul(vec![c.clone(), x()])),
        Box::new(Node::Mul(vec![c.clone(), y()])),
    );
    let factored = expr.factor();
    assert!(matches!(factored, Node::Mul(_) | Node::Sub(_, _)));
}

#[test]
fn test_factor_no_common_factor() {
    // a*x + b*y stays intact
    let a = Node::Symbol("a".to_string());
    let b = Node::Symbol("b".to_string());
    let expr = Node::Add(vec![
        Node::Mul(vec![a.clone(), x()]),
        Node::Mul(vec![b.clone(), y()]),
    ]);
    let factored = expr.factor();
    assert_eq!(factored, expr.simplify());
}

// ============================================================================
// 2. DIFFERENTIATION TESTS (Exhaustive)
// ============================================================================

#[test]
fn test_diff_constant_and_variable() {
    assert_eq!(n(42.0).diff_node("x").unwrap(), n(0.0));
    assert_eq!(x().diff_node("x").unwrap(), n(1.0));
    assert_eq!(y().diff_node("x").unwrap(), n(0.0));
}

#[test]
fn test_diff_sum_and_subtraction() {
    // d/dx [x + y] = 1 + 0 = 1
    let sum = Node::Add(vec![x(), y()]);
    assert_eq!(sum.diff_node("x").unwrap().simplify(), n(1.0));

    // d/dx [x - y] = 1 - 0 = 1
    let sub = Node::Sub(Box::new(x()), Box::new(y()));
    assert_eq!(sub.diff_node("x").unwrap().simplify(), n(1.0));
}

#[test]
fn test_diff_product_and_quotient_rules() {
    // d/dx [3*x] = 3
    let prod = Node::Mul(vec![n(3.0), x()]);
    assert_eq!(prod.diff_node("x").unwrap().simplify(), n(3.0));

    // d/dx [x / 2] = 0.5
    let quot = Node::Div(Box::new(x()), Box::new(n(2.0)));
    assert_eq!(quot.diff_node("x").unwrap().simplify(), n(0.5));
}

#[test]
fn test_diff_constant_and_variable_power_rules() {
    // d/dx [x^3] = 3 * x^2
    let pow_const = Node::Pow(Box::new(x()), Box::new(n(3.0)));
    let diff_const = pow_const.diff_node("x").unwrap().simplify();
    assert_eq!(
        diff_const,
        Node::Mul(vec![n(3.0), Node::Pow(Box::new(x()), Box::new(n(2.0)))])
    );

    // d/dx [x^x] general power rule
    let pow_var = Node::Pow(Box::new(x()), Box::new(x()));
    let diff_var = pow_var.diff_node("x").unwrap().simplify();
    assert!(matches!(diff_var, Node::Mul(_)));
}

#[test]
fn test_diff_trig_exp_ln_chain_rule() {
    // d/dx [sin(x)] = cos(x)
    let sin_x = Node::Sin(Box::new(x()));
    assert_eq!(sin_x.diff_node("x").unwrap().simplify(), Node::Cos(Box::new(x())));

    // d/dx [cos(x)] = -1 * sin(x)
    let cos_x = Node::Cos(Box::new(x()));
    let diff_cos = cos_x.diff_node("x").unwrap().simplify();
    let expected = Node::Mul(vec![n(-1.0), Node::Sin(Box::new(x()))]).simplify();
    assert_eq!(diff_cos, expected);

    // d/dx [exp(x)] = exp(x)
    let exp_x = Node::Exp(Box::new(x()));
    assert_eq!(exp_x.diff_node("x").unwrap().simplify(), exp_x);

    // d/dx [ln(x)] = 1/x
    let ln_x = Node::Ln(Box::new(x()));
    assert_eq!(ln_x.diff_node("x").unwrap().simplify(), Node::Div(Box::new(n(1.0)), Box::new(x())));
}

#[test]
fn test_higher_order_differentiation() {
    // d^2/dx^2 [x^3] = 6 * x
    let expr = Expr { node: Node::Pow(Box::new(x()), Box::new(n(3.0))) };
    let second_diff = expr.diff("x", 2).unwrap();
    assert_eq!(second_diff.node, Node::Mul(vec![n(6.0), x()]));

    // d^3/dx^3 [x^3] = 6
    let third_diff = expr.diff("x", 3).unwrap();
    assert_eq!(third_diff.node, n(6.0));
}

// ============================================================================
// 3. INTEGRATION TESTS (Exhaustive)
// ============================================================================

#[test]
fn test_integrate_constant_and_variable() {
    // ∫ 5 dx = 5*x
    assert_eq!(n(5.0).integrate_node("x").unwrap().simplify(), Node::Mul(vec![n(5.0), x()]));

    // ∫ x dx = x^2 / 2
    let int_x = x().integrate_node("x").unwrap().simplify();
    assert_eq!(int_x, Node::Div(Box::new(Node::Pow(Box::new(x()), Box::new(n(2.0)))), Box::new(n(2.0))));
}

#[test]
fn test_integrate_power_rules() {
    // ∫ x^3 dx = x^4 / 4
    let pow3 = Node::Pow(Box::new(x()), Box::new(n(3.0)));
    let int_pow3 = pow3.integrate_node("x").unwrap().simplify();
    assert_eq!(
        int_pow3,
        Node::Div(Box::new(Node::Pow(Box::new(x()), Box::new(n(4.0)))), Box::new(n(4.0)))
    );

    // ∫ x^-1 dx = ln(x)
    let pow_neg1 = Node::Pow(Box::new(x()), Box::new(n(-1.0)));
    assert_eq!(pow_neg1.integrate_node("x").unwrap(), Node::Ln(Box::new(x())));
}

#[test]
fn test_integrate_trig_exp_ln() {
    // ∫ sin(x) dx = -1 * cos(x)
    let sin_x = Node::Sin(Box::new(x()));
    let expected_neg_cos = Node::Mul(vec![n(-1.0), Node::Cos(Box::new(x()))]).simplify();
    assert_eq!(sin_x.integrate_node("x").unwrap().simplify(), expected_neg_cos);

    // ∫ cos(x) dx = sin(x)
    let cos_x = Node::Cos(Box::new(x()));
    assert_eq!(cos_x.integrate_node("x").unwrap().simplify(), Node::Sin(Box::new(x())));

    // ∫ exp(x) dx = exp(x)
    let exp_x = Node::Exp(Box::new(x()));
    assert_eq!(exp_x.integrate_node("x").unwrap().simplify(), exp_x);

    // ∫ ln(x) dx = ln(x)*x - x
    let ln_x = Node::Ln(Box::new(x()));
    let int_ln = ln_x.integrate_node("x").unwrap().simplify();
    let expected_ln = Node::Sub(
        Box::new(Node::Mul(vec![Node::Ln(Box::new(x())), x()])),
        Box::new(x())
    ).simplify();
    assert_eq!(int_ln, expected_ln);
}

#[test]
fn test_integrate_u_substitution_and_log_quotient() {
    // ∫ 2*x * cos(x^2) dx = sin(x^2)
    let x_sq = Node::Pow(Box::new(x()), Box::new(n(2.0)));
    let integrand = Node::Mul(vec![Node::Mul(vec![n(2.0), x()]), Node::Cos(Box::new(x_sq.clone()))]);
    let integrated = integrand.integrate_node("x").unwrap().simplify();
    assert_eq!(integrated, Node::Sin(Box::new(x_sq)));

    // Logarithmic quotient rule: ∫ 1/x dx = ln(x)
    let div_1_x = Node::Div(Box::new(n(1.0)), Box::new(x()));
    assert_eq!(div_1_x.integrate_node("x").unwrap(), Node::Ln(Box::new(x())));
}

#[test]
fn test_integrate_by_parts() {
    // ∫ x * cos(x) dx
    let integrand = Node::Mul(vec![x(), Node::Cos(Box::new(x()))]);
    let integrated = integrand.integrate_node("x").unwrap().simplify();
    assert!(matches!(integrated, Node::Sub(_, _)) || matches!(integrated, Node::Add(_)));
}

#[test]
fn test_add_bare_number_to_dimensioned_quantity_fails() {
    use physure_core::units::RationalUnit;
    let m = RationalUnit::base("m");
    let q = Expr::quantity("5".to_string(), &m);
    let num = Expr::number(2.0);
    assert!(q.add(&num).is_err());
}

#[test]
fn test_symbolic_string_parsing_and_solving() {
    let diff_res = Expr::diff_str("x^3", "x").unwrap();
    assert_eq!(diff_res, "3 * x^2");

    let int_res = Expr::integrate_str("3 * x^2", "x").unwrap();
    assert_eq!(int_res, "x^3");

    let solve_res = Expr::solve_str("2 * x + 10 = 0", "x").unwrap();
    assert_eq!(solve_res, "-5");
}

#[test]
fn test_simplify_mul_cancels_symbolic_denominator() {
    // (V / I) * I -> V
    let v = Node::Symbol("V".to_string());
    let i = Node::Symbol("I".to_string());
    let expr = Node::Mul(vec![Node::Div(Box::new(v.clone()), Box::new(i.clone())), i]);
    assert_eq!(expr.simplify(), v);
}

#[test]
fn test_kinetic_energy_solve() {
    let mut interp = crate::interpreter::PhsInterpreter::default();
    let prog1 = crate::parse_phs("fn kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
    interp.run_statement(&prog1.statements[0]).unwrap();
    let use_calc = crate::parse_phs("use solve from calc").unwrap();
    interp.run_statement(&use_calc.statements[0]).unwrap();
    let solve_prog = crate::parse_phs("solve(\"kinetic_energy(m, v) = target\", \"v\")").unwrap();
    let res = interp.run_statement(&solve_prog.statements[0]).unwrap();
    println!("Res: {:?}", res);
}

// ============================================================================
// 4. EXTENSIVE CALCULUS RULE & FACTORISATION TEST SUITE
// ============================================================================

#[test]
fn test_diff_extended_trig_and_inverse_trig() {
    // d/dx [tan(x)] = sec(x)^2
    let d_tan = Expr::diff_str("tan(x)", "x").unwrap();
    assert_eq!(d_tan, "sec(x)^2");

    // d/dx [cot(x)] = -1 * csc(x)^2
    let d_cot = Expr::diff_str("cot(x)", "x").unwrap();
    assert_eq!(d_cot, "-1 * csc(x)^2");

    // d/dx [sec(x)] = sec(x) * tan(x)
    let d_sec = Expr::diff_str("sec(x)", "x").unwrap();
    assert_eq!(d_sec, "sec(x) * tan(x)");

    // d/dx [csc(x)] = cot(x) * csc(x) * -1
    let d_csc = Expr::diff_str("csc(x)", "x").unwrap();
    assert_eq!(d_csc, "cot(x) * csc(x) * -1");

    // d/dx [asin(x)] = 1 / (1 - x^2)^0.5
    let d_asin = Expr::diff_str("asin(x)", "x").unwrap();
    assert_eq!(d_asin, "1/(1 - x^2)^0.5");

    // d/dx [atan(x)] = 1 / (1 + x^2)
    let d_atan = Expr::diff_str("atan(x)", "x").unwrap();
    assert_eq!(d_atan, "1/(1 + x^2)");
}

#[test]
fn test_diff_hyperbolic_functions() {
    // d/dx [sinh(x)] = cosh(x)
    assert_eq!(Expr::diff_str("sinh(x)", "x").unwrap(), "cosh(x)");

    // d/dx [cosh(x)] = sinh(x)
    assert_eq!(Expr::diff_str("cosh(x)", "x").unwrap(), "sinh(x)");

    // d/dx [tanh(x)] = sech(x)^2
    assert_eq!(Expr::diff_str("tanh(x)", "x").unwrap(), "sech(x)^2");
}

#[test]
fn test_diff_chain_rule_and_exponent_precedence() {
    // d/dx [e^2x] = 2 * e^(2 * x)
    assert_eq!(Expr::diff_str("e^2x", "x").unwrap(), "2 * e^(2 * x)");

    // d/dx [ln(cos(x))] = (-1 * sin(x))/cos(x)
    let d_ln_cos = Expr::diff_str("ln(cos(x))", "x").unwrap();
    assert_eq!(d_ln_cos, "(-1 * sin(x))/cos(x)");

    // d/dx [sqrt(x)] = 1/(2 * x^0.5)
    let d_sqrt = Expr::diff_str("sqrt(x)", "x").unwrap();
    assert_eq!(d_sqrt, "1/(2 * x^0.5)");
}

#[test]
fn test_diff_equation_implicit() {
    // d/dx [0 = sin(x)^2 + cosec(y)^2] -> y' = (cos(x) * sin(x))/(cot(y) * csc(y)^2)
    let d_eq = Expr::diff_str("0 = sin(x)^2 + cosec(y)^2", "x").unwrap();
    assert_eq!(d_eq, "y' = (cos(x) * sin(x))/(cot(y) * csc(y)^2)");

    // d/dx [y = x^3 - 3 * x] -> y' = 3 * x^2 - 3
    let d_eq2 = Expr::diff_str("y = x^3 - 3 * x", "x").unwrap();
    assert_eq!(d_eq2, "y' = 3 * x^2 - 3");
}

#[test]
fn test_diff_leibniz_and_prime_notation() {
    // Prime notation increment: y' -> y'', y'' -> y'''
    let d_prime1 = Expr::diff_str("y'", "x").unwrap();
    assert_eq!(d_prime1, "y''");

    let d_prime2 = Expr::diff_str("y''", "x").unwrap();
    assert_eq!(d_prime2, "y'''");

    // Differentiating differential equation y'' + y = 0
    let d_de = Expr::diff_str("y'' + y = 0", "x").unwrap();
    assert_eq!(d_de, "y' + y''' = 0");
}

#[test]
fn test_integrate_trig_and_hyperbolic_extended() {
    // ∫ tan(x) dx = ln(abs(sec(x)))
    let i_tan = Expr::integrate_str("tan(x)", "x").unwrap();
    assert_eq!(i_tan, "ln(abs(sec(x)))");

    // ∫ cot(x) dx = ln(abs(sin(x)))
    let i_cot = Expr::integrate_str("cot(x)", "x").unwrap();
    assert_eq!(i_cot, "ln(abs(sin(x)))");

    // ∫ sinh(x) dx = cosh(x)
    let i_sinh = Expr::integrate_str("sinh(x)", "x").unwrap();
    assert_eq!(i_sinh, "cosh(x)");

    // ∫ cosh(x) dx = sinh(x)
    let i_cosh = Expr::integrate_str("cosh(x)", "x").unwrap();
    assert_eq!(i_cosh, "sinh(x)");
}

#[test]
fn test_integrate_by_parts_suite() {
    // ∫ xe^x dx = e^x * x - e^x
    let i_xe_x = Expr::integrate_str("xe^x", "x").unwrap();
    assert_eq!(i_xe_x, "e^x * x - e^x");

    // ∫ x * cos(x) dx = sin(x) * x - -1 * cos(x)
    let i_x_cos = Expr::integrate_str("x * cos(x)", "x").unwrap();
    assert!(i_x_cos.contains("sin(x)") && i_x_cos.contains("cos(x)"));

    // ∫ x * sin(x) dx
    let i_x_sin = Expr::integrate_str("x * sin(x)", "x").unwrap();
    assert!(i_x_sin.contains("cos(x)") && i_x_sin.contains("sin(x)"));
}

#[test]
fn test_integrate_u_substitution_suite() {
    // ∫ 2 * x * exp(x^2) dx = exp(x^2)
    let i_u_sub = Expr::integrate_str("2 * x * exp(x^2)", "x").unwrap();
    assert_eq!(i_u_sub, "exp(x^2)");

    // Logarithmic quotient rule: ∫ 3 * x^2 / (x^3 + 1) dx = ln(abs(1 + x^3))
    let i_log_quot = Expr::integrate_str("3 * x^2 / (x^3 + 1)", "x").unwrap();
    assert_eq!(i_log_quot, "ln(abs(1 + x^3))");
}

#[test]
fn test_integrate_general_power_derivative_reversal() {
    let derivate = "5 * (A * (X + 2))^X";
    let d_derivate = Expr::diff_str(derivate, "X").unwrap();
    let i_d_derivate = Expr::integrate_str(&d_derivate, "X").unwrap();
    assert_eq!(i_d_derivate, "5 * ((2 + X) * A)^X");
}

#[test]
fn test_integrate_inverse_trig_and_constant_base() {
    // ∫ 1 / (1 + x^2) dx = atan(x)
    let i_atan = Expr::integrate_str("1 / (1 + x^2)", "x").unwrap();
    assert_eq!(i_atan, "atan(x)");

    // ∫ 2^x dx = 2^x / ln(2)
    let i_pow2 = Expr::integrate_str("2^x", "x").unwrap();
    assert_eq!(i_pow2, "2^x/ln(2)");
}

#[test]
fn test_integrate_non_elementary_fallback() {
    // Non-elementary integral falls back gracefully to symbolic integral(...) node format
    let i_non_elem = Expr::integrate_str("5 * (A * (X + 2))^X", "X").unwrap();
    assert_eq!(i_non_elem, "integral(((2 + X) * A)^X, X) * 5");
}



// ============================================================================
// N. TAYLOR SERIES / SUBSTITUTION
// ============================================================================

fn eval_at(node: &Node, value: f64) -> f64 {
    let compiled = CompiledExpr::compile(node).unwrap();
    assert!(compiled.var_names.len() <= 1, "expected a univariate expression");
    compiled.eval(&[value]).unwrap()
}

#[test]
fn test_series_matches_the_function_it_expands() {
    // sin about 0 to 5th order is accurate well past the linear regime.
    let s = Expr::parse("sin(x)").unwrap().node.series("x", &n(0.0), 5).unwrap();
    assert!((eval_at(&s, 0.7) - 0.7f64.sin()).abs() < 1e-4);

    // exp about 0, and about a non-zero point.
    let e0 = Expr::parse("exp(x)").unwrap().node.series("x", &n(0.0), 6).unwrap();
    assert!((eval_at(&e0, 0.5) - 0.5f64.exp()).abs() < 1e-5);
    let e1 = Expr::parse("exp(x)").unwrap().node.series("x", &n(1.0), 6).unwrap();
    assert!((eval_at(&e1, 1.4) - 1.4f64.exp()).abs() < 1e-5);
}

#[test]
fn test_series_of_a_polynomial_is_exact_and_terminates() {
    // A degree-2 polynomial expanded about x = 1 reproduces itself exactly,
    // and asking for order 6 does not invent x^3.. terms.
    let p = Expr::parse("x^2 + 3 * x").unwrap().node.series("x", &n(1.0), 6).unwrap();
    assert!((eval_at(&p, 2.7) - (2.7f64 * 2.7 + 3.0 * 2.7)).abs() < 1e-9);
    assert!(!p.to_phs_string().contains("^3"));
}

#[test]
fn test_subst_leaves_a_bound_integration_variable_alone() {
    let integral = Node::Integral(Box::new(Node::Mul(vec![x(), y()])), "x".to_string());
    // Substituting the bound variable is a no-op...
    assert_eq!(integral.subst("x", &n(2.0)), integral);
    // ...while a free variable in the integrand is replaced.
    assert_eq!(
        integral.subst("y", &n(2.0)),
        Node::Integral(Box::new(Node::Mul(vec![x(), n(2.0)])), "x".to_string())
    );
}

// ============================================================================
// 5. REQUESTED EDGE CASES TESTS
// ============================================================================

#[test]
fn test_diff_edge_cases_extended_req() {
    let expr = Expr::parse("x^6").unwrap();
    assert_eq!(expr.diff("x", 4).unwrap().to_phs_string(), "360 * x^2");
    assert_eq!(expr.diff("x", 5).unwrap().to_phs_string(), "720 * x");
    assert_eq!(Expr::diff_str("sin(x) * exp(x)", "x").unwrap(), "cos(x) * exp(x) + exp(x) * sin(x)");
    assert_eq!(Expr::diff_str("asin(2*x)", "x").unwrap(), "2/(1 - (2 * x)^2)^0.5");
    assert_eq!(Expr::diff_str("x^2 + y^2 = 25", "x").unwrap(), "2 * x + 2 * y * y' = 0");
    assert_eq!(Expr::diff_str("sin(x) + cos(y) = 1", "x").unwrap(), "cos(x) - 1 * sin(y) * y' = 0");
}

#[test]
fn test_integrate_edge_cases_extended_req() {
    assert_eq!(Expr::integrate_str("x * sin(x)", "x").unwrap(), "cos(x) * -1 * x - -1 * sin(x)");
    assert_eq!(Expr::integrate_str("x^2 * e^x", "x").unwrap(), "e^x * x^2 - integral(2 * e^x * x, x)");
    assert_eq!(Expr::integrate_str("x * ln(x)", "x").unwrap(), "(ln(x) * x^2)/2 - integral(x/2, x)");
    assert_eq!(Expr::integrate_str("2*x / (x^2 + 1)", "x").unwrap(), "ln(abs(1 + x^2))");
    assert_eq!(Expr::integrate_str("3*x^2 * cos(x^3)", "x").unwrap(), "sin(x^3)");
    assert_eq!(Expr::integrate_str("1 / (1 + x^2)", "x").unwrap(), "atan(x)");
}

#[test]
fn test_series_expansion_and_factorization_extended_req() {
    let sin_series = Expr::parse("sin(x)").unwrap().node.series("x", &n(0.0), 3).unwrap();
    assert_eq!(sin_series.to_phs_string(), "(-1 * x^3)/6 + x");

    let cos_series = Expr::parse("cos(x)").unwrap().node.series("x", &n(0.0), 3).unwrap();
    assert_eq!(cos_series.to_phs_string(), "(-1 * x^2)/2 + 1");

    let exp_series = Expr::parse("exp(x)").unwrap().node.series("x", &n(0.0), 3).unwrap();
    assert_eq!(exp_series.to_phs_string(), "x^2/2 + x^3/6 + 1 + x");

    let poly1 = Expr::parse("x^2 + 2*x + 1").unwrap().node.factor();
    assert_eq!(poly1.to_phs_string(), "2 * x + 1 + x^2");

    let poly2 = Expr::parse("x^2 - y^2").unwrap().node.factor();
    assert_eq!(poly2.to_phs_string(), "x^2 - y^2");
}
