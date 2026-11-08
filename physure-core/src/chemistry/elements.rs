//! Standard atomic weights, read from the `[AtomicWeights]` section of `physure.conf` (the CIAAW Standard
//! Atomic Weights 2024, intervals kept as intervals; a conf of your own or a unit system can replace or add an
//! element). See `docs/atomic_weights.md`.

/// `(weight in g/mol, standard uncertainty)` of an element by symbol, from the catalog in scope.
pub fn atomic_weight(symbol: &str) -> Option<(f64, f64)> {
    super::with_catalog_in_scope(|registry| registry.atomic_weights.get(symbol).copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::conf::{build_registry_from_conf, extend_registry};

    #[test]
    fn known_weights() {
        // H is the interval [1.00784, 1.00811]: midpoint, and (b - a) / sqrt(12) (IUPAC)
        assert_eq!(atomic_weight("H"), Some(((1.00784 + 1.00811) / 2.0, (1.00811 - 1.00784) / 12f64.sqrt())));
        // Fe is 55.845(2): half-width 0.002, standard uncertainty 0.002 / sqrt(3)
        assert_eq!(atomic_weight("Fe"), Some((55.845, 0.002 / 3f64.sqrt())));
        assert_eq!(atomic_weight("Og"), Some((294.0, 0.0)));
    }

    #[test]
    fn a_symbol_that_is_not_an_element_is_none() {
        assert_eq!(atomic_weight("Xx"), None);
        assert_eq!(atomic_weight("h"), None);
        assert_eq!(atomic_weight(""), None);
    }

    #[test]
    fn an_element_of_the_catalog_in_scope_is_found_and_only_there() {
        let (mut registry, mut constants) = build_registry_from_conf();
        extend_registry(&mut registry, &mut constants, "[AtomicWeights]
D = 2.01410177812, 0.00000000012, standard
").unwrap();
        let inside = crate::units::with_catalog(&registry, || atomic_weight("D"));
        assert_eq!(inside, Some((2.01410177812, 0.00000000012)));
        assert_eq!(atomic_weight("D"), None);
    }
}
