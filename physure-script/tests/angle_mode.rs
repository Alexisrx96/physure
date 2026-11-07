//! The angle mode, seen from PHS: what a conversion may read as 1, and what nothing may.

use physure_core::units::angle_mode::{scoped, AngleMode};
use physure_script::interpreter::eval_phs;
use physure_script::value::PhsValue;

/// The magnitude, in the unit the program asked for, of the quantity its last statement produced.
fn magnitude(source: &str) -> f64 {
    match eval_phs(source).unwrap_or_else(|e| panic!("{source:?} failed: {e:?}")).into_iter().last() {
        Some(PhsValue::Quantity(q)) => q.value.mean(),
        other => panic!("{source:?} produced {other:?}, expected a quantity"),
    }
}

fn assert_magnitude(source: &str, expected: f64) {
    let actual = magnitude(source);
    assert!(
        (actual - expected).abs() <= expected.abs().max(1.0) * 1e-12,
        "{source:?}: expected {expected}, got {actual}",
    );
}

/// What a conversion may read as 1, with its answer in the default mode; the strict mode refuses all of them.
const CONVERSIONS: &[(&str, f64)] = &[
    ("(4 kg) * (0.5 m)^2 / 4 * (10 rad/s)^2 => J", 25.0),
    ("(10 rad/s) * (0.5 m) => m/s", 5.0),
    ("(2 m) * (0.5 rad) => m", 1.0),
    ("(10 N*m) * (3 rad/s) => W", 30.0),
    ("(377 rad/s) * (0.1 H) => ohm", 37.7),
    ("(2 rad) => unity", 2.0),
    ("(90 deg) => unity", std::f64::consts::FRAC_PI_2),
    ("(100 cd) / (2 m)^2 => lx", 25.0),
    ("(5 lm) => cd", 5.0),
    ("(30 W) / (4 sr) / (2 m)^2 => W/m^2", 1.875),
    ("(2 rad => unity) + 1", 3.0),
    ("(25 kg*m^2*rad^2/s^2 => J) - (25 J)", 0.0),
];

/// Accepted in both modes, as they were before the angle mode existed.
const UNCHANGED: &[(&str, f64)] = &[
    ("(10 rad/s) / (6.283185307179586 rad) => Hz", 1.5915494309189535),
    ("(6.283185307179586 rad) * (50 Hz) => rad/s", 314.1592653589793),
    ("(1 rad) + (90 deg)", 2.5707963267948966),
    ("(10 N*m/rad) * (2 rad) => J", 20.0),
    ("(1 m) + (100 cm)", 2.0),
    ("(20 dB) => percent", 10000.0),
];

/// Refused in both modes: a sum or a comparison across a marker, one marker against another, a level against an
/// angle, a rate of angle against a frequency, and a trigonometric function of something that is not an angle.
const REFUSED: &[&str] = &[
    "(2 rad) + 1",
    "1 + (2 rad)",
    "(5 deg) + (50 %)",
    "(1 rad) == 1",
    "(2 m*rad) + (1 m)",
    "(25 kg*m^2*rad^2/s^2) - (25 J)",
    "(1 rad)^2 + (1 rad)",
    "(1 deg) => sr",
    "(1 rad) + (1 sr)",
    "(1 deg) + (1 sr)",
    "(1 rad) => dB",
    "sin(9.81 m/s^2)",
    "(1 rad/s) => sr/s",
    "(1 sr) => rad",
    "(10 rad/s) => Hz",
    "(10 rad/s) => kHz",
    "(10 Hz) => rad/s",
];

#[test]
fn the_default_mode_converts_across_the_markers() {
    let _mode = scoped(AngleMode::Dimensionless);
    for (source, expected) in CONVERSIONS {
        assert_magnitude(source, *expected);
    }
}

#[test]
fn the_strict_mode_refuses_every_such_conversion() {
    let _mode = scoped(AngleMode::Dimension);
    for (source, _) in CONVERSIONS {
        assert!(eval_phs(source).is_err(), "{source:?} should be refused in the strict mode");
    }
}

#[test]
fn what_never_depended_on_the_mode_still_does_not() {
    for mode in [AngleMode::Dimensionless, AngleMode::Dimension] {
        let _mode = scoped(mode);
        for (source, expected) in UNCHANGED {
            assert_magnitude(source, *expected);
        }
        for source in ["sin(90 deg)", "sin(2 rad)"] {
            assert!(eval_phs(source).is_ok(), "{source:?} in {mode:?}");
        }
        for source in REFUSED {
            assert!(eval_phs(source).is_err(), "{source:?} should be refused in {mode:?}");
        }
    }
}

#[test]
fn a_parallel_for_runs_in_the_mode_of_its_caller() {
    let program = "v = for i in 1..9 { (10 rad/s) * (0.5 m) => m/s }";
    let _every_loop_is_parallel = physure_core::settings::scoped(0);
    {
        let _strict = scoped(AngleMode::Dimension);
        assert!(eval_phs(program).is_err(), "the strict mode must reach the rayon workers");
    }
    let _default = scoped(AngleMode::Dimensionless);
    assert!(eval_phs(program).is_ok());
}

#[test]
fn parallel_map_runs_in_the_mode_of_its_caller() {
    let program = "fn speed(w) = w * (0.5 m) => m/s\nres = parallel_map(speed, vector(1 rad/s, 2 rad/s, 3 rad/s))";
    {
        let _strict = scoped(AngleMode::Dimension);
        assert!(eval_phs(program).is_err(), "the strict mode must reach the rayon workers");
    }
    let _default = scoped(AngleMode::Dimensionless);
    assert!(eval_phs(program).is_ok());
}
