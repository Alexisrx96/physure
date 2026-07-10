// ponytail: allow clippy style lints

#![allow(clippy::type_complexity, clippy::too_many_arguments)]

use pyo3::prelude::*;

mod units;
mod uncertainty;
mod quantity;
mod covariance;
mod math;
mod serialization;
mod symbolic;

use units::{RationalUnit, UnitRegistry};
use quantity::Quantity;
use covariance::{CovarianceStore, PruningConfig};
use serialization::to_arrow_record_batch;
use symbolic::Expr;

#[pymodule]
fn measurekit_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<RationalUnit>()?;
    m.add_class::<UnitRegistry>()?;
    m.add_class::<Quantity>()?;
    m.add_class::<PruningConfig>()?;
    m.add_class::<CovarianceStore>()?;
    m.add_class::<Expr>()?;
    m.add_function(wrap_pyfunction!(to_arrow_record_batch, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
