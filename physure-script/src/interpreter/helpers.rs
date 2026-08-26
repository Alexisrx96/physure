use physure_core::error::{PhysureError, PhysureResult};
use physure_core::quantity::Quantity;
use physure_core::units::RationalUnit;
use crate::value::PhsValue;

/// The magnitude a range endpoint denotes, or `None` when it denotes none.
pub(crate) fn range_endpoint(val: &PhsValue) -> Option<Quantity> {
    match val {
        PhsValue::Quantity(q) => Some(q.clone()),
        PhsValue::Number(n) => Some(Quantity::new_scalar(*n, 0.0, RationalUnit::dimensionless(), None, None)),
        _ => None,
    }
}

/// Builds `min .. max`, after checking that it names an interval at all.
///
/// An endpoint with no dimension of its own takes the other's unit, so `0 .. 100 m` reads
/// as `0 m .. 100 m` — on paper the lower bound of an interval does not repeat the unit
/// either. Everything else is refused rather than repaired: a range whose sides measure
/// different things, one that does not run upwards, and anything that is not a magnitude.
/// A missing endpoint never reaches here; the grammar requires both.
pub(crate) fn make_range(l_val: PhsValue, r_val: PhsValue) -> PhysureResult<PhsValue> {
    let (Some(mut min), Some(mut max)) = (range_endpoint(&l_val), range_endpoint(&r_val)) else {
        return Err(PhysureError::Generic(format!(
            "A range runs between two magnitudes, and `{} .. {}` has something else on at least one side",
            l_val, r_val,
        )));
    };

    if min.unit.dimensions.is_empty() && !max.unit.dimensions.is_empty() {
        min.unit = max.unit.clone();
    } else if max.unit.dimensions.is_empty() && !min.unit.dimensions.is_empty() {
        max.unit = min.unit.clone();
    } else if !min.unit.same_dimensions(&max.unit) {
        return Err(PhysureError::IncompatibleDimensions {
            op: "range",
            dim1: min.unit.__repr__(),
            dim2: max.unit.__repr__(),
        });
    }

    let (lo, hi) = (min.canonical_magnitude(), max.canonical_magnitude());
    if lo.is_nan() || hi.is_nan() {
        return Err(PhysureError::Generic(format!(
            "A range needs two magnitudes that can be ordered, and `{} .. {}` has one that cannot",
            min, max,
        )));
    }
    if lo >= hi {
        return Err(PhysureError::Generic(format!(
            "A range runs from its minimum to its maximum: `{}` is not below `{}`",
            min, max,
        )));
    }

    // A bare number stays a bare number when nothing was adopted — a dimensionless range is
    // written `-2 .. 2` and the consumers that read one distinguish the two cases.
    let rewrap = |original: PhsValue, q: Quantity| match original {
        PhsValue::Number(_) if q.unit.dimensions.is_empty() => original,
        _ => PhsValue::Quantity(q),
    };
    Ok(PhsValue::Range(
        Box::new(rewrap(l_val, min)),
        Box::new(rewrap(r_val, max)),
    ))
}

pub(crate) fn is_truthy(val: &PhsValue) -> bool {
    match val {
        PhsValue::Quantity(q) => q.value.mean().abs() > 1e-15,
        PhsValue::Number(n) => n.abs() > 1e-15,
        PhsValue::Bool(b) => *b,
        PhsValue::String(s) => s == "true" || s == "True" || s == "1",
        _ => false,
    }
}


/// Trailing `#` / `//` comments survive into a unit annotation's text; the unit parser
/// must never see them.
pub(crate) fn strip_unit_comment(text: &str) -> &str {
    text.split('#').next().unwrap().split("//").next().unwrap().trim()
}

