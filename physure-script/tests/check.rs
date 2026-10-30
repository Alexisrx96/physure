//! Static dimensional checking of PHS: what it reports, what it leaves alone, and that an error is an error
//! the interpreter would also raise.

use physure_core::units::conf::{build_registry_from_conf, extend_registry};
use physure_script::check::{check_program, check_source, Diagnostic, Severity};
use physure_script::interpreter::eval_phs;

fn found(source: &str) -> Vec<Diagnostic> {
    check_source(source).unwrap_or_else(|e| panic!("{source:?} does not parse: {e}"))
}

/// `(line, code)` of every error.
fn errors(source: &str) -> Vec<(usize, &'static str)> {
    found(source).iter().filter(|d| d.severity == Severity::Error).map(|d| (d.line, d.code)).collect()
}

/// The checker reports `code` at `line`, and the interpreter really does refuse the program.
fn assert_refused(source: &str, line: usize, code: &'static str) {
    assert!(errors(source).contains(&(line, code)), "{source:?}: expected {code} at line {line}, found {:?}", found(source));
    assert!(eval_phs(source).is_err(), "{source:?}: the checker reports it and the interpreter accepts it");
}

/// The checker is silent, and the interpreter runs the program.
fn assert_clean(source: &str) {
    assert!(found(source).is_empty(), "{source:?}: false positive {:?}", found(source));
    eval_phs(source).unwrap_or_else(|e| panic!("{source:?}: the interpreter refuses a clean program: {e}"));
}

// ---- sums, differences, comparisons

#[test]
fn a_sum_or_a_difference_of_different_dimensions_is_an_error() {
    assert_refused("1 m + 1 s", 1, "dimension-mismatch");
    assert_refused("5 kg - 2 m", 1, "dimension-mismatch");
    assert_refused("x = 3 m\ny = 2 s\nx + y", 3, "dimension-mismatch");
    assert_refused("1 m + 1", 1, "dimension-mismatch");
}

#[test]
fn a_sum_in_different_units_of_one_dimension_is_fine() {
    assert_clean("1 km + 5 m");
    assert_clean("1 J + 2 N*m");
    assert_clean("3 W*s - 1 J");
}

#[test]
fn a_comparison_of_different_dimensions_is_an_error() {
    assert_refused("1 m < 1 s", 1, "dimension-mismatch");
    assert_clean("1 km > 5 m");
}

#[test]
fn a_dimension_follows_assignments_products_and_quotients() {
    assert_refused("d = 10 m\nt = 2 s\nv = d / t\nv + d", 4, "dimension-mismatch");
    assert_clean("d = 10 m\nt = 2 s\nv = d / t\nv * t + d");
    assert_clean("mass = 2 kg\nacc = 3 m/s^2\nF = mass * acc\nF + 1 N");
    assert_refused("mass = 2 kg\nacc = 3 m/s^2\nF = mass * acc\nF + 1 J", 4, "dimension-mismatch");
}

// ---- conversions and units

#[test]
fn a_conversion_between_dimensions_is_an_error() {
    assert_refused("5 m => s", 1, "dimension-mismatch");
    assert_clean("5 km => m");
    assert_refused("v = 3 m/s\nv => kg", 2, "dimension-mismatch");
}

#[test]
fn a_unit_the_catalog_does_not_have_is_an_error() {
    assert_refused("5 m => fooz", 1, "unknown-unit");
    // written after a number the word is read as a name, which the interpreter refuses just the same
    assert_refused("5 fooz", 1, "unknown-name");
    assert_refused("x = 2 barrelz\nx", 1, "unknown-name");
}

#[test]
fn a_name_that_is_neither_a_variable_nor_a_unit_is_an_error_when_it_meets_a_quantity() {
    assert_refused("5 m * foo", 1, "unknown-name");
    assert_refused("5 m + foo", 1, "unknown-name");
    // the same word, bound
    assert_clean("foo = 2\n5 m * foo");
    // a word that is a unit is a unit
    assert_clean("5 m * s");
}

#[test]
fn names_that_may_come_from_elsewhere_are_not_reported() {
    // bound later, as a function parameter, by an import
    assert_clean("fn f(foo) = 5 m * foo\nf(2)");
    assert!(errors("use * from \"other\"\n5 m * foo").is_empty());
    assert!(errors("use foo from \"other\"\n5 m * foo").is_empty());
}

#[test]
fn a_binding_beats_a_unit_even_before_it_is_made() {
    // `s` is seconds until it is assigned, and a body runs when it is called, after
    assert!(errors("fn f(x) = x + s\ns = 2 m\nf(1 m)").is_empty());
}

// ---- temperatures

#[test]
fn temperature_points_and_intervals_follow_the_core() {
    assert_clean("20 degC + 5 K");
    assert_clean("25 degC - 20 degC");
    assert_clean("20 degC - 5 K");
    assert_refused("20 degC + 5 degC", 1, "temperature-points");
    assert_refused("5 K - 20 degC", 1, "temperature-points");
    assert_refused("20 degC * 2", 1, "temperature-points");
    assert_refused("20 degC / 2", 1, "temperature-points");
    // an absolute temperature in a product with a dimensioned quantity is the absolute scale
    assert!(errors("8.314 J/(mol*K) * 25 degC").is_empty());
}

// ---- powers

#[test]
fn powers_scale_the_dimension_and_an_exponent_is_a_dimensionless_constant() {
    assert_clean("(2 m)^2 + 1 m^2");
    assert_refused("(2 m)^2 + 1 m", 1, "dimension-mismatch");
    assert_clean("(2 m)^2 / (1 m) + 1 m");
    assert_refused("2 m ^ (1 s)", 1, "bad-exponent");
    assert_clean("x = 3\n(2 m)^x");
}

// ---- builtins

#[test]
fn the_builtins_apply_their_dimension_rules() {
    assert_clean("sin(90 deg) + 1");
    assert_refused("sin(1 m)", 1, "bad-argument");
    assert_refused("exp(1 s)", 1, "bad-argument");
    assert_refused("ln(2 kg)", 1, "bad-argument");
    assert_clean("sqrt(4 m^2) + 1 m");
    assert_refused("sqrt(4 m) + 1 m", 1, "dimension-mismatch");
    assert_clean("abs(-3 m) + 1 m");
    assert_clean("max(1 m, 2 km) + 1 m");
}

#[test]
fn a_maximum_of_different_dimensions_is_a_warning_the_interpreter_accepts() {
    let diagnostics = found("max(1 m, 1 s)");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].severity, diagnostics[0].code), (Severity::Warning, "mixed-extremum"));
    assert!(eval_phs("max(1 m, 1 s)").is_ok(), "the interpreter compares base-unit magnitudes");
}

// ---- functions

#[test]
fn an_annotated_parameter_is_checked_at_the_call_and_typed_in_the_body() {
    let program = "fn kinetic(m: kg, v: m/s) = 0.5 * m * v^2\n";
    assert_clean(&format!("{program}kinetic(2 kg, 3 m/s) + 1 J"));
    assert_refused(&format!("{program}kinetic(2 s, 3 m/s)"), 2, "bad-argument");
    assert_refused(&format!("{program}kinetic(2 kg, 3 m/s) + 1 m"), 2, "dimension-mismatch");
    // inside the body the parameters have their declared dimension
    assert_refused("fn f(m: kg) = m + 1 s\nf(1 kg)", 1, "dimension-mismatch");
}

#[test]
fn an_argument_in_another_unit_of_the_same_dimension_is_converted_not_refused() {
    assert_clean("fn speed(v: m/s) = v * 2\nspeed(18 km/h)");
}

#[test]
fn a_bare_number_for_an_annotated_parameter_needs_implicit_units() {
    assert_refused("fn speed(v: m/s) = v * 2\nspeed(5)", 2, "bad-argument");
    assert_clean("@implicit_units\nfn speed(v: m/s) = v * 2\nspeed(5)");
}

#[test]
fn an_unannotated_parameter_takes_the_dimension_of_each_call() {
    // the sum inside is fine for two lengths and wrong for a length and a time
    assert_clean("fn double(a, b) = a + b\ndouble(1 m, 2 km)");
    assert_refused("fn add(a, b) = a + b\nadd(1 m, 1 s)", 1, "dimension-mismatch");
    let message = &found("fn add(a, b) = a + b\nadd(1 m, 1 s)")[0].message;
    assert!(message.contains("called as add("), "{message}");
    assert!(message.contains("at line 2"), "{message}");
}

#[test]
fn what_a_function_returns_is_typed_from_its_arguments() {
    assert_clean("fn area(w, h) = w * h\narea(2 m, 3 m) + 1 m^2");
    assert_refused("fn area(w, h) = w * h\narea(2 m, 3 m) + 1 m", 2, "dimension-mismatch");
    assert_refused("fn half(x) = x / 2\nhalf(4 s) + 1 m", 2, "dimension-mismatch");
}

#[test]
fn a_function_is_checked_for_every_distinct_set_of_dimensions_once() {
    let source = "fn add(a, b) = a + b\nadd(1 m, 1 s)\nadd(1 m, 1 s)\nadd(1 kg, 1 s)";
    let mismatches = found(source).iter().filter(|d| d.code == "dimension-mismatch").count();
    assert_eq!(mismatches, 2, "one for (L, T), one for (M, T): {:?}", found(source));
}

#[test]
fn recursion_ends() {
    assert!(errors("fn f(n) = f(n)\nf(1 m)").is_empty());
    assert!(errors("fn a(x) = b(x)\nfn b(x) = a(x)\na(1 m)").is_empty());
}

#[test]
fn a_call_to_something_the_pass_cannot_see_is_unknown_and_silences_what_depends_on_it() {
    assert_clean("x = 2 m\nx + 1 m");
    assert!(errors("unknown_function(1 m) + 1 s").is_empty());
}

#[test]
fn a_loop_forgets_what_its_body_may_have_changed() {
    let after_loop = errors("x = 1 m\nwhile x < 5 m { x = x + 1 m }\nx + 1 kg");
    assert!(after_loop.is_empty(), "x is not known after the loop: {after_loop:?}");
}

// ---- the report

#[test]
fn a_diagnostic_is_at_the_line_of_its_statement() {
    let diagnostics = found("a = 1 m\n\nb = 2 s\n\nc = a + b");
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 5);
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert!(diagnostics[0].message.contains("left side is L"), "{}", diagnostics[0].message);
    assert!(diagnostics[0].message.contains("right side is T"), "{}", diagnostics[0].message);
}

#[test]
fn source_that_does_not_parse_is_an_error_of_the_parser_and_not_a_diagnostic() {
    assert!(check_source("1 +").is_err());
}

#[test]
fn the_checker_reads_the_catalog_it_is_given() {
    let mut registry = build_registry_from_conf().0;
    let (_, mut constants) = build_registry_from_conf();
    let program = physure_script::parse_phs("5 fur + 1 m").unwrap();
    assert!(check_program(&program, &registry).iter().any(|d| d.severity == Severity::Error));
    extend_registry(&mut registry, &mut constants, "[Units]\nfurlong = 201.168, L, [fur, furlong]\n").unwrap();
    assert!(check_program(&program, &registry).is_empty());
}

// ---- no false positives on real programs

#[test]
fn the_tutorial_primer_has_no_error() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/tutorials/phs_primer.phs")).unwrap();
    let errors: Vec<_> = found(&source).into_iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "false positives on the primer: {errors:?}");
}

#[test]
fn programs_the_interpreter_runs_get_no_error() {
    for source in [
        "x = 5 m\ny = 3 m\nx + y",
        "g = 9.81 m/s^2\nt = 2 s\n0.5 * g * t^2 => m",
        "E = 3 kWh\nE => J",
        "p = 101325 Pa\np => kPa",
        "T = 300 K\nR = 8.314 J/(mol*K)\nn = 2 mol\nn * R * T => J",
        "fn f(x) = x^2\nf(3 m) => m^2",
        "v = 18 km/h\nv => m/s",
        "a = 2\nb = 3\na * b + 1",
        "x = 0.5 rad\nsin(x) + cos(x)",
        "10 dB + 20 dB",
        "1 ft + 1 inch",
    ] {
        assert_clean(source);
    }
}

// ---- the remaining forms

#[test]
fn a_fractional_power_is_exact() {
    assert_clean("(8 m^3)^(1/3) + 1 m");
    assert_refused("(8 m^3)^(1/3) + 1 m^3", 1, "dimension-mismatch");
    assert_clean("(4 m^2)^0.5 + 1 m");
}

#[test]
fn a_name_that_is_not_a_variable_converts_to_itself() {
    // the interpreter returns text unconverted, so there is nothing to report
    assert!(errors("foo => m").is_empty());
}

#[test]
fn a_function_with_returns_yields_what_they_agree_on() {
    let source = "fn f(x) =\n  y = x * 2\n  return y\nf(1 m) + 1 s";
    assert!(errors(source).iter().any(|(_, code)| *code == "dimension-mismatch"), "{:?}", found(source));
    assert_refused("fn g(x) =\n  return x * 2\ng(1 m) + 1 s", 3, "dimension-mismatch");
}

#[test]
fn the_body_of_a_loop_is_checked_and_its_results_are_forgotten() {
    assert!(errors("x = 1 m\nwhile x < 5 m { x = x + 1 s }").iter().any(|(_, code)| *code == "dimension-mismatch"));
    assert!(errors("x = 1 m\nwhile x < 5 m { x = x + 1 m }\nx + 1 kg").is_empty());
}

#[test]
fn a_comparison_yields_a_boolean_and_a_branch_yields_what_both_arms_agree_on() {
    assert!(errors("(1 m < 2 m) + 1").is_empty(), "a boolean is not a quantity: nothing to say");
    assert_refused("((1 m < 2 m) ? 3 m : 4 m) + 1 s", 1, "dimension-mismatch");
}

#[test]
fn rounding_and_the_uncertainty_keep_the_dimension() {
    assert_clean("round(2.567 m, 1) + 1 m");
    assert_refused("round(2.567 m, 1) + 1 s", 1, "dimension-mismatch");
    assert_clean("floor(2.5 m) + ceil(1.2 m)");
    assert_clean("x = 1.5 +/- 0.1 m\nuncertainty(x) + 1 m");
}

#[test]
fn a_function_reached_through_an_alias_or_defined_twice_is_the_latest() {
    assert_refused("fn f(x) = x + 1 m\nfn f(x) = x + 1 s\nf(1 m)", 2, "dimension-mismatch");
    assert!(errors("use g from other\ng(1 m) + 1 s").is_empty());
}

#[test]
fn the_message_names_both_dimensions() {
    let message = &found("1 m / 1 s + 1 kg")[0].message;
    assert!(message.contains("L·T⁻¹"), "{message}");
    assert!(message.contains('M'), "{message}");
    let message = &found("1 + 1 m")[0].message;
    assert!(message.contains("dimensionless"), "{message}");
}

// ---- what the pass leaves alone

#[test]
fn returns_that_disagree_yield_nothing_and_returns_that_agree_yield_their_dimension() {
    let disagree = "fn h(x) =\n  if x > 0 then return 1 m\n  return 1 s\nh(1) + 1 kg";
    assert!(errors(disagree).is_empty(), "{:?}", found(disagree));
    let agree = "fn h(x) =\n  if x > 0 then return 1 m\n  return 2 m\nh(1) + 1 s";
    assert!(errors(agree).iter().any(|(_, code)| *code == "dimension-mismatch"), "{:?}", found(agree));
}

#[test]
fn a_call_with_keywords_an_export_and_an_asymmetric_uncertainty_are_not_typed() {
    assert!(errors("fn f(a) = a\nf(a = 1 m) + 1 s").is_empty());
    assert!(errors("x = 1 m\nexport x").is_empty());
    assert!(errors("x = 12.3 +/- (0.5, 0.4) m\nx + 1 s").is_empty());
}

#[test]
fn an_exponent_that_is_not_a_constant_types_what_it_can() {
    // the exponent is a parameter: its value is not known, so a dimensioned base has no known result...
    assert!(errors("fn f(p) = (2 m)^p\nf(2) + 1 s").is_empty());
    // ...and a dimensionless base stays dimensionless
    assert_refused("fn g(p) = 2^p\ng(3) + 1 s", 2, "dimension-mismatch");
}

#[test]
fn constants_fold_through_the_arithmetic_that_makes_an_exponent() {
    assert_clean("(2 m)^(4 - 2) + 1 m^2");
    assert_clean("(2 m)^(6 / 3) + 1 m^2");
    assert_clean("(2 m)^(1 * 2) + 1 m^2");
}

#[test]
fn a_boolean_is_not_a_quantity_and_is_not_reported_as_one() {
    assert!(errors("(1 m < 2 m) and (1 s < 2 s)").is_empty());
    assert!(errors("fn f(x) = x\nf(1 m < 2 m) + 1 s").is_empty());
    assert!(errors("not (1 m < 2 m)").is_empty());
}

#[test]
fn a_function_called_before_it_is_defined_and_one_that_is_shadowed_are_unknown() {
    assert!(errors("f(1 m) + 1 s\nfn f(x) = x").is_empty());
    assert!(errors("fn f(x) = x\nf = 3 kg\nf(1 m) + 1 s").is_empty());
}
