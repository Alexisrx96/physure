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
            Node::Tan(u) => integrate_tan(u, var)?,
            Node::Cot(u) => integrate_cot(u, var)?,
            Node::Sec(u) => integrate_sec(u, var)?,
            Node::Csc(u) => integrate_csc(u, var)?,
            Node::Sinh(u) => integrate_sinh(u, var)?,
            Node::Cosh(u) => integrate_cosh(u, var)?,
            Node::Tanh(u) => integrate_tanh(u, var)?,
            Node::Ln(u) => integrate_ln(u, var)?,
            Node::Exp(u) => integrate_exp(u, var)?,
            _ => Node::Integral(Box::new(self.clone()), var.to_string()),
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
    if let Some(result) = integrate_exponential(base, exp, var) {
        return Ok(result);
    }

    let Node::Number(n) = exp else {
        return Ok(Node::Integral(Box::new(Node::Pow(Box::new(base.clone()), Box::new(exp.clone()))), var.to_string()));
    };

    if let Some(result) = integrate_trig_square(base, *n, var).or_else(|| integrate_inverse_trig(base, *n, var)) {
        return Ok(result);
    }

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
        None => Ok(Node::Integral(Box::new(Node::Pow(Box::new(base.clone()), Box::new(Node::Number(*n)))), var.to_string())),
    }
}

/// `a^(k x)` with a constant base: `a^(k x) / (k ln a)`.
fn integrate_exponential(base: &Node, exp: &Node, var: &str) -> Option<Node> {
    if base.depends_on(var) || !exp.depends_on(var) {
        return None;
    }
    let power = Node::Pow(Box::new(base.clone()), Box::new(exp.clone()));
    let ln_base = Node::Ln(Box::new(base.clone()));
    match arg_form(exp, var)? {
        ArgForm::Var => Some(Node::Div(Box::new(power), Box::new(ln_base))),
        ArgForm::Linear(k) => Some(Node::Div(Box::new(power), Box::new(Node::Mul(vec![Node::Number(k), ln_base])))),
        ArgForm::Constant => None,
    }
}

/// `sec(u)^2` and `cos(u)^-2` (tangent), `csc(u)^2` and `sin(u)^-2` (minus the cotangent), for `u` linear in the variable.
fn integrate_trig_square(base: &Node, n: f64, var: &str) -> Option<Node> {
    let (u, antiderivative) = match (n == 2.0, n == -2.0, base) {
        (true, _, Node::Sec(u)) | (_, true, Node::Cos(u)) => (u, Node::Tan(Box::new((**u).clone()))),
        (true, _, Node::Csc(u)) | (_, true, Node::Sin(u)) => {
            (u, Node::Mul(vec![Node::Number(-1.0), Node::Cot(Box::new((**u).clone()))]))
        }
        _ => return None,
    };
    match arg_form(u, var)? {
        ArgForm::Var => Some(antiderivative),
        ArgForm::Linear(a) => Some(Node::Div(Box::new(antiderivative), Box::new(Node::Number(a)))),
        ArgForm::Constant => None,
    }
}

/// Fallback inverse trig for `1/(a^2 + x^2)` (n = -1) and `1/sqrt(1 - x^2)` (n = -0.5).
fn integrate_inverse_trig(base: &Node, n: f64, var: &str) -> Option<Node> {
    if n == -1.0 {
        integrate_arctan_form(base, var)
    } else if n == -0.5 {
        integrate_arcsin_form(base, var)
    } else {
        None
    }
}

fn integrate_arctan_form(base: &Node, var: &str) -> Option<Node> {
    let Node::Add(terms) = base else {
        return None;
    };
    if terms.len() != 2 {
        return None;
    }
    let mut x_term = None;
    let mut const_term = None;
    for t in terms {
        if let Node::Pow(b, e) = t {
            if let Node::Number(2.0) = **e {
                match arg_form(b, var) {
                    Some(ArgForm::Var) => x_term = Some((**b).clone()),
                    // Simplified for 1/(a^2 + (bx)^2)
                    Some(ArgForm::Linear(a)) => x_term = Some(Node::Mul(vec![Node::Number(a), (**b).clone()])),
                    _ => {}
                }
            }
        } else if let Node::Number(v) = t {
            const_term = Some(*v);
        }
        // a bare x term, but not x^2, is ignored
    }
    let (Some(x), Some(c)) = (x_term, const_term) else {
        return None;
    };
    if c > 0.0 {
        let a = c.sqrt();
        return Some(Node::Mul(vec![
            Node::Div(Box::new(Node::Number(1.0)), Box::new(Node::Number(a))),
            Node::Arctan(Box::new(Node::Div(Box::new(x), Box::new(Node::Number(a))))),
        ]));
    }
    None
}

fn integrate_arcsin_form(base: &Node, var: &str) -> Option<Node> {
    let Node::Sub(c, xsq) = base else {
        return None;
    };
    if !matches!(**c, Node::Number(c_val) if c_val == 1.0) {
        return None;
    }
    let Node::Pow(b, e) = &**xsq else {
        return None;
    };
    if !matches!(**e, Node::Number(2.0)) {
        return None;
    }
    match arg_form(b, var) {
        Some(ArgForm::Var) => Some(Node::Arcsin(Box::new((**b).clone()))),
        _ => None,
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
        Node::Pow(_, exp) => {
            if let Node::Number(e) = **exp {
                if e == -1.0 {
                    Some(Node::Ln(Box::new(Node::Abs(Box::new(u.clone())))))
                } else {
                    Some(Node::Div(
                        Box::new(Node::Pow(Box::new(u.clone()), Box::new(Node::Number(e + 1.0)))),
                        Box::new(Node::Number(e + 1.0)),
                    ))
                }
            } else {
                None
            }
        }
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
    if !matches!(dv, Node::Sin(_) | Node::Cos(_) | Node::Exp(_) | Node::Pow(..) | Node::Ln(_)) && !matches!(u, Node::Ln(_)) {
        return None;
    }
    
    // Poly * Ln
    if matches!(u, Node::Ln(_)) {
        let v = dv.integrate_node(var).ok()?;
        let du = u.diff_node(var).ok()?;
        let v_du = Node::Mul(vec![v.clone(), du]).simplify();
        let v_du_integrated = v_du.integrate_node(var).ok()?;
        let u_v = Node::Mul(vec![u.clone(), v]);
        return Some(Node::Sub(Box::new(u_v), Box::new(v_du_integrated)));
    }
    
    // Poly * Exp / Trig
    let u_is_poly = match u {
        Node::Symbol(s) if s == var => true,
        Node::Pow(base, exp) => matches!(**base, Node::Symbol(ref s) if s == var) && matches!(**exp, Node::Number(n) if n > 0.0 && n.fract() == 0.0),
        _ => false,
    };
    
    if u_is_poly {
        let v = dv.integrate_node(var).ok()?;
        let du = u.diff_node(var).ok()?;
        let v_du = Node::Mul(vec![v.clone(), du]).simplify();
        let v_du_integrated = v_du.integrate_node(var).ok()?;
        let u_v = Node::Mul(vec![u.clone(), v]);
        return Some(Node::Sub(Box::new(u_v), Box::new(v_du_integrated)));
    }
    
    None
}

/// The product of the factors when they are all numbers; `None` as soon as one is not.
fn constant_coefficient(factors: &[&Node]) -> Option<f64> {
    let mut c = 1.0;
    for f in factors {
        match f {
            Node::Number(v) => c *= v,
            _ => return None,
        }
    }
    Some(c)
}

/// `coeff * node`, written as `node` itself when the coefficient is 1.
fn scaled(coeff: f64, node: Node) -> Node {
    if coeff == 1.0 {
        node
    } else {
        Node::Mul(vec![Node::Number(coeff), node])
    }
}

fn integrate_mul(factors: &[Node], var: &str) -> PhysureResult<Node> {
    let (const_factors, non_const): (Vec<&Node>, Vec<&Node>) =
        factors.iter().partition(|f| !f.depends_on(var));

    match non_const.len() {
        0 => Ok(Node::Mul(vec![
            Node::Mul(factors.to_vec()),
            Node::Symbol(var.to_string()),
        ])),
        1 => {
            let inner = non_const[0].integrate_node(var)?;
            match constant_coefficient(&const_factors) {
                Some(c) => Ok(Node::Mul(vec![Node::Number(c), inner])),
                None => {
                    let mut parts: Vec<Node> = const_factors.into_iter().cloned().collect();
                    parts.push(inner);
                    Ok(Node::Mul(parts))
                }
            }
        }
        2 => {
            let coeff = constant_coefficient(&const_factors).unwrap_or(1.0);
            Ok(integrate_pair(factors, non_const[0], non_const[1], coeff, var))
        }
        _ => Ok(Node::Integral(Box::new(Node::Mul(factors.to_vec())), var.to_string())),
    }
}

/// A product of two factors that depend on the variable: a general power, a substitution, integration by parts,
/// or else left as an integral.
fn integrate_pair(factors: &[Node], first: &Node, second: &Node, coeff: f64, var: &str) -> Node {
    for (p, q) in [(first, second), (second, first)] {
        if let Some(antideriv) = try_general_power_antiderivative(p, q, var) {
            return scaled(coeff, antideriv);
        }
        if let Some((antideriv, remaining)) = try_u_substitution(p, q, var, coeff) {
            return scaled(remaining, antideriv);
        }
    }
    // Try Integration by Parts: ∫ u dv = u v - ∫ v du
    for (u, dv) in [(first, second), (second, first)] {
        if let Some(res) = try_integration_by_parts(u, dv, var) {
            return scaled(coeff, res);
        }
    }
    Node::Integral(Box::new(Node::Mul(factors.to_vec())), var.to_string())
}

fn try_general_power_antiderivative(p: &Node, q: &Node, var: &str) -> Option<Node> {
    if let Node::Pow(u, v) = q {
        if u.depends_on(var) && v.depends_on(var) {
            let du = u.diff_node(var).ok()?.simplify();
            let dv = v.diff_node(var).ok()?.simplify();
            
            let term1 = Node::Mul(vec![dv, Node::Ln(u.clone())]);
            let term2 = Node::Div(Box::new(Node::Mul(vec![(**v).clone(), du])), u.clone());
            let w = Node::Add(vec![term1, term2]).simplify();
            
            if p.simplify() == w {
                return Some(q.clone());
            }
        }
    }
    None
}

fn integrate_div(a: &Node, b: &Node, var: &str) -> PhysureResult<Node> {
    if !b.depends_on(var) {
        let inner = a.integrate_node(var)?;
        return Ok(Node::Div(Box::new(inner), Box::new(b.clone())));
    }
    if matches!(a, Node::Number(v) if *v == 1.0) {
        if let Some(result) = integrate_reciprocal(b, var) {
            return Ok(result);
        }
    }
    if let Some(result) = logarithmic_quotient(a, b, var) {
        return Ok(result);
    }
    Err(PhysureError::UnsupportedIntegration("Quotient integration not supported".into()))
}

/// `1 / b`: a logarithm when `b` is linear in the variable, an arctangent when it is `c + x^2`.
fn integrate_reciprocal(b: &Node, var: &str) -> Option<Node> {
    match arg_form(b, var) {
        Some(ArgForm::Var) => Some(Node::Ln(Box::new(b.clone()))),
        Some(ArgForm::Linear(coeff)) => Some(Node::Div(
            Box::new(Node::Ln(Box::new(b.clone()))),
            Box::new(Node::Number(coeff)),
        )),
        _ => integrate_arctan_reciprocal(b, var),
    }
}

/// `1 / (c + x^2)` for a positive `c`: `arctan(x / k) / k` with `k = sqrt(c)`.
fn integrate_arctan_reciprocal(b: &Node, var: &str) -> Option<Node> {
    let Node::Add(terms) = b else {
        return None;
    };
    if terms.len() != 2 {
        return None;
    }
    let mut c_val = None;
    let mut x_sq_base = None;
    for t in terms {
        if let Node::Number(c) = t {
            c_val = Some(*c);
        } else if let Node::Pow(base, exp) = t {
            if let Node::Number(2.0) = **exp {
                if let Some(ArgForm::Var) = arg_form(base, var) {
                    x_sq_base = Some((**base).clone());
                }
            }
        }
    }
    let (Some(c), Some(base_node)) = (c_val, x_sq_base) else {
        return None;
    };
    if c > 0.0 {
        let k = c.sqrt();
        if k == 1.0 {
            return Some(Node::Arctan(Box::new(base_node)));
        }
        return Some(Node::Div(
            Box::new(Node::Arctan(Box::new(Node::Div(Box::new(base_node), Box::new(Node::Number(k)))))),
            Box::new(Node::Number(k)),
        ));
    }
    None
}

/// Logarithmic Quotient Rule: ∫ g'(x)/g(x) dx = ln|g(x)|, and the same with a numeric factor.
fn logarithmic_quotient(a: &Node, b: &Node, var: &str) -> Option<Node> {
    let db_simp = b.diff_node(var).ok()?.simplify();
    let a_simp = a.simplify();
    let ln_abs = || Node::Ln(Box::new(Node::Abs(Box::new(b.clone()))));
    if db_simp == a_simp {
        return Some(ln_abs());
    }
    // Also check if a_simp / db_simp is a constant?
    // simple heuristic: if a is k*db, it will simplify to Mul(k, db)
    let Node::Mul(factors) = &a_simp else {
        return None;
    };
    if factors.len() == 2 && factors[1] == db_simp {
        if let Node::Number(k) = factors[0] {
            return Some(Node::Mul(vec![Node::Number(k), ln_abs()]));
        }
    }
    None
}

fn integrate_tan(u: &Node, var: &str) -> PhysureResult<Node> {
    let ln_sec = Node::Ln(Box::new(Node::Abs(Box::new(Node::Sec(Box::new(u.clone()))))));
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(ln_sec),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(ln_sec), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![Node::Tan(Box::new(u.clone())), Node::Symbol(var.to_string())])),
        None => Err(PhysureError::NonLinearArgument { function: "tan" }),
    }
}

fn integrate_cot(u: &Node, var: &str) -> PhysureResult<Node> {
    let ln_sin = Node::Ln(Box::new(Node::Abs(Box::new(Node::Sin(Box::new(u.clone()))))));
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(ln_sin),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(ln_sin), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![Node::Cot(Box::new(u.clone())), Node::Symbol(var.to_string())])),
        None => Err(PhysureError::NonLinearArgument { function: "cot" }),
    }
}

fn integrate_sec(u: &Node, var: &str) -> PhysureResult<Node> {
    let sec_plus_tan = Node::Add(vec![Node::Sec(Box::new(u.clone())), Node::Tan(Box::new(u.clone()))]);
    let ln_val = Node::Ln(Box::new(Node::Abs(Box::new(sec_plus_tan))));
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(ln_val),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(ln_val), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![Node::Sec(Box::new(u.clone())), Node::Symbol(var.to_string())])),
        None => Err(PhysureError::NonLinearArgument { function: "sec" }),
    }
}

fn integrate_csc(u: &Node, var: &str) -> PhysureResult<Node> {
    let csc_plus_cot = Node::Add(vec![Node::Csc(Box::new(u.clone())), Node::Cot(Box::new(u.clone()))]);
    let neg_ln = Node::Mul(vec![Node::Number(-1.0), Node::Ln(Box::new(Node::Abs(Box::new(csc_plus_cot))))]);
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(neg_ln),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(neg_ln), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![Node::Csc(Box::new(u.clone())), Node::Symbol(var.to_string())])),
        None => Err(PhysureError::NonLinearArgument { function: "csc" }),
    }
}

fn integrate_sinh(u: &Node, var: &str) -> PhysureResult<Node> {
    let cosh = Node::Cosh(Box::new(u.clone()));
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(cosh),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(cosh), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![Node::Sinh(Box::new(u.clone())), Node::Symbol(var.to_string())])),
        None => Err(PhysureError::NonLinearArgument { function: "sinh" }),
    }
}

fn integrate_cosh(u: &Node, var: &str) -> PhysureResult<Node> {
    let sinh = Node::Sinh(Box::new(u.clone()));
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(sinh),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(sinh), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![Node::Cosh(Box::new(u.clone())), Node::Symbol(var.to_string())])),
        None => Err(PhysureError::NonLinearArgument { function: "cosh" }),
    }
}

fn integrate_tanh(u: &Node, var: &str) -> PhysureResult<Node> {
    let ln_cosh = Node::Ln(Box::new(Node::Cosh(Box::new(u.clone()))));
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(ln_cosh),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(ln_cosh), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![Node::Tanh(Box::new(u.clone())), Node::Symbol(var.to_string())])),
        None => Err(PhysureError::NonLinearArgument { function: "tanh" }),
    }
}

