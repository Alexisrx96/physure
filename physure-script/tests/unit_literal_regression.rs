//! Regression net for the quantity-literal → unit path in `phs.pest`.
//!
//! `_is_unit_start` was written as `!keyword ~ _unit_char` instead of
//! `&(!keyword ~ _unit_char)`, so the predicate *consumed* the first character of every
//! unit it was only meant to peek at: `1 kg` parsed as `1 g`, `100 kPa` as `100 Pa`,
//! `1 mol` as `1 * ol`. Magnitudes came out wrong by orders of magnitude while the
//! printed unit still looked plausible.
//!
//! The whole suite stayed green through it because nothing asserted the SI magnitude a
//! prefixed literal actually carries. These tests are that missing assertion.

use physure_core::quantity::Quantity;
use physure_script::interpreter::eval_phs;
use physure_script::value::PhsValue;

/// Evaluates `src` and returns the quantity its last statement produced.
fn eval_quantity(src: &str) -> Quantity {
    let values = eval_phs(src).unwrap_or_else(|e| panic!("{src:?} failed to evaluate: {e:?}"));
    match values.into_iter().last() {
        Some(PhsValue::Quantity(q)) => q,
        other => panic!("{src:?} produced {other:?}, expected a quantity"),
    }
}

/// Evaluates `src` and returns the string its last statement formatted.
fn formatted(src: &str) -> String {
    match eval_phs(src).unwrap_or_else(|e| panic!("{src:?} failed: {e:?}")).into_iter().last() {
        Some(PhsValue::String(s)) => s,
        other => panic!("{src:?} produced {other:?}, expected a formatted string"),
    }
}

fn assert_close(actual: f64, expected: f64, src: &str) {
    let tolerance = expected.abs() * 1e-9;
    assert!(
        (actual - expected).abs() <= tolerance,
        "{src:?}: expected {expected} in base SI, got {actual}",
    );
}

/// A prefixed literal must carry the prefix's factor in its magnitude *and* keep the
/// symbol the user wrote. `canonical_magnitude()` is `mean * scale`, i.e. the value in
/// base SI units — the number the prefix bug silently destroyed.
#[test]
fn prefixed_literals_carry_their_scale() {
    // (source, magnitude in base SI, printed unit)
    let cases = [
        ("1 kg", 1.0, "kg"),
        ("1 g", 1e-3, "g"),
        ("1 t", 1e3, "t"),
        ("1 km", 1e3, "km"),
        ("1 cm", 1e-2, "cm"),
        ("1 mm", 1e-3, "mm"),
        ("1 nm", 1e-9, "nm"),
        ("100 kPa", 1e5, "kPa"),
        ("1 mN", 1e-3, "mN"),
        ("1 MJ", 1e6, "MJ"),
        ("1 GHz", 1e9, "GHz"),
        ("1 us", 1e-6, "us"),
        ("1 mA", 1e-3, "mA"),
        ("1 kOhm", 1e3, "kOhm"),
        ("1 mL", 1e-6, "mL"),
        ("1 eV", 1.602176634e-19, "eV"),
        // Unprefixed multi-letter symbols: these lost their leading character too
        // ("mol" → "ol", "Hz" → "z", "Pa" → "a").
        ("1 mol", 1.0, "mol"),
        ("1 Hz", 1.0, "Hz"),
        ("1 Pa", 1.0, "Pa"),
        ("1 rad", 1.0, "rad"),
        // physure.conf:185 declares degree to 10 significant digits (π/180 is
        // 0.017453292519943295); this asserts the parser preserves the registry's value.
        ("1 deg", 0.0174532925, "deg"),
        ("1 min", 60.0, "min"),
        ("1 h", 3600.0, "h"),
        // Symbols carrying a digit. `a0` parsed as `1 a * 0` — the Bohr radius evaluated
        // to zero — until `unit_term` accepted digits after the first character.
        ("1 a0", 5.29177210903e-11, "a0"),
        ("1 tau0", 2.4188843265e-17, "tau0"),
        // Non-ASCII symbols. The registry lookup was `is_ascii_alphabetic`-only, so these
        // resolved to "" and fell through to the expression parser, which rejects them.
        ("1 Å", 1e-10, "Å"),
        ("1 °", 0.0174532925, "°"),
        ("1 Ω", 1.0, "Ω"),
    ];

    for (src, expected, unit) in cases {
        let q = eval_quantity(src);
        assert_close(q.canonical_magnitude(), expected, src);
        assert_eq!(q.unit.__repr__(), unit, "{src:?} printed the wrong unit");
    }
}

/// A prefixed literal and its expanded form denote the same physical quantity. This is
/// the property the bug broke most visibly: `1 kg` was equal to `1 g`, not to `1000 g`.
#[test]
fn prefixed_literals_equal_their_expanded_form() {
    let equivalences = [
        ("1 kg", "1000 g"),
        ("1 km", "1000 m"),
        ("100 kPa", "1 bar"),
        ("1 mN", "0.001 N"),
        ("1 kOhm", "1000 Ohm"),
        ("1 h", "60 min"),
    ];

    for (left, right) in equivalences {
        let (a, b) = (eval_quantity(left), eval_quantity(right));
        assert!(
            a.unit.same_dimensions(&b.unit),
            "{left:?} and {right:?} have different dimensions ({} vs {})",
            a.unit.__repr__(),
            b.unit.__repr__(),
        );
        assert_close(a.canonical_magnitude(), b.canonical_magnitude(), left);
    }
}

/// Arithmetic over prefixed literals. `0.5 * m * v^2` used to yield `7.2e-8 kg^3 * s^-2`
/// — a dimensionally impossible result — because both operands had been truncated.
#[test]
fn arithmetic_over_prefixed_literals() {
    let kinetic_energy = eval_quantity("m = 2 kg\nv = 3 m/s\n0.5 * m * v^2");
    assert_close(kinetic_energy.canonical_magnitude(), 9.0, "0.5 * m * v^2");
    assert_eq!(kinetic_energy.unit.__repr__(), "J");

    let converted = eval_quantity("100.0 kPa => bar");
    assert_close(converted.canonical_magnitude(), 1e5, "100.0 kPa => bar");
    assert_eq!(converted.unit.__repr__(), "bar");

    let sum = eval_quantity("1.0 km + 500.0 m");
    assert_close(sum.canonical_magnitude(), 1500.0, "1.0 km + 500.0 m");

    let area = eval_quantity("50.0 cm * 20.0 cm");
    assert_close(area.canonical_magnitude(), 0.1, "50.0 cm * 20.0 cm");
}

/// A digit run at the end of a unit symbol is an embedded exponent ("m2" is metre squared)
/// unless the whole symbol is registered ("a0" is the Bohr radius). Getting this wrong is
/// silent: `1 m2` used to evaluate to `2.0 m` and `1 a0` to `0.0 a`, both via an implicit
/// multiplication by the digit the symbol had been truncated at.
#[test]
fn trailing_digits_are_exponents_or_part_of_the_symbol() {
    let embedded = eval_quantity("1 m2");
    assert_close(embedded.canonical_magnitude(), 1.0, "1 m2");
    assert!(
        embedded.unit.same_dimensions(&eval_quantity("1 m^2").unit),
        "`1 m2` should be an area, got {}",
        embedded.unit.__repr__(),
    );

    // A digit-bearing name that is *not* a unit stays an ordinary identifier.
    let variable = eval_quantity("x2 = 5.0 m\n1 x2");
    assert_close(variable.canonical_magnitude(), 5.0, "1 x2");
}

/// When a `unit_expr` runs past the real unit into an expression — `1.602e-19 C / r ^ 2`,
/// where `r` is a bound parameter, not a unit — `split_unit_expr` hands the tail back to
/// the expression grammar. The exponent has to travel with that tail: an atomic
/// `unit_term` that stops before ` ^ 2` leaves the `^` to bind against the whole quantity,
/// silently turning `C / r^2` into `(C / r)^2`.
#[test]
fn unit_leftover_keeps_exponent_precedence() {
    let coulomb = eval_quantity("fn F(r) = 1.602e-19 C / r ^ 2\nF(2 m)");
    assert_close(coulomb.canonical_magnitude(), 4.005e-20, "F(2 m)");
    assert!(
        coulomb.unit.same_dimensions(&eval_quantity("1 C / 1 m^2").unit),
        "F(2 m) has unit {}, expected charge per area",
        coulomb.unit.__repr__(),
    );

    // Whitespace around `^` must not change what the exponent applies to.
    let spaced = eval_quantity("2 m ^ 2");
    assert_close(spaced.canonical_magnitude(), 2.0, "2 m ^ 2");
    assert!(spaced.unit.same_dimensions(&eval_quantity("1 m^2").unit));
}

/// A symbol that isn't registered used to become a brand-new dimension, so `5 foobar`
/// evaluated to `5.0 foobar` and every typo produced a confident wrong answer. It is now
/// an error naming the closest registered symbol.
#[test]
fn unregistered_symbols_are_errors_not_new_dimensions() {
    let message = |src: &str| match eval_phs(src) {
        Err(e) => e.to_string(),
        Ok(v) => panic!("{src:?} should not evaluate, got {v:?}"),
    };

    // A plausible typo names its correction.
    let metre = message("5 metre");
    assert!(metre.contains("metre") && metre.contains("did you mean"), "{metre}");

    // An invented word gets an error, but no misleading suggestion.
    let foobar = message("5 foobar");
    assert!(foobar.contains("foobar"), "{foobar}");
    assert!(!foobar.contains("did you mean"), "{foobar}");

    // Case slips on a prefixed symbol are the common failure and must be caught even
    // though "km"/"kPa" are synthesised by the registry rather than stored in it.
    for (src, expected) in [("5 Km", "km"), ("100 KPa", "kPa"), ("5 Kg", "kg")] {
        let msg = message(src);
        assert!(msg.contains(expected), "{src:?} should suggest {expected:?}, said: {msg}");
    }
}

/// Comparisons used to read the raw magnitude, so `1 km == 1000 m` was false while
/// `1 km + 1000 m` converted correctly — the same two quantities disagreeing with
/// themselves depending on the operator.
#[test]
fn comparisons_convert_scale_and_reject_mismatched_dimensions() {
    let truth = |src: &str| eval_quantity(src).value.mean() > 0.5;

    assert!(truth("1.0 km == 1000.0 m"));
    assert!(truth("100.0 kPa == 1.0 bar"));
    assert!(truth("1.0 h == 60.0 min"));
    assert!(truth("1.0 km > 999.0 m"));
    assert!(truth("999.0 m < 1.0 km"));
    assert!(!truth("1.0 km != 1000.0 m"));

    // A sign test against a bare zero stays legal: zero reads the same in every unit.
    assert!(truth("2.0 m > 0"));
    assert!(!truth("0.0 m > 0"));

    // Comparing across dimensions has no answer, and answering `false` would let it
    // pass silently through a conditional.
    assert!(eval_phs("5.0 m > 2.0 s").is_err(), "m vs s should not compare");
}

/// A declared uncertainty has to reach the output; printing only the mean discards the
/// half of a measurement that says how far to trust the other half.
#[test]
fn uncertainty_survives_to_display_and_is_readable() {
    assert_eq!(eval_quantity("10.0 +/- 0.5 m").to_string(), "10.0 ± 0.5 m");

    // Propagation is visible too: 0.5 and 0.2 add in quadrature.
    let sum = eval_quantity("a = 10.0 +/- 0.5 m\nb = 4.0 +/- 0.2 m\na + b");
    assert_close(sum.value.std_dev(), (0.5f64 * 0.5 + 0.2 * 0.2).sqrt(), "a + b");
    assert!(sum.to_string().contains('±'), "{sum} should show its uncertainty");

    // Exact quantities stay clean — no "± 0" noise on every line.
    assert_eq!(eval_quantity("5.0 m").to_string(), "5.0 m");

    // `uncertainty(x)` hands the standard uncertainty back as a quantity, so it can be
    // divided by the value to get a relative error.
    let sigma = eval_quantity("uncertainty(10.0 +/- 0.5 m)");
    assert_close(sigma.canonical_magnitude(), 0.5, "uncertainty(10.0 +/- 0.5 m)");
    assert_eq!(sigma.unit.__repr__(), "m");
}

/// A unit chain runs `symbol (* symbol)*`, which made every identifier after a `*` look
/// like a unit — `100 kPa * sin(1.0)` used to consume `sin` as a dimension and drop the
/// argument list, yielding `100000 kg*m^-1*s^-2*sin`. A call is never a unit.
#[test]
fn a_call_after_a_unit_is_a_call_and_not_a_unit() {
    let cases = [
        ("100.0 kPa * sin(1.0)", 1.0f64.sin() * 100_000.0, "kPa"),
        ("100.0 kPa * cos(0.0)", 100_000.0, "kPa"),
        ("9.8 m/s^2 * sin(0.5)", 0.5f64.sin() * 9.8, "m/s^2"),
    ];
    for (src, expected, unit) in cases {
        let q = eval_quantity(src);
        assert_close(q.canonical_magnitude(), expected, src);
        assert_eq!(q.unit.__repr__(), unit, "{src:?} lost its unit");
    }
}

/// Conversions leave rounding debris past the 15 digits an f64 carries exactly, and
/// printing the shortest round-tripping string exposes it: `25 m/s => km/h` read
/// "89.99999999999999 km/h". Rounding must not eat digits the user actually typed.
#[test]
fn conversions_do_not_print_floating_point_debris() {
    assert_eq!(eval_quantity("25.0 m/s => km/h").to_string(), "90.0 km/h");
    assert_eq!(eval_quantity("100.0 kPa => bar").to_string(), "1.0 bar");
    assert_eq!(eval_quantity("0.1 + 0.2").to_string(), "0.3");
    assert_eq!(eval_quantity("3.14159265358979 m").to_string(), "3.14159265358979 m");
}

/// `%` is a `_unit_char` in the grammar but had no registry entry, so `5.0 %` was a syntax
/// error. Registering it as a dimensionless 0.01 also had to skip the `dim_to_base` lookup
/// that turns dimension "1" into the `unity` unit, or the ratio would carry a dimension.
#[test]
fn ratios_are_dimensionless_units_with_a_scale() {
    assert_close(eval_quantity("5.0 %").canonical_magnitude(), 0.05, "5.0 %");
    assert_eq!(eval_quantity("5.0 %").to_string(), "5.0 %");
    assert_close(eval_quantity("5.0 ppm").canonical_magnitude(), 5e-6, "5.0 ppm");

    // A ratio applied to a quantity scales it and leaves the unit alone.
    let scaled = eval_quantity("200.0 kPa * 5.0 %");
    assert_close(scaled.canonical_magnitude(), 10_000.0, "200.0 kPa * 5.0 %");
    assert_eq!(scaled.to_string(), "10000.0 Pa");

    // A trailing comment must not reach the unit parser.
    assert_close(eval_quantity("5.0 % # relative error").canonical_magnitude(), 0.05, "commented %");
}

/// `a .. b` builds the range that `plot3d`/`export3d` sample over. Endpoints are ordinary
/// expressions, so they carry units, and a call argument may be named with `:` or `=`.
#[test]
fn ranges_and_named_arguments_parse() {
    let values = eval_phs("r = -2.0 m .. 2.0 m\nr").expect("range failed to evaluate");
    match values.into_iter().last() {
        Some(PhsValue::Range(start, end)) => {
            match (*start, *end) {
                (PhsValue::Quantity(s), PhsValue::Quantity(e)) => {
                    assert_close(s.canonical_magnitude(), -2.0, "range start");
                    assert_close(e.canonical_magnitude(), 2.0, "range end");
                }
                other => panic!("range endpoints came out as {other:?}"),
            };
        }
        other => panic!("`-2.0 m .. 2.0 m` produced {other:?}, expected a range"),
    }

    // Both spellings of a named argument have to reach the same AST — the guide writes
    // `plot3d(P, x: r)` while the rest of the language writes `x = r`.
    let colon = physure_script::parser::parse_phs("plot3d(P, x: r, title: \"t\")");
    let equals = physure_script::parser::parse_phs("plot3d(P, x = r, title = \"t\")");
    assert_eq!(
        format!("{:?}", colon.expect("`x:` form failed to parse")),
        format!("{:?}", equals.expect("`x =` form failed to parse")),
    );

    // A format spec is not a named argument: `.2f` is no expression, so `expr` still wins.
    // The spec is applied, and a quantity keeps its unit through the formatting.
    for (src, expected) in [
        ("3.14159:.2f", "3.14"),
        ("9.8 m/s^2:.3f", "9.800 m/s^2"),
        ("1234.5:.2e", "1.23e3"),
    ] {
        match eval_phs(src).expect("format spec failed to evaluate").into_iter().last() {
            Some(PhsValue::String(s)) => assert_eq!(s, expected, "{src:?} formatted wrong"),
            other => panic!("{src:?} produced {other:?}, expected a formatted string"),
        }
    }

    // `a ? b : c` used to be a parse error: `: c` looked like a format spec and consumed
    // the ternary's colon before the ternary rule could see it.
    let ternary = eval_phs("a = 5 m\nb = 2 m\n1 > 0 ? a : b").expect("identifier ternary failed");
    match ternary.into_iter().last() {
        Some(PhsValue::Quantity(q)) => assert_close(q.canonical_magnitude(), 5.0, "identifier ternary"),
        other => panic!("`1 > 0 ? a : b` produced {other:?}"),
    }
}

/// Unit symbols the literal parser cannot round-trip today, each with the reason. The
/// sweep below asserts these still fail, so fixing one forces removing it from this list
/// rather than letting the list quietly go stale.
const KNOWN_GAPS: &[(&str, &str)] = &[
    // `unity`: dimensionless, prints as the empty string by design.
    ("1", "dimensionless unity has no symbol"),
];

/// Symbols that cannot be written after `=>` today, each with the reason. The sweep below
/// asserts these still fail, so fixing one forces removing it from this list.
///
/// Both are the same gap: the right-hand side of `=>` is parsed as an ordinary expression,
/// so the target has to be an `identifier`, and an identifier is made of `LETTER`s. `°` and
/// `%` are units the grammar accepts in `_unit_char` but that no identifier may contain —
/// and it cannot, or `x%` would be a name. Closing this means deciding that the operand of
/// `=>` is a *unit expression* rather than any expression, which would also stop a variable
/// holding a unit name from being used as a target.
const KNOWN_CONVERSION_GAPS: &[(&str, &str)] = &[
    ("°", "not a LETTER, so no identifier can spell it; use `deg`"),
    ("%", "not a LETTER, so no identifier can spell it; use `percent`"),
];

/// Every symbol in the registry, swept through the literal parser. The prefix bug was a
/// grammar defect, not a per-unit one — a hand-written list of examples would have missed
/// it just like the rest of the suite did, so this asserts the whole registry at once.
#[test]
fn every_registered_symbol_round_trips_through_the_literal_parser() {
    let (registry, _) = physure_core::units::conf::build_registry_from_conf();
    let mut symbols: Vec<String> = registry
        .base_units
        .keys()
        .chain(registry.derived_units.keys())
        .chain(registry.aliases.keys())
        .cloned()
        .collect();
    symbols.sort();
    symbols.dedup();

    // A registry this small means the conf failed to load; the sweep would pass vacuously.
    assert!(symbols.len() > 200, "only {} symbols in the registry", symbols.len());

    let round_trips = |symbol: &str| -> bool {
        matches!(
            eval_phs(&format!("1 {symbol}")).as_deref().map(<[PhsValue]>::last),
            Ok(Some(PhsValue::Quantity(q))) if q.unit.__repr__() == symbol
        )
    };

    let mut broken = Vec::new();
    for symbol in &symbols {
        let known_gap = KNOWN_GAPS.iter().find(|(s, _)| s == symbol);
        match (round_trips(symbol), known_gap) {
            (false, None) => broken.push(format!("  {symbol:?} no longer parses as itself")),
            (true, Some((_, reason))) => {
                broken.push(format!("  {symbol:?} now works — drop it from KNOWN_GAPS ({reason})"))
            }
            _ => {}
        }
    }
    assert!(broken.is_empty(), "unit literal sweep:\n{}", broken.join("\n"));
}

/// The same registry, swept through the *conversion target* position.
///
/// A unit symbol is read by more than one rule: `unit_term` for the unit of a literal,
/// `identifier` for a bare name like the right-hand side of `=>`. They did not agree on
/// what a symbol may contain — `identifier` was ASCII after its first character — so
/// `1 kΩ` parsed as a quantity while `2 Ω => kΩ` cut the target down to `k`, a prefix that
/// is not a unit. One position passing is no evidence about the other, so both are swept.
#[test]
fn every_registered_symbol_is_a_usable_conversion_target() {
    let (registry, _) = physure_core::units::conf::build_registry_from_conf();
    let mut symbols: Vec<String> = registry
        .base_units
        .keys()
        .chain(registry.derived_units.keys())
        .chain(registry.aliases.keys())
        .cloned()
        .collect();
    symbols.sort();
    symbols.dedup();
    assert!(symbols.len() > 200, "only {} symbols in the registry", symbols.len());

    let converts = |symbol: &str| -> bool {
        matches!(
            eval_phs(&format!("1 {symbol} => {symbol}")).as_deref().map(<[PhsValue]>::last),
            Ok(Some(PhsValue::Quantity(q))) if (q.value.mean() - 1.0).abs() < 1e-9
        )
    };

    let mut broken = Vec::new();
    for symbol in &symbols {
        let known_gap = KNOWN_CONVERSION_GAPS.iter().find(|(s, _)| s == symbol);
        match (converts(symbol), known_gap) {
            (false, None) => broken.push(format!("  {symbol:?} is not usable after `=>`")),
            (true, Some((_, reason))) => {
                broken.push(format!("  {symbol:?} now works — drop it from KNOWN_CONVERSION_GAPS ({reason})"))
            }
            _ => {}
        }
    }
    assert!(broken.is_empty(), "conversion target sweep:\n{}", broken.join("\n"));
}

/// A quoted string used to be parsed as an identifier, so it was looked up in the
/// environment first: with `v = 3.0 m/s` in scope, `deriv("0.5*m*v^2", "v")` received the
/// quantity instead of the name and failed. A literal is now literal, and `{name}` is the
/// explicit way to fold a value into it.
#[test]
fn string_literals_are_text_and_braces_interpolate() {
    let last_string = |src: &str| -> String {
        match eval_phs(src).unwrap_or_else(|e| panic!("{src:?} failed: {e:?}")).into_iter().last() {
            Some(PhsValue::String(s)) => s,
            other => panic!("{src:?} produced {other:?}, expected a string"),
        }
    };

    // The variable does not leak into the literal that happens to share its name.
    assert_eq!(last_string("v = 3.0 m/s\n\"v\""), "v");
    assert_eq!(last_string("m = 2.0 kg\n\"0.5 * m * v^2\""), "0.5 * m * v^2");
    // Braces opt in, and the surrounding text (including spaces) survives.
    assert_eq!(last_string("m = 2.0 kg\n\"0.5 * {m} * v^2\""), "0.5 * 2.0 kg * v^2");
    assert_eq!(last_string("x = 3\n\" val {x} end \""), " val 3.0 end ");

    // deriv now sees the name, not the quantity that shares it.
    assert_eq!(last_string("use deriv from calc\nv = 3.0 m/s\nderiv(\"0.5 * m * v^2\", \"v\")"), "m * v");
    // An interpolated quantity keeps its unit through the symbolic layer: implicit
    // multiplication used to strand `kg` after the number and collapse the result to 0.
    assert_eq!(last_string("use deriv from calc\nm = 2.0 kg\nderiv(\"0.5 * {m} * v^2\", \"v\")"), "2 * kg * v");
    assert_eq!(last_string("use deriv from calc\nderiv(\"2 x^2\", \"x\")"), "4 * x");
}

/// `in` was a grammar keyword, taken by `let ... in ...`, so the inch symbol was a parse
/// error and only the `inch` alias worked. Local bindings moved to a postfix `where`, which
/// cannot collide with a unit, and `in` went back to being what the registry says it is.
#[test]
fn inches_parse_and_local_bindings_use_where() {
    assert_close(eval_quantity("12 in").canonical_magnitude(), 12.0 * 0.0254, "12 in");
    assert_close(eval_quantity("12 in => cm").canonical_magnitude(), 0.3048, "12 in => cm");
    assert_close(eval_quantity("1 in^2").canonical_magnitude(), 0.0254 * 0.0254, "1 in^2");
    assert_close(eval_quantity("2.0 in * 3.0").canonical_magnitude(), 6.0 * 0.0254, "2.0 in * 3.0");
    assert_eq!(eval_quantity("12 in").unit.__repr__(), "in", "the symbol the user wrote must survive");

    assert_close(eval_quantity("x * 2.0 where x = 10.0 m").canonical_magnitude(), 20.0, "single where");
    // A later binding sees an earlier one, so the bindings nest in source order.
    assert_close(
        eval_quantity("a + b where a = 2.0 m, b = a * 3.0").canonical_magnitude(),
        8.0,
        "chained where",
    );
    // The clause binds the whole expression that precedes it, ternary included.
    assert_close(
        eval_quantity("x > 1.0 m ? x : 0.0 m where x = 3.0 m").canonical_magnitude(),
        3.0,
        "where after a ternary",
    );
    // The retired `let ... in ...` must fail loudly: read as an expression it would mean
    // "let times inches", a silently wrong answer.
    assert!(eval_phs("let x = 10.0 m in x * 2.0").is_err(), "`let ... in` should no longer parse");
}

/// Evaluates `src` and returns what the last statement rendered as text.
fn eval_string(src: &str) -> String {
    let values = eval_phs(src).unwrap_or_else(|e| panic!("{src:?} failed to evaluate: {e:?}"));
    match values.into_iter().last() {
        Some(PhsValue::String(s)) => s,
        other => panic!("{src:?} produced {other:?}, expected a string"),
    }
}

/// An uncertainty the user wrote has to survive every path that renders or rebuilds the
/// quantity. Three of them threw it away: a format spec printed the mean alone, `round`
/// rebuilt the quantity with a zero std_dev, and a percent uncertainty was parsed as an
/// absolute one — so `9.81 +/- 0.5%` claimed a spread twenty times too wide.
#[test]
fn uncertainty_survives_formatting_rounding_and_percent() {
    let g = eval_quantity("9.81 +/- 0.05 m / s ^ 2");
    assert_close(g.value.std_dev(), 0.05, "literal uncertainty");
    assert_eq!(g.to_string(), "9.81 ± 0.05 m / s ^ 2");

    // A format spec chooses how many digits to show, not which half of the measurement to keep.
    assert_eq!(eval_string("9.81 +/- 0.05 m/s^2 :.2f"), "9.81 ± 0.05 m/s^2");
    assert_eq!(eval_string("9.81 +/- 0.05 m/s^2 :.1e"), "9.8e0 ± 5.0e-2 m/s^2");
    // A quantity with no uncertainty must not grow a "± 0" tail.
    assert_eq!(eval_string("9.81 m/s^2 :.2f"), "9.81 m/s^2");

    // Rounding the mean says nothing about how well it is known.
    let rounded = eval_quantity("round(9.81 +/- 0.05 m/s^2, 1)");
    assert_close(rounded.value.mean(), 9.8, "round keeps the mean");
    assert_close(rounded.value.std_dev(), 0.05, "round keeps the uncertainty");

    // `+/- 0.5%` is 0.5% *of the magnitude*, not 0.5 in the quantity's own unit.
    let relative = eval_quantity("9.81 +/- 0.5% m/s^2");
    assert_close(relative.value.std_dev(), 9.81 * 0.005, "percent uncertainty is relative");
    // A percentage of a magnitude that only exists at run time is not a number.
    assert!(
        eval_phs("(2.0 + 3.0) +/- 1% m").is_err(),
        "a percent uncertainty on a computed magnitude should be rejected, not guessed"
    );
}

/// `abs` rejected quantities outright ("abs expects a number"), so a measurement could not
/// be passed to it at all. Now that it accepts one, the sign is the only thing it may touch:
/// a standard deviation is non-negative and |x| says nothing about how well x is known, so
/// the unit and the uncertainty have to come back exactly as they went in.
#[test]
fn abs_keeps_the_unit_and_the_uncertainty() {
    for src in ["abs(9.81 +/- 0.05 m / s ^ 2)", "abs(-9.81 +/- 0.05 m / s ^ 2)"] {
        let q = eval_quantity(src);
        assert_close(q.value.mean(), 9.81, src);
        assert_eq!(q.unit.__repr__(), "m / s ^ 2", "{src:?} printed the wrong unit");
        assert_close(q.value.std_dev(), 0.05, src);
        assert_eq!(q.to_string(), "9.81 ± 0.05 m / s ^ 2");
    }

    // A bare number keeps the behaviour it always had (the interpreter hands it over as a
    // dimensionless quantity), and a value with no absolute value to take still has to fail
    // loudly rather than be coerced into one.
    let plain = eval_quantity("abs(-3)");
    assert_close(plain.canonical_magnitude(), 3.0, "abs(-3)");
    assert!(eval_phs("abs(\"hello\")").is_err(), "abs of a string should be rejected");
}

/// `floor` and `ceil` rebuilt the quantity with `new_scalar(..., 0.0, ...)`, the same defect
/// `round` had: `floor(9.81 +/- 0.05 m/s^2)` came back as an exact `9.0 m / s ^ 2`, so a
/// measurement that was known to ±0.05 printed as if it had been measured perfectly. Moving
/// the mean to a neighbouring integer says nothing about how well the mean is known, so the
/// unit and the standard deviation both have to survive the trip.
#[test]
fn floor_and_ceil_keep_the_unit_and_the_uncertainty() {
    let floored = eval_quantity("floor(9.81 +/- 0.05 m / s ^ 2)");
    assert_close(floored.value.mean(), 9.0, "floor moves the mean down");
    assert_eq!(floored.unit.__repr__(), "m / s ^ 2", "floor dropped the unit");
    assert_close(floored.value.std_dev(), 0.05, "floor kept the uncertainty");
    assert_eq!(floored.to_string(), "9.0 ± 0.05 m / s ^ 2");

    let ceiled = eval_quantity("ceil(9.81 +/- 0.05 m / s ^ 2)");
    assert_close(ceiled.value.mean(), 10.0, "ceil moves the mean up");
    assert_eq!(ceiled.unit.__repr__(), "m / s ^ 2", "ceil dropped the unit");
    assert_close(ceiled.value.std_dev(), 0.05, "ceil kept the uncertainty");
    assert_eq!(ceiled.to_string(), "10.0 ± 0.05 m / s ^ 2");

    // A dimensionless measurement is the worst case for the old code: every sample of
    // `0.5 +/- 0.01` floors to zero, so anything that rounds the distribution rather than
    // sliding it reports a standard deviation of zero and nobody notices.
    let unit_less = eval_quantity("floor(0.5 +/- 0.01)");
    assert_eq!(unit_less.value.mean(), 0.0, "floor(0.5) is 0");
    assert_close(unit_less.value.std_dev(), 0.01, "floor kept the uncertainty");
    assert_close(eval_quantity("ceil(0.5 +/- 0.01)").value.std_dev(), 0.01, "ceil kept it too");

    // An exact quantity still comes back exact — no "± 0" tail invented on the way.
    assert_eq!(eval_quantity("floor(9.81 m / s ^ 2)").to_string(), "9.0 m / s ^ 2");
    assert_eq!(eval_quantity("ceil(9.81 m / s ^ 2)").to_string(), "10.0 m / s ^ 2");
}

/// `sin`, `cos` and `tan` used to return a bare number built from the mean, throwing away
/// the unit *and* the uncertainty, and they applied themselves to any quantity at all —
/// `sin(9.81 m/s^2)` answered -0.379 as though metres per second squared were radians.
///
/// The contract now: the argument is an angle (converted to radians through its own scale)
/// or a dimensionless value (read as radians, exactly as a bare number is); the result is
/// dimensionless; the uncertainty comes through the derivative. The expected sigmas below
/// are the analytic ones — σ_sin = |cos x|·σ_x, σ_cos = |sin x|·σ_x, σ_tan = sec²x·σ_x — so
/// a wrong propagation formula fails here, not just a discarded one.
#[test]
fn trig_takes_an_angle_and_propagates_the_derivative() {
    let sin = eval_quantity("sin(0.5 +/- 0.01)");
    assert_close(sin.value.mean(), 0.5_f64.sin(), "sin mean");
    assert!(sin.unit.dimensions.is_empty(), "sin of an angle is a pure ratio");
    assert_close(sin.value.std_dev(), 0.5_f64.cos().abs() * 0.01, "σ_sin = |cos x|·σ_x");

    let cos = eval_quantity("cos(0.5 +/- 0.01)");
    assert_close(cos.value.mean(), 0.5_f64.cos(), "cos mean");
    assert!(cos.unit.dimensions.is_empty(), "cos of an angle is a pure ratio");
    assert_close(cos.value.std_dev(), 0.5_f64.sin().abs() * 0.01, "σ_cos = |sin x|·σ_x");

    let tan = eval_quantity("tan(0.5 +/- 0.01)");
    assert_close(tan.value.mean(), 0.5_f64.tan(), "tan mean");
    assert!(tan.unit.dimensions.is_empty(), "tan of an angle is a pure ratio");
    assert_close(tan.value.std_dev(), (1.0 + 0.5_f64.tan().powi(2)) * 0.01, "σ_tan = sec²x·σ_x");

    // An angle in its own unit is converted first: `deg` carries the degrees → radians
    // factor as its scale, so sin(90°) is 1, not sin(90 rad) = 0.894.
    let deg: f64 = 0.0174532925; // the factor physure.conf gives `deg`
    assert_close(eval_quantity("sin(90 deg)").value.mean(), 1.0, "sin(90 deg)");
    let cos_deg = eval_quantity("cos(45 +/- 1 deg)");
    assert_close(cos_deg.value.mean(), (45.0 * deg).cos(), "cos(45 deg)");
    assert_close(cos_deg.value.std_dev(), (45.0 * deg).sin() * deg, "one degree of spread");
    assert!(cos_deg.unit.dimensions.is_empty(), "cos of a degree angle is still a pure ratio");

    // A quantity that is not an angle has no trigonometric value at all. Answering anyway is
    // the confident wrong answer: 9.81 m/s^2 is not 9.81 radians.
    for src in ["sin(9.81 +/- 0.05 m / s ^ 2)", "cos(5 m)", "tan(2 kg)"] {
        assert!(eval_phs(src).is_err(), "{src:?} should be rejected as a dimensional error");
    }

    // An exact argument still gives an exact answer, with no unit and no invented spread.
    let exact = eval_quantity("sin(0.5)");
    assert_close(exact.value.mean(), 0.5_f64.sin(), "sin(0.5)");
    assert_eq!(exact.value.std_dev(), 0.0, "an exact argument has an exact sine");
}

/// `exp`, `ln` and `log` rejected every quantity ("exp expects a number"), so they were
/// unusable even on a dimensionless one — `exp(0.5 +/- 0.01)` could not be evaluated at all.
/// They are power series in their argument, so the argument must be dimensionless and the
/// result is too; a dimensioned argument is a physics error and is reported as one rather
/// than computed from the bare magnitude.
#[test]
fn transcendentals_accept_a_dimensionless_quantity_and_reject_a_dimensioned_one() {
    let exp = eval_quantity("exp(0.5 +/- 0.01)");
    assert_close(exp.value.mean(), 0.5_f64.exp(), "exp mean");
    assert!(exp.unit.dimensions.is_empty(), "exp of a pure number is a pure number");
    assert_close(exp.value.std_dev(), 0.5_f64.exp() * 0.01, "σ_exp = e^x·σ_x");

    let ln = eval_quantity("ln(0.5 +/- 0.01)");
    assert_close(ln.value.mean(), 0.5_f64.ln(), "ln mean");
    assert!(ln.unit.dimensions.is_empty(), "ln of a pure number is a pure number");
    assert_close(ln.value.std_dev(), 0.01 / 0.5, "σ_ln = σ_x/x");

    // `log` is base 10 in PHS, so both the mean and the sigma carry the 1/ln(10) factor.
    let log = eval_quantity("log(0.5 +/- 0.01)");
    assert_close(log.value.mean(), 0.5_f64.log10(), "log is base 10");
    assert!(log.unit.dimensions.is_empty(), "log of a pure number is a pure number");
    assert_close(log.value.std_dev(), 0.01 / (0.5 * std::f64::consts::LN_10), "σ_log10 = σ_x/(x·ln10)");

    // ln(5 m) is a physics error the tool should catch, not compute.
    for src in ["exp(9.81 +/- 0.05 m / s ^ 2)", "ln(5 m)", "log(2 kg)"] {
        assert!(eval_phs(src).is_err(), "{src:?} should be rejected as a dimensional error");
    }

    // Exact arguments stay exact.
    assert_eq!(eval_quantity("exp(0.5)").value.std_dev(), 0.0, "an exact argument has an exact exp");
    assert_close(eval_quantity("ln(0.5)").value.mean(), 0.5_f64.ln(), "ln(0.5)");
    assert_close(eval_quantity("log(0.5)").value.mean(), 0.5_f64.log10(), "log(0.5)");
}

/// `physure.conf` declares both `radian` and `steradian` against dimension `A`, and the
/// loader let the later line win, so the angle dimension's base unit became the steradian:
/// `deg`, `arcmin` and `arcsec` were registered as scaled steradians. `90 deg => rad` failed
/// as a unit mismatch while `1 deg + 1 sr` was cheerfully accepted — and trigonometry could
/// not take a quantity in degrees at all.
#[test]
fn degrees_are_an_angle_in_radians_not_in_steradians() {
    let radians = eval_quantity("90 deg => rad");
    assert_close(radians.value.mean(), 90.0 * 0.0174532925, "90 deg in radians");
    assert_eq!(radians.unit.__repr__(), "rad");

    let sum = eval_quantity("30 deg + 15 deg");
    assert_close(sum.value.mean(), 45.0, "degrees add as degrees");

    // A solid angle is still not a plane angle.
    assert!(eval_phs("1 deg + 1 sr").is_err(), "a degree is not a steradian");
}

/// `x: base` quotes a quantity in the units it is built from. `physure-cli/README.md` has
/// advertised it alongside `.3e` since the CLI's first page, while `format_spec` only ever
/// accepted the numeric form — so it was a parse error, not a wrong answer, which is the
/// one honest failure mode of the three but still not the documented one.
#[test]
fn the_base_format_spec_quotes_a_quantity_in_its_base_units() {
    assert_eq!(formatted("2 Ohm: base"), "2.0 A^-2 * kg * m^2 * s^-3");
    // The prefix's factor moves into the magnitude: the physical value is the same one
    // `Display` prints, only the terms it is quoted in change.
    assert_eq!(formatted("2 kOhm: base"), "2000.0 A^-2 * kg * m^2 * s^-3");
    assert_eq!(formatted("100 kPa: base"), "100000.0 kg * m^-1 * s^-2");
    // The uncertainty is carried through the rescaling rather than dropped.
    assert_eq!(formatted("9.81 +/- 0.05 m/s^2: base"), "9.81 ± 0.05 m * s^-2");
    // The word is matched exactly: a longer name is an ordinary expression, so a ternary
    // whose else branch merely starts with those letters still parses.
    let ternary = eval_phs("based = 5 m\n1 > 0 ? based : 2 m").expect("`: based` broke the ternary");
    match ternary.into_iter().last() {
        Some(PhsValue::Quantity(q)) => assert_close(q.canonical_magnitude(), 5.0, "`: based` ternary"),
        other => panic!("`1 > 0 ? based : 2 m` produced {other:?}"),
    }
}

/// A unit symbol has to mean the same thing wherever it is written. `identifier` was ASCII
/// after its first character while `unit_term` took any `LETTER`, so a prefixed non-ASCII
/// symbol parsed as a literal and not as a conversion target: `2 Ω => kΩ` reported
/// "Unknown unit 'k'", the prefix left over once `Ω` had been cut off it.
#[test]
fn a_prefixed_non_ascii_symbol_is_a_valid_conversion_target() {
    let converted = eval_quantity("2 Ω => kΩ");
    assert_close(converted.canonical_magnitude(), 2.0, "2 Ω => kΩ");
    assert_eq!(converted.unit.__repr__(), "kΩ");
    assert!((converted.value.mean() - 0.002).abs() < 1e-12, "expected 0.002 kΩ");

    // The ASCII spelling of the same unit was never broken; both must agree.
    let ascii = eval_quantity("2 Ohm => kOhm");
    assert_close(ascii.canonical_magnitude(), converted.canonical_magnitude(), "2 Ohm => kOhm");

    // A non-ASCII symbol is also a usable name, not only a unit.
    let bound = eval_quantity("Δx = 3 m\nΔx + 1 m");
    assert_close(bound.canonical_magnitude(), 4.0, "Δx + 1 m");
}

/// A format spec closes the expression it is written on. Bound one level too low, inside
/// `comp_expr`, it took the *right operand* instead: `0.1 + 0.2: base` was a parse error,
/// and `25 m/s => km/h: base` printed `25.0 m/s` — the spec formatted the conversion target
/// and the conversion itself was dropped without a word, which is the one thing a unit
/// library may never do quietly.
#[test]
fn a_format_spec_applies_to_the_whole_expression() {
    assert_eq!(formatted("25 m/s => km/h:.2f"), "90.00 km/h");
    assert_eq!(formatted("0.1 + 0.2:.2f"), "0.30");
    assert_eq!(formatted("1 m + 50 cm: base"), "1.5 m");
    // Parenthesising was the workaround; it still parses and still means the same thing.
    assert_eq!(formatted("(25 m/s => km/h):.2f"), "90.00 km/h");
    // A comparison keeps its own operands — the spec lands on the result of the test.
    assert_eq!(formatted("2 m > 1 m:.1f"), "1.0");
}

/// `frac` and `ifrac` write a magnitude as a common and as a mixed fraction. "When one
/// applies" is the whole contract: a number with no small fraction behind it keeps its
/// decimal instead of being rounded into a tidier lie.
#[test]
fn the_frac_format_spec_writes_a_number_as_a_fraction() {
    assert_eq!(formatted("1.5: frac"), "3/2");
    assert_eq!(formatted("1.5: ifrac"), "1 1/2");
    // A value below 1 has no whole part to quote, and `0 1/2` is nobody's notation.
    assert_eq!(formatted("0.5: ifrac"), "1/2");
    assert_eq!(formatted("-1.5: ifrac"), "-1 1/2");
    assert_eq!(formatted("-1.5: frac"), "-3/2");
    // A whole number is a whole number, not `3/1`.
    assert_eq!(formatted("3.0: frac"), "3");
    assert_eq!(formatted("1.5 m: frac"), "3/2 m");
    assert_eq!(formatted("2.75 kg: ifrac"), "2 3/4 kg");
    // Both halves of a measurement are quoted, not just the mean.
    assert_eq!(formatted("9.81 +/- 0.05 m/s^2: frac"), "981/100 ± 1/20 m/s^2");
    // Rounding debris is not a fraction: `0.1 + 0.2` is 0.30000000000000004 and `25 m/s`
    // in km/h lands on 89.99999999999999, whose exact ratios are astronomical.
    assert_eq!(formatted("0.1 + 0.2: frac"), "3/10");
    assert_eq!(formatted("25 m/s => km/h: frac"), "90 km/h");
    // π has no fraction to give, so the decimal stands.
    assert_eq!(formatted("3.14159265358979: frac"), "3.14159265358979");
    // The words are matched exactly: a longer name is an ordinary expression, so a ternary
    // whose else branch merely starts with those letters still parses.
    let ternary = eval_phs("fraction = 5 m\n1 > 0 ? fraction : 2 m").expect("`: fraction` broke the ternary");
    match ternary.into_iter().last() {
        Some(PhsValue::Quantity(q)) => assert_close(q.canonical_magnitude(), 5.0, "`: fraction` ternary"),
        other => panic!("`1 > 0 ? fraction : 2 m` produced {other:?}"),
    }
}

/// `..` binds looser than everything else, so each endpoint is a whole expression: it is
/// the conversion written next to `=>` that moves, and only a parenthesised range converts
/// as a range. At the precedence `..` used to have — the same level as `+` and `=>`, left
/// associative — `0 m .. 100 m => km` read as `(0 m .. 100 m) => km`, and since nothing
/// converted a range the `=> km` was dropped without a word.
#[test]
fn a_range_binds_looser_than_the_operators_inside_it() {
    let range = |src: &str| -> String { eval_phs(src).unwrap_or_else(|e| panic!("{src:?} failed: {e:?}")).into_iter().last().expect("no value").to_string() };

    // A unit written outside the parentheses reaches both endpoints.
    assert_eq!(range("(0 .. 100) m"), "0.0 m .. 100.0 m");
    // An endpoint with no dimension of its own takes the other's unit.
    assert_eq!(range("0 .. 100 m"), "0.0 m .. 100.0 m");
    // The conversion belongs to the endpoint it was written on...
    assert_eq!(range("0 m .. 100 m => km"), "0.0 m .. 0.1 km");
    assert_eq!(range("0 m => km .. 100 m"), "0.0 km .. 100.0 m");
    // ...unless the range is parenthesised, which converts both.
    assert_eq!(range("(0 m .. 100 m) => km"), "0.0 km .. 0.1 km");
    // A dimensionless range stays dimensionless.
    assert_eq!(range("-2 .. 2"), "-2.0 .. 2.0");
    // Endpoints may be names, and a range may be held in one.
    assert_eq!(range("a = 0 m\nb = 100 m\na .. b"), "0.0 m .. 100.0 m");
    assert_eq!(range("r = 0 m .. 100 m\nr => km"), "0.0 km .. 0.1 km");
    // A format spec reaches both endpoints rather than neither.
    assert_eq!(range("0 m .. 100 m: .2f"), "0.00 m .. 100.00 m");
    assert_eq!(range("0.5 m .. 1.5 m: ifrac"), "1/2 m .. 1 1/2 m");
}

/// A range that is not an interval is refused rather than built. Nothing downstream — a
/// plot's sampling, an integration limit — has an answer for one that measures two
/// different things or does not run upwards, and inventing an order for it produces a
/// figure that looks fine and is not.
#[test]
fn a_range_that_is_not_an_interval_is_refused() {
    let refused = |src: &str| -> String {
        match eval_phs(src) {
            Err(e) => e.to_string(),
            Ok(v) => panic!("{src:?} was accepted, producing {:?}", v.last()),
        }
    };

    assert!(refused("0 m .. 100 s").contains("Incompatible dimensions in range"));
    assert!(refused("100 m .. 0 m").contains("not below"));
    // The bounds must be distinct: an empty interval has no points to sample.
    assert!(refused("5 m .. 5 m").contains("not below"));
    assert!(refused("\"a\" .. 100 m").contains("runs between two magnitudes"));
    // A missing endpoint, and a third one, are grammar errors — `..` takes exactly two.
    assert!(eval_phs("0 m ..").is_err(), "`0 m ..` parsed with no upper bound");
    assert!(eval_phs(".. 100 m").is_err(), "`.. 100 m` parsed with no lower bound");
    assert!(eval_phs("0 m .. 100 m .. 200 m").is_err(), "`a .. b .. c` parsed as a range");
}
