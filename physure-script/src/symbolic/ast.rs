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
    Tan(Box<Node>),
    Cot(Box<Node>),
    Sec(Box<Node>),
    Csc(Box<Node>),
    Arcsin(Box<Node>),
    Arccos(Box<Node>),
    Arctan(Box<Node>),
    Arccot(Box<Node>),
    Arcsec(Box<Node>),
    Arccsc(Box<Node>),
    Sinh(Box<Node>),
    Cosh(Box<Node>),
    Tanh(Box<Node>),
    Coth(Box<Node>),
    Sech(Box<Node>),
    Csch(Box<Node>),
    Ln(Box<Node>),
    Exp(Box<Node>),
    Abs(Box<Node>),
    Sqrt(Box<Node>),
    Equation(Box<Node>, Box<Node>),
    Integral(Box<Node>, String),
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
            Node::Sin(u) | Node::Cos(u) | Node::Tan(u) | Node::Cot(u) | Node::Sec(u) | Node::Csc(u) |
            Node::Arcsin(u) | Node::Arccos(u) | Node::Arctan(u) | Node::Arccot(u) | Node::Arcsec(u) | Node::Arccsc(u) |
            Node::Sinh(u) | Node::Cosh(u) | Node::Tanh(u) | Node::Coth(u) | Node::Sech(u) | Node::Csch(u) |
            Node::Ln(u) | Node::Exp(u) => {
                if let Some(unit) = u.infer_unit()? {
                    if !unit.dimensions.is_empty() {
                        return Err(PhysureError::Generic("Transcendental function argument must be dimensionless".to_string()));
                    }
                }
                Ok(None)
            }
            Node::Abs(u) => u.infer_unit(),
            Node::Sqrt(u) => {
                if let Some(unit) = u.infer_unit()? {
                    Ok(Some(unit.pow(Rational64::new(1, 2))))
                } else {
                    Ok(None)
                }
            }
            Node::Equation(a, b) => {
                let ua = a.infer_unit()?;
                let ub = b.infer_unit()?;
                if let (Some(x), Some(y)) = (&ua, &ub) {
                    if x != y {
                        return Err(PhysureError::IncompatibleDimensions {
                            op: "Equation",
                            dim1: x.__repr__(),
                            dim2: y.__repr__(),
                        });
                    }
                }
                Ok(ua.or(ub))
            }
            Node::Integral(u, _) => u.infer_unit(),
        }
    }

    pub fn simplify(&self) -> Node {
        let simplified = match self {
            Node::Number(_) | Node::Symbol(_) | Node::Quantity(..) => self.clone(),
            Node::Add(terms) => simplify_add(terms.iter().map(Node::simplify).collect()),
            Node::Sub(a, b) => simplify_sub(a.simplify(), b.simplify()),
            Node::Mul(factors) => simplify_mul(factors.iter().map(Node::simplify).collect()),
            Node::Div(a, b) => simplify_div(a.simplify(), b.simplify()),
            Node::Pow(base, exp) => simplify_pow(base.simplify(), exp.simplify()),
            Node::Sin(u) => Node::Sin(Box::new(u.simplify())),
            Node::Cos(u) => Node::Cos(Box::new(u.simplify())),
            Node::Tan(u) => Node::Tan(Box::new(u.simplify())),
            Node::Cot(u) => Node::Cot(Box::new(u.simplify())),
            Node::Sec(u) => Node::Sec(Box::new(u.simplify())),
            Node::Csc(u) => Node::Csc(Box::new(u.simplify())),
            Node::Arcsin(u) => Node::Arcsin(Box::new(u.simplify())),
            Node::Arccos(u) => Node::Arccos(Box::new(u.simplify())),
            Node::Arctan(u) => Node::Arctan(Box::new(u.simplify())),
            Node::Arccot(u) => Node::Arccot(Box::new(u.simplify())),
            Node::Arcsec(u) => Node::Arcsec(Box::new(u.simplify())),
            Node::Arccsc(u) => Node::Arccsc(Box::new(u.simplify())),
            Node::Sinh(u) => Node::Sinh(Box::new(u.simplify())),
            Node::Cosh(u) => Node::Cosh(Box::new(u.simplify())),
            Node::Tanh(u) => Node::Tanh(Box::new(u.simplify())),
            Node::Coth(u) => Node::Coth(Box::new(u.simplify())),
            Node::Sech(u) => Node::Sech(Box::new(u.simplify())),
            Node::Csch(u) => Node::Csch(Box::new(u.simplify())),
            Node::Abs(u) => Node::Abs(Box::new(u.simplify())),
            Node::Sqrt(u) => Node::Sqrt(Box::new(u.simplify())),
            Node::Equation(a, b) => Node::Equation(Box::new(a.simplify()), Box::new(b.simplify())),
            Node::Integral(u, v) => Node::Integral(Box::new(u.simplify()), v.clone()),
            Node::Ln(u) => {
                let su = u.simplify();
                match su {
                    Node::Symbol(ref s) if s == "e" => Node::Number(1.0),
                    Node::Number(n) if n == 1.0 => Node::Number(0.0),
                    Node::Exp(inner) => *inner,
                    _ => Node::Ln(Box::new(su)),
                }
            },
            Node::Exp(u) => {
                let su = u.simplify();
                match su {
                    Node::Number(n) if n == 0.0 => Node::Number(1.0),
                    Node::Ln(inner) => *inner,
                    _ => Node::Exp(Box::new(su)),
                }
            },
        };
        fold_numeric_unary(simplified)
    }
}

fn as_number(node: &Node) -> Option<f64> {
    match node {
        Node::Number(v) => Some(*v),
        _ => None,
    }
}

/// Evaluates a unary function applied to a numeric literal (`sin(0)` -> `0`).
///
/// Only whole-number results are folded, so the closed forms stay readable:
/// `ln(2)` and `exp(1)` survive as themselves (a Taylor series about a non-zero
/// point keeps `exp(1)` coefficients, the way sympy keeps `E`) while `cos(0)`
/// collapses to `1`. Non-finite results (`ln(0)`, `sqrt(-1)`) are also left
/// symbolic, so an undefined point stays visible instead of printing `inf`/`NaN`.
fn fold_numeric_unary(node: Node) -> Node {
    let folded = match &node {
        Node::Sin(u) => as_number(u).map(f64::sin),
        Node::Cos(u) => as_number(u).map(f64::cos),
        Node::Tan(u) => as_number(u).map(f64::tan),
        Node::Cot(u) => as_number(u).map(|v| 1.0 / v.tan()),
        Node::Sec(u) => as_number(u).map(|v| 1.0 / v.cos()),
        Node::Csc(u) => as_number(u).map(|v| 1.0 / v.sin()),
        Node::Arcsin(u) => as_number(u).map(f64::asin),
        Node::Arccos(u) => as_number(u).map(f64::acos),
        Node::Arctan(u) => as_number(u).map(f64::atan),
        Node::Arccot(u) => as_number(u).map(|v| (1.0 / v).atan()),
        Node::Arcsec(u) => as_number(u).map(|v| (1.0 / v).acos()),
        Node::Arccsc(u) => as_number(u).map(|v| (1.0 / v).asin()),
        Node::Sinh(u) => as_number(u).map(f64::sinh),
        Node::Cosh(u) => as_number(u).map(f64::cosh),
        Node::Tanh(u) => as_number(u).map(f64::tanh),
        Node::Coth(u) => as_number(u).map(|v| 1.0 / v.tanh()),
        Node::Sech(u) => as_number(u).map(|v| 1.0 / v.cosh()),
        Node::Csch(u) => as_number(u).map(|v| 1.0 / v.sinh()),
        Node::Ln(u) => as_number(u).map(f64::ln),
        Node::Exp(u) => as_number(u).map(f64::exp),
        Node::Abs(u) => as_number(u).map(f64::abs),
        Node::Sqrt(u) => as_number(u).map(f64::sqrt),
        _ => None,
    };
    match folded {
        Some(v) if v.is_finite() && v.fract() == 0.0 => Node::Number(v),
        _ => node,
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
        let (coeff, base) = extract_coeff(t);
        if let Some(entry) = collected.iter_mut().find(|(n, _)| *n == base) {
            entry.1 += coeff;
        } else {
            collected.push((base, coeff));
        }
    }
    let mut out_terms: Vec<Node> = collected
        .into_iter()
        .filter(|(_, count)| *count != 0.0)
        .map(|(t, count)| {
            if count == 1.0 {
                t
            } else {
                match t {
                    Node::Mul(mut factors) => {
                        factors.insert(0, Node::Number(count));
                        Node::Mul(factors)
                    }
                    _ => Node::Mul(vec![Node::Number(count), t]),
                }
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

fn extract_coeff(t: Node) -> (f64, Node) {
    match t {
        Node::Mul(factors) => {
            let mut coeff = 1.0;
            let mut rest = Vec::new();
            for f in factors {
                if let Node::Number(n) = f {
                    coeff *= n;
                } else {
                    rest.push(f);
                }
            }
            let base = if rest.len() == 1 {
                rest.into_iter().next().unwrap()
            } else if rest.is_empty() {
                Node::Number(1.0)
            } else {
                Node::Mul(rest)
            };
            (coeff, base)
        }
        _ => (1.0, t),
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

// Recursively decomposes a product into a constant coefficient plus a multiset of
// (base, signed exponent) pairs, so a `Div` denominator can cancel against a factor
// appearing elsewhere in the same product (e.g. `(V / I) * I` -> `V`).
fn collect_mul_powers(
    nodes: Vec<Node>,
    exponent: f64,
    const_prod: &mut f64,
    powers: &mut Vec<(Node, f64)>,
) {
    for node in nodes {
        match node {
            Node::Number(v) => {
                if v != 0.0 {
                    *const_prod *= v.powf(exponent);
                } else if exponent > 0.0 {
                    *const_prod = 0.0;
                }
            }
            Node::Mul(inner) => collect_mul_powers(flatten_mul(inner), exponent, const_prod, powers),
            Node::Div(num, denom) => {
                collect_mul_powers(vec![*num], exponent, const_prod, powers);
                collect_mul_powers(vec![*denom], -exponent, const_prod, powers);
            }
            Node::Pow(base, exp) => {
                if let Node::Number(e) = *exp {
                    collect_mul_powers(vec![*base], exponent * e, const_prod, powers);
                } else {
                    add_power(Node::Pow(base, exp), exponent, powers);
                }
            }
            other => add_power(other, exponent, powers),
        }
    }
}

fn add_power(node: Node, exp: f64, powers: &mut Vec<(Node, f64)>) {
    if let Some(entry) = powers.iter_mut().find(|(n, _)| *n == node) {
        entry.1 += exp;
    } else {
        powers.push((node, exp));
    }
}

fn reconstruct_from_powers(const_prod: f64, powers: Vec<(Node, f64)>) -> Node {
    if const_prod == 0.0 {
        return Node::Number(0.0);
    }
    let mut numerator: Vec<Node> = Vec::new();
    let mut denominator: Vec<Node> = Vec::new();
    for (f, exp) in powers {
        if exp == 0.0 {
            continue;
        }
        let dest = if exp > 0.0 {
            &mut numerator
        } else {
            &mut denominator
        };
        let abs_exp = exp.abs();
        dest.push(if abs_exp == 1.0 {
            f
        } else {
            Node::Pow(Box::new(f), Box::new(Node::Number(abs_exp)))
        });
    }

    let mut const_prod_num = const_prod;
    let mut extra_denom: Option<f64> = None;

    if const_prod != 0.0 && const_prod.abs() < 1.0 {
        let recip = (1.0 / const_prod.abs()).round();
        if (1.0 / const_prod.abs() - recip).abs() < 1e-9 && recip > 1.0 {
            extra_denom = Some(recip);
            const_prod_num = if const_prod < 0.0 { -1.0 } else { 1.0 };
        }
    }

    if const_prod_num != 1.0 || numerator.is_empty() {
        numerator.push(Node::Number(const_prod_num));
    }
    if let Some(d) = extra_denom {
        denominator.push(Node::Number(d));
    }
    numerator.sort_by_key(sort_key);
    let num_node = if numerator.len() == 1 {
        numerator.into_iter().next().unwrap()
    } else {
        Node::Mul(numerator)
    };

    if denominator.is_empty() {
        return num_node;
    }
    denominator.sort_by_key(sort_key);
    let denom_node = if denominator.len() == 1 {
        denominator.into_iter().next().unwrap()
    } else {
        Node::Mul(denominator)
    };
    Node::Div(Box::new(num_node), Box::new(denom_node))
}

fn simplify_mul(factors: Vec<Node>) -> Node {
    let flat = flatten_mul(factors);
    let mut const_prod = 1.0;
    let mut powers: Vec<(Node, f64)> = Vec::new();
    collect_mul_powers(flat, 1.0, &mut const_prod, &mut powers);
    reconstruct_from_powers(const_prod, powers)
}

fn simplify_div(a: Node, b: Node) -> Node {
    if a == b {
        return Node::Number(1.0);
    }
    if let (Node::Number(x), Node::Number(y)) = (&a, &b) {
        if *y != 0.0 {
            return Node::Number(x / y);
        }
    }
    let mut const_prod = 1.0;
    let mut powers: Vec<(Node, f64)> = Vec::new();
    collect_mul_powers(vec![a], 1.0, &mut const_prod, &mut powers);
    collect_mul_powers(vec![b], -1.0, &mut const_prod, &mut powers);
    reconstruct_from_powers(const_prod, powers)
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
