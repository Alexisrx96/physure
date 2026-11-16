//! The sources of a quantity as plain data.
//!
//! A binding that keeps its quantities outside the core (a Java object is a bare triple the JVM owns) has to
//! carry the lineage with them, or every native call would make a new measurement of an old one and
//! `x - x` would come out at `sqrt(2) u`. The lineage is exported here as `(source id, contribution)` pairs,
//! the contribution being `c_i u_i` in the unit of the quantity, and rebuilt on the way back in: the core
//! still does all the arithmetic, and the binding only carries numbers.
//!
//! Only the Gaussian model keeps its sources (see [`Quantity::effective_dof`] for the same limit).

use super::lineage::{fresh_id, Lineage};
use super::{GaussianBackend, UncertaintyValue};
use crate::quantity::Quantity;
use crate::units::RationalUnit;

/// An id for a measurement a binding creates, from the one counter the core, PHS and Python use, so that
/// two measurements never share an id by accident.
pub fn new_source_id() -> u32 {
    fresh_id()
}

impl Quantity {
    /// A quantity rebuilt from the sources it came from: the other half of [`Quantity::sources`].
    ///
    /// The ids may come from outside the process of this call (a binding mints them with
    /// [`new_source_id`], a file can carry them): they are reserved, so a later measurement never gets one
    /// of them again. Terms are sorted and merged by id, and a zero contribution is dropped.
    pub fn from_sources(mean: f64, sources: Vec<(u32, f64)>, unit: RationalUnit) -> Quantity {
        Quantity::from_value(UncertaintyValue::Gaussian(GaussianBackend::derived(mean, Lineage::from_terms(sources))), unit)
    }

    /// The sources of this quantity, `(id, contribution)`, in the unit it is stored in; empty for an exact
    /// quantity and `None` for a model that does not keep them (Moments, Monte Carlo, unscented, custom).
    pub fn sources(&self) -> Option<Vec<(u32, f64)>> {
        match &self.value {
            UncertaintyValue::Gaussian(g) => Some(g.sigma.terms().to_vec()),
            _ => None,
        }
    }

    /// The sources in the unit [`Quantity::display_parts`] reports, so that they add up in quadrature to the
    /// uncertainty shown: an anonymous compound unit folds its scale into the numbers (`2 m * 3 km` is
    /// `6000 m^2`), and the sources have to be folded the same way.
    pub fn display_sources(&self) -> Option<Vec<(u32, f64)>> {
        let factor = self.folded_scale().unwrap_or(1.0);
        self.sources().map(|terms| terms.into_iter().map(|(id, c)| (id, c * factor)).collect())
    }
}
