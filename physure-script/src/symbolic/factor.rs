use super::ast::{Node, flatten_add, flatten_mul};

impl Node {
    /// Applies algebraic factorization rules:
    /// 1. Extracts common factors from additive terms: c*a + c*b -> c*(a + b)
    /// 2. Combines powers of identical bases: x^a * x^b -> x^(a + b)
    pub fn factor(&self) -> Node {
        match self {
            Node::Add(terms) => factor_add(terms),
            Node::Mul(factors) => factor_mul(factors),
            Node::Sub(a, b) => Node::Sub(Box::new(a.factor()), Box::new(b.factor())).simplify(),
            Node::Div(a, b) => Node::Div(Box::new(a.factor()), Box::new(b.factor())).simplify(),
            Node::Pow(b, e) => Node::Pow(Box::new(b.factor()), Box::new(e.factor())).simplify(),
            _ => self.clone(),
        }
    }
}

fn count_vec_add(vec: &mut Vec<(Node, usize)>, item: &Node) {
    if let Some(entry) = vec.iter_mut().find(|(n, _)| n == item) {
        entry.1 += 1;
    } else {
        vec.push((item.clone(), 1));
    }
}

fn count_vec_get(vec: &[(Node, usize)], item: &Node) -> usize {
    vec.iter().find(|(n, _)| n == item).map(|(_, c)| *c).unwrap_or(0)
}

/// How many times each factor that is not a number occurs among `factors`.
fn non_number_counts(factors: &[Node]) -> Vec<(Node, usize)> {
    let mut counts: Vec<(Node, usize)> = Vec::new();
    for factor in factors {
        if !matches!(factor, Node::Number(_)) {
            count_vec_add(&mut counts, factor);
        }
    }
    counts
}

/// The factors every term has, with the number of times every term has them (the smallest count among the terms).
fn common_factor_counts(term_factors: &[Vec<Node>]) -> Vec<(Node, usize)> {
    let Some((first_term, other_terms)) = term_factors.split_first() else {
        return Vec::new();
    };
    let mut common_counts = non_number_counts(first_term);
    for term in other_terms {
        let current_counts = non_number_counts(term);
        for (node, count) in common_counts.iter_mut() {
            let current_cnt = count_vec_get(&current_counts, node);
            *count = (*count).min(current_cnt);
        }
    }
    common_counts
}

/// A term's factors once one occurrence of each shared factor is taken out, as a node.
fn remaining_term(term: Vec<Node>, shared_factors: &[Node]) -> Node {
    let mut rem = term;
    for sf in shared_factors {
        if let Some(pos) = rem.iter().position(|x| x == sf) {
            rem.remove(pos);
        }
    }
    match rem.len() {
        0 => Node::Number(1.0),
        1 => rem.pop().unwrap(),
        _ => Node::Mul(rem).simplify(),
    }
}

fn factor_add(terms: &[Node]) -> Node {
    let terms_factored: Vec<Node> = terms.iter().map(Node::factor).collect();
    let flat_terms = flatten_add(terms_factored);
    if flat_terms.len() < 2 {
        return Node::Add(flat_terms).simplify();
    }

    // Step 1: Extract factor lists for each term
    let term_factors: Vec<Vec<Node>> = flat_terms.iter().map(|t| match t {
        Node::Mul(fs) => flatten_mul(fs.clone()),
        other => vec![other.clone()],
    }).collect();

    // Step 2: Find intersection of non-number terms
    let common_counts = common_factor_counts(&term_factors);

    // Step 3: Check if common factors exist
    let mut shared_factors = Vec::new();
    for (node, count) in common_counts {
        for _ in 0..count {
            shared_factors.push(node.clone());
        }
    }

    if shared_factors.is_empty() {
        return Node::Add(flat_terms).simplify();
    }

    // Step 4: Divide out common factors from each term
    let remaining_terms: Vec<Node> = term_factors.into_iter().map(|term| remaining_term(term, &shared_factors)).collect();

    let inner_sum = Node::Add(remaining_terms).simplify();
    shared_factors.push(inner_sum);
    Node::Mul(shared_factors).simplify()
}

fn factor_mul(factors: &[Node]) -> Node {
    let flat_factors = flatten_mul(factors.iter().map(Node::factor).collect());
    let mut base_groups: Vec<(Node, Node)> = Vec::new();

    for factor in flat_factors {
        let (base, exp) = match factor {
            Node::Pow(b, e) => ((*b).clone(), (*e).clone()),
            other => (other, Node::Number(1.0)),
        };

        if let Some(entry) = base_groups.iter_mut().find(|(b, _)| *b == base) {
            entry.1 = Node::Add(vec![entry.1.clone(), exp]).simplify();
        } else {
            base_groups.push((base, exp));
        }
    }

    let mut out = Vec::new();
    for (base, exp) in base_groups {
        if exp == Node::Number(1.0) {
            out.push(base);
        } else if exp != Node::Number(0.0) {
            out.push(Node::Pow(Box::new(base), Box::new(exp)).simplify());
        }
    }

    if out.is_empty() {
        Node::Number(1.0)
    } else if out.len() == 1 {
        out.pop().unwrap()
    } else {
        Node::Mul(out).simplify()
    }
}
