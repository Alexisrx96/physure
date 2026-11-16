//! What the covariance store keeps (audit B14). The store used to drop every entry of a registered variance and of
//! every Jacobian with `|value| <= 1e-12`, an absolute threshold in whatever unit the caller's numbers were in: a
//! numpy array `[1, 2] m ± 1e-7 m` (variance 1e-14) lost its uncertainty in `x + x`, the same array in `mm` or
//! `km` did too for `u = 1e-9`, and `x * 1e-13` lost the Jacobian. The rule now: a Jacobian entry and a variance
//! (diagonal) entry are dropped only when exactly zero; a covariance between two components of one registered
//! variable is dropped only when negligible relative to their standard deviations,
//! `|cov(i, j)| <= 1e-12 sqrt(var_i var_j)` (a bound on the correlation coefficient, which no change of unit moves).
//!
//! The call sequences are those of the Python array path (`vectorized_uncertainty.py`: `register_diagonal` for an
//! array with independent components, then `propagate` with the operation's Jacobians), through the public API.

use physure_core::covariance::{CovarianceStore, PruningConfig};

fn store() -> CovarianceStore {
    CovarianceStore::new(PruningConfig::default())
}

fn identity(n: usize) -> Vec<f64> {
    let mut j = vec![0.0; n * n];
    for i in 0..n {
        j[i * n + i] = 1.0;
    }
    j
}

/// The dense matrix of a stored block (`None`: no block).
fn block(store: &CovarianceStore, a: u64, b: u64) -> Option<Vec<Vec<f64>>> {
    let mat = store.get_block_internal(a, b)?;
    let mut dense = vec![vec![0.0; mat.cols()]; mat.rows()];
    for (v, (r, c)) in mat.iter() {
        dense[r][c] = *v;
    }
    Some(dense)
}

/// The positions the block stores, sorted.
fn stored_positions(store: &CovarianceStore, a: u64, b: u64) -> Vec<(usize, usize)> {
    let mat = store.get_block_internal(a, b).expect("a stored block");
    let mut positions: Vec<_> = mat.iter().map(|(_, (r, c))| (r, c)).collect();
    positions.sort_unstable();
    positions
}

fn assert_rel(actual: f64, expected: f64, what: &str) {
    assert!(
        ((actual - expected) / expected).abs() <= 1e-14,
        "{what}: {actual:e}, expected {expected:e}"
    );
}

/// `x + x` for `x = [1, 2] ± [u, u]`: the uncertainty is `2 u`, whatever `u` and whatever unit the numbers are
/// in (`m`, `mm`, `km` are the same numbers times 1, 1e3, 1e-3).
#[test]
fn x_plus_x_has_twice_the_uncertainty_for_every_u_in_every_unit() {
    for u_in_m in [1e-9, 1e-7, 1e-5] {
        for (unit, per_m) in [("m", 1.0), ("mm", 1e3), ("km", 1e-3)] {
            let u: f64 = u_in_m * per_m;
            let mut s = store();
            s.register_diagonal_slice(0, &[u * u, u * u]);
            let j = identity(2);
            s.propagate_slices(2, vec![0, 0], vec![(&j, &[2, 2]), (&j, &[2, 2])]);
            let var = block(&s, 2, 2).unwrap_or_else(|| panic!("u = {u_in_m} m in {unit}: no variance stored"));
            for (i, row) in var.iter().enumerate() {
                assert_eq!(row[i].sqrt(), 2.0 * u, "u = {u_in_m} m in {unit}, component {i}");
            }
        }
    }
}

/// A Jacobian entry is a sensitivity coefficient in the units of output per input: `1e-13` is not "zero".
#[test]
fn a_small_jacobian_entry_is_kept() {
    for factor in [1e-11, 1e-13, 1e-20] {
        let mut s = store();
        s.register_diagonal_slice(0, &[1e-6, 1e-6]);
        let j = [factor, 0.0, 0.0, factor];
        s.propagate_slices(1, vec![0], vec![(&j, &[2, 2])]);
        let var = block(&s, 1, 1).expect("the output variance");
        for (i, row) in var.iter().enumerate() {
            assert_rel(row[i].sqrt(), factor * 1e-3, &format!("x * {factor:e}, component {i}"));
        }
        // the exact zeros of the Jacobian are not stored: the output stays diagonal
        assert_eq!(stored_positions(&s, 1, 1), vec![(0, 0), (1, 1)]);
    }
}

/// A large array is no different (the Python path switches representation at 1000 elements).
#[test]
fn a_two_thousand_element_array_keeps_its_uncertainty() {
    let n = 2000;
    let u = 1e-7;
    let mut s = store();
    s.register_diagonal_slice(0, &vec![u * u; n]);
    let j = identity(n);
    let shape = [n, n];
    s.propagate_slices(1, vec![0, 0], vec![(&j, &shape), (&j, &shape)]);
    let var = s.get_block_internal(1, 1).expect("the output variance");
    assert_eq!(var.nnz(), n, "one variance per component, nothing else");
    for (v, (r, c)) in var.iter() {
        assert_eq!(r, c);
        assert_rel(v.sqrt(), 2.0 * u, &format!("component {r}"));
    }
}

/// A variance is dropped only when it is exactly zero, however small it is.
#[test]
fn a_variance_is_dropped_only_when_exactly_zero() {
    let mut s = store();
    s.register_diagonal_slice(0, &[1e-300, 0.0, 4e-30]);
    assert_eq!(stored_positions(&s, 0, 0), vec![(0, 0), (2, 2)]);

    let mut s = store();
    #[rustfmt::skip]
    let full = [
        1e-40, 0.0,  0.0,
        0.0,   0.0,  0.0,
        0.0,   0.0,  9e-36,
    ];
    s.register_variable_slice(0, &full, &[3, 3]);
    assert_eq!(stored_positions(&s, 0, 0), vec![(0, 0), (2, 2)]);
}

/// The covariance between two components of one registered variable is dropped when its correlation coefficient is
/// at most 1e-12, and kept otherwise, however small it is in absolute terms.
#[test]
fn a_covariance_inside_a_registered_variable_is_judged_by_its_correlation() {
    let (ua, ub) = (1e-9_f64, 3e-8_f64);
    for (rho, kept) in [(0.5, true), (1e-6, true), (2e-12, true), (1e-12, false), (1e-15, false), (0.0, false)] {
        let cov = rho * ua * ub;
        let mut s = store();
        s.register_variable_slice(0, &[ua * ua, cov, cov, ub * ub], &[2, 2]);
        let mut expected = vec![(0, 0), (1, 1)];
        if kept {
            expected.extend([(0, 1), (1, 0)]);
            expected.sort_unstable();
        }
        assert_eq!(stored_positions(&s, 0, 0), expected, "correlation {rho:e}");
    }
}

/// Scaling a variable by `10^k` (a change of unit) scales its stored entries and the result, and changes nothing
/// about which entries are stored: the property the absolute threshold broke.
#[test]
fn which_entries_are_stored_does_not_depend_on_the_unit() {
    let reference = scaled_case(1.0);
    for k in -9..=9 {
        let scale = 10f64.powi(k);
        let case = scaled_case(scale);
        assert_eq!(case.a_positions, reference.a_positions, "10^{k}: the registered block");
        assert_eq!(case.y_positions, reference.y_positions, "10^{k}: the output variance");
        assert_eq!(case.cross_positions, reference.cross_positions, "10^{k}: the cross covariance y, a");
        for (i, (&got, &want)) in case.u_y.iter().zip(&reference.u_y).enumerate() {
            // `y` does not depend on the unit of `a`: its Jacobian carries the inverse scale
            assert_rel(got, want, &format!("10^{k}: u(y) component {i}"));
        }
    }
}

struct Case {
    a_positions: Vec<(usize, usize)>,
    y_positions: Vec<(usize, usize)>,
    cross_positions: Vec<(usize, usize)>,
    u_y: Vec<f64>,
}

/// `a` (two components, correlation 0.3 between them, and a third component correlated at 1e-13 with the first:
/// negligible) in a unit `scale` times smaller, `b` independent; `y = J_a a + J_b b` with a tiny but non-zero
/// sensitivity.
fn scaled_case(scale: f64) -> Case {
    let ua = [2e-6 * scale, 5e-7 * scale, 1e-4 * scale];
    let corr = |i: usize, j: usize, rho: f64| rho * ua[i] * ua[j];
    #[rustfmt::skip]
    let sigma_a = [
        ua[0] * ua[0],     corr(0, 1, 0.3),   corr(0, 2, 1e-13),
        corr(1, 0, 0.3),   ua[1] * ua[1],     0.0,
        corr(2, 0, 1e-13), 0.0,               ua[2] * ua[2],
    ];
    let mut s = store();
    s.register_variable_slice(1, &sigma_a, &[3, 3]);
    s.register_diagonal_slice(2, &[1e-10, 1e-10]);
    #[rustfmt::skip]
    let j_a = [
        1.0 / scale, 1e-15 / scale, 0.0,
        0.0,         2.0 / scale,   1e-13 / scale,
    ];
    let j_b = [3.0, 0.0, 0.0, 1e-14];
    s.propagate_slices(3, vec![1, 2], vec![(&j_a, &[2, 3]), (&j_b, &[2, 2])]);
    let y = block(&s, 3, 3).expect("the output variance");
    Case {
        a_positions: stored_positions(&s, 1, 1),
        y_positions: stored_positions(&s, 3, 3),
        cross_positions: stored_positions(&s, 3, 1),
        u_y: (0..2).map(|i| y[i][i].sqrt()).collect(),
    }
}

/// An input that appears twice in one operation (`x + x`) contributes through both of its Jacobians to the
/// covariance of the result with every other variable: `cov(x + x, x) = 2 var(x)`, so `(x + x) - x` has the
/// uncertainty of `x`. Found while measuring B14: the store used only the first Jacobian of a repeated input, and
/// the Python array `(x + x) - x` came out with `sqrt(3) u`, `(x + x) - 2 x` with `2 u` instead of `0`.
#[test]
fn a_repeated_input_contributes_every_jacobian_to_the_cross_covariance() {
    let u = 1e-3;
    let mut s = store();
    s.register_diagonal_slice(0, &[u * u, u * u]);
    let j = identity(2);
    let minus_j: Vec<f64> = j.iter().map(|v| -v).collect();
    s.propagate_slices(1, vec![0, 0], vec![(&j, &[2, 2]), (&j, &[2, 2])]);
    let cross = block(&s, 1, 0).expect("cov(x + x, x)");
    for (i, row) in cross.iter().enumerate() {
        assert_eq!(row[i], 2.0 * u * u, "cov(x + x, x), component {i}");
    }
    s.propagate_slices(2, vec![1, 0], vec![(&j, &[2, 2]), (&minus_j, &[2, 2])]);
    let var = block(&s, 2, 2).expect("var((x + x) - x)");
    for (i, row) in var.iter().enumerate() {
        assert_rel(row[i].sqrt(), u, &format!("(x + x) - x, component {i}"));
    }
}
