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
