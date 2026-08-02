use std::borrow::Cow;

use dyn_clone::DynClone;
use ndarray::Array1;
use super::lineage::Lineage;
use crate::error::PhysureResult;
use super::gaussian::GaussianBackend;
use super::moments::MomentsBackend;
use super::monte_carlo::MonteCarloBackend;
use super::unscented::UnscentedBackend;

/// Core trait for uncertainty propagation. Uses native Rust types — no PyO3.
pub trait UncertaintyBackend: DynClone + Send + Sync {
    fn mean(&self) -> f64;
    fn std_dev(&self) -> f64;
    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn propagate_pow(&self, exponent: f64) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn propagate_function(&self, func: &str) -> PhysureResult<Box<dyn UncertaintyBackend>>;
    fn get_model_name(&self) -> &str;
}

dyn_clone::clone_trait_object!(UncertaintyBackend);

/// Zero-allocation inline enum for built-in uncertainty models.
#[derive(Clone)]
pub enum UncertaintyValue {
    Gaussian(GaussianBackend),
    /// An asymmetric measurement, kept as moments so its skew survives propagation.
    Moments(MomentsBackend),
    MonteCarlo(MonteCarloBackend),
    Unscented(UnscentedBackend),
    Custom(Box<dyn UncertaintyBackend>),
}

impl UncertaintyValue {
    pub fn mean(&self) -> f64 {
        match self {
            Self::Gaussian(g) => g.mean(),
            Self::Moments(m) => m.mean(),
            Self::MonteCarlo(m) => m.mean(),
            Self::Unscented(u) => u.mean(),
            Self::Custom(c) => c.mean(),
        }
    }

    pub fn std_dev(&self) -> f64 {
        match self {
            Self::Gaussian(g) => g.std_dev(),
            Self::Moments(m) => m.std_dev(),
            Self::MonteCarlo(m) => m.std_dev(),
            Self::Unscented(u) => u.std_dev(),
            Self::Custom(c) => c.std_dev(),
        }
    }

    pub fn get_model_name(&self) -> &str {
        match self {
            Self::Gaussian(g) => g.get_model_name(),
            Self::Moments(m) => m.get_model_name(),
            Self::MonteCarlo(m) => m.get_model_name(),
            Self::Unscented(u) => u.get_model_name(),
            Self::Custom(c) => c.get_model_name(),
        }
    }

    /// The provenance of this value's uncertainty.
    ///
    /// Monte Carlo carries correlation in its sample array rather than in a lineage, and a
    /// `Custom` backend's provenance is unknowable from here. Both therefore report a fresh
    /// independent source, which makes a mixed-model operation fall back to quadrature — the
    /// honest answer when the two sides cannot be related.
    pub fn lineage(&self) -> Lineage {
        match self {
            Self::Gaussian(g) => g.sigma.clone(),
            Self::Moments(m) => m.sigma.clone(),
            Self::Unscented(u) => u.sigma.clone(),
            Self::MonteCarlo(_) | Self::Custom(_) => Lineage::measured(self.std_dev()),
        }
    }

    pub fn as_backend_ref(&self) -> &dyn UncertaintyBackend {
        match self {
            Self::Gaussian(g) => g,
            Self::Moments(m) => m,
            Self::MonteCarlo(m) => m,
            Self::Unscented(u) => u,
            Self::Custom(c) => c.as_ref(),
        }
    }

    /// Picks the sample array that pairs elementwise with `m1.samples`.
    ///
    /// Correlation for free is the entire reason to pay for 50_000 samples: when both
    /// operands already carry sample arrays, elementwise arithmetic cancels the shared
    /// draws by itself, so `x - x` is exactly zero and `x + x` has twice the spread of
    /// `x`. Redrawing the right operand from its mean and std_dev — which is what
    /// `ensure_samples` does — throws that away and silently reports the uncorrelated
    /// answer for correlated inputs.
    ///
    /// Borrowing `other`'s array is the right answer in both situations:
    /// - shared ancestry (`x` and `x`): the arrays are literally the same draws, so the
    ///   arithmetic cancels exactly;
    /// - two independent Monte Carlo quantities: each drew its own independent variates in
    ///   `from_stats`, so pairing them elementwise is plain Monte Carlo over independent
    ///   inputs, which still comes out in quadrature.
    ///
    /// Differing lengths mean the two arrays were not drawn together and cannot be paired
    /// at all, so that case falls back to resampling from `other`'s moments.
    fn mc_operand_samples<'a>(
        m1: &MonteCarloBackend,
        other: &'a UncertaintyValue,
    ) -> PhysureResult<Cow<'a, Array1<f64>>> {
        if let Self::MonteCarlo(m2) = other {
            if m2.samples.len() == m1.samples.len() {
                return Ok(Cow::Borrowed(&m2.samples));
            }
        }
        Ok(Cow::Owned(m1.ensure_samples(other.as_backend_ref())?))
    }

    pub fn propagate_add(&self, other: &UncertaintyValue) -> PhysureResult<UncertaintyValue> {
        match (self, other) {
            // Refused rather than approximated: every arm below is symmetric, so any of
            // them would drop the skew and look like it had worked.
            (Self::Moments(_), _) | (_, Self::Moments(_)) => Err(super::moments::not_implemented("addition")),
            (Self::Gaussian(g1), Self::Gaussian(g2)) => {
                let m = g1.mean + g2.mean;
                let sigma = Lineage::combine(&g1.sigma, 1.0, &g2.sigma, 1.0);
                Ok(Self::Gaussian(GaussianBackend::derived(m, sigma)))
            }
            (Self::MonteCarlo(m1), other_val) => {
                let other_samples = Self::mc_operand_samples(m1, other_val)?;
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: &m1.samples + &*other_samples }))
            }
            (Self::Unscented(u1), other_val) => {
                let m = u1.mean() + other_val.mean();
                let sigma = Lineage::combine(&u1.sigma, 1.0, &other_val.lineage(), 1.0);
                Ok(Self::Unscented(UnscentedBackend::derived(m, sigma)))
            }
            _ => {
                let b = self.as_backend_ref().propagate_add(other.as_backend_ref())?;
                Ok(Self::Custom(b))
            }
        }
    }

    pub fn propagate_sub(&self, other: &UncertaintyValue) -> PhysureResult<UncertaintyValue> {
        match (self, other) {
            // Refused rather than approximated: every arm below is symmetric, so any of
            // them would drop the skew and look like it had worked.
            (Self::Moments(_), _) | (_, Self::Moments(_)) => Err(super::moments::not_implemented("subtraction")),
            (Self::Gaussian(g1), Self::Gaussian(g2)) => {
                let m = g1.mean - g2.mean;
                let sigma = Lineage::combine(&g1.sigma, 1.0, &g2.sigma, -1.0);
                Ok(Self::Gaussian(GaussianBackend::derived(m, sigma)))
            }
            (Self::MonteCarlo(m1), other_val) => {
                let other_samples = Self::mc_operand_samples(m1, other_val)?;
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: &m1.samples - &*other_samples }))
            }
            (Self::Unscented(u1), other_val) => {
                let m = u1.mean() - other_val.mean();
                let sigma = Lineage::combine(&u1.sigma, 1.0, &other_val.lineage(), -1.0);
                Ok(Self::Unscented(UnscentedBackend::derived(m, sigma)))
            }
            _ => {
                let b = self.as_backend_ref().propagate_sub(other.as_backend_ref())?;
                Ok(Self::Custom(b))
            }
        }
    }

    pub fn propagate_mul(&self, other: &UncertaintyValue) -> PhysureResult<UncertaintyValue> {
        match (self, other) {
            // Refused rather than approximated: every arm below is symmetric, so any of
            // them would drop the skew and look like it had worked.
            (Self::Moments(_), _) | (_, Self::Moments(_)) => Err(super::moments::not_implemented("multiplication")),
            (Self::Gaussian(g1), Self::Gaussian(g2)) => {
                // d(ab)/da = b, d(ab)/db = a. With disjoint lineages this reproduces the
                // quadrature form it replaces; with a shared source it does not, which is
                // the point: x * x is 2*x*sigma, not sqrt(2)*x*sigma.
                let sigma = Lineage::combine(&g1.sigma, g2.mean, &g2.sigma, g1.mean);
                Ok(Self::Gaussian(GaussianBackend::derived(g1.mean * g2.mean, sigma)))
            }
            (Self::MonteCarlo(m1), other_val) => {
                let other_samples = Self::mc_operand_samples(m1, other_val)?;
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: &m1.samples * &*other_samples }))
            }
            (Self::Unscented(u1), other_val) => {
                let m1 = u1.mean(); let m2 = other_val.mean();
                let sigma = Lineage::combine(&u1.sigma, m2, &other_val.lineage(), m1);
                Ok(Self::Unscented(UnscentedBackend::derived(m1 * m2, sigma)))
            }
            _ => {
                let b = self.as_backend_ref().propagate_mul(other.as_backend_ref())?;
                Ok(Self::Custom(b))
            }
        }
    }

    pub fn propagate_div(&self, other: &UncertaintyValue) -> PhysureResult<UncertaintyValue> {
        match (self, other) {
            // Refused rather than approximated: every arm below is symmetric, so any of
            // them would drop the skew and look like it had worked.
            (Self::Moments(_), _) | (_, Self::Moments(_)) => Err(super::moments::not_implemented("division")),
            (Self::Gaussian(g1), Self::Gaussian(g2)) => {
                let m1 = g1.mean; let m2 = g2.mean;
                if m2 == 0.0 {
                    return Err(crate::error::PhysureError::DivisionByZero("Uncertainty propagation denominator is zero".into()));
                }
                // d(a/b)/da = 1/b, d(a/b)/db = -a/b^2.
                let sigma = Lineage::combine(&g1.sigma, 1.0 / m2, &g2.sigma, -m1 / m2.powi(2));
                Ok(Self::Gaussian(GaussianBackend::derived(m1 / m2, sigma)))
            }
            (Self::MonteCarlo(m1), other_val) => {
                // Unlike the Gaussian and Unscented arms, this one has never raised
                // DivisionByZero: it divides sample by sample, so a zero draw shows up as a
                // non-finite sample rather than an error. That behaviour is left as it was;
                // only where the denominator's samples come from changes here.
                let other_samples = Self::mc_operand_samples(m1, other_val)?;
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: &m1.samples / &*other_samples }))
            }
            (Self::Unscented(u1), other_val) => {
                let m1 = u1.mean(); let m2 = other_val.mean();
                if m2 == 0.0 {
                    return Err(crate::error::PhysureError::DivisionByZero("Uncertainty propagation denominator is zero".into()));
                }
                let sigma = Lineage::combine(&u1.sigma, 1.0 / m2, &other_val.lineage(), -m1 / m2.powi(2));
                Ok(Self::Unscented(UnscentedBackend::derived(m1 / m2, sigma)))
            }
            _ => {
                let b = self.as_backend_ref().propagate_div(other.as_backend_ref())?;
                Ok(Self::Custom(b))
            }
        }
    }

    pub fn propagate_pow(&self, exponent: f64) -> PhysureResult<UncertaintyValue> {
        match self {
            Self::Gaussian(g) => {
                let m = g.mean;
                let new_mean = m.powf(exponent);
                if m == 0.0 && exponent > 0.0 {
                    return Ok(Self::Gaussian(GaussianBackend::exact(0.0)));
                }
                // One operand, so there is nothing to merge — the derivative scales the
                // existing terms and their source ids survive, which is what lets
                // `x^2 / x^2` cancel instead of accumulating spurious spread.
                let sigma = g.sigma.scale(exponent * m.powf(exponent - 1.0));
                Ok(Self::Gaussian(GaussianBackend::derived(new_mean, sigma)))
            }
            Self::Moments(_) => Err(super::moments::not_implemented("exponentiation")),
            Self::MonteCarlo(m) => {
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: m.samples.mapv(|x| x.powf(exponent)) }))
            }
            Self::Unscented(u) => {
                let new_points = u.sigma_points.mapv(|x| x.powf(exponent));
                Ok(Self::Unscented(UnscentedBackend::from_points(new_points, u.weights.clone(), &u.sigma)))
            }
            Self::Custom(c) => {
                let b = c.propagate_pow(exponent)?;
                Ok(Self::Custom(b))
            }
        }
    }

    pub fn propagate_function(&self, func: &str) -> PhysureResult<UncertaintyValue> {
        match self {
            Self::Gaussian(g) => {
                let (new_mean, jacobian) = super::gaussian::function_mean_and_jacobian(func, g.mean);
                Ok(Self::Gaussian(GaussianBackend::derived(new_mean, g.sigma.scale(jacobian))))
            }
            Self::Moments(_) => Err(super::moments::not_implemented(func)),
            Self::MonteCarlo(m) => {
                let new_samples = match func {
                    "sin" => m.samples.mapv(|x| x.sin()),
                    "cos" => m.samples.mapv(|x| x.cos()),
                    "exp" => m.samples.mapv(|x| x.exp()),
                    "log" => m.samples.mapv(|x| x.ln()),
                    "abs" => m.samples.mapv(|x| x.abs()),
                    "tan" => m.samples.mapv(|x| x.tan()),
                    "tanh" => m.samples.mapv(|x| x.tanh()),
                    _ => m.samples.clone(),
                };
                Ok(Self::MonteCarlo(MonteCarloBackend { samples: new_samples }))
            }
            Self::Unscented(u) => {
                let new_points = match func {
                    "sin" => u.sigma_points.mapv(|x| x.sin()),
                    "cos" => u.sigma_points.mapv(|x| x.cos()),
                    "exp" => u.sigma_points.mapv(|x| x.exp()),
                    "log" => u.sigma_points.mapv(|x| x.ln()),
                    "abs" => u.sigma_points.mapv(|x| x.abs()),
                    "tan" => u.sigma_points.mapv(|x| x.tan()),
                    "tanh" => u.sigma_points.mapv(|x| x.tanh()),
                    _ => u.sigma_points.clone(),
                };
                Ok(Self::Unscented(UnscentedBackend::from_points(new_points, u.weights.clone(), &u.sigma)))
            }
            Self::Custom(c) => {
                let b = c.propagate_function(func)?;
                Ok(Self::Custom(b))
            }
        }
    }
}
