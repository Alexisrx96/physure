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
