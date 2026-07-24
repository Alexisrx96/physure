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
