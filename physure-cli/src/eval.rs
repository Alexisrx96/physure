//! `phs eval <file|-> [--seed N] [--timeout-ms N]` and `phs unit <symbol>`: the structured answers for agents.
//!
//! `eval` runs the program in the sandbox of `physure_script::limits` (no files, no plugins, bounded recursion, a
//! deadline, a cap on the output) and prints one JSON document, `physure_script::report::Report`. The exit code is
//! 0 when the report is `ok`, 1 when it holds an error, 2 for a usage error or a file that cannot be read.
//! The deadline is cooperative; a watchdog ends the process if the evaluation does not honour it.
//!
//! `eval` is the deterministic surface: its Monte Carlo draws come from `--seed N` when given, otherwise from the
//! integer `[Settings] monte_carlo_seed` in force, and from seed 0 when that setting is `random`. The document says
//! which (`"seed": {"value": 0, "source": "flag" | "setting" | "default"}`).

use physure_script::report::{self, Report, SeedUsed};
use physure_script::Limits;
use std::io::Read;
use std::time::Duration;

const DEFAULT_TIMEOUT_MS: u64 = 5_000;
/// How long after the deadline the watchdog gives the evaluation to stop on its own.
const WATCHDOG_GRACE: Duration = Duration::from_secs(2);

/// What a command prints and the exit code it ends with.
pub struct Outcome {
    pub json: String,
    pub exit_code: i32,
}

/// The program text of `path`, or of standard input for `-`.
pub fn read_source(path: &str) -> std::io::Result<String> {
    if path == "-" {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        Ok(text)
    } else {
        std::fs::read_to_string(path)
    }
}

struct EvalArgs {
    path: String,
    /// `--seed N`; `None` reads the setting in force.
    seed: Option<u64>,
    timeout: Duration,
}

fn parse_args(args: &[String]) -> Result<EvalArgs, String> {
    let mut path = None;
    let (mut seed, mut timeout_ms) = (None, DEFAULT_TIMEOUT_MS);
    let mut rest = args.iter().skip(2);
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--seed" => seed = Some(rest.next().and_then(|v| v.parse().ok()).ok_or("--seed needs a whole number")?),
            "--timeout-ms" => timeout_ms = rest.next().and_then(|v| v.parse().ok()).ok_or("--timeout-ms needs a whole number")?,
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other if path.is_none() => path = Some(other.to_string()),
            other => return Err(format!("unexpected argument {other}")),
        }
    }
    let path = path.ok_or("usage: phs eval <file.phs|-> [--seed N] [--timeout-ms N]")?;
    Ok(EvalArgs { path, seed, timeout: Duration::from_millis(timeout_ms) })
}

pub fn run_eval(args: &[String]) {
    let parsed = parse_args(args).unwrap_or_else(|message| {
        eprintln!("{message}");
        std::process::exit(2);
    });
    let source = read_source(&parsed.path).unwrap_or_else(|error| {
        eprintln!("{}: cannot read: {error}", parsed.path);
        std::process::exit(2);
    });
    let outcome = evaluate_text(&source, parsed.seed, parsed.timeout, WATCHDOG_GRACE);
    println!("{}", outcome.json);
    std::process::exit(outcome.exit_code);
}

/// Evaluates `source` in the sandbox under `seed` (`--seed`, or `None` for the setting in force: see the module
/// documentation) and renders the report. If the evaluation has not returned `grace` after its deadline the report
/// says so (the caller ends the process, which ends the stuck thread).
pub fn evaluate_text(source: &str, seed: Option<u64>, timeout: Duration, grace: Duration) -> Outcome {
    let source = source.to_string();
    let seed = SeedUsed::resolve(seed);
    let report = with_watchdog(move || report::evaluate(&source, Limits::sandboxed(timeout), seed.value), timeout + grace);
    outcome_of(&report, seed)
}

/// The report as JSON, with the seed it was evaluated under after its other fields.
fn outcome_of(report: &Report, seed: SeedUsed) -> Outcome {
    let mut json = serde_json::to_value(report).unwrap_or_default();
    if let Some(object) = json.as_object_mut() {
        object.insert("seed".to_string(), serde_json::to_value(seed).unwrap_or_default());
    }
    Outcome { json: serde_json::to_string_pretty(&json).unwrap_or_default(), exit_code: if report.ok { 0 } else { 1 } }
}

/// Runs `work` on its own thread and returns its report, or a report that says it did not return within `limit`.
fn with_watchdog(work: impl FnOnce() -> Report + Send + 'static, limit: Duration) -> Report {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver.recv_timeout(limit).unwrap_or_else(|_| Report {
        ok: false,
        results: Vec::new(),
        diagnostics: vec![report::Finding {
            line: 0,
            severity: "error",
            code: "limit".to_string(),
            message: format!("the evaluation did not stop {} ms after its deadline", limit.as_millis()),
        }],
    })
}

pub fn run_unit(args: &[String]) {
    let Some(symbol) = args.get(2) else {
        eprintln!("usage: phs unit <symbol>");
        std::process::exit(2);
    };
    let outcome = unit_json(symbol);
    println!("{}", outcome.json);
    std::process::exit(outcome.exit_code);
}

pub fn unit_json(symbol: &str) -> Outcome {
    match report::describe_unit(symbol) {
        Ok(description) => Outcome { json: serde_json::to_string_pretty(&description).unwrap_or_default(), exit_code: 0 },
        Err(message) => Outcome { json: serde_json::json!({ "error": message }).to_string(), exit_code: 1 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(source: &str) -> (serde_json::Value, i32) {
        let outcome = evaluate_text(source, None, Duration::from_secs(30), Duration::from_secs(5));
        (serde_json::from_str(&outcome.json).expect("the output is JSON"), outcome.exit_code)
    }

    /// The seed the report says it used: `--seed` wins; without it the setting in force (scoped here), and 0 when
    /// that is `random`.
    #[test]
    fn the_report_says_which_seed_it_used_and_where_it_came_from() {
        use physure_core::uncertainty::{scoped_monte_carlo_seed, MonteCarloSeed};
        let seed_of = |flag: Option<u64>| {
            let outcome = evaluate_text("1 m", flag, Duration::from_secs(30), Duration::from_secs(5));
            let json: serde_json::Value = serde_json::from_str(&outcome.json).unwrap();
            assert_eq!(json["ok"], true, "the other fields are kept");
            assert!(json["results"].is_array() && json["diagnostics"].is_array());
            (json["seed"]["value"].as_u64().unwrap(), json["seed"]["source"].as_str().unwrap().to_string())
        };
        let _five = scoped_monte_carlo_seed(MonteCarloSeed::Fixed(5));
        assert_eq!(seed_of(None), (5, "setting".to_string()));
        assert_eq!(seed_of(Some(3)), (3, "flag".to_string()));
        let _random = scoped_monte_carlo_seed(MonteCarloSeed::Random);
        assert_eq!(seed_of(None), (0, "default".to_string()));
        assert_eq!(seed_of(Some(0)), (0, "flag".to_string()));
    }

    #[test]
    fn a_good_program_is_ok_with_exit_0() {
        let (json, code) = eval("d = 120 m\nt = 8 s\nd / t");
        assert_eq!(code, 0);
        assert_eq!(json["ok"], true);
        assert_eq!(json["results"][2]["dimension"], "L·T⁻¹");
        assert_eq!(json["results"][2]["value"], 15.0);
    }

    #[test]
    fn a_dimension_error_is_a_finding_with_exit_1() {
        let (json, code) = eval("1 m + 1 s");
        assert_eq!(code, 1);
        assert_eq!(json["ok"], false);
        assert_eq!(json["diagnostics"][0]["code"], "dimension-mismatch");
        assert_eq!(json["diagnostics"][0]["line"], 1);
    }

    #[test]
    fn a_syntax_error_is_a_syntax_finding() {
        let (json, code) = eval("x = (1 +\n");
        assert_eq!(code, 1);
        assert_eq!(json["diagnostics"][0]["code"], "syntax");
    }

    #[test]
    fn an_unknown_unit_is_a_finding() {
        let (json, code) = eval("5 fooz");
        assert_eq!(code, 1);
        assert_eq!(json["diagnostics"][0]["code"], "unknown-name");
    }

    #[test]
    fn a_runaway_recursion_ends_in_a_limit_finding_and_not_a_crash() {
        let (json, code) = eval("fn f(x) = f(x)\nf(1)");
        assert_eq!(code, 1);
        assert_eq!(json["diagnostics"][0]["code"], "limit");
    }

    #[test]
    fn the_watchdog_reports_an_evaluation_that_never_returns() {
        let report = with_watchdog(
            || {
                std::thread::sleep(Duration::from_secs(10));
                unreachable!("the watchdog answers first")
            },
            Duration::from_millis(100),
        );
        assert!(!report.ok);
        assert_eq!(report.diagnostics[0].code, "limit");
        assert!(report.diagnostics[0].message.contains("did not stop"), "{}", report.diagnostics[0].message);
    }

    #[test]
    fn the_flags_are_read_and_a_bad_one_is_refused() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let ok = parse_args(&args(&["phs", "eval", "a.phs", "--seed", "7", "--timeout-ms", "250"])).unwrap();
        assert_eq!((ok.path.as_str(), ok.seed, ok.timeout), ("a.phs", Some(7), Duration::from_millis(250)));
        assert_eq!(parse_args(&args(&["phs", "eval", "-"])).unwrap().path, "-");
        assert_eq!(parse_args(&args(&["phs", "eval", "-"])).unwrap().seed, None, "no flag: the setting in force");
        assert!(parse_args(&args(&["phs", "eval"])).is_err());
        assert!(parse_args(&args(&["phs", "eval", "a.phs", "--seed", "x"])).is_err());
        assert!(parse_args(&args(&["phs", "eval", "a.phs", "--nope"])).is_err());
    }

    #[test]
    fn unit_describes_a_unit_and_refuses_an_unknown_one() {
        let km = unit_json("km");
        assert_eq!(km.exit_code, 0);
        let json: serde_json::Value = serde_json::from_str(&km.json).unwrap();
        assert_eq!((json["dimension"].as_str(), json["kind"].as_str()), (Some("L"), Some("delta")));
        let unknown = unit_json("fooz");
        assert_eq!(unknown.exit_code, 1);
        assert!(unknown.json.contains("unknown unit"));
    }
}
