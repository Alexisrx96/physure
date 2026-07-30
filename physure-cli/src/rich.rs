use physure_script::value::{PhsValue, PlotData};
use std::collections::HashMap;

pub struct RichRenderer;

impl RichRenderer {
    pub fn render_header(title: &str) {
        println!("\x1b[1;36m┌──────────────────────────────────────────────────────────────┐\x1b[0m");
        println!("\x1b[1;36m│ \x1b[1;37mPhysure Engine Execution: {:<34}\x1b[1;36m │\x1b[0m", title);
        println!("\x1b[1;36m└──────────────────────────────────────────────────────────────┘\x1b[0m");
    }

    pub fn render_variable_card(name: &str, val: &PhsValue) {
        match val {
            PhsValue::Quantity(q) => {
                println!("\x1b[1;36m{:<24}\x1b[0m = \x1b[1;33m{}\x1b[0m", name, q.to_string());
            }
            PhsValue::Number(n) => {
                println!("\x1b[1;36m{:<24}\x1b[0m = \x1b[1;33m{}\x1b[0m", name, n);
            }
            PhsValue::Plot(PlotData { ascii, .. }) => {
                println!("\n{}", ascii);
            }
            _ => {
                println!("\x1b[1;36m{:<24}\x1b[0m = {}", name, val);
            }
        }
    }

    pub fn render_summary_box(vars: &HashMap<String, PhsValue>) {
        if vars.is_empty() {
            return;
        }
        println!("\n\x1b[1;34m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
        println!("\x1b[1;34m║ \x1b[1;37mCOPYABLE SUMMARY (PHYSICAL QUANTITIES & RESULTS)\x1b[1;34m            ║\x1b[0m");
        println!("\x1b[1;34m╠══════════════════════════════════════════════════════════════╣\x1b[0m");
        for (k, v) in vars {
            if matches!(v, PhsValue::Plot(_)) {
                continue;
            }
            let val_str = v.to_string();
            let truncated_v = if val_str.len() > 38 { format!("{}...", &val_str[..35]) } else { val_str };
            println!("\x1b[1;34m║ \x1b[36m{:<16}\x1b[0m : \x1b[37m{:<39}\x1b[1;34m ║\x1b[0m", k, truncated_v);
        }
        println!("\x1b[1;34m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    }

    pub fn render_parse_error(file: &str, err: &dyn std::fmt::Debug, code: &str) {
        let raw_err = format!("{:?}", err);
        let clean_msg = raw_err
            .trim_start_matches("Generic(\"")
            .trim_end_matches("\")")
            .replace("\\n", "\n")
            .replace("␊", "");

        eprintln!("\n\x1b[1;31m┌── ❌ Physure Syntax Error ──────────────────────────────────────────┐\x1b[0m");
        eprintln!("\x1b[1;31m│\x1b[0m \x1b[1;37mFile:\x1b[0m \x1b[36m{}\x1b[0m", file);
        for line in clean_msg.lines() {
            if line.contains("-->") {
                eprintln!("\x1b[1;31m│\x1b[0m \x1b[1;33mLocation: {}\x1b[0m", line.trim());
            } else if line.contains('|') {
                eprintln!("\x1b[1;31m│\x1b[0m \x1b[37m{}\x1b[0m", line);
            } else if line.contains('=') {
                eprintln!("\x1b[1;31m│\x1b[0m \x1b[36m{}\x1b[0m", line.trim());
            } else if !line.trim().is_empty() {
                eprintln!("\x1b[1;31m│\x1b[0m \x1b[1;31m{}\x1b[0m", line.trim());
            }
        }
        eprintln!("\x1b[1;31m└─────────────────────────────────────────────────────────────────────┘\x1b[0m\n");
    }

    pub fn render_runtime_error(file: &str, err: &dyn std::fmt::Display, stmt_code: &str) {
        eprintln!("\n\x1b[1;31m┌── ❌ Physure Execution & Dimensional Analysis Error ─────────────────┐\x1b[0m");
        eprintln!("\x1b[1;31m│\x1b[0m \x1b[1;37mFile:\x1b[0m \x1b[36m{}\x1b[0m", file);
        if !stmt_code.is_empty() {
            eprintln!("\x1b[1;31m│\x1b[0m \x1b[1;37mStatement:\x1b[0m \x1b[33m{}\x1b[0m", stmt_code.trim());
        }
        eprintln!("\x1b[1;31m│\x1b[0m \x1b[1;31mError Details: {}\x1b[0m", err);
        eprintln!("\x1b[1;31m└─────────────────────────────────────────────────────────────────────┘\x1b[0m\n");
    }
}
