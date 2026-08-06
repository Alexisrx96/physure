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
    if !base.depends_on(var) && exp.depends_on(var) {
        if let Some(ArgForm::Var) = arg_form(exp, var) {
            return Ok(Node::Div(Box::new(Node::Pow(Box::new(base.clone()), Box::new(exp.clone()))), Box::new(Node::Ln(Box::new(base.clone())))));
        }
        if let Some(ArgForm::Linear(a)) = arg_form(exp, var) {
            return Ok(Node::Div(Box::new(Node::Pow(Box::new(base.clone()), Box::new(exp.clone()))), Box::new(Node::Mul(vec![Node::Number(a), Node::Ln(Box::new(base.clone()))]))));
        }
    }

    let Node::Number(n) = exp else {
        if !base.depends_on(var) && exp.depends_on(var) {
            match arg_form(exp, var) {
                Some(ArgForm::Var) => {
                    return Ok(Node::Div(
                        Box::new(Node::Pow(Box::new(base.clone()), Box::new(exp.clone()))),
                        Box::new(Node::Ln(Box::new(base.clone()))),
                    ));
                }
                Some(ArgForm::Linear(k)) => {
                    return Ok(Node::Div(
                        Box::new(Node::Pow(Box::new(base.clone()), Box::new(exp.clone()))),
                        Box::new(Node::Mul(vec![Node::Number(k), Node::Ln(Box::new(base.clone()))])),
                    ));
                }
                _ => {}
            }
        }
        return Ok(Node::Integral(Box::new(Node::Pow(Box::new(base.clone()), Box::new(exp.clone()))), var.to_string()));
    };

    if *n == 2.0 {
        if let Node::Sec(u) = base {
            match arg_form(u, var) {
                Some(ArgForm::Var) => return Ok(Node::Tan(Box::new((**u).clone()))),
                Some(ArgForm::Linear(a)) => return Ok(Node::Div(Box::new(Node::Tan(Box::new((**u).clone()))), Box::new(Node::Number(a)))),
                _ => {}
            }
        }
        if let Node::Csc(u) = base {
            match arg_form(u, var) {
                Some(ArgForm::Var) => return Ok(Node::Mul(vec![Node::Number(-1.0), Node::Cot(Box::new((**u).clone()))])),
                Some(ArgForm::Linear(a)) => return Ok(Node::Div(Box::new(Node::Mul(vec![Node::Number(-1.0), Node::Cot(Box::new((**u).clone()))])), Box::new(Node::Number(a)))),
                _ => {}
            }
        }
    } else if *n == -2.0 {
        if let Node::Cos(u) = base {
            match arg_form(u, var) {
                Some(ArgForm::Var) => return Ok(Node::Tan(Box::new((**u).clone()))),
                Some(ArgForm::Linear(a)) => return Ok(Node::Div(Box::new(Node::Tan(Box::new((**u).clone()))), Box::new(Node::Number(a)))),
                _ => {}
            }
        }
        if let Node::Sin(u) = base {
            match arg_form(u, var) {
                Some(ArgForm::Var) => return Ok(Node::Mul(vec![Node::Number(-1.0), Node::Cot(Box::new((**u).clone()))])),
                Some(ArgForm::Linear(a)) => return Ok(Node::Div(Box::new(Node::Mul(vec![Node::Number(-1.0), Node::Cot(Box::new((**u).clone()))])), Box::new(Node::Number(a)))),
                _ => {}
            }
        }
    }

    // fallback inverse trig for 1/(a^2 + x^2) and 1/sqrt(1 - x^2)
    if *n == -1.0 {
        if let Node::Add(terms) = base {
            if terms.len() == 2 {
                let mut x_term = None;
                let mut const_term = None;
                for t in terms {
                    if let Node::Pow(b, e) = t {
                        if let Node::Number(2.0) = **e {
                            if let Some(ArgForm::Var) = arg_form(b, var) {
                                x_term = Some((**b).clone());
                            } else if let Some(ArgForm::Linear(a)) = arg_form(b, var) {
                                x_term = Some(Node::Mul(vec![Node::Number(a), (**b).clone()])); // Simplified for 1/(a^2 + (bx)^2)
                            }
                        }
                    } else if let Node::Number(v) = t {
                        const_term = Some(*v);
                    } else if let Some(ArgForm::Var) = arg_form(t, var) {
                        // x term, but not x^2. Ignore.
                    }
                }
                if let (Some(x), Some(c)) = (x_term, const_term) {
                    if c > 0.0 {
                        let a = c.sqrt();
                        return Ok(Node::Mul(vec![
                            Node::Div(Box::new(Node::Number(1.0)), Box::new(Node::Number(a))),
                            Node::Arctan(Box::new(Node::Div(Box::new(x), Box::new(Node::Number(a)))))
                        ]));
                    }
                }
            }
        }
    } else if *n == -0.5 {
        if let Node::Sub(c, xsq) = base {
            if let Node::Number(c_val) = **c {
                if c_val == 1.0 {
                    if let Node::Pow(b, e) = &**xsq {
                        if let Node::Number(2.0) = **e {
                            if let Some(ArgForm::Var) = arg_form(&b, var) {
                                return Ok(Node::Arcsin(Box::new((**b).clone())));
                            }
                        }
                    }
                }
            }
        }
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
                if let Some(antideriv) = try_general_power_antiderivative(p, q, var) {
                    if coeff == 1.0 {
                        return Ok(antideriv);
                    }
                    return Ok(Node::Mul(vec![Node::Number(coeff), antideriv]));
                }
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
            Ok(Node::Integral(Box::new(Node::Mul(factors.to_vec())), var.to_string()))
        }
        _ => Ok(Node::Integral(Box::new(Node::Mul(factors.to_vec())), var.to_string())),
    }
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
        match arg_form(b, var) {
            Some(ArgForm::Var) => return Ok(Node::Ln(Box::new(b.clone()))),
            Some(ArgForm::Linear(coeff)) => {
                return Ok(Node::Div(
                    Box::new(Node::Ln(Box::new(b.clone()))),
                    Box::new(Node::Number(coeff)),
                ));
            }
            _ => {
                if let Node::Add(terms) = b {
                    if terms.len() == 2 {
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
                        if let (Some(c), Some(base_node)) = (c_val, x_sq_base) {
                            if c > 0.0 {
                                let k = c.sqrt();
                                if k == 1.0 {
                                    return Ok(Node::Arctan(Box::new(base_node)));
                                } else {
                                    return Ok(Node::Div(
                                        Box::new(Node::Arctan(Box::new(Node::Div(Box::new(base_node), Box::new(Node::Number(k)))))),
                                        Box::new(Node::Number(k)),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // Logarithmic Quotient Rule: ∫ g'(x)/g(x) dx = ln|g(x)|
    if let Ok(db) = b.diff_node(var) {
        let db_simp = db.simplify();
        let a_simp = a.simplify();
        if db_simp == a_simp {
            return Ok(Node::Ln(Box::new(Node::Abs(Box::new(b.clone())))));
        }
        
        if let Node::Mul(factors) = &a_simp {
            if factors.len() == 2 {
                if let Node::Number(k) = factors[0] {
                    if factors[1] == db_simp {
                        return Ok(Node::Mul(vec![Node::Number(k), Node::Ln(Box::new(Node::Abs(Box::new(b.clone()))))]));
                    }
                }
            }
        }
        
        // Also check if a_simp / db_simp is a constant?
        // simple heuristic: if a is k*db, it will simplify to Mul(k, db)
    }
    Err(PhysureError::UnsupportedIntegration("Quotient integration not supported".into()))
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

