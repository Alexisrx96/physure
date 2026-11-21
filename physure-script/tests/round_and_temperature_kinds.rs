//! `round` overflow and degrees of freedom, and a temperature point against a temperature interval (spec
//! `2026-11-20-round-and-temperature-kinds-design.md`). Every row of its section 1: the refused ones with the words
//! of the error, the valid neighbours with their values.

use physure_script::interpreter::eval_phs;
use physure_script::PhsValue;

fn refused(source: &str) -> String {
    match eval_phs(source) {
        Ok(values) => panic!("{source:?} gave {values:?}, expected an error"),
        Err(error) => error.to_string(),
    }
}

fn last_value(source: &str) -> PhsValue {
    eval_phs(source).unwrap_or_else(|e| panic!("{source:?}: {e}")).pop().unwrap()
}

fn last(source: &str) -> String {
    last_value(source).to_string()
}

/// The mean and the unit of the quantity `source` ends with.
fn mean_and_unit(source: &str) -> (f64, String) {
    match last_value(source) {
        PhsValue::Quantity(q) => (q.value.mean(), q.unit.__repr__()),
        other => panic!("{source:?} gave {other:?}, expected a quantity"),
    }
}

const POINT_AND_INTERVAL: &str = "cannot compare a temperature point (`20.0 degC`) with a temperature interval (`5.0 delta_degC`)";

#[test]
fn round_to_more_decimals_than_a_double_holds_is_the_value_itself() {
    assert_eq!(mean_and_unit("round(1.5 m, 400)"), (1.5, "m".to_string()));
    assert_eq!(mean_and_unit("round(1e300 m, 10)"), (1e300, "m".to_string()));
    assert_eq!(mean_and_unit("round(-1e300 m, 10)"), (-1e300, "m".to_string()));
    // past 2^53 every double is a whole number: nothing to round, and the value is returned bit for bit
    assert_eq!(mean_and_unit("round(9007199254740993.0 m, 3)").0.to_bits(), 9007199254740992.0f64.to_bits());
    assert_eq!(mean_and_unit("round(123456789.123456789 m, 9)"), (123456789.123456789, "m".to_string()));
    assert_eq!(last("round(1.5, 400)"), "1.5");
    assert_eq!(last("round(1e300, 10)"), last("1e300"));
}

#[test]
fn round_to_a_place_above_every_digit_is_zero() {
    assert_eq!(mean_and_unit("round(123456.789 m, -400)"), (0.0, "m".to_string()));
    assert_eq!(mean_and_unit("round(-123456.789 m, -400)").0, 0.0);
    assert_eq!(last("round(123456.789, -400)"), "0.0");
    assert_eq!(last("round(123456.789, -2147483648)"), "0.0");
    assert_eq!(last("round(1.5, 2147483647)"), "1.5");
    // a place the double can still hold, read by division: 123500, not 123500.00000000001
    assert_eq!(mean_and_unit("round(123456.789 m, -2)"), (123500.0, "m".to_string()));
    assert_eq!(last("round(1234.5, -1)"), "1230.0");
    assert_eq!(last("round(1235, -1)"), "1240.0");
    // a negative place the value has no digit at leaves it bit for bit (dividing and multiplying back moved it)
    assert_eq!(mean_and_unit("round(1.2345e23 m, -1)").0.to_bits(), 1.2345e23f64.to_bits());
    assert_eq!(mean_and_unit("round(1.7e308 m, -15)").0.to_bits(), 1.7e308f64.to_bits());
    assert_eq!(mean_and_unit("round(5e300 m, -301)").0, 1e301);
    assert_eq!(mean_and_unit("round(1e300 m, -301)").0, 0.0);
}

#[test]
fn round_never_answers_nan_or_infinity_for_a_finite_value() {
    for source in [
        "round(1.5 m, 400)",
        "round(123456.789 m, -400)",
        "round(1e300 m, 10)",
        "round(1.7e308 m, -308)",
        "round(-1.7e308 m, -308)",
        "round(1e-300 m, 400)",
        "round(1.7e308, -308)",
    ] {
        let mean = match last_value(source) {
            PhsValue::Quantity(q) => q.value.mean(),
            PhsValue::Number(n) => n,
            other => panic!("{source}: {other:?}"),
        };
        assert!(mean.is_finite(), "{source} gave {mean}");
    }
}

#[test]
fn round_of_ordinary_values_is_unchanged() {
    assert_eq!(mean_and_unit("round(1.2345 m, 2)"), (1.23, "m".to_string()));
    assert_eq!(last("round(1.2345 m, 2)"), last("1.23 m"));
    assert_eq!(last("round(1.2345, 2)"), "1.23");
    assert_eq!(last("round(2.5)"), "3.0");
    assert_eq!(last("round(-2.5)"), "-3.0");
    assert_eq!(last("round(1.5 m)"), last("2 m"));
    assert_eq!(last("round(0.125, 2)"), "0.13");
}

#[test]
fn round_keeps_the_degrees_of_freedom_and_the_uncertainty() {
    assert_eq!(last("dof(from_samples([1 V, 2 V, 4 V]))"), "2.0");
    assert_eq!(last("dof(round(from_samples([1 V, 2 V, 4 V]), 1))"), "2.0");
    assert_eq!(last("dof(round(from_samples([1 V, 2 V, 4 V])))"), "2.0");
    let (std_dev, rounded_std_dev) = match (
        last_value("from_samples([1 V, 2 V, 4 V])"),
        last_value("round(from_samples([1 V, 2 V, 4 V]), 1)"),
    ) {
        (PhsValue::Quantity(a), PhsValue::Quantity(b)) => (a.value.std_dev(), b.value.std_dev()),
        other => panic!("{other:?}"),
    };
    assert_eq!(std_dev, rounded_std_dev);
    assert_eq!(mean_and_unit("round(from_samples([1 V, 2 V, 4 V]), 1)"), (2.3, "V".to_string()));
    // the rounded value is the same measurement: it keeps its source, so the difference has no uncertainty left
    match last_value("x = from_samples([1 V, 2 V, 4 V])\nx - round(x, 1)") {
        PhsValue::Quantity(difference) => assert_eq!(difference.value.std_dev(), 0.0, "{difference}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_temperature_point_and_a_temperature_interval_have_no_order() {
    for op in ["<", ">", "<=", ">=", "==", "!="] {
        for source in [format!("5 delta_degC {op} 20 degC"), format!("20 degC {op} 5 delta_degC")] {
            let message = refused(&source);
            assert!(message.contains(POINT_AND_INTERVAL), "{source}: {message}");
        }
    }
    let message = refused("5 delta_degC == 5 degC");
    assert!(message.contains("a temperature point (`5.0 degC`) with a temperature interval (`5.0 delta_degC`)"), "{message}");
    assert!(refused("5 delta_degC != 5 degC").contains("with a temperature interval"));
    for source in ["5 delta_degF < 20 degC", "5 degC_delta < 20 degF", "5 delta_fahrenheit < 20 degF", "2 delta_celsius >= 1 degC"] {
        assert!(refused(source).contains("with a temperature interval"), "{source}: {}", refused(source));
    }
    let message = refused("5 delta_degC < 20 degC");
    assert!(message.contains("compare a difference of two points with the interval (`T2 - T1 > 5 delta_degC`)"), "{message}");
    assert!(message.contains("`x => K` does not make an interval a point, an interval in K is still an interval"), "{message}");
    assert!(!message.contains("convert the interval to a point"), "{message}");
    let message = refused("max(5 delta_degC, 20 degC)");
    assert!(message.contains("; compare a difference of two points with the interval"), "{message}");
}

#[test]
fn points_with_points_and_intervals_with_intervals_still_compare() {
    assert_eq!(last("20 degC > 10 K"), "True");
    assert_eq!(last("10 K < 20 degC"), "True");
    assert_eq!(last("20 degC == 68 degF"), "True");
    assert_eq!(last("5 delta_degC < 6 delta_degC"), "True");
    assert_eq!(last("9 delta_degF == 5 delta_degC"), "True");
    assert_eq!(last("5 delta_degC < 6 K"), "True");
    assert_eq!(last("(30 degC - 20 degC) > 5 delta_degC"), "True");
    // an interval in K is still an interval (temperature kind review): `(5 delta_degC => K) < 20 degC` is refused
    assert!(refused("(5 delta_degC => K) < 20 degC").contains("with a temperature interval"));
    assert_eq!(last("20 degC + 5 delta_degC"), last("25 degC"));
    assert_eq!(last("20 degC - 10 degC"), last("10 K"));
}

#[test]
fn an_extremum_of_a_point_and_an_interval_is_refused() {
    let message = refused("max(5 delta_degC, 20 degC)");
    assert!(message.starts_with("max of a temperature point and a temperature interval"), "{message}");
    assert!(message.contains("`5 delta_degC` is a temperature interval and `20 degC` a temperature point"), "{message}");
    let message = refused("min(20 degC, 5 delta_degC)");
    assert!(message.starts_with("min of a temperature point and a temperature interval"), "{message}");
    assert!(message.contains("`20 degC` is a temperature point and `5 delta_degC` a temperature interval"), "{message}");
    assert!(refused("max(vector(5 delta_degC, 20 degC))").contains("one kind of temperature"));
    assert_eq!(last("max(5 degC, 3 degC)"), last("5 degC"));
    assert_eq!(last("min(5 delta_degC, 3 delta_degC)"), last("3 delta_degC"));
    assert_eq!(last("max(20 degC, 300 K)"), last("300 K"));
}

#[test]
fn a_vector_holds_one_kind_of_temperature() {
    for source in ["vector(5 delta_degC, 20 degC)", "[20 degC, 5 delta_degC]", "[20 degC, 300 K, 5 delta_degC]"] {
        let message = refused(source);
        assert!(message.starts_with("a vector holds one dimension and one kind of temperature"), "{source}: {message}");
    }
    let message = refused("vector(5 delta_degC, 20 degC)");
    assert!(message.contains("`5 delta_degC` is a temperature interval and `20 degC` a temperature point"), "{message}");
    assert!(eval_phs("vector(20 degC, 300 K)").is_ok());
    assert!(eval_phs("vector(5 delta_degC, 3 K)").is_ok());
    assert!(eval_phs("vector(5 delta_degC, 3 delta_degF)").is_ok());
    // a dimension mismatch keeps its own words
    assert!(refused("vector(1 m, 2 s)").starts_with("a vector holds one dimension: "));
}

/// `K` stands for a point or an interval, so "one kind of temperature" is not transitive through it: every element is
/// set against the first point and the first interval of the list, not only against the first element.
#[test]
fn a_kelvin_does_not_join_a_point_and_an_interval() {
    for source in [
        "max(1 K, 25 delta_degC, 20 degC)",
        "max(25 delta_degC, 1 K, 20 degC)",
        "max(25 delta_degC, 20 degC, 1 K)",
        "min(1 K, 25 delta_degC, 20 degC)",
        "min(20 degC, 1 K, 25 delta_degC)",
    ] {
        let message = refused(source);
        assert!(message.contains("of a temperature point and a temperature interval"), "{source}: {message}");
    }
    let message = refused("max(1 K, 25 delta_degC, 20 degC)");
    assert!(message.contains("`25 delta_degC` is a temperature interval and `20 degC` a temperature point"), "{message}");
    for source in [
        "[1 K, 25 delta_degC, 20 degC]",
        "vector(300 K, 5 delta_degC, 20 degC)",
        "vector(5 delta_degC, 300 K, 20 degC)",
        "vector(5 delta_degC, 20 degC, 300 K)",
        "[20 degC, 1 K, 5 delta_degC]",
        // the vector is refused where it is made, before max sees it
        "max(vector(1 K, 25 delta_degC, 20 degC))",
    ] {
        let message = refused(source);
        assert!(message.starts_with("a vector holds one dimension and one kind of temperature"), "{source}: {message}");
    }
    let message = refused("vector(300 K, 5 delta_degC, 20 degC)");
    assert!(message.contains("`5 delta_degC` is a temperature interval and `20 degC` a temperature point"), "{message}");
    // a K with points only, or with intervals only, is fine
    assert_eq!(last("max(1 K, 2 degC)"), last("2 degC"));
    assert_eq!(last("max(1 K, 2 delta_degC)"), last("2 delta_degC"));
    assert!(eval_phs("vector(1 K, 20 degC, 30 degC)").is_ok());
    assert!(eval_phs("vector(1 K, 5 delta_degC)").is_ok());
    assert!(eval_phs("vector(5 delta_degC, 1 K, 3 delta_degF)").is_ok());
    // a dimension mismatch is still said as one
    assert!(refused("vector(1 K, 20 degC, 1 m)").starts_with("a vector holds one dimension: "));
}

#[test]
fn linspace_needs_endpoints_of_one_kind_of_temperature() {
    let message = refused("use * from array\nlinspace(5 delta_degC, 20 degC, 3)");
    assert!(message.contains("one kind of temperature"), "{message}");
    assert!(eval_phs("use * from array\nlinspace(10 degC, 20 degC, 3)").is_ok());
}

/// The items of the vector `source` ends with, as they print.
fn items(source: &str) -> Vec<String> {
    match last_value(source) {
        PhsValue::Vector(items) => items.iter().map(|item| item.to_string()).collect(),
        other => panic!("{source:?} gave {other:?}, expected a vector"),
    }
}

/// A range is iterated in the unit of its start: the end is converted to it, as `linspace` converts its endpoints.
/// It read the end's raw number in the start's unit: `1 km .. 3000 m` ran to 2999 km.
#[test]
fn a_range_is_iterated_in_the_unit_of_its_start() {
    assert_eq!(items("for t in 1 km .. 3000 m { t }"), vec![last("1 km"), last("2 km")]);
    assert_eq!(items("for t in 1 m .. 300 cm { t }"), vec![last("1 m"), last("2 m")]);
    let kelvins = items("for t in 0 K .. 2 degC { t }");
    assert_eq!(kelvins.len(), 275, "0 K .. 275.15 K");
    assert_eq!((kelvins[0].as_str(), kelvins[274].as_str()), (last("0 K").as_str(), last("274 K").as_str()));
    // the range itself keeps its ends as written (`0 m .. 10 m => km` shows where the conversion went)
    assert_eq!(last("1 km .. 3000 m"), "1.0 km .. 3000.0 m");
    // the end stays exclusive, and a plain-number range is unchanged
    assert_eq!(items("for t in 1 m .. 5 m { t }").len(), 4);
    assert_eq!(items("for t in 0 degC .. 3 degC { t }"), vec![last("0 degC"), last("1 degC"), last("2 degC")]);
    assert_eq!(items("for t in 0 K .. 3 K { t }"), vec![last("0 K"), last("1 K"), last("2 K")]);
    assert_eq!(items("for t in 1 .. 4 { t }"), vec!["1.0", "2.0", "3.0"]);
    assert_eq!(items("for t in 0 .. 3 m { t }").len(), 3);
}

#[test]
fn a_range_of_a_point_and_an_interval_is_refused() {
    for source in ["5 delta_degC .. 20 degC", "r = 5 delta_degC .. 20 degC\nr", "for t in 5 delta_degC .. 20 degC { t }"] {
        let message = refused(source);
        assert!(message.starts_with("a range of a temperature point and a temperature interval"), "{source}: {message}");
        assert!(message.contains("`5 delta_degC` is a temperature interval and `20 degC` a temperature point"), "{message}");
    }
    // different dimensions are refused where the range is made, as before
    assert!(refused("1 m .. 5 s").contains("range"));
}
