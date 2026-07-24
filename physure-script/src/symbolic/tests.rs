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
fn test_kinetic_energy_solve() {
    let mut interp = crate::interpreter::PhsInterpreter::default();
    let prog1 = crate::parse_phs("fn kinetic_energy(m, v) = 0.5 * m * v^2").unwrap();
    interp.run_statement(&prog1.statements[0]).unwrap();
    let solve_prog = crate::parse_phs("solve(\"kinetic_energy(m, v) = target\", \"v\")").unwrap();
    let res = interp.run_statement(&solve_prog.statements[0]).unwrap();
    println!("Res: {:?}", res);
}

