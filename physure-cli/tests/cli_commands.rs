//! The subcommands of `phs`, driven as the subprocess a user runs: help, the REPL, the editor daemon, the
//! language guide, the transpiler, the plugin scaffold, the checker, `eval`, `unit`, the debugger, `pack` and the
//! model server with its authentication.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn phs() -> Command {
    Command::new(env!("CARGO_BIN_EXE_phs"))
}

/// A fresh directory that is removed when the test ends, even by a failed assertion.
struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("phs_cli_commands_{label}_{}_{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let file = self.0.join(name);
        std::fs::write(&file, text).unwrap();
        file
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Runs `phs` with `args` in `dir`, feeding `input` to its standard input.
fn run_with_input(dir: &Path, args: &[&str], input: &str) -> Output {
    let mut child = phs()
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn phs");
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn run_in(dir: &Path, args: &[&str]) -> Output {
    phs().args(args).current_dir(dir).output().expect("failed to run phs")
}

#[test]
fn help_lists_the_commands_however_it_is_asked_for() {
    let dir = TempDir::new("help");
    for args in [vec!["--help"], vec!["-h"], vec!["help"]] {
        let output = run_in(dir.path(), &args);
        assert!(output.status.success(), "{args:?}");
        let text = stdout(&output);
        assert!(text.contains("phs --repl") && text.contains("phs check") && text.contains("phs upgrade"), "{args:?}: {text}");
    }
    // with nothing to run it prints the same help
    assert!(stdout(&run_in(dir.path(), &[])).contains("phs --repl"));
}

#[test]
fn the_repl_evaluates_each_line_and_reports_what_it_cannot() {
    let dir = TempDir::new("repl");
    let output = run_with_input(dir.path(), &["--repl"], "m = 75 kg\nm * 2\nhelp\nunknown_function(\n1 m + 1 s\nexit\n");
    let text = stdout(&output);
    assert!(output.status.success());
    assert!(text.contains("=> 150.0 kg"), "{text}");
    assert!(text.contains("Enter physical expressions"), "{text}");
    assert!(text.contains("Goodbye!"), "{text}");
    let errors = stderr(&output);
    assert!(errors.contains("Parse Error") && errors.contains("Error:"), "{errors}");
}

/// A REPL session is one evaluation unit (conf freeze Phase 4b): the Monte Carlo stream of the seed setting goes on
/// from line to line, so a measurement typed later is independent of an earlier one (`x - y` keeps an uncertainty),
/// and the same session gives the same values in every process (it differed on every run before the setting).
#[test]
fn a_repl_session_draws_one_stream_of_the_seed_setting() {
    let dir = TempDir::new("repl_seed");
    dir.write("physure.conf", "[Settings]\npropagation_mode = monte_carlo\n");
    let session = "x = 10 +/- 1 m\ny = 10 +/- 1 m\nd = x - y\nd: base\nexit\n";
    let first = stdout(&run_with_input(dir.path(), &["--repl"], session));
    // `d: base` prints the full digits of `x - y`: an uncertainty of about sqrt(2), never the 0 of shared samples
    let base = first.lines().filter(|l| l.contains("=> ")).nth(3).unwrap_or_else(|| panic!("{first}"));
    let u: f64 = base
        .split('\u{b1}')
        .nth(1)
        .and_then(|t| t.trim().trim_end_matches(" m").parse().ok())
        .unwrap_or_else(|| panic!("{base}"));
    assert!((u - 2f64.sqrt()).abs() < 0.15, "{base}");
    assert_eq!(stdout(&run_with_input(dir.path(), &["--repl"], session)), first, "the same session, the same values");
    dir.write("physure.conf", "[Settings]\npropagation_mode = monte_carlo\nmonte_carlo_seed = random\n");
    let random = stdout(&run_with_input(dir.path(), &["--repl"], session));
    assert_ne!(stdout(&run_with_input(dir.path(), &["--repl"], session)), random);
}

/// A loop in `monte_carlo` mode stays on the thread of its evaluation unit (conf freeze Phase 4b), so its 20000
/// measurements are the next 20000 draws of the stream of the seed, in item order: the same bits with one rayon
/// worker and with eight, and on every run. (On the workers each item drew from the stream of whichever thread ran
/// it: three runs gave three values of the digest below.) `trapz` over all the items is the digest: the display of
/// a long vector shows three of them. `z`, drawn after the loops, goes on from where they left the stream.
#[test]
fn a_monte_carlo_loop_gives_the_same_bits_on_one_worker_and_on_eight() {
    let dir = TempDir::new("mc_loop_threads");
    dir.write("physure.conf", "[Settings]\npropagation_mode = monte_carlo\nmonte_carlo_samples = 8\n");
    dir.write(
        "loop.phs",
        "use * from array\nfn f(v) = v + (1 +/- 0.5 m)\np = parallel_map(f, vector(1 m, 2 m, 3 m, 4 m))\n\
         q = for i in 1..2001 { 5 +/- 1 m }\ns = trapz(q, linspace(0, 1, 2000))\ns: base\n\
         r = for i in 1..20001 { 10 +/- 1 m }\nt = trapz(r, linspace(0, 1, 20000))\nt: base\nz = 10 +/- 1 m\nz: base\n",
    );
    let run = |workers: &str| {
        let output = phs().arg("loop.phs").current_dir(dir.path()).env("RAYON_NUM_THREADS", workers).output().unwrap();
        assert!(output.status.success(), "{}{}", stdout(&output), stderr(&output));
        stdout(&output)
    };
    let one = run("1");
    assert!(one.contains("(20000 items)") && one.matches('\u{b1}').count() >= 10, "{one}");
    assert_eq!(run("8"), one, "eight workers");
    assert_eq!(run("8"), one, "and again");
    assert_eq!(run("3"), one, "three workers");
}

#[test]
fn the_repl_ends_at_the_end_of_its_input() {
    let dir = TempDir::new("repl_eof");
    let output = run_with_input(dir.path(), &["repl"], "1 + 1\n");
    assert!(stdout(&output).contains("Goodbye!"));
}

#[test]
fn the_daemon_answers_each_request_with_its_values_and_diagnostics() {
    let dir = TempDir::new("daemon");
    let requests = [
        r#"{"id": 1, "source": "x = 2 m + 3 m\nx * 2"}"#,
        r#"{"id": 2, "source": "1 m + 1 s"}"#,
        r#"{"id": 3, "source": "1 +"}"#,
        r#"{"id": 4, "source": "use * from plot\nplot(vector(1, 2, 3), vector(1, 4, 9))"}"#,
        "not json at all",
        "",
    ]
    .join("\n");
    let output = run_with_input(dir.path(), &["--daemon"], &requests);
    let answers: Vec<serde_json::Value> = stdout(&output).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(answers.len(), 4, "{answers:?}");
    assert_eq!(answers[0]["id"], 1);
    assert_eq!(answers[0]["results"][1]["output"], "10.0 m");
    assert!(answers[1]["diagnostics"][0]["message"].as_str().unwrap().contains("Unit mismatch"));
    assert_eq!(answers[2]["diagnostics"][0]["line"], 0);
    assert!(answers[3]["results"][0]["output"].as_str().unwrap().starts_with("[PLOT_IMAGE:"), "{}", answers[3]);
}

/// Each daemon request is one evaluation unit (conf freeze Phase 4b): it starts the Monte Carlo stream of the seed
/// setting, so the same source gives the same values in every request (an editor rendering a file twice shows the
/// same numbers; they differed on every render before the setting), and two measurements in it are independent.
#[test]
fn every_daemon_request_starts_the_stream_of_the_seed_setting() {
    let dir = TempDir::new("daemon_seed");
    dir.write("physure.conf", "[Settings]\npropagation_mode = monte_carlo\n");
    let request = |id: u32| {
        serde_json::json!({"id": id, "source": "x = 10 +/- 1 m\ny = 10 +/- 1 m\nx: base\ny: base\nx - y: base"}).to_string()
    };
    let answers = |input: String| -> Vec<serde_json::Value> {
        stdout(&run_with_input(dir.path(), &["--daemon"], &input)).lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    };
    let two = answers(format!("{}\n{}\n", request(1), request(2)));
    assert_eq!(two.len(), 2, "{two:?}");
    let results = two[0]["results"].as_array().unwrap();
    let outputs: Vec<&str> = results.iter().rev().take(3).rev().map(|r| r["output"].as_str().unwrap()).collect();
    assert_eq!(outputs.len(), 3, "{}", two[0]);
    assert_ne!(outputs[0], outputs[1], "two measurements, two sample sets");
    let u: f64 = outputs[2]
        .split('\u{b1}')
        .nth(1)
        .and_then(|t| t.trim().trim_end_matches(" m").parse().ok())
        .unwrap_or_else(|| panic!("{}", outputs[2]));
    assert!((u - 2f64.sqrt()).abs() < 0.15, "{}", outputs[2]);
    assert_eq!(two[1]["results"], two[0]["results"], "the same request, the same values");
    assert_eq!(answers(format!("{}\n", request(1)))[0]["results"], two[0]["results"], "and in another process");
    dir.write("physure.conf", "[Settings]\npropagation_mode = monte_carlo\nmonte_carlo_seed = random\n");
    let random = answers(format!("{}\n{}\n", request(1), request(2)));
    assert_ne!(random[0]["results"], random[1]["results"]);
}

#[test]
fn the_daemon_resolves_a_module_next_to_the_file_being_edited() {
    let dir = TempDir::new("daemon_path");
    dir.write("constants.phs", "export g = 9.81 m/s^2\n");
    let main = dir.path().join("main.phs");
    let request = serde_json::json!({"id": 7, "source": "use g from \"constants\"\ng * 2 s^2", "path": main.to_str().unwrap()});
    let output = run_with_input(dir.path(), &["daemon"], &format!("{request}\n"));
    let answer: serde_json::Value = serde_json::from_str(stdout(&output).lines().next().unwrap()).unwrap();
    assert_eq!(answer["id"], 7);
}

#[test]
fn the_language_guide_is_printed_or_saved() {
    let dir = TempDir::new("doc");
    let printed = stdout(&run_in(dir.path(), &["doc"]));
    assert!(printed.len() > 1000, "the guide is {} bytes", printed.len());
    let saved = run_in(dir.path(), &["--doc", "--save"]);
    assert!(stdout(&saved).contains("PHYSURE_LANGUAGE_GUIDE.md"), "{}", stdout(&saved));
    let file = std::fs::read_to_string(dir.path().join("PHYSURE_LANGUAGE_GUIDE.md")).unwrap();
    assert!(file.len() > 1000);
}

#[test]
fn the_committed_language_guide_is_what_phs_doc_prints() {
    let dir = TempDir::new("guide");
    let output = phs()
        .args(["doc"])
        .current_dir(dir.path())
        .env_remove("PHYSURE_CONF")
        .env_remove("PHYSURE_REQUIRE_FROZEN")
        .output()
        .expect("failed to run phs");
    let printed = stdout(&output);
    let committed = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("PHYSURE_LANGUAGE_GUIDE.md"))
        .expect("the committed guide");
    let normal = |text: &str| text.replace("\r\n", "\n").trim().to_string();
    assert!(
        normal(&printed) == normal(&committed),
        "PHYSURE_LANGUAGE_GUIDE.md is out of date: run `phs doc --save` from the repository root and commit the file"
    );
}

#[test]
fn the_guide_lists_every_reserved_name_and_every_module_function() {
    use physure_script::reserved::{CORE_FUNCTIONS, KEYWORDS, LITERALS, STATEMENT_WORDS};
    let dir = TempDir::new("reserved");
    let printed = stdout(&run_in(dir.path(), &["doc"]));
    let section = printed.split("### 1.7 Reserved names").nth(1).expect("the section 1.7 of the guide");
    let section = section.split("\n## ").next().unwrap_or(section);
    for name in KEYWORDS.iter().chain(&STATEMENT_WORDS).chain(&LITERALS).chain(&CORE_FUNCTIONS) {
        assert!(section.contains(&format!("`{name}`")), "the guide does not list `{name}`");
    }
    for domain in ["calc", "plot", "array"] {
        for name in physure_script::builtins::domain_members(domain).expect("a module") {
            assert!(section.contains(&format!("`{name}`")), "the guide does not list `{name}` of {domain}");
        }
    }
}

#[test]
fn the_transpiler_writes_the_file_of_each_target() {
    let dir = TempDir::new("transpile");
    dir.write("calc.phs", "mass = 2 kg\nv = 3 m/s\ne = 0.5 * mass * v^2\n");
    for (args, produced) in [
        (vec!["transpile", "calc.phs", "--target", "python"], "calc.py"),
        (vec!["transpile", "calc.phs", "--target", "java"], "calc.java"),
        (vec!["transpile", "calc.phs", "-t", "js"], "calc.js"),
        (vec!["transpile", "calc.phs", "-t", "typescript"], "calc.ts"),
        (vec!["transpile", "calc.phs", "-t", "rust"], "calc.rs"),
        (vec!["transpile", "calc.phs"], "calc.rs"),
    ] {
        let output = run_in(dir.path(), &args);
        assert!(output.status.success(), "{args:?}: {}", stderr(&output));
        assert!(stdout(&output).contains("Transpiled"), "{args:?}: {}", stdout(&output));
        let text = std::fs::read_to_string(dir.path().join(produced)).unwrap_or_else(|_| panic!("{produced} was not written"));
        assert!(!text.trim().is_empty(), "{produced}");
    }
}

#[test]
fn a_builtin_the_webassembly_quantity_lacks_stops_the_javascript_transpile() {
    let dir = TempDir::new("transpile_js_refusal");
    dir.write("calc.phs", "a = 4 m^2\nside = sqrt(a)\n");
    for target in ["js", "ts"] {
        let output = run_in(dir.path(), &["transpile", "calc.phs", "-t", target]);
        assert!(!output.status.success(), "{target}: {}", stdout(&output));
        let error = stderr(&output);
        assert!(error.contains("'sqrt'") && error.contains("WebAssembly"), "{target}: {error}");
    }
}

#[test]
fn the_transpile_target_follows_the_output_file_when_none_is_named() {
    let dir = TempDir::new("transpile_ext");
    dir.write("calc.phs", "e = 2 m * 3 m\n");
    for (output_name, marker) in [
        ("out.py", "from physure import"),
        ("Calculator.java", "class Calculator"),
        ("out.ts", "import { Quantity }"),
        ("out.js", "import { Quantity }"),
        ("out.txt", "fn main"),
    ] {
        let output = run_in(dir.path(), &["transpile", "calc.phs", "-o", output_name]);
        assert!(output.status.success(), "{output_name}: {}", stderr(&output));
        let text = std::fs::read_to_string(dir.path().join(output_name)).unwrap();
        assert!(text.contains(marker), "{output_name}: {text}");
    }
}

#[test]
fn the_transpiler_says_why_it_could_not() {
    let dir = TempDir::new("transpile_errors");
    dir.write("bad.phs", "1 +\n");
    let missing = run_in(dir.path(), &["transpile"]);
    assert!(!missing.status.success() && stderr(&missing).contains("missing script file path"), "{}", stderr(&missing));
    let absent = run_in(dir.path(), &["transpile", "nowhere.phs", "-t", "python"]);
    assert!(!absent.status.success() && stderr(&absent).contains("Error reading file"), "{}", stderr(&absent));
    let syntax = run_in(dir.path(), &["transpile", "bad.phs", "-t", "python"]);
    assert!(!syntax.status.success() && stderr(&syntax).contains("Error parsing script"), "{}", stderr(&syntax));
}

#[test]
fn a_plugin_scaffold_has_a_manifest_a_source_and_a_driver() {
    let dir = TempDir::new("scaffold");
    for (lang, expected) in [
        ("rust", vec!["ext/my_plugin.rs", "build_plugin.sh", "build_plugin.ps1", "build_plugin.cmd", "driver.py"]),
        ("python", vec!["ext/my_plugin.py", "driver.py"]),
        ("both", vec!["ext/my_plugin.rs", "ext/my_plugin.py", "build_plugin.sh", "driver.py"]),
    ] {
        let target = dir.path().join(lang);
        let output = run_in(dir.path(), &["new-plugin", "my_plugin", "--lang", lang, "--dir", target.to_str().unwrap()]);
        assert!(output.status.success(), "{lang}: {}", stderr(&output));
        assert!(stdout(&output).contains("next steps"), "{lang}: {}", stdout(&output));
        for file in expected {
            assert!(target.join(file).is_file(), "{lang}: {file} was not written");
        }
    }
    let source = std::fs::read_to_string(dir.path().join("rust").join("ext").join("my_plugin.rs")).unwrap();
    assert!(source.contains("my_plugin"), "{source}");
    let usage = run_in(dir.path(), &["new-plugin"]);
    assert!(!usage.status.success() && stderr(&usage).contains("Usage: phs new-plugin"), "{}", stderr(&usage));
    let unknown = run_in(dir.path(), &["new-plugin", "x", "--lang", "cobol"]);
    assert!(!unknown.status.success(), "{}", stderr(&unknown));
}

#[test]
fn the_checker_reports_dimensions_without_running_the_program() {
    let dir = TempDir::new("check");
    dir.write("good.phs", "d = 5 m + 3 m\n");
    dir.write("bad.phs", "d = 5 m + 3 s\n");
    dir.write("broken.phs", "d = 5 m +\n");
    assert!(run_in(dir.path(), &["check", "good.phs"]).status.success());

    let bad = run_in(dir.path(), &["check", "bad.phs"]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(stdout(&bad).contains("bad.phs:1: error[dimension-mismatch]"), "{}", stdout(&bad));

    let as_json = run_in(dir.path(), &["check", "bad.phs", "--json"]);
    let findings: serde_json::Value = serde_json::from_str(&stdout(&as_json)).unwrap();
    assert_eq!(findings[0]["code"], "dimension-mismatch");

    let unreadable = run_in(dir.path(), &["check", "broken.phs", "nowhere.phs"]);
    assert_eq!(unreadable.status.code(), Some(2));
    assert!(stdout(&unreadable).contains("does not parse") && stdout(&unreadable).contains("cannot read"), "{}", stdout(&unreadable));
}

#[test]
fn eval_and_unit_answer_in_json() {
    let dir = TempDir::new("eval");
    let evaluated = run_with_input(dir.path(), &["eval", "-"], "x = 2 m + 3 m\n1 m + 1 s\n");
    let report: serde_json::Value = serde_json::from_str(&stdout(&evaluated)).unwrap();
    assert_eq!(report["ok"], false);
    assert!(report["diagnostics"].as_array().unwrap().len() >= 1);

    let unit = run_in(dir.path(), &["unit", "kWh"]);
    let facts: serde_json::Value = serde_json::from_str(&stdout(&unit)).unwrap();
    assert!(facts.to_string().contains("dimension"), "{facts}");
}

#[test]
fn the_suggestion_for_an_unknown_unit_is_the_same_in_every_process() {
    // `barz` is one edit from `bar` and `barn`: each process used to suggest whichever its maps met first. (`x`, one
    // edit from `A`, `K`, `m` and `s`, showed it before; a one-letter name gets no unit hint since audit T5. The name
    // is assigned so that the checker, which flags a free name meeting a quantity, leaves the line to the interpreter.)
    let dir = TempDir::new("suggestion");
    for _ in 0..10 {
        let evaluated = run_with_input(dir.path(), &["eval", "-"], "barz = 5 barz\n");
        let report: serde_json::Value = serde_json::from_str(&stdout(&evaluated)).unwrap();
        assert_eq!(report["diagnostics"][0]["message"], "`barz` has no value — did you mean the unit 'bar'?", "{report}");
    }
}

#[test]
fn the_debugger_answers_each_command_at_a_breakpoint() {
    let dir = TempDir::new("debug");
    let script = dir.write(
        "sim.phs",
        "g = 9.81 m/s^2\nfn fall(t) =\n  d = 0.5 * g * t^2\n  d\nresult = fall(2 s)\n",
    );
    let commands = "print g\nprint 1 +\ninspect d\ninspect missing\nlocals\nglobals\nbacktrace\nunknown\nstep\nnext\nfinish\ncontinue\n";
    let output = run_with_input(dir.path(), &["debug", script.to_str().unwrap(), "--break", "4"], commands);
    let text = stdout(&output);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(text.contains("Paused at line"), "{text}");
    assert!(text.contains("could not parse"), "{text}");
    assert!(text.contains("no variable named 'missing'"), "{text}");
    assert!(text.contains("unknown command 'unknown'"), "{text}");
    assert!(text.contains("Program finished."), "{text}");
}

#[test]
fn the_debugger_takes_a_function_and_a_condition_to_break_on() {
    let dir = TempDir::new("debug_flags");
    let script = dir.write("sim.phs", "fn twice(x) = x * 2\na = twice(2)\nb = twice(3)\n");
    let by_function = run_with_input(dir.path(), &["debug", script.to_str().unwrap(), "--break-fn", "twice"], "backtrace\ncontinue\ncontinue\ncontinue\n");
    assert!(stdout(&by_function).contains("in twice()"), "{}", stdout(&by_function));
    let conditional = run_with_input(dir.path(), &["debug", script.to_str().unwrap(), "--break", "3:a > 1"], "continue\n");
    assert!(stdout(&conditional).contains("Program finished."), "{}", stdout(&conditional));
    let bad_flag = run_with_input(dir.path(), &["debug", script.to_str().unwrap(), "--break", "zero"], "");
    assert!(stderr(&bad_flag).contains("could not parse breakpoint"), "{}", stderr(&bad_flag));
    let usage = run_in(dir.path(), &["debug"]);
    assert!(!usage.status.success() && stderr(&usage).contains("Usage: phs debug"), "{}", stderr(&usage));
}

#[test]
fn pack_says_what_is_wrong_with_what_it_is_given() {
    let dir = TempDir::new("pack_errors");
    let empty = run_in(dir.path(), &["pack", "."]);
    assert!(!empty.status.success() && stderr(&empty).contains("phs.toml"), "{}", stderr(&empty));
    let absent = run_in(dir.path(), &["pack", "nowhere"]);
    assert!(!absent.status.success() && stderr(&absent).contains("does not exist"), "{}", stderr(&absent));
    dir.write("phs.toml", "this is not a manifest");
    let broken = run_in(dir.path(), &["pack", "."]);
    assert!(!broken.status.success() && stderr(&broken).contains("Error"), "{}", stderr(&broken));
}

/// The status of a ureq answer, whether it is a success or a refusal.
fn status(answer: Result<ureq::Response, ureq::Error>) -> u16 {
    match answer {
        Ok(response) => response.status(),
        Err(ureq::Error::Status(code, _)) => code,
        Err(other) => panic!("no answer: {other}"),
    }
}

#[test]
fn the_model_server_answers_a_token_holder_and_refuses_the_rest() {
    let dir = TempDir::new("serve");
    dir.write("math.phs", "fn double(x: m) = x * 2\nfn add(a: m, b: m) = a + b\n");
    let port = 19877u16;
    let mut server = phs()
        .args(["serve", dir.path().to_str().unwrap(), "--port", &port.to_string(), "--token", "secret"])
        .stdout(Stdio::null())
        .spawn()
        .expect("failed to spawn phs serve");
    std::thread::sleep(std::time::Duration::from_millis(800));
    let url = |path: &str| format!("http://127.0.0.1:{port}{path}");
    let authorized = |request: ureq::Request| request.set("Authorization", "Bearer secret");

    let outcomes = (|| {
        let no_token = status(ureq::get(&url("/health")).call());
        let wrong_token = status(ureq::get(&url("/health")).set("X-API-Key", "nope").call());
        let by_key = status(ureq::get(&url("/health")).set("X-API-Key", "secret").call());
        let health = authorized(ureq::get(&url("/health"))).call().unwrap().into_json::<serde_json::Value>().unwrap();
        let catalog = authorized(ureq::get(&url("/api/v1/catalog"))).call().unwrap().into_json::<serde_json::Value>().unwrap();
        let preflight = status(ureq::request("OPTIONS", &url("/api/v1/math/double")).call());
        let by_name = authorized(ureq::post(&url("/api/v1/math/double"))).send_json(serde_json::json!({"x": "3 m"})).unwrap().into_json::<serde_json::Value>().unwrap();
        let positional = authorized(ureq::post(&url("/api/v1/math/add"))).send_json(serde_json::json!({"args": ["1 m", "2 m"]})).unwrap().into_json::<serde_json::Value>().unwrap();
        let no_module = status(authorized(ureq::post(&url("/api/v1/nothing/double"))).send_json(serde_json::json!({})));
        let no_function = status(authorized(ureq::post(&url("/api/v1/math/nothing"))).send_json(serde_json::json!({})));
        let wrong_route = status(authorized(ureq::post(&url("/api/v1/math"))).send_json(serde_json::json!({})));
        let wrong_unit = status(authorized(ureq::post(&url("/api/v1/math/double"))).send_json(serde_json::json!({"x": "3 s"})));
        let missing = status(authorized(ureq::post(&url("/api/v1/math/add"))).send_json(serde_json::json!({"a": "1 m"})));
        let unexpected = status(authorized(ureq::post(&url("/api/v1/math/double"))).send_json(serde_json::json!({"x": "1 m", "y": "2 m"})));
        let not_json = status(authorized(ureq::post(&url("/api/v1/math/double"))).send_string("{"));
        let pipeline = authorized(ureq::post(&url("/api/v1/pipeline")))
            .send_json(serde_json::json!({"steps": [
                {"module": "math", "function": "double", "inputs": {"x": "2 m"}, "output": "a"},
                {"module": "math", "function": "add", "inputs": {"a": {"$ref": "a"}, "b": "1 m"}, "output": "b"},
            ]}))
            .unwrap()
            .into_json::<serde_json::Value>()
            .unwrap();
        let bad_pipeline = status(authorized(ureq::post(&url("/api/v1/pipeline"))).send_json(serde_json::json!({"steps": [
            {"module": "math", "function": "nothing", "inputs": {}, "output": "a"}]})));
        let unreadable_pipeline = status(authorized(ureq::post(&url("/pipeline"))).send_string("nope"));
        let unknown = status(authorized(ureq::get(&url("/nowhere"))).call());
        (no_token, wrong_token, by_key, health, catalog, preflight, by_name, positional, no_module, no_function, wrong_route, wrong_unit, missing, unexpected, not_json, pipeline, bad_pipeline, unreadable_pipeline, unknown)
    })();

    let _ = server.kill();
    let _ = server.wait();

    let (no_token, wrong_token, by_key, health, catalog, preflight, by_name, positional, no_module, no_function, wrong_route, wrong_unit, missing, unexpected, not_json, pipeline, bad_pipeline, unreadable_pipeline, unknown) = outcomes;
    assert_eq!((no_token, wrong_token, by_key), (401, 401, 200));
    assert_eq!(health["modules_loaded"], 1);
    assert!(catalog["modules"]["math"]["functions"]["double"].is_object(), "{catalog}");
    assert_eq!(preflight, 200);
    assert_eq!(by_name["result"]["repr"], "6.0 m");
    assert_eq!(positional["result"]["repr"], "3.0 m");
    assert_eq!((no_module, no_function, wrong_route), (404, 404, 404));
    assert_eq!((wrong_unit, missing, unexpected, not_json), (400, 400, 400, 400));
    assert_eq!(pipeline["results"]["b"]["repr"], "5.0 m");
    assert_eq!((bad_pipeline, unreadable_pipeline, unknown), (400, 400, 404));
}

#[test]
fn the_model_server_refuses_an_empty_token_and_a_missing_path() {
    let dir = TempDir::new("serve_errors");
    let empty_token = run_in(dir.path(), &["serve", ".", "--port", "19878", "--token", " "]);
    assert!(!empty_token.status.success() && stderr(&empty_token).contains("empty"), "{}", stderr(&empty_token));
    let nothing_to_serve = run_in(dir.path(), &["serve", ".", "--port", "19879"]);
    assert!(!nothing_to_serve.status.success() && stderr(&nothing_to_serve).contains("No .phs modules"), "{}", stderr(&nothing_to_serve));
    let absent = run_in(dir.path(), &["serve", "nowhere", "--port", "19880"]);
    assert!(!absent.status.success() && stderr(&absent).contains("does not exist"), "{}", stderr(&absent));
}
