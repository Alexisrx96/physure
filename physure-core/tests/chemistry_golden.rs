//! The core against `tests/golden/chemistry_*.tsv`: what `physure.ext.chemistry` answered for the
//! molar masses of 118 elements and 20 compounds and for the coefficients of 13 reactions (and
//! which equations it refused), written by `scripts/gen_chemistry_golden.py`.

use physure_core::chemistry::{Reaction, Species};

const MOLAR_MASS: &str = include_str!("../../tests/golden/chemistry_molar_mass.tsv");
const REACTIONS: &str = include_str!("../../tests/golden/chemistry_reactions.tsv");

fn rows(text: &str) -> impl Iterator<Item = Vec<&str>> {
    text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(|l| l.split('\t').collect())
}

fn numbers(list: &str) -> Vec<i64> {
    list.split(',').map(|n| n.parse().unwrap()).collect()
}

#[test]
fn every_molar_mass_is_reproduced() {
    let mut count = 0;
    for row in rows(MOLAR_MASS) {
        let (formula, mass, std): (&str, f64, f64) = (row[0], row[1].parse().unwrap(), row[2].parse().unwrap());
        let (got_mass, got_std) = Species::new(formula).unwrap().molar_mass_parts();
        // the atoms are summed in a different order, so the last bits may differ
        assert!((got_mass - mass).abs() <= 1e-12 * mass.abs().max(1.0), "{formula}: {got_mass} vs {mass}");
        assert!((got_std - std).abs() <= 1e-12 * std.abs().max(1e-12), "{formula}: {got_std} vs {std}");
        count += 1;
    }
    assert_eq!(count, 138);
}

#[test]
fn every_reaction_is_balanced_or_refused_as_in_python() {
    let mut count = 0;
    for row in rows(REACTIONS) {
        let equation = row[0];
        match row[1] {
            "ERROR" => assert!(Reaction::from_string(equation).is_err(), "{equation:?} should be refused"),
            reversible => {
                let r = Reaction::from_string(equation).unwrap_or_else(|e| panic!("{equation:?}: {e}"));
                assert_eq!(r.reversible, reversible == "1", "{equation}");
                assert_eq!(r.reactant_coeffs, numbers(row[2]), "{equation}");
                assert_eq!(r.product_coeffs, numbers(row[3]), "{equation}");
            }
        }
        count += 1;
    }
    assert_eq!(count, 21);
}
