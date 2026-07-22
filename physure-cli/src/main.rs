use std::collections::HashMap;
use std::env;
use std::fs;
use std::process;
use physure_script::{parse_phs, transpile, PhsInterpreter, PhsValue, Target};

mod config;
mod html;
mod katex_assets;
mod protocol;
mod rich;
mod step;
mod tui;
mod web;

use rich::RichRenderer;
use step::ExecutionStep;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("PhysureScript (PHS) Visual CLI v0.2.4");
        eprintln!("Usage: phs <script.phs> [--tui | --web | --view | --html]");
        eprintln!("       phs register-protocol");
        eprintln!("       phs transpile <script.phs> --target <rust|python|java>");
        process::exit(1);
    }

    if args[1] == "register-protocol" {
        if let Err(e) = protocol::register_phs_protocol() {
            eprintln!("Failed to register phs:// protocol: {}", e);
            process::exit(1);
        }
        return;
    }

    if args[1] == "transpile" && args.len() >= 3 {
        let file_path = &args[2];
        let target_str = if args.len() >= 5 && args[3] == "--target" { &args[4] } else { "rust" };
        let target = match target_str.to_lowercase().as_str() {
            "python" | "py" => Target::Python,
            "java" => Target::Java,
            _ => Target::Rust,
        };
        let code = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error reading file: {}", e);
                process::exit(1);
            }
        };
        let result = transpile(target, &code).expect("Transpilation failed");
        println!("{}", result);
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

    let stmts = match parse_phs(&code) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error parsing script: {:?}", e);
            process::exit(1);
        }
    };

    let mut interp = PhsInterpreter::new();
    let mut vars_map = HashMap::new();
    let mut steps = Vec::new();

    if !is_tui && !is_web && !is_view {
        RichRenderer::render_header(raw_input);
    }

    for stmt in stmts {
        let (label, expr_code, latex_expr, is_disp) = match stmt {
            physure_script::Statement::Assignment(ref node) => {
                (node.name.clone(), "expr".to_string(), "latex".to_string(), false)
            }
            _ => ("expr".to_string(), "expr".to_string(), "latex".to_string(), false)
        };

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
                eprintln!("error executing statement: {:?}", e);
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
