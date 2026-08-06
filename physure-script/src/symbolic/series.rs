use physure_core::error::PhysureResult;
use super::ast::Node;

macro_rules! subst_unary {
    ($variant:ident, $arg:expr, $var:expr, $value:expr) => {
        Node::$variant(Box::new($arg.subst($var, $value)))
    };
}

impl Node {
    /// Replaces every free occurrence of the symbol `var` with `value`.
    ///
    /// The `Integral` integration variable is a bound name, so a substitution
    /// targeting it is a no-op rather than a rewrite of the integrand.
    pub fn subst(&self, var: &str, value: &Node) -> Node {
        match self {
            Node::Number(_) => self.clone(),
            Node::Symbol(s) => {
                if s == var {
                    value.clone()
                } else {
                    self.clone()
                }
            }
            Node::Quantity(name, _) => {
                if name == var {
                    value.clone()
                } else {
                    self.clone()
                }
            }
            Node::Add(terms) => Node::Add(terms.iter().map(|t| t.subst(var, value)).collect()),
            Node::Mul(factors) => Node::Mul(factors.iter().map(|f| f.subst(var, value)).collect()),
            Node::Sub(a, b) => Node::Sub(
                Box::new(a.subst(var, value)),
                Box::new(b.subst(var, value)),
            ),
            Node::Div(a, b) => Node::Div(
                Box::new(a.subst(var, value)),
                Box::new(b.subst(var, value)),
            ),
            Node::Pow(a, b) => Node::Pow(
                Box::new(a.subst(var, value)),
                Box::new(b.subst(var, value)),
            ),
            Node::Equation(a, b) => Node::Equation(
                Box::new(a.subst(var, value)),
                Box::new(b.subst(var, value)),
            ),
            Node::Integral(u, v) => {
                if v == var {
                    self.clone()
                } else {
                    Node::Integral(Box::new(u.subst(var, value)), v.clone())
                }
            }
            Node::Sin(u) => subst_unary!(Sin, u, var, value),
            Node::Cos(u) => subst_unary!(Cos, u, var, value),
            Node::Tan(u) => subst_unary!(Tan, u, var, value),
            Node::Cot(u) => subst_unary!(Cot, u, var, value),
            Node::Sec(u) => subst_unary!(Sec, u, var, value),
            Node::Csc(u) => subst_unary!(Csc, u, var, value),
            Node::Arcsin(u) => subst_unary!(Arcsin, u, var, value),
            Node::Arccos(u) => subst_unary!(Arccos, u, var, value),
            Node::Arctan(u) => subst_unary!(Arctan, u, var, value),
            Node::Arccot(u) => subst_unary!(Arccot, u, var, value),
            Node::Arcsec(u) => subst_unary!(Arcsec, u, var, value),
            Node::Arccsc(u) => subst_unary!(Arccsc, u, var, value),
            Node::Sinh(u) => subst_unary!(Sinh, u, var, value),
            Node::Cosh(u) => subst_unary!(Cosh, u, var, value),
            Node::Tanh(u) => subst_unary!(Tanh, u, var, value),
            Node::Coth(u) => subst_unary!(Coth, u, var, value),
            Node::Sech(u) => subst_unary!(Sech, u, var, value),
            Node::Csch(u) => subst_unary!(Csch, u, var, value),
            Node::Ln(u) => subst_unary!(Ln, u, var, value),
            Node::Exp(u) => subst_unary!(Exp, u, var, value),
            Node::Abs(u) => subst_unary!(Abs, u, var, value),
            Node::Sqrt(u) => subst_unary!(Sqrt, u, var, value),
        }
    }

    /// Taylor polynomial of `self` about `var = at`, keeping powers up to and
    /// including `(var - at)^order`.
    ///
    /// Unlike sympy's `series()`, no `O()` remainder term is produced and no
    /// Laurent/Puiseux branch exists: an expansion point where a derivative is
    /// undefined (`ln(x)` at 0) yields that undefined coefficient verbatim in
    /// the output rather than an error.
    // ponytail: term-by-term differentiation, no singularity handling. Add a
    // Laurent path when someone actually expands about a pole.
    pub fn series(&self, var: &str, at: &Node, order: usize) -> PhysureResult<Node> {
        let shift = Node::Sub(
            Box::new(Node::Symbol(var.to_string())),
            Box::new(at.clone()),
        )
        .simplify();

        let mut deriv = self.clone();
        let mut factorial = 1.0f64;
        let mut terms: Vec<Node> = Vec::new();

        for k in 0..=order {
            if k > 0 {
                factorial *= k as f64;
                deriv = deriv.diff_node(var)?.simplify();
            }
            let coeff = deriv.subst(var, at).simplify();
            if matches!(coeff, Node::Number(c) if c == 0.0) {
                continue;
            }
            let numerator = if k == 0 {
                coeff
            } else {
                Node::Mul(vec![
                    coeff,
                    Node::Pow(Box::new(shift.clone()), Box::new(Node::Number(k as f64))),
                ])
            };
            terms.push(if factorial == 1.0 {
                numerator.simplify()
            } else {
                Node::Div(Box::new(numerator), Box::new(Node::Number(factorial))).simplify()
            });
        }

        if terms.is_empty() {
            return Ok(Node::Number(0.0));
        }
        Ok(Node::Add(terms).simplify())
    }
}
