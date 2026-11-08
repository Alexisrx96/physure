//! The two shipped editions of physure.conf held to their transcriptions of the CIAAW pages, and frozen:
//!
//! - `[AtomicWeights:standard-2024]` to `tests/golden/ciaaw_standard_atomic_weights_2024.tsv` (https://ciaaw.org/atomic-weights.htm);
//! - `[AtomicWeights:abridged-2024]` to `tests/golden/ciaaw_abridged_atomic_weights_2024.tsv`
//!   (https://ciaaw.org/abridged-atomic-weights.htm).
//!
//! A published edition is never edited: a new CIAAW release is a new edition next to these. The two fingerprint
//! tests at the end fail if either shipped table changes by a single bit, so that a plant that validated its
//! calculations against `standard-2024` or `abridged-2024` can rely on the numbers staying what they were.
//!
//! The expected `(weight, standard uncertainty)` of every row is derived here, by a small parser that shares
//! nothing with the conf reader or with `scripts/gen_atomic_weights_conf.py`, from the reading rules of
//! `docs/atomic_weights.md`:
//!
//! - standard `x(n)`: weight `x`, half-width `U` = `n` units of the last digit shown, standard uncertainty `U / sqrt(3)`;
//! - standard `[a, b]`: weight `(a + b) / 2`, standard uncertainty `(b - a) / sqrt(12)` (IUPAC);
//! - abridged `x`, `±`: weight `x`, half-width `U` = the printed `±`, standard uncertainty `U / sqrt(3)`;
//! - a dash (either table): the mass number of the last column and an uncertainty of 0.

use physure_core::units::conf::build_registry_from_conf;

const STANDARD: &str = include_str!("../../tests/golden/ciaaw_standard_atomic_weights_2024.tsv");
const ABRIDGED: &str = include_str!("../../tests/golden/ciaaw_abridged_atomic_weights_2024.tsv");

/// The data rows of a TSV, split into fields.
fn rows_of(tsv: &'static str) -> Vec<Vec<&'static str>> {
    tsv.lines().filter(|l| !l.is_empty() && !l.starts_with('#')).map(|l| l.split('\t').collect()).collect()
}

/// Standard rows: `[Z, symbol, as printed, footnotes (if any), mass number (dashes only)]`.
fn standard_rows() -> Vec<Vec<&'static str>> {
    rows_of(STANDARD)
}

/// Abridged rows: `[Z, symbol, value as printed, "±" as printed, footnotes, mass number (dashes only)]`.
fn abridged_rows() -> Vec<Vec<&'static str>> {
    rows_of(ABRIDGED)
}

/// What a standard row reads as: `(weight, standard uncertainty)`.
fn expected_standard(printed: &str, mass_number: Option<&str>) -> (f64, f64) {
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

/// What an abridged row reads as: `(weight, standard uncertainty)`.
fn expected_abridged(value: &str, plus_minus: &str, mass_number: Option<&str>) -> (f64, f64) {
    if value == "-" {
        assert_eq!(plus_minus, "-", "a dash has no ± either");
        return (mass_number.expect("a dash needs a mass number").parse().unwrap(), 0.0);
    }
    (value.parse().unwrap(), plus_minus.parse::<f64>().unwrap() / 3f64.sqrt())
}

fn close(got: f64, want: f64) -> bool {
    (got - want).abs() <= 1e-12 * want.abs()
}

#[test]
fn the_standard_table_has_the_shape_the_cross_check_found() {
    let rows = standard_rows();
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
fn every_element_of_standard_2024_is_the_published_one() {
    let (registry, _) = build_registry_from_conf();
    let rows = standard_rows();
    assert_eq!(registry.atomic_weight_tables["standard-2024"].len(), rows.len(), "the section has exactly the elements of the table");
    for row in &rows {
        let symbol = row[1];
        let (weight, std) = expected_standard(row[2], row.get(4).copied());
        let &(got_weight, got_std) = registry.atomic_weight_tables["standard-2024"]
            .get(symbol)
            .unwrap_or_else(|| panic!("{symbol} is not in [AtomicWeights:standard-2024]"));
        if row[2] == "-" {
            assert_eq!((got_weight, got_std), (weight, 0.0), "{symbol}: mass number, no uncertainty");
        } else {
            assert!(close(got_weight, weight), "{symbol}: weight {got_weight} vs {weight}");
            assert!(close(got_std, std), "{symbol}: standard uncertainty {got_std} vs {std}");
        }
    }
}

#[test]
fn the_abridged_table_has_the_shape_the_cross_check_found() {
    let rows = abridged_rows();
    assert_eq!(rows.len(), 118);
    assert_eq!(rows.iter().filter(|r| r[2] == "-").count(), 34, "no abridged atomic weight");
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row[0].parse::<usize>().unwrap(), i + 1, "atomic numbers run 1..=118 in order");
        assert_eq!(row.len(), 6, "{}: six columns, the last empty when there is no mass number", row[1]);
    }
    let find = |symbol: &str| {
        let row = rows.iter().find(|r| r[1] == symbol).unwrap();
        (row[2], row[3])
    };
    assert_eq!(find("H"), ("1.0080", "0.0002"));
    assert_eq!(find("Pb"), ("207.2", "1.1"));
}

#[test]
fn every_element_of_abridged_2024_is_the_published_one() {
    let (registry, _) = build_registry_from_conf();
    let rows = abridged_rows();
    assert_eq!(registry.atomic_weight_tables["abridged-2024"].len(), rows.len(), "the section has exactly the elements of the table");
    for row in &rows {
        let symbol = row[1];
        let mass_number = Some(row[5]).filter(|m| !m.is_empty());
        let (weight, std) = expected_abridged(row[2], row[3], mass_number);
        let &(got_weight, got_std) = registry.atomic_weight_tables["abridged-2024"]
            .get(symbol)
            .unwrap_or_else(|| panic!("{symbol} is not in [AtomicWeights:abridged-2024]"));
        if row[2] == "-" {
            assert_eq!((got_weight, got_std), (weight, 0.0), "{symbol}: mass number, no uncertainty");
        } else {
            assert!(close(got_weight, weight), "{symbol}: weight {got_weight} vs {weight}");
            assert!(close(got_std, std), "{symbol}: standard uncertainty {got_std} vs {std}");
        }
    }
}

#[test]
fn the_34_mass_numbers_of_the_two_tables_agree() {
    let standard: Vec<(&str, &str)> = standard_rows().iter().filter(|r| r[2] == "-").map(|r| (r[1], r[4])).collect();
    let abridged: Vec<(&str, &str)> = abridged_rows().iter().filter(|r| r[2] == "-").map(|r| (r[1], r[5])).collect();
    assert_eq!(standard.len(), 34);
    assert_eq!(standard, abridged);
}

/// `value` (a plain decimal, at least 1) rounded half-up to five significant figures, as text with its
/// trailing zeros: 22.98976928 -> "22.990", 173.045 -> "173.05". Done on the digits, so there is no float
/// in it.
fn five_figures_half_up(value: &str) -> String {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    assert!(!whole.starts_with('0') && whole.len() <= 5, "{value}: the digits below are laid out for 1 <= x < 100000");
    let digits = format!("{whole}{fraction}");
    let padded = format!("{digits:0<6}");
    let mut kept: u64 = padded[..5].parse().unwrap();
    if padded.as_bytes()[5] >= b'5' {
        kept += 1;
    }
    assert!(kept < 100_000, "{value}: rounding carries into a sixth digit");
    let kept = kept.to_string();
    if whole.len() == 5 { kept } else { format!("{}.{}", &kept[..whole.len()], &kept[whole.len()..]) }
}

#[test]
fn every_single_standard_value_rounds_to_the_abridged_value() {
    let standard = standard_rows();
    let abridged = abridged_rows();
    let mut checked = 0;
    for row in &standard {
        let printed = row[2];
        if printed == "-" || printed.starts_with('[') {
            continue; // an interval has no single value to round; a dash has none either
        }
        let text: String = printed.chars().filter(|c| *c != ' ').collect();
        let value = text.split_once('(').unwrap().0;
        let abridged_row = abridged.iter().find(|r| r[1] == row[1]).unwrap();
        // compared as numbers: CIAAW prints 65.38 for Zn (the standard value has four figures) but 22.990 for Na
        let rounded = five_figures_half_up(value);
        assert_eq!(
            abridged_row[2].parse::<f64>().unwrap(),
            rounded.parse::<f64>().unwrap(),
            "{}: standard {printed} rounds half-up to {rounded}, the abridged table prints {}",
            row[1],
            abridged_row[2]
        );
        checked += 1;
    }
    assert_eq!(checked, 118 - 14 - 34, "the 70 elements with a single standard value");
}

/// FNV-1a, 64 bits (offset basis `0xcbf29ce484222325`, prime `0x100000001b3`).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// The canonical text of an edition: `Sym=<weight bits in hex>,<standard uncertainty bits in hex>;` for every
/// element in symbol order, the bits being the IEEE-754 binary64 pattern as 16 lower-case hex digits.
fn canonical_text(edition: &str) -> String {
    let (registry, _) = build_registry_from_conf();
    let mut table: Vec<(&String, &(f64, f64))> = registry.atomic_weight_tables[edition].iter().collect();
    table.sort_by_key(|(symbol, _)| *symbol);
    table.iter().map(|(symbol, (weight, std))| format!("{symbol}={:016x},{:016x};", weight.to_bits(), std.to_bits())).collect()
}

const FROZEN: &str = "A published edition is frozen: its numbers must never change, because a plant may have validated its calculations against them. Do not edit the section or the constant; add a NEW edition (for example `standard-2026`) next to this one, generated from its own table, and move an alias only in a release whose CHANGELOG says so.";

// The constants below were computed from the TSVs with a separate Python script (the same canonical text,
// `struct.pack('>d', x).hex()` for the bits), not from this code: the Rust and the Python values agree.

#[test]
fn standard_2024_is_frozen() {
    let text = canonical_text("standard-2024");
    assert_eq!(text.len(), 4352, "118 elements; the fingerprint below is of this text");
    assert_eq!(fnv1a64(text.as_bytes()), 0xc92e_208b_c767_774b, "the standard-2024 table changed. {FROZEN}");
}

#[test]
fn abridged_2024_is_frozen() {
    let text = canonical_text("abridged-2024");
    assert_eq!(text.len(), 4352, "118 elements; the fingerprint below is of this text");
    assert_eq!(fnv1a64(text.as_bytes()), 0xac3e_57bf_7704_52a6, "the abridged-2024 table changed. {FROZEN}");
}
