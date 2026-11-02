//! Degrees of freedom through the WASM binding (D4): declared on a measurement, combined by the core's lineage,
//! and read back as the effective degrees of freedom, the coverage factor and the expanded uncertainty.
//!
//! Reads `tests/golden/uncertainty_dof.tsv`, the file that `physure-core` and the Java binding are held to as
//! well. Run with `wasm-pack test --node`.

use physure_wasm::Quantity;
use wasm_bindgen_test::*;

const GOLDEN: &str = include_str!("../../tests/golden/uncertainty_dof.tsv");

fn number(text: &str) -> f64 {
    if text == "inf" {
        f64::INFINITY
    } else {
        text.parse().expect("a number")
    }
}

fn close(a: f64, b: f64, relative: f64) -> bool {
    a == b || ((a - b) / b).abs() < relative
}

/// The sum of the measurements of a budget (`mean:u:nu;...`), as a JavaScript caller builds it.
fn sum_of(budget: &str) -> Quantity {
    let mut sum: Option<Quantity> = None;
    for term in budget.split(';') {
        let parts: Vec<&str> = term.split(':').collect();
        let measurement = Quantity::with_uncertainty(number(parts[0]), number(parts[1]), "m").unwrap();
        let measurement = if number(parts[2]).is_finite() { measurement.with_dof(number(parts[2])).unwrap() } else { measurement };
        sum = Some(match sum {
            None => measurement,
            Some(total) => total.add(&measurement).unwrap(),
        });
    }
    sum.expect("a budget has a measurement")
}

#[wasm_bindgen_test]
fn every_row_of_the_golden_file_comes_back_unchanged() {
    let mut rows = 0;
    for line in GOLDEN.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let f: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
        let (name, p) = (f[0], number(f[2]));
        let sum = sum_of(f[1]);
        rows += 1;
        assert!(close(sum.uncertainty(), number(f[5]), 1e-12), "{name}: u_c {} vs {}", sum.uncertainty(), f[5]);
        assert!(close(sum.effective_dof().unwrap(), number(f[3]), 1e-12), "{name}: nu_eff {:?} vs {}", sum.effective_dof(), f[3]);
        assert!(close(sum.coverage_factor(p).unwrap(), number(f[4]), 1e-9), "{name}: k {:?} vs {}", sum.coverage_factor(p), f[4]);
        assert!(close(sum.expanded_uncertainty(p).unwrap(), number(f[6]), 1e-9), "{name}: U {:?} vs {}", sum.expanded_uncertainty(p), f[6]);
    }
    assert_eq!(rows, 11);
}

#[wasm_bindgen_test]
fn a_conversion_keeps_the_degrees_of_freedom_and_the_expanded_uncertainty_is_in_the_unit_shown() {
    let metres = Quantity::with_uncertainty(1.0, 0.3, "m").unwrap().with_dof(4.0).unwrap();
    let millimetres = metres.convert_to("mm").unwrap();
    assert!(close(millimetres.uncertainty(), 300.0, 1e-12));
    assert!(close(millimetres.effective_dof().unwrap(), 4.0, 1e-12));
    let k = millimetres.coverage_factor(0.95).unwrap();
    assert!(close(millimetres.expanded_uncertainty(0.95).unwrap(), k * 300.0, 1e-12));
    // the same measurement in another unit cancels against itself, and takes its degrees of freedom with it
    assert_eq!(metres.subtract(&metres).unwrap().effective_dof().unwrap(), f64::INFINITY);
}

#[wasm_bindgen_test]
fn a_type_a_evaluation_is_the_mean_the_standard_error_and_n_minus_one_degrees_of_freedom() {
    let v = Quantity::from_samples(vec![10.0, 10.2, 9.9, 10.1, 10.3], "V").unwrap();
    assert!(close(v.value(), 10.1, 1e-12));
    assert!(close(v.uncertainty(), (0.025f64 / 5.0).sqrt(), 1e-12));
    assert!(close(v.effective_dof().unwrap(), 4.0, 1e-12));
    let same = Quantity::from_samples(vec![2.0, 2.0, 2.0], "V").unwrap();
    assert_eq!((same.uncertainty(), same.effective_dof().unwrap()), (0.0, f64::INFINITY));
}

#[wasm_bindgen_test]
fn what_cannot_be_declared_or_asked_is_an_error() {
    let m = || Quantity::with_uncertainty(1.0, 0.1, "m").unwrap();
    assert!(Quantity::new(3.0, "m").unwrap().with_dof(4.0).is_err(), "an exact quantity has no uncertainty to count");
    assert!(m().with_dof(0.0).is_err() && m().with_dof(-1.0).is_err() && m().with_dof(f64::NAN).is_err());
    let declared = m().with_dof(5.0).unwrap();
    assert!(declared.with_dof(6.0).is_err(), "the source already has 5");
    assert!(declared.with_dof(5.0).is_ok(), "the same value again is a no-op");
    assert!(m().add(&m()).unwrap().with_dof(4.0).is_err(), "a sum of two measurements is not one");
    for p in [0.0, 1.0, -0.5, 1.5, f64::NAN] {
        assert!(declared.coverage_factor(p).is_err() && declared.expanded_uncertainty(p).is_err(), "p = {p}");
    }
    assert!(Quantity::from_samples(vec![1.0], "V").is_err());
    assert!(Quantity::from_samples(vec![1.0, f64::NAN], "V").is_err());
}
