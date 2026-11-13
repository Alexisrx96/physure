//! The built-in witness battery against references that do not come from the engine: each value below is written
//! as a literal from a primary source (NIST SP 811 (2008) Appendix B, the SI Brochure 9th edition (2019), CODATA
//! 2022 as cited in the catalog), and compared with an explicit tolerance in ulps (0 where the value is exact by
//! definition and the conversion is a single exactly-rounded operation).
//!
//! Every test runs the battery against the configuration in force, which is process-global (the current
//! directory, the cache of the configuration in force, the settings): they take one lock, and the test that
//! activates a frozen file restores the process through [`Env`].

// the references keep the digits of their source (an exact fraction written out), more than an f64 holds
#![allow(clippy::excessive_precision)]

mod common;

use common::set_compiled_defaults;
use physure_core::frozen::active::{ENV_CONF, ENV_REQUIRE};
use physure_core::frozen::witness::{is_builtin_id, ulp_distance, WitnessValue};
use physure_core::frozen::{
    active, battery, ensure_valid, freeze, reset_cache, run_battery, run_cases, set_engine_file, set_explicit_conf,
    write_atomic, Case, CaseError, CaseResult, EnginePolicy, FreezeOptions,
};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// (id, reference, tolerance in ulps, the unit text the case shows: the target unit as written, a constant's unit
/// as the catalog writes it; not a reference, a record of the format).
///
/// Sources:
/// * NIST SP 811 (2008), Appendix B.8 (factors in alphabetical order) and B.9 (by kind of quantity);
/// * SI Brochure, 9th edition (2019): the defining constants (Table 1) and t/°C = T/K - 273.15 (§2.3.1);
/// * CODATA 2022 (Mohr et al., Rev. Mod. Phys. 97, 025002 (2025)): exact values since the 2019 SI;
/// * IEC 80000-3:2019 / ISO 80000-1 Annex C: the decibel of a power quantity, L = 10 lg(P/P0) dB;
/// * IUPAC Gold Book, "pH": pH = -lg a(H+).
const REFERENCES: &[(&str, f64, u64, &str)] = &[
    // NIST SP 811 B.8: foot = 3.048 E-01 m, exact (international foot, 1959)
    ("units.foot_to_metre", 0.3048, 0, "m"),
    // NIST SP 811 B.8: kilometer per hour = 2.777 778 E-01 m/s; exactly 1000/3600 = 5/18
    ("units.kmh_to_ms", 0.277_777_777_777_777_78, 4, "m/s"),
    // SI Brochure §2.3.1: T = t + 273.15 K, exact
    ("units.degc_to_k_25", 298.15, 0, "K"),
    // NIST SP 811 B.9: t/°C = (t/°F - 32)/1.8, exact: 212 °F is 100 °C
    ("units.degf_to_degc_212", 100.0, 0, "degC"),
    // IEC 80000-3: 20 dB of a power ratio is 10^(20/10) = 100
    ("units.db_20_to_ratio", 100.0, 4, "unity"),
    // NIST SP 811 B.8: pound-force per square inch = 6.894 757 E+03 Pa; exactly
    // 0.45359237 kg * 9.80665 m/s^2 / (0.0254 m)^2 = 6894.757293168361336722673...
    ("units.psi_to_pa", 6_894.757_293_168_361_336_7, 4, "Pa"),
    // NIST SP 811 B.8: kilowatt hour = 3.6 E+06 J, exact
    ("units.kwh_to_j", 3.6e6, 0, "J"),
    // SI Brochure Table 8 / CODATA 2022: 1 eV = 1.602 176 634 E-19 J, exact
    ("units.ev_to_j", 1.602_176_634e-19, 0, "J"),
    // NIST SP 811 B.8: atmosphere, standard = 1.013 25 E+05 Pa, exact
    ("units.atm_to_kpa", 101.325, 4, "kPa"),
    // NIST SP 811 B.8: calorie (thermochemical) = 4.184 E+00 J, exact
    ("units.cal_th_to_j", 4.184, 0, "J"),
    // NIST SP 811 B.8: knot (nautical mile per hour) = 5.144 444 E-01 m/s; exactly 1852/3600 = 463/900
    ("units.nmi_per_h_to_ms", 0.514_444_444_444_444_44, 4, "m/s"),
    // IUPAC: pH 7 is an activity of 10^-7
    ("units.ph_7_to_ratio", 1e-7, 4, "unity"),
    // CODATA 2022 / SI 2019: R = N_A k = 6.022 140 76 E+23 * 1.380 649 E-23 = 8.314 462 618 153 24 J/(mol K), exact
    ("const.molar_gas_constant", 8.314_462_618_153_24, 0, "J*mol^-1*K^-1"),
    // SI Brochure Table 1: k = 1.380 649 E-23 J/K, exact
    ("const.boltzmann_constant", 1.380_649e-23, 0, "J*K^-1"),
    // SI Brochure Table 1: h = 6.626 070 15 E-34 J s (J/Hz), exact
    ("const.planck_constant", 6.626_070_15e-34, 0, "J*Hz^-1"),
    // SI Brochure Table 1: N_A = 6.022 140 76 E+23 mol^-1, exact
    ("const.avogadro_constant", 6.022_140_76e23, 0, "mol^-1"),
    // SI Brochure Table 1: c = 299 792 458 m/s, exact
    ("const.speed_of_light_in_vacuum", 299_792_458.0, 0, "m*s^-1"),
    // SI Brochure Table 1: e = 1.602 176 634 E-19 C, exact
    ("const.elementary_charge", 1.602_176_634e-19, 0, "C"),
];

fn value_of(rows: &[(String, Result<WitnessValue, String>)], id: &str) -> WitnessValue {
    let key = format!("builtin.{id}");
    let (_, value) = rows.iter().find(|(k, _)| *k == key).unwrap_or_else(|| panic!("no row {key}"));
    value.clone().unwrap_or_else(|e| panic!("{key} errored: {e}"))
}

fn mean_of(value: &WitnessValue) -> (f64, f64, String) {
    match value {
        WitnessValue::Number { mean, uncertainty, unit } => (f64::from_bits(*mean), f64::from_bits(*uncertainty), unit.clone()),
        other => panic!("not a number: {other:?}"),
    }
}

#[test]
fn every_units_and_constants_case_matches_its_independent_reference() {
    let _lock = lock();
    let rows = run_battery();
    let mut failures = Vec::new();
    for (id, reference, tolerance, _) in REFERENCES {
        let (mean, uncertainty, unit) = mean_of(&value_of(&rows, id));
        let distance = ulp_distance(mean, *reference);
        if distance.is_none_or(|d| d > *tolerance) {
            failures.push(format!("{id}: {mean:e} {unit} vs {reference:e}: {distance:?} ulps > {tolerance}"));
        }
        if uncertainty != 0.0 {
            failures.push(format!("{id}: an exact value has an uncertainty {uncertainty:e}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn the_unit_text_is_what_the_interpreter_shows() {
    let _lock = lock();
    let rows = run_battery();
    let found: Vec<(&str, String)> = REFERENCES.iter().map(|(id, ..)| (*id, mean_of(&value_of(&rows, id)).2)).collect();
    let expected: Vec<(&str, String)> = REFERENCES.iter().map(|(id, _, _, unit)| (*id, unit.to_string())).collect();
    assert_eq!(found, expected);
}

/// The ids of this task, which no later release may change or remove.
const STABLE_IDS: &[&str] = &[
    "const.avogadro_constant",
    "const.boltzmann_constant",
    "const.elementary_charge",
    "const.molar_gas_constant",
    "const.planck_constant",
    "const.speed_of_light_in_vacuum",
    "units.atm_to_kpa",
    "units.cal_th_to_j",
    "units.db_20_to_ratio",
    "units.degc_to_k_25",
    "units.degf_to_degc_212",
    "units.ev_to_j",
    "units.foot_to_metre",
    "units.kmh_to_ms",
    "units.kwh_to_j",
    "units.nmi_per_h_to_ms",
    "units.ph_7_to_ratio",
    "units.psi_to_pa",
];

#[test]
fn battery_ids_are_unique_stable_lowercase_and_every_case_runs() {
    let _lock = lock();
    let cases = battery();
    let ids: Vec<&str> = cases.iter().map(|c| c.id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids, sorted, "the battery is in id order and no id repeats");
    for case in &cases {
        assert!(is_builtin_id(case.id), "{} is not [a-z0-9_.-]", case.id);
        assert!(case.id.starts_with(&format!("{}.", case.area)), "{} is not in its area {}", case.id, case.area);
    }
    let present: BTreeSet<&str> = ids.iter().copied().collect();
    for id in STABLE_IDS {
        assert!(present.contains(id), "the stable id {id} is gone");
    }
    // every case of the battery has a reference here or in a later task's test; these are this task's
    for (id, ..) in REFERENCES {
        assert!(present.contains(id), "{id}");
    }
    let rows = run_battery();
    assert_eq!(rows.len(), cases.len());
    for ((key, value), case) in rows.iter().zip(&cases) {
        assert_eq!(key, &format!("builtin.{}", case.id));
        match value {
            Ok(WitnessValue::Failed(m)) => panic!("{key} failed under the embedded catalog: {m}"),
            Ok(_) => {}
            Err(e) => panic!("{key} errored: {e}"),
        }
    }
}

#[test]
fn the_battery_is_deterministic_run_twice_bit_for_bit() {
    let _lock = lock();
    let first = run_battery();
    let second = run_battery();
    assert_eq!(first, second);
    let other_thread = std::thread::spawn(run_battery).join().unwrap();
    assert_eq!(first, other_thread, "another thread computes the same bits");
}

fn panics() -> CaseResult {
    panic!("boom in a case")
}

fn fails() -> CaseResult {
    Err(CaseError::Failed("the catalog has no atomic weight for Ca\nsecond line".into()))
}

fn limited() -> CaseResult {
    Err(CaseError::Error("limit: more than 10 iterations".into()))
}

fn fine() -> CaseResult {
    Ok(WitnessValue::number(1.0, 0.0, "m"))
}

fn physure_error() -> CaseResult {
    Ok(WitnessValue::number(physure_core::Quantity::new(1.0, "zz_no_such_unit")?.value.mean(), 0.0, ""))
}

#[test]
fn a_panicking_case_is_an_error_row_not_an_abort() {
    let _lock = lock();
    let cases = [
        Case { id: "zz.panics", area: "zz", run: panics },
        Case { id: "zz.fails", area: "zz", run: fails },
        Case { id: "zz.limited", area: "zz", run: limited },
        Case { id: "zz.physure_error", area: "zz", run: physure_error },
        Case { id: "zz.fine", area: "zz", run: fine },
    ];
    let rows = run_cases(&cases);
    let keys: Vec<&str> = rows.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["builtin.zz.panics", "builtin.zz.fails", "builtin.zz.limited", "builtin.zz.physure_error", "builtin.zz.fine"]);
    assert!(matches!(&rows[0].1, Err(m) if m.starts_with("panic: ") && m.contains("boom in a case")), "{rows:?}");
    // a legitimate refusal is a recorded value: its first line
    assert_eq!(rows[1].1, Ok(WitnessValue::Failed("the catalog has no atomic weight for Ca".into())));
    assert_eq!(rows[2].1, Err("limit: more than 10 iterations".into()));
    assert!(matches!(&rows[3].1, Ok(WitnessValue::Failed(m)) if m.contains("zz_no_such_unit")), "{rows:?}");
    assert_eq!(rows[4].1, Ok(WitnessValue::number(1.0, 0.0, "m")), "the run went on after the panic");
}

/// The process as the test found it, restored on drop; a fresh temporary directory as the current directory.
struct Env {
    dir: PathBuf,
    cwd: PathBuf,
    vars: Vec<(&'static str, Option<OsString>)>,
}

impl Env {
    fn new() -> Env {
        let vars = [ENV_CONF, ENV_REQUIRE].into_iter().map(|name| (name, std::env::var_os(name))).collect();
        for name in [ENV_CONF, ENV_REQUIRE] {
            std::env::remove_var(name);
        }
        let dir = std::env::temp_dir().join(format!("physure_battery_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        set_explicit_conf(None);
        set_engine_file(None);
        reset_cache();
        Env { dir, cwd, vars }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.cwd);
        for (name, value) in &self.vars {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        set_explicit_conf(None);
        set_engine_file(None);
        reset_cache();
        set_compiled_defaults();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn changing_an_override_changes_the_dependent_witness() {
    let _lock = lock();
    let unfrozen = run_battery();
    {
        let env = Env::new();
        fs::write(env.dir.join("physure.conf"), "[Constants]\nmolar_gas_constant = 8.0 J*mol^-1*K^-1\n").unwrap();
        let options = FreezeOptions {
            cwd: &env.dir,
            out: None,
            reason: "battery test",
            policy: EnginePolicy::Release,
            engine_file: None,
            time_unix: 1_794_132_000,
            actor: "t@h",
        };
        let (file, out) = freeze(&options).unwrap();
        write_atomic(&out, &file.render().unwrap(), false).unwrap();
        // the working directory's conf is ignored while the frozen file is in force: remove it so the witness can
        // only have come from the sealed overrides
        fs::remove_file(env.dir.join("physure.conf")).unwrap();
        reset_cache();
        ensure_valid().unwrap();
        assert!(active().unwrap().is_some(), "the frozen file is in force");
        let frozen = run_battery();
        assert_eq!(mean_of(&value_of(&frozen, "const.molar_gas_constant")).0, 8.0);
        assert_ne!(value_of(&frozen, "const.molar_gas_constant"), value_of(&unfrozen, "const.molar_gas_constant"));
        let changed: Vec<&String> =
            frozen.iter().zip(&unfrozen).filter(|(a, b)| a != b).map(|((key, _), _)| key).collect();
        assert_eq!(changed, ["builtin.const.molar_gas_constant"], "only the witness that depends on R changed");
    }
    assert_eq!(run_battery(), unfrozen, "the process is restored");
}

#[test]
fn a_configuration_that_cannot_be_used_runs_nothing() {
    let _lock = lock();
    let _env = Env::new();
    // required to be frozen, and no frozen file: every case is an error row naming why, none is evaluated
    std::env::set_var(ENV_REQUIRE, "1");
    reset_cache();
    let rows = run_battery();
    assert_eq!(rows.len(), battery().len());
    for (key, value) in &rows {
        assert!(matches!(value, Err(m) if m.starts_with("not run: the configuration in force cannot be used")), "{key}: {value:?}");
    }
}
