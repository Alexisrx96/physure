//! The golden table of printed numbers: what `format_float`, `gum_round`, `Quantity`'s `Display` (with and without
//! `@precision`), `base_display` and `format_fraction` write for values across the display boundaries (the
//! scientific thresholds 1e-4 and 1e16, the 15-digit rounding, the GUM 1-or-2-figure rule, negative zero, the
//! infinities, subnormals, `f64::MAX`, folded anonymous units).
//!
//! The expected strings were generated once by the implementation before the display rules became catalog settings
//! (Phase 4b, Task 2) and reviewed: the compiled defaults of `display_significant_digits`, `display_scientific_below`,
//! `display_scientific_from` and `uncertainty_digits` must reproduce every one of them. `format_float(NaN)` is `NaN` (it
//! was `NaN.0`, audit T20). Some rows record quirks on purpose, so that a change to them is a decision: `gum_round`
//! writes `-0.00` for a negative zero mean where `format_float` writes `0.0`, writes a large rounded mean without a
//! `.0`, and switches to the unrounded scientific form one ulp below an uncertainty of 1e-4.

use physure_core::quantity::{format_float, format_fraction, gum_round, Quantity};

fn up(x: f64) -> f64 {
    f64::from_bits(x.to_bits() + 1)
}
fn down(x: f64) -> f64 {
    f64::from_bits(x.to_bits() - 1)
}

fn rows() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let floats: Vec<(&str, f64)> = vec![
        ("1e-4", 1e-4),
        ("down(1e-4)", down(1e-4)),
        ("up(1e-4)", up(1e-4)),
        ("9.99e-5", 9.99e-5),
        ("1.0001e-4", 1.0001e-4),
        ("-1e-4", -1e-4),
        ("0.00012345678901234567", 0.00012345678901234567),
        ("0.001", 0.001),
        ("0.0012345678901234567", 0.0012345678901234567),
        ("1e16", 1e16),
        ("9999999999999998.0", 9999999999999998.0),
        ("up(1e16)", up(1e16)),
        ("-1e16", -1e16),
        ("1e15", 1e15),
        ("999999999999999.0", 999999999999999.0),
        ("1e15 + 1", 1e15 + 1.0),
        ("1e15 + 0.5", 1e15 + 0.5),
        ("1234567890123456.0", 1234567890123456.0),
        ("123456789012345.0", 123456789012345.0),
        ("123456789012345.67", 123456789012345.67),
        ("2^53 + 2", 9007199254740994.0),
        ("0.1 + 0.2", 0.1 + 0.2),
        ("89.99999999999999", 89.99999999999999),
        ("25 * 3.6 / 1", 25.0 / 3.6 * 3.6 * 3.6 / 3.6),
        ("1/3", 1.0 / 3.0),
        ("2/3", 2.0 / 3.0),
        ("100/3", 100.0 / 3.0),
        ("1.1 * 1.1", 1.1 * 1.1),
        ("0.1 * 3", 0.1 * 3.0),
        ("0.7", 0.7),
        ("2.675", 2.675),
        ("1.005", 1.005),
        ("123456.789", 123456.789),
        ("1.0", 1.0),
        ("42.0", 42.0),
        ("-1.0", -1.0),
        ("-123.456", -123.456),
        ("0.0", 0.0),
        ("-0.0", -0.0),
        ("inf", f64::INFINITY),
        ("-inf", f64::NEG_INFINITY),
        ("NaN", f64::NAN),
        ("smallest subnormal", f64::from_bits(1)),
        ("1e-310 subnormal", 1e-310),
        ("min positive normal", f64::MIN_POSITIVE),
        ("f64::MAX", f64::MAX),
        ("f64::MIN", f64::MIN),
        ("1e300", 1e300),
        ("1e-300", 1e-300),
        ("-1e-5", -1e-5),
        ("1.2345678901234567e-5", 1.2345678901234567e-5),
        ("6.02214076e23", 6.02214076e23),
        ("1e-5", 1e-5),
    ];
    for (label, x) in floats {
        out.push((format!("float {label}"), format_float(x)));
    }
    let gums: Vec<(&str, f64, f64, Option<u32>)> = vec![
        ("10 ± 0.95", 10.0, 0.95, None),
        ("10 ± 0.0995", 10.0, 0.0995, None),
        ("10 ± 0.15", 10.0, 0.15, None),
        ("10 ± 0.981", 10.0, 0.981, None),
        ("10 ± 1.9", 10.0, 1.9, None),
        ("10 ± 2.0", 10.0, 2.0, None),
        ("10 ± 9.5", 10.0, 9.5, None),
        ("123.456 ± 0.95", 123.456, 0.95, None),
        ("1.2345 ± 0.0995", 1.2345, 0.0995, None),
        ("0.001 ± 5", 0.001, 5.0, None),
        ("0.3 ± 400", 0.3, 400.0, None),
        ("-12.345 ± 0.23", -12.345, 0.23, None),
        ("-0.5 ± 0.05", -0.5, 0.05, None),
        ("0.00012 ± 1e-5", 0.00012, 1e-5, None),
        ("1 ± 1e-4", 1.0, 1e-4, None),
        ("1 ± down(1e-4)", 1.0, down(1e-4), None),
        ("1e-5 ± 1e-3", 1e-5, 1e-3, None),
        ("1000 ± 1e16", 1000.0, 1e16, None),
        ("1e17 ± 1e16", 1e17, 1e16, None),
        ("1e15 ± 2e14", 1e15, 2e14, None),
        ("9999999999999998 ± 3", 9999999999999998.0, 3.0, None),
        ("625 ± 40.0195264839553", 625.0, 40.0195264839553, None),
        ("625 ± 40", 625.0, 40.0, None),
        ("12345 ± 678", 12345.0, 678.0, None),
        ("19.62 ± 0.981 @1", 19.62, 0.981, Some(1)),
        ("19.62 ± 0.981 @20", 19.62, 0.981, Some(20)),
        ("625 ± 40.02 @1", 625.0, 40.0195264839553, Some(1)),
        ("625 ± 40.02 @3", 625.0, 40.0195264839553, Some(3)),
        ("2.675 ± 0.01", 2.675, 0.01, None),
        ("2.675 ± 0.005", 2.675, 0.005, None),
        ("1.005 ± 0.01", 1.005, 0.01, None),
        ("0.125 ± 0.01", 0.125, 0.01, None),
        ("2.5 ± 1", 2.5, 1.0, None),
        ("5 ± 0", 5.0, 0.0, None),
        ("5 ± inf", 5.0, f64::INFINITY, None),
        ("inf ± 1", f64::INFINITY, 1.0, None),
        ("-0.0 ± 0.1", -0.0, 0.1, None),
        ("0.1+0.2 ± 0.01", 0.1 + 0.2, 0.01, None),
    ];
    for (label, mean, std, prec) in gums {
        let (m, s) = gum_round(mean, std, prec);
        out.push((format!("gum {label}"), format!("{m} | {s}")));
    }
    let q = |mean: f64, u: f64, unit: &str| Quantity::with_uncertainty(mean, u, unit).unwrap();
    let e = |mean: f64, unit: &str| Quantity::new(mean, unit).unwrap();
    out.push(("qty 625 ± 40 J".into(), q(625.0, 40.0, "J").to_string()));
    out.push(("qty 0.00012 ± 1e-5 m".into(), q(0.00012, 1e-5, "m").to_string()));
    out.push(("qty 9.81 ± 0.05 m/s^2".into(), q(9.81, 0.05, "m/s^2").to_string()));
    out.push(("qty 3.14159265 m @1".into(), e(3.14159265, "m").to_string_with_precision(Some(1))));
    out.push(("qty 3.14159265 m @20".into(), e(3.14159265, "m").to_string_with_precision(Some(20))));
    out.push(("qty 2.675 m @2".into(), e(2.675, "m").to_string_with_precision(Some(2))));
    out.push(("qty 19.62 ± 0.981 N @1".into(), q(19.62, 0.981, "N").to_string_with_precision(Some(1))));
    out.push(("qty 19.62 ± 0.981 N @20".into(), q(19.62, 0.981, "N").to_string_with_precision(Some(20))));
    out.push(("qty 0.1+0.2 m".into(), e(0.1 + 0.2, "m").to_string()));
    out.push(("qty 1e-5 dimensionless".into(), e(1e-5, "").to_string()));
    out.push(("qty 2 m * 3 km (folded)".into(), e(2.0, "m").mul(&e(3.0, "km")).unwrap().to_string()));
    out.push(("qty 2 ± 0.1 m * 3 km (folded)".into(), q(2.0, 0.1, "m").mul(&e(3.0, "km")).unwrap().to_string()));
    out.push(("qty 25 m/s => km/h".into(), e(25.0, "m/s").to("km/h").unwrap().to_string()));
    out.push(("base 2 ± 0.1 kohm".into(), q(2.0, 0.1, "kohm").base_display()));
    out.push(("base 0 degC".into(), e(0.0, "degC").base_display()));
    out.push(("base 1.5e-5 km".into(), e(1.5e-5, "km").base_display()));
    out.push(("base 625 ± 40.0195264839553 J".into(), q(625.0, 40.0195264839553, "J").base_display()));
    let fracs: Vec<(&str, f64, bool)> = vec![
        ("1.5", 1.5, false),
        ("1.5 mixed", 1.5, true),
        ("-2.5 mixed", -2.5, true),
        ("0.1 + 0.2", 0.1 + 0.2, false),
        ("89.99999999999999", 89.99999999999999, false),
        ("1/3", 1.0 / 3.0, false),
        ("1/7 mixed", 1.0 / 7.0 + 3.0, true),
        ("1/10001", 1.0 / 10001.0, false),
        ("pi", std::f64::consts::PI, false),
        ("1e-30", 1e-30, false),
        ("0.0", 0.0, false),
        ("inf", f64::INFINITY, false),
        ("1e16", 1e16, false),
    ];
    for (label, x, mixed) in fracs {
        out.push((format!("fraction {label}"), format_fraction(x, mixed).unwrap_or_else(|| "None".into())));
    }
    out
}

/// Every row of [`GOLDEN`] that the implementation in force does not reproduce, as `label: got, want`.
pub fn mismatches() -> Vec<String> {
    let got = rows();
    assert_eq!(got.len(), GOLDEN.len(), "the rows and the golden table have the same length");
    got.iter()
        .zip(GOLDEN)
        .filter(|((label, text), (want_label, want))| label != want_label || text != want)
        .map(|((label, text), (_, want))| format!("{label}: got {text:?}, want {want:?}"))
        .collect()
}

#[test]
fn the_compiled_display_rules_print_the_golden_table() {
    let wrong = mismatches();
    assert!(wrong.is_empty(), "{} rows differ:
{}", wrong.len(), wrong.join("
"));
}

/// The expected text of each row of [`rows`], in order.
const GOLDEN: &[(&str, &str)] = &[
        ("float 1e-4", "0.0001"),
        ("float down(1e-4)", "9.999999999999999e-5"),
        ("float up(1e-4)", "0.0001"),
        ("float 9.99e-5", "9.99e-5"),
        ("float 1.0001e-4", "0.00010001"),
        ("float -1e-4", "-0.0001"),
        ("float 0.00012345678901234567", "0.00012345678901235"),
        ("float 0.001", "0.001"),
        ("float 0.0012345678901234567", "0.00123456789012346"),
        ("float 1e16", "1e16"),
        ("float 9999999999999998.0", "9999999999999998.0"),
        ("float up(1e16)", "1.0000000000000002e16"),
        ("float -1e16", "-1e16"),
        ("float 1e15", "1000000000000000.0"),
        ("float 999999999999999.0", "999999999999999.0"),
        ("float 1e15 + 1", "1000000000000001.0"),
        ("float 1e15 + 0.5", "1000000000000000.0"),
        ("float 1234567890123456.0", "1234567890123456.0"),
        ("float 123456789012345.0", "123456789012345.0"),
        ("float 123456789012345.67", "123456789012346.0"),
        ("float 2^53 + 2", "9007199254740994.0"),
        ("float 0.1 + 0.2", "0.3"),
        ("float 89.99999999999999", "90.0"),
        ("float 25 * 3.6 / 1", "25.0"),
        ("float 1/3", "0.333333333333333"),
        ("float 2/3", "0.666666666666667"),
        ("float 100/3", "33.3333333333333"),
        ("float 1.1 * 1.1", "1.21"),
        ("float 0.1 * 3", "0.3"),
        ("float 0.7", "0.7"),
        ("float 2.675", "2.675"),
        ("float 1.005", "1.005"),
        ("float 123456.789", "123456.789"),
        ("float 1.0", "1.0"),
        ("float 42.0", "42.0"),
        ("float -1.0", "-1.0"),
        ("float -123.456", "-123.456"),
        ("float 0.0", "0.0"),
        ("float -0.0", "0.0"),
        ("float inf", "inf"),
        ("float -inf", "-inf"),
        ("float NaN", "NaN"),
        ("float smallest subnormal", "5e-324"),
        ("float 1e-310 subnormal", "1e-310"),
        ("float min positive normal", "2.2250738585072014e-308"),
        ("float f64::MAX", "1.7976931348623157e308"),
        ("float f64::MIN", "-1.7976931348623157e308"),
        ("float 1e300", "1e300"),
        ("float 1e-300", "1e-300"),
        ("float -1e-5", "-1e-5"),
        ("float 1.2345678901234567e-5", "1.2345678901234568e-5"),
        ("float 6.02214076e23", "6.02214076e23"),
        ("float 1e-5", "1e-5"),
        ("gum 10 ± 0.95", "10.00 | 0.95"),
        ("gum 10 ± 0.0995", "10.00 | 0.10"),
        ("gum 10 ± 0.15", "10.0 | 0.2"),
        ("gum 10 ± 0.981", "10.00 | 0.98"),
        ("gum 10 ± 1.9", "10 | 2"),
        ("gum 10 ± 2.0", "10 | 2"),
        ("gum 10 ± 9.5", "10.0 | 9.5"),
        ("gum 123.456 ± 0.95", "123.46 | 0.95"),
        ("gum 1.2345 ± 0.0995", "1.23 | 0.10"),
        ("gum 0.001 ± 5", "0 | 5"),
        ("gum 0.3 ± 400", "0 | 400"),
        ("gum -12.345 ± 0.23", "-12.3 | 0.2"),
        ("gum -0.5 ± 0.05", "-0.50 | 0.05"),
        ("gum 0.00012 ± 1e-5", "0.00012 | 1e-5"),
        ("gum 1 ± 1e-4", "1.00000 | 0.00010"),
        ("gum 1 ± down(1e-4)", "1.0 | 9.999999999999999e-5"),
        ("gum 1e-5 ± 1e-3", "1e-5 | 0.001"),
        ("gum 1000 ± 1e16", "1000.0 | 1e16"),
        ("gum 1e17 ± 1e16", "1e17 | 1e16"),
        ("gum 1e15 ± 2e14", "1000000000000000 | 200000000000000"),
        ("gum 9999999999999998 ± 3", "9999999999999998 | 3"),
        ("gum 625 ± 40.0195264839553", "630 | 40"),
        ("gum 625 ± 40", "630 | 40"),
        ("gum 12345 ± 678", "12300 | 700"),
        ("gum 19.62 ± 0.981 @1", "20 | 1"),
        ("gum 19.62 ± 0.981 @20", "19.62000000000000099476 | 0.98099999999999998312"),
        ("gum 625 ± 40.02 @1", "630 | 40"),
        ("gum 625 ± 40.02 @3", "625.0 | 40.0"),
        ("gum 2.675 ± 0.01", "2.675 | 0.010"),
        ("gum 2.675 ± 0.005", "2.675 | 0.005"),
        ("gum 1.005 ± 0.01", "1.005 | 0.010"),
        ("gum 0.125 ± 0.01", "0.125 | 0.010"),
        ("gum 2.5 ± 1", "2.5 | 1.0"),
        ("gum 5 ± 0", "5.0 | 0.0"),
        ("gum 5 ± inf", "5.0 | inf"),
        ("gum inf ± 1", "inf | 1.0"),
        ("gum -0.0 ± 0.1", "-0.00 | 0.10"),
        ("gum 0.1+0.2 ± 0.01", "0.300 | 0.010"),
        ("qty 625 ± 40 J", "630 ± 40 J"),
        ("qty 0.00012 ± 1e-5 m", "0.00012 ± 1e-5 m"),
        ("qty 9.81 ± 0.05 m/s^2", "9.81 ± 0.05 m/s^2"),
        ("qty 3.14159265 m @1", "3.1 m"),
        ("qty 3.14159265 m @20", "3.14159265000000020862 m"),
        ("qty 2.675 m @2", "2.67 m"),
        ("qty 19.62 ± 0.981 N @1", "20 ± 1 N"),
        ("qty 19.62 ± 0.981 N @20", "19.62000000000000099476 ± 0.98099999999999998312 N"),
        ("qty 0.1+0.2 m", "0.3 m"),
        ("qty 1e-5 dimensionless", "1e-5"),
        ("qty 2 m * 3 km (folded)", "6000.0 m^2"),
        ("qty 2 ± 0.1 m * 3 km (folded)", "6000 ± 300 m^2"),
        ("qty 25 m/s => km/h", "90.0 km/h"),
        ("base 2 ± 0.1 kohm", "2000.0 ± 100.0 A^-2 * kg * m^2 * s^-3"),
        ("base 0 degC", "273.15 K"),
        ("base 1.5e-5 km", "0.015 m"),
        ("base 625 ± 40.0195264839553 J", "625.0 ± 40.0195264839553 kg * m^2 * s^-2"),
        ("fraction 1.5", "3/2"),
        ("fraction 1.5 mixed", "1 1/2"),
        ("fraction -2.5 mixed", "-2 1/2"),
        ("fraction 0.1 + 0.2", "3/10"),
        ("fraction 89.99999999999999", "90"),
        ("fraction 1/3", "None"),
        ("fraction 1/7 mixed", "None"),
        ("fraction 1/10001", "None"),
        ("fraction pi", "None"),
        ("fraction 1e-30", "None"),
        ("fraction 0.0", "0"),
        ("fraction inf", "None"),
        ("fraction 1e16", "10000000000000000"),
];
