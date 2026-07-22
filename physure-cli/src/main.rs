use std::env;
use std::fs;
use std::process;
use physure_script::{parse_phs, transpile, PhsInterpreter, PhsValue, Target};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("PhysureScript (PHS) CLI");
        eprintln!("Usage: phs <script.phs>");
        eprintln!("       phs transpile <script.phs> --target <rust|python|java>");
        process::exit(1);
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

    let input_arg = &args[1];
    let code = if let Ok(content) = fs::read_to_string(input_arg) {
        content
    } else if input_arg.ends_with(".phs") {
        eprintln!("error: file not found '{}'", input_arg);
        process::exit(1);
    } else {
        input_arg.clone()
    };

    let stmts = match parse_phs(&code) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error parsing script: {:?}", e);
            process::exit(1);
        }
    };

    let mut interp = PhsInterpreter::new();
    for stmt in stmts {
        match interp.run_statement(&stmt) {
            Ok(val) => {
                if val != PhsValue::None {
                    println!("{}", val);
                }
            }
            Err(e) => {
                eprintln!("error executing statement: {:?}", e);
                process::exit(1);
            }
        }
    }
}
