use crate::error::{PhysureError, PhysureResult};
use super::ast::Node;

impl Node {
    pub fn diff_node(&self, var: &str) -> PhysureResult<Node> {
        Ok(match self {
            Node::Number(_) => Node::Number(0.0),
            Node::Symbol(s) => Node::Number(if s == var { 1.0 } else { 0.0 }),
            Node::Quantity(name, _) => Node::Number(if name == var { 1.0 } else { 0.0 }),
            Node::Add(terms) => Node::Add(
                terms
                    .iter()
                    .map(|t| t.diff_node(var))
                    .collect::<PhysureResult<Vec<_>>>()?,
            ),
            Node::Sub(a, b) => Node::Sub(Box::new(a.diff_node(var)?), Box::new(b.diff_node(var)?)),
            Node::Mul(factors) => {
                let mut sum_terms = Vec::with_capacity(factors.len());
                for i in 0..factors.len() {
                    let mut term_factors = factors.clone();
                    term_factors[i] = factors[i].diff_node(var)?;
                    sum_terms.push(Node::Mul(term_factors));
                }
                Node::Add(sum_terms)
            }
            Node::Div(a, b) => {
                let da = a.diff_node(var)?;
                let db = b.diff_node(var)?;
                let numerator = Node::Sub(
                    Box::new(Node::Mul(vec![da, (**b).clone()])),
                    Box::new(Node::Mul(vec![(**a).clone(), db])),
                );
                let denom = Node::Pow(b.clone(), Box::new(Node::Number(2.0)));
                Node::Div(Box::new(numerator), Box::new(denom))
            }
            Node::Pow(base, exp) => {
                if let Node::Number(n) = **exp {
                    let db = base.diff_node(var)?;
                    Node::Mul(vec![
                        Node::Number(n),
                        Node::Pow(base.clone(), Box::new(Node::Number(n - 1.0))),
                        db,
                    ])
                } else {
                    return Err(PhysureError::NonConstantExponent("Differentiation of non-constant exponents is not supported yet".to_string()));
                }
            }
            Node::Sin(u) => Node::Mul(vec![Node::Cos(u.clone()), u.diff_node(var)?]),
            Node::Cos(u) => Node::Mul(vec![
                Node::Number(-1.0),
                Node::Sin(u.clone()),
                u.diff_node(var)?,
            ]),
            Node::Ln(u) => Node::Div(Box::new(u.diff_node(var)?), u.clone()),
            Node::Exp(u) => Node::Mul(vec![Node::Exp(u.clone()), u.diff_node(var)?]),
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
            Node::Sin(u) | Node::Cos(u) | Node::Ln(u) | Node::Exp(u) => u.depends_on(var),
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
}
