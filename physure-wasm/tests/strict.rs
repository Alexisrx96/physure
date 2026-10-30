//! Strict tests of the WASM binding: what a JavaScript caller gets must be what the core says, with the
//! uncertainty, the scale and the dimension intact, and an error where there is no answer.
//! Run with `wasm-pack test --node`.

use physure_wasm::{PhyFunction, Quantity, UnitRegistry};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;

fn q(value: f64, unit: &str) -> Quantity {
    Quantity::new(value, unit).unwrap()
}

fn qu(value: f64, uncertainty: f64, unit: &str) -> Quantity {
    Quantity::with_uncertainty(value, uncertainty, unit).unwrap()
}

fn message(error: JsValue) -> String {
    error.dyn_into::<js_sys::Error>().map(|e| String::from(e.message())).unwrap_or_else(|v| format!("{v:?}"))
}

fn close(actual: f64, expected: f64) {
    let tolerance = 1e-12 * expected.abs().max(1.0);
    assert!((actual - expected).abs() <= tolerance, "{actual} is not {expected}");
}

// ---- uncertainty

#[wasm_bindgen_test]
fn a_sum_and_a_difference_add_their_uncertainties_in_quadrature() {
    let (a, b) = (qu(10.0, 0.5, "m"), qu(5.0, 0.3, "m"));
    let sum = a.add(&b).unwrap();
    close(sum.value(), 15.0);
    close(sum.uncertainty(), 0.5f64.hypot(0.3));
    let difference = a.subtract(&b).unwrap();
    close(difference.value(), 5.0);
    close(difference.uncertainty(), 0.5f64.hypot(0.3));
}

#[wasm_bindgen_test]
fn a_product_and_a_quotient_add_their_relative_uncertainties_in_quadrature() {
    let (a, b) = (qu(10.0, 0.5, "m"), qu(5.0, 0.3, "s"));
    let relative = (0.5f64 / 10.0).hypot(0.3 / 5.0);
    let product = a.multiply(&b).unwrap();
    close(product.value(), 50.0);
    close(product.uncertainty(), 50.0 * relative);
    let quotient = a.divide(&b).unwrap();
    close(quotient.value(), 2.0);
    close(quotient.uncertainty(), 2.0 * relative);
}

#[wasm_bindgen_test]
fn a_power_scales_the_relative_uncertainty_by_the_exponent() {
    let squared = qu(3.0, 0.1, "m").pow(2.0).unwrap();
    close(squared.value(), 9.0);
    close(squared.uncertainty(), 2.0 * 3.0 * 0.1);
}

#[wasm_bindgen_test]
fn a_conversion_scales_the_uncertainty_with_the_value() {
    let converted = qu(10.0, 0.5, "m").convert_to("cm").unwrap();
    close(converted.value(), 1000.0);
    close(converted.uncertainty(), 50.0);
    assert_eq!(converted.unit(), "cm");
}

#[wasm_bindgen_test]
fn a_sum_in_two_units_keeps_the_uncertainty_of_both() {
    let sum = qu(1.0, 0.01, "km").add(&qu(500.0, 5.0, "m")).unwrap();
    close(sum.value(), 1.5);
    close(sum.uncertainty(), 0.01f64.hypot(0.005));
    assert_eq!(sum.unit(), "km");
}

// ---- scale: the number and the unit a caller reads belong together

#[wasm_bindgen_test]
fn a_product_of_two_scales_is_read_in_the_scale_it_is_shown() {
    let product = q(2.0, "m").multiply(&q(3.0, "km")).unwrap();
    assert_eq!((product.value(), product.unit()), (6000.0, "m^2".to_string()));
    assert_eq!(product.to_js_string(), "6000.0 m^2");
    let swapped = q(3.0, "km").multiply(&q(2.0, "m")).unwrap();
    assert_eq!((swapped.value(), swapped.unit()), (6000.0, "m^2".to_string()));
}

#[wasm_bindgen_test]
fn a_quotient_of_two_scales_is_a_dimensionless_number_of_the_right_size() {
    let ratio = q(3.0, "km").divide(&q(2.0, "m")).unwrap();
    assert_eq!((ratio.value(), ratio.unit()), (1500.0, String::new()));
}

#[wasm_bindgen_test]
fn the_uncertainty_of_a_product_of_two_scales_is_in_the_scale_of_its_value() {
    let product = qu(2.0, 0.1, "m").multiply(&q(3.0, "km")).unwrap();
    close(product.value(), 6000.0);
    close(product.uncertainty(), 300.0);
}

// ---- dimensions: an error, never a number

#[wasm_bindgen_test]
fn incompatible_quantities_do_not_add_subtract_or_convert() {
    let metre = q(1.0, "m");
    for other in [q(1.0, "s"), q(1.0, "kg"), q(5.0, "")] {
        assert!(metre.add(&other).is_err(), "m + {}", other.unit());
        assert!(metre.subtract(&other).is_err(), "m - {}", other.unit());
        assert!(other.add(&metre).is_err(), "{} + m", other.unit());
    }
    assert!(metre.convert_to("s").is_err());
    assert!(metre.convert_to("").is_err());
    assert!(q(5.0, "").convert_to("m").is_err());
    assert!(message(metre.convert_to("not_a_unit").unwrap_err()).contains("Unknown unit"));
}

#[wasm_bindgen_test]
fn assertions_compare_after_conversion_and_refuse_other_dimensions() {
    assert!(q(1.0, "m").phs_assert(&q(1000.0, "mm")).is_ok());
    assert!(q(1.0, "m").phs_assert(&q(2.0, "m")).is_err());
    assert!(q(1.0, "m").phs_assert(&q(1.0, "s")).is_err());
    assert!(q(1.0, "m").phs_exact_assert(&q(1.0, "meter")).is_ok());
    assert!(q(1.0, "m").phs_exact_assert(&q(1000.0, "mm")).is_err());
}

#[wasm_bindgen_test]
fn a_negative_base_has_no_fractional_power() {
    assert!(q(-4.0, "m^2").pow(0.5).is_err());
}

#[wasm_bindgen_test]
fn dividing_by_zero_is_an_error() {
    assert!(q(1.0, "m").divide(&q(0.0, "s")).is_err());
    assert!(q(1.0, "m").divide(&q(0.0, "")).is_err());
}

// ---- temperatures are points and intervals

#[wasm_bindgen_test]
fn a_temperature_point_plus_an_interval_is_a_point() {
    let sum = q(20.0, "degC").add(&q(5.0, "K")).unwrap();
    assert_eq!((sum.value(), sum.unit()), (25.0, "degC".to_string()));
    let difference = q(20.0, "degC").subtract(&q(5.0, "K")).unwrap();
    assert_eq!((difference.value(), difference.unit()), (15.0, "degC".to_string()));
    let interval = q(25.0, "degC").subtract(&q(20.0, "degC")).unwrap();
    assert_eq!((interval.value(), interval.unit()), (5.0, "K".to_string()));
}

#[wasm_bindgen_test]
fn two_temperature_points_do_not_add_and_a_plain_number_does_not_scale_one() {
    assert!(q(20.0, "degC").add(&q(5.0, "degC")).is_err());
    assert!(q(20.0, "degC").multiply(&q(2.0, "")).is_err());
}

#[wasm_bindgen_test]
fn temperatures_convert_between_scales() {
    close(q(20.0, "degC").convert_to("K").unwrap().value(), 293.15);
    close(q(20.0, "degC").convert_to("degF").unwrap().value(), 68.0);
    close(q(-40.0, "degC").convert_to("degF").unwrap().value(), -40.0);
}

// ---- the unit a result carries can be fed back in

#[wasm_bindgen_test]
fn the_unit_of_every_result_is_a_unit_the_binding_reads() {
    let results = [
        q(4.0, "m").pow(0.5).unwrap(),
        q(8.0, "m").pow(1.0 / 3.0).unwrap(),
        q(4.0, "s").pow(-0.5).unwrap(),
        q(2.0, "m").multiply(&q(3.0, "km")).unwrap(),
        q(6.0, "m").divide(&q(2.0, "s")).unwrap(),
        q(1.0, "kg").multiply(&q(1.0, "m")).unwrap().pow(0.25).unwrap(),
    ];
    for result in &results {
        let unit = result.unit();
        let read = Quantity::new(result.value(), &unit).unwrap_or_else(|e| panic!("{unit:?} does not read back: {}", message(e)));
        close(read.value(), result.value());
        // the same quantity, whatever the spacing of the symbol it was written with
        let in_read_unit = result.convert_to(&read.unit()).unwrap_or_else(|e| panic!("{unit:?} is not {:?}: {}", read.unit(), message(e)));
        close(in_read_unit.value(), read.value());
    }
}

#[wasm_bindgen_test]
fn a_square_root_times_itself_is_the_original_unit() {
    let root = q(4.0, "m").pow(0.5).unwrap();
    let back = root.multiply(&root).unwrap();
    assert_eq!((back.value(), back.unit()), (4.0, "m".to_string()));
}

// ---- numbers

#[wasm_bindgen_test]
fn not_a_number_and_infinity_travel_through_arithmetic() {
    assert!(q(f64::NAN, "m").add(&q(1.0, "m")).unwrap().value().is_nan());
    assert_eq!(q(f64::INFINITY, "m").multiply(&q(2.0, "s")).unwrap().value(), f64::INFINITY);
    assert_eq!(q(f64::INFINITY, "m").add(&q(1.0, "m")).unwrap().value(), f64::INFINITY);
}

#[wasm_bindgen_test]
fn negative_zero_and_extremes_survive() {
    assert!(q(-0.0, "m").value().is_sign_negative());
    assert_eq!(q(1e308, "m").multiply(&q(10.0, "")).unwrap().value(), f64::INFINITY);
    assert_eq!(q(5e-324, "m").value(), 5e-324);
}

#[wasm_bindgen_test]
fn an_uncertainty_that_is_not_a_finite_spread_is_refused_and_a_negative_one_is_its_magnitude() {
    assert!(Quantity::with_uncertainty(1.0, f64::NAN, "m").is_err());
    assert!(Quantity::with_uncertainty(1.0, f64::INFINITY, "m").is_err());
    assert_eq!(Quantity::with_uncertainty(1.0, -0.5, "m").unwrap().uncertainty(), 0.5);
}

// ---- the registry: evaluate

#[wasm_bindgen_test]
fn evaluate_returns_one_line_per_statement_and_the_last_is_the_answer() {
    let registry = UnitRegistry::new();
    let out = registry.evaluate("a = 15 m; b = 5 m; a * b").unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.last().copied(), Some("75.0 m^2"), "{out:?}");
    assert_eq!(registry.evaluate("").unwrap(), "");
}

#[wasm_bindgen_test]
fn variables_belong_to_their_registry() {
    let (one, other) = (UnitRegistry::new(), UnitRegistry::new());
    one.evaluate("x = 5 m").unwrap();
    assert_eq!(one.evaluate("x * 2").unwrap().trim(), "10.0 m");
    assert_ne!(other.evaluate("x").unwrap().trim(), "5.0 m");
}

#[wasm_bindgen_test]
fn evaluate_reports_errors_with_their_cause() {
    let registry = UnitRegistry::new();
    assert!(message(registry.evaluate("1 +").unwrap_err()).contains("Parse error"));
    assert!(message(registry.evaluate("1 m + 2 s").unwrap_err()).contains("mismatch"));
    assert!(message(registry.evaluate("1/0").unwrap_err()).contains("Division by zero"));
    assert!(message(registry.evaluate("nosuchfn(1)").unwrap_err()).contains("nosuchfn"));
}

#[wasm_bindgen_test]
fn evaluate_keeps_uncertainty_and_temperature_semantics() {
    let registry = UnitRegistry::new();
    let out = registry.evaluate("(1 +/- 0.1 m) + (2 +/- 0.2 m)").unwrap();
    assert!(out.starts_with("3.0 ± 0.2"), "{out:?}");
    assert_eq!(registry.evaluate("20 degC + 5 K").unwrap().trim(), "25.0 degC");
    assert!(registry.evaluate("2 * 20 degC").is_err());
    assert_eq!(registry.evaluate("sin(90 deg)").unwrap().trim(), "1.0");
    assert!(registry.evaluate("sin(1 m)").is_err());
}

#[wasm_bindgen_test]
fn evaluate_reads_and_writes_unicode_and_a_nul_without_damage() {
    let registry = UnitRegistry::new();
    assert_eq!(registry.evaluate("\"h\u{e9}llo \u{2713} \u{1F600}\"").unwrap(), "h\u{e9}llo \u{2713} \u{1F600}");
    assert_eq!(registry.evaluate("\"a\u{0}b\"").unwrap(), "a\u{0}b");
    assert_eq!(registry.evaluate(&format!("\"{}\"", "x".repeat(1_000_000))).unwrap().len(), 1_000_000);
}

#[wasm_bindgen_test]
fn a_runaway_loop_stops_with_an_error() {
    assert!(message(UnitRegistry::new().evaluate("while true { x = 1 }").unwrap_err()).contains("did not converge"));
}

#[wasm_bindgen_test]
fn moderate_nesting_is_parsed_quickly() {
    // 24 nested parentheses took 53 s: each level parsed its contents twice
    let registry = UnitRegistry::new();
    let source = format!("{}1{}", "(".repeat(18), ")".repeat(18));
    let start = js_sys::Date::now();
    assert_eq!(registry.evaluate(&source).unwrap().trim(), "1.0");
    assert!(js_sys::Date::now() - start < 250.0, "took {} ms", js_sys::Date::now() - start);
}

// ---- the registry: calculus

#[wasm_bindgen_test]
fn solve_derive_and_integrate_return_expressions() {
    let registry = UnitRegistry::new();
    let solved = registry.solve("P * V = n * R * T", "T").unwrap();
    assert!(solved.contains('P') && solved.contains('V') && solved.contains('n') && solved.contains('R'), "{solved}");
    let derivative = registry.deriv("v0 * t + 0.5 * a * t^2", "t").unwrap();
    assert!(derivative.contains("v0") && derivative.contains('a') && derivative.contains('t'), "{derivative}");
    let integral = registry.integral("3 * t^2", "t").unwrap();
    assert!(integral.contains("t^3"), "{integral}");
}

#[wasm_bindgen_test]
fn calculus_arguments_with_quotes_are_data_not_code() {
    let registry = UnitRegistry::new();
    assert!(registry.solve("x = \"a\"", "x").is_err());
    assert!(registry.deriv("t\"); 1/0; (\"t", "t").is_err());
    assert!(registry.solve("x = 1", "x\", \"y").is_err());
}

// ---- the registry: catalog and custom definitions

#[wasm_bindgen_test]
fn every_unit_of_every_category_is_known_to_the_registry() {
    let registry = UnitRegistry::new();
    let categories = registry.get_categories().unwrap();
    assert!(categories.size() >= 10);
    let mut checked = 0;
    let mut unknown = Vec::new();
    categories.for_each(&mut |units, _category| {
        for unit in js_sys::Array::from(&units).iter() {
            let name = unit.as_string().unwrap();
            checked += 1;
            match registry.get_unit_scale(&name) {
                Ok(scale) if scale.is_finite() && scale != 0.0 => {}
                other => unknown.push(format!("{name}: {other:?}")),
            }
        }
    });
    assert!(checked > 50, "only {checked} units");
    assert!(unknown.is_empty(), "{} units of the categories are not units: {}", unknown.len(), unknown.join(", "));
}

#[wasm_bindgen_test]
fn a_registry_from_content_knows_its_own_units_and_not_those_of_another() {
    let custom = UnitRegistry::from_content("[Units]\nfurlong = 201.168, L, [fur, furlong]\n").unwrap();
    let plain = UnitRegistry::new();
    assert_eq!(custom.get_unit_scale("fur").unwrap(), 201.168);
    assert_eq!(custom.get_unit_scale("km").unwrap(), 1000.0);
    assert!(plain.get_unit_scale("fur").is_err());
}

#[wasm_bindgen_test]
fn definitions_that_cannot_be_read_are_an_error_naming_the_cause() {
    for content in ["[Units]\nbad = notanumber, L, [bad]\n", "[Units]\nthis is not a unit line\n"] {
        let error = UnitRegistry::from_content(content).err().unwrap_or_else(|| panic!("{content:?} was accepted"));
        assert!(message(error).contains("Invalid unit definitions"), "{content:?}");
    }
}

#[wasm_bindgen_test]
fn definitions_in_the_content_replace_those_of_the_catalog_for_that_registry_only() {
    let custom = UnitRegistry::from_content("[Units]\nkm = 7.0, L, [km]\n").unwrap();
    assert_eq!(custom.get_unit_scale("km").unwrap(), 7.0);
    assert_eq!(UnitRegistry::new().get_unit_scale("km").unwrap(), 1000.0);
}

// ---- PhyFunction

fn area(registry: &UnitRegistry) -> PhyFunction {
    PhyFunction::new(registry, "area", "area(w, h) = w * h").unwrap()
}

fn args(values: &[&str]) -> Vec<JsValue> {
    values.iter().map(|v| JsValue::from_str(v)).collect()
}

#[wasm_bindgen_test]
fn a_function_reports_its_name_and_parameters_and_calls_with_text_arguments() {
    let registry = UnitRegistry::new();
    let f = area(&registry);
    assert_eq!(f.get_name(), "area");
    assert_eq!(f.get_params(), vec!["w".to_string(), "h".to_string()]);
    let result = f.call(args(&["2 m", "3 m"])).unwrap();
    assert_eq!((result.value(), result.unit()), (6.0, "m^2".to_string()));
}

#[wasm_bindgen_test]
fn a_function_called_with_quantities_loses_neither_precision_nor_uncertainty() {
    let registry = UnitRegistry::new();
    let f = area(&registry);
    let width = JsValue::from(q(1.123456789123, "m"));
    let exact = f.call(vec![width, JsValue::from(q(2.0, "m"))]).unwrap();
    close(exact.value(), 2.0 * 1.123456789123);
    let measured = f.call(vec![JsValue::from(qu(2.0, 0.1, "m")), JsValue::from(q(3.0, "m"))]).unwrap();
    close(measured.value(), 6.0);
    close(measured.uncertainty(), 6.0 * (0.1f64 / 2.0));
}

#[wasm_bindgen_test]
fn a_call_with_the_wrong_number_or_kind_of_argument_is_an_error() {
    let registry = UnitRegistry::new();
    let f = area(&registry);
    assert!(f.call(args(&["2 m"])).is_err());
    assert!(f.call(args(&["2 m", "3 m", "4 m"])).is_err());
    assert!(f.call(vec![JsValue::from_f64(2.0), JsValue::from_f64(3.0)]).is_err());
    let sum = PhyFunction::new(&registry, "plus", "plus(a, b) = a + b").unwrap();
    assert!(sum.call(args(&["2 m", "3 s"])).is_err());
}

#[wasm_bindgen_test]
fn derivatives_integrals_and_solutions_are_functions_that_compute() {
    let registry = UnitRegistry::new();
    let f = PhyFunction::new(&registry, "kinetic", "kinetic(m, v) = 0.5 * m * v^2").unwrap();
    let d = f.deriv("v").unwrap();
    let result = d.call(args(&["2 kg", "3 m/s"])).unwrap();
    close(result.value(), 6.0);
    let solved = f.solve("m").unwrap();
    let mass = solved.call(args(&["9 J", "3 m/s"])).unwrap();
    close(mass.value(), 2.0);
}

#[wasm_bindgen_test]
fn combining_functions_gives_functions_with_the_combined_values() {
    let registry = UnitRegistry::new();
    let f = PhyFunction::new(&registry, "fa", "fa(x) = x * 2").unwrap();
    let g = PhyFunction::new(&registry, "ga", "ga(x) = x + 1 m").unwrap();
    let x = || args(&["3 m"]);
    close(f.add(&g).unwrap().call(x()).unwrap().value(), 10.0);
    close(f.subtract(&g).unwrap().call(x()).unwrap().value(), 2.0);
    close(f.multiply(&g).unwrap().call(x()).unwrap().value(), 24.0);
    close(f.divide(&g).unwrap().call(x()).unwrap().value(), 1.5);
    close(f.compose(&g).unwrap().call(x()).unwrap().value(), 8.0);
    close(g.compose(&f).unwrap().call(x()).unwrap().value(), 7.0);
}

#[wasm_bindgen_test]
fn functions_of_two_registries_do_not_combine() {
    let (one, other) = (UnitRegistry::new(), UnitRegistry::new());
    let f = PhyFunction::new(&one, "fa", "fa(x) = x").unwrap();
    let g = PhyFunction::new(&other, "ga", "ga(x) = x").unwrap();
    assert!(f.add(&g).is_err());
    assert!(f.compose(&g).is_err());
}

#[wasm_bindgen_test]
fn a_body_that_is_not_a_definition_is_an_error() {
    let registry = UnitRegistry::new();
    assert!(PhyFunction::new(&registry, "bad", "1 +").is_err());
    assert!(PhyFunction::new(&registry, "empty", "").is_err());
}

// ---- evaluate reads unit text against the registry's own catalog

#[wasm_bindgen_test]
fn evaluate_reads_units_against_the_registrys_own_catalog() {
    let custom = UnitRegistry::from_content("[Units]\nkm = 7.0, L, [km]\nfurlong = 201.168, L, [fur, furlong]\n").unwrap();
    assert_eq!(custom.get_unit_scale("km").unwrap(), 7.0);
    // before, evaluate read the embedded catalog: 1 km was 1000 m here and `fur` was an unknown unit
    assert_eq!(custom.evaluate("1 km => m").unwrap().trim(), "7.0 m");
    assert_eq!(custom.evaluate("5 fur => m").unwrap().trim(), "1005.84 m");
}

#[wasm_bindgen_test]
fn functions_and_calculus_run_under_the_registrys_catalog_too() {
    let custom = UnitRegistry::from_content("[Units]\nfurlong = 201.168, L, [fur, furlong]\n").unwrap();
    let distance = PhyFunction::new(&custom, "distance", "distance(n) = n * 1 fur").unwrap();
    close(distance.call(args(&["3"])).unwrap().value(), 3.0);
    assert_eq!(custom.evaluate("distance(3) => m").unwrap().trim(), "603.504 m");
    assert!(custom.deriv("x * 2", "x").unwrap().contains('2'));
}

#[wasm_bindgen_test]
fn two_registries_in_turn_each_answer_with_their_own_meaning() {
    let custom = UnitRegistry::from_content("[Units]\nkm = 7.0, L, [km]\n").unwrap();
    let plain = UnitRegistry::new();
    for _ in 0..3 {
        assert_eq!(custom.evaluate("1 km => m").unwrap().trim(), "7.0 m");
        assert_eq!(plain.evaluate("1 km => m").unwrap().trim(), "1000.0 m");
    }
}

#[wasm_bindgen_test]
fn an_error_while_evaluating_leaves_no_catalog_behind() {
    let custom = UnitRegistry::from_content("[Units]\nkm = 7.0, L, [km]\n").unwrap();
    assert!(custom.evaluate("1 +").is_err());
    assert!(custom.evaluate("1 m + 1 s").is_err());
    // the failed calls did not leave the custom catalog in place for the next registry
    assert_eq!(UnitRegistry::new().evaluate("1 km => m").unwrap().trim(), "1000.0 m");
}
