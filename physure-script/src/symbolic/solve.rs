use physure_core::error::{PhysureError, PhysureResult};
use super::ast::Node;

impl Node {
    pub fn solve_equation(&self, target: &str) -> PhysureResult<Node> {
        let simplified = match self.simplify() {
            Node::Equation(l, r) | Node::Sub(l, r) => equation_as_difference(l, r),
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
            if let Some(solved) = solve_difference(left, right, target) {
                return solved;
            }
        }

        if let Node::Add(terms) = &simplified {
            if let Some(solved) = solve_sum(terms, target) {
                return solved;
            }
        }

        Err(PhysureError::Generic(format!(
            "Cannot solve equation symbolically for target '{}'",
            target
        )))
    }
}

/// `l = r` as the node `l - r`; when `l` is 0 the sign of `r` is flipped instead (term by term for a sum).
fn equation_as_difference(l: Box<Node>, r: Box<Node>) -> Node {
    if !matches!(*l, Node::Number(c) if c == 0.0) {
        return Node::Sub(l, r);
    }
    let negate = |t: &Node| Node::Mul(vec![Node::Number(-1.0), t.clone()]).simplify();
    match &*r {
        Node::Add(terms) => Node::Add(terms.iter().map(negate).collect()).simplify(),
        other => negate(other),
    }
}

/// One node for a list of factors: 1 for none, the factor itself for one, their product otherwise.
fn product_of(mut factors: Vec<Node>) -> Node {
    match factors.len() {
        0 => Node::Number(1.0),
        1 => factors.remove(0),
        _ => Node::Mul(factors),
    }
}

/// One node for a list of terms: 0 for none, the term itself for one, their sum otherwise.
fn sum_of(mut terms: Vec<Node>) -> Node {
    match terms.len() {
        0 => Node::Number(0.0),
        1 => terms.remove(0),
        _ => Node::Add(terms),
    }
}

/// `left - right = 0` where only one side depends on the target: solves `dependent = constant` for the target.
fn solve_difference(left: &Node, right: &Node, target: &str) -> Option<PhysureResult<Node>> {
    if !left.depends_on(target) && right.depends_on(target) {
        solve_isolated(right, left, target, true)
    } else if left.depends_on(target) && !right.depends_on(target) {
        solve_isolated(left, right, target, false)
    } else {
        None
    }
}

/// `dependent = constant` for the target, when `dependent` is the target, a product with it, or a quotient
/// with it. `constant_on_left` says which side of the original difference the constant was, which is the
/// orientation of the difference a quotient is cleared into.
fn solve_isolated(dependent: &Node, constant: &Node, target: &str, constant_on_left: bool) -> Option<PhysureResult<Node>> {
    let target_symbol = Node::Symbol(target.to_string());
    match dependent {
        Node::Mul(factors) => solve_product(factors, constant, target).map(Ok),
        _ if *dependent == target_symbol => Some(Ok(constant.clone())),
        Node::Div(num, den) => {
            if num.depends_on(target) && !den.depends_on(target) {
                let cleared = Box::new(Node::Mul(vec![constant.clone(), (**den).clone()]));
                let difference = if constant_on_left {
                    Node::Sub(cleared, num.clone())
                } else {
                    Node::Sub(num.clone(), cleared)
                };
                Some(difference.solve_equation(target))
            } else if den.depends_on(target) && !num.depends_on(target) && **den == target_symbol {
                Some(Ok(Node::Div(num.clone(), Box::new(constant.clone())).simplify()))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// `target * k = constant` or `target^n * k = constant`, for a product with exactly one factor that has the target.
fn solve_product(factors: &[Node], constant: &Node, target: &str) -> Option<Node> {
    let (target_factors, other_factors): (Vec<_>, Vec<_>) = factors.iter().cloned().partition(|f| f.depends_on(target));
    if target_factors.len() != 1 {
        return None;
    }
    let other_node = product_of(other_factors);
    let quotient = Node::Div(Box::new(constant.clone()), Box::new(other_node));
    if target_factors[0] == Node::Symbol(target.to_string()) {
        return Some(quotient.simplify());
    }
    if let Node::Pow(b, exp) = &target_factors[0] {
        if **b == Node::Symbol(target.to_string()) {
            let solution = Node::Pow(
                Box::new(quotient),
                Box::new(Node::Div(Box::new(Node::Number(1.0)), exp.clone())),
            );
            return Some(solution.simplify());
        }
    }
    None
}

/// A sum with one term that has the target: the term is the target (or the target times a coefficient) and the
/// rest moves across; or the term is a power of the target and the rest is a number.
fn solve_sum(terms: &[Node], target: &str) -> Option<PhysureResult<Node>> {
    let (target_terms, other_terms): (Vec<&Node>, Vec<&Node>) = terms.iter().partition(|t| t.depends_on(target));
    if target_terms.len() == 1 {
        if let Some(solution) = solve_single_term(target_terms[0], other_terms, target) {
            return Some(Ok(solution));
        }
    }
    solve_power_term(terms, target)
}

/// `term + rest = 0` where `term` is the target, or a product of the target and a coefficient.
fn solve_single_term(target_node: &Node, other_terms: Vec<&Node>, target: &str) -> Option<Node> {
    let other_sum = sum_of(other_terms.into_iter().cloned().collect());
    let neg_other = Node::Mul(vec![Node::Number(-1.0), other_sum]);

    if *target_node == Node::Symbol(target.to_string()) {
        return Some(neg_other.simplify());
    }
    let Node::Mul(factors) = target_node else {
        return None;
    };
    let (t_factors, o_factors): (Vec<&Node>, Vec<&Node>) = factors.iter().partition(|f| f.depends_on(target));
    if t_factors.len() == 1 && *t_factors[0] == Node::Symbol(target.to_string()) {
        let coeff = product_of(o_factors.into_iter().cloned().collect());
        return Some(Node::Div(Box::new(neg_other), Box::new(coeff)).simplify());
    }
    None
}

/// `target^n + c = 0` where the sum has no other term with the target: `(-c)^(1/n)`.
fn solve_power_term(terms: &[Node], target: &str) -> Option<PhysureResult<Node>> {
    let mut target_term: Option<&Node> = None;
    let mut const_val = 0.0;

    for t in terms {
        if t.depends_on(target) {
            if target_term.is_none() {
                target_term = Some(t);
            } else {
                return Some(Err(PhysureError::Generic(format!(
                    "Multiple non-linear terms for target '{}' in equation",
                    target
                ))));
            }
        } else if let Node::Number(n) = t {
            const_val += n;
        }
    }

    let Node::Pow(base, exp) = target_term? else {
        return None;
    };
    let Node::Symbol(s) = &**base else {
        return None;
    };
    if s != target {
        return None;
    }
    let solution = Node::Pow(
        Box::new(Node::Number(-const_val)),
        Box::new(Node::Div(Box::new(Node::Number(1.0)), exp.clone())),
    );
    Some(Ok(solution.simplify()))
}
