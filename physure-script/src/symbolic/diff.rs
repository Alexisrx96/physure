use physure_core::error::PhysureResult;
use super::ast::Node;

impl Node {
    pub fn diff_node(&self, var: &str) -> PhysureResult<Node> {
        self.diff_node_implicit(var, None)
    }

    pub fn diff_node_implicit(&self, var: &str, dep_var: Option<&str>) -> PhysureResult<Node> {
        Ok(match self {
            Node::Number(_) => Node::Number(0.0),
            Node::Symbol(s) => {
                if s == var {
                    Node::Number(1.0)
                } else if let Some(d) = dep_var {
                    if s == d || (s.starts_with(d) && s[d.len()..].chars().all(|c| c == '\'')) {
                        Node::Symbol(format!("{}'", s))
                    } else if s == &format!("d{}/d{}", d, var) || s == &format!("d({})/d({})", d, var) {
                        Node::Symbol(format!("d^2{}/d{}^2", d, var))
                    } else {
                        Node::Number(0.0)
                    }
                } else if s.ends_with('\'') && s.chars().any(|c| c.is_alphanumeric() && c != '\'') {
                    Node::Symbol(format!("{}'", s))
                } else {
                    Node::Number(0.0)
                }
            }
            Node::Quantity(name, _) => {
                if name == var {
                    Node::Number(1.0)
                } else if let Some(d) = dep_var {
                    if name == d || (name.starts_with(d) && name[d.len()..].chars().all(|c| c == '\'')) {
                        Node::Symbol(format!("{}'", name))
                    } else {
                        Node::Number(0.0)
                    }
                } else if name.ends_with('\'') && name.chars().any(|c| c.is_alphanumeric() && c != '\'') {
                    Node::Symbol(format!("{}'", name))
                } else {
                    Node::Number(0.0)
                }
            }
            Node::Add(terms) => Node::Add(
                terms
                    .iter()
                    .map(|t| t.diff_node_implicit(var, dep_var))
                    .collect::<PhysureResult<Vec<_>>>()?,
            ),
            Node::Sub(a, b) => Node::Sub(
                Box::new(a.diff_node_implicit(var, dep_var)?),
                Box::new(b.diff_node_implicit(var, dep_var)?),
            ),
            Node::Mul(factors) => {
                let mut sum_terms = Vec::with_capacity(factors.len());
                for i in 0..factors.len() {
                    let mut term_factors = factors.clone();
                    term_factors[i] = factors[i].diff_node_implicit(var, dep_var)?;
                    sum_terms.push(Node::Mul(term_factors));
                }
                Node::Add(sum_terms)
            }
            Node::Div(a, b) => {
                let da = a.diff_node_implicit(var, dep_var)?;
                let db = b.diff_node_implicit(var, dep_var)?;
                let numerator = Node::Sub(
                    Box::new(Node::Mul(vec![da, (**b).clone()])),
                    Box::new(Node::Mul(vec![(**a).clone(), db])),
                );
                let denom = Node::Pow(b.clone(), Box::new(Node::Number(2.0)));
                Node::Div(Box::new(numerator), Box::new(denom))
            }
            Node::Pow(base, exp) => {
                let db = base.diff_node_implicit(var, dep_var)?;
                let dv = exp.diff_node_implicit(var, dep_var)?;
                let exp_dep = exp.depends_on(var) || dep_var.map_or(false, |d| exp.depends_on(d));
                if let Node::Number(n) = **exp {
                    Node::Mul(vec![
                        Node::Number(n),
                        Node::Pow(base.clone(), Box::new(Node::Number(n - 1.0))),
                        db,
                    ])
                } else if !exp_dep {
                    Node::Mul(vec![
                        (**exp).clone(),
                        Node::Pow(base.clone(), Box::new(Node::Sub(exp.clone(), Box::new(Node::Number(1.0))))),
                        db,
                    ])
                } else {
                    let term1 = Node::Mul(vec![dv, Node::Ln(base.clone())]);
                    let term2 = Node::Div(Box::new(Node::Mul(vec![(**exp).clone(), db])), base.clone());
                    Node::Mul(vec![
                        self.clone(),
                        Node::Add(vec![term1, term2]),
                    ])
                }
            }
            Node::Sin(u) => Node::Mul(vec![Node::Cos(u.clone()), u.diff_node_implicit(var, dep_var)?]),
            Node::Cos(u) => Node::Mul(vec![Node::Number(-1.0), Node::Sin(u.clone()), u.diff_node_implicit(var, dep_var)?]),
            Node::Tan(u) => Node::Mul(vec![Node::Pow(Box::new(Node::Sec(u.clone())), Box::new(Node::Number(2.0))), u.diff_node_implicit(var, dep_var)?]),
            Node::Cot(u) => Node::Mul(vec![Node::Number(-1.0), Node::Pow(Box::new(Node::Csc(u.clone())), Box::new(Node::Number(2.0))), u.diff_node_implicit(var, dep_var)?]),
            Node::Sec(u) => Node::Mul(vec![Node::Sec(u.clone()), Node::Tan(u.clone()), u.diff_node_implicit(var, dep_var)?]),
            Node::Csc(u) => Node::Mul(vec![Node::Number(-1.0), Node::Csc(u.clone()), Node::Cot(u.clone()), u.diff_node_implicit(var, dep_var)?]),
            Node::Arcsin(u) => Node::Div(Box::new(u.diff_node_implicit(var, dep_var)?), Box::new(Node::Pow(Box::new(Node::Sub(Box::new(Node::Number(1.0)), Box::new(Node::Pow(u.clone(), Box::new(Node::Number(2.0)))))), Box::new(Node::Number(0.5))))),
            Node::Arccos(u) => Node::Mul(vec![Node::Number(-1.0), Node::Div(Box::new(u.diff_node_implicit(var, dep_var)?), Box::new(Node::Pow(Box::new(Node::Sub(Box::new(Node::Number(1.0)), Box::new(Node::Pow(u.clone(), Box::new(Node::Number(2.0)))))), Box::new(Node::Number(0.5)))))]),
            Node::Arctan(u) => Node::Div(Box::new(u.diff_node_implicit(var, dep_var)?), Box::new(Node::Add(vec![Node::Number(1.0), Node::Pow(u.clone(), Box::new(Node::Number(2.0)))]))),
            Node::Arccot(u) => Node::Mul(vec![Node::Number(-1.0), Node::Div(Box::new(u.diff_node_implicit(var, dep_var)?), Box::new(Node::Add(vec![Node::Number(1.0), Node::Pow(u.clone(), Box::new(Node::Number(2.0)))])))]),
            Node::Arcsec(u) => Node::Div(Box::new(u.diff_node_implicit(var, dep_var)?), Box::new(Node::Mul(vec![Node::Abs(u.clone()), Node::Pow(Box::new(Node::Sub(Box::new(Node::Pow(u.clone(), Box::new(Node::Number(2.0)))), Box::new(Node::Number(1.0)))), Box::new(Node::Number(0.5)))]))),
            Node::Arccsc(u) => Node::Mul(vec![Node::Number(-1.0), Node::Div(Box::new(u.diff_node_implicit(var, dep_var)?), Box::new(Node::Mul(vec![Node::Abs(u.clone()), Node::Pow(Box::new(Node::Sub(Box::new(Node::Pow(u.clone(), Box::new(Node::Number(2.0)))), Box::new(Node::Number(1.0)))), Box::new(Node::Number(0.5)))])))]),
            Node::Sinh(u) => Node::Mul(vec![Node::Cosh(u.clone()), u.diff_node_implicit(var, dep_var)?]),
            Node::Cosh(u) => Node::Mul(vec![Node::Sinh(u.clone()), u.diff_node_implicit(var, dep_var)?]),
            Node::Tanh(u) => Node::Mul(vec![Node::Pow(Box::new(Node::Sech(u.clone())), Box::new(Node::Number(2.0))), u.diff_node_implicit(var, dep_var)?]),
            Node::Coth(u) => Node::Mul(vec![Node::Number(-1.0), Node::Pow(Box::new(Node::Csch(u.clone())), Box::new(Node::Number(2.0))), u.diff_node_implicit(var, dep_var)?]),
            Node::Sech(u) => Node::Mul(vec![Node::Number(-1.0), Node::Sech(u.clone()), Node::Tanh(u.clone()), u.diff_node_implicit(var, dep_var)?]),
            Node::Csch(u) => Node::Mul(vec![Node::Number(-1.0), Node::Csch(u.clone()), Node::Coth(u.clone()), u.diff_node_implicit(var, dep_var)?]),
            Node::Ln(u) => Node::Div(Box::new(u.diff_node_implicit(var, dep_var)?), u.clone()),
            Node::Exp(u) => Node::Mul(vec![Node::Exp(u.clone()), u.diff_node_implicit(var, dep_var)?]),
            Node::Abs(u) => Node::Mul(vec![
                Node::Div(Box::new((**u).clone()), Box::new(Node::Abs(u.clone()))),
                u.diff_node_implicit(var, dep_var)?
            ]),
            Node::Sqrt(u) => Node::Mul(vec![
                Node::Number(0.5),
                Node::Pow(u.clone(), Box::new(Node::Number(-0.5))),
                u.diff_node_implicit(var, dep_var)?
            ]),
            Node::Equation(a, b) => {
                let is_known_fn = |s: &str| -> bool {
                    matches!(
                        s,
                        "sin" | "cos" | "tan" | "cot" | "sec" | "csc" | "cosec"
                            | "asin" | "arcsin" | "acos" | "arccos" | "atan" | "arctan"
                            | "acot" | "arccot" | "asec" | "arcsec" | "acsc" | "arccsc"
                            | "arccosec" | "sinh" | "cosh" | "tanh" | "coth" | "sech"
                            | "csch" | "abs" | "sqrt" | "ln" | "exp" | "log" | "e" | "pi"
                    )
                };
                let mut symbols = std::collections::HashSet::new();
                self.free_symbols(&mut symbols);
                let mut dep_vars: Vec<_> = symbols
                    .into_iter()
                    .filter(|s| s != var && !is_known_fn(s))
                    .collect();
                dep_vars.sort();
                if let Some(dep_var) = dep_vars.first() {
                    let d_a = a.diff_node_implicit(var, Some(dep_var))?.simplify();
                    let d_b = b.diff_node_implicit(var, Some(dep_var))?.simplify();
                    let diff_eq = Node::Equation(Box::new(d_a), Box::new(d_b));
                    let target_deriv = format!("{}'", dep_var);
                    let solved_res = diff_eq.solve_equation(&target_deriv);
                    if let Ok(ref solved) = solved_res {
                        Node::Equation(Box::new(Node::Symbol(target_deriv)), Box::new(solved.clone()))
                    } else {
                        diff_eq.simplify()
                    }
                } else {
                    Node::Equation(
                        Box::new(a.diff_node_implicit(var, dep_var)?),
                        Box::new(b.diff_node_implicit(var, dep_var)?),
                    )
                }
            }
            Node::Integral(u, v) => {
                if v == var {
                    (**u).clone()
                } else {
                    Node::Integral(Box::new(u.diff_node_implicit(var, dep_var)?), v.clone())
                }
            }
        })
    }

    pub fn depends_on(&self, var: &str) -> bool {
        match self {
            Node::Number(_) => false,
            Node::Symbol(s) => s == var,
            Node::Quantity(name, _) => name == var,
            Node::Add(terms) | Node::Mul(terms) => terms.iter().any(|t| t.depends_on(var)),
            Node::Sub(a, b) | Node::Div(a, b) | Node::Pow(a, b) => {
                a.depends_on(var) || b.depends_on(var)
            }
            Node::Sin(u) | Node::Cos(u) | Node::Tan(u) | Node::Cot(u) | Node::Sec(u) | Node::Csc(u) |
            Node::Arcsin(u) | Node::Arccos(u) | Node::Arctan(u) | Node::Arccot(u) | Node::Arcsec(u) | Node::Arccsc(u) |
            Node::Sinh(u) | Node::Cosh(u) | Node::Tanh(u) | Node::Coth(u) | Node::Sech(u) | Node::Csch(u) |
            Node::Ln(u) | Node::Exp(u) | Node::Abs(u) | Node::Sqrt(u) => u.depends_on(var),
            Node::Equation(a, b) => a.depends_on(var) || b.depends_on(var),
            Node::Integral(u, v) => v == var || u.depends_on(var),
        }
    }

    /// Collects the names of every free `Symbol`/`Quantity` node, used to validate
    /// that a keyword call to an `Equation` binds every variable the solved side needs.
    pub fn free_symbols(&self, out: &mut std::collections::HashSet<String>) {
        match self {
            Node::Number(_) => {}
            Node::Symbol(s) => {
                out.insert(s.clone());
            }
            Node::Quantity(name, _) => {
                out.insert(name.clone());
            }
            Node::Add(terms) | Node::Mul(terms) => terms.iter().for_each(|t| t.free_symbols(out)),
            Node::Sub(a, b) | Node::Div(a, b) | Node::Pow(a, b) => {
                a.free_symbols(out);
                b.free_symbols(out);
            }
            Node::Sin(u) | Node::Cos(u) | Node::Tan(u) | Node::Cot(u) | Node::Sec(u) | Node::Csc(u) |
            Node::Arcsin(u) | Node::Arccos(u) | Node::Arctan(u) | Node::Arccot(u) | Node::Arcsec(u) | Node::Arccsc(u) |
            Node::Sinh(u) | Node::Cosh(u) | Node::Tanh(u) | Node::Coth(u) | Node::Sech(u) | Node::Csch(u) |
            Node::Ln(u) | Node::Exp(u) | Node::Abs(u) | Node::Sqrt(u) => u.free_symbols(out),
            Node::Equation(a, b) => {
                a.free_symbols(out);
                b.free_symbols(out);
            }
            Node::Integral(u, v) => {
                u.free_symbols(out);
                out.insert(v.clone());
            }
        }
    }

    pub fn linear_coeff(&self, var: &str) -> Option<(f64, f64)> {
        match self {
            Node::Number(c) => Some((0.0, *c)),
            Node::Symbol(s) if s == var => Some((1.0, 0.0)),
            Node::Quantity(name, _) if name == var => Some((1.0, 0.0)),
            Node::Symbol(_) | Node::Quantity(..) => None,
            Node::Add(terms) => terms.iter().try_fold((0.0, 0.0), |(a, b), t| {
                let (ta, tb) = t.linear_coeff(var)?;
                Some((a + ta, b + tb))
            }),
            Node::Sub(x, y) => {
                let (xa, xb) = x.linear_coeff(var)?;
                let (ya, yb) = y.linear_coeff(var)?;
                Some((xa - ya, xb - yb))
            }
            Node::Mul(factors) => {
                let mut coeff = 1.0;
                let mut lin: Option<(f64, f64)> = None;
                for f in factors {
                    if f.depends_on(var) {
                        if lin.is_some() {
                            return None;
                        }
                        lin = Some(f.linear_coeff(var)?);
                    } else if let Node::Number(c) = f {
                        coeff *= c;
                    } else {
                        return None;
                    }
                }
                let (la, lb) = lin.unwrap_or((0.0, 1.0));
                Some((coeff * la, coeff * lb))
            }
            _ => None,
        }
    }

    pub fn diff_node_n(&self, var: &str, n: usize) -> PhysureResult<Node> {
        let mut cur = self.clone();
        for _ in 0..n {
            cur = cur.diff_node(var)?.simplify();
        }
        Ok(cur)
    }
}
