//! The `[AtomicWeights]` section of physure.conf held to the CIAAW Standard Atomic Weights 2024 as printed
//! (`tests/golden/ciaaw_standard_atomic_weights_2024.tsv`, a transcription of https://ciaaw.org/atomic-weights.htm).
//!
//! The expected `(weight, standard uncertainty)` of every row is derived here, by a small parser that shares
//! nothing with the conf reader or with `scripts/gen_atomic_weights_conf.py`, from the reading rules of
//! `docs/atomic_weights.md`:
//!
//! - `x(n)`: weight `x`, half-width `U` = `n` units of the last digit shown, standard uncertainty `U / sqrt(3)`;
//! - `[a, b]`: weight `(a + b) / 2`, standard uncertainty `(b - a) / sqrt(12)` (IUPAC);
//! - a dash: the mass number of the last column and an uncertainty of 0.

use physure_core::units::conf::build_registry_from_conf;

const CIAAW: &str = include_str!("../../tests/golden/ciaaw_standard_atomic_weights_2024.tsv");

/// The data rows: `[Z, symbol, as printed, footnotes (if any), mass number (dashes only)]`.
fn rows() -> Vec<Vec<&'static str>> {
    CIAAW.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(|l| l.split('\t').collect()).collect()
}

/// What the published value reads as: `(weight, standard uncertainty)`.
fn expected(printed: &str, mass_number: Option<&str>) -> (f64, f64) {
    let text: String = printed.chars().filter(|c| *c != ' ').collect();
    if text == "-" {
        return (mass_number.expect("a dash needs a mass number").parse().unwrap(), 0.0);
    }
    if let Some(inside) = text.strip_prefix('[') {
        let (low, high) = inside.strip_suffix(']').unwrap().split_once(',').unwrap();
        let (low, high): (f64, f64) = (low.parse().unwrap(), high.parse().unwrap());
        return ((low + high) / 2.0, (high - low) / 12f64.sqrt());
    }
    let (value, digits) = text.strip_suffix(')').unwrap().split_once('(').unwrap();
    let decimals = value.split_once('.').map_or(0, |(_, after)| after.len());
    let half_width = digits.parse::<f64>().unwrap() * 10f64.powi(-i32::try_from(decimals).unwrap());
    (value.parse().unwrap(), half_width / 3f64.sqrt())
}

fn close(got: f64, want: f64) -> bool {
    (got - want).abs() <= 1e-12 * want.abs()
}

#[test]
fn the_table_has_the_shape_the_cross_check_found() {
    let rows = rows();
    assert_eq!(rows.len(), 118);
    assert_eq!(rows.iter().filter(|r| r[2].starts_with('[')).count(), 14, "intervals");
    assert_eq!(rows.iter().filter(|r| r[2] == "-").count(), 34, "no standard atomic weight");
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row[0].parse::<usize>().unwrap(), i + 1, "atomic numbers run 1..=118 in order");
    }
    let find = |symbol: &str| rows.iter().find(|r| r[1] == symbol).unwrap()[2];
    assert_eq!(find("H"), "[1.007 84, 1.008 11]");
    assert_eq!(find("Na"), "22.989 769 28(2)");
}

#[test]
fn every_element_of_the_conf_is_the_published_one() {
    let (registry, _) = build_registry_from_conf();
    let rows = rows();
    assert_eq!(registry.atomic_weights.len(), rows.len(), "the conf has exactly the elements of the table");
    for row in &rows {
        let symbol = row[1];
        let (weight, std) = expected(row[2], row.get(4).copied());
        let &(got_weight, got_std) =
            registry.atomic_weights.get(symbol).unwrap_or_else(|| panic!("{symbol} is not in the conf"));
        if row[2] == "-" {
            assert_eq!((got_weight, got_std), (weight, 0.0), "{symbol}: mass number, no uncertainty");
        } else {
            assert!(close(got_weight, weight), "{symbol}: weight {got_weight} vs {weight}");
            assert!(close(got_std, std), "{symbol}: standard uncertainty {got_std} vs {std}");
        }
    }
}
