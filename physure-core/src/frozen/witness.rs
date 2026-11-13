//! Witnesses (spec §8): reference results stored bit for bit in the sealed `[Witness]` block, and the comparison of
//! what a file recorded with what the running build computes.
//!
//! A `[Witness]` value is one of (decision S10 of the Phase 3 plan: each prefix is a prefix of the **whole** value):
//!
//! * `<mean>:<uncertainty>:<unit>`: a number, as the IEEE-754 bit patterns of its mean and standard uncertainty, 16
//!   lowercase hexadecimal digits each, and the unit text (possibly empty, possibly holding `:`; the value is split at
//!   its first two colons);
//! * `text=<one line>`: a result that is not a number (a balancing);
//! * `failed=<one line>`: a case that legitimately returns an error under the configuration (decision S6), recorded
//!   and compared like any other value;
//! * `sha256:<64 lowercase hex>`: the hash of a user script's canonical result.
//!
//! Keys are `builtin.<id>` (id: `[a-z0-9_.-]`) for the built-in battery and `user.<name>` for user witnesses. A
//! `user.*` value is always a hash, a `builtin.*` value never is.
//!
//! Comparison has no tolerance: a difference is any difference of bits. It carries the distance in ulps so a person
//! can tell a last-digit difference from a changed formula.

use super::model::{is_name, Pairs};

/// The result of one witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WitnessValue {
    /// The mean and the standard uncertainty as bit patterns, and the unit text.
    Number { mean: u64, uncertainty: u64, unit: String },
    /// A result that is not a number (a balancing): one line of text.
    Text(String),
    /// A case that returned an error under the configuration: the first line of the error.
    Failed(String),
    /// The lowercase hexadecimal SHA-256 (without the `sha256:` prefix) of a user script's canonical result.
    Hash(String),
}

impl WitnessValue {
    /// A number from its mean, standard uncertainty and unit text.
    pub fn number(mean: f64, uncertainty: f64, unit: &str) -> WitnessValue {
        WitnessValue::Number { mean: mean.to_bits(), uncertainty: uncertainty.to_bits(), unit: unit.to_string() }
    }

    /// A failure from any error text: its first non-blank line, trimmed, with control characters replaced, so that
    /// it can be stored as a `[Witness]` value.
    pub fn failed(message: &str) -> WitnessValue {
        WitnessValue::Failed(one_line(message, "an error without a message"))
    }

    /// A text result from any text, made storable the same way as [`WitnessValue::failed`].
    pub fn text(text: &str) -> WitnessValue {
        WitnessValue::Text(one_line(text, "(empty)"))
    }
}

fn one_line(text: &str, empty: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or(empty);
    line.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string()
}

const TEXT: &str = "text=";
const FAILED: &str = "failed=";
const HASH: &str = "sha256:";

/// The `[Witness]` value of `v`. For a value [`parse_value`] accepts, `parse_value(&format_value(v)) == Ok(v)`; the
/// constructors [`WitnessValue::text`] and [`WitnessValue::failed`] make texts that it accepts.
pub fn format_value(v: &WitnessValue) -> String {
    match v {
        WitnessValue::Number { mean, uncertainty, unit } => format!("{mean:016x}:{uncertainty:016x}:{unit}"),
        WitnessValue::Text(text) => format!("{TEXT}{text}"),
        WitnessValue::Failed(text) => format!("{FAILED}{text}"),
        WitnessValue::Hash(hex) => format!("{HASH}{hex}"),
    }
}

fn is_lower_hex(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Why `text` cannot be stored as one line of a value, if it cannot.
fn line_problem(what: &str, text: &str, may_be_empty: bool) -> Result<(), String> {
    if text.is_empty() && !may_be_empty {
        return Err(format!("{what} is empty"));
    }
    if text.chars().any(char::is_control) {
        return Err(format!("{what} contains a control character: {text:?}"));
    }
    if text.trim() != text {
        return Err(format!("{what} starts or ends with whitespace: {text:?}"));
    }
    Ok(())
}

/// Reads a `[Witness]` value strictly (see the module documentation).
pub fn parse_value(s: &str) -> Result<WitnessValue, String> {
    if let Some(text) = s.strip_prefix(TEXT) {
        line_problem("the text", text, false)?;
        return Ok(WitnessValue::Text(text.to_string()));
    }
    if let Some(text) = s.strip_prefix(FAILED) {
        line_problem("the failure", text, false)?;
        return Ok(WitnessValue::Failed(text.to_string()));
    }
    if let Some(hex) = s.strip_prefix(HASH) {
        if !is_lower_hex(hex, 64) {
            return Err(format!("expected sha256: and 64 lowercase hexadecimal digits, found {s:?}"));
        }
        return Ok(WitnessValue::Hash(hex.to_string()));
    }
    let mut parts = s.splitn(3, ':');
    let (Some(mean), Some(uncertainty), Some(unit)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(format!(
            "expected <mean>:<uncertainty>:<unit> (16 lowercase hexadecimal digits each), text=, failed= or sha256:, found {s:?}"
        ));
    };
    for (what, bits) in [("mean", mean), ("uncertainty", uncertainty)] {
        if !is_lower_hex(bits, 16) {
            return Err(format!("the {what} must be 16 lowercase hexadecimal digits, found {bits:?} in {s:?}"));
        }
    }
    line_problem("the unit", unit, true)?;
    let bits = |text: &str| u64::from_str_radix(text, 16).map_err(|e| format!("{text:?}: {e}"));
    Ok(WitnessValue::Number { mean: bits(mean)?, uncertainty: bits(uncertainty)?, unit: unit.to_string() })
}

/// Whether `id` is a battery id: non-empty, `[a-z0-9_.-]`.
pub fn is_builtin_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-'))
}

/// Why a `[Witness]` pair is not a witness entry, if it is not: the key must be `builtin.<id>` or `user.<name>`,
/// the value must read ([`parse_value`]), a `user.*` value is a hash and a `builtin.*` value is not.
pub fn check_entry(key: &str, value: &str) -> Result<(), String> {
    let user = if let Some(id) = key.strip_prefix("builtin.") {
        if !is_builtin_id(id) {
            return Err(format!("the key {key} is not builtin.<id> with an id of a-z 0-9 _ . -"));
        }
        false
    } else if let Some(name) = key.strip_prefix("user.") {
        if !is_name(name) {
            return Err(format!("the key {key} is not user.<name> with a name of letters, digits and _ . : / -"));
        }
        true
    } else {
        return Err(format!("the key {key} is neither builtin.<id> nor user.<name>"));
    };
    let parsed = parse_value(value).map_err(|e| format!("the value of {key} does not read: {e}"))?;
    match (user, matches!(parsed, WitnessValue::Hash(_))) {
        (true, false) => Err(format!("the value of {key} must be sha256:<hex> (a user witness is a hash), found {value:?}")),
        (false, true) => Err(format!("the value of {key} must not be sha256:<hex> (a built-in witness is a result), found {value:?}")),
        _ => Ok(()),
    }
}

/// The distance between two finite floats in units in the last place: how many representable values lie between
/// them, plus one (`-0.0` and `0.0` are 0 apart). `None` when either is not finite, or when their signs differ and
/// neither is zero.
pub fn ulp_distance(a: f64, b: f64) -> Option<u64> {
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    if a != 0.0 && b != 0.0 && a.is_sign_negative() != b.is_sign_negative() {
        return None;
    }
    // map the bit patterns to a scale that is monotonic in the value, with both zeros at 0
    let ordered = |x: f64| -> i128 {
        let magnitude = i128::from(x.to_bits() & !(1u64 << 63));
        if x.is_sign_negative() {
            -magnitude
        } else {
            magnitude
        }
    };
    Some((ordered(a) - ordered(b)).unsigned_abs() as u64)
}

/// What the comparison found for one witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WitnessStatus {
    Match,
    /// The formatted recorded and running values, and the larger of the ulp distances of the means and of the
    /// uncertainties when both are numbers in the same unit text (`None` otherwise).
    Differs { recorded: String, running: String, ulps: Option<u64> },
    /// Recorded, but the running build does not have it: a discrepancy.
    Missing,
    /// The running build has it, the file does not: informational.
    New,
    /// It could not be compared: the running case panicked or hit a limit, or the recorded entry is not a witness.
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessRow {
    pub key: String,
    pub status: WitnessStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WitnessReport {
    pub rows: Vec<WitnessRow>,
}

impl WitnessReport {
    /// Every row is `Match` or `New`.
    pub fn all_match(&self) -> bool {
        self.rows.iter().all(|r| matches!(r.status, WitnessStatus::Match | WitnessStatus::New))
    }

    /// `(differ, new, missing, error)`.
    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let mut counts = (0, 0, 0, 0);
        for row in &self.rows {
            match row.status {
                WitnessStatus::Match => {}
                WitnessStatus::Differs { .. } => counts.0 += 1,
                WitnessStatus::New => counts.1 += 1,
                WitnessStatus::Missing => counts.2 += 1,
                WitnessStatus::Error(_) => counts.3 += 1,
            }
        }
        counts
    }
}

fn number_ulps(recorded: &WitnessValue, running: &WitnessValue) -> Option<u64> {
    match (recorded, running) {
        (
            WitnessValue::Number { mean: m1, uncertainty: u1, unit: a },
            WitnessValue::Number { mean: m2, uncertainty: u2, unit: b },
        ) if a == b => {
            let mean = ulp_distance(f64::from_bits(*m1), f64::from_bits(*m2))?;
            let uncertainty = ulp_distance(f64::from_bits(*u1), f64::from_bits(*u2))?;
            Some(mean.max(uncertainty))
        }
        _ => None,
    }
}

/// Compares the file's `[Witness]` pairs with freshly computed results (`Err` = the case could not run: a panic, a
/// limit). Rows: the recorded keys in their order, then the running keys the file does not record. A running key
/// that appears twice counts once (the first).
pub fn compare(recorded: &Pairs, running: &[(String, Result<WitnessValue, String>)]) -> WitnessReport {
    let find = |key: &str| running.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let mut rows = Vec::new();
    for (key, value) in recorded.iter() {
        let status = if let Err(e) = check_entry(key, value) {
            if parse_value(value).is_err() {
                WitnessStatus::Error(format!("recorded value does not read: {e}"))
            } else {
                WitnessStatus::Error(format!("not a witness entry (expected builtin.<id> or user.<name>): {e}"))
            }
        } else {
            let recorded_value = parse_value(value).expect("checked above");
            match find(key) {
                None => WitnessStatus::Missing,
                Some(Err(e)) => WitnessStatus::Error(e.clone()),
                Some(Ok(v)) if *v == recorded_value => WitnessStatus::Match,
                Some(Ok(v)) => WitnessStatus::Differs {
                    recorded: format_value(&recorded_value),
                    running: format_value(v),
                    ulps: number_ulps(&recorded_value, v),
                },
            }
        };
        rows.push(WitnessRow { key: key.to_string(), status });
    }
    for (key, value) in running {
        if recorded.get(key).is_some() || rows.iter().any(|r: &WitnessRow| r.key == *key) {
            continue;
        }
        let status = match value {
            Ok(_) => WitnessStatus::New,
            Err(e) => WitnessStatus::Error(e.clone()),
        };
        rows.push(WitnessRow { key: key.clone(), status });
    }
    WitnessReport { rows }
}
