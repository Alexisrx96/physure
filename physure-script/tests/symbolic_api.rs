//! The symbolic expression API that Rust callers (and the bindings) use directly: build an expression, differentiate
//! it, integrate it, simplify it, compile it and evaluate it. And the exporters of PHS values.

use physure_script::exporter::{DataExporter, Exporter};
use physure_script::symbolic::Expr;
use physure_script::PhsValue;
use std::collections::HashMap;

fn x() -> Expr {
    Expr::symbol("x".to_string())
}

#[test]
fn an_expression_is_built_from_symbols_numbers_and_functions() {
    let e = Expr::sin(&x()).mul(&Expr::number(2.0)).add(&Expr::number(1.0)).unwrap();
    assert_eq!(e.simplify().to_phs_string(), "2 * sin(x) + 1");
    for (built, text) in [
        (Expr::cos(&x()), "cos(x)"),
        (Expr::tan(&x()), "tan(x)"),
        (Expr::cot(&x()), "cot(x)"),
        (Expr::sec(&x()), "sec(x)"),
        (Expr::csc(&x()), "csc(x)"),
        (Expr::ln(&x()), "ln(x)"),
        (Expr::exp(&x()), "exp(x)"),
        (Expr::sinh(&x()), "sinh(x)"),
        (Expr::cosh(&x()), "cosh(x)"),
        (Expr::tanh(&x()), "tanh(x)"),
        (Expr::coth(&x()), "coth(x)"),
        (Expr::sech(&x()), "sech(x)"),
        (Expr::csch(&x()), "csch(x)"),
        (Expr::abs(&x()), "abs(x)"),
        (Expr::sqrt(&x()), "sqrt(x)"),
    ] {
        assert_eq!(built.to_phs_string(), text);
    }
    for (built, text) in [
        (Expr::arcsin(&x()), "asin(x)"),
        (Expr::arccos(&x()), "acos(x)"),
        (Expr::arctan(&x()), "atan(x)"),
        (Expr::arccot(&x()), "acot(x)"),
        (Expr::arcsec(&x()), "asec(x)"),
        (Expr::arccsc(&x()), "acsc(x)"),
    ] {
        // the inverse functions are written the short way
        assert_eq!(built.to_phs_string(), text);
    }
}

#[test]
fn an_expression_is_differentiated_and_integrated() {
    let cube = x().pow(&Expr::number(3.0));
    assert_eq!(cube.diff("x", 1).unwrap().simplify().to_phs_string(), "3 * x^2");
    assert_eq!(cube.diff("x", 2).unwrap().simplify().to_phs_string(), "6 * x");
    assert_eq!(cube.integrate("x").unwrap().simplify().to_phs_string(), "x^4/4");
    assert_eq!(Expr::diff_str("x^3", "x").unwrap(), "3 * x^2");
    assert_eq!(Expr::integrate_str("x^3", "x").unwrap(), "x^4/4");
    assert_eq!(Expr::solve_str("2*x + 3 = 7", "x").unwrap(), "2");
}

#[test]
fn a_sum_is_factored_by_what_its_terms_share() {
    let e = Expr::parse("x*y + x*z").unwrap();
    assert_eq!(e.factor().to_phs_string(), "(y + z) * x");
    // nothing shared: the sum stays
    assert_eq!(Expr::parse("x + y").unwrap().factor().to_phs_string(), "x + y");
    // powers of one base merge
    assert_eq!(Expr::parse("x^2 * x^3").unwrap().factor().to_phs_string(), "x^5");
}

#[test]
fn an_expression_compiles_to_a_function_of_its_symbols() {
    let compiled = Expr::parse("x^2 + 3*x").unwrap().compile().unwrap();
    assert_eq!(compiled.eval(&[2.0]).unwrap(), 10.0);
    assert_eq!(compiled.eval(&[0.0]).unwrap(), 0.0);
    let trig = Expr::parse("sin(x)^2 + cos(x)^2").unwrap().compile().unwrap();
    assert!((trig.eval(&[0.7]).unwrap() - 1.0).abs() < 1e-12);
    let quotient = Expr::parse("exp(x)/(1 + x)").unwrap().compile().unwrap();
    assert!((quotient.eval(&[1.0]).unwrap() - std::f64::consts::E / 2.0).abs() < 1e-12);
}

#[test]
fn a_quantity_in_an_expression_carries_its_unit() {
    let metre = physure_core::units::parser::Parser::parse_expression("m").unwrap();
    let length = Expr::quantity("L".to_string(), &metre);
    assert_eq!(length.unit().unwrap().unwrap().__repr__(), "m");
    // a sum needs the same unit on both sides
    let second = physure_core::units::parser::Parser::parse_expression("s").unwrap();
    let time = Expr::quantity("t".to_string(), &second);
    assert!(length.add(&time).is_err());
    assert!(length.sub(&time).is_err());
    // a product multiplies them
    assert_eq!(length.div(&time).unit().unwrap().unwrap().__repr__(), "m * s^-1");
    // a plain number has none to add
    assert!(Expr::number(2.0).unit().unwrap().unwrap().dimensions.is_empty());
}

#[test]
fn an_expression_that_does_not_parse_is_an_error() {
    assert!(Expr::parse("2 +").is_err());
    assert!(Expr::parse("foo(1, 2)").is_err());
}

fn exports() -> HashMap<String, PhsValue> {
    let mut exports = HashMap::new();
    exports.insert("count".to_string(), PhsValue::Number(3.0));
    exports.insert("flag".to_string(), PhsValue::Bool(true));
    exports.insert("label".to_string(), PhsValue::String("a \"quoted\" word".to_string()));
    exports.insert("speed".to_string(), PhsValue::Quantity(physure_core::Quantity::new(9.5, "m/s").unwrap()));
    exports.insert("samples".to_string(), PhsValue::Vector(vec![PhsValue::Number(1.0), PhsValue::Number(2.5)]));
    exports.insert("nothing".to_string(), PhsValue::None);
    exports
}

#[test]
fn the_exports_of_a_module_go_to_json() {
    let json = Exporter::export_json(&exports()).unwrap();
    for part in ["\"count\"", "\"flag\": true", "\\\"quoted\\\"", "9.5", "[1", "null"] {
        assert!(json.contains(part), "{part} is not in {json}");
    }
    assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok(), "{json}");
}

#[test]
fn the_exports_of_a_module_go_to_csv_and_to_a_python_dict() {
    let csv = Exporter::export_csv(&exports()).unwrap();
    assert!(csv.lines().count() >= 7, "{csv}");
    assert!(csv.contains("count") && csv.contains("speed"), "{csv}");
    let dict = Exporter::export_py_dict(&exports()).unwrap();
    assert!(dict.starts_with('{') && dict.contains("'count': 3") && dict.contains("True"), "{dict}");
    // a quote in a string is escaped in each format
    assert!(csv.contains("\"a \"\"quoted\"\" word\""), "{csv}");
    assert!(dict.contains("'label': 'a \"quoted\" word'"), "{dict}");
}
