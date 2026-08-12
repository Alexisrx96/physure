use std::process::Command;
use std::fs;
use std::io::Write;

fn get_phs_bin() -> String {
    env!("CARGO_BIN_EXE_phs").to_string()
}

#[test]
fn test_phs_file_execution() {
    let temp_file = "temp_test_script.phs";
    let mut file = fs::File::create(temp_file).unwrap();
    file.write_all(b"500 N / 2 m^2 => kPa").unwrap();

    let output = Command::new(get_phs_bin())
        .arg(temp_file)
        .output()
        .expect("Failed to execute phs binary");

    fs::remove_file(temp_file).unwrap();

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0.25"), "Expected output to contain '0.25', got: {}", stdout);
}

#[test]
fn test_phs_missing_file() {
    let output = Command::new(get_phs_bin())
        .arg("non_existent_file.phs")
        .output()
        .expect("Failed to execute phs binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error"));
}

#[test]
fn test_cli_run_subcommand() {
    let output = Command::new(get_phs_bin())
        .arg("2 + 2")
        .output()
        .expect("Failed to execute phs binary");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("4"));
}

#[test]
fn test_cli_advanced_script() {
    let temp_file = "temp_test_advanced_script.phs";
    let mut file = fs::File::create(temp_file).unwrap();
    file.write_all(b"use deriv, integral from calc\n\
v = 3.0 m/s\n\
d = deriv(\"v^2\", \"v\")\n\
i = integral(\"v\", \"v\")\n\
36 km/h => m/s\n\
").unwrap();

    let output = Command::new(get_phs_bin())
        .arg(temp_file)
        .output()
        .expect("Failed to execute phs binary");

    fs::remove_file(temp_file).unwrap();

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("10"));
}

#[test]
fn test_cli_transpile_typescript() {
    let temp_file = "temp_test_transpile_ts.phs";
    let temp_output = "temp_test_transpile_ts.ts";
    let mut file = fs::File::create(temp_file).unwrap();
    file.write_all(b"m = 75.0 kg\n").unwrap();

    let output = Command::new(get_phs_bin())
        .args(["transpile", temp_file, "--target", "ts", "-o", temp_output])
        .output()
        .expect("Failed to execute phs binary");

    fs::remove_file(temp_file).unwrap();

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("TypeScript"), "Expected output to contain 'TypeScript', got: {}", stdout);

    let generated = fs::read_to_string(temp_output).unwrap();
    let _ = fs::remove_file(temp_output);

    assert!(generated.contains("import { Quantity } from \"physure\";"), "Expected generated code to import Quantity, got: {}", generated);
    assert!(generated.contains("const m: Quantity = Quantity.of(75, \"kg\");"), "Expected generated code to declare typed const, got: {}", generated);
}
