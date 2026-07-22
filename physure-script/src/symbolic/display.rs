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
            Node::Ln(u) => format!("ln({})", u.to_phs_string()),
            Node::Exp(u) => format!("exp({})", u.to_phs_string()),
        }
    }

    fn precedence(&self) -> u8 {
        match self {
            Node::Number(_)
            | Node::Symbol(_)
            | Node::Quantity(..)
            | Node::Sin(_)
            | Node::Cos(_)
            | Node::Ln(_)
            | Node::Exp(_) => 5,
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
