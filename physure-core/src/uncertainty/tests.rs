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
    let g = UncertaintyValue::Gaussian(GaussianBackend { mean: 0.5, std_dev: 0.1 });
    let result = g.propagate_function("tan").unwrap();
    let expected_mean = 0.5_f64.tan();
    let expected_std = ((1.0 + expected_mean.powi(2)) * 0.1).abs();
    assert!((result.mean() - expected_mean).abs() < 1e-10);
    assert!((result.std_dev() - expected_std).abs() < 1e-10);
}

#[test]
fn test_gaussian_tanh_enum() {
    let g = UncertaintyValue::Gaussian(GaussianBackend { mean: 0.5, std_dev: 0.1 });
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
    let u = UncertaintyValue::Unscented(UnscentedBackend {
        sigma_points: Array1::from_vec(vec![0.0, 0.5, -0.5]),
        weights: Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]),
    });
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
    let u = UncertaintyValue::Unscented(UnscentedBackend {
        sigma_points: Array1::from_vec(vec![0.0, 0.5, -0.5]),
        weights: Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]),
    });
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
    let g = GaussianBackend { mean: 0.5, std_dev: 0.1 };
    let result = g.propagate_function("tan").unwrap();
    let expected_mean = 0.5_f64.tan();
    let expected_std = ((1.0 + expected_mean.powi(2)) * 0.1).abs();
    assert!((result.mean() - expected_mean).abs() < 1e-10);
    assert!((result.std_dev() - expected_std).abs() < 1e-10);
    assert_eq!(result.get_model_name(), "gaussian");
}

#[test]
fn test_gaussian_backend_tanh_trait_impl() {
    let g = GaussianBackend { mean: 0.5, std_dev: 0.1 };
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
    let u = UnscentedBackend {
        sigma_points: Array1::from_vec(vec![0.1, 0.3, 0.7]),
        weights: Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]),
    };
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
    let u = UnscentedBackend {
        sigma_points: Array1::from_vec(vec![0.1, 0.3, 0.7]),
        weights: Array1::from_vec(vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]),
    };
    let result = u.propagate_function("tanh").unwrap();
    let expected_mean = (0.1_f64.tanh() + 0.3_f64.tanh() + 0.7_f64.tanh()) / 3.0;
    assert!((result.mean() - expected_mean).abs() < 1e-9);
}
