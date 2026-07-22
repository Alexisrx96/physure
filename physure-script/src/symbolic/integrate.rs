use physure_core::error::{PhysureError, PhysureResult};
use super::ast::Node;

enum ArgForm {
    Var,
    Linear(f64),
    Constant,
}

fn arg_form(u: &Node, var: &str) -> Option<ArgForm> {
    if matches!(u, Node::Symbol(s) if s == var)
        || matches!(u, Node::Quantity(name, _) if name == var)
    {
        return Some(ArgForm::Var);
    }
    if !u.depends_on(var) {
        return Some(ArgForm::Constant);
    }
    match u.linear_coeff(var) {
        Some((a, _)) if a != 0.0 => Some(ArgForm::Linear(a)),
        _ => None,
    }
}

impl Node {
    pub fn integrate_node(&self, var: &str) -> PhysureResult<Node> {
        Ok(match self {
            Node::Number(c) => Node::Mul(vec![Node::Number(*c), Node::Symbol(var.to_string())]),
            Node::Symbol(s) if s == var => Node::Div(
                Box::new(Node::Pow(
                    Box::new(self.clone()),
                    Box::new(Node::Number(2.0)),
                )),
                Box::new(Node::Number(2.0)),
            ),
            Node::Quantity(name, _) if name == var => Node::Div(
                Box::new(Node::Pow(
                    Box::new(self.clone()),
                    Box::new(Node::Number(2.0)),
                )),
                Box::new(Node::Number(2.0)),
            ),
            Node::Symbol(_) | Node::Quantity(..) => {
                Node::Mul(vec![self.clone(), Node::Symbol(var.to_string())])
            }
            Node::Add(terms) => Node::Add(
                terms
                    .iter()
                    .map(|t| t.integrate_node(var))
                    .collect::<PhysureResult<Vec<_>>>()?,
            ),
            Node::Sub(a, b) => Node::Sub(
                Box::new(a.integrate_node(var)?),
                Box::new(b.integrate_node(var)?),
            ),
            Node::Mul(factors) => integrate_mul(factors, var)?,
            Node::Div(a, b) => integrate_div(a, b, var)?,
            Node::Pow(base, exp) => integrate_pow(base, exp, var)?,
            Node::Sin(u) => integrate_sin(u, var)?,
            Node::Cos(u) => integrate_cos(u, var)?,
            Node::Ln(u) => integrate_ln(u, var)?,
            Node::Exp(u) => integrate_exp(u, var)?,
        })
    }
}

fn integrate_sin(u: &Node, var: &str) -> PhysureResult<Node> {
    let neg_cos = Node::Mul(vec![Node::Number(-1.0), Node::Cos(Box::new(u.clone()))]);
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(neg_cos),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(neg_cos), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![
            Node::Sin(Box::new(u.clone())),
            Node::Symbol(var.to_string()),
        ])),
        None => Err(PhysureError::NonLinearArgument { function: "sin" }),
    }
}

fn integrate_cos(u: &Node, var: &str) -> PhysureResult<Node> {
    let sin_u = Node::Sin(Box::new(u.clone()));
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(sin_u),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(sin_u), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![
            Node::Cos(Box::new(u.clone())),
            Node::Symbol(var.to_string()),
        ])),
        None => Err(PhysureError::NonLinearArgument { function: "cos" }),
    }
}

fn integrate_exp(u: &Node, var: &str) -> PhysureResult<Node> {
    let exp_u = Node::Exp(Box::new(u.clone()));
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(exp_u),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(exp_u), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![exp_u, Node::Symbol(var.to_string())])),
        None => Err(PhysureError::NonLinearArgument { function: "exp" }),
    }
}

fn integrate_ln(u: &Node, var: &str) -> PhysureResult<Node> {
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(Node::Sub(
            Box::new(Node::Mul(vec![u.clone(), Node::Ln(Box::new(u.clone()))])),
            Box::new(u.clone()),
        )),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![
            Node::Ln(Box::new(u.clone())),
            Node::Symbol(var.to_string()),
        ])),
        _ => Err(PhysureError::UnsupportedIntegration("ln(u) only supports linear argument".into())),
    }
}

fn integrate_pow(base: &Node, exp: &Node, var: &str) -> PhysureResult<Node> {
    let Node::Number(n) = exp else {
        return Err(PhysureError::NonConstantExponent("Integration of non-constant exponent".into()));
    };
    match arg_form(base, var) {
        Some(ArgForm::Var) if *n == -1.0 => Ok(Node::Ln(Box::new(base.clone()))),
        Some(ArgForm::Var) => Ok(Node::Div(
            Box::new(Node::Pow(
                Box::new(base.clone()),
                Box::new(Node::Number(n + 1.0)),
            )),
            Box::new(Node::Number(n + 1.0)),
        )),
        Some(ArgForm::Linear(a)) if *n == -1.0 => Ok(Node::Div(
            Box::new(Node::Ln(Box::new(base.clone()))),
            Box::new(Node::Number(a)),
        )),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(
            Box::new(Node::Pow(
                Box::new(base.clone()),
                Box::new(Node::Number(n + 1.0)),
            )),
            Box::new(Node::Number(a * (n + 1.0))),
        )),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![
            Node::Pow(Box::new(base.clone()), Box::new(Node::Number(*n))),
            Node::Symbol(var.to_string()),
        ])),
        None => Err(PhysureError::NonLinearArgument { function: "base^n" }),
    }
}

fn antiderivative_of_outer(f: &Node, u: &Node) -> Option<Node> {
    match f {
        Node::Sin(_) => Some(Node::Mul(vec![
            Node::Number(-1.0),
            Node::Cos(Box::new(u.clone())),
        ])),
        Node::Cos(_) => Some(Node::Sin(Box::new(u.clone()))),
        Node::Exp(_) => Some(Node::Exp(Box::new(u.clone()))),
        _ => None,
    }
}

fn inner_arg(f: &Node) -> Option<&Node> {
    match f {
        Node::Sin(u) | Node::Cos(u) | Node::Exp(u) => Some(u),
        _ => None,
    }
}

fn try_u_substitution(p: &Node, q: &Node, var: &str, coeff: f64) -> Option<(Node, f64)> {
    let u = inner_arg(q)?;
    let du = u.diff_node(var).ok()?.simplify();
    let scaled_p = Node::Mul(vec![Node::Number(coeff), p.clone()]).simplify();
    if du == scaled_p {
        antiderivative_of_outer(q, u).map(|a| (a, 1.0))
    } else if du == p.simplify() {
        antiderivative_of_outer(q, u).map(|a| (a, coeff))
    } else {
        None
    }
}

fn try_integration_by_parts(u: &Node, dv: &Node, var: &str) -> Option<Node> {
    // Only apply integration by parts when dv is Sin, Cos, Exp, or Pow to avoid infinite recursion loops.
    if !matches!(dv, Node::Sin(_) | Node::Cos(_) | Node::Exp(_) | Node::Pow(..)) {
        return None;
    }
    if matches!(u, Node::Symbol(s) if s == var) || matches!(u, Node::Quantity(s, _) if s == var) {
        let v = dv.integrate_node(var).ok()?;
        let du = u.diff_node(var).ok()?;
        let v_du = Node::Mul(vec![v.clone(), du]).integrate_node(var).ok()?;
        let u_v = Node::Mul(vec![u.clone(), v]);
        return Some(Node::Sub(Box::new(u_v), Box::new(v_du)));
    }
    None
}

fn integrate_mul(factors: &[Node], var: &str) -> PhysureResult<Node> {
    let (const_factors, non_const): (Vec<&Node>, Vec<&Node>) =
        factors.iter().partition(|f| !f.depends_on(var));
    let const_coeff = |fs: &[&Node]| -> Option<f64> {
        let mut c = 1.0;
        for f in fs {
            match f {
                Node::Number(v) => c *= v,
                _ => return None,
            }
        }
        Some(c)
    };

    match non_const.len() {
        0 => Ok(Node::Mul(vec![
            Node::Mul(factors.to_vec()),
            Node::Symbol(var.to_string()),
        ])),
        1 => {
            let inner = non_const[0].integrate_node(var)?;
            match const_coeff(&const_factors) {
                Some(c) => Ok(Node::Mul(vec![Node::Number(c), inner])),
                None => {
                    let mut parts: Vec<Node> = const_factors.into_iter().cloned().collect();
                    parts.push(inner);
                    Ok(Node::Mul(parts))
                }
            }
        }
        2 => {
            let coeff = const_coeff(&const_factors).unwrap_or(1.0);
            for (p, q) in [(non_const[0], non_const[1]), (non_const[1], non_const[0])] {
                if let Some((antideriv, remaining)) = try_u_substitution(p, q, var, coeff) {
                    if remaining == 1.0 {
                        return Ok(antideriv);
                    }
                    return Ok(Node::Mul(vec![Node::Number(remaining), antideriv]));
                }
            }
            // Try Integration by Parts: ∫ u dv = u v - ∫ v du
            for (u, dv) in [(non_const[0], non_const[1]), (non_const[1], non_const[0])] {
                if let Some(res) = try_integration_by_parts(u, dv, var) {
                    if coeff == 1.0 {
                        return Ok(res);
                    }
                    return Ok(Node::Mul(vec![Node::Number(coeff), res]));
                }
            }
            Err(PhysureError::UnsupportedIntegration("No u-substitution or integration-by-parts pattern matched product".into()))
        }
        _ => Err(PhysureError::UnsupportedIntegration("Product with >2 non-constant factors".into())),
    }
}

fn integrate_div(a: &Node, b: &Node, var: &str) -> PhysureResult<Node> {
    if !b.depends_on(var) {
        let inner = a.integrate_node(var)?;
        return Ok(Node::Div(Box::new(inner), Box::new(b.clone())));
    }
    if matches!(a, Node::Number(v) if *v == 1.0) {
        match arg_form(b, var) {
            Some(ArgForm::Var) => return Ok(Node::Ln(Box::new(b.clone()))),
            Some(ArgForm::Linear(coeff)) => {
                return Ok(Node::Div(
                    Box::new(Node::Ln(Box::new(b.clone()))),
                    Box::new(Node::Number(coeff)),
                ));
            }
            _ => {}
        }
    }
    // Logarithmic Quotient Rule: ∫ g'(x)/g(x) dx = ln|g(x)|
    if let Ok(db) = b.diff_node(var) {
        let db_simp = db.simplify();
        let a_simp = a.simplify();
        if db_simp == a_simp {
            return Ok(Node::Ln(Box::new(b.clone())));
        }
    }
    Err(PhysureError::UnsupportedIntegration("Quotient integration not supported".into()))
}
