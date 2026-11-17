//! Special functions of a real argument.

/// The error function, erf x = 2/√π ∫₀ˣ e^(−t²) dt, accurate to the last bit or two over the
/// whole real line (including a tiny argument, where erf x ≈ 2x/√π).
pub fn erf(x: f64) -> f64 {
    libm::erf(x)
}

/// The derivative of [`erf`]: 2/√π · e^(−x²).
pub fn erf_derivative(x: f64) -> f64 {
    std::f64::consts::FRAC_2_SQRT_PI * (-x * x).exp()
}
