//! A golden file that pins what the angle work must not change.
//!
//! `tests/golden/angle_mode_strict.tsv` (at the repository root) holds, for every expression of `CORPUS`, what PHS
//! answered on `main` @ `b27d0f8`, before the angle mode existed. In `Dimension` mode the answers must be the same
//! ones, byte for byte: that is the claim that the strict mode is today's behaviour.
//!
//! The file is written once, on a tree that has no angle mode yet, with
//!
//!     RECORD_GOLDEN=1 cargo test -p physure-script --test angle_mode_golden

use physure_script::interpreter::eval_phs;
use std::path::PathBuf;

const CORPUS: &[&str] = &[
    // what a conversion may read as 1
    "(4 kg) * (0.5 m)^2 / 4 * (10 rad/s)^2 => J",
    "(10 rad/s) * (0.5 m) => m/s",
    "(2 m) * (0.5 rad) => m",
    "(10 N*m) * (3 rad/s) => W",
    "(377 rad/s) * (0.1 H) => ohm",
    "(2 rad) => unity",
    "(90 deg) => unity",
    "(100 cd) / (2 m)^2 => lx",
    "(5 lm) => cd",
    "(30 W) / (4 sr) / (2 m)^2 => W/m^2",
    "(2 rad => unity) + 1",
    "(25 kg*m^2*rad^2/s^2 => J) - (25 J)",
    // a rate of angle and a frequency
    "(10 rad/s) => Hz",
    "(10 rad/s) => kHz",
    "(10 Hz) => rad/s",
    "(10 rad/s) / (6.283185307179586 rad) => Hz",
    "(6.283185307179586 rad) * (50 Hz) => rad/s",
    // sums and comparisons that must stay refused
    "(2 rad) + 1",
    "1 + (2 rad)",
    "(5 deg) + (50 %)",
    "(1 rad) == 1",
    "(2 m*rad) + (1 m)",
    "(25 kg*m^2*rad^2/s^2) - (25 J)",
    "(1 rad)^2 + (1 rad)",
    // accepted today, and still
    "(1 rad) + (90 deg)",
    "(10 N*m/rad) * (2 rad) => J",
    "(1 m) + (100 cm)",
    "(20 dB) => percent",
    "(5 Hz) => kHz",
    "sin(90 deg)",
    "sin(2 rad)",
    // refused today, and still
    "(1 deg) => sr",
    "(1 rad) + (1 sr)",
    "(1 deg) + (1 sr)",
    "(1 rad) => dB",
    "sin(9.81 m/s^2)",
    "(1 rad/s) => sr/s",
    "(1 sr) => rad",
];

fn answer(source: &str) -> String {
    match eval_phs(source) {
        Ok(values) => match values.last() {
            Some(value) => format!("ok {value}"),
            None => "ok".to_string(),
        },
        Err(_) => "error".to_string(),
    }
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("tests").join("golden").join("angle_mode_strict.tsv")
}

fn render() -> String {
    CORPUS.iter().map(|source| format!("{source}\t{}\n", answer(source))).collect()
}

#[test]
fn the_answers_are_the_ones_recorded() {
    if std::env::var_os("RECORD_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        std::fs::write(golden_path(), render()).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(golden_path())
        .expect("tests/golden/angle_mode_strict.tsv is missing: record it with RECORD_GOLDEN=1");
    assert_eq!(render(), golden.replace("\r\n", "\n"));
}
