use crate::error::{PhysureError, PhysureResult};
use super::lineage::Lineage;
use super::trait_def::UncertaintyBackend;

/// A first-order Gaussian uncertainty that remembers where it came from.
///
/// Implements 1st-order Taylor series uncertainty propagation compliant with the
/// ISO/IEC Guide 98-3:2008 / JCGM 100:2008 (Guide to the Expression of Uncertainty in Measurement, GUM §5.1).
///
/// `sigma` carries the standard deviation *and* its provenance, so two values derived from
/// the same measurement cancel instead of adding in quadrature. See [`Lineage`].
#[derive(Clone)]
pub struct GaussianBackend {
    pub mean: f64,
    pub sigma: Lineage,
}

impl GaussianBackend {
    /// A newly measured quantity. Each call mints a fresh source id, so measuring the same
    /// number twice gives two independent measurements.
    pub fn new(mean: f64, std_dev: f64) -> Self {
        GaussianBackend { mean, sigma: Lineage::measured(std_dev) }
    }

    /// A value derived from others, carrying the merged lineage of its operands.
    pub fn derived(mean: f64, sigma: Lineage) -> Self {
        GaussianBackend { mean, sigma }
    }

    /// An exact value — a constant, or a conversion factor.
    pub fn exact(mean: f64) -> Self {
        GaussianBackend { mean, sigma: Lineage::exact() }
    }
}

// NOTE: these trait methods take `&dyn UncertaintyBackend`, which cannot expose a lineage, so
// they fall back to quadrature and will not cancel shared sources. They are not the live path:
// `UncertaintyValue` in trait_def.rs matches `Self::Gaussian(..)` first and does the lineage
// merge there. What reaches here is a `Custom` backend, whose provenance is unknowable anyway,
// and for that quadrature is the honest answer.
impl UncertaintyBackend for GaussianBackend {
    fn mean(&self) -> f64 { self.mean }
    fn std_dev(&self) -> f64 { self.sigma.std_dev() }

    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let new_mean = self.mean + other.mean();
        let new_std = (self.std_dev().powi(2) + other.std_dev().powi(2)).sqrt();
        Ok(Box::new(GaussianBackend::new(new_mean, new_std)))
    }

    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let new_mean = self.mean - other.mean();
        let new_std = (self.std_dev().powi(2) + other.std_dev().powi(2)).sqrt();
        Ok(Box::new(GaussianBackend::new(new_mean, new_std)))
    }

    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m1 = self.mean; let s1 = self.std_dev();
        let m2 = other.mean(); let s2 = other.std_dev();
        let new_mean = m1 * m2;
        let new_std = ((m2 * s1).powi(2) + (m1 * s2).powi(2)).sqrt();
        Ok(Box::new(GaussianBackend::new(new_mean, new_std)))
    }

    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m1 = self.mean; let s1 = self.std_dev();
        let m2 = other.mean(); let s2 = other.std_dev();
        if m2 == 0.0 {
            return Err(PhysureError::DivisionByZero("Division by zero in uncertainty propagation".into()));
        }
        let new_mean = m1 / m2;
        let new_std = ((s1 / m2).powi(2) + (m1 * s2 / m2.powi(2)).powi(2)).sqrt();
        Ok(Box::new(GaussianBackend::new(new_mean, new_std)))
    }

    fn propagate_pow(&self, exponent: f64) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m = self.mean;
        let new_mean = m.powf(exponent);
        if m == 0.0 && exponent > 0.0 {
            return Ok(Box::new(GaussianBackend::exact(0.0)));
        }
        // Single operand, so the lineage only needs the derivative applied — no merge, and
        // the source ids survive, which is what makes `x^2 / x^2` cancel.
        let jacobian = exponent * m.powf(exponent - 1.0);
        Ok(Box::new(GaussianBackend::derived(new_mean, self.sigma.scale(jacobian))))
    }

    fn propagate_function(&self, func: &str) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m = self.mean;
        let (new_mean, jacobian) = function_mean_and_jacobian(func, m);
        Ok(Box::new(GaussianBackend::derived(new_mean, self.sigma.scale(jacobian))))
    }

    fn get_model_name(&self) -> &str { "gaussian" }
}

/// The value and the first derivative of a supported one-argument function at `m`.
///
/// `abs` has a jacobian of 1 rather than `signum`: folding a magnitude to its absolute value
/// moves where the measurement sits, not how well it is known, and a sign flip on every
/// coefficient would be an arbitrary choice that breaks cancellation for no gain.
pub(crate) fn function_mean_and_jacobian(func: &str, m: f64) -> (f64, f64) {
    match func {
        "sin" => (m.sin(), m.cos()),
        "cos" => (m.cos(), -m.sin()),
        "exp" => (m.exp(), m.exp()),
        "log" => (m.ln(), 1.0 / m),
        "abs" => (m.abs(), 1.0),
        "tan" => (m.tan(), 1.0 + m.tan().powi(2)),
        "tanh" => (m.tanh(), 1.0 - m.tanh().powi(2)),
        _ => (m, 1.0),
    }
}
