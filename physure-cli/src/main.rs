use std::collections::HashMap;
use std::env;
use std::fs;
use std::process;
use physure_script::{parse_phs, transpile, PhsInterpreter, PhsValue, Target};

mod config;
mod debug;
mod export;
mod html;
mod katex_assets;
mod latex;
mod protocol;
mod rich;
mod scaffold;
mod step;
mod tui;
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
    println!("    phs doc [--save]         Generate full Markdown language & syntax specification");
    println!();
    println!("FLAGS & OPTIONS:");
    println!("    -h, --help               Print this help information");
    println!("    -r, --repl               Start interactive PHS REPL environment");
    println!("    -t, --target <lang>      Transpile target: rust, python, java, js, ts (default: rust)");
    println!("    -o, --output <file>      Specify output file path (e.g. out.py, Main.java)");
    println!("    --doc, doc [--save]      Generate full Markdown reference specification");
    println!("    --tui                    Launch terminal UI dashboard mode");
    println!("    --web                    Launch local web visualizer server");
    println!("    --html, --view           Generate and open HTML report");
    println!();
    println!("EXAMPLES:");
    println!("    phs 1_cargas.phs");
    println!("    phs --repl");
    println!("    phs doc --save");
    println!("    phs transpile 1_cargas.phs --target python");
    println!("    phs transpile 1_cargas.phs -t java -o Calculator.java");
    println!("    phs new-plugin myplugin --lang rust");
    println!("    phs export orbit_sim.phs --fn kinetic_energy --native -o dist/");
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
        (Some("js") | Some("javascript"), _) => Target::JavaScript,
        (Some("ts") | Some("typescript"), _) => Target::TypeScript,
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
            } else if out_p.ends_with(".ts") {
                Target::TypeScript
            } else if out_p.ends_with(".js") {
                Target::JavaScript
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

fn run_daemon() {
    use std::io::{self, BufRead, Write};
    use serde::{Deserialize, Serialize};

    #[derive(Deserialize)]
    struct DaemonRequest {
        id: usize,
        source: String,
        /// Path of the file being edited, sent by the editor so `use ... from
        /// <plugin/module>` can be resolved relative to it. `None` for ad-hoc
        /// snippets with no file (falls back to no plugin/module resolution).
        #[serde(default)]
        path: Option<String>,
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
        let interp = match req.path.as_deref().and_then(|p| std::path::Path::new(p).parent()) {
            Some(dir) if !dir.as_os_str().is_empty() => PhsInterpreter::with_base_dir(dir),
            _ => PhsInterpreter::default(),
        };
        let mut env = HashMap::new();

        match physure_script::parse_phs_with_lines(&req.source) {
            Ok(statements_with_lines) => {
                for (line_num, stmt) in statements_with_lines {
                    match interp.eval_statement_with_env(&stmt, &mut env) {
                        Ok(val) => {
                            if val != PhsValue::None {
                                let output = match &val {
                                    PhsValue::Plot(p) => {
                                        let trimmed = p.svg.trim_start();
                                        if trimmed.starts_with("<!DOCTYPE") || trimmed.starts_with("<html") {
                                            let b64 = base64_encode_bytes(p.svg.as_bytes());
                                            format!("[PLOT_HTML:data:text/html;charset=utf-8;base64,{}]", b64)
                                        } else {
                                            format!("[PLOT_IMAGE:data:image/svg+xml;utf8,{}]", p.svg)
                                        }
                                    }
                                    _ => val.to_string(),
                                };
                                results.push(DaemonLineResult { line: line_num, output });
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
    md.push_str("presion = 100.0 kPa\n");
    md.push_str("velocidad = 25.0 m / s\n");
    md.push_str("resistencia = 50.0 Ohm\n");
    md.push_str("medicion = 10.0 +/- 0.5 m        # Measurement with 0.5 m standard deviation\n");
    md.push_str("porcentaje = 5.0 %               # Relative uncertainty or percentage\n\n");
    md.push_str("# Unit Conversion Operator '=>'\n");
    md.push_str("p_bar = 100.0 kPa => bar         # Converts 100.0 kPa to bar\n");
    md.push_str("v_kmh = 25.0 m / s => km / h     # Converts 25 m/s to km/h\n");
    md.push_str("```\n\n");

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
    md.push_str("duplo = x * 2.0 where x = 10.0 m\n");
    md.push_str("estado = if presion > 50.0 kPa then \"Alta Presion\" else \"Presion Normal\"\n");
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
    md.push_str("| `calc` | `integral(expr, var)` | Symbolic integral | `integral(m * g, h)` |\n");
    md.push_str("| `array` | `linspace(a, b, n)` | Vector generation | `linspace(0.0 m, 10.0 m, 100)` |\n");
    md.push_str("| `array` | `gradient(y, x)` | Numerical derivative dy/dx | `gradient(presion_vec, pos_vec)` |\n");
    md.push_str("| `array` | `trapz(y, x)` | Numerical integration (area) | `trapz(fuerza_vec, pos_vec)` |\n\n");

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

    if args.iter().any(|a| a == "--doc" || a == "doc" || a == "docs" || a == "--docs") {
        run_doc_generator(&args);
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

    if args[1] == "new-plugin" {
        scaffold::run_new_plugin(&args);
        return;
    }

    if args[1] == "export" {
        export::run_export(&args);
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
                RichRenderer::render_runtime_error(raw_input, &e, &expr_code);
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
