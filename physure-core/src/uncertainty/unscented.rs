use ndarray::Array1;
use crate::error::{PhysureError, PhysureResult};
use super::lineage::Lineage;
use super::trait_def::UncertaintyBackend;

/// An unscented-transform uncertainty that remembers where it came from.
///
/// Implements derivative-free non-linear uncertainty propagation via deterministic
/// sigma-point sampling (Julier, S. J., & Uhlmann, J. K., 2004, "Unscented filtering and
/// nonlinear estimation", Proc. IEEE, 92(3), 401-422, DOI: 10.1109/JPROC.2003.823141).
///
/// The sigma points stay the source of truth for the spread, since propagating them through a
/// nonlinear function is the whole point of the transform. `sigma` runs alongside them
/// recording *which* measurements produced that spread, so shared sources still cancel. The
/// two are kept consistent: arithmetic rebuilds the points from the merged lineage, and a
/// nonlinear transform rescales the lineage to whatever spread the points came out with.
#[derive(Clone)]
pub struct UnscentedBackend {
    pub sigma_points: Array1<f64>,
    pub weights: Array1<f64>,
    pub sigma: Lineage,
}

impl UnscentedBackend {
    /// A newly measured quantity. Each call mints a fresh source id.
    pub fn new_scalar(mean: f64, std_dev: f64) -> Self {
        Self::derived(mean, Lineage::measured(std_dev))
    }

    /// A value derived from others, carrying the merged lineage of its operands.
    pub fn derived(mean: f64, sigma: Lineage) -> Self {
        let std_dev = sigma.std_dev();
        if std_dev == 0.0 {
            return UnscentedBackend {
                sigma_points: Array1::from_elem(1, mean),
                weights: Array1::from_elem(1, 1.0),
                sigma,
            };
        }
        let n: f64 = 1.0;
        let lambda = 3.0 - n;
        let spread = ((n + lambda).sqrt() * std_dev).abs();

        UnscentedBackend {
            sigma_points: Array1::from_vec(vec![mean, mean + spread, mean - spread]),
            weights: Array1::from_vec(vec![lambda/(n+lambda), 1.0/(2.0*(n+lambda)), 1.0/(2.0*(n+lambda))]),
            sigma,
        }
    }

    /// Rebuilds after a nonlinear transform of the points, keeping the lineage's shape but
    /// resizing it to the spread the transform actually produced.
    pub fn from_points(sigma_points: Array1<f64>, weights: Array1<f64>, sigma: &Lineage) -> Self {
        let spread = weighted_spread(&sigma_points, &weights);
        UnscentedBackend { sigma_points, weights, sigma: sigma.rescaled_to(spread) }
    }

    /// Treats an explicit point set as a single new measurement, taking its uncertainty from
    /// the spread the points themselves describe.
    pub fn from_measured_points(sigma_points: Array1<f64>, weights: Array1<f64>) -> Self {
        let spread = weighted_spread(&sigma_points, &weights);
        UnscentedBackend { sigma_points, weights, sigma: Lineage::measured(spread) }
    }
}

fn weighted_spread(sigma_points: &Array1<f64>, weights: &Array1<f64>) -> f64 {
    let mu = (sigma_points * weights).sum();
    let var: f64 = sigma_points.iter()
        .zip(weights.iter())
        .map(|(x, w)| w * (x - mu).powi(2))
        .sum();
    var.sqrt()
}

// NOTE: as in gaussian.rs, these trait methods take `&dyn UncertaintyBackend` and so cannot see
// the other operand's lineage. The live path is `UncertaintyValue` in trait_def.rs, which
// matches `Self::Unscented(..)` first; only a `Custom` backend reaches these.
impl UncertaintyBackend for UnscentedBackend {
    fn mean(&self) -> f64 { (&self.sigma_points * &self.weights).sum() }
    fn std_dev(&self) -> f64 {
        let mu = (&self.sigma_points * &self.weights).sum();
        let var: f64 = self.sigma_points.iter()
            .zip(self.weights.iter())
            .map(|(x, w)| w * (x - mu).powi(2))
            .sum();
        var.sqrt()
    }

    fn propagate_add(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m = self.mean() + other.mean();
        let s = (self.std_dev().powi(2) + other.std_dev().powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_sub(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m = self.mean() - other.mean();
        let s = (self.std_dev().powi(2) + other.std_dev().powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_mul(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m1 = self.mean(); let s1 = self.std_dev();
        let m2 = other.mean(); let s2 = other.std_dev();
        let m = m1 * m2;
        let s = ((m1 * s2).powi(2) + (m2 * s1).powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_div(&self, other: &dyn UncertaintyBackend) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let m1 = self.mean(); let s1 = self.std_dev();
        let m2 = other.mean(); let s2 = other.std_dev();
        if m2 == 0.0 {
            return Err(PhysureError::DivisionByZero("Division by zero in uncertainty propagation".into()));
        }
        let m = m1 / m2;
        let s = ((s1 / m2).powi(2) + (m1 * s2 / m2.powi(2)).powi(2)).sqrt();
        Ok(Box::new(UnscentedBackend::new_scalar(m, s)))
    }
    fn propagate_pow(&self, exponent: f64) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let new_points = self.sigma_points.mapv(|x| x.powf(exponent));
        Ok(Box::new(UnscentedBackend::from_points(new_points, self.weights.clone(), &self.sigma)))
    }
    fn propagate_function(&self, func: &str) -> PhysureResult<Box<dyn UncertaintyBackend>> {
        let new_points = match func {
            "sin" => self.sigma_points.mapv(|x| x.sin()),
            "cos" => self.sigma_points.mapv(|x| x.cos()),
            "exp" => self.sigma_points.mapv(|x| x.exp()),
            "log" => self.sigma_points.mapv(|x| x.ln()),
            "abs" => self.sigma_points.mapv(|x| x.abs()),
            "tan" => self.sigma_points.mapv(|x| x.tan()),
            "tanh" => self.sigma_points.mapv(|x| x.tanh()),
            _ => self.sigma_points.clone(),
        };
        Ok(Box::new(UnscentedBackend::from_points(new_points, self.weights.clone(), &self.sigma)))
    }

    fn get_model_name(&self) -> &str { "unscented" }
}
