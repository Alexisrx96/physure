use super::*;
use crate::quantity::Quantity;
use crate::units::RationalUnit;
use ndarray::Array1;

/// Sample count shared by the Monte Carlo correlation tests. Big enough that the sampling
/// noise on a standard deviation, sigma / sqrt(2n), is ~0.3% of sigma.
const MC_SAMPLES: usize = 50_000;

fn mc(mean: f64, std_dev: f64) -> Quantity {
    Quantity::new_scalar(
        mean,
        std_dev,
        RationalUnit::dimensionless(),
        Some("monte_carlo"),
        Some(MC_SAMPLES),
    )
}

#[test]
fn test_gaussian_tan_enum() {
    let g = UncertaintyValue::Gaussian(GaussianBackend::new(0.5, 0.1));
    let result = g.propagate_function("tan").unwrap();
    let expected_mean = 0.5_f64.tan();
    let expected_std = ((1.0 + expected_mean.powi(2)) * 0.1).abs();
    assert!((result.mean() - expected_mean).abs() < 1e-10);
    assert!((result.std_dev() - expected_std).abs() < 1e-10);
}

#[test]
fn test_gaussian_tanh_enum() {
    let g = UncertaintyValue::Gaussian(GaussianBackend::new(0.5, 0.1));
    let result = g.propagate_function("tanh").unwrap();
    let expected_mean = 0.5_f64.tanh();
    let expected_std = ((1.0 - expected_mean.powi(2)) * 0.1).abs();
    assert!((result.mean() - expected_mean).abs() < 1e-10);
    assert!((result.std_dev() - expected_std).abs() < 1e-10);
}

#[test]
fn test_montecarlo_tan_enum() {
    let mc = UncertaintyValue::MonteCarlo(MonteCarloBackend {
        samples: Array1::from_vec(vec![0.0, 0.5, -0.5]),
    });
    let result = mc.propagate_function("tan").unwrap();
    match result {
        UncertaintyValue::MonteCarlo(m) => {
            let expected = [0.0_f64.tan(), 0.5_f64.tan(), (-0.5_f64).tan()];
            for (actual, expected) in m.samples.iter().zip(expected.iter()) {
                assert!((actual - expected).abs() < 1e-10);
            }
        }
        _ => panic!("expected MonteCarlo variant"),
    }
}

#[test]
fn test_montecarlo_tanh_enum() {
    let mc = UncertaintyValue::MonteCarlo(MonteCarloBackend {
        samples: Array1::from_vec(vec![0.0, 0.5, -0.5]),
    });
    let result = mc.propagate_function("tanh").unwrap();
    match result {
        UncertaintyValue::MonteCarlo(m) => {
            let expected = [0.0_f64.tanh(), 0.5_f64.tanh(), (-0.5_f64).tanh()];
            for (actual, expected) in m.samples.iter().zip(expected.iter()) {
                assert!((actual - expected).abs() < 1e-10);
            }
        }
        _ => panic!("expected MonteCarlo variant"),
    }
}

#[test]
fn test_unscented_tan_enum() {
    let u = UncertaintyValue::Unscented(UnscentedBackend::from_measured_points(Array1::from_vec(vec![0.0, 0.5, -0.5]), Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0])));
    let result = u.propagate_function("tan").unwrap();
    match result {
        UncertaintyValue::Unscented(uu) => {
            let expected = [0.0_f64.tan(), 0.5_f64.tan(), (-0.5_f64).tan()];
            for (actual, expected) in uu.sigma_points.iter().zip(expected.iter()) {
                assert!((actual - expected).abs() < 1e-10);
            }
        }
        _ => panic!("expected Unscented variant"),
    }
}

#[test]
fn test_unscented_tanh_enum() {
    let u = UncertaintyValue::Unscented(UnscentedBackend::from_measured_points(Array1::from_vec(vec![0.0, 0.5, -0.5]), Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0])));
    let result = u.propagate_function("tanh").unwrap();
    match result {
        UncertaintyValue::Unscented(uu) => {
            let expected = [0.0_f64.tanh(), 0.5_f64.tanh(), (-0.5_f64).tanh()];
            for (actual, expected) in uu.sigma_points.iter().zip(expected.iter()) {
                assert!((actual - expected).abs() < 1e-10);
            }
        }
        _ => panic!("expected Unscented variant"),
    }
}

#[test]
fn test_gaussian_backend_tan_trait_impl() {
    let g = GaussianBackend::new(0.5, 0.1);
    let result = g.propagate_function("tan").unwrap();
    let expected_mean = 0.5_f64.tan();
    let expected_std = ((1.0 + expected_mean.powi(2)) * 0.1).abs();
    assert!((result.mean() - expected_mean).abs() < 1e-10);
    assert!((result.std_dev() - expected_std).abs() < 1e-10);
    assert_eq!(result.get_model_name(), "gaussian");
}

#[test]
fn test_gaussian_backend_tanh_trait_impl() {
    let g = GaussianBackend::new(0.5, 0.1);
    let result = g.propagate_function("tanh").unwrap();
    let expected_mean = 0.5_f64.tanh();
    let expected_std = ((1.0 - expected_mean.powi(2)) * 0.1).abs();
    assert!((result.mean() - expected_mean).abs() < 1e-10);
    assert!((result.std_dev() - expected_std).abs() < 1e-10);
}

#[test]
fn test_montecarlo_backend_tan_trait_impl() {
    let mc = MonteCarloBackend { samples: Array1::from_vec(vec![0.1, 0.3, 0.7]) };
    let result = mc.propagate_function("tan").unwrap();
    let expected_mean = (0.1_f64.tan() + 0.3_f64.tan() + 0.7_f64.tan()) / 3.0;
    assert!((result.mean() - expected_mean).abs() < 1e-9);
    assert_eq!(result.get_model_name(), "monte_carlo");
}

#[test]
fn test_montecarlo_backend_tanh_trait_impl() {
    let mc = MonteCarloBackend { samples: Array1::from_vec(vec![0.1, 0.3, 0.7]) };
    let result = mc.propagate_function("tanh").unwrap();
    let expected_mean = (0.1_f64.tanh() + 0.3_f64.tanh() + 0.7_f64.tanh()) / 3.0;
    assert!((result.mean() - expected_mean).abs() < 1e-9);
}

#[test]
fn test_unscented_backend_tan_trait_impl() {
    let u = UnscentedBackend::from_measured_points(Array1::from_vec(vec![0.1, 0.3, 0.7]), Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]));
    let result = u.propagate_function("tan").unwrap();
    let expected_mean = (0.1_f64.tan() + 0.3_f64.tan() + 0.7_f64.tan()) / 3.0;
    assert!((result.mean() - expected_mean).abs() < 1e-9);
    assert_eq!(result.get_model_name(), "unscented");
}

/// `x - x` must be identically zero: the two operands are the same draws, so every sample
/// cancels against itself. The tolerance is 1e-9 rather than 0.0 only to stay honest about
/// float arithmetic; the answer this guards against is the uncorrelated one, sigma ~ 0.42.
#[test]
fn test_monte_carlo_self_subtraction_cancels_exactly() {
    let x = mc(3.0, 0.3);
    let d = x.sub(&x).expect("dimensionless subtraction");
    assert!(
        d.value.mean().abs() < 1e-9,
        "x - x mean should cancel to 0, got {}",
        d.value.mean()
    );
    assert!(
        d.value.std_dev().abs() < 1e-9,
        "x - x std_dev should cancel to 0, got {} (0.42 means the samples were redrawn)",
        d.value.std_dev()
    );
}

/// `x + x` is `2x`: mean 6.0 and sigma 0.6, not the quadrature 0.424 that independent draws
/// would give. Tolerances: the mean of 50_000 draws wanders by sigma/sqrt(n) ~ 0.0013 (0.0027
/// after doubling) and the estimated sigma by sigma/sqrt(2n) ~ 0.0019 after doubling, so 2% of
/// each target is roughly a 9-sigma band — immune to sampling noise while still 15x tighter
/// than the 29% error the uncorrelated answer would show.
#[test]
fn test_monte_carlo_self_addition_doubles_sigma() {
    let x = mc(3.0, 0.3);
    let s = x.add(&x).expect("dimensionless addition");
    assert!(
        (s.value.mean() - 6.0).abs() < 6.0 * 0.02,
        "x + x mean should be ~6.0, got {}",
        s.value.mean()
    );
    assert!(
        (s.value.std_dev() - 0.6).abs() < 0.6 * 0.02,
        "x + x std_dev should be ~0.6, got {} (0.42 means the samples were redrawn)",
        s.value.std_dev()
    );
}

/// Regression guard for the opposite mistake: reusing the operand's array must NOT make
/// independent quantities correlated. `a` and `b` drew separately, so their sum still adds in
/// quadrature — sqrt(0.3^2 + 0.4^2) = 0.5, not the 0.7 of perfectly correlated inputs. Same
/// 2% (~9-sigma) band as above.
#[test]
fn test_monte_carlo_independent_quantities_add_in_quadrature() {
    let a = mc(3.0, 0.3);
    let b = mc(5.0, 0.4);
    let s = a.add(&b).expect("dimensionless addition");
    let expected = (0.3_f64.powi(2) + 0.4_f64.powi(2)).sqrt();
    assert!(
        (s.value.mean() - 8.0).abs() < 8.0 * 0.02,
        "a + b mean should be ~8.0, got {}",
        s.value.mean()
    );
    assert!(
        (s.value.std_dev() - expected).abs() < expected * 0.02,
        "a + b std_dev should be ~{expected} (quadrature), got {} (0.7 would mean everything \
         became correlated)",
        s.value.std_dev()
    );
}

/// `x * x` squares each sample, so sigma is 2 * mean * sigma_x = 1.8, not the 1.27 of two
/// independent draws. The mean picks up the second-order term (mean^2 + sigma^2 = 9.09), which
/// is exactly the non-linearity Monte Carlo exists to capture.
#[test]
fn test_monte_carlo_self_multiplication_is_correlated() {
    let x = mc(3.0, 0.3);
    let p = x.mul(&x).expect("multiplication");
    assert!(
        (p.value.mean() - 9.09).abs() < 9.09 * 0.02,
        "x * x mean should be ~9.09, got {}",
        p.value.mean()
    );
    assert!(
        (p.value.std_dev() - 1.8).abs() < 1.8 * 0.03,
        "x * x std_dev should be ~1.8, got {} (1.27 means the samples were redrawn)",
        p.value.std_dev()
    );
}

/// Sample arrays of different lengths were not drawn together and cannot be paired
/// elementwise, so that case must still fall back to resampling from the other operand's
/// moments — and must not panic on an ndarray shape mismatch.
#[test]
fn test_monte_carlo_mismatched_sample_counts_resample() {
    let short = UncertaintyValue::MonteCarlo(MonteCarloBackend::from_stats(1.0, 0.1, 32));
    let long = UncertaintyValue::MonteCarlo(MonteCarloBackend::from_stats(2.0, 0.2, 4096));
    let sum = long
        .propagate_add(&short)
        .expect("mismatched lengths resample");
    match sum {
        UncertaintyValue::MonteCarlo(m) => {
            assert_eq!(
                m.samples.len(),
                4096,
                "result keeps the left operand's sample count"
            );
            // Independent draws: quadrature, sqrt(0.2^2 + 0.1^2) ~ 0.2236. Only 4096 samples
            // here, so the band is wide (15%) on purpose — this test is about not panicking
            // and staying in the right ballpark, not about precision.
            let expected = (0.2_f64.powi(2) + 0.1_f64.powi(2)).sqrt();
            assert!(
                (m.std_dev() - expected).abs() < expected * 0.15,
                "expected ~{expected}, got {}",
                m.std_dev()
            );
        }
        _ => panic!("expected MonteCarlo variant"),
    }
}

#[test]
fn test_unscented_backend_tanh_trait_impl() {
    let u = UnscentedBackend::from_measured_points(Array1::from_vec(vec![0.1, 0.3, 0.7]), Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]));
    let result = u.propagate_function("tanh").unwrap();
    let expected_mean = (0.1_f64.tanh() + 0.3_f64.tanh() + 0.7_f64.tanh()) / 3.0;
    assert!((result.mean() - expected_mean).abs() < 1e-9);
}

// --- Self-correlation -------------------------------------------------------------------
//
// A standard deviation alone cannot say whether two operands share a source, so before
// lineage tracking every one of these came out wrong: `x - x` reported sigma*sqrt(2) instead
// of zero. The cases below are run across all three backends, because the gap was in the
// propagation model rather than in any one of them.

fn scalar(mean: f64, std_dev: f64, mode: Option<&str>) -> Quantity {
    Quantity::new_scalar(mean, std_dev, RationalUnit::dimensionless(), mode, Some(50_000))
}

/// The three built-in models, with the tolerance each one can honestly promise. Monte Carlo
/// estimates its spread from 50 000 draws, so it gets a sampling band; the analytic models
/// are exact.
const MODES: [(Option<&str>, f64); 3] =
    [(None, 1e-12), (Some("unscented"), 1e-12), (Some("monte_carlo"), 0.02)];

#[test]
fn a_quantity_cancels_against_itself() {
    for (mode, tol) in MODES {
        let x = scalar(3.0, 0.3, mode);
        let name = mode.unwrap_or("gaussian");

        let diff = x.sub(&x).unwrap();
        assert!(diff.value.std_dev().abs() < 1e-12, "{name}: x - x kept a spread");
        assert!(!diff.value.std_dev().is_sign_negative(), "{name}: negative zero sigma");

        let ratio = x.div(&x).unwrap();
        assert!(ratio.value.std_dev().abs() < 1e-12, "{name}: x / x kept a spread");

        // Doubling, not quadrature: 0.6, not 0.3*sqrt(2) = 0.424.
        let sum = x.add(&x).unwrap();
        assert!((sum.value.std_dev() - 0.6).abs() < 0.6 * tol.max(1e-12), "{name}: x + x");

        // d(x^2)/dx = 2x, so 2 * 3.0 * 0.3 = 1.8, not sqrt(2)*3*0.3 = 1.27.
        let sq = x.mul(&x).unwrap();
        assert!((sq.value.std_dev() - 1.8).abs() < 1.8 * tol.max(1e-12), "{name}: x * x");
    }
}

#[test]
fn two_measurements_of_the_same_value_stay_independent() {
    // The guard against over-correlating: these are two separate readings that happen to
    // agree, so they must still combine in quadrature.
    for (mode, tol) in MODES {
        let name = mode.unwrap_or("gaussian");
        let a = scalar(3.0, 0.3, mode);
        let b = scalar(3.0, 0.3, mode);
        let expected = (0.09_f64 + 0.09).sqrt();
        let got = a.sub(&b).unwrap().value.std_dev();
        assert!(
            (got - expected).abs() < expected * tol.max(1e-12),
            "{name}: expected ~{expected}, got {got}"
        );
    }
}

#[test]
fn cancellation_survives_scaling_and_intermediate_steps() {
    for (mode, tol) in MODES {
        let name = mode.unwrap_or("gaussian");
        let x = scalar(3.0, 0.3, mode);
        let two = scalar(2.0, 0.0, mode);

        // 2x - 2x. Proves this is provenance tracking rather than a special case for the
        // literal `x - x` shape.
        let two_x = x.mul(&two).unwrap();
        assert!(
            two_x.sub(&two_x).unwrap().value.std_dev().abs() < 1e-12,
            "{name}: 2x - 2x kept a spread"
        );

        // (x + y) - y must give x back, with x's own uncertainty and nothing more.
        let y = scalar(5.0, 0.4, mode);
        let round_trip = x.add(&y).unwrap().sub(&y).unwrap();
        assert!(
            (round_trip.value.std_dev() - 0.3).abs() < 0.3 * tol.max(1e-12),
            "{name}: (x + y) - y gave {}",
            round_trip.value.std_dev()
        );
    }
}

#[test]
fn an_exact_constant_never_becomes_a_source() {
    // A conversion factor or a plain number carries no uncertainty, so it must not mint a
    // measurement id — otherwise it would leave a term that never cancels.
    let x = scalar(3.0, 0.3, None);
    let k = scalar(2.0, 0.0, None);
    let scaled = x.mul(&k).unwrap();
    assert!((scaled.value.std_dev() - 0.6).abs() < 1e-12);
    assert!(scaled.sub(&scaled).unwrap().value.std_dev().abs() < 1e-12);
}

#[test]
fn a_moments_value_reads_back_through_the_enum() {
    let x = UncertaintyValue::Moments(MomentsBackend::measured(12.3, 0.4, 0.5).unwrap());
    assert_eq!(x.get_model_name(), "moments");
    assert!(x.mean() > 12.3, "the long tail is upwards, so the mean sits above the quoted mode");
    assert!(x.std_dev() > 0.0);
    assert_eq!(x.lineage().terms().len(), 1, "a measurement is one source");
}

#[test]
fn arithmetic_on_an_asymmetric_value_refuses_instead_of_symmetrising() {
    // Until moment propagation lands, every route through the symmetric arms would return a
    // plausible-looking number with the asymmetry quietly averaged out. Each one must refuse,
    // whichever side the asymmetric operand is on.
    let x = UncertaintyValue::Moments(MomentsBackend::measured(12.3, 0.4, 0.5).unwrap());
    let g = UncertaintyValue::Gaussian(GaussianBackend::new(1.0, 0.3));
    let mc = UncertaintyValue::MonteCarlo(MonteCarloBackend::from_stats(2.0, 0.3, 1000));
    let u = UncertaintyValue::Unscented(UnscentedBackend::new_scalar(2.0, 0.3));

    for other in [&g, &mc, &u, &x] {
        for (name, result) in [
            ("add", x.propagate_add(other)),
            ("sub", x.propagate_sub(other)),
            ("mul", x.propagate_mul(other)),
            ("div", x.propagate_div(other)),
            ("add-rhs", other.propagate_add(&x)),
            ("sub-rhs", other.propagate_sub(&x)),
            ("mul-rhs", other.propagate_mul(&x)),
            ("div-rhs", other.propagate_div(&x)),
        ] {
            let Err(err) = result else { panic!("{name} answered instead of refusing") };
            assert!(err.to_string().contains("not yet propagated"), "{name}: {err}");
        }
    }

    assert!(x.propagate_pow(2.0).is_err());
    assert!(x.propagate_function("sin").is_err());
}

// --- The uncorrelated opt-out ------------------------------------------------------------
#[test]
fn uncorrelated_mode_stops_a_value_from_cancelling_against_itself() {
    let _guard = mode::scoped(PropagationMode::Uncorrelated);

    let x = scalar(10.0, 1.0, None);
    let diff = x.sub(&x).unwrap();
    assert!(
        (diff.value.std_dev() - 2.0f64.sqrt()).abs() < 1e-12,
        "x - x should add in quadrature here, got {}",
        diff.value.std_dev()
    );
    assert!(diff.value.mean().abs() < 1e-12, "only the uncertainty changes, not the value");

    // Genuinely independent operands are unaffected: quadrature is what they did anyway.
    let y = scalar(4.0, 1.0, None);
    assert!((x.sub(&y).unwrap().value.std_dev() - 2.0f64.sqrt()).abs() < 1e-12);
}

#[test]
fn a_result_built_uncorrelated_does_not_cancel_later_either() {
    // Keeping the operands' ids on the result would let a second operation find the shared
    // source the first one was told to ignore, so `(x + x) - x` would come back below x's
    // own sigma -- correlated arithmetic reached through an uncorrelated step.
    let _guard = mode::scoped(PropagationMode::Uncorrelated);

    let x = scalar(10.0, 1.0, None);
    let sum = x.add(&x).unwrap();
    let back = sum.sub(&x).unwrap();
    assert!(
        (back.value.std_dev() - 3.0f64.sqrt()).abs() < 1e-12,
        "expected sqrt(2 + 1), got {}",
        back.value.std_dev()
    );
}

#[test]
fn the_scope_ends_where_the_guard_does() {
    {
        let _guard = mode::scoped(PropagationMode::Uncorrelated);
        assert_eq!(propagation_mode(), PropagationMode::Uncorrelated);
    }
    assert_eq!(propagation_mode(), PropagationMode::Correlated);
    let x = scalar(10.0, 1.0, None);
    assert!(x.sub(&x).unwrap().value.std_dev() < 1e-12, "cancellation is back");
}

#[test]
fn an_exact_value_stays_gaussian_whatever_the_configured_model_is() {
    // A plain number has no distribution to sample. Drawing a thousand identical samples
    // for every literal in a script would be a cost with nothing behind it.
    let _guard = mode::scoped(PropagationMode::MonteCarlo);

    assert_eq!(scalar(3.0, 0.0, None).value.get_model_name(), "gaussian");
    assert_eq!(scalar(3.0, 0.5, None).value.get_model_name(), "monte_carlo");
    // An explicit request still wins over the setting.
    let named = Quantity::new_scalar(3.0, 0.5, RationalUnit::dimensionless(), Some("unscented"), None);
    assert_eq!(named.value.get_model_name(), "unscented");
}

#[test]
fn monte_carlo_stops_sharing_its_draws_when_asked_to_be_uncorrelated() {
    // The Monte Carlo arm carries correlation in the sample array rather than the lineage,
    // so it needs its own answer to the mode -- otherwise a hand-picked backend would keep
    // cancelling inside a scope that exists to switch cancellation off.
    let _guard = mode::scoped(PropagationMode::Uncorrelated);

    let x = mc(10.0, 1.0);
    let diff = x.sub(&x).unwrap();
    assert!(
        (diff.value.std_dev() - 2.0f64.sqrt()).abs() < 0.05,
        "expected ~sqrt(2), got {}",
        diff.value.std_dev()
    );
}
