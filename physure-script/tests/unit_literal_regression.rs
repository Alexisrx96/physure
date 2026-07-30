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
    // `in` is a grammar keyword (from `let ... in`), so inches are unusable as a literal.
    // Fixing it needs a context-sensitive keyword rule, not a symbol change.
    ("in", "shadowed by the `in` keyword"),
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
