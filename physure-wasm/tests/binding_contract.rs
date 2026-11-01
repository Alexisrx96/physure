//! What the core says about a unit is what the WASM binding returns (F3).
//!
//! Reads `tests/golden/unit_dimension_scale.tsv`, the file that `physure-core` and the Java binding are
//! held to as well, and checks the exponents and the scale after they cross the wasm-bindgen boundary.
//! Run with `wasm-pack test --node`.

use physure_wasm::UnitRegistry;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

const GOLDEN: &str = include_str!("../../tests/golden/unit_dimension_scale.tsv");

/// `symbol:exponent;...` as the golden file writes it; `-` is no exponents at all.
fn expected_exponents(field: &str) -> Vec<(&str, f64)> {
    if field == "-" {
        return Vec::new();
    }
    field
        .split(';')
        .map(|pair| {
            let (symbol, exponent) = pair.split_once(':').expect("symbol:exponent");
            (symbol, exponent.parse().expect("an f64 exponent"))
        })
        .collect()
}

/// What the binding gets wrong about one row of the golden file.
fn disagreements(registry: &UnitRegistry, expression: &str, scale: f64, exponents: &[(&str, f64)]) -> Vec<String> {
    let mut failures = Vec::new();
    match registry.get_unit_scale(expression) {
        Ok(got) if got == scale => {}
        other => failures.push(format!("{expression}: scale {other:?}, expected {scale:?}")),
    }
    match registry.get_unit_exponents(expression) {
        Ok(map) => {
            if map.size() as usize != exponents.len() {
                failures.push(format!("{expression}: {} exponents, expected {}", map.size(), exponents.len()));
            }
            for (symbol, exponent) in exponents {
                let got = map.get(&JsValue::from_str(symbol)).as_f64();
                if got != Some(*exponent) {
                    failures.push(format!("{expression}: {symbol} is {got:?}, expected {exponent:?}"));
                }
            }
        }
        Err(e) => failures.push(format!("{expression}: exponents failed with {e:?}")),
    }
    failures
}

#[wasm_bindgen_test]
fn every_row_of_the_golden_file_comes_back_unchanged() {
    let registry = UnitRegistry::new();
    let mut failures = Vec::new();
    let mut rows = 0;
    for line in GOLDEN.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let fields: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
        let (expression, scale, exponents) = (fields[0], fields[1].parse::<f64>().unwrap(), expected_exponents(fields[2]));
        rows += 1;
        failures.extend(disagreements(&registry, expression, scale, &exponents));
    }
    assert!(rows >= 40, "the golden file should hold its rows, found {rows}");
    assert!(failures.is_empty(), "{} disagreements with the golden file:\n{}", failures.len(), failures.join("\n"));
}
