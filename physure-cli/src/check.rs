//! `phs check <file>... [--json]`: the dimensional findings of `physure_script::check`, without running the script.
//!
//! One line per finding, `file:line: error[code]: message` (warnings say `warning`), or with `--json` an array
//! of `{file, line, severity, code, message}`. The exit code is 0 when there is no error (warnings do not
//! fail), 1 when there is at least one, and 2 when a file cannot be read or does not parse.

use physure_script::{check_source, Severity};
use std::fs;

/// What `phs check` prints and the exit code it ends with.
pub struct Report {
    pub output: String,
    pub exit_code: i32,
}

pub fn run_check(args: &[String]) {
    let json = args.iter().any(|a| a == "--json");
    let files: Vec<&str> = args.iter().skip(2).filter(|a| !a.starts_with("--")).map(String::as_str).collect();
    if files.is_empty() {
        eprintln!("usage: phs check <file.phs>... [--json]");
        std::process::exit(2);
    }
    let report = check_files(&files, json);
    if !report.output.is_empty() {
        println!("{}", report.output);
    }
    std::process::exit(report.exit_code);
}

pub fn check_files(files: &[&str], json: bool) -> Report {
    let mut lines: Vec<String> = Vec::new();
    let mut records: Vec<serde_json::Value> = Vec::new();
    let mut exit_code = 0;
    for file in files {
        let source = match fs::read_to_string(file) {
            Ok(source) => source,
            Err(error) => {
                lines.push(format!("{file}: cannot read: {error}"));
                exit_code = 2;
                continue;
            }
        };
        let findings = match check_source(&source) {
            Ok(findings) => findings,
            Err(error) => {
                lines.push(format!("{file}: does not parse: {error}"));
                exit_code = 2;
                continue;
            }
        };
        for finding in findings {
            let severity = if finding.severity == Severity::Error { "error" } else { "warning" };
            if finding.severity == Severity::Error && exit_code == 0 {
                exit_code = 1;
            }
            lines.push(format!("{file}:{}: {severity}[{}]: {}", finding.line, finding.code, finding.message));
            records.push(serde_json::json!({
                "file": file, "line": finding.line, "severity": severity, "code": finding.code, "message": finding.message,
            }));
        }
    }
    let output = if json { serde_json::to_string_pretty(&records).unwrap_or_default() } else { lines.join("\n") };
    // a file that could not be read or parsed is a line of text even in JSON mode: say so on stderr
    if json && exit_code == 2 {
        eprintln!("{}", lines.iter().filter(|l| l.contains("cannot read") || l.contains("does not parse")).cloned().collect::<Vec<_>>().join("\n"));
    }
    Report { output, exit_code }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_with(name: &str, source: &str) -> String {
        let path = std::env::temp_dir().join(format!("phs-check-{}-{name}", std::process::id()));
        fs::write(&path, source).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn a_clean_file_prints_nothing_and_exits_0() {
        let path = file_with("clean.phs", "a = 1 m\nb = 2 m\na + b\n");
        let report = check_files(&[&path], false);
        assert_eq!((report.output.as_str(), report.exit_code), ("", 0));
    }

    #[test]
    fn an_error_is_a_line_of_text_and_exit_1() {
        let path = file_with("error.phs", "a = 1 m\nb = 2 s\na + b\n");
        let report = check_files(&[&path], false);
        assert_eq!(report.exit_code, 1);
        assert!(report.output.contains(":3: error[dimension-mismatch]: "), "{}", report.output);
        assert!(report.output.starts_with(&path), "{}", report.output);
    }

    #[test]
    fn a_warning_does_not_fail() {
        let path = file_with("warning.phs", "max(1 m, 1 s)\n");
        let report = check_files(&[&path], false);
        assert_eq!(report.exit_code, 0);
        assert!(report.output.contains("warning[mixed-extremum]"), "{}", report.output);
    }

    #[test]
    fn json_is_an_array_of_findings() {
        let path = file_with("json.phs", "1 m + 1 s\n");
        let report = check_files(&[&path], true);
        let parsed: serde_json::Value = serde_json::from_str(&report.output).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 1);
        assert_eq!(parsed[0]["line"], 1);
        assert_eq!(parsed[0]["severity"], "error");
        assert_eq!(parsed[0]["code"], "dimension-mismatch");
        assert_eq!(parsed[0]["file"], path.as_str());
        assert_eq!(check_files(&[&file_with("jsonclean.phs", "1 m + 1 m\n")], true).output, "[]");
    }

    #[test]
    fn a_file_that_cannot_be_read_or_parsed_exits_2() {
        assert_eq!(check_files(&["no/such/file.phs"], false).exit_code, 2);
        let path = file_with("broken.phs", "1 +");
        let report = check_files(&[&path], false);
        assert_eq!(report.exit_code, 2);
        assert!(report.output.contains("does not parse"), "{}", report.output);
    }

    #[test]
    fn several_files_are_all_checked_and_the_worst_exit_code_wins() {
        let good = file_with("one.phs", "1 m + 1 m\n");
        let bad = file_with("two.phs", "1 m + 1 s\n");
        let report = check_files(&[&good, &bad], false);
        assert_eq!(report.exit_code, 1);
        assert!(report.output.contains("two.phs:1:"), "{}", report.output);
        assert!(!report.output.contains("one.phs:"), "{}", report.output);
    }
}
