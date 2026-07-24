use std::collections::HashMap;
use std::env;
use std::fs;
use std::process;
use physure_script::{parse_phs, transpile, PhsInterpreter, PhsValue, Target};

mod config;
mod html;
mod katex_assets;
mod latex;
mod protocol;
mod rich;
mod step;
mod tui;
mod web;

use config::PhysureConfig;
use rich::RichRenderer;
use step::ExecutionStep;

fn print_help() {
    println!("PhysureScript (PHS) CLI & Transpiler Engine v0.2.4");
    println!();
    println!("USAGE:");
    println!("    phs <script.phs> [OPTIONS]");
    println!("    phs --repl");
    println!("    phs transpile <script.phs> [--target <rust|python|java>] [--output <file>]");
    println!("    phs register-protocol");
    println!();
    println!("FLAGS & OPTIONS:");
    println!("    -h, --help               Print this help information");
    println!("    -r, --repl               Start interactive PHS REPL environment");
    println!("    -t, --target <lang>      Transpile target: rust, python, java (default: rust)");
    println!("    -o, --output <file>      Specify output file path (e.g. out.py, Main.java)");
    println!("    --tui                    Launch terminal UI dashboard mode");
    println!("    --web                    Launch local web visualizer server");
    println!("    --html, --view           Generate and open HTML report");
    println!();
    println!("EXAMPLES:");
    println!("    phs 1_cargas.phs");
    println!("    phs --repl");
    println!("    phs transpile 1_cargas.phs --target python");
    println!("    phs transpile 1_cargas.phs -t java -o Calculator.java");
}

fn run_repl() {
    use std::io::{self, Write};
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│ Physure Interactive REPL (PHS v0.2.4)                       │");
    println!("│ Type 'exit', 'quit', or 'help' for instructions.            │");
    println!("└──────────────────────────────────────────────────────────────┘");

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

        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        if trimmed == "exit" || trimmed == "quit" {
            println!("Goodbye!");
            break;
        }
        if trimmed == "help" {
            println!("Enter physical expressions, assignments, or functions.");
            println!("Examples:");
            println!("  m = 75.0 kg");
            println!("  v = 10 m / s");
            println!("  E = 1/2 m v^2 => J");
            println!("  f(x: m) = x * 2");
            continue;
        }

        match parse_phs(trimmed) {
            Ok(program) => {
                for stmt in &program.statements {
                    match interp.eval_statement_with_env(stmt, &mut env) {
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
            Err(e) => {
                eprintln!("Parse Error: {:?}", e);
            }
        }
    }
}

fn get_flag_value(args: &[String], flag: &str) -> Option<String> {
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
    let has_output_flag = args.iter().any(|a| a == "--output" || a == "-o");

    if !is_transpile_cmd && !has_target_flag && !has_output_flag {
        return false;
    }

    let mut script_path = None;
    for (i, arg) in args.iter().enumerate().skip(1) {
        if arg == "transpile" || arg == "--target" || arg == "-t" || arg == "--output" || arg == "-o" {
            continue;
        }
        if i > 0 && (args[i - 1] == "--target" || args[i - 1] == "-t" || args[i - 1] == "--output" || args[i - 1] == "-o") {
            continue;
        }
        script_path = Some(arg.clone());
        break;
    }

    let script_path = match script_path {
        Some(p) => p,
        None => {
            eprintln!("Error: missing script file path for transpilation");
            process::exit(1);
        }
    };

    let target_flag_val = get_flag_value(args, "--target").or_else(|| get_flag_value(args, "-t"));
    let output_flag_val = get_flag_value(args, "--output").or_else(|| get_flag_value(args, "-o"));

    let target = match (target_flag_val.as_deref(), output_flag_val.as_deref()) {
        (Some("python") | Some("py"), _) => Target::Python,
        (Some("java"), Some(out_p)) => {
            let class_name = std::path::Path::new(out_p)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Main");
            Target::JavaWithClass(class_name.to_string())
        }
        (Some("java"), None) => Target::Java,
        (Some(_), _) => Target::Rust,
        (None, Some(out_p)) => {
            if out_p.ends_with(".py") {
                Target::Python
            } else if out_p.ends_with(".java") {
                let class_name = std::path::Path::new(out_p)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Main");
                Target::JavaWithClass(class_name.to_string())
            } else {
                Target::Rust
            }
        }
        (None, None) => Target::Rust,
    };

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
    };
    println!("✓ Transpiled '{}' -> '{}' ({} target)", script_path, out_file_path, target_name);
    true
}

fn format_statement_latex(stmt: &physure_script::ast::Statement, i18n: &config::I18nLabels) -> (String, String, String, bool) {
    match stmt {
        physure_script::ast::Statement::Assignment(node) => {
            let sym_latex = latex::format_symbol_latex(&node.name);
            match &node.value {
                physure_script::ast::Expr::FunctionCall { name, args } if name == "solve" && args.len() == 2 => {
                    let clean_eq = latex::escape_latex_text(&latex::raw_identifier_text(&args[0], i18n));
                    let clean_var = latex::escape_latex_text(&latex::raw_identifier_text(&args[1], i18n));
                    let precursor = format!(
                        "\\text{{{} }} \\text{{{}}} \\text{{ {} }} \\text{{{}}}: \\quad {} =",
                        i18n.solve_from, clean_eq, i18n.solve_solving_for, clean_var, sym_latex
                    );
                    (node.name.clone(), format!("{} = ...", node.name), precursor, false)
                }
                physure_script::ast::Expr::FunctionCall { name, args } if (name == "deriv" || name == "diff") && args.len() == 2 => {
                    let expr_math = latex::render_raw_math(&latex::raw_identifier_text(&args[0], i18n), i18n);
                    let clean_var = latex::escape_latex_text(&latex::raw_identifier_text(&args[1], i18n));
                    let precursor = format!("{} = \\frac{{d}}{{d {}}}\\!\\left[{}\\right] =", sym_latex, clean_var, expr_math);
                    (node.name.clone(), format!("{} = ...", node.name), precursor, false)
                }
                physure_script::ast::Expr::FunctionCall { name, args } if (name == "integral" || name == "integrate") && args.len() == 2 => {
                    let expr_math = latex::render_raw_math(&latex::raw_identifier_text(&args[0], i18n), i18n);
                    let clean_var = latex::escape_latex_text(&latex::raw_identifier_text(&args[1], i18n));
                    let precursor = format!("{} = \\int {} \\; d{} =", sym_latex, expr_math, clean_var);
                    (node.name.clone(), format!("{} = ...", node.name), precursor, false)
                }
                physure_script::ast::Expr::FunctionCall { name, args } if (name == "ternary" || name == "if_then_else") && args.len() == 3 => {
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
        physure_script::ast::Statement::Expr(expr) => {
            let latex_s = latex::format_expr_latex_summary(expr, i18n);
            ("expr".to_string(), "expr".to_string(), latex_s, false)
        }
        _ => ("expr".to_string(), "expr".to_string(), String::new(), false)
    }
}

fn run_daemon() {
    use std::io::{self, BufRead, Write};
    use serde::{Deserialize, Serialize};

    #[derive(Deserialize)]
    struct DaemonRequest {
        id: usize,
        source: String,
    }

    #[derive(Serialize)]
    struct DaemonLineResult {
        line: usize,
        output: String,
    }

    #[derive(Serialize)]
    struct DaemonDiagnostic {
        line: usize,
        message: String,
        severity: String,
    }

    #[derive(Serialize)]
    struct DaemonResponse {
        id: usize,
        results: Vec<DaemonLineResult>,
        diagnostics: Vec<DaemonDiagnostic>,
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut handle = stdout.lock();

    for line_res in stdin.lock().lines() {
        let line_str = match line_res {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line_str.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: DaemonRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let mut results = Vec::new();
        let mut diagnostics = Vec::new();
        let interp = PhsInterpreter::default();
        let mut env = HashMap::new();

        match physure_script::parse_phs_with_lines(&req.source) {
            Ok(statements_with_lines) => {
                for (line_num, stmt) in statements_with_lines {
                    match interp.eval_statement_with_env(&stmt, &mut env) {
                        Ok(val) => {
                            if val != PhsValue::None {
                                let is_raw_block = match &stmt {
                                    physure_script::Statement::Expr(physure_script::Expr::Identifier(s)) => {
                                        s.starts_with('`')
                                    }
                                    _ => false,
                                };
                                if !is_raw_block {
                                    results.push(DaemonLineResult {
                                        line: line_num,
                                        output: val.to_string(),
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            diagnostics.push(DaemonDiagnostic {
                                line: line_num,
                                message: e.to_string(),
                                severity: "error".to_string(),
                            });
                        }
                    }
                }
            }
            Err(e) => {
                diagnostics.push(DaemonDiagnostic {
                    line: 0,
                    message: format!("{:?}", e),
                    severity: "error".to_string(),
                });
            }
        }

        let resp = DaemonResponse {
            id: req.id,
            results,
            diagnostics,
        };

        if let Ok(json_str) = serde_json::to_string(&resp) {
            let _ = writeln!(handle, "{}", json_str);
            let _ = handle.flush();
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--daemon" || a == "-d" || a == "daemon") {
        run_daemon();
        return;
    }

    if args.len() < 2 || args.iter().any(|a| a == "--help" || a == "-h" || a == "help") {
        print_help();
        return;
    }

    if args.iter().any(|a| a == "--repl" || a == "-r" || a == "repl") {
        run_repl();
        return;
    }

    if args[1] == "register-protocol" {
        if let Err(e) = protocol::register_phs_protocol() {
            eprintln!("Failed to register phs:// protocol: {}", e);
            process::exit(1);
        }
        return;
    }

    if handle_transpile(&args) {
        return;
    }

    let is_tui = args.iter().any(|a| a == "--tui");
    let is_web = args.iter().any(|a| a == "--web");
    let is_view = args.iter().any(|a| a == "--view" || a == "--html");

    let mut raw_input = args[1].as_str();
    if raw_input.starts_with("phs://") {
        raw_input = raw_input.trim_start_matches("phs://").trim_start_matches('/');
    }

    let code = if let Ok(content) = fs::read_to_string(raw_input) {
        content
    } else if raw_input.ends_with(".phs") {
        eprintln!("error: file not found '{}'", raw_input);
        process::exit(1);
    } else {
        raw_input.to_string()
    };

    let program = match parse_phs(&code) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error parsing script: {:?}", e);
            process::exit(1);
        }
    };

    let mut interp = PhsInterpreter::default();
    let vars_map = HashMap::new();
    let mut steps = Vec::new();
    let i18n = PhysureConfig::load().i18n();

    if !is_tui && !is_web && !is_view {
        RichRenderer::render_header(raw_input);
    }

    for stmt in program.statements {
        let (label, expr_code, latex_expr, is_disp) = format_statement_latex(&stmt, &i18n);

        match interp.run_statement(&stmt) {
            Ok(val) => {
                if val != PhsValue::None {
                    if !is_tui && !is_web && !is_view {
                        if is_disp {
                            if let PhsValue::String(ref txt) = val {
                                println!("\x1b[90m{}\x1b[0m", txt);
                            }
                        } else {
                            RichRenderer::render_variable_card(&label, &val);
                        }
                    }

                    steps.push(ExecutionStep {
                        label,
                        expr_code,
                        latex_expr,
                        value: val,
                        is_display_text: is_disp,
                    });
                }
            }
            Err(e) => {
                eprintln!("error executing statement ({:?}): {:?}", stmt, e);
                process::exit(1);
            }
        }
    }

    if is_tui {
        if let Err(e) = tui::run_tui(&code, &steps, &vars_map) {
            eprintln!("TUI Error: {}", e);
        }
    } else if is_web {
        if let Err(e) = web::start_web_server(raw_input, &code, &steps, &vars_map) {
            eprintln!("Web Visualizer Error: {}", e);
        }
    } else if is_view {
        if let Err(e) = html::open_standalone_html(raw_input, &code, &steps, &vars_map) {
            eprintln!("HTML Report Error: {}", e);
        }
    } else {
        RichRenderer::render_summary_box(&vars_map);
    }
}
