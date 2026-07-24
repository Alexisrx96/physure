use physure_core::error::{PhysureError, PhysureResult};
use super::ast::Node;

impl Node {
    pub fn solve_equation(&self, target: &str) -> PhysureResult<Node> {
        let simplified = self.simplify();

        // Check if expression is linear: a * target + b = 0
        if let Some((a, b)) = simplified.linear_coeff(target) {
            if a != 0.0 {
                let solution = Node::Div(
                    Box::new(Node::Number(-b)),
                    Box::new(Node::Number(a)),
                );
                return Ok(solution.simplify());
            }
        }

        if let Node::Sub(left, right) = &simplified {
            if !left.depends_on(target) && right.depends_on(target) {
                if let Node::Mul(factors) = &**right {
                    let (target_factors, other_factors): (Vec<_>, Vec<_>) = factors.iter().cloned().partition(|f| f.depends_on(target));
                    if target_factors.len() == 1 {
                        if target_factors[0] == Node::Symbol(target.to_string()) {
                            let other_node = if other_factors.is_empty() {
                                Node::Number(1.0)
                            } else if other_factors.len() == 1 {
                                other_factors[0].clone()
                            } else {
                                Node::Mul(other_factors)
                            };
                            let solution = Node::Div(left.clone(), Box::new(other_node));
                            return Ok(solution.simplify());
                        } else if let Node::Pow(b, exp) = &target_factors[0] {
                            if **b == Node::Symbol(target.to_string()) {
                                let other_node = if other_factors.is_empty() {
                                    Node::Number(1.0)
                                } else if other_factors.len() == 1 {
                                    other_factors[0].clone()
                                } else {
                                    Node::Mul(other_factors)
                                };
                                let div = Node::Div(left.clone(), Box::new(other_node));
                                let solution = Node::Pow(
                                    Box::new(div),
                                    Box::new(Node::Div(Box::new(Node::Number(1.0)), exp.clone())),
                                );
                                return Ok(solution.simplify());
                            }
                        }
                    }
                } else if **right == Node::Symbol(target.to_string()) {
                    return Ok((**left).clone());
                }
            } else if left.depends_on(target) && !right.depends_on(target) {
                if let Node::Mul(factors) = &**left {
                    let (target_factors, other_factors): (Vec<_>, Vec<_>) = factors.iter().cloned().partition(|f| f.depends_on(target));
                    if target_factors.len() == 1 {
                        if target_factors[0] == Node::Symbol(target.to_string()) {
                            let other_node = if other_factors.is_empty() {
                                Node::Number(1.0)
                            } else if other_factors.len() == 1 {
                                other_factors[0].clone()
                            } else {
                                Node::Mul(other_factors)
                            };
                            let solution = Node::Div(right.clone(), Box::new(other_node));
                            return Ok(solution.simplify());
                        } else if let Node::Pow(b, exp) = &target_factors[0] {
                            if **b == Node::Symbol(target.to_string()) {
                                let other_node = if other_factors.is_empty() {
                                    Node::Number(1.0)
                                } else if other_factors.len() == 1 {
                                    other_factors[0].clone()
                                } else {
                                    Node::Mul(other_factors)
                                };
                                let div = Node::Div(right.clone(), Box::new(other_node));
                                let solution = Node::Pow(
                                    Box::new(div),
                                    Box::new(Node::Div(Box::new(Node::Number(1.0)), exp.clone())),
                                );
                                return Ok(solution.simplify());
                            }
                        }
                    }
                } else if **left == Node::Symbol(target.to_string()) {
                    return Ok((**right).clone());
                }
            }
        }

        // Try power rule equation: a * target^n + b = 0 => target = (-b / a)^(1/n)
        if let Node::Add(terms) = &simplified {
            let mut target_term: Option<&Node> = None;
            let mut const_val = 0.0;

            for t in terms {
                if t.depends_on(target) {
                    if target_term.is_none() {
                        target_term = Some(t);
                    } else {
                        return Err(PhysureError::Generic(format!(
                            "Multiple non-linear terms for target '{}' in equation",
                            target
                        )));
                    }
                } else if let Node::Number(n) = t {
                    const_val += n;
                }
            }

            if let Some(t_node) = target_term {
                if let Node::Pow(base, exp) = t_node {
                    if let Node::Symbol(s) = &**base {
                        if s == target {
                            let solution = Node::Pow(
                                Box::new(Node::Number(-const_val)),
                                Box::new(Node::Div(Box::new(Node::Number(1.0)), exp.clone())),
                            );
                            return Ok(solution.simplify());
                        }
                    }
                }
            }
        }

        Err(PhysureError::Generic(format!(
            "Cannot solve equation symbolically for target '{}'",
            target
        )))
    }
}
