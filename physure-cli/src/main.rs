use std::collections::HashMap;
use std::env;
use std::fs;
use std::process;
use physure_script::{parse_phs, transpile, PhsInterpreter, PhsValue, Target};

mod check;
mod conf;
mod conf_witness;
mod config;
mod debug;
mod eval;
mod export;
mod html;
mod katex_assets;
mod latex;
mod mcp;
mod package;
mod protocol;
mod rich;
mod scaffold;
mod server;
mod step;
mod tui;
mod upgrade;
mod web;

use config::PhysureConfig;
use rich::RichRenderer;
use step::ExecutionStep;

fn print_help() {
    println!(
        "PhysureScript (PHS) CLI & Transpiler Engine v{}",
        env!("CARGO_PKG_VERSION")
    );
    println!();
    println!("USAGE:");
    println!("    phs <script.phs> [OPTIONS]");
    println!("    phs --repl");
    println!("    phs transpile <script.phs> [--target <rust|python|java|js|ts>] [--output <file>]");
    println!("    phs export <script.phs> --fn <name> [--native] [-o <dir>]");
    println!("    phs debug <script.phs> [--break-fn name] [--break N[:cond]]");
    println!("    phs pack [dir|manifest]  [-o <bundle.phspkg>] [--allow-execution] Package repository into .phspkg bundle");
    println!("    phs serve [dir|file]     [--port 8080] [--host 127.0.0.1] [--token <secret>] Run Model REST Server");
    println!("    phs doc [--save]         Generate full Markdown language & syntax specification");
    println!("    phs upgrade [--nightly]  Update phs and physure-lsp to the latest release (or main, with --nightly)");
    println!("    phs conf [status]        The frozen configuration in force (physure.frozen.conf); phs conf --help");
    println!("    phs conf show [KEY] | diff [A] [B] | freeze | verify [FILE] | update | seal | history");
    println!();
    println!("FLAGS & OPTIONS:");
    println!("    -h, --help               Print this help information");
    println!("    -V, --version            Print version and build commit");
    println!("    -r, --repl               Start interactive PHS REPL environment");
    println!("    -t, --target <lang>      Transpile target: rust, python, java, js, ts (default: rust)");
    println!("    -o, --output <file>      Specify output file path (e.g. out.py, Main.java)");
    println!("    --doc, doc [--save]      Generate full Markdown reference specification");
    println!("    --tui                    Launch terminal UI dashboard mode");
    println!("    --web                    Launch local web visualizer server");
    println!("    --html, --view           Generate and open HTML report");
    println!();
    println!("EXAMPLES:");
    println!("    phs 1_loads.phs");
    println!("    phs --repl");
    println!("    phs doc --save");
    println!("    phs transpile 1_loads.phs --target python");
    println!("    phs transpile 1_loads.phs -t java -o Calculator.java");
    println!("    phs new-plugin myplugin --lang rust");
    println!("    phs check orbit_sim.phs                (dimensions, without running; --json for tools)");
    println!("    phs eval orbit_sim.phs --seed 1        (sandboxed run, one JSON document; - reads stdin)");
    println!("    phs unit kWh                           (dimension, kind and alias of a unit, as JSON)");
    println!("    phs mcp                                (Model Context Protocol server on stdio, for agents)");
    println!("    phs export orbit_sim.phs --fn kinetic_energy --native -o dist/");
    println!("    phs debug orbit_sim.phs --break 12");
    println!("    phs upgrade");
    println!("    phs upgrade --nightly");
}

fn print_version() {
    match option_env!("PHS_BUILD_SHA") {
        Some(sha) => println!("phs {} ({})", env!("CARGO_PKG_VERSION"), sha),
        None => println!("phs {}", env!("CARGO_PKG_VERSION")),
    }
}

fn run_repl() {
    use std::io::{self, Write};
    // Padded to the frame's inner width so the right edge lines up whatever
    // the version string's length turns out to be.
    const W: usize = 62;
    let title = format!(
        " Physure Interactive REPL (PHS v{})",
        env!("CARGO_PKG_VERSION")
    );
    println!("┌{}┐", "─".repeat(W));
    println!("│{title:<W$}│");
    println!("│{:<W$}│", " Type 'exit', 'quit', or 'help' for instructions.");
    println!("└{}┘", "─".repeat(W));

    let interp = PhsInterpreter::default();
    let mut env = HashMap::new();
    let stdin = io::stdin();

    loop {
        print!("phs> ");
        if io::stdout().flush().is_err() { break; }

        let mut line = String::new();
        if stdin.read_line(&mut line).is_err() || line.is_empty() {
            println!("\nGoodbye!");
            break;
        }

        match line.trim() {
            "" => {}
            "exit" | "quit" => {
                println!("Goodbye!");
                break;
            }
            "help" => print_repl_help(),
            trimmed => eval_repl_line(&interp, &mut env, trimmed),
        }
    }
}

fn print_repl_help() {
    println!("Enter physical expressions, assignments, or functions.");
    println!("Examples:");
    println!("  m = 75.0 kg");
    println!("  v = 10 m / s");
    println!("  E = 1/2 m v^2 => J");
    println!("  f(x: m) = x * 2");
}

/// Evaluates what was typed at the prompt in the session's environment, reporting each error.
fn eval_repl_line(interp: &PhsInterpreter, env: &mut HashMap<String, PhsValue>, line: &str) {
    let program = match parse_phs(line) {
        Ok(program) => program,
        Err(e) => {
            eprintln!("Parse Error: {:?}", e);
            return;
        }
    };
    for stmt in &program.statements {
        match interp.eval_statement_with_env(stmt, env) {
            Ok(val) => {
                if val != PhsValue::None {
                    println!("=> {}", val);
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
    }
}

pub(crate) fn get_flag_value(args: &[String], flag: &str) -> Option<String> {
    if let Some(pos) = args.iter().position(|a| a == flag) {
        if pos + 1 < args.len() {
            return Some(args[pos + 1].clone());
        }
    }
    None
}

fn handle_transpile(args: &[String]) -> bool {
    let is_transpile_cmd = args.get(1).map(|s| s == "transpile").unwrap_or(false);
    let has_target_flag = args.iter().any(|a| a == "--target" || a == "-t");

    // `-o`/`--output` alone used to be enough to enter transpile mode -- but it's not a
    // transpile-specific flag (the `--html` report reuses it too, for its own output path),
    // so gating on it here meant `phs script.phs --html -o report.html` silently transpiled
    // to Rust and wrote that at "report.html" instead of ever generating a report. Only the
    // documented entry points (`transpile` as the subcommand, or an explicit `--target`)
    // commit to transpiling; `-o`'s value is still read below once one of those has.
    if !is_transpile_cmd && !has_target_flag {
        return false;
    }

    let script_path = match transpile_script_path(args) {
        Some(p) => p,
        None => {
            eprintln!("Error: missing script file path for transpilation");
            process::exit(1);
        }
    };

    let target_flag_val = get_flag_value(args, "--target").or_else(|| get_flag_value(args, "-t"));
    let output_flag_val = get_flag_value(args, "--output").or_else(|| get_flag_value(args, "-o"));
    let target = transpile_target(target_flag_val.as_deref(), output_flag_val.as_deref());

    let code = match fs::read_to_string(&script_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", script_path, e);
            process::exit(1);
        }
    };

    let program = match parse_phs(&code) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error parsing script '{}': {:?}", script_path, e);
            process::exit(1);
        }
    };

    let result = match transpile(&program, target.clone()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Transpilation error: {}", e);
            process::exit(1);
        }
    };

    let out_file_path = match output_flag_val {
        Some(p) => p,
        None => {
            let stem = std::path::Path::new(&script_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            let ext = match target {
                Target::Python => "py",
                Target::Java | Target::JavaWithClass(_) => "java",
                Target::Rust => "rs",
                Target::JavaScript => "js",
                Target::TypeScript => "ts",
            };
            format!("{}.{}", stem, ext)
        }
    };

    if let Err(e) = fs::write(&out_file_path, &result) {
        eprintln!("Error writing output file '{}': {}", out_file_path, e);
        process::exit(1);
    }

    let target_name = match target {
        Target::Python => "Python",
        Target::Java | Target::JavaWithClass(_) => "Java",
        Target::Rust => "Rust",
        Target::JavaScript => "JavaScript",
        Target::TypeScript => "TypeScript",
    };
    println!("✓ Transpiled '{}' -> '{}' ({} target)", script_path, out_file_path, target_name);
    true
}

/// The script to transpile: the first argument that is neither the subcommand, a flag nor a flag's value.
fn transpile_script_path(args: &[String]) -> Option<String> {
    let is_flag = |a: &str| matches!(a, "--target" | "-t" | "--output" | "-o");
    args.iter()
        .enumerate()
        .skip(1)
        .find(|(i, arg)| !(arg.as_str() == "transpile" || is_flag(arg)) && !is_flag(&args[i - 1]))
        .map(|(_, arg)| arg.clone())
}

/// A Java target named after the output file, so the class and the file agree.
fn java_target(out_path: &str) -> Target {
    let class_name = std::path::Path::new(out_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Main");
    Target::JavaWithClass(class_name.to_string())
}

/// The target a transpile asks for: the `--target` if there is one, else what the output file's extension says.
fn transpile_target(target_flag: Option<&str>, output_flag: Option<&str>) -> Target {
    match (target_flag, output_flag) {
        (Some("python") | Some("py"), _) => Target::Python,
        (Some("java"), Some(out_p)) => java_target(out_p),
        (Some("java"), None) => Target::Java,
        (Some("js") | Some("javascript"), _) => Target::JavaScript,
        (Some("ts") | Some("typescript"), _) => Target::TypeScript,
        (Some(_), _) => Target::Rust,
        (None, Some(out_p)) if out_p.ends_with(".py") => Target::Python,
        (None, Some(out_p)) if out_p.ends_with(".java") => java_target(out_p),
        (None, Some(out_p)) if out_p.ends_with(".ts") => Target::TypeScript,
        (None, Some(out_p)) if out_p.ends_with(".js") => Target::JavaScript,
        (None, _) => Target::Rust,
    }
}

fn format_statement_latex(stmt: &physure_script::ast::Statement, i18n: &config::I18nLabels) -> (String, String, String, bool) {
    match stmt {
        physure_script::ast::Statement::Assignment(node) => {
            let sym_latex = latex::format_symbol_latex(&node.name);
            match &node.value {
                physure_script::ast::Expr::FunctionCall { name, args, .. } if name == "solve" && args.len() == 2 => {
                    let clean_eq = latex::escape_latex_text(&latex::raw_identifier_text(&args[0], i18n));
                    let clean_var = latex::escape_latex_text(&latex::raw_identifier_text(&args[1], i18n));
                    let precursor = format!(
                        "\\text{{{} }} \\text{{{}}} \\text{{ {} }} \\text{{{}}}: \\quad {} =",
                        i18n.solve_from, clean_eq, i18n.solve_solving_for, clean_var, sym_latex
                    );
                    (node.name.clone(), format!("{} = ...", node.name), precursor, false)
                }
                physure_script::ast::Expr::FunctionCall { name, args, .. } if (name == "deriv" || name == "diff") && args.len() == 2 => {
                    let expr_math = latex::render_raw_math(&latex::raw_identifier_text(&args[0], i18n), i18n);
                    let clean_var = latex::escape_latex_text(&latex::raw_identifier_text(&args[1], i18n));
                    let precursor = format!("{} = \\frac{{d}}{{d {}}}\\!\\left[{}\\right] =", sym_latex, clean_var, expr_math);
                    (node.name.clone(), format!("{} = ...", node.name), precursor, false)
                }
                physure_script::ast::Expr::FunctionCall { name, args, .. } if (name == "integral" || name == "integrate") && args.len() == 2 => {
                    let expr_math = latex::render_raw_math(&latex::raw_identifier_text(&args[0], i18n), i18n);
                    let clean_var = latex::escape_latex_text(&latex::raw_identifier_text(&args[1], i18n));
                    let precursor = format!("{} = \\int {} \\; d{} =", sym_latex, expr_math, clean_var);
                    (node.name.clone(), format!("{} = ...", node.name), precursor, false)
                }
                physure_script::ast::Expr::FunctionCall { name, args, .. } if (name == "ternary" || name == "if_then_else") && args.len() == 3 => {
                    let cond_s = latex::format_expr_latex_summary(&args[0], i18n);
                    let precursor = format!("\\text{{{} }} {} \\quad \\Rightarrow \\quad {} =", i18n.given_prefix, cond_s, sym_latex);
                    (node.name.clone(), format!("{} = ...", node.name), precursor, false)
                }
                _ => {
                    (node.name.clone(), format!("{} = ...", node.name), format!("{} =", sym_latex), false)
                }
            }
        }
        physure_script::ast::Statement::Expr(physure_script::ast::Expr::Identifier(s)) if s.starts_with('`') => {
            ("note".to_string(), "note".to_string(), String::new(), true)
        }
        physure_script::ast::Statement::Expr(physure_script::ast::Expr::BinaryOp { op: physure_script::ast::BinaryOp::Convert, left, .. }) => {
            let l = latex::format_expr_latex_summary(left, i18n);
            ("expr".to_string(), "expr".to_string(), format!("{} \\Rightarrow", l), false)
        }
        physure_script::ast::Statement::Expr(physure_script::ast::Expr::FunctionCall { name, args, .. })
            if matches!(name.as_str(), "op_==" | "op_eq" | "op_!=" | "op_neq") && args.len() == 2 =>
        {
            let l = latex::format_expr_latex_summary(&args[0], i18n);
            let r = latex::format_expr_latex_summary(&args[1], i18n);
            let (true_sym, false_sym) = if matches!(name.as_str(), "op_!=" | "op_neq") {
                ("\\neq", "=")
            } else {
                ("=", "\\neq")
            };
            ("expr".to_string(), "expr".to_string(), latex::format_comparison_latex_expr(&l, &r, true_sym, false_sym), false)
        }
        physure_script::ast::Statement::Expr(expr) => {
            let latex_s = latex::format_expr_latex_summary(expr, i18n);
            ("expr".to_string(), "expr".to_string(), latex_s, false)
        }
        _ => ("expr".to_string(), "expr".to_string(), String::new(), false)
    }
}

#[derive(serde::Deserialize)]
struct DaemonRequest {
    id: usize,
    source: String,
    /// Path of the file being edited, sent by the editor so `use ... from
    /// <plugin/module>` can be resolved relative to it. `None` for ad-hoc
    /// snippets with no file (falls back to no plugin/module resolution).
    #[serde(default)]
    path: Option<String>,
}

#[derive(serde::Serialize)]
struct DaemonLineResult {
    line: usize,
    output: String,
}

#[derive(serde::Serialize)]
struct DaemonDiagnostic {
    line: usize,
    message: String,
    severity: String,
}

#[derive(serde::Serialize)]
struct DaemonResponse {
    id: usize,
    results: Vec<DaemonLineResult>,
    diagnostics: Vec<DaemonDiagnostic>,
}

/// How the editor shows a value: a plot as an image (or, for an HTML page, a data URL), anything else as text.
fn daemon_output(val: &PhsValue) -> String {
    let PhsValue::Plot(p) = val else {
        return val.to_string();
    };
    let trimmed = p.svg.trim_start();
    if trimmed.starts_with("<!DOCTYPE") || trimmed.starts_with("<html") {
        let b64 = base64_encode_bytes(p.svg.as_bytes());
        format!("[PLOT_HTML:data:text/html;charset=utf-8;base64,{}]", b64)
    } else {
        format!("[PLOT_IMAGE:data:image/svg+xml;utf8,{}]", p.svg)
    }
}

fn daemon_error(line: usize, message: String) -> DaemonDiagnostic {
    DaemonDiagnostic { line, message, severity: "error".to_string() }
}

/// Runs one request's source statement by statement: each value is a result on its line, each error a diagnostic.
fn run_daemon_request(req: &DaemonRequest) -> DaemonResponse {
    let mut results = Vec::new();
    let mut diagnostics = Vec::new();
    let interp = match req.path.as_deref().and_then(|p| std::path::Path::new(p).parent()) {
        Some(dir) if !dir.as_os_str().is_empty() => PhsInterpreter::with_base_dir(dir),
        _ => PhsInterpreter::default(),
    };
    let mut env = HashMap::new();

    match physure_script::parse_phs_with_lines(&req.source) {
        Ok(statements_with_lines) => {
            for (line_num, stmt) in statements_with_lines {
                match interp.eval_statement_with_env(&stmt, &mut env) {
                    Ok(val) if val != PhsValue::None => {
                        results.push(DaemonLineResult { line: line_num, output: daemon_output(&val) });
                    }
                    Ok(_) => {}
                    Err(e) => diagnostics.push(daemon_error(line_num, e.to_string())),
                }
            }
        }
        Err(e) => diagnostics.push(daemon_error(0, format!("{:?}", e))),
    }

    DaemonResponse { id: req.id, results, diagnostics }
}

fn run_daemon() {
    use std::io::{self, BufRead, Write};

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    for line_res in stdin.lock().lines() {
        let Ok(line_str) = line_res else { break };

        let trimmed = line_str.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Ok(req) = serde_json::from_str::<DaemonRequest>(trimmed) else { continue };

        if let Ok(json_str) = serde_json::to_string(&run_daemon_request(&req)) {
            let _ = writeln!(handle, "{}", json_str);
            let _ = handle.flush();
        }
    }
}

fn base64_encode_bytes(bytes: &[u8]) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i] as u32;
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(CHARSET[((triple >> 18) & 63) as usize] as char);
        out.push(CHARSET[((triple >> 12) & 63) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(CHARSET[((triple >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < bytes.len() {
            out.push(CHARSET[(triple & 63) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

fn generate_language_docs_md() -> String {
    let (registry, _) = physure_core::units::conf::build_registry_from_conf();
    let mut md = String::new();

    md.push_str("# 📐 Physure Language & Syntax Reference Specification\n\n");
    md.push_str("Physure (PHS) is a high-performance domain-specific programming language and computation engine for physical quantities, dimensional analysis, uncertainty propagation, symbolic calculus, and 3D WebGL scientific visualization.\n\n");

    md.push_str("---\n\n");
    md.push_str("## 1. Syntax & Language Constructs\n\n");

    md.push_str("### 1.1 Physical Quantities & Unit Conversions\n");
    md.push_str("Quantities consist of a numerical magnitude, an optional uncertainty, and a physical unit expression.\n\n");
    md.push_str("```phs\n");
    md.push_str("# Quantity literals with SI or derived units\n");
    md.push_str("pressure = 100.0 kPa\n");
    md.push_str("speed = 25.0 m / s\n");
    md.push_str("resistance = 50.0 Ohm\n");
    md.push_str("measurement = 10.0 +/- 0.5 m        # Measurement with 0.5 m standard deviation\n");
    md.push_str("percentage = 5.0 %               # Relative uncertainty or percentage\n\n");
    md.push_str("# Unit Conversion Operator '=>'\n");
    md.push_str("p_bar = 100.0 kPa => bar         # Converts 100.0 kPa to bar\n");
    md.push_str("v_kmh = 25.0 m / s => km / h     # Converts 25 m/s to km/h\n");
    md.push_str("```\n\n");
    md.push_str("#### Angles in a conversion\n\n");
    md.push_str("The SI gives the radian and the steradian the expression 1, and `=>` follows it: an angle is read as 1 when a conversion needs it, so the formulas of mechanics, circuits and photometry convert without ceremony.\n\n");
    md.push_str("```phs\n");
    md.push_str("energy = 0.5 * (0.5 kg * m^2) * (10 rad/s)^2   # kg * m^2 * rad^2 * s^-2\n");
    md.push_str("e_j = energy => J                              # 25 J\n");
    md.push_str("v = (10 rad/s) * (0.5 m) => m/s                # 5 m/s\n");
    md.push_str("lux = (100 cd) / (2 m)^2 => lx                 # 25 lx\n");
    md.push_str("```\n\n");
    md.push_str("A sum or a comparison never reads an angle as 1: `2 rad + 1` and `2 m*rad + 1 m` are errors, and the way is to convert first, `(2 rad => unity) + 1`. A rate of angle is not a frequency (they differ by 2π), so `10 rad/s => Hz` is an error; divide by the angle of a cycle instead, `(10 rad/s) / (6.283185307179586 rad) => Hz`. `angle_mode = dimension` in `physure.conf` makes the angle a dimension like any other.\n\n");
    md.push_str("#### Comparisons\n\n");
    md.push_str("`==` compares what two quantities stand for, whatever units they are written in: `1 km == 1000 m` and `0.01 degC == 273.16 K` are true in either order. Two values are equal when they agree within a relative 1e-12 of the larger (the rounding of a conversion or of ordinary arithmetic): `1 pm == 2 pm` is false and `(0.1 m + 0.2 m) == 0.3 m` is true. A relative bound never accepts a difference against an exact zero, so `cos(90 deg) == 0` is false: write `abs(cos(90 deg)) < 1e-12`, or `abs(x) < 1e-9 m` with the unit of `x`. Near the bound `==` is not transitive. `inf == inf` is true and NaN equals nothing. `!=` is exactly `not (a == b)`.\n\n");
    md.push_str("`a ≈ b` is `|a − b| <= rel · max(|a|, |b|)` with `rel` from `[Settings] approx_rel_tol` in `physure.conf` (default `1e-3`); `approx(a, b, rel)` names the tolerance (`approx(x, y, 1 %)`). `≈` with exactly one operand zero is an error: no relative tolerance says how close to zero is close.\n\n");
    md.push_str("`x == t +/- k sigma` asks whether the measured `x` lies within `k` of its own standard uncertainties of the exact target `t`: `(9.81 +/- 0.01) m/s^2 == 9.80 m/s^2 +/- 2 sigma` is true. An `x` with no uncertainty is an error; use `==`.\n\n");
    md.push_str("Different dimensions, and operand types that have no comparison (two vectors, a string and a quantity), are errors, never `False`. Arithmetic and conversion bind tighter than a comparison, so `0.1 m + 0.2 m == 0.3 m` is true and `x => km < 5 km` converts, then compares (see the operator table below). A number or a quantity used as a condition (`if`, `while`) is false only when it is exactly zero; a temperature point (`0 degC`), a level (`20 dB`) or NaN is not a condition.\n\n");

    md.push_str("#### Operator precedence\n\n");
    md.push_str("Tightest first; the operators of one row bind equally and, where they repeat, from left to right.\n\n");
    md.push_str("| Operators | Reading |\n");
    md.push_str("| :--- | :--- |\n");
    md.push_str("| `^`, unary `-` | `-2 ^ 2` is `-(2 ^ 2)`, -4; a power of a power needs parentheses: `(2 ^ 3) ^ 2` or `2 ^ (3 ^ 2)` |\n");
    md.push_str("| `*`, `/`, implicit multiplication | `6 m / 2 * 3` is 9 m; `2 3 m` is 6 m |\n");
    md.push_str("| `+`, `-` | `a - b + c` is `(a - b) + c` |\n");
    md.push_str("| `=>` | converts everything to its left: `1 m + 2 m => cm` is 300 cm; the target is a unit |\n");
    md.push_str("| `==`, `!=`, `<`, `<=`, `>`, `>=`, `≈` | one per expression: `0.1 + 0.2 == 0.3` is true |\n");
    md.push_str("| `..` | one per expression; each endpoint is a whole expression: `0 m .. 10 m => km` is `0 m .. 0.01 km` |\n");
    md.push_str("| `: spec` | formats the whole expression: `0.1 + 0.2: .3f` |\n");
    md.push_str("| `not` | `not a + b > c` is `not ((a + b) > c)` |\n");
    md.push_str("| `and` | |\n");
    md.push_str("| `or` | |\n");
    md.push_str("| `c ? a : b` | |\n");
    md.push_str("| `where` | |\n\n");
    md.push_str("Two forms are errors that say which parentheses to write. A `+` or `-` after `=> unit`: `x => km - 5 m` cannot subtract from the unit, and the standard order does not say `(x => km) - 5 m`, so write that. A chain of comparisons: `a < b < c` would compare the `True` or `False` of `a < b` with `c`, so write `a < b and b < c`. The strings the symbolic builtins read (`deriv`, `integral`, `limit`, ...) follow the same order: `deriv(\"-x^2\", \"x\")` is `-2 * x`, and `x^2^3` needs parentheses there too; in them an exponent may carry its own sign (`e^-x`) and `**` is `^`.\n\n");

    md.push_str("### 1.2 Function Definitions & Docstrings\n");
    md.push_str("Functions can be defined with or without the optional `fn` keyword. Unit constraints on parameters are checked automatically.\n\n");
    md.push_str("```phs\n");
    md.push_str("/// Computes kinetic energy in Joules\n");
    md.push_str("/// @param m Mass of the body in kg\n");
    md.push_str("/// @param v Velocity of the body in m/s\n");
    md.push_str("/// @returns Energy in Joules\n");
    md.push_str("fn E_k(m: kg, v: m/s) = 0.5 * m * v^2\n\n");
    md.push_str("# Direct mathematical function syntax (without 'fn')\n");
    md.push_str("P(x, y) = 100.0 kPa * sin(x / 1.0 m) * cos(y / 1.0 m)\n");
    md.push_str("```\n\n");

    md.push_str("### 1.3 Control Flow & Local Bindings\n");
    md.push_str("```phs\n");
    md.push_str("doubled = x * 2.0 where x = 10.0 m\n");
    md.push_str("status = if pressure > 50.0 kPa then \"High Pressure\" else \"Normal Pressure\"\n");
    md.push_str("```\n\n");

    md.push_str("### 1.4 Imports & Domain Modules\n");
    md.push_str("```phs\n");
    md.push_str("use solve, deriv, integral from calc\n");
    md.push_str("use plot, plot3d, export3d from plot\n");
    md.push_str("use linspace, gradient, trapz from array\n");
    md.push_str("```\n\n");

    md.push_str("---\n\n");
    md.push_str("## 2. 3D WebGL Surface Visualization & Mesh Export\n\n");
    md.push_str("Physure includes native 3D physical surface rendering (WebGL 100% offline) and CAD/3D mesh export.\n\n");
    md.push_str("```phs\n");
    md.push_str("use plot3d, export3d from plot\n\n");
    md.push_str("fn P(x, y) = 100.0 kPa * sin(x / 1.0 m) * cos(y / 1.0 m)\n");
    md.push_str("rango_x = -2.0 m .. 2.0 m\n");
    md.push_str("rango_y = -2.0 m .. 2.0 m\n\n");
    md.push_str("# 1. Render interactive 3D WebGL viewer\n");
    md.push_str("plot3d(P, x: rango_x, y: rango_y, title: \"Pressure Surface Distribution P(x, y)\")\n\n");
    md.push_str("# 2. Export standard 3D CAD meshes\n");
    md.push_str("export3d(P, x: rango_x, y: rango_y, file: \"mesh.stl\", format: \"stl\")\n");
    md.push_str("export3d(P, x: rango_x, y: rango_y, file: \"mesh.obj\", format: \"obj\")\n");
    md.push_str("export3d(P, x: rango_x, y: rango_y, file: \"mesh.gltf\", format: \"gltf\")\n");
    md.push_str("export3d(P, x: rango_x, y: rango_y, file: \"mesh.ply\", format: \"ply\")\n");
    md.push_str("```\n\n");

    md.push_str("---\n\n");
    md.push_str("## 3. Built-in Function Modules\n\n");
    md.push_str("| Domain / Module | Function | Description | Example |\n");
    md.push_str("| :--- | :--- | :--- | :--- |\n");
    md.push_str("| `core` | `sqrt(x)` | Square root | `sqrt(16.0 m^2)` |\n");
    md.push_str("| `core` | `sin(x)`, `cos(x)`, `tan(x)` | Trigonometric functions | `sin(3.14159 / 2)` |\n");
    md.push_str("| `core` | `exp(x)`, `ln(x)`, `log(x)` | Exponents and logarithms | `ln(10.0)` |\n");
    md.push_str("| `core` | `abs(x)` | Absolute value | `abs(-5.0 m)` |\n");
    md.push_str("| `core` | `round(x, n)` | Round to n decimal places | `round(3.14159, 2)` |\n");
    md.push_str("| `calc` | `solve(eq, target)` | Symbolic equation solver | `solve(P == F / A, F)` |\n");
    md.push_str("| `calc` | `deriv(expr, var)` | Symbolic derivative | `deriv(0.5 * m * v^2, v)` |\n");
    md.push_str("| `calc` | `integral(expr, var)` | Symbolic integral | `integral(m * g, h)` |\n| `calc` | `integral(expr, var, a, b)` | Definite integral (closed form confirmed numerically, or tanh-sinh quadrature with an error estimate) | `integral(\"v\", \"t\", 0 s, 1 h)` |\n");
    md.push_str("| `calc` | `limit(expr, var, point)` | Limit (value at a point of continuity, or Richardson extrapolation from both sides with a convergence check) | `limit(\"sin(x)/x\", \"x\", 0)` |\n");
    md.push_str("| `array` | `linspace(a, b, n)` | Vector generation | `linspace(0.0 m, 10.0 m, 100)` |\n");
    md.push_str("| `array` | `gradient(y, x)` | Numerical derivative dy/dx | `gradient(presion_vec, pos_vec)` |\n");
    md.push_str("| `array` | `trapz(y, x)` | Numerical integration (area) | `trapz(fuerza_vec, pos_vec)` |\n\n");
    md.push_str("#### Definite integrals\n\n");
    md.push_str("`integral(expr, var, a, b)` (after `use integral from calc`) is the definite integral of `expr` over `var` from `a` to `b`; `integral(expr, var)` is the symbolic antiderivative.\n\n");
    md.push_str("- **Bounds**: a number, a quantity, `\"inf\"` or `\"-inf\"`, or a name or an expression written as text (`pi`, `\"pi/2\"`, `\"bohr_radius\"`, a variable of the program). A name that resolves to nothing is an error, never 0. A quantity bound makes `var` a quantity of that unit; the other bound must have the same dimension, or be a plain `0` or an infinity. A temperature point or a level is not a bound.\n- **Names**: the integrand sees the program's variables (and a function's parameters). A free name resolves to a variable, a unit, `pi` or a constant of the catalog; any other name is an error.\n- **Result**: a quantity in (unit of `expr`) times (unit of `var`) whenever a unit or an uncertainty is involved, otherwise a number. With `v = 3 m/s`, `integral(\"v\", \"t\", 0 s, 1 h)` is `10800 m`.\n- **Uncertainty** propagates, from the program's uncertain variables and from uncertain bounds, and the result stays correlated with them.\n- **Method**: the closed form `F(b) - F(a)` when the symbolic engine has an antiderivative *and* the quadrature confirms it, so a closed form that jumps across a pole is refused (`integral(\"x^(-2)\", \"x\", -1, 1)` is an error, not -2). Otherwise tanh-sinh quadrature with an error estimate: accepted when two successive levels agree within a relative 1e-12 of max(|I|, the integral of |f|), from level 4 on, with at most 20000 samples per piece. An infinite end is mapped by x = c ± (1 - u)/u.\n- **Refused, not guessed**: a singularity at a non-zero end (`1/sqrt(1-x)` on [0, 1]: substitute so that it sits at 0), a jump or a kink inside the interval (split the interval there), `sin(1/x)` next to 0, an integrand that decays too slowly towards an infinite end. An integrand that cannot be evaluated, or is not finite, at a sample is an error.\n- **Known failure mode**: a peak much narrower than the spacing of the first levels' samples is never sampled, and every level agrees on a wrong value: a Gaussian of width 1e-4 at x = 0.3 on [0, 1] integrates to a confident 0. Split the interval around a known peak.\n\n");
    md.push_str("#### Limits\n\n`limit(expr, var, point)` (after `use limit from calc`) is the limit of `expr` as `var` tends to `point`. The symbolic engine has no limit: the answer is numeric, and it is given only when it converged; otherwise the call is an error, never a guessed `0` or `inf`.\n\n- **Point**: a number, a quantity, `\"inf\"` or `\"-inf\"`, or a name or an expression written as text (`pi`, `\"pi/2\"`, a variable, a constant of the catalog). A name that resolves to nothing is an error. A quantity point makes `var` a quantity of that unit; a temperature point or a level is refused. Towards an infinite point `var` is a plain number.\n- **Names**: `expr` sees the program's variables (and a function's parameters), as the integrand of `integral` does; any other free name is an error.\n- **Continuous point**: when `expr` calls only `sin`, `cos`, `tan`, `exp`, `ln`, `log`, `sqrt`, `min`, `max` and arithmetic (no exponent involving `var`) and its value at the point is defined and finite, the limit is that value, with the units and the uncertainty the core gives it (`limit(\"2000000+x\", \"x\", 1)` is `2000001`).\n- **Extrapolation**: otherwise (a removable singularity such as `sin(x)/x` at 0, or `floor`, `abs`, a comparison, a ternary in `expr`) the samples at `point ± h`, `h = 1/8, 1/16, ...` (12 steps), are extrapolated to `h = 0` (Richardson, by Neville's algorithm) separately on each side. A side is accepted when its error estimate is below `1e-9` of max(|L|, the largest sample) at two successive steps, so a limit of 0 works; the two sides must agree to the same tolerance. Measured: `(1-cos(x))/x^2` at 0 is 0.5 within 5e-11 (its samples cancel), `sin(x)/x` within 1e-15. A side where `expr` is undefined at every sample (outside the domain of `sqrt` or `ln`) does not count: `sin(sqrt(x))/sqrt(x)` at 0 is 1, read from the right.\n- **Infinite point**: `var = ±8, ±16, ±32, ...` (24 steps), extrapolated in `1/var`: `limit(\"1/x\", \"x\", \"inf\")` is 0, `limit(\"(1+1/x)^x\", \"x\", \"inf\")` is e.\n- **Zero and infinity**: samples of one sign falling geometrically give 0 (`x*ln(x)` at 0); samples of one sign growing with non-decreasing increments give `inf` or `-inf`, when both sides agree: `limit(\"1/x^2\", \"x\", 0)` is `inf`, `limit(\"1/x\", \"x\", 0)` is an error.\n- **Uncertainty** propagates: at a continuous point through the core, and an extrapolated limit is a weighted sum of samples formed with the core's arithmetic. An uncertain point where the limit must be extrapolated is refused.\n- **Refused, not guessed**: sides that disagree (`abs(x)/x` at 0, `floor(x)` at 1), a neighbourhood where `expr` is undefined (`sqrt(x)` at -1), oscillation (`sin(1/x)` at 0, `sin(x)` at `\"inf\"`), a change on a scale smaller than the smallest step (`(1-cos(1000*x))/x^2` at 0), and samples that rounding has made equal (`min(x, 100)` at `\"inf\"`).\n\n");

    md.push_str("---\n\n");
    md.push_str("## 4. Physical Units & Aliases Registry\n\n");
    md.push_str("| Symbol / Alias | Category | Base SI Dimensions |\n");
    md.push_str("| :--- | :--- | :--- |\n");

    let mut keys: Vec<&String> = registry.derived_units.keys().collect();
    keys.sort();
    for name in keys {
        let unit = &registry.derived_units[name];
        let meta = registry.unit_meta.get(name);
        let category = meta.and_then(|m| m.category.as_deref()).unwrap_or("Derived");
        let dim = unit.base_repr();
        md.push_str(&format!("| `{}` | {} | `{}` |\n", name, category, if dim.is_empty() { "dimensionless".into() } else { dim }));
    }

    let mut alias_keys: Vec<&String> = registry.aliases.keys().collect();
    alias_keys.sort();
    for alias in alias_keys {
        let target = &registry.aliases[alias];
        md.push_str(&format!("| `{}` | Alias -> `{}` | - |\n", alias, target));
    }

    md.push_str("\n---\n\n");
    md.push_str("## 5. Greek Letters & Mathematical Symbols\n\n");
    md.push_str("| Symbol | LaTeX / Name Aliases | Description |\n");
    md.push_str("| :--- | :--- | :--- |\n");
    md.push_str("| `Δ` | `delta`, `Delta`, `\\delta` | Difference / Variation / Change |\n");
    md.push_str("| `σ` | `sigma`, `\\sigma` | Standard deviation / Uncertainty / Stress |\n");
    md.push_str("| `Ω` | `omega`, `Omega`, `\\Omega` | Electric resistance (Ohm) |\n");
    md.push_str("| `π` | `pi`, `\\pi` | Circle constant (3.14159...) |\n");
    md.push_str("| `θ` | `theta`, `\\theta` | Angle / Temperature |\n");
    md.push_str("| `λ` | `lambda`, `\\lambda` | Wavelength |\n");
    md.push_str("| `μ` | `mu`, `micro`, `\\mu` | Micro prefix / Friction / Permeability |\n");
    md.push_str("| `α` | `alpha`, `\\alpha` | Thermal expansion / Alpha coefficient |\n");
    md.push_str("| `β` | `beta`, `\\beta` | Beta coefficient / Ratio |\n");
    md.push_str("| `γ` | `gamma`, `\\gamma` | Heat capacity ratio |\n");
    md.push_str("| `ε` | `epsilon`, `\\epsilon` | Permittivity / Strain |\n");
    md.push_str("| `η` | `eta`, `\\eta` | Efficiency |\n");
    md.push_str("| `ρ` | `rho`, `\\rho` | Density / Electrical resistivity |\n");
    md.push_str("| `τ` | `tau`, `\\tau` | Torque / Time constant |\n");
    md.push_str("| `ϕ` | `phi`, `\\phi` | Magnetic flux / Phase |\n");
    md.push_str("| `ψ` | `psi`, `\\psi` | Wavefunction |\n");
    md.push_str("| `ω` | `omega`, `\\omega` | Lowercase Angular frequency |\n");
    md.push_str("| `∞` | `infinity`, `\\infty` | Infinity symbol |\n");
    md.push_str("| `±` | `+/-`, `\\pm` | Plus-minus uncertainty |\n");

    md.push_str("\n---\n\n");
    md.push_str("## 6. Transpilation & Integration Targets\n\n");
    md.push_str("Physure scripts (`.phs`) can be transpiled natively into high-performance target code:\n\n");
    md.push_str("```bash\n");
    md.push_str("# Transpile to Python with NumPy & SciPy\n");
    md.push_str("phs transpile script.phs --target python --output script.py\n\n");
    md.push_str("# Transpile to Rust\n");
    md.push_str("phs transpile script.phs --target rust --output main.rs\n\n");
    md.push_str("# Transpile to Java\n");
    md.push_str("phs transpile script.phs --target java --output Main.java\n\n");
    md.push_str("# Transpile to TypeScript\n");
    md.push_str("phs transpile script.phs --target ts --output script.ts\n\n");
    md.push_str("# Transpile to JavaScript\n");
    md.push_str("phs transpile script.phs --target js --output script.js\n");
    md.push_str("```\n");

    md
}

fn run_doc_generator(args: &[String]) {
    let md = generate_language_docs_md();
    if args.iter().any(|a| a == "--save" || a == "-s") {
        let out_path = "PHYSURE_LANGUAGE_GUIDE.md";
        if let Err(e) = fs::write(out_path, &md) {
            eprintln!("Failed to save documentation: {}", e);
        } else {
            println!("📄 Language reference specification saved to: {}", out_path);
        }
    } else {
        println!("{}", md);
    }
}

/// The commands named by a flag anywhere in the arguments (and bare `help`/`doc`/`repl`/`daemon`). True when one ran.
fn run_flag_command(args: &[String]) -> bool {
    let has = |names: &[&str]| args.iter().any(|a| names.contains(&a.as_str()));
    if has(&["--daemon", "-d", "daemon"]) {
        run_daemon();
    } else if has(&["--version", "-V"]) {
        print_version();
    } else if args.len() < 2 || has(&["--help", "-h", "help"]) {
        print_help();
    } else if has(&["--doc", "doc", "docs", "--docs"]) {
        run_doc_generator(args);
    } else if has(&["--repl", "-r", "repl"]) {
        run_repl();
    } else {
        return false;
    }
    true
}

/// The subcommands named by the first argument. True when one ran.
fn run_subcommand(args: &[String]) -> bool {
    match args[1].as_str() {
        "register-protocol" => {
            if let Err(e) = protocol::register_phs_protocol() {
                eprintln!("Failed to register phs:// protocol: {}", e);
                process::exit(1);
            }
        }
        "new-plugin" => scaffold::run_new_plugin(args),
        "check" => check::run_check(args),
        "eval" => eval::run_eval(args),
        "unit" => eval::run_unit(args),
        "mcp" => mcp::run_mcp(),
        "export" => export::run_export(args),
        "debug" => debug::run_debug(args),
        "pack" => package::run_pack(args),
        "serve" => server::run_serve(args),
        "upgrade" => upgrade::run_upgrade(args.iter().any(|a| a == "--nightly")),
        _ => return false,
    }
    true
}

/// The text of a script argument: the file it names, or, when it names none, the argument itself as a snippet.
fn read_script_or_snippet(raw_input: &str) -> String {
    if let Ok(content) = fs::read_to_string(raw_input) {
        content
    } else if raw_input.ends_with(".phs") {
        eprintln!("error: file not found '{}'", raw_input);
        process::exit(1);
    } else {
        raw_input.to_string()
    }
}

/// The label under which a statement's value is shown: for a bare expression, the statement's own source line.
///
/// `format_statement_latex` gives every bare (unassigned) expression statement the
/// same generic "expr" label -- fine for the LaTeX/TUI paths, which also carry
/// `expr_code`/a table column alongside it, but the plain `label = value` card
/// has no other column, so two such statements printed identically with no way to
/// tell which value came from which line. Swap in the statement's own source line
/// (already available from the parser's `Program::lines`) for that one case.
fn display_label(stmt: &physure_script::ast::Statement, line: Option<usize>, code: &str, label: &str) -> String {
    if !matches!(stmt, physure_script::ast::Statement::Expr(_)) {
        return label.to_string();
    }
    line.and_then(|n| code.lines().nth(n.saturating_sub(1)))
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| label.to_string())
}

/// `@precision(N)` decorates a variable assignment; `decorators.rs` already guarantees
/// (at parse time) that when present it's exactly 1 positive whole-number argument, so
/// this only needs to read it back out, not re-validate it.
fn precision_override(stmt: &physure_script::ast::Statement) -> Option<u32> {
    let physure_script::ast::Statement::Assignment(node) = stmt else {
        return None;
    };
    node.decorators
        .iter()
        .find(|d| d.name == "precision")
        .and_then(|d| d.args.first())
        .and_then(|arg| match arg {
            physure_script::ast::Expr::Quantity(q) if q.magnitude > 0.0 && q.magnitude.fract() == 0.0 => Some(q.magnitude as u32),
            _ => None,
        })
}

/// Prints a value the way the terminal shows it: a display text as it is, anything else as a `label = value` card.
fn print_value(shown_label: &str, val: &PhsValue, is_disp: bool, precision_override: Option<u32>) {
    match val {
        PhsValue::String(txt) if is_disp => println!("{}", txt),
        _ if is_disp => {}
        _ => RichRenderer::render_variable_card(shown_label, val, precision_override),
    }
}

/// Runs the program, printing each value as it comes unless a viewer will show them, and returns the steps.
fn run_statements(
    interp: &mut PhsInterpreter,
    program: &physure_script::ast::Program,
    code: &str,
    raw_input: &str,
    i18n: &config::I18nLabels,
    quiet: bool,
) -> Vec<ExecutionStep> {
    let mut steps = Vec::new();
    for (stmt_idx, stmt) in program.statements.iter().enumerate() {
        let (label, expr_code, latex_expr, is_disp) = format_statement_latex(stmt, i18n);
        let line_no = program.lines.get(stmt_idx).copied();
        let shown_label = display_label(stmt, line_no, code, &label);
        let precision_override = precision_override(stmt);

        match interp.run_statement(stmt) {
            Ok(val) if val != PhsValue::None => {
                // A bare (unassigned) expression that evaluates to a String is a print,
                // not a named quantity -- showing it as `"<template source>" = <computed
                // text>` repeats the message twice with a confusing "=" between them. This
                // folds into the same `is_disp` signal the backtick-note convention
                // already used (both mean "render as plain text, not a label=value card"),
                // so both the terminal print below and the HTML report's existing
                // `is_display_text` handling pick it up with no separate fix needed there.
                let is_disp = is_disp
                    || (matches!(stmt, physure_script::ast::Statement::Expr(_)) && matches!(val, PhsValue::String(_)));
                if !quiet {
                    print_value(&shown_label, &val, is_disp, precision_override);
                }

                steps.push(ExecutionStep {
                    label,
                    expr_code,
                    latex_expr,
                    value: val,
                    is_display_text: is_disp,
                    precision_override,
                });
            }
            Ok(_) => {}
            Err(e) => {
                let line_text = line_no
                    .and_then(|n| code.lines().nth(n.saturating_sub(1)))
                    .unwrap_or("");
                RichRenderer::render_runtime_error(raw_input, &e, &expr_code, line_no, line_text);
                process::exit(1);
            }
        }
    }
    steps
}

fn main() {
    let args: Vec<String> = env::args().collect();
    // `phs conf` first: its own arguments (`--reason help`, `show doc`) must never reach `run_flag_command`, and it
    // exists to inspect a configuration the pre-command check would refuse (it sets the engine file itself).
    if args.get(1).map(String::as_str) == Some("conf") {
        conf::run_conf(&args);
    }
    refuse_an_unusable_configuration();
    if run_flag_command(&args) || run_subcommand(&args) || handle_transpile(&args) {
        return;
    }
    run_script(&args);
}

/// Before anything runs: the configuration in force (a frozen `physure.frozen.conf`, `PHYSURE_CONF`, ...) must be
/// usable, or the program stops with its message on stderr and an exit code that says what kind of problem it is:
/// 1 for the integrity of the file, 2 for resolution, 3 for the engine. Without this the first computation would
/// refuse in the middle of the run. The engine file of the policy `binary` is this executable.
fn refuse_an_unusable_configuration() {
    if let Ok(exe) = env::current_exe() {
        physure_core::frozen::set_engine_file(Some(exe));
    }
    if let Err(e) = physure_core::frozen::ensure_valid() {
        eprintln!("phs: refusing to run: {e}");
        process::exit(e.exit_code());
    }
}

/// Runs the script (or snippet) named by the first argument and shows the result the way the flags ask.
fn run_script(args: &[String]) {
    let viewer = viewer_for(args);

    let mut raw_input = args[1].as_str();
    if raw_input.starts_with("phs://") {
        raw_input = raw_input.trim_start_matches("phs://").trim_start_matches('/');
    }

    let code = read_script_or_snippet(raw_input);

    let program = match parse_phs(&code) {
        Ok(s) => s,
        Err(e) => {
            RichRenderer::render_parse_error(raw_input, &e, &code);
            process::exit(1);
        }
    };

    let script_dir = match std::path::Path::new(raw_input).parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => std::path::Path::new("."),
    };
    let mut interp = PhsInterpreter::with_base_dir(script_dir);
    let vars_map = HashMap::new();
    let i18n = PhysureConfig::load().i18n();

    let quiet = viewer != Viewer::Terminal;
    if !quiet {
        RichRenderer::render_header(raw_input);
    }

    let steps = run_statements(&mut interp, &program, &code, raw_input, &i18n, quiet);

    match viewer {
        Viewer::Tui => {
            if let Err(e) = tui::run_tui(&code, &steps, &vars_map) {
                eprintln!("TUI Error: {}", e);
            }
        }
        Viewer::Web => {
            if let Err(e) = web::start_web_server(raw_input, &code, &steps, &vars_map) {
                eprintln!("Web Visualizer Error: {}", e);
            }
        }
        Viewer::Html => {
            let html_output_path = html_report_path(args, raw_input);
            if let Err(e) = html::open_standalone_html(raw_input, &html_output_path, &code, &steps, &vars_map) {
                eprintln!("HTML Report Error: {}", e);
            }
        }
        Viewer::Terminal => RichRenderer::render_summary_box(&vars_map),
    }
}

/// Where a script's values are shown.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Viewer {
    Tui,
    Web,
    Html,
    Terminal,
}

fn viewer_for(args: &[String]) -> Viewer {
    if args.iter().any(|a| a == "--tui") {
        Viewer::Tui
    } else if args.iter().any(|a| a == "--web") {
        Viewer::Web
    } else if args.iter().any(|a| a == "--view" || a == "--html") {
        Viewer::Html
    } else {
        Viewer::Terminal
    }
}

/// Where the HTML report of a script is written.
///
/// Default: next to the script, same base name -- `calc.phs --html` used to always
/// land in the OS temp dir under a timestamp nobody could predict or find again once
/// the browser tab closed, which defeats the report's own point as a keepable
/// artifact. `-o`/`--output` overrides it; safe to reuse now that `-o` alone no
/// longer also means "transpile" (see `handle_transpile`).
fn html_report_path(args: &[String], raw_input: &str) -> std::path::PathBuf {
    match get_flag_value(args, "--output").or_else(|| get_flag_value(args, "-o")) {
        Some(custom) => std::path::PathBuf::from(custom),
        None if std::path::Path::new(raw_input).exists() => std::path::Path::new(raw_input).with_extension("html"),
        None => std::path::PathBuf::from("physure_report.html"),
    }
}
