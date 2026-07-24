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
}
