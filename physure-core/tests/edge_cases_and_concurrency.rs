use physure_core::math::{DualNumber, Interval, HessianPropagation};
use physure_core::units::{RationalUnit, UnitRegistry};
use physure_core::uncertainty::{UncertaintyValue, GaussianBackend};
use physure_core::covariance::{CovarianceStore, PruningConfig};

use physure_core::error::PhysureError;

use std::sync::{Arc, Mutex};
use std::thread;
use ndarray::Array2;
use num_rational::Rational64;

// ============================================================================
// 1. CONCURRENCY & RACE CONDITION TESTS
// ============================================================================

#[test]
fn test_concurrent_covariance_store_updates() {
    let store = Arc::new(Mutex::new(CovarianceStore::new(PruningConfig::default())));
    let mut handles = vec![];

    for thread_id in 0..10 {
        let store_clone = Arc::clone(&store);
        let handle = thread::spawn(move || {
            let data = vec![1.0, 2.0, 3.0, 4.0];
            let shape = [2, 2];
            let var_id = thread_id as u64;
            
            // Acquire lock and register variable
            let mut guard = store_clone.lock().unwrap();
            guard.register_variable_slice(var_id, &data, &shape);
            guard.propagate_slices(100 + var_id, vec![var_id], vec![(&[1.0, 0.0, 0.0, 1.0], &[2, 2])]);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let guard = store.lock().unwrap();
    assert_eq!(guard.num_blocks(), 30); // 10 original + 10 output + 10 cross-covariance blocks
}

#[test]
fn test_concurrent_unit_registry_lookups() {
    let mut reg = UnitRegistry::new();
    reg.add_base_unit("meter".into());
    reg.add_base_unit("second".into());
    reg.register_alias("m".into(), "meter".into());
    reg.register_alias("s".into(), "second".into());
    let reg_arc = Arc::new(reg);

    let mut handles = vec![];
    for _ in 0..16 {
        let r = Arc::clone(&reg_arc);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                assert!(r.contains("m"));
                assert!(r.contains("s"));
                let u = r.get_unit("m").unwrap();
                assert_eq!(u.get_exponent("meter"), Some((1, 1)));
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_uncertainty_value_send_sync_multithreaded_propagation() {
    let g1 = Arc::new(UncertaintyValue::Gaussian(GaussianBackend { mean: 10.0, std_dev: 2.0 }));
    let g2 = Arc::new(UncertaintyValue::Gaussian(GaussianBackend { mean: 5.0, std_dev: 1.0 }));

    let mut handles = vec![];
    for _ in 0..8 {
        let v1 = Arc::clone(&g1);
        let v2 = Arc::clone(&g2);
        handles.push(thread::spawn(move || {
            let res = v1.propagate_add(&v2).unwrap();
            assert_eq!(res.mean(), 15.0);
            assert!((res.std_dev() - 5.0_f64.sqrt()).abs() < 1e-10);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

// ============================================================================
// 2. EDGE CASE & NUMERICAL LIMIT TESTS
// ============================================================================

#[test]
fn test_interval_division_by_zero_and_negative_sqrt() {
    let zero_interval = Interval::new(-1.0, 1.0);
    let positive_interval = Interval::new(2.0, 4.0);
    
    // Division by interval containing zero must return None safely
    assert!((positive_interval / zero_interval).is_none());

    // Sqrt of strictly negative interval must return None
    let neg_interval = Interval::new(-5.0, -1.0);
    assert!(neg_interval.sqrt().is_none());

    // Sqrt of interval straddling zero starts at 0.0
    let straddle = Interval::new(-4.0, 9.0);
    let s = straddle.sqrt().unwrap();
    assert_eq!(s.min, 0.0);
    assert_eq!(s.max, 3.0);
}

#[test]
fn test_dual_number_nan_inf_safety() {
    let inf_dual = DualNumber::constant(f64::INFINITY);
    let nan_dual = DualNumber::constant(f64::NAN);

    assert!(inf_dual.sin().value.is_nan());
    assert!(nan_dual.exp().value.is_nan());
}



#[test]
fn test_gaussian_propagation_division_by_zero() {
    let g1 = UncertaintyValue::Gaussian(GaussianBackend { mean: 10.0, std_dev: 1.0 });
    let g_zero = UncertaintyValue::Gaussian(GaussianBackend { mean: 0.0, std_dev: 1.0 });

    assert!(matches!(g1.propagate_div(&g_zero), Err(PhysureError::DivisionByZero(_))));
}

#[test]
fn test_covariance_corrupted_arrow_ipc_bytes() {
    let mut store = CovarianceStore::new(PruningConfig::default());
    let corrupted_bytes = vec![0u8; 32];
    assert!(matches!(store.from_arrow_bytes(corrupted_bytes), Err(PhysureError::ArrowError(_))));
}

#[test]
fn test_alias_resolution_cycle_prevention() {
    let mut reg = UnitRegistry::new();
    reg.register_alias("a".into(), "b".into());
    reg.register_alias("b".into(), "a".into());

    // Alias cycle resolution must terminate without infinite loop
    assert_eq!(reg.resolve_symbol("a"), "a");
}

#[test]
fn test_extreme_unit_exponent_powers() {
    let length = RationalUnit::new_from_dimensions([("L".into(), (1, 1))]);
    let huge_power = length.pow(Rational64::new(1000000, 1));
    assert_eq!(huge_power.get_exponent("L"), Some((1000000, 1)));
}

#[test]
fn test_hessian_propagation_dimension_mismatch() {
    let mean = 5.0;
    let hessian = Array2::from_elem((2, 2), 1.0);
    let covariance = Array2::from_elem((3, 3), 1.0);

    // Mismatched matrix shape should safely return base mean without panicking
    let result = HessianPropagation::propagate_mean(mean, &hessian, &covariance);
    assert_eq!(result, mean);
}
