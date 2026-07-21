use std::env;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::process;
use physure::phs::{eval_phs, PhsValue};

fn print_results(results: &[PhsValue]) {
    for res in results {
        let s = res.to_string();
        if !s.is_empty() {
            println!("{}", s);
        }
    }
}

fn run_source(source: &str) -> i32 {
    match eval_phs(source) {
        Ok(results) => {
            print_results(&results);
            0
        }
        Err(e) => {
            eprintln!("error: {}", e);
            1
        }
    }
}

fn repl() {
    println!("physure — native standalone PHS calculator.");
    println!("Try `500 N / 2 m^2 => kPa`; exit with Ctrl-D or 'exit'.");

    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let mut line = String::new();

    loop {
        print!("phs> ");
        if io::stdout().flush().is_err() {
            break;
        }
        line.clear();
        match handle.read_line(&mut line) {
            Ok(0) => {
                println!();
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed == "exit" || trimmed == "quit" {
                    break;
                }
                if trimmed.is_empty() {
                    continue;
                }
                match eval_phs(trimmed) {
                    Ok(results) => print_results(&results),
                    Err(e) => eprintln!("error: {}", e),
                }
            }
            Err(e) => {
                eprintln!("error reading input: {}", e);
                break;
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if !args.is_empty() {
        let first = &args[0];
        if first == "-h" || first == "--help" {
            println!("Usage: phs [FILE | EXPR]");
            println!("  phs script.phs        # Execute .phs script file");
            println!("  phs \"500 N / 2 m^2\"   # Evaluate expression");
            println!("  phs                   # Run interactive REPL");
            process::exit(0);
        }

        if std::path::Path::new(first).is_file() {
            match fs::read_to_string(first) {
                Ok(content) => process::exit(run_source(&content)),
                Err(e) => {
                    eprintln!("error reading file '{}': {}", first, e);
                    process::exit(1);
                }
            }
        }

        let expr = args.join(" ");
        process::exit(run_source(&expr));
    }

    if io::stdin().is_terminal() {
        repl();
    } else {
        let mut buffer = String::new();
        if io::stdin().read_to_string(&mut buffer).is_ok() {
            process::exit(run_source(&buffer));
        }
    }
}
