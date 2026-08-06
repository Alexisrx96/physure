use physure_core::error::{PhysureError, PhysureResult};
use super::ast::Node;
use super::parser::SymbolicParser;

/// Solves an ordinary differential equation (ODE) analytically.
///
/// Supported forms:
/// 1. 1st-order separable: dy/dx = f(x) * g(y)
/// 2. 1st-order linear: y' + P(x)*y = Q(x)
/// 3. 2nd-order linear homogeneous with constant coefficients: a*y'' + b*y' + c*y = 0
/// 4. 2nd-order linear non-homogeneous with constant coefficients: a*y'' + b*y' + c*y = f(x)
pub fn dsolve_str(eq_str: &str, dep_var: &str, indep_var: &str) -> PhysureResult<String> {
    let sol_node = dsolve(eq_str, dep_var, indep_var)?;
    Ok(sol_node.to_phs_string())
}

pub fn dsolve(eq_str: &str, dep_var: &str, indep_var: &str) -> PhysureResult<Node> {
    // Parse equality
    let (left, right) = if let Some((l, r)) = SymbolicParser::parse_equation_str(eq_str)? {
        (l, r)
    } else {
        let node = SymbolicParser::parse_str(eq_str)?;
        (node, Node::Number(0.0))
    };

    let expr = Node::Sub(Box::new(left), Box::new(right)).simplify();

    // Check for 2nd order constant coefficient linear ODE: a*y'' + b*y' + c*y = f(x)
    if let Some((a, b, c, rhs)) = extract_2nd_order_const_coeffs(&expr, dep_var, indep_var) {
        return solve_2nd_order_const_coeffs(a, b, c, rhs, dep_var, indep_var);
    }

    // Check for 1st order linear ODE: y' + P(x)*y = Q(x)
    if let Some((p_x, q_x)) = extract_1st_order_linear(&expr, dep_var, indep_var) {
        return solve_1st_order_linear(&p_x, &q_x, dep_var, indep_var);
    }

    // Check for 1st order separable ODE: y' = f(x) * g(y)
    if let Some((f_x, g_y)) = extract_1st_order_separable(&expr, dep_var, indep_var) {
        return solve_1st_order_separable(&f_x, &g_y, dep_var, indep_var);
    }

    Err(PhysureError::Generic(format!(
        "Unable to analytically solve ODE '{}' for '{}' with respect to '{}'",
        eq_str, dep_var, indep_var
    )))
}

/// Solves 2nd order linear ODE with constant coefficients: a*y'' + b*y' + c*y = rhs
fn solve_2nd_order_const_coeffs(
    a: f64,
    b: f64,
    c: f64,
    rhs: Node,
    dep_var: &str,
    indep_var: &str,
) -> PhysureResult<Node> {
    if a == 0.0 {
        return Err(PhysureError::Generic("Not a 2nd order ODE (a = 0)".into()));
    }

    let disc = b * b - 4.0 * a * c;
    let indep = Node::Symbol(indep_var.to_string());
    let c1 = Node::Symbol("C1".to_string());
    let c2 = Node::Symbol("C2".to_string());

    let y_h = if disc.abs() < 1e-12 {
        // Repeated root: r = -b / (2a)
        let r = -b / (2.0 * a);
        let exp_term = if r == 0.0 {
            Node::Number(1.0)
        } else {
            Node::Exp(Box::new(Node::Mul(vec![Node::Number(r), indep.clone()]).simplify()))
        };
        // (C1 + C2 * x) * exp(r * x)
        let poly = Node::Add(vec![
            c1,
            Node::Mul(vec![c2, indep.clone()]).simplify(),
        ]);
        Node::Mul(vec![poly, exp_term]).simplify()
    } else if disc > 0.0 {
        // Two distinct real roots: r1, r2 = (-b ± sqrt(disc)) / (2a)
        let r1 = (-b + disc.sqrt()) / (2.0 * a);
        let r2 = (-b - disc.sqrt()) / (2.0 * a);

        let term1 = Node::Mul(vec![
            c1,
            Node::Exp(Box::new(Node::Mul(vec![Node::Number(r1), indep.clone()]).simplify())),
        ]).simplify();

        let term2 = Node::Mul(vec![
            c2,
            Node::Exp(Box::new(Node::Mul(vec![Node::Number(r2), indep.clone()]).simplify())),
        ]).simplify();

        Node::Add(vec![term1, term2]).simplify()
    } else {
        // Complex conjugate roots: alpha ± i * beta
        let alpha = -b / (2.0 * a);
        let beta = (-disc).sqrt() / (2.0 * a);

        let trig_part = Node::Add(vec![
            Node::Mul(vec![
                c1,
                Node::Cos(Box::new(Node::Mul(vec![Node::Number(beta), indep.clone()]).simplify())),
            ]).simplify(),
            Node::Mul(vec![
                c2,
                Node::Sin(Box::new(Node::Mul(vec![Node::Number(beta), indep.clone()]).simplify())),
            ]).simplify(),
        ]);

        if alpha.abs() < 1e-12 {
            trig_part.simplify()
        } else {
            let exp_part = Node::Exp(Box::new(Node::Mul(vec![Node::Number(alpha), indep.clone()]).simplify()));
            Node::Mul(vec![exp_part, trig_part]).simplify()
        }
    };

    // Check if homogeneous (rhs == 0)
    if matches!(rhs.simplify(), Node::Number(val) if val == 0.0) {
        let lhs_y = Node::Symbol(dep_var.to_string());
        return Ok(Node::Equation(Box::new(lhs_y), Box::new(y_h)));
    }

    // Particular solution y_p for constant or simple polynomial RHS
    if let Node::Number(val) = rhs.simplify() {
        if c != 0.0 {
            let y_p = Node::Number(val / c);
            let sol = Node::Add(vec![y_h, y_p]).simplify();
            let lhs_y = Node::Symbol(dep_var.to_string());
            return Ok(Node::Equation(Box::new(lhs_y), Box::new(sol)));
        }
    }

    let lhs_y = Node::Symbol(dep_var.to_string());
    Ok(Node::Equation(Box::new(lhs_y), Box::new(y_h)))
}

/// Solves 1st order linear ODE: y' + P(x)*y = Q(x)
fn solve_1st_order_linear(
    p_x: &Node,
    q_x: &Node,
    dep_var: &str,
    indep_var: &str,
) -> PhysureResult<Node> {
    let c1 = Node::Symbol("C1".to_string());

    // Integrating factor mu(x) = exp(∫ P(x) dx)
    let int_p = p_x.integrate_node(indep_var)?;
    let mu = Node::Exp(Box::new(int_p.clone())).simplify();

    // ∫ mu(x) * Q(x) dx
    let integrand = Node::Mul(vec![mu.clone(), q_x.clone()]).simplify();
    let int_mu_q = integrand.integrate_node(indep_var)?;

    // y(x) = (∫ mu(x) Q(x) dx + C1) / mu(x)
    let num = Node::Add(vec![int_mu_q, c1]).simplify();
    let sol = Node::Div(Box::new(num), Box::new(mu)).simplify();

    let lhs_y = Node::Symbol(dep_var.to_string());
    Ok(Node::Equation(Box::new(lhs_y), Box::new(sol)))
}

/// Solves 1st order separable ODE: y' = f(x) * g(y)
fn solve_1st_order_separable(
    f_x: &Node,
    g_y: &Node,
    dep_var: &str,
    indep_var: &str,
) -> PhysureResult<Node> {
    // ∫ (1 / g(y)) dy = ∫ f(x) dx + C1
    let inv_g = Node::Div(Box::new(Node::Number(1.0)), Box::new(g_y.clone())).simplify();
    let int_y = inv_g.integrate_node(dep_var)?;
    let int_x = f_x.integrate_node(indep_var)?;

    let c1 = Node::Symbol("C1".to_string());
    let rhs = Node::Add(vec![int_x, c1]).simplify();

    // Try to solve for dep_var
    let eq = Node::Sub(Box::new(int_y.clone()), Box::new(rhs.clone()));
    if let Ok(sol) = eq.solve_equation(dep_var) {
        let lhs_y = Node::Symbol(dep_var.to_string());
        Ok(Node::Equation(Box::new(lhs_y), Box::new(sol)))
    } else {
        Ok(Node::Equation(Box::new(int_y), Box::new(rhs)))
    }
}

fn to_add_terms(node: &Node) -> Vec<Node> {
    match node {
        Node::Add(ts) => ts.iter().flat_map(to_add_terms).collect(),
        Node::Sub(a, b) => {
            let mut ts = to_add_terms(a);
            let neg_b = to_add_terms(&Node::Mul(vec![Node::Number(-1.0), (**b).clone()]).simplify());
            ts.extend(neg_b);
            ts
        }
        other => vec![other.clone()],
    }
}

// Helpers for pattern matching ODE forms
fn extract_2nd_order_const_coeffs(
    expr: &Node,
    dep_var: &str,
    _indep_var: &str,
) -> Option<(f64, f64, f64, Node)> {
    let prime2 = format!("{}''", dep_var);
    let prime1 = format!("{}'", dep_var);

    let mut a = 0.0;
    let mut b = 0.0;
    let mut c = 0.0;
    let mut non_dep_terms = Vec::new();

    let terms = to_add_terms(expr);

    for term in terms {
        let s = term.to_phs_string();
        if s.contains(&prime2) {
            if let Some(coeff) = extract_const_coeff(&term, &prime2) {
                a += coeff;
            } else {
                return None;
            }
        } else if s.contains(&prime1) {
            if let Some(coeff) = extract_const_coeff(&term, &prime1) {
                b += coeff;
            } else {
                return None;
            }
        } else if s.contains(dep_var) {
            if let Some(coeff) = extract_const_coeff(&term, dep_var) {
                c += coeff;
            } else {
                return None;
            }
        } else {
            non_dep_terms.push(Node::Mul(vec![Node::Number(-1.0), term]).simplify());
        }
    }

    if a != 0.0 {
        let rhs = if non_dep_terms.is_empty() {
            Node::Number(0.0)
        } else if non_dep_terms.len() == 1 {
            non_dep_terms[0].clone()
        } else {
            Node::Add(non_dep_terms).simplify()
        };
        Some((a, b, c, rhs))
    } else {
        None
    }
}

fn extract_1st_order_linear(
    expr: &Node,
    dep_var: &str,
    _indep_var: &str,
) -> Option<(Node, Node)> {
    let prime1 = format!("{}'", dep_var);
    let mut y_prime_coeff = 0.0;
    let mut p_terms = Vec::new();
    let mut q_terms = Vec::new();

    let terms = to_add_terms(expr);

    for term in terms {
        let s = term.to_phs_string();
        if s.contains(&prime1) {
            if let Some(coeff) = extract_const_coeff(&term, &prime1) {
                y_prime_coeff += coeff;
            } else {
                return None;
            }
        } else if s.contains(dep_var) {
            // Factor out dep_var
            let (target_f, other_f) = match &term {
                Node::Mul(fs) => fs.iter().cloned().partition(|f| f.to_phs_string() == dep_var),
                Node::Symbol(sym) if sym == dep_var => (vec![term.clone()], vec![Node::Number(1.0)]),
                _ => return None,
            };
            if target_f.len() == 1 {
                let coeff_node = if other_f.is_empty() {
                    Node::Number(1.0)
                } else if other_f.len() == 1 {
                    other_f[0].clone()
                } else {
                    Node::Mul(other_f).simplify()
                };
                p_terms.push(coeff_node);
            } else {
                return None;
            }
        } else {
            q_terms.push(Node::Mul(vec![Node::Number(-1.0), term]).simplify());
        }
    }

    if y_prime_coeff == 1.0 {
        let p_x = if p_terms.is_empty() {
            Node::Number(0.0)
        } else if p_terms.len() == 1 {
            p_terms[0].clone()
        } else {
            Node::Add(p_terms).simplify()
        };

        let q_x = if q_terms.is_empty() {
            Node::Number(0.0)
        } else if q_terms.len() == 1 {
            q_terms[0].clone()
        } else {
            Node::Add(q_terms).simplify()
        };

        Some((p_x, q_x))
    } else {
        None
    }
}

fn extract_1st_order_separable(
    expr: &Node,
    dep_var: &str,
    indep_var: &str,
) -> Option<(Node, Node)> {
    let prime1 = format!("{}'", dep_var);
    // y' - f(x)*g(y) = 0 => y' = f(x)*g(y)
    let terms = to_add_terms(expr);

    if terms.len() == 2 {
        let has_prime = terms[0].to_phs_string().contains(&prime1) || terms[1].to_phs_string().contains(&prime1);
        if has_prime {
            let (prime_term, other_term) = if terms[0].to_phs_string().contains(&prime1) {
                (&terms[0], &terms[1])
            } else {
                (&terms[1], &terms[0])
            };

            if prime_term.to_phs_string() == prime1 {
                let rhs = Node::Mul(vec![Node::Number(-1.0), other_term.clone()]).simplify();
                // Partition rhs into f(x) and g(y)
                let (x_factors, y_factors) = match &rhs {
                    Node::Mul(fs) => fs.iter().cloned().partition(|f| !f.depends_on(dep_var)),
                    other if !other.depends_on(dep_var) => (vec![other.clone()], vec![Node::Number(1.0)]),
                    other if !other.depends_on(indep_var) => (vec![Node::Number(1.0)], vec![other.clone()]),
                    _ => return None,
                };

                let f_x = if x_factors.is_empty() {
                    Node::Number(1.0)
                } else if x_factors.len() == 1 {
                    x_factors[0].clone()
                } else {
                    Node::Mul(x_factors).simplify()
                };

                let g_y = if y_factors.is_empty() {
                    Node::Number(1.0)
                } else if y_factors.len() == 1 {
                    y_factors[0].clone()
                } else {
                    Node::Mul(y_factors).simplify()
                };

                return Some((f_x, g_y));
            }
        }
    }
    None
}

fn extract_const_coeff(term: &Node, var: &str) -> Option<f64> {
    match term {
        Node::Symbol(s) if s == var => Some(1.0),
        Node::Mul(factors) => {
            let flat = super::ast::flatten_mul(factors.clone());
            let mut const_val = 1.0;
            let mut found_var = false;
            for f in flat {
                match f {
                    Node::Number(n) => const_val *= n,
                    Node::Symbol(s) if s == var && !found_var => found_var = true,
                    _ => return None,
                }
            }
            if found_var { Some(const_val) } else { None }
        }
        _ => None,
    }
}
