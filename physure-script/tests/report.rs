//! `report::evaluate` and `report::describe_unit`: the structured answer an agent reads.

use physure_script::report::{describe_unit, evaluate, Report, DEFAULT_SEED};
use physure_script::Limits;
use std::time::Duration;

fn run(source: &str) -> Report {
    evaluate(source, Limits::sandboxed(Duration::from_secs(60)), DEFAULT_SEED)
}

fn codes(report: &Report) -> Vec<(usize, &str)> {
    report.diagnostics.iter().map(|d| (d.line, d.code.as_str())).collect()
}

#[test]
fn a_quantity_carries_its_value_unit_uncertainty_and_dimension() {
    let report = run("x = 9.81 +/- 0.02 m/s^2\ny = x * 2 s\ny");
    assert!(report.ok, "{:?}", report.diagnostics);
    let y = &report.results[1];
    assert_eq!((y.line, y.name.as_deref(), y.value.kind), (2, Some("y"), "quantity"));
    assert!((y.value.value.unwrap() - 19.62).abs() < 1e-9, "{:?}", y.value);
    assert!((y.value.uncertainty.unwrap() - 0.04).abs() < 1e-9, "{:?}", y.value);
    assert_eq!(y.value.dimension.as_deref(), Some("L·T⁻¹"));
    // the unit is one the core reads back, with the dimension the item states
    let unit = y.value.unit.as_deref().unwrap();
    assert_eq!(describe_unit(unit).unwrap().dimension, "L·T⁻¹", "unit {unit:?}");
    // the bare expression on line 3 is a result without a name
    assert_eq!(report.results[2].name, None);
}

#[test]
fn numbers_booleans_strings_and_whole_vectors_have_their_kind() {
    let report = run("2 + 3\n1 m < 2 m\n\"hello\"\nvector(1 m, 2 m, 3 m, 4 m, 5 m, 6 m)");
    assert!(report.ok, "{:?}", report.diagnostics);
    let kinds: Vec<&str> = report.results.iter().map(|item| item.value.kind).collect();
    // a sum of plain numbers is a dimensionless quantity to the interpreter
    assert_eq!(kinds, ["quantity", "bool", "string", "vector"]);
    let five = &report.results[0].value;
    assert_eq!((five.value, five.unit.as_deref(), five.dimension.as_deref()), (Some(5.0), Some(""), Some("1")));
    let vector = &report.results[3].value;
    assert_eq!(vector.items.as_ref().unwrap().len(), 6, "every element, not the display's first three");
    assert!(vector.items.as_ref().unwrap().iter().all(|item| item.kind == "quantity" && item.unit.as_deref() == Some("m")));
}

#[test]
fn a_dimension_error_is_said_once_by_the_checker() {
    let report = run("1 m + 1 s");
    assert!(!report.ok);
    assert_eq!(codes(&report), [(1, "dimension-mismatch")], "{:?}", report.diagnostics);
    assert!(report.results.is_empty());
}

#[test]
fn a_second_problem_on_a_flagged_line_is_reported_too() {
    // each line has two distinct problems: the checker says the first (the code), the interpreter stops at the other
    for (program, checker_code) in [
        ("undefined_fn(1) + (1 m + 1 s)", "dimension-mismatch"),
        ("x = 5 / 0 + (1 m + 1 s)", "dimension-mismatch"),
        ("y = ln(-1) + (1 m + 1 s)", "dimension-mismatch"),
        ("sqrt(-4 m^2) + 1 s", "dimension-mismatch"),
        ("x = vector(1 m, 2 s)[9] + 20 degC * 2", "temperature-points"),
        // an unknown name is another problem when the checker's is a different name
        ("undefined_fn(1) + 5 fooz", "unknown-name"),
        ("(\"a\" + 1) + 5 fooz", "unknown-name"),
        ("undefined_fn(1) + (5 m => fooz)", "unknown-unit"),
        ("(\"a\" + 1) + (5 m => zzz)", "unknown-unit"),
        // a comparison of two things that are not quantities is not the dimension error of the checker
        ("(vector(1, 2) == vector(1, 2)) == (1 m + 1 s == 2 m)", "dimension-mismatch"),
        ("(1 m != \"a\") == (1 m + 1 s == 2 m)", "dimension-mismatch"),
        ("undefined_fn(1) + (1 m > 0)", "dimension-mismatch"),
        // a function that wants another kind of argument is not the dimension of the checker's
        ("uncertainty(\"a\") + sin(1 m)", "bad-argument"),
        ("undefined_fn(1) + sin(1 m)", "bad-argument"),
        ("undefined_fn(1) + 2 m ^ (1 s)", "bad-exponent"),
        ("limit(\"x\", \"x\", 20 degC) + 20 degC * 2", "temperature-points"),
        ("undefined_fn(1) + (if 5 then 1 else 2)", "not-a-bool"),
        // a name with no value (T5) is a problem of its own when the checker's is another one
        ("undefined_fn(barz_q) + (1 m + 1 s)", "dimension-mismatch"),
        ("undefined_fn(barz_q) + 5 fooz", "unknown-name"),
        // a `max` of different dimensions is one problem, another on its line is another
        ("undefined_fn(1) + max(1 m, 1 s)", "mixed-extremum"),
    ] {
        let report = run(program);
        assert!(!report.ok, "{program:?}");
        let errors: Vec<(usize, &str)> = report.diagnostics.iter().filter(|d| d.severity == "error").map(|d| (d.line, d.code.as_str())).collect();
        assert_eq!(errors, [(1, checker_code), (1, "runtime")], "{program:?}: {:?}", report.diagnostics);
        assert_eq!(
            canonical_witness(&report),
            format!("{WITNESS_FORMAT}\nok false\nerror {checker_code}\nerror runtime\n"),
            "{program:?}"
        );
    }
}

/// `(the code the checker gives, the line it flags, a program with exactly ONE problem)`: the report says each
/// once, never again as the interpreter's `runtime` error. A reworded interpreter message that the matcher
/// (`check::explains`) stops recognising shows up here as a second finding.
const ONE_PROBLEM: &[(&str, usize, &str)] = &[
    ("dimension-mismatch", 1, "1 m + 1 s"),
    ("dimension-mismatch", 1, "1 m - 1 s"),
    ("dimension-mismatch", 1, "5 kg + 2 m/s"),
    ("dimension-mismatch", 1, "1 m > 1 s"),
    ("dimension-mismatch", 1, "1 m == 1 s"),
    ("dimension-mismatch", 1, "1 m > 0"),
    ("dimension-mismatch", 1, "0 < 1 m"),
    ("dimension-mismatch", 1, "1 m == 1"),
    ("dimension-mismatch", 1, "2 kg >= 0.5"),
    ("dimension-mismatch", 1, "(2 m)^2 + 1 m"),
    ("dimension-mismatch", 1, "sqrt(4 m) + 1 m"),
    ("dimension-mismatch", 2, "x = 2 m\nx > 0"),
    ("dimension-mismatch", 3, "x = 1 m\ny = 2 s\nx + y"),
    ("dimension-mismatch", 2, "x = 2 m\nx + 3 s"),
    ("dimension-mismatch", 1, "1 J + 1 W"),
    ("dimension-mismatch", 1, "20 dB == 100"),
    ("dimension-mismatch", 1, "5 m => kg"),
    ("unknown-unit", 1, "5 m => fooz"),
    ("unknown-unit", 1, "5 m => zzz"),
    ("unknown-name", 1, "5 fooz"),
    ("unknown-name", 1, "x = 3 barz"),
    ("bad-argument", 1, "sin(1 m)"),
    ("bad-argument", 1, "exp(1 s)"),
    ("bad-argument", 1, "ln(2 kg)"),
    ("bad-argument", 1, "sec(1 m)"),
    ("bad-argument", 1, "erf(1 m)"),
    ("bad-argument", 2, "fn kinetic(m: kg, v: m/s) = 0.5 * m * v^2\nkinetic(2 s, 3 m/s)"),
    ("bad-argument", 2, "fn speed(v: m/s) = v * 2\nspeed(5)"),
    ("bad-exponent", 1, "2 m ^ (1 s)"),
    ("temperature-points", 1, "20 degC + 5 degC"),
    ("temperature-points", 1, "5 K - 20 degC"),
    ("temperature-points", 1, "20 degC * 2"),
    ("temperature-points", 1, "20 degC / 2"),
    ("temperature-points", 1, "2 / 5 degC"),
    ("not-a-bool", 2, "x = 5\nif x then 1 else 2"),
    ("not-a-bool", 1, "if 5 m then 1 else 2"),
    ("not-a-bool", 2, "x = 2 m\nx ? 1 : 2"),
    ("not-a-bool", 1, "not 5"),
    ("not-a-bool", 2, "s = \"on\"\nif s then 1 else 2"),
    ("not-a-bool", 1, "while true {\n  1\n}"),
    ("reserved-name", 1, "max = 3"),
    ("reserved-name", 1, "use = 3"),
    ("reserved-name", 1, "inf = 3"),
    ("reserved-name", 1, "fn f(round) = round"),
    ("reserved-name", 1, "use solve as max from calc"),
    ("reserved-name", 2, "limit = 1\nuse solve, limit from calc"),
    ("reserved-name", 1, "x = 2 * y where y = 1, max = 3"),
    ("constant-shadowing", 1, "pi = 3"),
    ("constant-shadowing", 1, "molar_gas_constant = 8"),
    ("constant-shadowing", 1, "fn g(bohr_radius) = bohr_radius"),
    ("mixed-extremum", 1, "max(1 m, 1 s)"),
    ("mixed-extremum", 1, "min(1 m, 2)"),
    ("mixed-extremum", 2, "x = 1 rad
max(x, 1)"),
];

#[test]
fn the_checker_explains_an_interpreter_error_only_by_its_own_words_and_names() {
    use physure_script::check::explains;
    // the same unit, by name
    assert!(explains("unknown-name", "'fooz' is neither a variable, a constant nor a unit, and meets a quantity here", "Unknown unit 'fooz' - did you mean 'foot'?"));
    assert!(!explains("unknown-name", "'fooz' is neither a variable, a constant nor a unit, and meets a quantity here", "Unknown unit 'a' - did you mean 'A'?"));
    assert!(!explains("unknown-name", "'fooz' is neither a variable, a constant nor a unit, and meets a quantity here", "Undefined function 'fooz'"));
    // the same name, in the interpreter's words for a name with no value (T5)
    assert!(explains("unknown-name", "'fooz' is neither a variable, a constant nor a unit, and meets a quantity here", "`fooz` has no value — did you mean the unit 'foot'?"));
    assert!(!explains("unknown-name", "'fooz' is neither a variable, a constant nor a unit, and meets a quantity here", "`barz` has no value"));
    assert!(explains("unknown-unit", "Unknown unit 'zzz'", "Unknown unit 'zzz'"));
    assert!(!explains("unknown-unit", "Unknown unit 'zzz'", "Unknown unit 'yyy'"));
    // the same operation on the temperatures
    let added = "two absolute temperatures cannot be added: one of them is an interval (K, delta_celsius)";
    let scaled = "an absolute temperature cannot be multiplied or divided by a plain number: the result depends on the scale";
    assert!(explains("temperature-points", added, "cannot add two absolute temperatures ('degC' and 'degC'): add an interval to one of them"));
    assert!(!explains("temperature-points", added, "cannot multiply or divide the absolute temperature 'degC' by a plain number: the result depends on the scale"));
    assert!(explains("temperature-points", scaled, "cannot multiply or divide the absolute temperature 'degC' by a plain number: the result depends on the scale"));
    assert!(!explains("temperature-points", scaled, "cannot subtract the absolute temperature 'degC' from the interval 'K'"));
    // the same function, or the same parameter of the same function
    let sin = "sin expects an angle or a dimensionless value, got L";
    assert!(explains("bad-argument", sin, "sin expects an angle or a dimensionless value, got 'm'"));
    assert!(!explains("bad-argument", sin, "cos expects an angle or a dimensionless value, got 'm'"));
    assert!(!explains("bad-argument", sin, "uncertainty expects a quantity"));
    let parameter = "the argument for parameter 'm' of 'kinetic' is T, the declared unit is M";
    assert!(explains("bad-argument", parameter, "Argument for parameter 'm' of function 'kinetic' has a unit incompatible with declared unit 'kg': Unit mismatch"));
    assert!(!explains("bad-argument", parameter, "Argument for parameter 'v' of function 'kinetic' has a unit incompatible with declared unit 'kg': Unit mismatch"));
    // a comparison of things that are not quantities is not a dimension error
    assert!(explains("dimension-mismatch", "cannot add: the left side is L, the right side is T", "Unit mismatch: expected 'm', got 's'"));
    assert!(!explains("dimension-mismatch", "cannot add: the left side is L, the right side is T", "cannot compare Vector and Vector with =="));
    // the same name bound, the same condition
    let bound = "`max` is a function of PHS and cannot be bound as a variable; choose another name";
    assert!(explains("reserved-name", bound, bound));
    assert!(!explains("reserved-name", bound, "`use` is a word of the language and cannot be bound as a variable; choose another name"));
    let condition = "the condition of `if` needs a Bool (True or False), got a Quantity (of dimension L): a quantity is not a condition; compare it, e.g. `x != 0 m`";
    assert!(explains("not-a-bool", condition, "the condition of `if` needs a Bool (True or False), got a Quantity (5.0 km): a quantity is not a condition; compare it, e.g. `x != 0 km`"));
    assert!(!explains("not-a-bool", condition, "the condition of `while` needs a Bool (True or False), got a Quantity (5.0 m): a quantity is not a condition"));
    // the same function of different dimensions (unit holes), not another function nor another kind of problem
    let extremum = "max of different dimensions (L, T): compare values of one dimension";
    assert!(explains("mixed-extremum", extremum, "max of different dimensions: `1 m` is a length and `1 s` a time; compare values of one dimension"));
    assert!(!explains("mixed-extremum", extremum, "min of different dimensions: `1 m` is a length and `1 s` a time; compare values of one dimension"));
    assert!(!explains("mixed-extremum", extremum, "Unit mismatch: expected 'm', got 's'"));
    // a code the interpreter has no words for explains nothing
    assert!(!explains("unused", "x", "Unit mismatch: expected 'm', got 's'"));
}

#[test]
fn a_flagged_line_is_not_said_twice() {
    for &(code, line, program) in ONE_PROBLEM {
        let report = run(program);
        assert!(!report.ok, "{program:?}");
        let errors: Vec<(usize, &str)> = report.diagnostics.iter().filter(|d| d.severity == "error").map(|d| (d.line, d.code.as_str())).collect();
        assert_eq!(errors, [(line, code)], "{program:?}: {:?}", report.diagnostics);
    }
}

#[test]
fn an_unknown_unit_is_reported() {
    let report = run("5 fooz");
    assert!(!report.ok);
    assert_eq!(codes(&report), [(1, "unknown-name")], "{:?}", report.diagnostics);
}

#[test]
fn a_syntax_error_has_its_line_and_no_results() {
    let report = run("a = 1 m\nb = (2 +\n");
    assert!(!report.ok);
    assert!(report.results.is_empty());
    assert_eq!(report.diagnostics.len(), 1);
    let finding = &report.diagnostics[0];
    assert_eq!((finding.code.as_str(), finding.severity), ("syntax", "error"));
    assert!(finding.line >= 2, "line {} in {}", finding.line, finding.message);
}

#[test]
fn a_runtime_error_the_checker_did_not_see_is_reported_and_evaluation_goes_on() {
    let report = run("undefined_function(1)\n2 m + 3 m");
    assert!(!report.ok);
    assert_eq!(codes(&report), [(1, "runtime")], "{:?}", report.diagnostics);
    assert_eq!(report.results.len(), 1);
    assert_eq!(report.results[0].line, 2);
}

#[test]
fn a_limit_stops_the_run() {
    let recursion = run("fn f(x) = f(x)\nf(1)\n3 m");
    assert_eq!(codes(&recursion), [(2, "limit")], "{:?}", recursion.diagnostics);
    assert!(recursion.results.is_empty(), "evaluation went on past a limit: {:?}", recursion.results);

    let late = evaluate("1 m + 1 m\n2 m", Limits::sandboxed(Duration::ZERO), DEFAULT_SEED);
    assert_eq!(codes(&late), [(1, "limit")], "{:?}", late.diagnostics);
}

#[test]
fn a_file_write_is_forbidden() {
    let path = std::env::temp_dir().join(format!("physure_report_{}.stl", std::process::id()));
    let source = format!("use export3d from plot\nexport3d(\"sin(x)*cos(y)\", \"{}\", \"stl\")", path.to_str().unwrap().replace(std::path::MAIN_SEPARATOR, "/"));
    let report = run(&source);
    assert_eq!(codes(&report), [(2, "forbidden")], "{:?}", report.diagnostics);
    assert!(!path.exists());
}

#[test]
fn the_output_cap_stops_the_results_and_says_so() {
    let mut limits = Limits::sandboxed(Duration::from_secs(60));
    limits.max_output_bytes = Some(50);
    let line = "\"aaaaaaaaaaaaaaaaaaaa\"\n"; // 20 bytes of text each
    let report = evaluate(&line.repeat(5), limits, DEFAULT_SEED);
    assert!(!report.ok);
    assert_eq!(report.results.len(), 2, "{:?}", report.results);
    assert_eq!(codes(&report), [(3, "limit")], "{:?}", report.diagnostics);
}

#[test]
fn the_json_has_the_documented_shape() {
    let report = run("x = 2 +/- 0.1 m\nx * x");
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["ok"], true);
    let second = &json["results"][1];
    assert_eq!(second["line"], 2);
    assert_eq!(second["kind"], "quantity");
    assert_eq!(second["dimension"], "L²");
    assert!(second["value"].as_f64().unwrap() > 3.9 && second["unit"].is_string(), "{second}");
    assert!(second.get("name").is_none() && second.get("items").is_none(), "absent fields are omitted: {second}");
    assert!(json["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn the_same_seed_gives_the_same_json() {
    let source = "x = 10 +/- 1 m\ny = x * x + x\ny";
    let once = serde_json::to_string(&evaluate(source, Limits::sandboxed(Duration::from_secs(60)), 5)).unwrap();
    let twice = serde_json::to_string(&evaluate(source, Limits::sandboxed(Duration::from_secs(60)), 5)).unwrap();
    assert_eq!(once, twice);
}

#[test]
fn describe_unit_answers_what_the_core_knows() {
    let km = describe_unit("km").unwrap();
    assert_eq!((km.dimension.as_str(), km.kind, km.linear), ("L", "delta", true));
    let newton = describe_unit("N").unwrap();
    assert_eq!(newton.dimension, "L·M·T⁻²");
    assert_ne!(newton.simplified, "N", "a recipe unit is spelled in base units");
    let celsius = describe_unit("degC").unwrap();
    assert_eq!((celsius.kind, celsius.linear), ("point", false));
    assert_eq!(describe_unit("fooz").unwrap_err(), "unknown unit 'fooz'");
}

#[test]
fn convert_gives_the_value_unit_and_dimension() {
    use physure_script::report::convert;
    let metres = convert(2.5, "km", "m").unwrap();
    assert_eq!((metres.kind, metres.dimension.as_deref()), ("quantity", Some("L")));
    assert!((metres.value.unwrap() - 2500.0).abs() < 1e-9, "{metres:?}");
    let kelvin = convert(20.0, "degC", "K").unwrap();
    assert!((kelvin.value.unwrap() - 293.15).abs() < 1e-9, "{kelvin:?}");
}

#[test]
fn convert_refuses_what_the_core_refuses_and_a_unit_that_is_too_long() {
    use physure_script::report::{convert, MAX_UNIT_LEN};
    assert!(convert(1.0, "m", "s").is_err());
    assert!(convert(1.0, "fooz", "m").is_err());
    let long = "m*".repeat(MAX_UNIT_LEN);
    assert!(convert(1.0, &long, "m").unwrap_err().contains("too long"));
    assert!(describe_unit(&long).unwrap_err().contains("too long"));
}


// ---- the canonical result of a witness (conf freeze Phase 3, correction C6) ----

use physure_script::report::{canonical_witness, evaluate_with_ceilings, Finding, Item, Value, WITNESS_FORMAT};

fn quantity(value: f64, uncertainty: f64) -> Value {
    Value {
        kind: "quantity",
        text: format!("{value} m"),
        value: Some(value),
        uncertainty: Some(uncertainty),
        unit: Some("m".into()),
        dimension: Some("L".into()),
        items: None,
    }
}

fn one_item(value: Value, diagnostics: Vec<Finding>) -> Report {
    Report { ok: true, results: vec![Item { line: 1, name: Some("x".into()), value }], diagnostics }
}

fn finding(severity: &'static str, code: &str, message: &str) -> Finding {
    Finding { line: 1, severity, code: code.into(), message: message.into() }
}

#[test]
fn the_canonical_witness_names_its_format_and_every_part_of_a_result() {
    let text = canonical_witness(&one_item(quantity(1.5, 0.25), vec![finding("error", "runtime", "boom")]));
    assert_eq!(
        text,
        format!(
            "{WITNESS_FORMAT}\nok true\nitem line=1 name=\"x\" kind=quantity value={:016x} uncertainty={:016x} unit=\"m\" dimension=\"L\"\nerror runtime\n",
            1.5f64.to_bits(),
            0.25f64.to_bits()
        )
    );
    assert_eq!(WITNESS_FORMAT, "phs-witness-1");
}

#[test]
fn the_canonical_witness_is_stable_under_message_text_and_warnings() {
    let base = canonical_witness(&one_item(quantity(2.0, 0.0), vec![finding("error", "dimension-mismatch", "cannot add m and s")]));
    let reworded = canonical_witness(&one_item(
        quantity(2.0, 0.0),
        vec![finding("error", "dimension-mismatch", "m and s do not add"), finding("warning", "unused", "x is never read")],
    ));
    assert_eq!(base, reworded);
    // the printed text of a number is not part of it either (only its bits are)
    let mut printed = quantity(2.0, 0.0);
    printed.text = "2.000 m".into();
    assert_eq!(canonical_witness(&one_item(printed, Vec::new())), canonical_witness(&one_item(quantity(2.0, 0.0), Vec::new())));
    // a code is
    let other_code = canonical_witness(&one_item(quantity(2.0, 0.0), vec![finding("error", "runtime", "cannot add m and s")]));
    assert_ne!(base, other_code);
}

#[test]
fn the_canonical_witness_changes_when_a_value_moves_by_one_ulp() {
    let x = 0.1f64 + 0.2;
    let next = f64::from_bits(x.to_bits() + 1);
    let a = canonical_witness(&one_item(quantity(x, 0.01), Vec::new()));
    assert_ne!(a, canonical_witness(&one_item(quantity(next, 0.01), Vec::new())));
    assert_ne!(a, canonical_witness(&one_item(quantity(x, f64::from_bits(0.01f64.to_bits() + 1)), Vec::new())));
    assert_ne!(canonical_witness(&one_item(quantity(0.0, 0.0), Vec::new())), canonical_witness(&one_item(quantity(-0.0, 0.0), Vec::new())));
}

#[test]
fn non_finite_and_absent_values_are_explicit() {
    let infinite = canonical_witness(&one_item(quantity(f64::INFINITY, 0.0), Vec::new()));
    let negative = canonical_witness(&one_item(quantity(f64::NEG_INFINITY, 0.0), Vec::new()));
    let nan = canonical_witness(&one_item(quantity(f64::NAN, 0.0), Vec::new()));
    assert!(infinite.contains("value=7ff0000000000000"), "{infinite}");
    assert!(negative.contains("value=fff0000000000000"), "{negative}");
    assert!(nan.contains(&format!("value={:016x}", f64::NAN.to_bits())), "{nan}");
    assert!(infinite != negative && infinite != nan && negative != nan);
    // a value without a number says so, and what it is
    let boolean = Value { kind: "bool", text: "true".into(), value: None, uncertainty: None, unit: None, dimension: None, items: None };
    let text = canonical_witness(&Report { ok: true, results: vec![Item { line: 2, name: None, value: boolean }], diagnostics: Vec::new() });
    assert!(text.contains("item line=2 name=- kind=bool value=- uncertainty=- unit=- dimension=- text=\"true\""), "{text}");
}

#[test]
fn the_canonical_witness_of_a_real_run_has_its_vector_elements_and_strings() {
    let report = run("v = vector(1 m, 2 m)\ns = \"two\nlines\"\nok_flag = 1 m < 2 m");
    assert!(report.ok, "{:?}", report.diagnostics);
    let text = canonical_witness(&report);
    assert_eq!(text.lines().filter(|l| l.starts_with("element 1 kind=quantity")).count(), 2, "{text}");
    assert!(text.contains(r#"text="\"two\nlines\"""#) || text.contains(r#"text="two\nlines""#), "{text}");
    assert_eq!(text.lines().count(), 2 + 3 + 2, "{text}");
    assert_eq!(canonical_witness(&run("v = vector(1 m, 2 m)\ns = \"two\nlines\"\nok_flag = 1 m < 2 m")), text, "deterministic");
}

#[test]
fn the_fixed_ceilings_hold_whatever_the_process_wide_settings_say() {
    let program = "x = for i in 0..5000 { i }";
    let limits = || Limits::sandboxed(Duration::from_secs(60));
    let before = physure_core::settings::set_max_loop_iterations(usize::MAX);
    let refused = evaluate_with_ceilings(program, limits(), DEFAULT_SEED, 1_000, 1_000, 10_000);
    let allowed = evaluate_with_ceilings(program, limits(), DEFAULT_SEED, 10_000, 1_000, 10_000);
    physure_core::settings::set_max_loop_iterations(before);
    assert!(!refused.ok);
    assert_eq!(codes(&refused), [(1, "limit")], "{:?}", refused.diagnostics);
    assert!(refused.diagnostics[0].message.contains("ceiling of 1000"), "{:?}", refused.diagnostics);
    assert!(allowed.ok, "{:?}", allowed.diagnostics);
}

/// The `while` ceiling of a witness is the one given, not the process-wide `[Settings] max_while_iterations` (which
/// a frozen file received from elsewhere sets).
#[test]
fn the_fixed_while_ceiling_holds_whatever_the_process_wide_setting_says() {
    let program = "i = 0\nwhile i < 20000 {\n  i = i + 1\n}\ni";
    let limits = || Limits::sandboxed(Duration::from_secs(60));
    let before = physure_core::settings::set_max_while_iterations(usize::MAX);
    let refused = evaluate_with_ceilings(program, limits(), DEFAULT_SEED, 1_000_000, 1_000, 10_000);
    let allowed = evaluate_with_ceilings(program, limits(), DEFAULT_SEED, 1_000_000, 1_000, 20_000);
    physure_core::settings::set_max_while_iterations(before);
    assert!(!refused.ok);
    assert!(refused.diagnostics.iter().any(|d| d.message.contains("did not converge after 10000 iterations")), "{:?}", refused.diagnostics);
    assert!(allowed.ok, "{:?}", allowed.diagnostics);
}
