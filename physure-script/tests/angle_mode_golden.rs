//! A golden file that pins what the angle work must not change.
//!
//! `tests/golden/angle_mode_strict.tsv` (at the repository root) holds, for every expression of `CORPUS`, what PHS
//! answered on `main` @ `b27d0f8`, before the angle mode existed: the verdict and, for a success, the value, and for a
//! refusal, the text of the error. Once the angle mode exists, the strict mode (`Dimension`) must give the same rows,
//! byte for byte: that is the claim that the strict mode is today's behaviour.
//!
//! The file was written once, on a tree that had no angle mode yet. Recording refuses to overwrite an existing file, so
//! delete it by hand before recording again. On Bash:
//!
//!     RECORD_GOLDEN=1 cargo test -p physure-script --test angle_mode_golden
//!
//! On PowerShell, set the variable first and remove it afterwards:
//!
//!     $env:RECORD_GOLDEN = "1"
//!     cargo test -p physure-script --test angle_mode_golden
//!     Remove-Item Env:RECORD_GOLDEN
//!
//! A recorded value depends on the number formatting and on the error text of the core, so the file may only be
//! re-recorded on a tree that has no angle code (a tree whose `physure-core` and `physure-script/src` are those of
//! `b27d0f8`), never once `physure-core/src/units/angle_mode.rs` exists.

use physure_core::units::angle_mode::{scoped, AngleMode};
use physure_script::interpreter::eval_phs;
use std::fs::OpenOptions;
use std::io::Write;
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
    let text = match eval_phs(source) {
        Ok(values) => match values.last() {
            Some(value) => format!("ok {value}"),
            None => "ok".to_string(),
        },
        Err(e) => format!("error {e}"),
    };
    assert!(
        !text.contains('\n') && !text.contains('\t'),
        "the answer to {source:?} holds a newline or a tab, which would break the rows of the golden file: {text:?}",
    );
    text
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("tests").join("golden").join("angle_mode_strict.tsv")
}

fn render() -> String {
    let _strict = scoped(AngleMode::Dimension);
    CORPUS.iter().map(|source| format!("{source}\t{}\n", answer(source))).collect()
}

#[test]
fn the_answers_are_the_ones_recorded() {
    let current = render();
    if std::env::var("RECORD_GOLDEN").as_deref() == Ok("1") {
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(golden_path())
            .expect("the golden exists: delete it by hand to record again");
        file.write_all(current.as_bytes()).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(golden_path())
        .expect("tests/golden/angle_mode_strict.tsv is missing: record it with RECORD_GOLDEN=1");
    let golden = golden.replace("\r\n", "\n");
    let golden_rows: Vec<&str> = golden.lines().collect();
    let current_rows: Vec<&str> = current.lines().collect();
    assert_eq!(
        current_rows.len(),
        golden_rows.len(),
        "the corpus has {} rows and the golden file has {}",
        current_rows.len(),
        golden_rows.len(),
    );
    let disagreements: Vec<String> = current_rows
        .iter()
        .zip(&golden_rows)
        .filter(|(current_row, golden_row)| current_row != golden_row)
        .map(|(current_row, golden_row)| {
            let (source, got) = current_row.split_once('\t').expect("every row is a source and an answer");
            match golden_row.split_once('\t') {
                Some((golden_source, expected)) if golden_source == source => {
                    format!("{source}: expected {expected}, got {got}")
                }
                _ => format!("{source}: expected the row {golden_row:?}, got {got}"),
            }
        })
        .collect();
    assert!(
        disagreements.is_empty(),
        "{} disagreements with the golden file:\n{}",
        disagreements.len(),
        disagreements.join("\n"),
    );
}
