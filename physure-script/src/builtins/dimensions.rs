//! The one place a builtin asks whether two numeric values have one dimension (the unit holes of `max`, `min`,
//! `vector`, `linspace`, `round`), and the words that say they do not.
//!
//! The comparison is the one `m + s` is refused by (`Quantity::add` asks `RationalUnit::same_dimensions`): a plain
//! number is dimensionless, so is a ratio in a unit (`50 percent`), and an angle (`rad`) is a dimension of its own in
//! every angle mode, as it is for a sum and a comparison.

use physure_core::units::{formula_text, RationalUnit};

use crate::value::PhsValue;

/// The unit of a number (dimensionless) or a quantity; `None` for any other value.
fn unit_of(value: &PhsValue) -> Option<RationalUnit> {
    match value {
        PhsValue::Number(_) => Some(RationalUnit::dimensionless()),
        PhsValue::Quantity(q) => Some(q.unit.clone()),
        _ => None,
    }
}

/// Whether `value` is a number or a quantity, the values that have a dimension.
pub(crate) fn is_numeric(value: &PhsValue) -> bool {
    unit_of(value).is_some()
}

/// `None` when `a` and `b` have one dimension, or when either is not a number or a quantity (nothing to compare);
/// otherwise the clause that says why not, both values as they print and their dimensions in words:
/// `` `1 m` is a length and `1 s` a time ``.
pub(crate) fn mismatch(a: &PhsValue, b: &PhsValue) -> Option<String> {
    let (ua, ub) = (unit_of(a)?, unit_of(b)?);
    if ua.same_dimensions(&ub) {
        return None;
    }
    Some(format!("`{}` is {} and `{}` {}", written(a), dimension_words(&ua), written(b), dimension_words(&ub)))
}

/// The first two numeric items of `items` whose dimensions differ, as the clause of [`mismatch`]: every numeric item
/// is set against the first one. Items that are not numbers or quantities (text, the rows of a matrix) are not
/// compared.
pub(crate) fn first_mismatch(items: &[PhsValue]) -> Option<String> {
    let mut numeric = items.iter().filter(|item| is_numeric(item));
    let first = numeric.next()?;
    numeric.find_map(|item| mismatch(first, item))
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

/// A dimension in words: a base dimension by its name (`a length`, `an angle`), `dimensionless`, anything else by
/// its formula (`of dimension L·T⁻¹`), read from the catalog in force as `phs eval` reports a dimension.
fn dimension_words(unit: &RationalUnit) -> String {
    if unit.dimensions.is_empty() {
        return "dimensionless".to_string();
    }
    let dimension = crate::globals::with_catalog_in_force(|registry| registry.dimension_of(unit).ok())
        .ok()
        .flatten()
        .unwrap_or_else(|| unit.dimensions.iter().cloned().collect());
    let named = match dimension.as_slice() {
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
    };
    named.map_or_else(|| format!("of dimension {}", formula_text(&dimension)), str::to_string)
}
