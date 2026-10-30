//! One rule for how a power is written, and every unit the core writes is a unit it reads back.
//!
//! The same fractional power was `m⁰⋅⁵` in the core and in a Python class, `m^1/2` in another, `T^(1/2)` in a
//! third, and a third of a power was the decimal `s⁰⋅³³³³³³³³³³³³³³³³`: not the exponent, and a text that
//! the parser refuses. A whole power is superscript digits, a fractional power is `^(n/d)`.

use physure_core::units::conf::build_registry_from_conf;
use physure_core::units::parser::Parser;
use physure_core::units::{formula_text, power_text, unit_text};

fn atomic(text: &str) -> physure_core::units::RationalUnit {
    Parser::parse_expression_atomic(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

#[test]
fn a_whole_power_is_superscript_digits_and_a_fraction_is_exact_in_parentheses() {
    assert_eq!(power_text(2, 1), "²");
    assert_eq!(power_text(-1, 1), "⁻¹");
    assert_eq!(power_text(12, 1), "¹²");
    assert_eq!(power_text(1, 2), "^(1/2)");
    assert_eq!(power_text(-1, 3), "^(-1/3)");
    assert_eq!(power_text(3, 2), "^(3/2)");
}

#[test]
fn a_unit_is_written_with_that_rule() {
    let show = |text: &str| atomic(text).symbolic_repr();
    assert_eq!(show("m^2"), "m²");
    assert_eq!(show("kg*m/s^2"), "kg·m/s²");
    assert_eq!(show("kg/(m*s^2)"), "kg/(m·s²)");
    assert_eq!(show("s^-1"), "1/s");
    assert_eq!(show("m^0.5"), "m^(1/2)");
    assert_eq!(show("s^(1/3)"), "s^(1/3)", "a third stays a third");
    assert_eq!(show("s^-0.5"), "1/s^(1/2)");
    assert_eq!(show("kg^0.25*m/s^0.5"), "kg^(1/4)·m/s^(1/2)");
    assert_eq!(show("m^(-3/2)"), "1/m^(3/2)");
    assert_eq!(show("1"), "1");
}

#[test]
fn a_dimension_is_written_with_the_same_power_rule() {
    let formula = |dims: &[(&str, i64, i64)]| formula_text(&dims.iter().map(|(s, n, d)| (s.to_string(), (*n, *d))).collect::<Vec<_>>());
    assert_eq!(formula(&[("L", 2, 1), ("M", 1, 1), ("T", -2, 1)]), "L²·M·T⁻²");
    assert_eq!(formula(&[("L", 1, 2)]), "L^(1/2)");
    assert_eq!(formula(&[("L", 1, 1), ("M", 1, 4), ("T", -1, 2)]), "L·M^(1/4)·T^(-1/2)");
    assert_eq!(unit_text(&[("m".to_string(), (1, 2))]), "m^(1/2)");
}

/// For every name of the catalog: the unit written as symbols reads back as the same dimensions.
#[test]
fn every_unit_of_the_catalog_is_written_in_a_way_the_parser_reads_back() {
    let (registry, _) = build_registry_from_conf();
    let names = registry.resolvable_names();
    assert!(names.len() > 5000, "{}", names.len());
    let mut broken = Vec::new();
    for name in &names {
        let Ok(unit) = Parser::parse_expression_atomic(name) else { continue };
        let written = unit.symbolic_repr();
        match Parser::parse_expression_atomic(&written) {
            Ok(read) if read.dimensions == unit.dimensions => {}
            Ok(read) => broken.push(format!("{name}: {written} reads as {:?}", read.dimensions)),
            Err(error) => broken.push(format!("{name}: {written} ({error})")),
        }
    }
    assert!(broken.is_empty(), "{} of {} names: {}", broken.len(), names.len(), broken.iter().take(10).cloned().collect::<Vec<_>>().join("; "));
}

/// The "simplified" form that `facts` reports is the same text, so a caller can feed it back in.
#[test]
fn the_simplified_form_of_a_unit_is_a_unit() {
    let (registry, _) = build_registry_from_conf();
    for expression in ["m^0.5", "s^(1/3)", "Hz^0.5", "kg^0.25*m/s^0.5", "kg*m^2/s^2", "km/h", "N*m"] {
        let facts = registry.facts(expression).unwrap();
        let again = registry.facts(&facts.simplified).unwrap_or_else(|e| panic!("{expression} simplified to {:?}: {e}", facts.simplified));
        assert_eq!(again.dimension, facts.dimension, "{expression} -> {}", facts.simplified);
    }
}

#[test]
fn a_third_is_not_a_decimal_anywhere() {
    assert!(!atomic("s^(1/3)").symbolic_repr().contains('⋅'));
    let quantity = physure_core::quantity::Quantity::new(27.0, "m").unwrap().pow(1.0 / 3.0).unwrap();
    let (_, _, unit) = quantity.display_parts();
    assert!(!unit.contains("3333"), "{unit}");
}

// ---- the micro sign

#[test]
fn the_micro_sign_in_either_spelling_is_the_prefix_u() {
    let scale = |text: &str| physure_core::quantity::Quantity::new(1.0, text).unwrap().unit.scale;
    let micro_sign = "\u{b5}m"; // MICRO SIGN, what a keyboard types
    let greek_mu = "\u{3bc}m"; // GREEK SMALL LETTER MU
    assert_eq!(scale("um"), 1e-6);
    assert_eq!(scale(micro_sign), 1e-6);
    assert_eq!(scale(greek_mu), 1e-6);
    assert_eq!(scale("\u{b5}s"), 1e-6);
    assert!((scale("\u{b5}g") - 1e-9).abs() < 1e-24, "µg is a microgram");
    // the text it was written with is the text it keeps
    assert_eq!(physure_core::quantity::Quantity::new(1.0, micro_sign).unwrap().unit.__repr__(), micro_sign);
}

#[test]
fn the_micro_sign_is_only_a_prefix() {
    let (registry, _) = build_registry_from_conf();
    assert!(!registry.knows("\u{b5}"));
    assert!(!registry.knows("\u{3bc}"));
    assert!(!registry.knows("\u{b5}nosuchunit"));
    // a prefix has nothing to scale on a level, as with `u`
    assert_eq!(registry.knows("udB"), registry.knows("\u{b5}dB"));
}
