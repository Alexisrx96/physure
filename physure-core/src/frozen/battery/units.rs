//! Unit conversions (linear, affine and logarithmic) and catalog constants.
//!
//! Each conversion reads both units through the hooked catalog (`Quantity::new` and `Quantity::to` parse against
//! the configuration in force); each constant is the catalog's `[Constants]` entry as the configuration in force
//! states it, so an override of a unit or a constant shows in its witness. None reads a process setting.

use super::{case, Case, CaseError, CaseResult};
use crate::frozen::witness::WitnessValue;
use crate::quantity::Quantity;

/// The witness of a quantity: what `display_parts` shows.
pub(crate) fn witness_of(quantity: &Quantity) -> WitnessValue {
    let (mean, uncertainty, unit) = quantity.display_parts();
    WitnessValue::number(mean, uncertainty, &unit)
}

/// `value from` expressed in `to`.
fn convert(value: f64, from: &str, to: &str) -> CaseResult {
    Ok(witness_of(&Quantity::new(value, from)?.to(to)?))
}

/// The catalog constant `name` of the configuration in force, as a quantity.
pub(crate) fn constant(name: &str) -> Result<Quantity, CaseError> {
    let value = crate::chemistry::try_with_catalog_in_scope(|registry| registry.constants_meta.get(name).map(|m| m.value.clone()))?
        .ok_or_else(|| CaseError::Failed(format!("the catalog defines no constant {name}")))?;
    let (number, unit) = value.trim().split_once(char::is_whitespace).unwrap_or((value.trim(), ""));
    let number: f64 =
        number.parse().map_err(|_| CaseError::Failed(format!("the constant {name} is not a number and a unit: {value:?}")))?;
    Ok(Quantity::new(number, unit.trim())?)
}

fn read_constant(name: &str) -> CaseResult {
    Ok(witness_of(&constant(name)?))
}

pub(super) fn cases() -> Vec<Case> {
    vec![
        case("units.foot_to_metre", || convert(1.0, "ft", "m")),
        case("units.kmh_to_ms", || convert(1.0, "km/h", "m/s")),
        case("units.degc_to_k_25", || convert(25.0, "degC", "K")),
        case("units.degf_to_degc_212", || convert(212.0, "degF", "degC")),
        case("units.db_20_to_ratio", || convert(20.0, "dB", "unity")),
        case("units.psi_to_pa", || convert(1.0, "psi", "Pa")),
        case("units.kwh_to_j", || convert(1.0, "kWh", "J")),
        case("units.ev_to_j", || convert(1.0, "eV", "J")),
        case("units.atm_to_kpa", || convert(1.0, "atm", "kPa")),
        case("units.cal_th_to_j", || convert(1.0, "cal_th", "J")),
        case("units.nmi_per_h_to_ms", || convert(1.0, "nmi/h", "m/s")),
        case("units.ph_7_to_ratio", || convert(7.0, "pH", "unity")),
        case("const.molar_gas_constant", || read_constant("molar_gas_constant")),
        case("const.boltzmann_constant", || read_constant("boltzmann_constant")),
        case("const.planck_constant", || read_constant("planck_constant")),
        case("const.avogadro_constant", || read_constant("avogadro_constant")),
        case("const.speed_of_light_in_vacuum", || read_constant("speed_of_light_in_vacuum")),
        case("const.elementary_charge", || read_constant("elementary_charge")),
    ]
}
