//! Witness values, their `[Witness]` format, the comparison of recorded and running witnesses, and the structural
//! rule `verify` applies to `[Witness]` (conf freeze Phase 3, Task A1).

use physure_core::frozen::witness::{
    check_entry, compare, format_value, parse_value, ulp_distance, WitnessReport, WitnessStatus, WitnessValue,
};
use physure_core::frozen::{
    engine_pairs, EngineIdentity, EnginePolicy, EntryInput, FrozenFile, FrozenParts, Pairs, Problem, Source, WitnessScript,
};

const HASH: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn number(mean: f64, u: f64, unit: &str) -> WitnessValue {
    WitnessValue::Number { mean: mean.to_bits(), uncertainty: u.to_bits(), unit: unit.into() }
}

#[test]
fn format_then_parse_roundtrips_for_every_variant() {
    let values = [
        number(0.3048, 0.0, "m"),
        number(8.31446261815324, 1e-9, "J/(mol*K)"),
        number(100.0, 0.0, ""),
        WitnessValue::Text("C3H8 + 5 O2 -> 3 CO2 + 4 H2O".into()),
        WitnessValue::Failed("the catalog has no atomic weight for Ca".into()),
        WitnessValue::Hash(HASH["sha256:".len()..].into()),
    ];
    for value in values {
        let text = format_value(&value);
        assert_eq!(parse_value(&text), Ok(value.clone()), "{text}");
    }
    assert_eq!(format_value(&number(1.0, 0.0, "m")), "3ff0000000000000:0000000000000000:m");
    assert_eq!(format_value(&number(1.0, 0.0, "")), "3ff0000000000000:0000000000000000:");
    assert_eq!(format_value(&WitnessValue::Text("a b".into())), "text=a b");
    assert_eq!(format_value(&WitnessValue::Failed("no".into())), "failed=no");
    assert_eq!(format_value(&WitnessValue::Hash("ab".repeat(32))), format!("sha256:{}", "ab".repeat(32)));
}

#[test]
fn parse_refuses_uppercase_hex_short_hex_and_missing_parts() {
    for bad in [
        "",
        "3FF0000000000000:0000000000000000:m",
        "3ff0000000000000:000000000000000A:m",
        "3ff000000000000:0000000000000000:m",
        "3ff00000000000000:0000000000000000:m",
        "3ff0000000000000:0000000000000000",
        "3ff0000000000000",
        "3ff0000000000000::m",
        "+ff0000000000000:0000000000000000:m",
        "3ff0000000000000:0000000000000000: m",
        "3ff0000000000000:0000000000000000:m ",
        "3ff0000000000000:0000000000000000:m\nx",
        "text=",
        "text= padded",
        "failed=",
        "failed=two\nlines",
        "sha256:abc",
        "sha256:0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef",
        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0",
        "SHA256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "1",
        "abc",
    ] {
        assert!(parse_value(bad).is_err(), "{bad:?} was accepted");
    }
}

#[test]
fn a_unit_with_a_colon_survives() {
    let value = number(2.0, 0.0, "m:s");
    let text = format_value(&value);
    assert_eq!(text, "4000000000000000:0000000000000000:m:s");
    assert_eq!(parse_value(&text), Ok(value));
    // a text may hold colons and equals signs too: the prefix is of the whole value
    let text = WitnessValue::Text("a:b=c".into());
    assert_eq!(parse_value(&format_value(&text)), Ok(text));
}

#[test]
fn negative_zero_and_nan_bit_patterns_are_preserved() {
    let negative_zero = number(-0.0, 0.0, "m");
    let zero = number(0.0, 0.0, "m");
    assert_ne!(format_value(&negative_zero), format_value(&zero));
    assert_eq!(parse_value(&format_value(&negative_zero)), Ok(negative_zero.clone()));
    let payload = f64::from_bits(0x7ff8_0000_0000_0abc);
    assert!(payload.is_nan());
    let nan = number(payload, f64::NAN, "K");
    let back = parse_value(&format_value(&nan)).unwrap();
    assert_eq!(back, nan);
    let WitnessValue::Number { mean, .. } = back else { panic!() };
    assert_eq!(mean, 0x7ff8_0000_0000_0abc);
    // and the comparison sees the difference
    let recorded = Pairs::new(&[("builtin.z", &format_value(&zero))]).unwrap();
    let report = compare(&recorded, &[("builtin.z".into(), Ok(negative_zero))]);
    assert!(matches!(report.rows[0].status, WitnessStatus::Differs { ulps: Some(0), .. }), "{report:?}");
}

#[test]
fn ulp_distance_of_adjacent_floats_is_1() {
    let a = 1.0f64;
    let b = f64::from_bits(a.to_bits() + 1);
    assert_eq!(ulp_distance(a, b), Some(1));
    assert_eq!(ulp_distance(b, a), Some(1));
    assert_eq!(ulp_distance(a, a), Some(0));
    assert_eq!(ulp_distance(-a, -b), Some(1));
    assert_eq!(ulp_distance(0.1 + 0.2, 0.3), Some(1));
    assert_eq!(ulp_distance(1.0, 2.0), Some(1 << 52));
}

#[test]
fn across_zero() {
    let tiny = f64::from_bits(1);
    assert_eq!(ulp_distance(0.0, tiny), Some(1));
    assert_eq!(ulp_distance(-0.0, tiny), Some(1));
    assert_eq!(ulp_distance(-0.0, 0.0), Some(0));
    assert_eq!(ulp_distance(-tiny, 0.0), Some(1));
    // opposite signs, neither zero: not a meaningful distance
    assert_eq!(ulp_distance(-tiny, tiny), None);
    assert_eq!(ulp_distance(-1.0, 1.0), None);
}

#[test]
fn non_finite_is_none() {
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(ulp_distance(x, 1.0), None);
        assert_eq!(ulp_distance(1.0, x), None);
        assert_eq!(ulp_distance(x, x), None);
    }
    assert_eq!(ulp_distance(f64::MAX, f64::MAX), Some(0));
}

fn report() -> WitnessReport {
    let recorded = Pairs::new(&[
        ("builtin.same", "3ff0000000000000:0000000000000000:m"),
        ("builtin.changed", "3ff0000000000000:0000000000000000:m"),
        ("builtin.gone", "3ff0000000000000:0000000000000000:m"),
        ("builtin.broken", "3ff0000000000000:0000000000000000:m"),
        ("builtin.unreadable", "1"),
        ("other.key", "3ff0000000000000:0000000000000000:m"),
        ("user.v", HASH),
    ])
    .unwrap();
    let running: Vec<(String, Result<WitnessValue, String>)> = vec![
        ("builtin.same".into(), Ok(number(1.0, 0.0, "m"))),
        ("builtin.fresh".into(), Ok(number(2.0, 0.0, "m"))),
        ("builtin.changed".into(), Ok(number(f64::from_bits(1.0f64.to_bits() + 3), 0.0, "m"))),
        ("builtin.broken".into(), Err("panic: boom".into())),
        ("builtin.unreadable".into(), Ok(number(1.0, 0.0, "m"))),
        ("other.key".into(), Ok(number(1.0, 0.0, "m"))),
        ("user.v".into(), Ok(parse_value(HASH).unwrap())),
    ];
    compare(&recorded, &running)
}

#[test]
fn compare_reports_match_differs_missing_new_and_error() {
    let report = report();
    let keys: Vec<&str> = report.rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(
        keys,
        ["builtin.same", "builtin.changed", "builtin.gone", "builtin.broken", "builtin.unreadable", "other.key", "user.v", "builtin.fresh"],
        "recorded order, then the new ones"
    );
    let status = |key: &str| &report.rows.iter().find(|r| r.key == key).unwrap().status;
    assert_eq!(status("builtin.same"), &WitnessStatus::Match);
    assert_eq!(status("user.v"), &WitnessStatus::Match);
    assert!(matches!(status("builtin.changed"), WitnessStatus::Differs { .. }));
    assert_eq!(status("builtin.gone"), &WitnessStatus::Missing);
    assert_eq!(status("builtin.fresh"), &WitnessStatus::New);
    assert_eq!(status("builtin.broken"), &WitnessStatus::Error("panic: boom".into()));
    assert!(matches!(status("builtin.unreadable"), WitnessStatus::Error(m) if m.starts_with("recorded value does not read")), "{report:?}");
    assert!(matches!(status("other.key"), WitnessStatus::Error(m) if m.contains("builtin.<id>")), "{report:?}");
    assert_eq!(report.counts(), (1, 1, 1, 3));
    assert!(!report.all_match());
    // Match and New only: all match
    let recorded = Pairs::new(&[("builtin.same", "3ff0000000000000:0000000000000000:m")]).unwrap();
    let ok = compare(&recorded, &[("builtin.same".into(), Ok(number(1.0, 0.0, "m"))), ("builtin.new".into(), Ok(number(1.0, 0.0, "")))]);
    assert!(ok.all_match(), "{ok:?}");
    assert_eq!(ok.counts(), (0, 1, 0, 0));
    // a new case that errors is an error, not something new to record
    let failing = compare(&recorded, &[("builtin.same".into(), Ok(number(1.0, 0.0, "m"))), ("builtin.new".into(), Err("panic".into()))]);
    assert_eq!(failing.counts(), (0, 0, 0, 1));
    assert!(!failing.all_match());
    // a recorded failure compares like any value
    let failed = WitnessValue::Failed("no Ca".into());
    let recorded = Pairs::new(&[("builtin.f", &format_value(&failed))]).unwrap();
    assert!(compare(&recorded, &[("builtin.f".into(), Ok(failed))]).all_match());
}

#[test]
fn differs_carries_both_bit_patterns_and_ulps() {
    let report = report();
    let row = report.rows.iter().find(|r| r.key == "builtin.changed").unwrap();
    let WitnessStatus::Differs { recorded, running, ulps } = &row.status else { panic!("{row:?}") };
    assert_eq!(recorded, "3ff0000000000000:0000000000000000:m");
    assert_eq!(running, "3ff0000000000003:0000000000000000:m");
    assert_eq!(*ulps, Some(3));
    // the uncertainty counts too, and the larger distance is reported
    let recorded = Pairs::new(&[("builtin.u", &format_value(&number(1.0, 1.0, "m")))]).unwrap();
    let r = compare(&recorded, &[("builtin.u".into(), Ok(number(1.0, f64::from_bits(1.0f64.to_bits() + 5), "m")))]);
    assert!(matches!(r.rows[0].status, WitnessStatus::Differs { ulps: Some(5), .. }), "{r:?}");
    // another unit or another kind of value has no ulp distance
    for running in [number(1.0, 0.0, "ft"), WitnessValue::Text("1 m".into()), WitnessValue::Failed("x".into())] {
        let recorded = Pairs::new(&[("builtin.k", "3ff0000000000000:0000000000000000:m")]).unwrap();
        let r = compare(&recorded, &[("builtin.k".into(), Ok(running))]);
        assert!(matches!(r.rows[0].status, WitnessStatus::Differs { ulps: None, .. }), "{r:?}");
    }
}

// ---- S8: verify reports a [Witness] entry that is not a witness ----

#[test]
fn check_entry_names_the_rule() {
    assert!(check_entry("builtin.units.foot_to_metre", "3fd381d7dbf487fd:0000000000000000:m").is_ok());
    assert!(check_entry("builtin.chem.balance-x_2", "text=C + O2 -> CO2").is_ok());
    assert!(check_entry("builtin.chem.molar_mass_ca", "failed=no Ca").is_ok());
    assert!(check_entry("user.validation.1", HASH).is_ok());
    for (key, value, why) in [
        ("other.x", "3ff0000000000000:0000000000000000:m", "builtin.<id>"),
        ("builtin.", "3ff0000000000000:0000000000000000:m", "builtin.<id>"),
        ("builtin.Upper", "3ff0000000000000:0000000000000000:m", "builtin.<id>"),
        ("builtin.a/b", "3ff0000000000000:0000000000000000:m", "builtin.<id>"),
        ("user.", HASH, "user.<name>"),
        ("builtin.x", "1", "does not read"),
        ("builtin.x", HASH, "sha256"),
        ("user.v", "3ff0000000000000:0000000000000000:m", "sha256"),
        ("user.v", "text=hi", "sha256"),
    ] {
        let message = check_entry(key, value).unwrap_err();
        assert!(message.contains(why), "{key} = {value}: {message}");
    }
}

fn engine() -> Pairs {
    engine_pairs(&EngineIdentity::current(), EnginePolicy::Record, None).unwrap()
}

fn entry() -> EntryInput<'static> {
    EntryInput { time: "2026-11-13T10:00:00Z", action: "freeze", engine: "0.0.1", actor: "t@h", reason: "r", diff: "d" }
}

fn file_with(witnesses: &[(&str, &str)], scripts: &[&str]) -> Result<FrozenFile, String> {
    FrozenFile::new(
        "2026-11-13T10:00:00Z",
        FrozenParts {
            engine: engine(),
            witnesses: Pairs::new(witnesses).unwrap(),
            sources: vec![Source::new("embedded", "[Units]\nm = 1").unwrap(), Source::new("overrides", "").unwrap()],
            witness_scripts: scripts.iter().map(|n| WitnessScript::new(n, "1 m").unwrap()).collect(),
            ..FrozenParts::default()
        },
        entry(),
    )
    .map_err(|e| e.to_string())
}

#[test]
fn verify_reports_a_witness_entry_that_is_not_a_witness() {
    let good = file_with(&[("builtin.units.x", "3ff0000000000000:0000000000000000:m"), ("user.v", HASH)], &["v"]).unwrap();
    assert!(good.verify().is_empty(), "{:?}", good.verify());
    for (witnesses, scripts, what) in [
        (vec![("other.x", "3ff0000000000000:0000000000000000:m")], vec![], "other.x"),
        (vec![("builtin.x", "1")], vec![], "builtin.x"),
        (vec![("builtin.x", HASH)], vec![], "builtin.x"),
        (vec![("user.v", "abc")], vec!["v"], "user.v"),
    ] {
        let message = file_with(&witnesses, &scripts).unwrap_err();
        assert!(message.contains("[Witness] entry") && message.contains(what), "{witnesses:?}: {message}");
    }
    // reached through text: verify reports it as its own problem, and the user.* rule is still WitnessKeys
    let text = good.render().unwrap();
    let edited = FrozenFile::parse(&text.replace("builtin.units.x = ", "builtin.Units.x = ")).unwrap();
    let problems = edited.verify();
    assert!(problems.iter().any(|p| matches!(p, Problem::WitnessEntry { key, .. } if key == "builtin.Units.x")), "{problems:?}");
    let edited = FrozenFile::parse(&text.replace(HASH, "sha256:00")).unwrap();
    let problems = edited.verify();
    assert!(problems.iter().any(|p| matches!(p, Problem::WitnessEntry { key, .. } if key == "user.v")), "{problems:?}");
    assert!(!problems.iter().any(|p| matches!(p, Problem::WitnessKeys { .. })), "{problems:?}");
    let edited = FrozenFile::parse(&text.replace(&format!("user.v = {HASH}\n"), "")).unwrap();
    assert!(edited.verify().iter().any(|p| matches!(p, Problem::WitnessKeys { .. })));
    let text = Problem::WitnessEntry { key: "k".into(), found: "f".into() }.to_string();
    assert!(text.contains("expected") && text.contains("found"), "{text}");
}
