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
use physure_core::uncertainty::{propagation_mode, set_propagation_mode, PropagationMode};
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

/// The ids released so far, which no later release may change or remove.
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
    // task A3
    "stat.t_p95_nu1",
    "stat.t_p95_nu10",
    "stat.t_p95_nu100",
    "stat.t_p95_nu2",
    "stat.t_p95_nu30",
    "stat.t_p95_nu5",
    "stat.t_p99_nu10",
    "stat.t_p99_nu100",
    "unc.expanded_k_p95",
    "unc.gum_h1_nu_eff",
    "unc.gum_h1_uc",
    "unc.montecarlo_seed_1",
    "unc.power_first_order",
    "unc.product_first_order",
    "unc.quotient_first_order",
    "unc.sum_correlated",
    "unc.sum_uncorrelated",
    "unc.unscented_square",
    "unc.welch_satterthwaite_g41",
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
    // the cases that could vary most: a seeded generator, sigma points, a quantile found by bisection
    for id in ["unc.montecarlo_seed_1", "unc.unscented_square", "stat.t_p95_nu1"] {
        assert!(first.iter().any(|(key, value)| *key == format!("builtin.{id}") && value.is_ok()), "{id} runs");
    }
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
        assert_eq!(
            changed,
            ["builtin.chem.arrhenius", "builtin.chem.clausius_clapeyron_water", "builtin.const.molar_gas_constant"],
            "only the witnesses that depend on R changed"
        );
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

// ---------------------------------------------------------------------------------------------------------------
// Uncertainty and Student-t cases (Phase 3, task A3).
// ---------------------------------------------------------------------------------------------------------------

fn run_one(id: &str) -> WitnessValue {
    let case = battery().into_iter().find(|c| c.id == id).unwrap_or_else(|| panic!("no case {id}"));
    match (case.run)() {
        Ok(value) => value,
        Err(e) => panic!("{id}: {e:?}"),
    }
}

fn within_ulps(id: &str, what: &str, found: f64, reference: f64, ulps: u64, failures: &mut Vec<String>) {
    let distance = ulp_distance(found, reference);
    if distance.is_none_or(|d| d > ulps) {
        failures.push(format!("{id} {what}: {found:e} vs {reference:e}: {distance:?} ulps > {ulps}"));
    }
}

/// The first-order (GUM 5.1.2 / 5.2.2) cases against closed forms derived by hand for each model:
///
/// * `x = 1.5 ± 0.02 m`, `y = 2.5 x`, `s = x + y`. Fully correlated (one source): `u(s) = u(x) + 2.5 u(x)`;
///   treated as independent: `u(s) = sqrt(u(x)² + (2.5 u(x))²)` (GUM 5.1.2 with r = 0);
/// * `a = 2.0 ± 0.1 m`, `b = 3.0 ± 0.2 s` independent: `u(ab) = sqrt((b u_a)² + (a u_b)²)`,
///   `u(a/b) = sqrt((u_a / b)² + (a u_b / b²)²)`, `u(a³) = 3 a² u_a`.
///
/// Tolerance: 2 ulps (the engine and the formula do the same few operations, possibly in another order).
#[test]
fn every_first_order_case_matches_its_closed_form() {
    let _lock = lock();
    let (ux, k) = (0.02_f64, 2.5_f64);
    let (a, ua, b, ub) = (2.0_f64, 0.1_f64, 3.0_f64, 0.2_f64);
    let cases: [(&str, f64, f64, &str); 5] = [
        ("unc.sum_correlated", 1.5 + k * 1.5, ux + k * ux, "m"),
        ("unc.sum_uncorrelated", 1.5 + k * 1.5, (ux * ux + (k * ux) * (k * ux)).sqrt(), "m"),
        ("unc.product_first_order", a * b, ((b * ua).powi(2) + (a * ub).powi(2)).sqrt(), "m * s"),
        ("unc.quotient_first_order", a / b, ((ua / b).powi(2) + (a * ub / (b * b)).powi(2)).sqrt(), "m * s^-1"),
        ("unc.power_first_order", a * a * a, 3.0 * a * a * ua, "m^3"),
    ];
    let mut failures = Vec::new();
    for (id, mean, u, unit) in cases {
        let (found_mean, found_u, found_unit) = mean_of(&run_one(id));
        within_ulps(id, "mean", found_mean, mean, 2, &mut failures);
        within_ulps(id, "u", found_u, u, 2, &mut failures);
        if found_unit != unit {
            failures.push(format!("{id}: unit {found_unit:?}, expected {unit:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Two-sided Student-t coverage factors `t_{(1+p)/2}(nu)` from a published table (NIST/SEMATECH e-Handbook of
/// Statistical Methods, section 1.3.6.7.2, "Critical values of the Student's t distribution"; JCGM 100:2008 Table
/// G.2 prints the same values to fewer digits): t0.975 and t0.995. The table prints 4 decimals, so the tolerance
/// is 5e-5 relative.
#[test]
fn every_student_t_case_matches_the_published_table() {
    let _lock = lock();
    let table: [(&str, f64); 8] = [
        ("stat.t_p95_nu1", 12.7062),
        ("stat.t_p95_nu2", 4.3027),
        ("stat.t_p95_nu5", 2.5706),
        ("stat.t_p95_nu10", 2.2281),
        ("stat.t_p95_nu30", 2.0423),
        ("stat.t_p95_nu100", 1.9840),
        ("stat.t_p99_nu10", 3.1693),
        ("stat.t_p99_nu100", 2.6259),
    ];
    let mut failures = Vec::new();
    for (id, reference) in table {
        let (t, u, unit) = mean_of(&run_one(id));
        if ((t - reference) / reference).abs() > 5e-5 || u != 0.0 || !unit.is_empty() {
            failures.push(format!("{id}: {t} ± {u} {unit:?} vs {reference}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// JCGM 100:2008 Annex H.1 (the end gauge), as the six-contribution linear budget of
/// `tests/expanded_uncertainty.rs`: the Guide prints u_c = 32 nm, nu_eff = 16.7 (H.1.6) and k95 = 2.12; the values
/// computed independently (scipy, recorded in that test) are u_c = 31.66386517 nm, nu_eff = 16.7518953 and
/// k95 = 2.1199053. G.4.1, example 1: relative uncertainties 0.25 %, 0.57 %, 0.82 % with 9, 4 and 14 degrees of
/// freedom give nu_eff = 19.0 (the Guide rounds to one decimal).
#[test]
fn the_gum_cases_reproduce_the_guide() {
    let _lock = lock();
    let (length, uc, unit) = mean_of(&run_one("unc.gum_h1_uc"));
    assert_eq!(length, 50_000_838.0);
    assert!(((uc - 31.663_865_17) / 31.663_865_17).abs() < 1e-9, "{uc}");
    assert_eq!(format!("{uc:.0}"), "32", "the 32 nm of the Guide");
    assert_eq!(unit, "nm");

    let (nu_eff, u, unit) = mean_of(&run_one("unc.gum_h1_nu_eff"));
    assert!(((nu_eff - 16.751_895_3) / 16.751_895_3).abs() < 1e-8, "{nu_eff}");
    assert_eq!(format!("{:.1}", (nu_eff * 10.0).floor() / 10.0), "16.7", "the 16.7 of the Guide");
    assert_eq!((u, unit.as_str()), (0.0, ""));

    let (k95, u, unit) = mean_of(&run_one("unc.expanded_k_p95"));
    assert!(((k95 - 2.119_905_3) / 2.119_905_3).abs() < 1e-7, "{k95}");
    assert_eq!(format!("{k95:.2}"), "2.12");
    assert_eq!((u, unit.as_str()), (0.0, ""));

    let (g41, u, unit) = mean_of(&run_one("unc.welch_satterthwaite_g41"));
    assert_eq!(format!("{g41:.1}"), "19.0", "G.4.1: {g41}");
    assert!((g41 - 19.0).abs() < 0.05, "{g41}");
    assert_eq!((u, unit.as_str()), (0.0, ""));
}

/// `unc.montecarlo_seed_1`: `a b` (the inputs of `unc.product_first_order`) with 1000 samples drawn under seed 1.
/// It really sampled: the result is not the first-order value; it agrees with the exact moments of the product of
/// two independent normals (mean `a b`, variance `b² u_a² + a² u_b² + u_a² u_b²`) within five standard errors of a
/// 1000-sample estimate (the standard error of a mean is `u / sqrt(M)`, that of a standard deviation about
/// `u / sqrt(2 M)`, JCGM 101:2008 7.2); and the same seed gives the same bits.
#[test]
fn the_monte_carlo_case_really_samples_and_replays_its_seed() {
    let _lock = lock();
    let first = run_one("unc.montecarlo_seed_1");
    assert_eq!(first, run_one("unc.montecarlo_seed_1"), "the same seed gives the same bits");
    assert_ne!(first, run_one("unc.product_first_order"), "a sampled result is not the analytic one");
    let (mean, u, unit) = mean_of(&first);
    let exact_u = (0.25_f64 + 0.0004).sqrt();
    let m = 1000_f64;
    assert!((mean - 6.0).abs() < 5.0 * exact_u / m.sqrt(), "{mean}");
    assert!((u - exact_u).abs() < 5.0 * exact_u / (2.0 * m).sqrt(), "{u}");
    assert_ne!(u, 0.5);
    assert_eq!(unit, "m * s");
}

/// `unc.unscented_square`: `a²` for `a = 2.0 ± 0.1 m` through the sigma points of the unscented transform (Julier
/// and Uhlmann 2004; one dimension, kappa = 2, so the three points match the first four moments of a normal). For
/// a normal input the transform of `x²` is exact: mean `a² + u_a²` and variance `4 a² u_a² + 2 u_a⁴` (the moments
/// of a non-central chi-square), where first order says `a²` and `2 a u_a`.
#[test]
fn the_unscented_case_goes_through_the_sigma_points() {
    let _lock = lock();
    let (a, ua) = (2.0_f64, 0.1_f64);
    let (mean, u, unit) = mean_of(&run_one("unc.unscented_square"));
    let exact_mean = a * a + ua * ua;
    let exact_u = (4.0 * a * a * ua * ua + 2.0 * ua.powi(4)).sqrt();
    assert!(((mean - exact_mean) / exact_mean).abs() < 1e-12, "{mean} vs {exact_mean}");
    assert!(((u - exact_u) / exact_u).abs() < 1e-12, "{u} vs {exact_u}");
    assert!((mean - a * a).abs() > 1e-3 && (u - 2.0 * a * ua).abs() > 1e-5, "not the first-order result");
    assert_eq!(unit, "m^2");
}

/// Every `unc.*` case pins its mode on its own thread: with the process-wide mode set to each of the four, the
/// battery computes the same bits. The modes matter (so this is not vacuous): the same arithmetic without a pin
/// gives another result under every mode but the correlated one; the correlated and the uncorrelated sums share
/// their inputs and differ; and the degrees of freedom of `unc.gum_*` exist for the Gaussian model only, so under a
/// process-wide Monte Carlo or unscented mode they would fail without the pin.
#[test]
fn every_uncertainty_case_pins_its_mode_whatever_the_process_says() {
    let _lock = lock();
    let baseline = run_battery();
    assert_ne!(value_of(&baseline, "unc.sum_correlated"), value_of(&baseline, "unc.sum_uncorrelated"));
    let previous = propagation_mode();
    for mode in [PropagationMode::Uncorrelated, PropagationMode::MonteCarlo, PropagationMode::Unscented, PropagationMode::Correlated] {
        set_propagation_mode(mode);
        let rows = run_battery();
        assert_eq!(propagation_mode(), mode, "the process-wide mode stayed {mode:?} during the run (not vacuous)");
        assert_eq!(rows, baseline, "under a process-wide {mode:?}");
        // the arithmetic of three Gaussian cases, without a pin: the mode changes it
        let unpinned = unpinned_gaussian_arithmetic();
        let pinned: Vec<WitnessValue> = ["unc.sum_correlated", "unc.product_first_order", "unc.power_first_order"]
            .iter()
            .map(|id| value_of(&baseline, id))
            .collect();
        let differs = |i: usize| unpinned[i] != pinned[i];
        match mode {
            PropagationMode::Correlated => assert_eq!(unpinned, pinned),
            PropagationMode::Uncorrelated => assert!(differs(0), "an unpinned correlated sum changes"),
            PropagationMode::MonteCarlo => assert!(differs(1), "an unpinned product is sampled"),
            PropagationMode::Unscented => assert!(differs(2), "an unpinned cube goes through sigma points"),
        }
    }
    set_propagation_mode(previous);
}

/// `unc.sum_correlated`, `unc.product_first_order` and `unc.power_first_order` computed here under whatever mode
/// the process is in.
fn unpinned_gaussian_arithmetic() -> Vec<WitnessValue> {
    use physure_core::Quantity;
    let show = |q: Quantity| {
        let (mean, u, unit) = q.display_parts();
        WitnessValue::number(mean, u, &unit)
    };
    let x = Quantity::with_uncertainty(1.5, 0.02, "m").unwrap();
    let sum = x.add(&x.mul(&Quantity::new(2.5, "").unwrap()).unwrap()).unwrap();
    let a = Quantity::with_uncertainty(2.0, 0.1, "m").unwrap();
    let b = Quantity::with_uncertainty(3.0, 0.2, "s").unwrap();
    vec![show(sum), show(a.mul(&b).unwrap()), show(a.pow(3.0).unwrap())]
}

// ---------------------------------------------------------------------------------------------------------------
// Chemistry cases (Phase 3, task A4).
// ---------------------------------------------------------------------------------------------------------------

const CIAAW_STANDARD_2024: &str = include_str!("../../tests/golden/ciaaw_standard_atomic_weights_2024.tsv");

/// `(weight, standard uncertainty)` of `symbol` from the CIAAW page as transcribed (the default edition,
/// `standard-2024`), with the conversion of `docs/atomic_weights.md` section 3: an interval `[a, b]` is its
/// midpoint with `(b - a) / sqrt(12)`; `x(n)` is `x` with the half-width `n` in its last digits, `U / sqrt(3)`.
fn ciaaw(symbol: &str) -> (f64, f64) {
    let line = CIAAW_STANDARD_2024
        .lines()
        .filter(|l| !l.starts_with('#'))
        .find(|l| l.split('\t').nth(1) == Some(symbol))
        .unwrap_or_else(|| panic!("no {symbol} in the CIAAW table"));
    let printed: String = line.split('\t').nth(2).unwrap().chars().filter(|c| *c != ' ').collect();
    if let Some(interval) = printed.strip_prefix('[').and_then(|p| p.strip_suffix(']')) {
        let (a, b) = interval.split_once(',').unwrap();
        let (a, b): (f64, f64) = (a.parse().unwrap(), b.parse().unwrap());
        ((a + b) / 2.0, (b - a) / 12f64.sqrt())
    } else {
        let (value, rest) = printed.split_once('(').unwrap();
        let digits = value.split_once('.').map_or(0, |(_, d)| d.len());
        let half_width: f64 = format!("{}e-{digits}", rest.trim_end_matches(')')).parse().unwrap();
        (value.parse().unwrap(), half_width / 3f64.sqrt())
    }
}

/// `(M, u(M))` of a composition by hand: `sum n_i A_i` and `sqrt(sum (n_i u_i)²)`.
fn by_hand(composition: &[(&str, f64)]) -> (f64, f64) {
    let mass = composition.iter().map(|(e, n)| n * ciaaw(e).0).sum();
    let variance: f64 = composition.iter().map(|(e, n)| (n * ciaaw(e).1).powi(2)).sum();
    (mass, variance.sqrt())
}

const MOLAR_MASSES: &[(&str, &[(&str, f64)])] = &[
    ("chem.molar_mass_h2o", &[("H", 2.0), ("O", 1.0)]),
    ("chem.molar_mass_glucose", &[("C", 6.0), ("H", 12.0), ("O", 6.0)]),
    ("chem.molar_mass_calcium_hydroxide", &[("Ca", 1.0), ("O", 2.0), ("H", 2.0)]),
    ("chem.molar_mass_iron_sulfate", &[("Fe", 1.0), ("S", 1.0), ("O", 4.0)]),
];

#[test]
fn every_molar_mass_case_is_the_sum_of_the_standard_weights() {
    let _lock = lock();
    let mut failures = Vec::new();
    for (id, composition) in MOLAR_MASSES {
        let (mass, u, unit) = mean_of(&run_one(id));
        let (reference, reference_u) = by_hand(composition);
        within_ulps(id, "M", mass, reference, 4, &mut failures);
        within_ulps(id, "u(M)", u, reference_u, 4, &mut failures);
        if unit != "g/mol" {
            failures.push(format!("{id}: unit {unit:?}"));
        }
    }
    // the values chemists know (CIAAW 2024, conventional): water 18.015, glucose 180.16, Ca(OH)2 74.09, FeSO4 151.90
    for (id, known) in [
        ("chem.molar_mass_h2o", 18.015),
        ("chem.molar_mass_glucose", 180.156),
        ("chem.molar_mass_calcium_hydroxide", 74.092),
        ("chem.molar_mass_iron_sulfate", 151.90),
    ] {
        let mass = mean_of(&run_one(id)).0;
        if ((mass - known) / known).abs() > 1e-4 {
            failures.push(format!("{id}: {mass} is not about {known}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// `chem.mass_to_moles_nacl`: 100 g of NaCl is `100 / M` mol with `u = 100 u(M) / M²`.
#[test]
fn the_amount_of_nacl_is_the_mass_over_the_molar_mass() {
    let _lock = lock();
    let (moles, u, unit) = mean_of(&run_one("chem.mass_to_moles_nacl"));
    let (m, um) = by_hand(&[("Na", 1.0), ("Cl", 1.0)]);
    let mut failures = Vec::new();
    within_ulps("chem.mass_to_moles_nacl", "n", moles, 100.0 / m, 4, &mut failures);
    within_ulps("chem.mass_to_moles_nacl", "u(n)", u, 100.0 * um / (m * m), 4, &mut failures);
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(unit, "mol");
    assert!((moles - 1.7112).abs() < 1e-3, "100 g of NaCl is about 1.711 mol: {moles}");
}

/// Balancing against the textbook coefficients: C3H8 + 5 O2 -> 3 CO2 + 4 H2O (combustion of propane) and
/// Fe2O3 + 3 CO -> 2 Fe + 3 CO2 (the blast furnace). The canonical text writes every coefficient, 1 included.
#[test]
fn the_balancing_cases_give_the_textbook_coefficients() {
    let _lock = lock();
    assert_eq!(run_one("chem.balance_propane_combustion"), WitnessValue::Text("1 C3H8 + 5 O2 -> 3 CO2 + 4 H2O".into()));
    assert_eq!(run_one("chem.balance_iron_oxide_reduction"), WitnessValue::Text("1 Fe2O3 + 3 CO -> 2 Fe + 3 CO2".into()));
}

fn relative(id: &str, found: f64, reference: f64, tolerance: f64) {
    assert!(((found - reference) / reference).abs() <= tolerance, "{id}: {found:e} vs {reference:e}");
}

/// Arrhenius (k = A exp(-Ea / RT)), Clausius-Clapeyron (P2 = P1 exp(-(dH/R)(1/T2 - 1/T1))) and Gibbs
/// (dG = dH - T dS) evaluated here with the catalog's R (CODATA 2022, exact). The exponentials amplify a relative
/// rounding by the size of their argument (about 30 and 0.9), hence 1e-13; Gibbs is one subtraction, 1e-14.
#[test]
fn the_thermodynamic_cases_match_their_closed_forms() {
    let _lock = lock();
    let r: f64 = 8.314_462_618_153_24;
    let (k, u, unit) = mean_of(&run_one("chem.arrhenius"));
    relative("chem.arrhenius", k, 1e13 * (-75_000.0 / (r * 298.15)).exp(), 1e-13);
    assert_eq!((u, unit.as_str()), (0.0, "s^-1"));
    assert!((k - 0.73).abs() < 0.005, "{k}");

    let (p2, u, unit) = mean_of(&run_one("chem.clausius_clapeyron_water"));
    relative("chem.clausius_clapeyron_water", p2, 101.325 * (-(40_700.0 / r) * (1.0 / 363.15 - 1.0 / 373.15)).exp(), 1e-13);
    assert_eq!((u, unit.as_str()), (0.0, "kPa"));
    assert!(p2 > 69.5 && p2 < 71.5, "about 70 kPa at 90 degC: {p2}");

    let (g, u, unit) = mean_of(&run_one("chem.gibbs_298"));
    relative("chem.gibbs_298", g, -285.8 + 298.15 * 163.2 / 1000.0, 1e-14);
    assert_eq!((u, unit.as_str()), (0.0, "kJ/mol"));
    assert!((g + 237.1).abs() < 0.05, "the Gibbs energy of formation of liquid water, -237.1 kJ/mol: {g}");
}

/// The battery under a frozen file made from `conf` (the working directory's `physure.conf` at freeze time), with
/// the process restored afterwards.
fn battery_under_frozen(conf: &str) -> Vec<(String, Result<WitnessValue, String>)> {
    let env = Env::new();
    fs::write(env.dir.join("physure.conf"), conf).unwrap();
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
    fs::remove_file(env.dir.join("physure.conf")).unwrap();
    reset_cache();
    ensure_valid().unwrap();
    assert!(active().unwrap().is_some(), "the frozen file is in force");
    run_battery()
}

fn changed_keys(a: &[(String, Result<WitnessValue, String>)], b: &[(String, Result<WitnessValue, String>)]) -> Vec<String> {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).filter(|(x, y)| x != y).map(|((key, _), _)| key.clone()).collect()
}

#[test]
fn a_user_atomic_weight_changes_the_molar_masses_that_use_it_and_only_those() {
    let _lock = lock();
    let unfrozen = run_battery();
    // O = 16.5 with the half-width 0.03, stored as 0.03 / sqrt(3) (docs/atomic_weights.md 5.4)
    let frozen = battery_under_frozen("[AtomicWeights]\nO = 16.5, 0.03\n");
    let (h, uh) = ciaaw("H");
    let (water, u, _) = mean_of(&value_of(&frozen, "chem.molar_mass_h2o"));
    let mut failures = Vec::new();
    within_ulps("chem.molar_mass_h2o", "M", water, 2.0 * h + 16.5, 4, &mut failures);
    within_ulps("chem.molar_mass_h2o", "u(M)", u, ((2.0 * uh).powi(2) + (0.03 / 3f64.sqrt()).powi(2)).sqrt(), 4, &mut failures);
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(
        changed_keys(&frozen, &unfrozen),
        [
            "builtin.chem.molar_mass_calcium_hydroxide",
            "builtin.chem.molar_mass_glucose",
            "builtin.chem.molar_mass_h2o",
            "builtin.chem.molar_mass_iron_sulfate",
        ],
        "every case with oxygen changed, NaCl and the rest did not"
    );
    assert_eq!(run_battery(), unfrozen, "the process is restored");
}

#[test]
fn a_user_table_without_calcium_records_a_failure_and_does_not_panic() {
    let _lock = lock();
    let unfrozen = run_battery();
    let frozen = battery_under_frozen("[Chemistry]\natomic_weights = mine\n[AtomicWeights:mine]\nH = 1.008, 0\nO = 15.999, 0\n");
    match value_of(&frozen, "chem.molar_mass_calcium_hydroxide") {
        WitnessValue::Failed(message) => assert!(message.contains("'Ca'") && message.contains("mine"), "{message}"),
        other => panic!("not a recorded failure: {other:?}"),
    }
    assert_eq!(value_of(&frozen, "chem.molar_mass_h2o"), WitnessValue::number(2.0 * 1.008 + 15.999, 0.0, "g/mol"));
    assert!(matches!(value_of(&frozen, "chem.mass_to_moles_nacl"), WitnessValue::Failed(_)));
    // a failure is a value: every row is Ok, nothing is an error row
    assert!(frozen.iter().all(|(_, value)| value.is_ok()), "{frozen:?}");
    assert_eq!(run_battery(), unfrozen, "the process is restored");
}

/// The ids of the chemistry task.
const CHEMISTRY_IDS: &[&str] = &[
    "chem.arrhenius",
    "chem.balance_iron_oxide_reduction",
    "chem.balance_propane_combustion",
    "chem.clausius_clapeyron_water",
    "chem.gibbs_298",
    "chem.mass_to_moles_nacl",
    "chem.molar_mass_calcium_hydroxide",
    "chem.molar_mass_glucose",
    "chem.molar_mass_h2o",
    "chem.molar_mass_iron_sulfate",
];

#[test]
fn the_battery_has_its_size_and_every_released_id() {
    let _lock = lock();
    let ids: BTreeSet<&str> = battery().iter().map(|c| c.id).collect();
    for id in STABLE_IDS.iter().chain(CHEMISTRY_IDS) {
        assert!(ids.contains(id), "{id}");
    }
    assert!(battery().len() >= 38, "{}", battery().len());
    assert_eq!(battery().len(), STABLE_IDS.len() + CHEMISTRY_IDS.len(), "every case is listed here: {ids:#?}");
}
