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
