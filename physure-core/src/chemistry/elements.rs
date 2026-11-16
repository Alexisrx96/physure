//! Atomic weights, read from the catalog in scope: the definitions of the user (`[AtomicWeights]`) first, then
//! the table that `[Chemistry] atomic_weights` selects, a named edition (`[AtomicWeights:standard-2024]`, the CIAAW
//! Standard Atomic Weights 2024 with the intervals kept as intervals, by default; `[AtomicWeights:abridged-2024]`,
//! the CIAAW abridged table; or a complete table of your own). There is no fallback from one table to another. A
//! conf of your own or a unit system can replace or add an element, whichever table is selected. See
//! `docs/atomic_weights.md`.

use crate::error::{PhysureError, PhysureResult};
use crate::units::UnitRegistry;

/// `(weight in g/mol, standard uncertainty)` of an element by symbol, from the catalog in scope
/// ([`UnitRegistry::atomic_weight`]).
pub fn atomic_weight(symbol: &str) -> PhysureResult<Option<(f64, f64)>> {
    super::try_with_catalog_in_scope(|registry| registry.atomic_weight(symbol))
}

/// The table that produced the atomic weights of the catalog in scope, for a report to quote: the resolved
/// edition (`standard-2024`, or `nope-1 (undefined)` when no text defines the selected one), followed by
/// ` + N definitions (fnv1a:<hash>)` when the user defined any, the hash identifying them
/// ([`UnitRegistry::atomic_weight_edition`]).
pub fn atomic_weight_edition() -> PhysureResult<String> {
    super::try_with_catalog_in_scope(UnitRegistry::atomic_weight_edition)
}

/// An error naming the selected edition when no text has defined it: with no fallback to another table, every
/// use of the weights would otherwise fail one element at a time.
pub(crate) fn check_table_is_defined(registry: &UnitRegistry) -> PhysureResult<()> {
    match registry.selected_atomic_weight_table_missing() {
        None => Ok(()),
        Some(edition) => Err(PhysureError::DomainError(format!(
            "the atomic weight edition '{edition}' is selected by [Chemistry] atomic_weights but no [AtomicWeights:{edition}] section defines it"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::conf::{build_registry_with, extend_registry};

    /// Runs `f` against the shipped catalog and nothing else: a `physure.conf` in the working directory must not
    /// change what these tests assert about the shipped editions.
    fn shipped<R>(f: impl FnOnce() -> R) -> R {
        let (registry, _) = build_registry_with("").unwrap();
        crate::units::with_catalog(&registry, f)
    }

    #[test]
    fn known_weights() {
        shipped(|| {
            // H is the interval [1.00784, 1.00811]: midpoint, and (b - a) / sqrt(12) (IUPAC)
            assert_eq!(atomic_weight("H").unwrap(), Some(((1.00784 + 1.00811) / 2.0, (1.00811 - 1.00784) / 12f64.sqrt())));
            // Fe is 55.845(2): half-width 0.002, standard uncertainty 0.002 / sqrt(3)
            assert_eq!(atomic_weight("Fe").unwrap(), Some((55.845, 0.002 / 3f64.sqrt())));
            assert_eq!(atomic_weight("Og").unwrap(), Some((294.0, 0.0)));
        });
    }

    #[test]
    fn a_symbol_that_is_not_an_element_is_none() {
        shipped(|| {
            assert_eq!(atomic_weight("Xx").unwrap(), None);
            assert_eq!(atomic_weight("h").unwrap(), None);
            assert_eq!(atomic_weight("").unwrap(), None);
        });
    }

    #[test]
    fn an_element_of_the_catalog_in_scope_is_found_and_only_there() {
        let (mut registry, mut constants) = build_registry_with("").unwrap();
        extend_registry(&mut registry, &mut constants, "[AtomicWeights]\nD = 2.01410177812, 0.00000000012, standard\n")
            .unwrap();
        let inside = crate::units::with_catalog(&registry, || atomic_weight("D").unwrap());
        assert_eq!(inside, Some((2.01410177812, 0.00000000012)));
        assert_eq!(shipped(|| atomic_weight("D").unwrap()), None);
    }

    #[test]
    fn the_selected_edition_and_the_definitions_decide_the_weight() {
        let (mut registry, mut constants) = build_registry_with("").unwrap();
        extend_registry(
            &mut registry,
            &mut constants,
            "[Chemistry]\natomic_weights = abridged\n[AtomicWeights]\nFe = 56.0, 0.3\n",
        )
        .unwrap();
        // abridged-2024: Na 22.990 +- 0.001; the definition of Fe wins over the selected table
        assert_eq!(crate::units::with_catalog(&registry, || atomic_weight("Na").unwrap()), Some((22.990, 0.001 / 3f64.sqrt())));
        assert_eq!(crate::units::with_catalog(&registry, || atomic_weight("Fe").unwrap()), Some((56.0, 0.3 / 3f64.sqrt())));
        // outside the scope the default (standard-2024) is read again
        assert_eq!(shipped(|| atomic_weight("Na").unwrap()), Some((22.98976928, 0.00000002 / 3f64.sqrt())));
    }

    #[test]
    fn the_edition_report_names_the_table_and_the_number_of_definitions() {
        assert_eq!(shipped(atomic_weight_edition).unwrap(), "standard-2024");
        let (mut registry, mut constants) = build_registry_with("").unwrap();
        extend_registry(&mut registry, &mut constants, "[Chemistry]\natomic_weights = abridged\n[AtomicWeights]\nFe = 56.0, 0.3\n")
            .unwrap();
        assert_eq!(crate::units::with_catalog(&registry, atomic_weight_edition).unwrap(), "abridged-2024 + 1 definitions (fnv1a:e03dd5b5b2476b01)");
        assert_eq!(shipped(atomic_weight_edition).unwrap(), "standard-2024");
    }
}
