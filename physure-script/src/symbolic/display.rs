use super::ast::Node;
use std::fmt;

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_phs_string())
    }
}

impl Node {
    pub fn to_phs_string(&self) -> String {
        match self {
            Node::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{:.0}", n)
                } else {
                    format!("{}", n)
                }
            }
            Node::Symbol(s) => s.clone(),
            Node::Quantity(name, u) => format!("{} {}", name, u.__repr__()),
            Node::Add(terms) => {
                let s = terms
                    .iter()
                    .map(|t| t.to_phs_string())
                    .collect::<Vec<_>>()
                    .join(" + ");
                s.replace("+ -", "- ")
            }
            Node::Sub(a, b) => format!(
                "{} - {}",
                a.to_phs_string_parenthesized(1),
                b.to_phs_string_parenthesized(1)
            ),
            Node::Mul(factors) => factors
                .iter()
                .map(|f| f.to_phs_string_parenthesized(2))
                .collect::<Vec<_>>()
                .join(" * "),
            Node::Div(a, b) => format!(
                "{}/{}",
                a.to_phs_string_parenthesized(3),
                b.to_phs_string_parenthesized(3)
            ),
            Node::Pow(base, exp) => format!(
                "{}^{}",
                base.to_phs_string_parenthesized(4),
                exp.to_phs_string_parenthesized(4)
            ),
            Node::Sin(u) => format!("sin({})", u.to_phs_string()),
            Node::Cos(u) => format!("cos({})", u.to_phs_string()),
            Node::Tan(u) => format!("tan({})", u.to_phs_string()),
            Node::Cot(u) => format!("cot({})", u.to_phs_string()),
            Node::Sec(u) => format!("sec({})", u.to_phs_string()),
            Node::Csc(u) => format!("csc({})", u.to_phs_string()),
            Node::Arcsin(u) => format!("asin({})", u.to_phs_string()),
            Node::Arccos(u) => format!("acos({})", u.to_phs_string()),
            Node::Arctan(u) => format!("atan({})", u.to_phs_string()),
            Node::Arccot(u) => format!("acot({})", u.to_phs_string()),
            Node::Arcsec(u) => format!("asec({})", u.to_phs_string()),
            Node::Arccsc(u) => format!("acsc({})", u.to_phs_string()),
            Node::Sinh(u) => format!("sinh({})", u.to_phs_string()),
            Node::Cosh(u) => format!("cosh({})", u.to_phs_string()),
            Node::Tanh(u) => format!("tanh({})", u.to_phs_string()),
            Node::Coth(u) => format!("coth({})", u.to_phs_string()),
            Node::Sech(u) => format!("sech({})", u.to_phs_string()),
            Node::Csch(u) => format!("csch({})", u.to_phs_string()),
            Node::Ln(u) => format!("ln({})", u.to_phs_string()),
            Node::Exp(u) => format!("exp({})", u.to_phs_string()),
            Node::Abs(u) => format!("abs({})", u.to_phs_string()),
            Node::Sqrt(u) => format!("sqrt({})", u.to_phs_string()),
            Node::Equation(a, b) => format!("{} = {}", a.to_phs_string(), b.to_phs_string()),
            Node::Integral(u, v) => format!("integral({}, {})", u.to_phs_string(), v),
        }
    }

    fn precedence(&self) -> u8 {
        match self {
            Node::Equation(..) => 0,
            Node::Number(_)
            | Node::Symbol(_)
            | Node::Quantity(..)
            | Node::Sin(_)
            | Node::Cos(_)
            | Node::Tan(_)
            | Node::Cot(_)
            | Node::Sec(_)
            | Node::Csc(_)
            | Node::Arcsin(_)
            | Node::Arccos(_)
            | Node::Arctan(_)
            | Node::Arccot(_)
            | Node::Arcsec(_)
            | Node::Arccsc(_)
            | Node::Sinh(_)
            | Node::Cosh(_)
            | Node::Tanh(_)
            | Node::Coth(_)
            | Node::Sech(_)
            | Node::Csch(_)
            | Node::Ln(_)
            | Node::Exp(_)
            | Node::Abs(_)
            | Node::Sqrt(_)
            | Node::Integral(..) => 5,
            Node::Pow(..) => 4,
            Node::Div(..) => 3,
            Node::Mul(..) => 2,
            Node::Add(..) | Node::Sub(..) => 1,
        }
    }

    fn to_phs_string_parenthesized(&self, parent_prec: u8) -> String {
        if self.precedence() < parent_prec {
            format!("({})", self.to_phs_string())
        } else {
            self.to_phs_string()
        }
    }
}
