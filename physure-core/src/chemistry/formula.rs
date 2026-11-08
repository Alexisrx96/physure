//! Chemical formulas: `"Ca(NO3)2"` is `{Ca: 1, N: 2, O: 6}`.
//!
//! Ionic charges (`SO4^2-`) and aggregation states (`(aq)`) are not formulas of a compound but of
//! a species in a medium; they are refused by name, because dropping them would give a wrong
//! composition and so a wrong molar mass.

use std::collections::{BTreeMap, HashMap};

use crate::error::{PhysureError, PhysureResult};

/// Element symbol to the number of its atoms. Ordered by symbol.
pub type Composition = BTreeMap<String, u32>;

fn bad(formula: &str, why: &str) -> PhysureError {
    PhysureError::ParseError(format!("invalid formula {formula:?}: {why}"))
}

/// Unicode subscript digits (`₀`..`₉`) as ASCII digits.
pub fn subscripts_to_ascii(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '₀'..='₉' => char::from(b'0' + (c as u32 - '₀' as u32) as u8),
            other => other,
        })
        .collect()
}

/// The number at `at` in `bytes` and the position after it; `(1, at)` when there is none.
fn read_count(bytes: &[u8], at: usize, formula: &str) -> PhysureResult<(u32, usize)> {
    let digits = bytes[at..].iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return Ok((1, at));
    }
    let text = std::str::from_utf8(&bytes[at..at + digits]).expect("ASCII digits are UTF-8");
    let count: u32 = text.parse().map_err(|_| bad(formula, "a count is too large"))?;
    if count == 0 {
        return Err(bad(formula, "a count of zero"));
    }
    Ok((count, at + digits))
}

fn add(into: &mut Composition, element: String, n: u32, formula: &str) -> PhysureResult<()> {
    let slot = into.entry(element).or_insert(0);
    *slot = slot.checked_add(n).ok_or_else(|| bad(formula, "a count is too large"))?;
    Ok(())
}

/// One part of a formula, without a hydrate dot: element symbols, counts and parentheses.
fn parse_group(part: &str, formula: &str, weights: &HashMap<String, (f64, f64)>) -> PhysureResult<Composition> {
    if part.is_empty() {
        return Err(bad(formula, "nothing between the separators"));
    }
    if let Some(c) = part.chars().find(|c| !c.is_ascii()) {
        return Err(bad(formula, &format!("unsupported character {c:?}")));
    }
    let bytes = part.as_bytes();
    let mut stack = vec![Composition::new()];
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => {
                stack.push(Composition::new());
                i += 1;
            }
            b')' => {
                let (times, next) = read_count(bytes, i + 1, formula)?;
                i = next;
                if stack.len() < 2 {
                    return Err(bad(formula, "unbalanced parentheses"));
                }
                let inner = stack.pop().expect("two or more groups");
                if inner.is_empty() {
                    return Err(bad(formula, "empty parentheses"));
                }
                let outer = stack.last_mut().expect("the outer group");
                for (element, n) in inner {
                    let n = n.checked_mul(times).ok_or_else(|| bad(formula, "a count is too large"))?;
                    add(outer, element, n, formula)?;
                }
            }
            b'A'..=b'Z' => {
                let start = i;
                i += 1;
                if i < bytes.len() && bytes[i].is_ascii_lowercase() {
                    i += 1;
                }
                let symbol = &part[start..i];
                if !weights.contains_key(symbol) {
                    return Err(bad(formula, &format!("unknown element {symbol:?}")));
                }
                let (n, next) = read_count(bytes, i, formula)?;
                i = next;
                add(stack.last_mut().expect("the outer group"), symbol.to_string(), n, formula)?;
            }
            other => {
                return Err(bad(formula, &format!("unsupported character '{}'", other as char)));
            }
        }
    }
    if stack.len() != 1 {
        return Err(bad(formula, "unbalanced parentheses"));
    }
    Ok(stack.pop().expect("the outer group"))
}

/// The atoms of a formula by element. `·` or `*` separates a hydrate, with an optional number in
/// front of the part after it (`CuSO4·5H2O`).
pub fn parse_formula(formula: &str) -> PhysureResult<Composition> {
    let text = subscripts_to_ascii(formula);
    for state in ["(aq)", "(s)", "(l)", "(g)"] {
        if text.contains(state) {
            return Err(bad(formula, &format!("the aggregation state {state} is not supported")));
        }
    }
    // the catalog is resolved once, not once per element
    super::with_catalog_in_scope(|registry| {
        let mut total = Composition::new();
        for (index, part) in text.split(['·', '*']).enumerate() {
            let digits = part.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && index == 0 {
                return Err(bad(formula, "a formula cannot start with a number"));
            }
            let (times, _) = read_count(part.as_bytes(), 0, formula)?;
            for (element, n) in parse_group(&part[digits..], formula, &registry.atomic_weights)? {
                let n = n.checked_mul(times).ok_or_else(|| bad(formula, "a count is too large"))?;
                add(&mut total, element, n, formula)?;
            }
        }
        Ok(total)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp(pairs: &[(&str, u32)]) -> Composition {
        pairs.iter().map(|&(e, n)| (e.to_string(), n)).collect()
    }

    #[test]
    fn plain_formulas() {
        assert_eq!(parse_formula("H2O").unwrap(), comp(&[("H", 2), ("O", 1)]));
        assert_eq!(parse_formula("NaCl").unwrap(), comp(&[("Cl", 1), ("Na", 1)]));
        assert_eq!(parse_formula("C6H12O6").unwrap(), comp(&[("C", 6), ("H", 12), ("O", 6)]));
    }

    #[test]
    fn groups_multiply_and_nest() {
        assert_eq!(parse_formula("Ca(NO3)2").unwrap(), comp(&[("Ca", 1), ("N", 2), ("O", 6)]));
        assert_eq!(
            parse_formula("Mg3(Fe(CN)6)2").unwrap(),
            comp(&[("C", 12), ("Fe", 2), ("Mg", 3), ("N", 12)])
        );
    }

    #[test]
    fn unicode_subscripts_are_digits() {
        assert_eq!(parse_formula("H₂O").unwrap(), parse_formula("H2O").unwrap());
        assert_eq!(parse_formula("C₆H₁₂O₆").unwrap(), parse_formula("C6H12O6").unwrap());
    }

    #[test]
    fn a_hydrate_adds_its_water_times_the_number_in_front() {
        let expected = comp(&[("Cu", 1), ("H", 10), ("O", 9), ("S", 1)]);
        assert_eq!(parse_formula("CuSO4·5H2O").unwrap(), expected);
        assert_eq!(parse_formula("CuSO4*5H2O").unwrap(), expected);
        // no number in front means one
        assert_eq!(parse_formula("CuSO4·H2O").unwrap(), comp(&[("Cu", 1), ("H", 2), ("O", 5), ("S", 1)]));
    }

    fn message(formula: &str) -> String {
        parse_formula(formula).unwrap_err().to_string()
    }

    #[test]
    fn what_is_not_a_formula_is_refused() {
        for bad in ["", "H2O)", "(H2O", "()", "Xx3", "h2o", "H2 O", "2H2O", "H2O·", "·H2O"] {
            assert!(parse_formula(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn a_count_of_zero_is_refused_not_kept_as_an_empty_species() {
        assert!(message("H0").contains("zero"));
        assert!(message("(OH)0").contains("zero"));
    }

    #[test]
    fn charges_and_aggregation_states_are_named_not_dropped() {
        assert!(message("SO4^2-").contains("'^'"));
        assert!(message("NaCl(aq)").contains("(aq)"));
        assert!(message("H2O(l)").contains("(l)"));
    }

    #[test]
    fn an_element_of_the_catalog_in_scope_is_known_and_one_it_lacks_is_not() {
        use crate::units::conf::{build_registry_from_conf, extend_registry};
        assert!(message("D2O").contains("\"D\""), "refused by default");
        let (mut registry, mut constants) = build_registry_from_conf();
        extend_registry(&mut registry, &mut constants, "[AtomicWeights]
D = 2.01410177812, 0.00000000012, standard
").unwrap();
        let heavy = crate::units::with_catalog(&registry, || parse_formula("D2O")).unwrap();
        assert_eq!(heavy, comp(&[("D", 2), ("O", 1)]));
        registry.atomic_weights.remove("O");
        let err = crate::units::with_catalog(&registry, || parse_formula("H2O")).unwrap_err();
        assert!(err.to_string().contains("\"O\""), "{err}");
    }

    #[test]
    fn an_unknown_element_is_named() {
        assert!(message("Xx3").contains("Xx"));
    }

    #[test]
    fn a_count_that_does_not_fit_is_an_error_not_a_wrap() {
        assert!(parse_formula("H4294967295H").is_err());
        assert!(parse_formula("(H4294967295)2").is_err());
    }
}
