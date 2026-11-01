//! Degrees of freedom, coverage factor and expanded uncertainty of a [`Quantity`] (JCGM 100:2008 4.2.2,
//! 4.3, 6.2 and G.4).
//!
//! Everything here is defined for the Gaussian model, whose lineage says which independent sources a
//! value came from and how much each contributes. The other models, which propagate a distribution and
//! not a decomposition, **raise** with the reason: a number they did not compute is never returned.

use super::dof::declare_dof;
use super::lineage::Lineage;
use super::trait_def::UncertaintyValue;
use crate::error::{PhysureError, PhysureResult};
use crate::math::stats::{normal_two_sided, student_t_two_sided};
use crate::quantity::Quantity;

/// An expanded uncertainty, with everything needed to say how it was obtained. It is a result of its own
/// and never a `±` that could be read as a standard uncertainty.
#[derive(Debug, Clone, PartialEq)]
pub struct Expanded {
    /// The estimate, in the unit of the quantity it was made from.
    pub value: f64,
    /// The combined standard uncertainty `u_c`.
    pub standard_uncertainty: f64,
    /// `U = k u_c`: the half-width of the interval with coverage probability `p`.
    pub expanded_uncertainty: f64,
    /// The coverage factor `k` (Student-t at the truncated effective degrees of freedom; the normal factor
    /// when they are infinite).
    pub coverage_factor: f64,
    /// The coverage probability `p`, in (0, 1).
    pub coverage_probability: f64,
    /// The effective degrees of freedom, as the Welch–Satterthwaite formula gives them (not truncated).
    pub dof_eff: f64,
}

impl Quantity {
    /// The lineage of a Gaussian quantity, the only model whose degrees of freedom can be read.
    fn gaussian_lineage(&self) -> PhysureResult<&Lineage> {
        match &self.value {
            UncertaintyValue::Gaussian(g) => Ok(&g.sigma),
            other => Err(PhysureError::Generic(format!(
                "degrees of freedom are defined for the Gaussian model, not '{}': it does not keep \
                 the independent sources a value came from",
                other.get_model_name()
            ))),
        }
    }

    /// This quantity with `nu` degrees of freedom declared for its uncertainty: the number of degrees of
    /// freedom of the standard uncertainty, as a Type A evaluation of `n` observations has `n - 1`.
    ///
    /// The degrees of freedom belong to the **measurement**, so the quantity must be one: exactly one
    /// source of uncertainty. A result that combines several has the degrees of freedom of its sources;
    /// declare them on each before combining. `nu` is positive, or `+∞` for an uncertainty known exactly.
    ///
    /// # Errors
    /// An exact quantity, one with several sources, one that is not Gaussian, `nu` not positive, or `nu`
    /// different from what this source already has.
    pub fn with_dof(self, nu: f64) -> PhysureResult<Quantity> {
        let terms = self.gaussian_lineage()?.terms();
        match terms {
            [(id, _)] => declare_dof(*id, nu)?,
            [] => {
                return Err(PhysureError::DomainError(
                    "an exact quantity has no uncertainty to have degrees of freedom".to_string(),
                ))
            }
            many => {
                return Err(PhysureError::DomainError(format!(
                    "a quantity that combines {} sources has no degrees of freedom to declare: declare \
                     them on each measurement before combining",
                    many.len()
                )))
            }
        }
        Ok(self)
    }

    /// A Type A evaluation (JCGM 100:2008 4.2): the mean of the observations, its experimental standard
    /// deviation `s / sqrt(n)` as the standard uncertainty, and `n - 1` degrees of freedom.
    ///
    /// Observations that are all equal have no spread to estimate and give an exact quantity.
    ///
    /// # Errors
    /// Fewer than two observations, or one that is not finite.
    pub fn from_samples(samples: &[f64], unit_expr: &str) -> PhysureResult<Quantity> {
        if samples.len() < 2 {
            return Err(PhysureError::DomainError(format!(
                "a Type A evaluation needs at least two observations, got {}",
                samples.len()
            )));
        }
        if let Some(bad) = samples.iter().find(|x| !x.is_finite()) {
            return Err(PhysureError::DomainError(format!("an observation must be finite, got {bad}")));
        }
        let n = samples.len() as f64;
        let mean = samples.iter().sum::<f64>() / n;
        let variance = samples.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (n - 1.0);
        let standard_error = (variance / n).sqrt();
        let quantity = Quantity::with_uncertainty(mean, standard_error, unit_expr)?;
        if standard_error > 0.0 {
            quantity.with_dof(n - 1.0)
        } else {
            Ok(quantity)
        }
    }

    /// The effective degrees of freedom of this quantity's uncertainty by the Welch–Satterthwaite formula
    /// (JCGM 100:2008 G.4.1): `+∞` when none of its sources has a finite number of them, and for an exact
    /// quantity.
    ///
    /// # Errors
    /// A quantity that is not Gaussian.
    pub fn effective_dof(&self) -> PhysureResult<f64> {
        Ok(self.gaussian_lineage()?.effective_dof())
    }

    /// The coverage factor `k_p` for a coverage probability `p` in (0, 1): the Student-t factor at the
    /// effective degrees of freedom **truncated to an integer** (not below one), as the Guide recommends
    /// (G.4.1, NOTE), or the normal factor when they are infinite.
    ///
    /// # Errors
    /// `p` outside (0, 1), or a quantity that is not Gaussian.
    pub fn coverage_factor(&self, p: f64) -> PhysureResult<f64> {
        let nu_eff = self.effective_dof()?;
        if nu_eff.is_infinite() {
            normal_two_sided(p)
        } else {
            student_t_two_sided(p, nu_eff.floor().max(1.0))
        }
    }

    /// The expanded uncertainty `U = k_p u_c` for a coverage probability `p`, with the factor and the
    /// degrees of freedom it came from.
    ///
    /// # Errors
    /// `p` outside (0, 1), or a quantity that is not Gaussian.
    pub fn expanded(&self, p: f64) -> PhysureResult<Expanded> {
        let coverage_factor = self.coverage_factor(p)?;
        let standard_uncertainty = self.value.std_dev();
        Ok(Expanded {
            value: self.value.mean(),
            standard_uncertainty,
            expanded_uncertainty: coverage_factor * standard_uncertainty,
            coverage_factor,
            coverage_probability: p,
            dof_eff: self.effective_dof()?,
        })
    }
}
