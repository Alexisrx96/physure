//! The answers the Java and WASM bindings are held to (F3).
//!
//! `tests/golden/unit_dimension_scale.tsv` freezes the base-unit exponents and the scale that the core
//! gives for a set of unit expressions. The Java test (`physure-java/src/test`) and the WASM test
//! (`physure-wasm/tests`) read the same file and check what each binding returns after its own
//! marshalling, so a change here, or a binding that rounds, truncates or drops something, fails a test.
//!
//! `PHYSURE_BLESS_GOLDEN=1` rewrites the file from the core's current answers (the rows and the header
//! stay); review the diff before committing it.

use physure_core::units::conf::build_registry_from_conf;
use physure_core::units::parser::Parser;

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/golden/unit_dimension_scale.tsv");

/// `symbol:exponent` pairs of a unit, each exponent the numerator over the denominator as an `f64`.
fn exponents_of(unit: &physure_core::units::RationalUnit) -> String {
    let pairs: Vec<String> = unit.dimensions.iter().map(|(symbol, (n, d))| format!("{symbol}:{:?}", *n as f64 / *d as f64)).collect();
    if pairs.is_empty() {
        "-".to_string()
    } else {
        pairs.join(";")
    }
}

#[test]
fn the_core_gives_the_answers_the_bindings_are_held_to() {
    let (registry, _) = build_registry_from_conf();
    let text = std::fs::read_to_string(GOLDEN).unwrap();
    let bless = std::env::var_os("PHYSURE_BLESS_GOLDEN").is_some();
    let mut rewritten = String::new();
    let mut rows = 0;
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            rewritten.push_str(line);
            rewritten.push('\n');
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 3, "a row is expression, scale, exponents: {line:?}");
        let expression = fields[0];
        let unit = Parser::parse_expression_with_registry(expression, &registry).unwrap_or_else(|e| panic!("{expression}: {e}"));
        let (scale, exponents) = (format!("{:?}", unit.scale), exponents_of(&unit));
        if !bless {
            assert_eq!(fields[1], scale, "{expression}: scale");
            assert_eq!(fields[2], exponents, "{expression}: exponents");
        }
        rewritten.push_str(&format!("{expression}\t{scale}\t{exponents}\n"));
        rows += 1;
    }
    assert!(rows >= 40, "the golden file should hold its 44 rows, found {rows}");
    if bless {
        std::fs::write(GOLDEN, rewritten).unwrap();
    }
}
