//! The built-in witness battery (spec §8, Phase 3): reference calculations whose results a frozen file records bit
//! for bit as `builtin.<id>` and that `phs conf verify --run` evaluates again.
//!
//! # Rules for a case
//!
//! * **An id is forever.** A release may add cases; it never changes what an id computes. Ids are lowercase
//!   `[a-z0-9_.-]`, `<area>.<what>`, and [`battery`] returns the cases in id order (decision S2: the order of
//!   evaluation is the same everywhere).
//! * **Through the hooked catalog only.** A case obtains units, constants and atomic weights from the configuration
//!   in force (`Quantity::new`, `Quantity::to`, the parser, `units::conf::try_with_conf_registry`, the chemistry
//!   functions), never from a registry it builds itself, so a changed override changes the witnesses that depend on
//!   it.
//! * **Every setting it depends on is pinned**, with a thread-local scope (`uncertainty::mode::scoped`,
//!   `units::angle_mode::scoped`, `settings::scoped`), never with the process-wide setters: a frozen file applies its
//!   `[Settings]` process-wide, and a case must not depend on them nor change them. The units and constants cases
//!   read no setting (an exact value is Gaussian whatever the propagation mode; no angle is involved).
//! * **Deterministic:** no clock, no environment, no threads, a fixed seed wherever a generator is used.
//! * **The stored number** is `Quantity::display_parts()`: the magnitude (with the scale of an anonymous compound
//!   unit folded in), the standard uncertainty and the unit text.
//!
//! # How a case reports
//!
//! A case returns [`CaseResult`]:
//!
//! * `Ok(value)`: its result.
//! * `Err(CaseError::Failed(message))`: the computation legitimately refused under this configuration (a user's
//!   atomic-weight table without Ca, a constant the overrides removed). It is **recorded** as
//!   `WitnessValue::Failed` (`failed=<first line>`, decision S6) and compared like any value; it does not block
//!   `freeze`. A `PhysureError` converts into this with `?`.
//! * `Err(CaseError::Error(message))`: the case could not be evaluated meaningfully (a limit was hit, the case is
//!   broken). It is an error row (`Err` from [`run_cases`]) that refuses `freeze`.
//! * A **panic** is caught (`catch_unwind`) and is that case's error, never an abort of the run. This needs the
//!   default `panic = "unwind"`; under `panic = "abort"` a panicking case ends the process.
//!
//! When the configuration in force cannot be used at all (a broken frozen file), [`run_cases`] evaluates nothing and
//! every case is an error naming why.

pub mod units;

use std::panic::{catch_unwind, AssertUnwindSafe};

use super::witness::WitnessValue;
use crate::error::PhysureError;

/// Why a case gave no value: see the module documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseError {
    /// A legitimate refusal under this configuration: recorded as `WitnessValue::Failed`.
    Failed(String),
    /// The case could not be evaluated (a limit, a broken case): an error row.
    Error(String),
}

impl From<PhysureError> for CaseError {
    fn from(e: PhysureError) -> CaseError {
        CaseError::Failed(e.to_string())
    }
}

pub type CaseResult = Result<WitnessValue, CaseError>;

/// One reference calculation.
#[derive(Debug, Clone, Copy)]
pub struct Case {
    /// The stable id; the `[Witness]` key is `builtin.<id>`.
    pub id: &'static str,
    /// The group it belongs to (`units`, `const`, ...), for reports and the guide.
    pub area: &'static str,
    pub run: fn() -> CaseResult,
}

/// Every built-in case, in id order.
pub fn battery() -> Vec<Case> {
    let mut cases = units::cases();
    cases.sort_by_key(|case| case.id);
    cases
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a panic without a message".to_string()
    }
}

/// Runs `cases` in the order given, each on its own under `catch_unwind`, on the calling thread. The key of each row
/// is `builtin.<id>`; `Ok` is the value to record or compare (a legitimate failure is `Ok(WitnessValue::Failed)`),
/// `Err` is an error row (a panic, a limit, a configuration that cannot be used).
pub fn run_cases(cases: &[Case]) -> Vec<(String, Result<WitnessValue, String>)> {
    let key = |case: &Case| format!("builtin.{}", case.id);
    if let Err(e) = crate::units::conf::try_with_conf_registry(|_| ()) {
        let why = format!("not run: the configuration in force cannot be used: {e}");
        return cases.iter().map(|case| (key(case), Err(why.clone()))).collect();
    }
    cases
        .iter()
        .map(|case| {
            let result = match catch_unwind(AssertUnwindSafe(case.run)) {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(CaseError::Failed(message))) => Ok(WitnessValue::failed(&message)),
                Ok(Err(CaseError::Error(message))) => Err(message),
                Err(payload) => Err(format!("panic: {}", panic_text(payload.as_ref()))),
            };
            (key(case), result)
        })
        .collect()
}

/// [`run_cases`] over the whole [`battery`].
pub fn run_battery() -> Vec<(String, Result<WitnessValue, String>)> {
    run_cases(&battery())
}
