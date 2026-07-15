use ndarray::{Array1, Array2};

/// 2nd-Order Hessian Non-Linear Uncertainty Propagation.
/// Extends 1st-order Jacobian linear estimation with exact curvature corrections:
///   Mean_out = f(μ) + 0.5 * Tr(H * Σ)
///   Var_out  = J * Σ * J^T + 0.5 * Tr(H * Σ * H * Σ)
#[derive(Debug, Clone)]
pub struct HessianPropagation;

impl HessianPropagation {
    /// Compute 2nd-order corrected scalar mean under non-linear transformation.
    pub fn propagate_mean(
        f_mean: f64,
        hessian: &Array2<f64>,
        covariance: &Array2<f64>,
    ) -> f64 {
        let (rows, cols) = hessian.dim();
        if rows != covariance.dim().0 || cols != covariance.dim().1 {
            return f_mean;
        }
        let trace_term: f64 = (hessian * covariance).diag().sum();
        f_mean + 0.5 * trace_term
    }

    /// Compute 2nd-order corrected variance under non-linear transformation.
    pub fn propagate_variance(
        jacobian: &Array1<f64>,
        hessian: &Array2<f64>,
        covariance: &Array2<f64>,
    ) -> f64 {
        // 1st order term: J * Σ * J^T
        let j_sig = jacobian.dot(covariance);
        let linear_var = j_sig.dot(jacobian);

        // 2nd order curvature term: 0.5 * Tr(H * Σ * H * Σ)
        let h_sig = hessian.dot(covariance);
        let h_sig_sq = h_sig.dot(&h_sig);
        let curvature_var = 0.5 * h_sig_sq.diag().sum();

        linear_var + curvature_var
    }

    /// Slice overload for FFI interoperability.
    pub fn propagate_mean_slices(
        f_mean: f64,
        hessian_slice: &[f64],
        covariance_slice: &[f64],
        rows: usize,
        cols: usize,
    ) -> f64 {
        if let (Ok(h), Ok(cov)) = (
            Array2::from_shape_vec((rows, cols), hessian_slice.to_vec()),
            Array2::from_shape_vec((rows, cols), covariance_slice.to_vec()),
        ) {
            Self::propagate_mean(f_mean, &h, &cov)
        } else {
            f_mean
        }
    }

    /// Slice overload for FFI interoperability.
    pub fn propagate_variance_slices(
        jacobian_slice: &[f64],
        hessian_slice: &[f64],
        covariance_slice: &[f64],
        rows: usize,
        cols: usize,
    ) -> f64 {
        if let (Ok(j), Ok(h), Ok(cov)) = (
            Array1::from_shape_vec(jacobian_slice.len(), jacobian_slice.to_vec()),
            Array2::from_shape_vec((rows, cols), hessian_slice.to_vec()),
            Array2::from_shape_vec((rows, cols), covariance_slice.to_vec()),
        ) {
            Self::propagate_variance(&j, &h, &cov)
        } else {
            0.0
        }
    }
}
