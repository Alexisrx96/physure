use crate::units::RationalUnit;
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
    /// Infers the physical unit of the expression, raising when terms
    /// combine incompatible dimensions (§6.1).
    /// Infers the physical unit of the expression, raising when terms
    /// combine incompatible dimensions (§6.1).
    fn infer_unit(&self) -> Result<Option<RationalUnit>, String> {
        match self {
            Node::Number(_) | Node::Symbol(_) => Ok(None),
            Node::Quantity(_, u) => Ok(Some(u.clone())),
            Node::Add(terms) => {
                let mut result: Option<RationalUnit> = None;
                for t in terms {
                    if let Some(u) = t.infer_unit()? {
                        match &result {
                            None => result = Some(u),
                            Some(existing) if *existing != u => {
                                return Err(format!(
                                    "Incompatible units in Add: {:?} vs {:?}",
                                    existing.dimensions, u.dimensions
                                ));
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
                        Some(RationalUnit::new_from_dimensions(Default::default()).div(&b))
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
                        Err("Cannot raise a dimensioned quantity to a non-constant power".to_string())
                    }
                }
            },
            Node::Sin(u) | Node::Cos(u) | Node::Ln(u) | Node::Exp(u) => {
                if let Some(unit) = u.infer_unit()? {
                    if !unit.dimensions.is_empty() {
                        return Err("Transcendental function argument must be dimensionless".to_string());
                    }
                }
                Ok(None)
            }
        }
    }

    /// Symbolic differentiation (§4), with unit propagation falling out of
    /// `infer_unit` naturally since every rule below is dimensionally sound.
    fn diff_node(&self, var: &str) -> Result<Node, String> {
        Ok(match self {
            Node::Number(_) => Node::Number(0.0),
            Node::Symbol(s) => Node::Number(if s == var { 1.0 } else { 0.0 }),
            Node::Quantity(name, _) => Node::Number(if name == var { 1.0 } else { 0.0 }),
            Node::Add(terms) => Node::Add(
                terms
                    .iter()
                    .map(|t| t.diff_node(var))
                    .collect::<Result<Vec<_>, String>>()?,
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
                    return Err("Differentiation of non-constant exponents is not supported yet".to_string());
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

    /// Whether `self` mentions `var` anywhere in its subtree.
    fn depends_on(&self, var: &str) -> bool {
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

    /// Detects `a*var + b` and returns `(a, b)`; `None` if `self` isn't an
    /// affine function of `var` (used by §4.2's linear chain rule).
    fn linear_coeff(&self, var: &str) -> Option<(f64, f64)> {
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

    /// Indefinite integration (§4.2, "Level 1"): pattern-table lookup,
    /// linear chain rule, and a narrow g'(x)*F(g(x)) u-substitution.
    /// Bails with `PyNotImplementedError` outside that pattern set.
    fn integrate_node(&self, var: &str) -> Result<Node, String> {
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
                    .collect::<Result<Vec<_>, String>>()?,
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

    /// Algebraic simplification (§3.1): identity/zero/inverse laws,
    /// associativity flattening, constant folding, collecting equal terms.
    fn simplify(&self) -> Node {
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

fn flatten_add(terms: Vec<Node>) -> Vec<Node> {
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

fn flatten_mul(factors: Vec<Node>) -> Vec<Node> {
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
        if let Some(entry) = collected.iter_mut().find(|(n, _)| *n == f) {
            entry.1 += 1.0;
        } else {
            collected.push((f, 1.0));
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

fn check_add_compat(a: &Node, b: &Node) -> Result<(), String> {
    let ua = a.infer_unit()?;
    let ub = b.infer_unit()?;
    if let (Some(x), Some(y)) = (&ua, &ub) {
        if x != y {
            return Err(format!(
                "Cannot add term with unit {:?} to term with unit {:?}: incompatible dimensions.",
                x.dimensions, y.dimensions
            ));
        }
    }
    Ok(())
}

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

fn integrate_sin(u: &Node, var: &str) -> Result<Node, String> {
    let neg_cos = Node::Mul(vec![Node::Number(-1.0), Node::Cos(Box::new(u.clone()))]);
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(neg_cos),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(neg_cos), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![
            Node::Sin(Box::new(u.clone())),
            Node::Symbol(var.to_string()),
        ])),
        None => Err("Integration of sin(u) needs u linear in the integration variable".to_string()),
    }
}

fn integrate_cos(u: &Node, var: &str) -> Result<Node, String> {
    let sin_u = Node::Sin(Box::new(u.clone()));
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(sin_u),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(sin_u), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![
            Node::Cos(Box::new(u.clone())),
            Node::Symbol(var.to_string()),
        ])),
        None => Err("Integration of cos(u) needs u linear in the integration variable".to_string()),
    }
}

fn integrate_exp(u: &Node, var: &str) -> Result<Node, String> {
    let exp_u = Node::Exp(Box::new(u.clone()));
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(exp_u),
        Some(ArgForm::Linear(a)) => Ok(Node::Div(Box::new(exp_u), Box::new(Node::Number(a)))),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![exp_u, Node::Symbol(var.to_string())])),
        None => Err("Integration of exp(u) needs u linear in the integration variable".to_string()),
    }
}

fn integrate_ln(u: &Node, var: &str) -> Result<Node, String> {
    match arg_form(u, var) {
        Some(ArgForm::Var) => Ok(Node::Sub(
            Box::new(Node::Mul(vec![u.clone(), Node::Ln(Box::new(u.clone()))])),
            Box::new(u.clone()),
        )),
        Some(ArgForm::Constant) => Ok(Node::Mul(vec![
            Node::Ln(Box::new(u.clone())),
            Node::Symbol(var.to_string()),
        ])),
        _ => Err("Integration of ln(u) only supports u = var or a var-independent constant".to_string()),
    }
}

fn integrate_pow(base: &Node, exp: &Node, var: &str) -> Result<Node, String> {
    let Node::Number(n) = exp else {
        return Err("Integration of non-constant exponents is not supported yet".to_string());
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
        None => Err("Integration of base^n needs base linear in the integration variable".to_string()),
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

fn integrate_mul(factors: &[Node], var: &str) -> Result<Node, String> {
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
            Err("No u-substitution pattern matched this product".to_string())
        }
        _ => Err("Integration of products with more than two non-constant factors is not supported yet".to_string()),
    }
}

fn integrate_div(a: &Node, b: &Node, var: &str) -> Result<Node, String> {
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
    Err("Integration of this quotient is not supported yet".to_string())
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub(crate) node: Node,
}

impl Expr {
    pub fn number(v: f64) -> Expr {
        Expr {
            node: Node::Number(v),
        }
    }

    pub fn symbol(s: String) -> Expr {
        Expr {
            node: Node::Symbol(s),
        }
    }

    pub fn quantity(name: String, unit: &RationalUnit) -> Expr {
        Expr {
            node: Node::Quantity(name, unit.clone()),
        }
    }

    pub fn sin(e: &Expr) -> Expr {
        Expr {
            node: Node::Sin(Box::new(e.node.clone())),
        }
    }

    pub fn cos(e: &Expr) -> Expr {
        Expr {
            node: Node::Cos(Box::new(e.node.clone())),
        }
    }

    pub fn ln(e: &Expr) -> Expr {
        Expr {
            node: Node::Ln(Box::new(e.node.clone())),
        }
    }

    pub fn exp(e: &Expr) -> Expr {
        Expr {
            node: Node::Exp(Box::new(e.node.clone())),
        }
    }

    pub fn add(&self, other: &Expr) -> Result<Expr, String> {
        check_add_compat(&self.node, &other.node)?;
        Ok(Expr {
            node: Node::Add(flatten_add(vec![self.node.clone(), other.node.clone()])),
        })
    }

    pub fn sub(&self, other: &Expr) -> Result<Expr, String> {
        check_add_compat(&self.node, &other.node)?;
        Ok(Expr {
            node: Node::Sub(Box::new(self.node.clone()), Box::new(other.node.clone())),
        })
    }

    pub fn mul(&self, other: &Expr) -> Expr {
        Expr {
            node: Node::Mul(flatten_mul(vec![self.node.clone(), other.node.clone()])),
        }
    }

    pub fn div(&self, other: &Expr) -> Expr {
        Expr {
            node: Node::Div(Box::new(self.node.clone()), Box::new(other.node.clone())),
        }
    }

    pub fn pow(&self, other: &Expr) -> Expr {
        Expr {
            node: Node::Pow(Box::new(self.node.clone()), Box::new(other.node.clone())),
        }
    }

    pub fn simplify(&self) -> Expr {
        Expr {
            node: self.node.simplify(),
        }
    }

    pub fn diff(&self, var: &str, n: usize) -> Result<Expr, String> {
        let mut cur = self.node.clone();
        for _ in 0..n {
            cur = cur.diff_node(var)?;
        }
        Ok(Expr {
            node: cur.simplify(),
        })
    }

    pub fn integrate(&self, var: &str) -> Result<Expr, String> {
        Ok(Expr {
            node: self.node.integrate_node(var)?.simplify(),
        })
    }

    pub fn unit(&self) -> Result<Option<RationalUnit>, String> {
        self.node.infer_unit()
    }


    fn __repr__(&self) -> String {
        format!("{:?}", self.node)
    }

    fn __eq__(&self, other: &Expr) -> bool {
        self.node == other.node
    }

    fn __hash__(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&format!("{:?}", self.node), &mut h);
        std::hash::Hasher::finish(&h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x() -> Node {
        Node::Symbol("x".to_string())
    }

    fn n(v: f64) -> Node {
        Node::Number(v)
    }

    fn meter() -> RationalUnit {
        RationalUnit::new_from_dimensions(HashMap::from([("m".to_string(), (1, 1))]))
    }

    fn second() -> RationalUnit {
        RationalUnit::new_from_dimensions(HashMap::from([("s".to_string(), (1, 1))]))
    }

    use std::collections::HashMap;

    // --- simplify laws (§3.1) ---

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
    fn pow_zero_is_one() {
        assert_eq!(
            Node::Pow(Box::new(x()), Box::new(n(0.0))).simplify(),
            n(1.0)
        );
    }

    #[test]
    fn one_pow_x_is_one() {
        assert_eq!(
            Node::Pow(Box::new(n(1.0)), Box::new(x())).simplify(),
            n(1.0)
        );
    }

    #[test]
    fn inverse_sub_self() {
        assert_eq!(Node::Sub(Box::new(x()), Box::new(x())).simplify(), n(0.0));
    }

    #[test]
    fn inverse_div_self() {
        assert_eq!(Node::Div(Box::new(x()), Box::new(x())).simplify(), n(1.0));
    }

    #[test]
    fn collect_equal_factors_into_power() {
        assert_eq!(
            Node::Mul(vec![x(), x()]).simplify(),
            Node::Pow(Box::new(x()), Box::new(n(2.0)))
        );
    }

    #[test]
    fn constant_folding() {
        assert_eq!(Node::Add(vec![n(2.0), n(3.0)]).simplify(), n(5.0));
    }

    // --- differentiation (§4 / §5 SymEngine-aligned cases) ---

    #[test]
    fn diff_constant_is_zero() {
        assert_eq!(n(5.0).diff_node("x").unwrap().simplify(), n(0.0));
    }

    #[test]
    fn diff_var_is_one() {
        assert_eq!(x().diff_node("x").unwrap().simplify(), n(1.0));
    }

    #[test]
    fn diff_other_symbol_is_zero() {
        let y = Node::Symbol("y".to_string());
        assert_eq!(y.diff_node("x").unwrap().simplify(), n(0.0));
    }

    #[test]
    fn diff_power_rule() {
        let x_cubed = Node::Pow(Box::new(x()), Box::new(n(3.0)));
        let d = x_cubed.diff_node("x").unwrap().simplify();
        // 3 * x^2
        assert_eq!(
            d,
            Node::Mul(vec![n(3.0), Node::Pow(Box::new(x()), Box::new(n(2.0)))])
        );
    }

    #[test]
    fn diff_sin() {
        let d = Node::Sin(Box::new(x())).diff_node("x").unwrap().simplify();
        assert_eq!(d, Node::Cos(Box::new(x())));
    }

    #[test]
    fn diff_product_rule() {
        let y = Node::Symbol("y".to_string());
        let prod = Node::Mul(vec![x(), y.clone()]);
        // d/dx(x*y) = y  (x treated as var, y as constant)
        let d = prod.diff_node("x").unwrap().simplify();
        assert_eq!(d, y);
    }

    // --- integration (§4.2 / §5 SymEngine-aligned cases) ---

    #[test]
    fn integrate_power_rule() {
        // integral(x^2, x) == x^3 / 3
        let x_sq = Node::Pow(Box::new(x()), Box::new(n(2.0)));
        let integral = x_sq.integrate_node("x").unwrap().simplify();
        let expected = Node::Div(
            Box::new(Node::Pow(Box::new(x()), Box::new(n(3.0)))),
            Box::new(n(3.0)),
        )
        .simplify();
        assert_eq!(integral, expected);
    }

    #[test]
    fn integrate_cos_is_sin() {
        // integral(cos(x), x) == sin(x)
        let d = Node::Cos(Box::new(x()))
            .integrate_node("x")
            .unwrap()
            .simplify();
        assert_eq!(d, Node::Sin(Box::new(x())));
    }

    #[test]
    fn integrate_sin_is_neg_cos() {
        let d = Node::Sin(Box::new(x()))
            .integrate_node("x")
            .unwrap()
            .simplify();
        assert_eq!(
            d,
            Node::Mul(vec![n(-1.0), Node::Cos(Box::new(x()))]).simplify()
        );
    }

    #[test]
    fn integrate_constant() {
        // integral(5, x) == 5*x
        assert_eq!(
            n(5.0).integrate_node("x").unwrap().simplify(),
            Node::Mul(vec![n(5.0), x()])
        );
    }

    #[test]
    fn integrate_linear_chain_rule() {
        // integral(cos(2x), x) == sin(2x) / 2
        let arg = Node::Mul(vec![n(2.0), x()]);
        let d = Node::Cos(Box::new(arg.clone()))
            .integrate_node("x")
            .unwrap()
            .simplify();
        let expected = Node::Div(Box::new(Node::Sin(Box::new(arg))), Box::new(n(2.0))).simplify();
        assert_eq!(d, expected);
    }

    #[test]
    fn integrate_u_substitution() {
        // integral(2x * cos(x^2), x) == sin(x^2)   (g(x)=x^2, g'(x)=2x)
        let g = Node::Pow(Box::new(x()), Box::new(n(2.0)));
        let g_prime = Node::Mul(vec![n(2.0), x()]);
        let expr = Node::Mul(vec![g_prime, Node::Cos(Box::new(g.clone()))]);
        let d = expr.integrate_node("x").unwrap().simplify();
        assert_eq!(d, Node::Sin(Box::new(g)));
    }

    #[test]
    fn integrate_reciprocal_is_ln() {
        // integral(1/x, x) == ln(x)
        let recip = Node::Pow(Box::new(x()), Box::new(n(-1.0)));
        let d = recip.integrate_node("x").unwrap().simplify();
        assert_eq!(d, Node::Ln(Box::new(x())));
    }

    #[test]
    fn integrate_non_matching_pattern_errors() {
        // integral(x * ln(x), x) -- product with two non-constant factors and
        // no matching u-substitution -- is outside the Level-1 pattern set.
        let expr = Node::Mul(vec![x(), Node::Ln(Box::new(x()))]);
        assert!(expr.integrate_node("x").is_err());
    }

    // --- unit-awareness (§6) ---

    #[test]
    fn add_matching_units_ok() {
        let a = Node::Quantity("a".to_string(), meter());
        let b = Node::Quantity("b".to_string(), meter());
        let sum = Node::Add(vec![a, b]);
        assert!(sum.infer_unit().unwrap().is_some());
    }

    #[test]
    fn add_mismatched_units_errors() {
        let a = Node::Quantity("a".to_string(), meter());
        let b = Node::Quantity("b".to_string(), second());
        let sum = Node::Add(vec![a, b]);
        assert!(sum.infer_unit().is_err());
    }

    #[test]
    fn mul_units_compose() {
        let a = Node::Quantity("x".to_string(), meter());
        let b = Node::Quantity("t".to_string(), second());
        let prod = Node::Mul(vec![a, b]);
        let u = prod.infer_unit().unwrap().unwrap();
        assert_eq!(u, meter().mul(&second()));
    }

    #[test]
    fn diff_propagates_unit_via_product_rule() {
        // x[m] * t[s], diff wrt t => x (unit m), matching unit(x*t)/unit(t) = m
        let xq = Node::Quantity("x".to_string(), meter());
        let tq = Node::Quantity("t".to_string(), second());
        let expr = Node::Mul(vec![xq.clone(), tq]);
        let d = expr.diff_node("t").unwrap().simplify();
        assert_eq!(d, xq);
        assert_eq!(d.infer_unit().unwrap().unwrap(), meter());
    }

    #[test]
    fn transcendental_of_dimensioned_arg_errors() {
        let m = Node::Quantity("x".to_string(), meter());
        let expr = Node::Sin(Box::new(m));
        assert!(expr.infer_unit().is_err());
    }
}
