//! The builtins of the degrees of freedom of a measurement (JCGM 100:2008 4.2, 4.3, G.4): `dof`,
//! `coverage_factor`, `expanded` and `from_samples`. The mathematics is `physure_core`'s
//! (`Quantity::effective_dof`, `coverage_factor`, `expanded`, `from_samples_in`); this module reads the
//! arguments and says what is wrong with them.

use physure_core::error::{PhysureError, PhysureResult};
use physure_core::{Quantity, RationalUnit};

use crate::value::PhsValue;

/// The result of the builtin `name`, or `None` when `name` is not one of this module's.
pub fn eval_dof_builtin(name: &str, args: &[PhsValue]) -> PhysureResult<Option<PhsValue>> {
    match name {
        "dof" => dof(args).map(Some),
        "coverage_factor" => coverage_factor(args).map(Some),
        "expanded" => expanded(args).map(Some),
        "from_samples" => from_samples(args).map(Some),
        _ => Ok(None),
    }
}

fn arity(name: &str, count: usize) -> PhysureError {
    let plural = if count == 1 { "" } else { "s" };
    PhysureError::Generic(format!("{name} expects {count} argument{plural}"))
}

/// A measurement or an exact number, as a quantity (a bare number is exact and dimensionless).
fn as_quantity(name: &str, value: &PhsValue) -> PhysureResult<Quantity> {
    match value {
        PhsValue::Quantity(q) => Ok(q.clone()),
        PhsValue::Number(n) => Ok(Quantity::new_scalar(*n, 0.0, RationalUnit::dimensionless(), None, None)),
        other => Err(PhysureError::Generic(format!("{name} expects a quantity, got {}", other.type_name()))),
    }
}

/// The coverage probability of a call: a plain number, or an exact dimensionless quantity.
fn probability(name: &str, value: &PhsValue) -> PhysureResult<f64> {
    match value {
        PhsValue::Number(p) => Ok(*p),
        PhsValue::Quantity(q) if q.unit == RationalUnit::dimensionless() && q.value.std_dev() <= 0.0 => Ok(q.value.mean()),
        other => Err(PhysureError::Generic(format!(
            "{name} expects a coverage probability between 0 and 1 as its second argument, got {}",
            other.type_name()
        ))),
    }
}

/// `dof(x)`: the effective degrees of freedom of a measurement (`inf` when it has no finite ones).
fn dof(args: &[PhsValue]) -> PhysureResult<PhsValue> {
    let [x] = args else { return Err(arity("dof", 1)) };
    Ok(PhsValue::Number(as_quantity("dof", x)?.effective_dof()?))
}

/// `coverage_factor(x, p)`: the Student-t factor for the coverage probability `p`.
fn coverage_factor(args: &[PhsValue]) -> PhysureResult<PhsValue> {
    let [x, p] = args else { return Err(arity("coverage_factor", 2)) };
    let p = probability("coverage_factor", p)?;
    Ok(PhsValue::Number(as_quantity("coverage_factor", x)?.coverage_factor(p)?))
}

/// `expanded(x, p)`: the expanded uncertainty `U = k u_c`, in the unit of `x`, as an exact bound.
fn expanded(args: &[PhsValue]) -> PhysureResult<PhsValue> {
    let [x, p] = args else { return Err(arity("expanded", 2)) };
    let p = probability("expanded", p)?;
    let q = as_quantity("expanded", x)?;
    let u = q.expanded(p)?.expanded_uncertainty;
    Ok(match x {
        PhsValue::Number(_) => PhsValue::Number(u),
        _ => PhsValue::Quantity(Quantity::new_scalar(u, 0.0, q.unit, None, None)),
    })
}

/// `from_samples([x1, x2, ...] unit)`: a Type A evaluation, the mean with its standard error and `n - 1`
/// degrees of freedom. The observations are exact numbers or quantities of one dimension, taken in the
/// unit of the first.
fn from_samples(args: &[PhsValue]) -> PhysureResult<PhsValue> {
    let [PhsValue::Vector(items)] = args else {
        return Err(PhysureError::Generic(
            "from_samples expects a vector of observations, e.g. from_samples([9.8, 9.9, 10.1] m)".into(),
        ));
    };
    let mut observations = Vec::with_capacity(items.len());
    for item in items {
        observations.push(as_quantity("from_samples", item)?);
    }
    let Some(first) = observations.first() else {
        return Err(PhysureError::DomainError("a Type A evaluation needs at least two observations, got 0".into()));
    };
    let unit = first.unit.clone();
    let mut values = Vec::with_capacity(observations.len());
    for observation in &observations {
        if observation.value.std_dev() > 0.0 {
            return Err(PhysureError::Generic(
                "the observations of from_samples must be exact: it estimates the uncertainty from their spread".into(),
            ));
        }
        let converted = observation.convert_to(&unit).map_err(|_| {
            PhysureError::Generic(format!(
                "from_samples needs observations in one unit, got '{}' and '{}'",
                unit.__repr__(),
                observation.unit.__repr__()
            ))
        })?;
        values.push(converted.value.mean());
    }
    Ok(PhsValue::Quantity(Quantity::from_samples_in(&values, unit)?))
}
