use crate::units::RationalUnit;
use crate::error::PhysureResult;
use super::ast::{Node, check_add_compat, flatten_add, flatten_mul};
use super::compiler::CompiledExpr;

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
