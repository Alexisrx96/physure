//! Printed text is a result (owner decision, conf freeze Phase 4b): the text the library writes for fixed values and
//! uncertainties across the display boundaries, recorded as `WitnessValue::Text` (`text=<line>`).
//!
//! # The settings these cases read, and how they are pinned
//!
//! Every case runs under the compiled display rules (`display_significant_digits = 15`, `display_scientific_below =
//! 1e-4`, `display_scientific_from = 1e16`, `uncertainty_digits = gum`) opened as a thread-local
//! [`scoped_display_rules`], and under the `correlated` propagation mode (an uncertain literal of a case is Gaussian
//! whatever the process says): a frozen file whose display keys differ prints differently **by design**, and these
//! witnesses do not move with it. They move when the engine's formatting changes: the 17-decimal cap, the
//! `{:e}` form, the GUM rounding, the fraction finder (`format_fraction`, not a setting), the folding of an anonymous
//! unit's scale. `display.folded_unit` and `display.base_units` read `m`, `km`, `kohm`, `degC` and `J` through the
//! hooked catalog like every other case.
//!
//! Each case writes several values joined by `"; "`; NaN is never recorded (it prints `NaN.0`, an open defect).

use super::{case, Case, CaseError, CaseResult};
use crate::frozen::witness::WitnessValue;
use crate::quantity::{format_float, format_fraction, gum_round, Quantity};
use crate::settings::{scoped_display_rules, DisplayRules};
use crate::uncertainty::mode::scoped;
use crate::uncertainty::PropagationMode;

/// Runs `f` under the compiled display rules and the correlated mode, on this thread only, and records the texts it
/// gives as one line.
fn pinned(f: impl FnOnce() -> Result<Vec<String>, CaseError>) -> CaseResult {
    let _display = scoped_display_rules(DisplayRules::DEFAULT);
    let _mode = scoped(PropagationMode::Correlated);
    Ok(WitnessValue::text(&f()?.join("; ")))
}

fn floats(values: &[f64]) -> CaseResult {
    pinned(|| Ok(values.iter().map(|&x| format_float(x)).collect()))
}

/// `mean ± u` as `gum_round` writes the two halves.
fn gum(pairs: &[(f64, f64)]) -> CaseResult {
    pinned(|| {
        Ok(pairs
            .iter()
            .map(|&(mean, u)| {
                let (mean, u) = gum_round(mean, u, None);
                format!("{mean} ± {u}")
            })
            .collect())
    })
}

/// The `f64` one ulp from `x` (positive `x`): `step = 1` above, `-1` below.
fn ulp(x: f64, step: i64) -> f64 {
    f64::from_bits(x.to_bits().wrapping_add_signed(step))
}

fn measured(mean: f64, u: f64, unit: &str) -> Result<Quantity, CaseError> {
    Ok(Quantity::with_uncertainty(mean, u, unit)?)
}

fn exact(mean: f64, unit: &str) -> Result<Quantity, CaseError> {
    Ok(Quantity::new(mean, unit)?)
}

pub(super) fn cases() -> Vec<Case> {
    vec![
        // the rounding to 15 significant digits that shortens conversion debris
        case("display.decimal_debris", || floats(&[0.1 + 0.2, 89.99999999999999, 1.0 / 3.0, 100.0 / 3.0, 1.1 * 1.1])),
        // the lower scientific threshold and its neighbours
        case("display.scientific_below", || {
            floats(&[1e-4, ulp(1e-4, -1), 9.99e-5, 1.0001e-4, -1e-5, 1.2345678901234567e-5])
        }),
        // the upper scientific threshold and its neighbours
        case("display.scientific_from", || floats(&[9999999999999998.0, 1e16, ulp(1e16, 1), -1e16, 6.02214076e23])),
        // whole numbers where the 15-digit rounding meets the integer digits
        case("display.integers_near_1e15", || {
            floats(&[1e15, 999999999999999.0, 1e15 + 1.0, 1234567890123456.0, 123456789012345.67])
        }),
        // negative zero, the infinities, the subnormals and the largest finite value
        case("display.extremes", || {
            floats(&[-0.0, f64::INFINITY, f64::NEG_INFINITY, f64::from_bits(1), f64::MIN_POSITIVE, f64::MAX])
        }),
        // the Guide's one-or-two-figure rule at its boundaries
        case("display.gum_figures", || gum(&[(10.0, 0.95), (10.0, 0.0995), (10.0, 0.15), (10.0, 0.981), (10.0, 1.9), (10.0, 2.0), (10.0, 9.5)])),
        // the decimal place the rounded uncertainty sets: negative places, a negative mean, a mean far below its
        // uncertainty, a double-rounding case
        case("display.gum_places", || {
            pinned(|| {
                let mut out = vec![measured(625.0, 40.0, "J")?.to_string()];
                for (mean, u) in [(12345.0, 678.0), (-12.345, 0.23), (0.001, 5.0), (2.675, 0.01)] {
                    let (mean, u) = gum_round(mean, u, None);
                    out.push(format!("{mean} ± {u}"));
                }
                Ok(out)
            })
        }),
        // where the rounding stops: an uncertainty below 1e-4 or from 1e16, at 1e-4 and one ulp below, a large mean
        case("display.gum_range", || {
            pinned(|| {
                let mut out = vec![measured(0.00012, 1e-5, "m")?.to_string()];
                for (mean, u) in [(1.0, 1e-4), (1.0, ulp(1e-4, -1)), (1000.0, 1e16), (1e15, 2e14)] {
                    let (mean, u) = gum_round(mean, u, None);
                    out.push(format!("{mean} ± {u}"));
                }
                Ok(out)
            })
        }),
        // `@precision(N)`: decimal places on an exact value, figures of the uncertainty on a measured one
        case("display.precision", || {
            pinned(|| {
                Ok(vec![
                    exact(3.14159265, "m")?.to_string_with_precision(Some(1)),
                    exact(3.14159265, "m")?.to_string_with_precision(Some(20)),
                    exact(2.675, "m")?.to_string_with_precision(Some(2)),
                    measured(19.62, 0.981, "N")?.to_string_with_precision(Some(1)),
                    measured(19.62, 0.981, "N")?.to_string_with_precision(Some(20)),
                ])
            })
        }),
        // the scale of an anonymous compound unit folded into the value and the uncertainty
        case("display.folded_unit", || {
            pinned(|| {
                let km = exact(3.0, "km")?;
                Ok(vec![exact(2.0, "m")?.mul(&km)?.to_string(), measured(2.0, 0.1, "m")?.mul(&km)?.to_string()])
            })
        }),
        // `x: base`: the base units, never rounded by the uncertainty
        case("display.base_units", || {
            pinned(|| {
                Ok(vec![
                    measured(2.0, 0.1, "kohm")?.base_display(),
                    exact(0.0, "degC")?.base_display(),
                    measured(625.0, 40.0195264839553, "J")?.base_display(),
                ])
            })
        }),
        // the `frac` / `ifrac` format: fractions after the 15-digit cut (`format_fraction`, not a setting)
        case("display.fractions", || {
            pinned(|| {
                let none = || CaseError::Error("no fraction".to_string());
                [(1.5, false), (1.5, true), (-2.5, true), (0.1 + 0.2, false), (89.99999999999999, false)]
                    .into_iter()
                    .map(|(x, mixed)| format_fraction(x, mixed).ok_or_else(none))
                    .collect()
            })
        }),
    ]
}
