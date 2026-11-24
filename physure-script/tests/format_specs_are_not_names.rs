use physure_script::interpreter::eval_phs;

fn shows(source: &str) -> String {
    let values = eval_phs(source).unwrap_or_else(|e| panic!("{source:?} failed: {e}"));
    values.last().expect("a value").to_string()
}

#[test]
fn a_format_word_formats_even_when_a_variable_has_its_name() {
    // the same text the format prints with no variable at all
    assert_eq!(shows("base = 3\n2 kohm: base"), "2000.0 A^-2 * kg * m^2 * s^-3");
    assert_eq!(shows("frac = 7\n1.5: frac"), "3/2");
    assert_eq!(shows("ifrac = 7\n1.5: ifrac"), "1 1/2");
    // inside a text span too
    assert_eq!(shows("frac = 7\nx = 1.5\n\"{x: frac}\""), "3/2");
}

#[test]
fn the_format_words_still_format() {
    // what the format prints without any variable (`base` is the unit-free, base-unit form)
    assert_eq!(shows("2 kohm: base"), "2000.0 A^-2 * kg * m^2 * s^-3");
    assert_eq!(shows("1.5: frac"), "3/2");
    assert_eq!(shows("1.5: ifrac"), "1 1/2");
    assert_eq!(shows("3.14159: .2f"), "3.14");
    assert_eq!(shows("12345.678: .2e"), "1.23e4");
}

fn refused(source: &str) -> String {
    match eval_phs(source) {
        Ok(values) => panic!("{source:?} gave {values:?}, expected an error"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn a_spec_held_by_a_variable_is_the_variable_value() {
    // `format(x, s1)` evaluates `s1`: only the `x: spec` syntax reads its word as text
    assert_eq!(shows("s1 = \".2f\"
format(3.14159, s1)"), "3.14");
    assert_eq!(shows("base = \"frac\"
format(1.5, base)"), "3/2");
    assert_eq!(shows("base = \"frac\"
1.5: base"), "1.5");
}

#[test]
fn every_documented_spec_formats() {
    // every spec the documentation, the guide and the tests use
    for (source, expected) in [
        ("3.14159: .2f", "3.14"),
        ("3.14159: .0f", "3"),
        ("3.14159: .1f", "3.1"),
        ("3.14159: .3f", "3.142"),
        ("3.14159: .4f", "3.1416"),
        ("3.14159: .6f", "3.141590"),
        ("3.14159: .15f", "3.141590000000000"),
        ("3.14159: .1", "3.1"),
        ("3.14159: .2", "3.14"),
        ("3.14159: .4g", "3.1416"),
        ("3.14159: .17g", "3.14158999999999988"),
        ("12345.678: .1e", "1.2e4"),
        ("12345.678: .3e", "1.235e4"),
        ("format(3.14159, \".2f\")", "3.14"),
        ("format(1.5, \"frac\")", "3/2"),
    ] {
        assert_eq!(shows(source), expected, "{source:?}");
    }
    assert!(shows("12345.678: .14e").ends_with("e4"));
    assert_eq!(shows("format(2 m, \"base\")"), shows("2 m: base"));
}

#[test]
fn an_unknown_spec_is_refused_naming_the_forms() {
    for source in ["3.14: .2z", "format(3.14, \"zz\")", "format(3.14, \".zz\")", "format(3.14, \"base2\")", "format(3.14, \"\")", "format(3.14, 2)"] {
        let message = refused(source);
        assert!(message.contains("`base`, `frac`, `ifrac`"), "{source:?}: {message}");
    }
}

#[test]
fn the_generators_write_the_spec_as_text_and_a_spec_variable_as_the_variable() {
    use physure_script::codegen::{transpile, Target};
    let emit = |source: &str, target| transpile(&physure_script::parser::parse_phs(source).unwrap(), target).unwrap();
    // the word of `x: .2f` used to be written as a bare `.2f`, which compiles in no target
    assert!(emit("x = 3.14159
y = x: .2f", Target::Python).contains("format(x, \".2f\")"));
    assert!(emit("x = 3.14159
y = x: .2f", Target::Java).contains("format(x, \".2f\")"));
    assert!(emit("x = 3.14159
y = x: .2f", Target::Rust).contains("format(x, \".2f\""));
    // the Python generator used to quote any second argument: `format(x, "s1")`
    assert!(emit("x = 3.14159
s1 = \".2f\"
y = format(x, s1)", Target::Python).contains("format(x, s1)"));
}
