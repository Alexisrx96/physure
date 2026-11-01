//! Effective degrees of freedom (Welch–Satterthwaite, JCGM 100:2008 G.4.1): the degrees of freedom of the
//! sources are kept once, in a table; the lineage of a value says how much each source contributes.

use physure_core::uncertainty::{declare_dof, dof_of, Lineage};

fn close(a: f64, b: f64) -> bool {
    ((a - b) / b).abs() < 1e-12
}

/// A freshly measured source with the given standard uncertainty and degrees of freedom.
fn source(std_dev: f64, nu: f64) -> Lineage {
    let lineage = Lineage::measured(std_dev);
    declare_dof(lineage.terms()[0].0, nu).unwrap();
    lineage
}

#[test]
fn a_source_with_no_declared_dof_has_an_infinite_one() {
    let lineage = Lineage::measured(0.1);
    assert_eq!(dof_of(lineage.terms()[0].0), f64::INFINITY);
    assert_eq!(lineage.effective_dof(), f64::INFINITY);
    assert_eq!(Lineage::exact().effective_dof(), f64::INFINITY);
}

#[test]
fn one_source_keeps_its_own_dof() {
    assert!(close(source(0.3, 7.0).effective_dof(), 7.0));
}

#[test]
fn independent_sources_combine_by_welch_satterthwaite() {
    // u_c = sqrt(3² + 4²) = 5; nu_eff = 5⁴ / (3⁴/4 + 4⁴/9)
    let sum = Lineage::combine(&source(3.0, 4.0), 1.0, &source(4.0, 9.0), 1.0);
    assert!(close(sum.std_dev(), 5.0));
    assert!(close(sum.effective_dof(), 625.0 / (81.0 / 4.0 + 256.0 / 9.0)));
}

#[test]
fn the_sensitivity_of_the_model_is_part_of_the_contribution() {
    // y = 2a + b/2: the contributions are 2·3 = 6 and 0.5·4 = 2
    let y = Lineage::combine(&source(3.0, 4.0), 2.0, &source(4.0, 9.0), 0.5);
    let u_c4 = (36.0f64 + 4.0).powi(2);
    assert!(close(y.effective_dof(), u_c4 / (6.0f64.powi(4) / 4.0 + 2.0f64.powi(4) / 9.0)));
}

#[test]
fn a_shared_source_is_counted_once_and_keeps_its_dof() {
    let x = source(0.2, 5.0);
    // x + x is one source with twice the contribution: the same nu_eff
    let twice = Lineage::combine(&x, 1.0, &x, 1.0);
    assert!(close(twice.std_dev(), 0.4));
    assert!(close(twice.effective_dof(), 5.0));
    // x - x cancels: nothing is left to have degrees of freedom
    let nothing = Lineage::combine(&x, 1.0, &x, -1.0);
    assert_eq!(nothing.effective_dof(), f64::INFINITY);
}

#[test]
fn equal_independent_sources_add_their_dof() {
    let mut sum = source(0.1, 6.0);
    for _ in 1..4 {
        sum = Lineage::combine(&sum, 1.0, &source(0.1, 6.0), 1.0);
    }
    assert!(close(sum.effective_dof(), 24.0), "four sources of 6 degrees of freedom: {}", sum.effective_dof());
}

#[test]
fn a_source_with_infinite_dof_adds_to_the_uncertainty_but_not_to_the_denominator() {
    // a: u = 3, nu = 4; b: u = 4, no dof. u_c⁴ = 625, and only a is in the denominator
    let sum = Lineage::combine(&source(3.0, 4.0), 1.0, &Lineage::measured(4.0), 1.0);
    assert!(close(sum.effective_dof(), 625.0 / (81.0 / 4.0)));
}

#[test]
fn a_dof_can_be_declared_once_and_the_same_value_again() {
    let lineage = Lineage::measured(1.0);
    let id = lineage.terms()[0].0;
    declare_dof(id, 9.0).unwrap();
    declare_dof(id, 9.0).unwrap();
    let error = declare_dof(id, 12.0).unwrap_err().to_string();
    assert!(error.contains("already has 9"), "{error}");
    assert_eq!(dof_of(id), 9.0);
    // infinity is a different statement from 9
    assert!(declare_dof(id, f64::INFINITY).is_err());
}

#[test]
fn infinity_may_be_declared_for_a_source_without_a_dof_and_changes_nothing() {
    let lineage = Lineage::measured(1.0);
    let id = lineage.terms()[0].0;
    declare_dof(id, f64::INFINITY).unwrap();
    assert_eq!(dof_of(id), f64::INFINITY);
    // so a finite value can still be declared later: infinity was never recorded
    declare_dof(id, 3.0).unwrap();
    assert_eq!(dof_of(id), 3.0);
}

#[test]
fn zero_negative_and_nan_degrees_of_freedom_are_refused() {
    let id = Lineage::measured(1.0).terms()[0].0;
    for nu in [0.0, -1.0, f64::NAN, f64::NEG_INFINITY] {
        let error = declare_dof(id, nu).unwrap_err().to_string();
        assert!(error.contains("must be positive"), "{nu}: {error}");
    }
    assert_eq!(dof_of(id), f64::INFINITY);
}
