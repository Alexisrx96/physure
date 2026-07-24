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
    let mut common_counts: Vec<(Node, usize)> = Vec::new();
    if let Some(first_term) = term_factors.first() {
        for factor in first_term {
            if !matches!(factor, Node::Number(_)) {
                count_vec_add(&mut common_counts, factor);
            }
        }
        for term in &term_factors[1..] {
            let mut current_counts: Vec<(Node, usize)> = Vec::new();
            for factor in term {
                if !matches!(factor, Node::Number(_)) {
                    count_vec_add(&mut current_counts, factor);
                }
            }
            for (node, count) in common_counts.iter_mut() {
                let current_cnt = count_vec_get(&current_counts, node);
                *count = (*count).min(current_cnt);
            }
        }
    }

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
    let mut remaining_terms = Vec::new();
    for term in term_factors {
        let mut rem = term;
        for sf in &shared_factors {
            if let Some(pos) = rem.iter().position(|x| x == sf) {
                rem.remove(pos);
            }
        }
        if rem.is_empty() {
            remaining_terms.push(Node::Number(1.0));
        } else if rem.len() == 1 {
            remaining_terms.push(rem.pop().unwrap());
        } else {
            remaining_terms.push(Node::Mul(rem).simplify());
        }
    }

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
