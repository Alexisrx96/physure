use physure_core::error::{PhysureError, PhysureResult};
use super::ast::Node;

impl Node {
    pub fn solve_equation(&self, target: &str) -> PhysureResult<Node> {
        let simplified = match self.simplify() {
            Node::Equation(l, r) => {
                if matches!(*l, Node::Number(c) if c == 0.0) {
                    if let Node::Add(terms) = &*r {
                        let neg_terms = terms
                            .iter()
                            .map(|t| Node::Mul(vec![Node::Number(-1.0), t.clone()]).simplify())
                            .collect();
                        Node::Add(neg_terms).simplify()
                    } else {
                        Node::Mul(vec![Node::Number(-1.0), (*r).clone()]).simplify()
                    }
                } else {
                    Node::Sub(l, r)
                }
            }
            Node::Sub(l, r) => {
                if matches!(*l, Node::Number(c) if c == 0.0) {
                    if let Node::Add(terms) = &*r {
                        let neg_terms = terms
                            .iter()
                            .map(|t| Node::Mul(vec![Node::Number(-1.0), t.clone()]).simplify())
                            .collect();
                        Node::Add(neg_terms).simplify()
                    } else {
                        Node::Mul(vec![Node::Number(-1.0), (*r).clone()]).simplify()
                    }
                } else {
                    Node::Sub(l, r)
                }
            }
            other => other,
        };

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
                } else if let Node::Div(num, den) = &**right {
                    if num.depends_on(target) && !den.depends_on(target) {
                        return Node::Sub(Box::new(Node::Mul(vec![(**left).clone(), (**den).clone()])), num.clone()).solve_equation(target);
                    } else if den.depends_on(target) && !num.depends_on(target) && **den == Node::Symbol(target.to_string()) {
                        let solution = Node::Div(num.clone(), left.clone());
                        return Ok(solution.simplify());
                    }
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
                } else if let Node::Div(num, den) = &**left {
                    if num.depends_on(target) && !den.depends_on(target) {
                        return Node::Sub(num.clone(), Box::new(Node::Mul(vec![(**right).clone(), (**den).clone()]))).solve_equation(target);
                    } else if den.depends_on(target) && !num.depends_on(target) && **den == Node::Symbol(target.to_string()) {
                        let solution = Node::Div(num.clone(), right.clone());
                        return Ok(solution.simplify());
                    }
                }
            }
        }

        if let Node::Add(terms) = &simplified {
            let (target_terms, other_terms): (Vec<_>, Vec<_>) =
                terms.iter().partition(|t| t.depends_on(target));
            if target_terms.len() == 1 {
                let target_node = target_terms[0];
                let other_sum = if other_terms.is_empty() {
                    Node::Number(0.0)
                } else if other_terms.len() == 1 {
                    other_terms[0].clone()
                } else {
                    Node::Add(other_terms.into_iter().cloned().collect())
                };

                let neg_other = Node::Mul(vec![Node::Number(-1.0), other_sum]);

                if target_node == &Node::Symbol(target.to_string()) {
                    return Ok(neg_other.simplify());
                } else if let Node::Mul(factors) = target_node {
                    let (t_factors, o_factors): (Vec<_>, Vec<_>) =
                        factors.iter().partition(|f| f.depends_on(target));
                    if t_factors.len() == 1 && *t_factors[0] == Node::Symbol(target.to_string()) {
                        let coeff = if o_factors.is_empty() {
                            Node::Number(1.0)
                        } else if o_factors.len() == 1 {
                            o_factors[0].clone()
                        } else {
                            Node::Mul(o_factors.into_iter().cloned().collect())
                        };
                        let sol = Node::Div(Box::new(neg_other), Box::new(coeff));
                        return Ok(sol.simplify());
                    }
                }
            }

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
