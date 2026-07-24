use physure_core::units::RationalUnit;
use physure_core::error::PhysureResult;
use super::ast::{Node, check_add_compat, flatten_add, flatten_mul};
use super::compiler::CompiledExpr;

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub(crate) node: Node,
}

impl std::hash::Hash for Expr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        format!("{:?}", self.node).hash(state);
    }
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

    pub fn add(&self, other: &Expr) -> PhysureResult<Expr> {
        check_add_compat(&self.node, &other.node)?;
        Ok(Expr {
            node: Node::Add(flatten_add(vec![self.node.clone(), other.node.clone()])),
        })
    }

    pub fn sub(&self, other: &Expr) -> PhysureResult<Expr> {
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

    pub fn factor(&self) -> Expr {
        Expr {
            node: self.node.factor(),
        }
    }

    pub fn diff(&self, var: &str, n: usize) -> PhysureResult<Expr> {
        let mut cur = self.node.clone();
        for _ in 0..n {
            cur = cur.diff_node(var)?;
        }
        Ok(Expr {
            node: cur.simplify(),
        })
    }

    pub fn integrate(&self, var: &str) -> PhysureResult<Expr> {
        Ok(Expr {
            node: self.node.integrate_node(var)?.simplify(),
        })
    }

    pub fn compile(&self) -> PhysureResult<CompiledExpr> {
        CompiledExpr::compile(&self.node)
    }

    pub fn unit(&self) -> PhysureResult<Option<RationalUnit>> {
        self.node.infer_unit()
    }

    pub fn parse(input: &str) -> PhysureResult<Expr> {
        let node = super::parser::SymbolicParser::parse_str(input)?;
        Ok(Expr { node })
    }

    pub fn to_phs_string(&self) -> String {
        self.node.to_phs_string()
    }

    pub fn diff_str(input: &str, var: &str) -> PhysureResult<String> {
        let expr = Self::parse(input)?;
        let diffed = expr.diff(var, 1)?;
        Ok(diffed.to_phs_string())
    }

    pub fn integrate_str(input: &str, var: &str) -> PhysureResult<String> {
        let expr = Self::parse(input)?;
        let integrated = expr.integrate(var)?;
        Ok(integrated.to_phs_string())
    }

    pub fn solve_str(eq_input: &str, target: &str) -> PhysureResult<String> {
        let expr = Self::parse(eq_input)?;
        let solved = expr.node.solve_equation(target)?;
        Ok(solved.to_phs_string())
    }

    fn __repr__(&self) -> String {
        self.to_phs_string()
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
