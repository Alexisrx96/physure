//! Degrees of freedom in PHS: the `dof` clause of a literal, `dof`, `coverage_factor`, `expanded` and
//! `from_samples`, ending with the example of JCGM 100:2008 Annex H.1 written as a program.

use physure_core::math::stats::student_t_two_sided;
use physure_core::{Quantity, RationalUnit};
use physure_script::interpreter::eval_phs;
use physure_script::value::PhsValue;

fn last(program: &str) -> PhsValue {
    eval_phs(program)
        .unwrap_or_else(|e| panic!("{program:?} failed: {e}"))
        .pop()
        .expect("a value")
}

fn number(program: &str) -> f64 {
    match last(program) {
        PhsValue::Number(n) => n,
        PhsValue::Quantity(q) if q.unit == RationalUnit::dimensionless() => q.value.mean(),
        other => panic!("{program:?} gave {other:?}, not a number"),
    }
}

fn quantity(program: &str) -> Quantity {
    match last(program) {
        PhsValue::Quantity(q) => q,
        other => panic!("{program:?} gave {other:?}, not a quantity"),
    }
}

fn refused(program: &str) -> String {
    match eval_phs(program) {
        Ok(values) => panic!("{program:?} should be refused, it gave {:?}", values.last().map(|v| v.to_string())),
        Err(e) => e.to_string(),
    }
}

fn close(a: f64, b: f64) -> bool {
    ((a - b) / b).abs() < 1e-12
}

#[test]
fn a_literal_declares_the_degrees_of_freedom_of_its_measurement() {
    assert!(close(number("x = 10.003 +/- 0.002 dof 9 V\ndof(x)"), 9.0));
    assert!(close(number("x = 10.003 ± 0.002 ν 9 V\ndof(x)"), 9.0));
    assert!(close(number("x = 10.003 V +/- 0.002 dof 16.7\ndof(x)"), 16.7));
    assert_eq!(number("x = 10.003 +/- 0.002 dof inf V\ndof(x)"), f64::INFINITY);
}

#[test]
fn a_measurement_that_declares_none_has_infinitely_many_and_so_has_an_exact_number() {
    assert_eq!(number("x = 10.003 +/- 0.002 V\ndof(x)"), f64::INFINITY);
    assert_eq!(number("dof(5 m)"), f64::INFINITY);
    assert_eq!(number("dof(3)"), f64::INFINITY);
}

#[test]
fn the_literal_keeps_its_value_unit_and_uncertainty() {
    let x = quantity("10.003 +/- 0.002 dof 9 V");
    assert_eq!((x.value.mean(), x.value.std_dev()), (10.003, 0.002));
    assert_eq!(x.unit, Quantity::new(1.0, "V").unwrap().unit);
}

#[test]
fn the_degrees_of_freedom_of_a_result_come_from_its_sources() {
    let program = "a = 1 +/- 3 dof 4 m\nb = 2 +/- 4 dof 9 m\ndof(a + b)";
    assert!(close(number(program), 625.0 / (81.0 / 4.0 + 256.0 / 9.0)));
    // a copy of a measurement is the same measurement
    assert!(close(number("a = 1 +/- 3 dof 4 m\ndof(a + a)"), 4.0));
    // and a conversion keeps them
    assert!(close(number("a = 1 +/- 3 dof 4 m\nb = a => mm\ndof(b)"), 4.0));
}

#[test]
fn a_source_with_no_declared_dof_adds_to_the_uncertainty_but_not_to_the_denominator() {
    assert!(close(number("a = 1 +/- 3 dof 4 m\nb = 2 +/- 4 m\ndof(a + b)"), 625.0 / (81.0 / 4.0)));
}

#[test]
fn the_coverage_factor_is_the_student_t_factor_at_the_truncated_dof() {
    let k = student_t_two_sided(0.95, 16.0).unwrap();
    assert!(close(number("x = 10.0 +/- 0.5 dof 16 V\ncoverage_factor(x, 0.95)"), k));
    assert!(close(number("x = 10.0 +/- 0.5 dof 16.9 V\ncoverage_factor(x, 0.95)"), k), "16.9 is read as 16");
    assert!(close(number("x = 10.0 +/- 0.5 V\ncoverage_factor(x, 0.95)"), 1.959_963_984_540_054));
    assert!(close(number("coverage_factor(7, 0.99)"), 2.575_829_303_548_900_6));
}

#[test]
fn the_expanded_uncertainty_is_a_quantity_in_the_unit_of_the_measurement() {
    let k = student_t_two_sided(0.95, 16.0).unwrap();
    let u = quantity("x = 10.0 +/- 0.5 dof 16 V\nexpanded(x, 0.95)");
    assert!(close(u.value.mean(), k * 0.5));
    assert_eq!(u.value.std_dev(), 0.0, "it is a bound, not a measurement");
    assert_eq!(u.unit, Quantity::new(1.0, "V").unwrap().unit);
    // in the unit it was given, not in base units
    let in_mm = quantity("x = 10.0 +/- 0.5 dof 16 mm\nexpanded(x, 0.95)");
    assert!(close(in_mm.value.mean(), k * 0.5));
    assert_eq!(in_mm.unit, Quantity::new(1.0, "mm").unwrap().unit);
    assert_eq!(number("expanded(7, 0.95)"), 0.0);
}

#[test]
fn a_type_a_evaluation_makes_a_measurement_from_observations() {
    // deviations from the mean 10.1: -0.1, 0.1, -0.2, 0, 0.2; s² = 0.10 / 4, u = s / sqrt(5)
    let program = "v = from_samples([10.0, 10.2, 9.9, 10.1, 10.3] V)\n";
    let v = quantity(&format!("{program}v"));
    assert!(close(v.value.mean(), 10.1));
    assert!(close(v.value.std_dev(), (0.025f64 / 5.0).sqrt()));
    assert_eq!(v.unit, Quantity::new(1.0, "V").unwrap().unit);
    assert!(close(number(&format!("{program}dof(v)")), 4.0));
    // observations in another unit of the same dimension are converted to the unit of the first
    let mixed = quantity("from_samples([1000 mm, 1.2 m, 1.1 m])");
    assert!(close(mixed.value.mean(), 1000.0 * 1.1));
    assert_eq!(mixed.unit, Quantity::new(1.0, "mm").unwrap().unit);
    // plain numbers are dimensionless
    assert!(close(number("dof(from_samples([1.0, 2.0, 3.0]))"), 2.0));
}

#[test]
fn what_cannot_be_evaluated_is_refused_with_the_reason() {
    assert!(refused("x = 5 +/- (0.5, 0.4) dof 4 m").contains("asymmetric"));
    assert!(refused("dof(\"text\")").contains("expects a quantity"));
    assert!(refused("x = 1 +/- 0.1 m\ncoverage_factor(x)").contains("2 arguments"));
    assert!(refused("x = 1 +/- 0.1 m\ncoverage_factor(x, 1.5)").contains("coverage probability"));
    assert!(refused("x = 1 +/- 0.1 m\ncoverage_factor(x, 0)").contains("coverage probability"));
    assert!(refused("x = 1 +/- 0.1 m\nexpanded(x, 2 m)").contains("probability"));
    assert!(refused("from_samples([1.0 V])").contains("at least two"));
    // observations of different dimensions: the vector that would hold them is refused before `from_samples` sees it
    assert!(refused("from_samples([1.0 V, 2.0 m])").contains("a vector holds one dimension"));
    assert!(refused("from_samples([1.0 +/- 0.1 V, 2.0 V])").contains("exact"));
    assert!(refused("from_samples(3)").contains("vector"));
    // a measurement with no uncertainty has nothing to give degrees of freedom to
    assert!(refused("x = 5 +/- 0 dof 4 m").contains("exact"));
}

/// JCGM 100:2008, Annex H.1, the calibration of an end gauge, as a program: the six contributions to the
/// standard uncertainty of the length that are not zero at the nominal point, with their degrees of freedom.
#[test]
fn the_end_gauge_of_the_guide_in_phs() {
    let program = "\
l_s = 50000623 +/- 25 dof 18 nm
d0 = 215 +/- 5.8 dof 24 nm
d1 = 0 +/- 3.9 dof 5 nm
d2 = 0 +/- 6.7 dof 8 nm
d_theta = 0 +/- 16.599 dof 2 nm
d_alpha = 0 +/- 2.88679 dof 50 nm
l = l_s + d0 + d1 + d2 + d_theta + d_alpha
";
    let length = quantity(&format!("{program}l"));
    assert_eq!(length.value.mean(), 50_000_838.0);
    assert!(close(length.value.std_dev(), 31.663_865_170_002_54));
    assert!(close(number(&format!("{program}dof(l)")), 16.751_895_334_905_974));
    assert!(close(number(&format!("{program}coverage_factor(l, 0.95)")), 2.119_905_299_221_254_6));
    assert!(close(quantity(&format!("{program}expanded(l, 0.95)")).value.mean(), 67.124_395_567_715_7));
    assert!(close(quantity(&format!("{program}expanded(l, 0.99)")).value.mean(), 92.483_235_483_489_64));
}

/// The worked example the tutorial ships (`docs/tutorials/dof_example.phs`) runs, and says what the primer says.
#[test]
fn the_tutorial_example_runs_and_gives_the_numbers_the_primer_quotes() {
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/tutorials/dof_example.phs")).unwrap();
    let values = eval_phs(&source).unwrap_or_else(|e| panic!("dof_example.phs failed: {e}"));
    let [.., PhsValue::Number(nu_eff), PhsValue::Number(k95), PhsValue::Quantity(u95), PhsValue::Quantity(u99)] = values.as_slice() else {
        panic!("the last four values are nu_eff, k95, U95 and U99: {values:?}");
    };
    assert_eq!(format!("{nu_eff:.2}"), "16.75");
    assert_eq!(format!("{k95:.2}"), "2.12");
    assert_eq!(format!("{:.1}", u95.value.mean()), "67.1");
    assert_eq!(format!("{:.1}", u99.value.mean()), "92.5");
}
