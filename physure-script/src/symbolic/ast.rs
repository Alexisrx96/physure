use physure_core::units::RationalUnit;
use physure_core::error::{PhysureError, PhysureResult};
use num_rational::Rational64;
use num_traits::FromPrimitive;

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Number(f64),
    Symbol(String),
    Quantity(String, RationalUnit),
    Add(Vec<Node>),
    Mul(Vec<Node>),
    Sub(Box<Node>, Box<Node>),
    Div(Box<Node>, Box<Node>),
    Pow(Box<Node>, Box<Node>),
    Sin(Box<Node>),
    Cos(Box<Node>),
    Ln(Box<Node>),
    Exp(Box<Node>),
}

impl Node {
    pub fn infer_unit(&self) -> PhysureResult<Option<RationalUnit>> {
        match self {
            Node::Number(_) => Ok(Some(RationalUnit::dimensionless())),
            Node::Symbol(_) => Ok(None),
            Node::Quantity(_, u) => Ok(Some(u.clone())),
            Node::Add(terms) => {
                let mut result: Option<RationalUnit> = None;
                for t in terms {
                    if let Some(u) = t.infer_unit()? {
                        match &result {
                            None => result = Some(u),
                            Some(existing) if *existing != u => {
                                return Err(PhysureError::IncompatibleDimensions {
                                    op: "Add",
                                    dim1: existing.__repr__(),
                                    dim2: u.__repr__(),
                                });
                            }
                            _ => {}
                        }
                    }
                }
                Ok(result)
            }
            Node::Sub(a, b) => Node::Add(vec![(**a).clone(), (**b).clone()]).infer_unit(),
            Node::Mul(factors) => {
                let mut acc: Option<RationalUnit> = None;
                for f in factors {
                    if let Some(u) = f.infer_unit()? {
                        acc = Some(match acc {
                            Some(a) => a.mul(&u),
                            None => u,
                        });
                    }
                }
                Ok(acc)
            }
            Node::Div(a, b) => {
                let ua = a.infer_unit()?;
                let ub = b.infer_unit()?;
                Ok(match (ua, ub) {
                    (Some(a), Some(b)) => Some(a.div(&b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => {
                        Some(RationalUnit::dimensionless().div(&b))
                    }
                    (None, None) => None,
                })
            }
            Node::Pow(base, exp) => match base.infer_unit()? {
                None => Ok(None),
                Some(u) => {
                    if let Node::Number(n) = **exp {
                        let r = Rational64::from_f64(n).unwrap_or(Rational64::new(0, 1));
                        Ok(Some(u.pow(r)))
                    } else {
                        Err(PhysureError::NonConstantExponent("Cannot raise a dimensioned quantity to a non-constant power".to_string()))
                    }
                }
            },
            Node::Sin(u) | Node::Cos(u) | Node::Ln(u) | Node::Exp(u) => {
                if let Some(unit) = u.infer_unit()? {
                    if !unit.dimensions.is_empty() {
                        return Err(PhysureError::Generic("Transcendental function argument must be dimensionless".to_string()));
                    }
                }
                Ok(None)
            }
        }
    }

    pub fn simplify(&self) -> Node {
        match self {
            Node::Number(_) | Node::Symbol(_) | Node::Quantity(..) => self.clone(),
            Node::Add(terms) => simplify_add(terms.iter().map(Node::simplify).collect()),
            Node::Sub(a, b) => simplify_sub(a.simplify(), b.simplify()),
            Node::Mul(factors) => simplify_mul(factors.iter().map(Node::simplify).collect()),
            Node::Div(a, b) => simplify_div(a.simplify(), b.simplify()),
            Node::Pow(base, exp) => simplify_pow(base.simplify(), exp.simplify()),
            Node::Sin(u) => Node::Sin(Box::new(u.simplify())),
            Node::Cos(u) => Node::Cos(Box::new(u.simplify())),
            Node::Ln(u) => Node::Ln(Box::new(u.simplify())),
            Node::Exp(u) => Node::Exp(Box::new(u.simplify())),
        }
    }
}

pub fn flatten_add(terms: Vec<Node>) -> Vec<Node> {
    let mut out = Vec::new();
    for t in terms {
        if let Node::Add(inner) = t {
            out.extend(flatten_add(inner));
        } else {
            out.push(t);
        }
    }
    out
}

pub fn flatten_mul(factors: Vec<Node>) -> Vec<Node> {
    let mut out = Vec::new();
    for f in factors {
        if let Node::Mul(inner) = f {
            out.extend(flatten_mul(inner));
        } else {
            out.push(f);
        }
    }
    out
}

fn sort_key(n: &Node) -> String {
    format!("{n:?}")
}

fn simplify_add(terms: Vec<Node>) -> Node {
    let flat = flatten_add(terms);
    let mut const_sum = 0.0;
    let mut rest: Vec<Node> = Vec::new();
    for t in flat {
        match t {
            Node::Number(n) => const_sum += n,
            other => rest.push(other),
        }
    }
    let mut collected: Vec<(Node, f64)> = Vec::new();
    for t in rest {
        if let Some(entry) = collected.iter_mut().find(|(n, _)| *n == t) {
            entry.1 += 1.0;
        } else {
            collected.push((t, 1.0));
        }
    }
    let mut out_terms: Vec<Node> = collected
        .into_iter()
        .map(|(t, count)| {
            if count == 1.0 {
                t
            } else {
                Node::Mul(vec![Node::Number(count), t])
            }
        })
        .collect();
    if const_sum != 0.0 || out_terms.is_empty() {
        out_terms.push(Node::Number(const_sum));
    }
    out_terms.sort_by_key(sort_key);
    if out_terms.len() == 1 {
        out_terms.into_iter().next().unwrap()
    } else {
        Node::Add(out_terms)
    }
}

fn simplify_sub(a: Node, b: Node) -> Node {
    if a == b {
        return Node::Number(0.0);
    }
    if let Node::Number(0.0) = b {
        return a;
    }
    if let (Node::Number(x), Node::Number(y)) = (&a, &b) {
        return Node::Number(x - y);
    }
    Node::Sub(Box::new(a), Box::new(b))
}

fn simplify_mul(factors: Vec<Node>) -> Node {
    let flat = flatten_mul(factors);
    let mut const_prod = 1.0;
    let mut rest: Vec<Node> = Vec::new();
    for f in flat {
        match f {
            Node::Number(n) => const_prod *= n,
            other => rest.push(other),
        }
    }
    if const_prod == 0.0 {
        return Node::Number(0.0);
    }
    let mut collected: Vec<(Node, f64)> = Vec::new();
    for f in rest {
        let f_clean = if let Node::Div(num, denom) = f {
            if let Node::Number(d) = *denom {
                const_prod /= d;
                *num
            } else {
                Node::Div(num, denom)
            }
        } else {
            f
        };

        if let Some(entry) = collected.iter_mut().find(|(n, _)| *n == f_clean) {
            entry.1 += 1.0;
        } else {
            collected.push((f_clean, 1.0));
        }
    }
    let mut out_factors: Vec<Node> = collected
        .into_iter()
        .map(|(f, count)| {
            if count == 1.0 {
                f
            } else {
                Node::Pow(Box::new(f), Box::new(Node::Number(count)))
            }
        })
        .collect();
    if const_prod != 1.0 || out_factors.is_empty() {
        out_factors.push(Node::Number(const_prod));
    }
    out_factors.sort_by_key(sort_key);
    if out_factors.len() == 1 {
        out_factors.into_iter().next().unwrap()
    } else {
        Node::Mul(out_factors)
    }
}

fn simplify_div(a: Node, b: Node) -> Node {
    if a == b {
        return Node::Number(1.0);
    }
    if let Node::Number(1.0) = b {
        return a;
    }
    if let (Node::Number(x), Node::Number(y)) = (&a, &b) {
        if *y != 0.0 {
            return Node::Number(x / y);
        }
    }
    Node::Div(Box::new(a), Box::new(b))
}

fn simplify_pow(base: Node, exp: Node) -> Node {
    match (&base, &exp) {
        (Node::Number(b), Node::Number(e)) => Node::Number(b.powf(*e)),
        (_, Node::Number(e)) if *e == 1.0 => base,
        (_, Node::Number(e)) if *e == 0.0 => Node::Number(1.0),
        (Node::Number(b), _) if *b == 1.0 => Node::Number(1.0),
        _ => Node::Pow(Box::new(base), Box::new(exp)),
    }
}

pub fn check_add_compat(a: &Node, b: &Node) -> PhysureResult<()> {
    let ua = a.infer_unit()?;
    let ub = b.infer_unit()?;
    if let (Some(x), Some(y)) = (&ua, &ub) {
        if x != y {
            return Err(PhysureError::IncompatibleDimensions {
                op: "Add",
                dim1: x.__repr__(),
                dim2: y.__repr__(),
            });
        }
    }
    Ok(())
}
