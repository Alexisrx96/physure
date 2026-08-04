use physure_core::error::{PhysureError, PhysureResult};
use super::ast::Node;
use super::parser::SymbolicParser;

/// Computes the forward Laplace transform L{f(t)} = F(s).
pub fn laplace_str(f_str: &str, t_var: &str, s_var: &str) -> PhysureResult<String> {
    let node = SymbolicParser::parse_str(f_str)?;
    let res = laplace(&node, t_var, s_var)?;
    Ok(res.to_phs_string())
}

/// Computes the inverse Laplace transform L^-1{F(s)} = f(t).
pub fn inv_laplace_str(f_str: &str, s_var: &str, t_var: &str) -> PhysureResult<String> {
    let node = SymbolicParser::parse_str(f_str)?;
    let res = inv_laplace(&node, s_var, t_var)?;
    Ok(res.to_phs_string())
}

pub fn laplace(node: &Node, t_var: &str, s_var: &str) -> PhysureResult<Node> {
    let s = Node::Symbol(s_var.to_string());
    let t = Node::Symbol(t_var.to_string());

    match node.simplify() {
        // L{c} = c / s
        Node::Number(c) => Ok(Node::Div(Box::new(Node::Number(c)), Box::new(s)).simplify()),

        // L{t} = 1 / s^2
        Node::Symbol(ref sym) if sym == t_var => Ok(Node::Div(
            Box::new(Node::Number(1.0)),
            Box::new(Node::Pow(Box::new(s), Box::new(Node::Number(2.0)))),
        ).simplify()),

        // Constant symbol not depending on t_var: L{a} = a / s
        Node::Symbol(ref sym) if sym != t_var => Ok(Node::Div(
            Box::new(Node::Symbol(sym.clone())),
            Box::new(s),
        ).simplify()),

        // L{a + b} = L{a} + L{b}
        Node::Add(terms) => {
            let mut l_terms = Vec::new();
            for term in terms {
                l_terms.push(laplace(&term, t_var, s_var)?);
            }
            Ok(Node::Add(l_terms).simplify())
        }

        // L{c * f(t)} = c * L{f(t)}
        Node::Mul(factors) => {
            let (t_factors, const_factors): (Vec<_>, Vec<_>) =
                factors.iter().cloned().partition(|f| f.depends_on(t_var));

            let const_node = if const_factors.is_empty() {
                Node::Number(1.0)
            } else if const_factors.len() == 1 {
                const_factors[0].clone()
            } else {
                Node::Mul(const_factors).simplify()
            };

            let t_node = if t_factors.is_empty() {
                Node::Number(1.0)
            } else if t_factors.len() == 1 {
                t_factors[0].clone()
            } else {
                Node::Mul(t_factors).simplify()
            };

            let l_t = laplace(&t_node, t_var, s_var)?;
            Ok(Node::Mul(vec![const_node, l_t]).simplify())
        }

        // L{t^n} = n! / s^(n+1)
        Node::Pow(ref base, ref exp) if **base == t => {
            if let Node::Number(n) = **exp {
                if n >= 0.0 && n.fract() == 0.0 {
                    let fact = factorial(n as u64) as f64;
                    let s_pow = Node::Pow(Box::new(s), Box::new(Node::Number(n + 1.0)));
                    return Ok(Node::Div(Box::new(Node::Number(fact)), Box::new(s_pow)).simplify());
                }
            }
            Err(PhysureError::Generic("Unsupported exponent for t^n in laplace".into()))
        }

        // L{exp(a * t)} = 1 / (s - a)
        Node::Exp(ref inner) => {
            if let Some(a) = extract_linear_coeff(inner, t_var) {
                let den = Node::Sub(Box::new(s), Box::new(a)).simplify();
                return Ok(Node::Div(Box::new(Node::Number(1.0)), Box::new(den)).simplify());
            }
            Err(PhysureError::Generic("Unsupported exponent in exp for laplace".into()))
        }

        // L{sin(w * t)} = w / (s^2 + w^2)
        Node::Sin(ref inner) => {
            if let Some(w) = extract_linear_coeff(inner, t_var) {
                let w_sq = Node::Mul(vec![w.clone(), w.clone()]).simplify();
                let s_sq = Node::Pow(Box::new(s), Box::new(Node::Number(2.0)));
                let den = Node::Add(vec![s_sq, w_sq]).simplify();
                return Ok(Node::Div(Box::new(w), Box::new(den)).simplify());
            }
            Err(PhysureError::Generic("Unsupported inner arg in sin for laplace".into()))
        }

        // L{cos(w * t)} = s / (s^2 + w^2)
        Node::Cos(ref inner) => {
            if let Some(w) = extract_linear_coeff(inner, t_var) {
                let w_sq = Node::Mul(vec![w.clone(), w.clone()]).simplify();
                let s_sq = Node::Pow(Box::new(s.clone()), Box::new(Node::Number(2.0)));
                let den = Node::Add(vec![s_sq, w_sq]).simplify();
                return Ok(Node::Div(Box::new(s.clone()), Box::new(den)).simplify());
            }
            Err(PhysureError::Generic("Unsupported inner arg in cos for laplace".into()))
        }

        // L{sinh(a * t)} = a / (s^2 - a^2)
        Node::Sinh(ref inner) => {
            if let Some(a) = extract_linear_coeff(inner, t_var) {
                let a_sq = Node::Mul(vec![a.clone(), a.clone()]).simplify();
                let s_sq = Node::Pow(Box::new(s.clone()), Box::new(Node::Number(2.0)));
                let den = Node::Sub(Box::new(s_sq), Box::new(a_sq)).simplify();
                return Ok(Node::Div(Box::new(a), Box::new(den)).simplify());
            }
            Err(PhysureError::Generic("Unsupported inner arg in sinh for laplace".into()))
        }

        // L{cosh(a * t)} = s / (s^2 - a^2)
        Node::Cosh(ref inner) => {
            if let Some(a) = extract_linear_coeff(inner, t_var) {
                let a_sq = Node::Mul(vec![a.clone(), a.clone()]).simplify();
                let s_sq = Node::Pow(Box::new(s.clone()), Box::new(Node::Number(2.0)));
                let den = Node::Sub(Box::new(s_sq), Box::new(a_sq)).simplify();
                return Ok(Node::Div(Box::new(s.clone()), Box::new(den)).simplify());
            }
            Err(PhysureError::Generic("Unsupported inner arg in cosh for laplace".into()))
        }

        other => Err(PhysureError::Generic(format!(
            "Laplace transform not implemented for expression '{}'",
            other.to_phs_string()
        ))),
    }
}

pub fn inv_laplace(node: &Node, s_var: &str, t_var: &str) -> PhysureResult<Node> {
    let t = Node::Symbol(t_var.to_string());
    let s = Node::Symbol(s_var.to_string());

    match node.simplify() {
        // L^-1{c / s} = c
        Node::Div(ref num, ref den) if **den == s => Ok((**num).clone()),

        // L^-1{1 / s^2} = t
        Node::Div(ref num, ref den) if matches!(**den, Node::Pow(ref b, ref e) if **b == s && **e == Node::Number(2.0)) && **num == Node::Number(1.0) => {
            Ok(t)
        }

        // L^-1{1 / (s - a)} = exp(a * t)
        Node::Div(ref num, ref den) if matches!(**den, Node::Sub(ref a, _) if **a == s) && **num == Node::Number(1.0) => {
            if let Node::Sub(ref a_node, ref b_node) = **den {
                if **a_node == s {
                    let a = (**b_node).clone();
                    let at = Node::Mul(vec![a, t.clone()]).simplify();
                    return Ok(Node::Exp(Box::new(at)));
                }
            }
            Err(PhysureError::Generic("Failed to invert Laplace pattern".into()))
        }

        // L^-1{w / (s^2 + w^2)} = sin(w * t)
        Node::Div(ref num, ref den) if matches!(**den, Node::Add(_)) => {
            if let Node::Add(ref ts) = **den {
                if ts.len() == 2 {
                    let (s_term, w_term) = if ts[0].to_phs_string().contains(s_var) {
                        (&ts[0], &ts[1])
                    } else {
                        (&ts[1], &ts[0])
                    };

                    if matches!(s_term, Node::Pow(ref b, ref e) if **b == s && **e == Node::Number(2.0)) {
                        // w^2 = w_term
                        let w = match w_term {
                            Node::Number(val) => Node::Number(val.sqrt()),
                            Node::Pow(b, e) if **e == Node::Number(2.0) => (**b).clone(),
                            other => Node::Sqrt(Box::new(other.clone())),
                        };

                        if **num == s {
                            // L^-1{s / (s^2 + w^2)} = cos(w * t)
                            let wt = Node::Mul(vec![w, t.clone()]).simplify();
                            return Ok(Node::Cos(Box::new(wt)));
                        } else if **num == w {
                            // L^-1{w / (s^2 + w^2)} = sin(w * t)
                            let wt = Node::Mul(vec![w, t.clone()]).simplify();
                            return Ok(Node::Sin(Box::new(wt)));
                        } else if **num == Node::Number(1.0) {
                            // 1 / (s^2 + w^2) = (1/w) * sin(w * t)
                            let wt = Node::Mul(vec![w.clone(), t.clone()]).simplify();
                            let sin_wt = Node::Sin(Box::new(wt));
                            return Ok(Node::Div(Box::new(sin_wt), Box::new(w)).simplify());
                        }
                    }
                }
            }
            Err(PhysureError::Generic("Failed to invert Laplace fraction".into()))
        }

        // Linear sum
        Node::Add(terms) => {
            let mut inv_terms = Vec::new();
            for term in terms {
                inv_terms.push(inv_laplace(&term, s_var, t_var)?);
            }
            Ok(Node::Add(inv_terms).simplify())
        }

        other => Err(PhysureError::Generic(format!(
            "Inverse Laplace transform not implemented for expression '{}'",
            other.to_phs_string()
        ))),
    }
}

fn extract_linear_coeff(node: &Node, var: &str) -> Option<Node> {
    match node.simplify() {
        Node::Symbol(s) if s == var => Some(Node::Number(1.0)),
        Node::Mul(factors) => {
            let (target, other): (Vec<_>, Vec<_>) = factors.iter().cloned().partition(|f| f.to_phs_string() == var);
            if target.len() == 1 {
                if other.is_empty() {
                    Some(Node::Number(1.0))
                } else if other.len() == 1 {
                    Some(other[0].clone())
                } else {
                    Some(Node::Mul(other).simplify())
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

fn factorial(n: u64) -> u64 {
    (1..=n).product()
}
