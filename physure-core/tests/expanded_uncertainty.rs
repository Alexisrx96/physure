//! Declaring degrees of freedom on a quantity, and reading the effective ones, the coverage factor and the
//! expanded uncertainty from it (JCGM 100:2008 4.3, 6.2 and G.4), ending with the Guide's own example, H.1.

use physure_core::math::stats::student_t_two_sided;
use physure_core::{Quantity, RationalUnit};

fn close(a: f64, b: f64) -> bool {
    ((a - b) / b).abs() < 1e-12
}

fn measured(mean: f64, std_dev: f64, unit: &str) -> Quantity {
    Quantity::with_uncertainty(mean, std_dev, unit).unwrap()
}

fn with_dof(mean: f64, std_dev: f64, nu: f64) -> Quantity {
    measured(mean, std_dev, "m").with_dof(nu).unwrap()
}

#[test]
fn a_declared_dof_belongs_to_the_value_and_changes_nothing_else() {
    let plain = measured(10.0, 0.5, "m");
    assert_eq!(plain.effective_dof().unwrap(), f64::INFINITY);
    let q = plain.with_dof(9.0).unwrap();
    assert_eq!((q.value.mean(), q.value.std_dev()), (10.0, 0.5));
    assert!(close(q.effective_dof().unwrap(), 9.0));
}

#[test]
fn the_dof_of_a_result_comes_from_its_sources() {
    let sum = with_dof(1.0, 3.0, 4.0).add(&with_dof(2.0, 4.0, 9.0)).unwrap();
    assert!(close(sum.value.std_dev(), 5.0));
    assert!(close(sum.effective_dof().unwrap(), 625.0 / (81.0 / 4.0 + 256.0 / 9.0)));
    // the same value, scaled by a unit conversion, keeps its sources
    let in_mm = sum.to("mm").unwrap();
    assert!(close(in_mm.effective_dof().unwrap(), sum.effective_dof().unwrap()));
}

#[test]
fn a_copy_of_a_value_shares_its_dof_and_its_cancellation() {
    let x = with_dof(5.0, 0.2, 7.0);
    let doubled = x.add(&x).unwrap();
    assert!(close(doubled.effective_dof().unwrap(), 7.0));
    assert_eq!(x.sub(&x).unwrap().effective_dof().unwrap(), f64::INFINITY);
}

#[test]
fn declaring_twice_is_a_no_op_for_the_same_value_and_an_error_for_another() {
    let q = measured(1.0, 0.1, "m");
    let first = q.clone().with_dof(5.0).unwrap();
    assert!(close(q.clone().with_dof(5.0).unwrap().effective_dof().unwrap(), 5.0));
    let error = q.with_dof(6.0).unwrap_err().to_string();
    assert!(error.contains("already has 5"), "{error}");
    assert!(close(first.effective_dof().unwrap(), 5.0));
}

#[test]
fn what_has_no_single_source_or_no_gaussian_model_cannot_be_given_a_dof() {
    let two = measured(1.0, 0.1, "m").add(&measured(2.0, 0.1, "m")).unwrap();
    let error = two.with_dof(4.0).unwrap_err().to_string();
    assert!(error.contains("combines 2 sources"), "{error}");

    let exact = measured(3.0, 0.0, "m");
    assert!(exact.clone().with_dof(4.0).unwrap_err().to_string().contains("exact"));
    assert_eq!(exact.effective_dof().unwrap(), f64::INFINITY);

    let unit = RationalUnit::dimensionless();
    for mode in ["monte_carlo", "unscented"] {
        let q = Quantity::new_scalar(5.0, 0.1, unit.clone(), Some(mode), Some(200));
        if mode == "monte_carlo" {
            let error = q.clone().with_dof(4.0).unwrap_err().to_string();
            assert!(error.contains("Gaussian") && error.contains("monte_carlo"), "{error}");
            assert!(q.effective_dof().is_err());
            assert!(q.coverage_factor(0.95).is_err());
            assert!(q.expanded(0.95).is_err());
        }
    }
    assert!(measured(1.0, 0.1, "m").with_dof(0.0).is_err());
    assert!(measured(1.0, 0.1, "m").with_dof(-3.0).is_err());
    assert!(measured(1.0, 0.1, "m").with_dof(f64::NAN).is_err());
}

#[test]
fn a_type_a_evaluation_is_the_mean_the_standard_error_and_n_minus_one_dof() {
    // deviations from the mean 10.1: -0.1, 0.1, -0.2, 0, 0.2; s² = 0.10 / 4, u = s / sqrt(5)
    let q = Quantity::from_samples(&[10.0, 10.2, 9.9, 10.1, 10.3], "V").unwrap();
    assert!(close(q.value.mean(), 10.1));
    assert!(close(q.value.std_dev(), (0.025f64 / 5.0).sqrt()));
    assert!(close(q.effective_dof().unwrap(), 4.0));

    assert!(Quantity::from_samples(&[1.0], "V").unwrap_err().to_string().contains("at least two"));
    assert!(Quantity::from_samples(&[], "V").is_err());
    assert!(Quantity::from_samples(&[1.0, f64::NAN], "V").is_err());
    // identical readings have no spread to estimate: an exact value
    let same = Quantity::from_samples(&[2.0, 2.0, 2.0], "V").unwrap();
    assert_eq!((same.value.mean(), same.value.std_dev(), same.effective_dof().unwrap()), (2.0, 0.0, f64::INFINITY));
}

#[test]
fn the_coverage_factor_uses_the_dof_truncated_to_an_integer_and_not_below_one() {
    let k = |nu: f64| with_dof(1.0, 0.1, nu).coverage_factor(0.95).unwrap();
    assert!(close(k(16.0), student_t_two_sided(0.95, 16.0).unwrap()));
    assert!(close(k(16.75), student_t_two_sided(0.95, 16.0).unwrap()), "16.75 is read as 16");
    assert!(close(k(0.4), student_t_two_sided(0.95, 1.0).unwrap()), "below one it is one");
    assert!(close(measured(1.0, 0.1, "m").coverage_factor(0.95).unwrap(), 1.959_963_984_540_054));
    assert!(close(measured(1.0, 0.0, "m").coverage_factor(0.99).unwrap(), 2.575_829_303_548_900_6));
    for p in [0.0, 1.0, -0.5, 2.0, f64::NAN] {
        assert!(with_dof(1.0, 0.1, 5.0).coverage_factor(p).is_err(), "p = {p}");
    }
}

#[test]
fn the_expanded_uncertainty_reports_every_part_of_it() {
    let e = with_dof(10.0, 0.5, 16.75).expanded(0.95).unwrap();
    let k = student_t_two_sided(0.95, 16.0).unwrap();
    assert_eq!((e.value, e.standard_uncertainty, e.coverage_probability), (10.0, 0.5, 0.95));
    assert!(close(e.coverage_factor, k));
    assert!(close(e.expanded_uncertainty, k * 0.5));
    assert!(close(e.dof_eff, 16.75), "nu_eff is reported as it is, not truncated");

    let infinite = measured(10.0, 0.5, "m").expanded(0.95).unwrap();
    assert!(close(infinite.expanded_uncertainty, 1.959_963_984_540_054 * 0.5));
    assert_eq!(infinite.dof_eff, f64::INFINITY);
}

/// JCGM 100:2008, Annex H.1: the calibration of an end gauge of nominal length 50 mm, 'the final result'.
/// The six contributions that are not zero at the nominal point, in nm, with their degrees of freedom: the
/// calibration of the standard (25, 18), the mean difference (5.8, 24), the random (3.9, 5) and systematic
/// (6.7, 8) effects of the comparator, the temperature cycle (16.599, 2) and the difference of the
/// expansion coefficients (2.88679, 50), as the GTC documentation reproduces the budget. The reference
/// numbers were computed independently with scipy; the Guide prints them rounded.
#[test]
fn the_end_gauge_of_the_guide_reproduces_its_result() {
    let budget = [
        (50_000_623.0, 25.0, 18.0),
        (215.0, 5.8, 24.0),
        (0.0, 3.9, 5.0),
        (0.0, 6.7, 8.0),
        (0.0, 16.599, 2.0),
        (0.0, 2.88679, 50.0),
    ];
    let mut length = None::<Quantity>;
    for (mean, u, nu) in budget {
        let part = measured(mean, u, "nm").with_dof(nu).unwrap();
        length = Some(match length {
            None => part,
            Some(sum) => sum.add(&part).unwrap(),
        });
    }
    let length = length.unwrap();

    // the value and the combined standard uncertainty: 50 000 838 nm and 32 nm in the Guide
    assert_eq!(length.value.mean(), 50_000_838.0);
    assert!(close(length.value.std_dev(), 31.663_865_170_002_54));
    assert_eq!(format!("{:.0}", length.value.std_dev()), "32");

    // nu_eff = 16.75: the Guide prints 16.7
    let nu_eff = length.effective_dof().unwrap();
    assert!(close(nu_eff, 16.751_895_334_905_974));
    assert_eq!(format!("{:.1}", (nu_eff * 10.0).floor() / 10.0), "16.7");

    // 95 %: k = 2.12 from nu = 16, U = 0.07 µm
    let p95 = length.expanded(0.95).unwrap();
    assert!(close(p95.coverage_factor, 2.119_905_299_221_254_6));
    assert!(close(p95.expanded_uncertainty, 67.124_395_567_715_7));
    assert_eq!(format!("{:.2}", p95.coverage_factor), "2.12");
    assert_eq!(format!("{:.2}", p95.expanded_uncertainty / 1000.0), "0.07");

    // 99 %: k = 2.92, U = 0.09 µm
    let p99 = length.expanded(0.99).unwrap();
    assert!(close(p99.coverage_factor, 2.920_781_622_425_099_8));
    assert!(close(p99.expanded_uncertainty, 92.483_235_483_489_64));
    assert_eq!(format!("{:.2}", p99.coverage_factor), "2.92");
    assert_eq!(format!("{:.2}", p99.expanded_uncertainty / 1000.0), "0.09");
}
