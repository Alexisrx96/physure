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
fn asymmetric_arithmetic_now_propagates_instead_of_refusing() {
    // A Moments value as `self` keeps propagating -- through the enum's explicit
    // `(Moments, Moments)` arm when both sides are Moments, or through `MomentsBackend`'s own
    // `&dyn` trait methods (reached via the generic `_` fallback) when the other side is some
    // other model, which treats the foreign side as an independent symmetric source.
    //
    // But a Moments value as `other` -- i.e. some *other* model's own symmetric arm running
    // with a Moments operand -- must still refuse: every one of those arms only ever reads
    // `mean()`/`std_dev()` off `other` (the `UncertaintyBackend` trait exposes nothing else),
    // so it would silently drop the Moments side's skew and look like it had worked. A prior
    // version of this test asserted only `is_ok()` on every ordering, which would still pass
    // if foreign-as-self silently symmetrised -- exactly the bug this refusal exists to catch.
    let x = UncertaintyValue::Moments(MomentsBackend::measured(12.3, 0.4, 0.5).unwrap());
    let y = UncertaintyValue::Moments(MomentsBackend::measured(1.0, 0.3, 0.6).unwrap());
    let g = UncertaintyValue::Gaussian(GaussianBackend::new(1.0, 0.3));
    let mc = UncertaintyValue::MonteCarlo(MonteCarloBackend::from_stats(2.0, 0.3, 1000));
    let u = UncertaintyValue::Unscented(UnscentedBackend::new_scalar(2.0, 0.3));

    for foreign in [&g, &mc, &u] {
        for (name, result) in [
            ("add", x.propagate_add(foreign)),
            ("sub", x.propagate_sub(foreign)),
            ("mul", x.propagate_mul(foreign)),
            ("div", x.propagate_div(foreign)),
        ] {
            assert!(result.is_ok(), "moments-left {name} refused instead of propagating");
        }
        for (name, result) in [
            ("add-rhs", foreign.propagate_add(&x)),
            ("sub-rhs", foreign.propagate_sub(&x)),
            ("mul-rhs", foreign.propagate_mul(&x)),
            ("div-rhs", foreign.propagate_div(&x)),
        ] {
            assert!(
                result.is_err(),
                "{name}: foreign-left with a Moments operand must refuse, not silently symmetrise"
            );
        }
    }

    // Both sides Moments succeeds either way, and keeps both operands' real skew -- unlike the
    // `&dyn` fallback above, which cannot see a foreign value's third moment at all.
    for (name, result) in [("add", x.propagate_add(&y)), ("add-rhs", y.propagate_add(&x)), ("mul", x.propagate_mul(&y))]
    {
        let r = result.unwrap();
        if let UncertaintyValue::Moments(m) = r {
            assert!(m.third() != 0.0, "{name}: combining two skewed sources must not average the skew away");
        } else {
            panic!("{name}: expected a Moments result");
        }
    }

    assert!(x.propagate_pow(2.0).is_ok());
    assert!(x.propagate_function("sin").is_ok());
    // The one corner that still refuses: a function name outside the model's derivative
    // table, rather than silently falling through to the identity map.
    assert!(x.propagate_function("gamma_fn_nobody_added").is_err());
}

// -- MomentsBackend restructure: shape/sources, combine, first-order trait arms ----------

#[test]
fn measured_carries_mean_off_the_quoted_value() {
    let b = MomentsBackend::measured(12.3, 0.4, 0.5).unwrap();
    // Equal-area now: shift = k·(σ⁺−σ⁻) = 0.3989…·0.1
    assert!((b.mean - 12.3 - 0.039_894_228_040_143_27).abs() < 1e-12);
    assert!((b.mode().unwrap() - 12.3).abs() < 1e-9);
    let (lo, hi) = b.sigmas().unwrap();
    assert!((lo - 0.4).abs() < 1e-8 && (hi - 0.5).abs() < 1e-8);
}

#[test]
fn combine_is_linear_in_all_three_moments() {
    let a = MomentsBackend::measured(10.0, 0.3, 0.5).unwrap();
    let b = MomentsBackend::measured(20.0, 0.3, 0.5).unwrap();
    let s = MomentsBackend::combine(&[(1.0, &a), (1.0, &b)]).unwrap();
    assert!((s.mean - (a.mean + b.mean)).abs() < 1e-12);
    assert!((s.variance() - 2.0 * a.variance()).abs() < 1e-12);
    assert!((s.third() - 2.0 * a.third()).abs() < 1e-12);
}

#[test]
fn combine_cancels_a_reused_source_in_the_third_moment() {
    let x = MomentsBackend::measured_with(5.0, 0.3, 0.5, ShapeKind::Dimidiated, Some(41)).unwrap();
    let y = MomentsBackend::combine(&[(2.0, &x), (-1.0, &x)]).unwrap(); // 2x − x == x
    assert!((y.variance() - x.variance()).abs() < 1e-12);
    assert!((y.third() - x.third()).abs() < 1e-12);
    assert!((y.mean - x.mean).abs() < 1e-12);
}

#[test]
fn combine_refuses_mixed_shapes() {
    let d = MomentsBackend::measured_with(0.0, 0.3, 0.5, ShapeKind::Dimidiated, None).unwrap();
    let f = MomentsBackend::measured_with(0.0, 0.3, 0.5, ShapeKind::Fechner, None).unwrap();
    assert!(MomentsBackend::combine(&[(1.0, &d), (1.0, &f)]).is_err());
}

#[test]
fn mul_and_div_refuse_mixed_shapes_too() {
    // `combine` (add/sub) already refused a shape mismatch; `first_order` (mul/div) used to be
    // an `&self` method that always answered with `self.shape` and so silently ignored the
    // other operand's shape entirely -- `Dimidiated * Fechner` returned `Ok` with the left
    // operand's shape instead of refusing the way `Dimidiated + Fechner` already did.
    use super::trait_def::UncertaintyValue;
    let d = UncertaintyValue::Moments(MomentsBackend::measured_with(2.0, 0.3, 0.5, ShapeKind::Dimidiated, None).unwrap());
    let f = UncertaintyValue::Moments(MomentsBackend::measured_with(3.0, 0.3, 0.5, ShapeKind::Fechner, None).unwrap());
    assert!(d.propagate_mul(&f).is_err());
    assert!(d.propagate_div(&f).is_err());
}

#[test]
fn unrepresentable_skew_is_an_error_not_a_mean() {
    // Hand-build a backend whose skew exceeds the dimidiated ceiling.
    let b = MomentsBackend {
        mean: 0.0,
        shape: ShapeKind::Dimidiated,
        sources: MomentLineage::measured(1.0, 1.7),
    };
    assert!(b.shift().is_err());
    assert!(b.mode().is_err());
    assert!(b.sigmas().is_err());
    // The moments themselves stay readable.
    assert_eq!(b.variance(), 1.0);
    assert_eq!(b.third(), 1.7);
}

#[test]
fn trait_arms_propagate_first_order() {
    use super::trait_def::UncertaintyValue;
    let a = UncertaintyValue::Moments(MomentsBackend::measured(3.0, 0.3, 0.5).unwrap());
    let b = UncertaintyValue::Moments(MomentsBackend::measured(4.0, 0.2, 0.4).unwrap());
    let p = a.propagate_mul(&b).unwrap();
    if let UncertaintyValue::Moments(m) = p {
        let (ba, bb) =
            (MomentsBackend::measured(3.0, 0.3, 0.5).unwrap(), MomentsBackend::measured(4.0, 0.2, 0.4).unwrap());
        // J_a = mean_b, J_b = mean_a; mean' = product of means (first order).
        assert!((m.mean - ba.mean * bb.mean).abs() < 1e-9);
        let want_var = bb.mean.powi(2) * ba.variance() + ba.mean.powi(2) * bb.variance();
        assert!((m.variance() - want_var).abs() < 1e-9);
        let want_third = bb.mean.powi(3) * ba.third() + ba.mean.powi(3) * bb.third();
        assert!((m.third() - want_third).abs() < 1e-9);
    } else {
        panic!("expected a Moments result")
    }
}

#[test]
fn negative_derivative_flips_the_skew_through_a_function() {
    use super::trait_def::UncertaintyValue;
    let backend = MomentsBackend::measured(1.0, 0.1, 0.3).unwrap();
    let input_third = backend.third();
    let backend_mean = backend.mean;
    assert!(input_third > 0.0);
    // cos is evaluated at the mean, not the mode (see `MomentsBackend::applied`): J =
    // −sin(mean) < 0, so a positive input skew must come out negative.
    let out = UncertaintyValue::Moments(backend).propagate_function("cos").unwrap();
    if let UncertaintyValue::Moments(m) = out {
        let j = -backend_mean.sin();
        assert!(m.third() < 0.0);
        assert!((m.third() - j.powi(3) * input_third).abs() < 1e-12);
        assert!((m.mean - backend_mean.cos()).abs() < 1e-12);
    } else {
        panic!("expected a Moments result")
    }
}

#[test]
fn unknown_function_refuses_instead_of_identity() {
    use super::trait_def::UncertaintyValue;
    let backend = MomentsBackend::measured(1.0, 0.1, 0.3).unwrap();
    let v = UncertaintyValue::Moments(backend);
    assert!(v.propagate_function("gamma_fn_nobody_added").is_err());
}

#[test]
fn dyn_fallback_div_uses_first_order_not_combines_sum() {
    // `MomentsBackend`'s own `&dyn` trait methods are reached whenever a Moments value meets a
    // foreign backend through the enum's generic fallback (`Moments / Gaussian`, say). Nothing
    // else exercises this specific body numerically -- `trait_arms_propagate_first_order` only
    // covers the *enum*'s `(Moments, Moments)` mul arm -- so a `combine`/`first_order` swap
    // here specifically would go uncaught without this.
    let a = MomentsBackend::measured(8.0, 0.3, 0.5).unwrap();
    let g = GaussianBackend::new(4.0, 0.2);
    let result = a.propagate_div(&g).unwrap();
    // First-order mean: a.mean / g.mean. `combine`'s `Σ(coeff * mean)` would instead give
    // (1/g.mean)*a.mean + (-a.mean/g.mean^2)*g.mean == 0 for any inputs -- nothing close to the
    // right answer, so a swap here would be obvious rather than subtly wrong.
    assert!((result.mean() - a.mean / g.mean).abs() < 1e-12);
    let want_variance =
        (1.0 / g.mean).powi(2) * a.variance() + (a.mean / g.mean.powi(2)).powi(2) * (g.std_dev() * g.std_dev());
    assert!((result.std_dev().powi(2) - want_variance).abs() < 1e-9);
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


#[test]
fn a_constant_cannot_change_the_model_by_standing_on_the_left() {
    // `new_scalar` leaves exact values Gaussian, so once a `physure.conf` names a model every
    // plain number in a script still arrives as a Gaussian. Propagation dispatches on the left
    // operand, so `3 + x` used to fall to the generic arm -- dropping x's samples and reporting
    // "gaussian" -- while `x + 3` kept them. Which side the constant sat on decided the model.
    for model in ["monte_carlo", "unscented"] {
        let _guard = mode::scoped(model.parse().unwrap());
        let exact = scalar(3.0, 0.0, None);
        let unc = scalar(10.0, 0.5, None);
        assert_eq!(unc.value.get_model_name(), model, "the scope was not honoured");

        let cases = [
            ("+", exact.add(&unc).unwrap(), unc.add(&exact).unwrap(), true),
            ("-", exact.sub(&unc).unwrap(), unc.sub(&exact).unwrap(), true),
            ("*", exact.mul(&unc).unwrap(), unc.mul(&exact).unwrap(), true),
            // a/b and b/a do not share a spread, so only the model is comparable here.
            ("/", exact.div(&unc).unwrap(), unc.div(&exact).unwrap(), false),
        ];
        for (op, left, right, same_sigma) in cases {
            assert_eq!(left.value.get_model_name(), model, "{model}: 3 {op} x lost the model");
            assert_eq!(right.value.get_model_name(), model, "{model}: x {op} 3 lost the model");
            if same_sigma {
                let (l, r) = (left.value.std_dev(), right.value.std_dev());
                assert!((l - r).abs() < 0.05, "{model}: 3 {op} x gave {l}, x {op} 3 gave {r}");
            }
        }
    }
}

#[test]
fn test_gaussian_vs_monte_carlo_propagation() {
    let g_x = scalar(3.0, 0.5, Some("gaussian"));
    let g_y = scalar(4.0, 0.6, Some("gaussian"));
    let mc_x = scalar(3.0, 0.5, Some("monte_carlo"));
    let mc_y = scalar(4.0, 0.6, Some("monte_carlo"));

    let g_sum = g_x.add(&g_y).unwrap();
    let mc_sum = mc_x.add(&mc_y).unwrap();
    assert!((g_sum.value.mean() - mc_sum.value.mean()).abs() < 0.05);
    assert!((g_sum.value.std_dev() - mc_sum.value.std_dev()).abs() < 0.05);
}

#[test]
fn test_asymmetric_measurement_handling() {
    let m = MomentsBackend::measured(10.0, 1.0, 2.0).unwrap();
    assert!(m.mean() > 10.0);
    assert!(m.std_dev() > 1.0 && m.std_dev() < 2.0);
    
    let u = UncertaintyValue::Moments(m);
    let u_mc = UncertaintyValue::MonteCarlo(MonteCarloBackend::from_stats(1.0, 0.1, 1000));
    assert!(u.propagate_add(&u_mc).is_err());
}
