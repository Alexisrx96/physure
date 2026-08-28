use std::process::Command;
use std::fs;
use std::io::Write;
use std::path::Path;

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
fn test_version_flag_prints_the_crate_version() {
    let output = Command::new(get_phs_bin())
        .arg("--version")
        .output()
        .expect("Failed to execute phs binary");

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "Expected the crate version in output, got: {}", stdout);
    assert!(stdout.starts_with("phs "), "Expected output to start with 'phs ', got: {}", stdout);
}

#[test]
fn test_runtime_error_reports_line_number_and_source_text() {
    // The failing statement is line 3 -- distinct from line 1, so a fix that only reports
    // "the first line" or a hardcoded 1 would not pass this.
    let temp_file = "temp_test_runtime_error_line.phs";
    let mut file = fs::File::create(temp_file).unwrap();
    file.write_all(b"a = 1.0 m\nb = 2.0 kg\nc = a + b\n").unwrap();

    let output = Command::new(get_phs_bin())
        .arg(temp_file)
        .output()
        .expect("Failed to execute phs binary");

    fs::remove_file(temp_file).unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("line 3"), "Expected the error to name line 3, got: {}", stderr);
    assert!(stderr.contains("a + b"), "Expected the error to show the failing line's own source text, got: {}", stderr);
}

#[test]
fn test_bare_expression_statements_are_labeled_by_their_own_source_not_a_generic_name() {
    // Two bare (unassigned) expression statements -- both used to print under the same
    // generic "expr" label, with no way to tell which value came from which line.
    let temp_file = "temp_test_bare_expr_labels.phs";
    let mut file = fs::File::create(temp_file).unwrap();
    file.write_all(b"a = 50.0 %\nb = 25.0 %\na / b\na * b\n").unwrap();

    let output = Command::new(get_phs_bin())
        .arg(temp_file)
        .output()
        .expect("Failed to execute phs binary");

    fs::remove_file(temp_file).unwrap();

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("a / b"), "Expected the division statement to be labeled by its own source, got: {}", stdout);
    assert!(stdout.contains("a * b"), "Expected the multiplication statement to be labeled by its own source, got: {}", stdout);
}

#[test]
fn test_bare_string_expressions_print_as_plain_text_not_a_label_equals_value_card() {
    // A bare (unassigned) string-template statement is a print, not a named quantity -- it
    // used to show up as `"<raw template source>" = <computed text>`, repeating the message
    // twice (once as an un-interpolated template, once as the real interpolated result) with
    // a confusing "=" between them.
    let temp_file = "temp_test_bare_string_print.phs";
    let mut file = fs::File::create(temp_file).unwrap();
    file.write_all(b"a = 5.0 m\n\"The value is {a}\"\n").unwrap();

    let output = Command::new(get_phs_bin())
        .arg(temp_file)
        .output()
        .expect("Failed to execute phs binary");

    fs::remove_file(temp_file).unwrap();

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("The value is 5.0 m"), "Expected the computed text, got: {}", stdout);
    assert!(!stdout.contains("{a}"), "Expected no raw, un-interpolated template source in the output, got: {}", stdout);
    assert!(!stdout.contains("\" = "), "Expected no label/equals-sign card for a bare string print, got: {}", stdout);

    // An *assigned* string is a real named value and must keep its label and "=".
    assert!(stdout.contains("a") && stdout.contains("5.0 m"), "Expected the assignment 'a = 5.0 m' to still render normally, got: {}", stdout);
}

#[test]
fn test_precision_decorator_overrides_sig_figs_on_an_uncertain_value() {
    let temp_file = "temp_test_precision_uncertain.phs";
    let mut file = fs::File::create(temp_file).unwrap();
    file.write_all(b"@precision(3)\nx = 40.0195264839553 +/- 40.0195264839553\n").unwrap();

    let output = Command::new(get_phs_bin())
        .arg(temp_file)
        .output()
        .expect("Failed to execute phs binary");

    fs::remove_file(temp_file).unwrap();

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    // 3 sig figs of 40.0195264839553 is 40.0; the mean (equal to the uncertainty here) rounds
    // to the same decimal place.
    assert!(stdout.contains("40.0"), "Expected 3-sig-fig rounding, got: {}", stdout);
}

#[test]
fn test_precision_decorator_sets_decimal_places_on_an_exact_value() {
    let temp_file = "temp_test_precision_exact.phs";
    let mut file = fs::File::create(temp_file).unwrap();
    file.write_all(b"@precision(2)\nx = 3.14159265\n").unwrap();

    let output = Command::new(get_phs_bin())
        .arg(temp_file)
        .output()
        .expect("Failed to execute phs binary");

    fs::remove_file(temp_file).unwrap();

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("3.14"), "Expected 2 decimal places on an exact value, got: {}", stdout);
    assert!(!stdout.contains("3.14159265"), "Expected the full-precision value not to leak through, got: {}", stdout);
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

#[test]
fn test_output_flag_alone_does_not_trigger_transpile_mode() {
    // `-o`/`--output` used to gate transpile mode all on its own, with neither the
    // `transpile` subcommand nor `--target` present -- so any other feature reusing `-o`
    // for its own purpose (the HTML report, below) silently transpiled to Rust instead.
    let temp_file = "temp_test_output_flag_no_transpile.phs";
    let bogus_output = "temp_test_output_flag_no_transpile_out.txt";
    let _ = fs::remove_file(bogus_output);
    let mut file = fs::File::create(temp_file).unwrap();
    file.write_all(b"m = 75.0 kg\n").unwrap();

    let output = Command::new(get_phs_bin())
        .args([temp_file, "-o", bogus_output])
        .output()
        .expect("Failed to execute phs binary");

    fs::remove_file(temp_file).unwrap();
    let existed = Path::new(bogus_output).exists();
    let _ = fs::remove_file(bogus_output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("Transpiled"), "Expected plain execution, not a transpile, got: {}", stdout);
    assert!(!existed, "-o alone should not have written a transpiled file at {bogus_output:?}");
    assert!(stdout.contains("75"), "Expected the script to actually run, got: {}", stdout);
}

#[test]
fn test_html_report_saves_next_to_the_script_by_default() {
    // PHS_NO_OPEN keeps this from popping an actual browser window during the test run.
    let temp_file = "temp_test_html_default_name.phs";
    let expected_output = "temp_test_html_default_name.html";
    let _ = fs::remove_file(expected_output);
    let mut file = fs::File::create(temp_file).unwrap();
    file.write_all(b"m = 75.0 kg\n").unwrap();

    let output = Command::new(get_phs_bin())
        .arg(temp_file)
        .arg("--html")
        .env("PHS_NO_OPEN", "1")
        .output()
        .expect("Failed to execute phs binary");

    fs::remove_file(temp_file).unwrap();

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(
        Path::new(expected_output).exists(),
        "Expected the report at {expected_output:?} (next to the script, not a random temp file) -- stdout: {}",
        String::from_utf8_lossy(&output.stdout),
    );
    let _ = fs::remove_file(expected_output);
}

#[test]
fn test_html_report_respects_output_flag() {
    let temp_file = "temp_test_html_custom_name.phs";
    let custom_output = "temp_test_html_custom_report.html";
    let _ = fs::remove_file(custom_output);
    let mut file = fs::File::create(temp_file).unwrap();
    file.write_all(b"m = 75.0 kg\n").unwrap();

    let output = Command::new(get_phs_bin())
        .args([temp_file, "--html", "-o", custom_output])
        .env("PHS_NO_OPEN", "1")
        .output()
        .expect("Failed to execute phs binary");

    fs::remove_file(temp_file).unwrap();

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("Transpiled"), "Expected an HTML report, not a transpile, got: {}", stdout);
    assert!(
        Path::new(custom_output).exists(),
        "Expected the report at the -o path {custom_output:?} -- stdout: {}",
        stdout,
    );
    let content = fs::read_to_string(custom_output).unwrap();
    let _ = fs::remove_file(custom_output);
    assert!(content.contains("<html") || content.contains("<!DOCTYPE"), "Expected real HTML content, got: {}", &content[..content.len().min(200)]);
}

#[test]
fn test_pack_subcommand_creates_bundle() {
    let temp_dir = std::env::temp_dir().join(format!("phs_cli_pack_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();

    let manifest = r#"
[package]
name = "cli-test-pkg"
version = "1.0.0"

[exports]
calc = "calc.phs"
"#;
    fs::write(temp_dir.join("phs.toml"), manifest).unwrap();
    fs::write(temp_dir.join("calc.phs"), "fn add(a, b) = a + b\n").unwrap();

    let out_bundle = temp_dir.join("bundle.phspkg");
    let output = Command::new(get_phs_bin())
        .args(["pack", temp_dir.to_str().unwrap(), "-o", out_bundle.to_str().unwrap()])
        .output()
        .expect("Failed to execute phs binary");

    assert!(output.status.success(), "Command failed with stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Successfully packed"), "Expected pack success message, got: {}", stdout);
    assert!(out_bundle.is_file(), "Expected output bundle file to exist");

    let bundle_content = fs::read_to_string(&out_bundle).unwrap();
    assert!(bundle_content.contains("cli-test-pkg"));
    assert!(bundle_content.contains("calc"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_serve_subcommand_via_cli() {
    let temp_dir = std::env::temp_dir().join(format!("phs_cli_serve_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir_all(&temp_dir).unwrap();
    fs::write(temp_dir.join("math.phs"), "fn double(x: m) = x * 2\n").unwrap();

    let port = 19876u16;
    let mut child = Command::new(get_phs_bin())
        .args(["serve", temp_dir.to_str().unwrap(), "--port", &port.to_string()])
        .spawn()
        .expect("Failed to spawn phs serve");

    // Wait a brief moment for server to bind
    std::thread::sleep(std::time::Duration::from_millis(500));

    let url = format!("http://127.0.0.1:{}/health", port);
    let health_resp = ureq::get(&url).call();

    // Kill the server process
    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_dir_all(&temp_dir);

    assert!(health_resp.is_ok(), "Failed to query health endpoint on spawned server");
    let health_json: serde_json::Value = health_resp.unwrap().into_json().unwrap();
    assert_eq!(health_json["status"], "ok");
    assert_eq!(health_json["modules_loaded"], 1);
}


