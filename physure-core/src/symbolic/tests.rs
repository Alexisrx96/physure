use super::*;

fn x() -> Node {
    Node::Symbol("x".to_string())
}

fn n(v: f64) -> Node {
    Node::Number(v)
}

#[test]
fn identity_add_zero() {
    assert_eq!(Node::Add(vec![x(), n(0.0)]).simplify(), x());
}

#[test]
fn identity_mul_one() {
    assert_eq!(Node::Mul(vec![x(), n(1.0)]).simplify(), x());
}

#[test]
fn zero_mul() {
    assert_eq!(Node::Mul(vec![x(), n(0.0)]).simplify(), n(0.0));
}

#[test]
fn test_compiled_expr_eval() {
    let expr = Expr::number(2.5)
        .mul(&Expr::symbol("x".into()))
        .add(&Expr::number(1.0))
        .unwrap();
    let compiled = expr.compile().unwrap();
    assert_eq!(compiled.var_names, vec!["x"]);
    let result = compiled.eval(&[42.0]).unwrap();
    assert_eq!(result, 106.0);
}

#[test]
fn test_general_power_differentiation() {
    // d/dx [x^x]
    let expr = Node::Pow(Box::new(x()), Box::new(x()));
    let diff = expr.diff_node("x").unwrap().simplify();
    assert!(matches!(diff, Node::Mul(_)));
}

#[test]
fn test_symbolic_factorization_common_factor() {
    // a*x + b*x -> (a + b)*x or x*(a + b)
    let a = Node::Symbol("a".to_string());
    let b = Node::Symbol("b".to_string());
    let term1 = Node::Mul(vec![a.clone(), x()]);
    let term2 = Node::Mul(vec![b.clone(), x()]);
    let expr = Node::Add(vec![term1, term2]);

    let factored = expr.factor();
    let opt1 = Node::Mul(vec![x(), Node::Add(vec![a.clone(), b.clone()])]);
    let opt2 = Node::Mul(vec![Node::Add(vec![a, b]), x()]);
    assert!(factored == opt1 || factored == opt2);
}

#[test]
fn test_symbolic_factorization_combine_powers() {
    // x^2 * x^3 -> x^5
    let expr1 = Node::Pow(Box::new(x()), Box::new(n(2.0)));
    let expr2 = Node::Pow(Box::new(x()), Box::new(n(3.0)));
    let mul = Node::Mul(vec![expr1, expr2]);

    let factored = mul.factor();
    assert_eq!(factored, Node::Pow(Box::new(x()), Box::new(n(5.0))));
}

#[test]
fn test_logarithmic_quotient_integration() {
    // ∫ 1/x dx = ln(x)
    let inv = Node::Div(Box::new(n(1.0)), Box::new(x()));
    let int = inv.integrate_node("x").unwrap();
    assert_eq!(int, Node::Ln(Box::new(x())));
}

#[test]
fn test_integration_by_parts() {
    // ∫ x * cos(x) dx
    let integrand = Node::Mul(vec![x(), Node::Cos(Box::new(x()))]);
    let integrated = integrand.integrate_node("x").unwrap().simplify();
    assert!(matches!(integrated, Node::Sub(_, _)) || matches!(integrated, Node::Add(_)));
}
