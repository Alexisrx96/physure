//! What the core says about a unit is what Python said before the core could (A1, stage 1c).
//!
//! `physure-python/tests/core/golden/unit_semantics.json` freezes the dimension, kind, linearity, alias
//! and simplified form that Python gave for 32 representative unit expressions. The core has to
//! reproduce them.

use physure_core::units::conf::build_registry_from_conf;
use physure_core::units::UnitKind;
use serde_json::Value;

/// Python's catalog restates celsius and fahrenheit (`international.conf`) with another symbol,
/// `°C`, so Python shows `degC` as written; in the core catalog `degC` is the symbol of the unit and
/// is shown by its shortest name. Closed when Python has one catalog.
const ALIAS_KNOWN_TO_DIFFER: [(&str, &str); 2] = [("degC", "°C"), ("degF", "°F")];

#[test]
fn the_core_says_what_python_said_before_it_could() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../physure-python/tests/core/golden/unit_semantics.json");
    let golden: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (registry, _) = build_registry_from_conf();
    let expressions = golden.as_object().unwrap();
    assert!(expressions.len() >= 30, "the golden file should hold the 32 expressions");
    for (expression, frozen) in expressions {
        let facts = registry.facts(expression).unwrap_or_else(|e| panic!("{expression}: {e}"));
        let dimension = frozen["dimension"].as_object().unwrap();
        assert_eq!(facts.dimension.len(), dimension.len(), "{expression}: dimension");
        for (symbol, (n, d)) in &facts.dimension {
            assert_eq!(dimension[symbol].as_f64().unwrap(), *n as f64 / *d as f64, "{expression}: {symbol}");
        }
        let kind = if facts.kind == UnitKind::Point { "absolute" } else { "delta" };
        assert_eq!(frozen["kind"], kind, "{expression}: kind");
        assert_eq!(frozen["is_linear"], facts.is_linear, "{expression}: is_linear");
        assert_eq!(frozen["simplified"], facts.simplified.as_str(), "{expression}: simplified");
        match ALIAS_KNOWN_TO_DIFFER.iter().find(|(name, _)| name == expression) {
            Some((_, core)) => assert_eq!(facts.alias, *core, "{expression}: alias"),
            None => assert_eq!(frozen["alias"], facts.alias.as_str(), "{expression}: alias"),
        }
    }
}
