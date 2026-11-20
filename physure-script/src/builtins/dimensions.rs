//! The one place the interpreter asks whether two numeric values have one dimension (the unit holes of `max`, `min`,
//! `vector`, `linspace`, `round`, and the invariant that a vector holds one dimension), and the words that say they
//! do not.
//!
//! Two values have one dimension when the comparison `<` would set them side by side
//! (`Quantity::comparable_magnitudes`): the same base dimensions (`RationalUnit::same_dimensions`, the test `m + s`
//! is refused by), and a level (`dB`, `pH`) never against a dimensionless value that is not one. So a plain number
//! is dimensionless, so is a ratio in a unit (`50 percent`), an angle (`rad`) is a dimension of its own in every angle
//! mode, `20 dB` and `2` are refused as `20 dB > 2` is, and two levels compare as `<` compares them.

use std::sync::OnceLock;

use physure_core::units::{formula_text, RationalUnit};
use physure_core::Quantity;

use crate::value::PhsValue;

/// The unit of a number (dimensionless) or a quantity, borrowed; `None` for any other value.
fn unit_of(value: &PhsValue) -> Option<&RationalUnit> {
    static DIMENSIONLESS: OnceLock<RationalUnit> = OnceLock::new();
    match value {
        PhsValue::Number(_) => Some(DIMENSIONLESS.get_or_init(RationalUnit::dimensionless)),
        PhsValue::Quantity(q) => Some(&q.unit),
        _ => None,
    }
}

/// Whether `value` is a number or a quantity, the values that have a dimension.
pub(crate) fn is_numeric(value: &PhsValue) -> bool {
    unit_of(value).is_some()
}

/// A number, or a dimensionless quantity that is not a level, as the plain number it stands for (`50 percent` is
/// 0.5); `None` for anything else. What a count or a number of decimals must be.
pub(crate) fn plain_number(value: &PhsValue) -> Option<f64> {
    match value {
        PhsValue::Number(n) => Some(*n),
        PhsValue::Quantity(q) if q.unit.dimensions.is_empty() && !q.unit.is_logarithmic() => Some(q.canonical_magnitude()),
        _ => None,
    }
}

/// [`plain_number`] when it is a finite whole number (`3`, `-1`, `300 percent`), `None` otherwise.
pub(crate) fn whole_number(value: &PhsValue) -> Option<f64> {
    plain_number(value).filter(|n| n.is_finite() && n.fract() == 0.0)
}

/// Whether two numbers or quantities have one dimension, by the rule of the module.
fn comparable(a: &PhsValue, ua: &RationalUnit, b: &PhsValue, ub: &RationalUnit) -> bool {
    if !ua.same_dimensions(ub) {
        return false;
    }
    if !ua.is_logarithmic() && !ub.is_logarithmic() {
        return true;
    }
    // a level is in it: the comparison of the core says whether the two have an order
    let as_quantity = |v: &PhsValue| match v {
        PhsValue::Quantity(q) => q.clone(),
        PhsValue::Number(n) => Quantity::new_scalar(*n, 0.0, RationalUnit::dimensionless(), None, None),
        _ => unreachable!("only numbers and quantities have a unit"),
    };
    as_quantity(a).comparable_magnitudes(&as_quantity(b)).is_ok()
}

/// Why two numeric values do not stand side by side: the clause that says so, both values as they print.
#[derive(Debug, PartialEq)]
pub(crate) enum Mismatch {
    /// Different dimensions, in words: `` `1 m` is a length and `1 s` a time ``.
    Dimension(String),
    /// One dimension, but a temperature point against a temperature interval, the pair the comparison of the core
    /// refuses (`RationalUnit::point_against_interval`): `` `5 delta_degC` is a temperature interval and `20 degC` a
    /// temperature point ``.
    TemperatureKind(String),
}

/// `None` when `a` and `b` have one dimension and one kind of temperature, or when either is not a number or a
/// quantity (nothing to compare); otherwise why not ([`Mismatch`]).
pub(crate) fn mismatch(a: &PhsValue, b: &PhsValue) -> Option<Mismatch> {
    let (ua, ub) = (unit_of(a)?, unit_of(b)?);
    if comparable(a, ua, b, ub) {
        return ua.point_against_interval(ub).then(|| kind_clash(a, ua, b, ub));
    }
    let (mut wa, mut wb) = (dimension_words(ua), dimension_words(ub));
    if wa == wb {
        // the words do not tell them apart (two markers read as one dimension): the units do
        wa = format!("{wa} in `{}`", ua.__repr__());
        wb = format!("{wb} in `{}`", ub.__repr__());
    }
    Some(Mismatch::Dimension(format!("`{}` is {wa} and `{}` {wb}", written(a), written(b))))
}

/// The [`Mismatch::TemperatureKind`] clause of a point and an interval, in the order given.
fn kind_clash(a: &PhsValue, ua: &RationalUnit, b: &PhsValue, ub: &RationalUnit) -> Mismatch {
    let kind = |unit: &RationalUnit| if unit.is_affine() { "a temperature point" } else { "a temperature interval" };
    Mismatch::TemperatureKind(format!("`{}` is {} and `{}` {}", written(a), kind(ua), written(b), kind(ub)))
}

/// Two numeric items of `items` that do not stand side by side ([`mismatch`]), in one pass: every numeric item is set
/// against the first one for its dimension, and the first temperature point and the first temperature interval of the
/// list against each other. One kind of temperature is not transitive: `K` stands for either, so `[1 K, 5
/// delta_degC, 20 degC]` holds a point and an interval although each element matches the first. A different
/// dimension is said before a kind. Items that are not numbers or quantities (text, the rows of a matrix) are not
/// compared.
pub(crate) fn first_mismatch(items: &[PhsValue]) -> Option<Mismatch> {
    let mut numeric = items.iter().filter_map(|item| unit_of(item).map(|unit| (item, unit))).enumerate();
    let (_, (first, _)) = numeric.clone().next()?;
    // the first point and the first interval, with their place in the list
    let (mut point, mut interval) = (None, None);
    for (place, (item, unit)) in numeric.by_ref() {
        if let Some(Mismatch::Dimension(why)) = mismatch(first, item) {
            return Some(Mismatch::Dimension(why));
        }
        if point.is_none() && unit.is_affine() {
            point = Some((place, item, unit));
        }
        if interval.is_none() && unit.is_interval() {
            interval = Some((place, item, unit));
        }
    }
    let (Some(p), Some(i)) = (point, interval) else { return None };
    let ((_, a, ua), (_, b, ub)) = if p.0 < i.0 { (p, i) } else { (i, p) };
    Some(kind_clash(a, ua, b, ub))
}

/// A value as it prints, with a whole number written without its `.0` (`1 m`, `2`), so that the message shows what
/// was most likely written.
fn written(value: &PhsValue) -> String {
    let short = |n: f64| if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { physure_core::quantity::format_float(n) };
    match value {
        PhsValue::Number(n) => short(*n),
        PhsValue::Quantity(q) => {
            let (magnitude, uncertainty, unit) = q.display_parts();
            match (uncertainty == 0.0, unit.is_empty() || unit == "Dimensionless") {
                (true, true) => short(magnitude),
                (true, false) => format!("{} {unit}", short(magnitude)),
                (false, _) => q.to_string(),
            }
        }
        other => other.to_string(),
    }
}

/// A dimension in words: a level as one (`a level`, `a level of L²·M·T⁻³`), the angles by their kind (`an angle`,
/// `a solid angle`), a base dimension by its name (`a length`), `dimensionless`, anything else by its formula
/// (`of dimension L·T⁻¹`), read from the catalog in force as `phs eval` reports a dimension.
fn dimension_words(unit: &RationalUnit) -> String {
    let level = unit.is_logarithmic();
    if unit.dimensions.is_empty() {
        return if level { "a level" } else { "dimensionless" }.to_string();
    }
    let dimension = crate::globals::with_catalog_in_force(|registry| registry.dimension_of(unit).ok())
        .ok()
        .flatten()
        .unwrap_or_else(|| unit.dimensions.iter().cloned().collect());
    let formula = formula_text(&dimension);
    if level {
        return format!("a level of {formula}");
    }
    let marker = match unit.dimensions.as_slice() {
        [(symbol, (1, 1))] if symbol == "rad" => Some("an angle"),
        [(symbol, (1, 1))] if symbol == "sr" => Some("a solid angle"),
        _ => None,
    };
    let named = marker.or(match dimension.as_slice() {
        [(symbol, (1, 1))] => match symbol.as_str() {
            "L" => Some("a length"),
            "M" => Some("a mass"),
            "T" => Some("a time"),
            "I" => Some("an electric current"),
            "O" => Some("a temperature"),
            "N" => Some("an amount of substance"),
            "J" => Some("a luminous intensity"),
            "A" => Some("an angle"),
            _ => None,
        },
        _ => None,
    });
    named.map_or_else(|| format!("of dimension {formula}"), str::to_string)
}
