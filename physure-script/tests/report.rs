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
