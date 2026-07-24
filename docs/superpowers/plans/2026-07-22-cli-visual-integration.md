# Physure-CLI Visual Integration (Hybrid Rich Terminal + TUI + Web) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform `physure-cli` (`phs`) into a visual CLI with three complementary visual presentation modes:
1. **Rich Terminal Cards (Default)**: High-definition ANSI box-drawing cards, unicode braille plots, and copyable summary blocks.
2. **Interactive TUI (`phs --tui` / `phs repl`)**: Multi-panel terminal dashboard powered by `ratatui` and `crossterm` with live variable inspection, keyboard navigation, braille plot zoom, and clipboard copy (Text, LaTeX, JSON).
3. **Web Visualizer (`phs --web` / `phs serve`)**: Zero-dependency local web server (`tiny_http`) that opens `http://localhost:3000` with a modern glassmorphism GUI, KaTeX math formulas, Chart.js interactive plots, and export capabilities.

**Architecture:**
- `physure-cli/src/rich.rs`: Terminal box renderer, ANSI color highlights, unicode braille chart renderer, copyable summary cards.
- `physure-cli/src/tui.rs`: Ratatui multi-panel dashboard (Code, Variable Inspector, Braille Plot Canvas, Copy Action Bar).
- `physure-cli/src/web.rs`: Embedded HTML/CSS/JS single-page visualizer served via `tiny_http` and opened via `open::that`.
- `physure-cli/src/main.rs`: CLI argument routing (`--tui`, `--web`, default rich mode, `transpile`, `repl`).

---

### Task 1: Add Dependencies to `physure-cli/Cargo.toml`

**Files:**
- Modify: `D:\Projects\physure\physure-cli\Cargo.toml`

- [ ] **Step 1: Add `ratatui`, `crossterm`, `arboard`, `tiny_http`, `open` to `physure-cli/Cargo.toml`**

```toml
[package]
name        = "physure-cli"
description = "Visual command line interface for PhysureScript (.phs)."
version.workspace    = true
edition.workspace    = true
license.workspace    = true
repository.workspace = true
homepage.workspace   = true
authors.workspace    = true

[[bin]]
name = "phs"
path = "src/main.rs"

[dependencies]
physure-core   = { path = "../physure-core" }
physure-script = { path = "../physure-script" }
ratatui        = "0.28"
crossterm      = "0.28"
arboard        = "3.4"
tiny_http      = "0.12"
open           = "5.3"
serde_json     = "1"
```

---

### Task 2: Implement Rich Terminal Cards & Braille Plotter (`physure-cli/src/rich.rs`)

**Files:**
- Create: `D:\Projects\physure\physure-cli\src\rich.rs`

- [ ] **Step 1: Create `rich.rs` with unicode box drawing, ANSI color styling, and braille plotting**

```rust
use physure_script::value::PhsValue;
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
                println!("\x1b[32m├─▸ \x1b[1;37m{:<16}\x1b[0m = \x1b[1;33m{:<24}\x1b[0m \x1b[90m[{}]\x1b[0m", name, q.to_string(), q.unit.__repr__());
            }
            PhsValue::Number(n) => {
                println!("\x1b[32m├─▸ \x1b[1;37m{:<16}\x1b[0m = \x1b[1;33m{:<24}\x1b[0m", name, n);
            }
            _ => {
                println!("\x1b[32m├─▸ \x1b[1;37m{:<16}\x1b[0m = {}", name, val);
            }
        }
    }

    pub fn render_summary_box(vars: &HashMap<String, PhsValue>) {
        println!("\n\x1b[1;34m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
        println!("\x1b[1;34m║ \x1b[1;37mCOPYABLE SUMMARY (PHYSICAL QUANTITIES & RESULTS)\x1b[1;34m            ║\x1b[0m");
        println!("\x1b[1;34m╠══════════════════════════════════════════════════════════════╣\x1b[0m");
        for (k, v) in vars {
            println!("\x1b[1;34m║ \x1b[36m{:<16}\x1b[0m : \x1b[37m{:<39}\x1b[1;34m ║\x1b[0m", k, v.to_string());
        }
        println!("\x1b[1;34m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    }
}
```

---

### Task 3: Implement Interactive Ratatui TUI Dashboard (`physure-cli/src/tui.rs`)

**Files:**
- Create: `D:\Projects\physure\physure-cli\src\tui.rs`

- [ ] **Step 1: Create `tui.rs` with multi-panel layout, live inspection, keyboard navigation, and clipboard copy**

```rust
use std::collections::HashMap;
use std::io;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table},
    Terminal,
};
use physure_script::value::PhsValue;
use arboard::Clipboard;

pub fn run_tui(code: &str, vars: &HashMap<String, PhsValue>) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut selected_idx = 0;
    let var_list: Vec<(&String, &PhsValue)> = vars.iter().collect();
    let mut status_msg = String::from("Press 'c' to copy selected variable | 'q' or Esc to exit");

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(10),
                    Constraint::Length(3),
                ])
                .split(f.area());

            let header = Paragraph::new("Physure TUI Dashboard v0.2.4")
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::ALL).title("Physure Interactive Inspection"));
            f.render_widget(header, chunks[0]);

            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(chunks[1]);

            let code_block = Paragraph::new(code)
                .style(Style::default().fg(Color::White))
                .block(Block::default().borders(Borders::ALL).title("PHS Source Code"));
            f.render_widget(code_block, main_chunks[0]);

            let rows: Vec<Row> = var_list
                .iter()
                .enumerate()
                .map(|(i, (k, v))| {
                    let style = if i == selected_idx {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Green)
                    };
                    Row::new(vec![k.to_string(), v.to_string()]).style(style)
                })
                .collect();

            let table = Table::new(rows, [Constraint::Percentage(40), Constraint::Percentage(60)])
                .header(Row::new(vec!["Variable", "Value"]).style(Style::default().fg(Color::Magenta)))
                .block(Block::default().borders(Borders::ALL).title("Variables & Quantities"));
            f.render_widget(table, main_chunks[1]);

            let footer = Paragraph::new(status_msg.as_str())
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().borders(Borders::ALL).title("Actions"));
            f.render_widget(footer, chunks[2]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Down => {
                        if !var_list.is_empty() {
                            selected_idx = (selected_idx + 1) % var_list.len();
                        }
                    }
                    KeyCode::Up => {
                        if !var_list.is_empty() {
                            selected_idx = if selected_idx == 0 { var_list.len() - 1 } else { selected_idx - 1 };
                        }
                    }
                    KeyCode::Char('c') => {
                        if selected_idx < var_list.len() {
                            let (k, v) = var_list[selected_idx];
                            let copy_str = format!("{} = {}", k, v);
                            if let Ok(mut cb) = Clipboard::new() {
                                if cb.set_text(copy_str.clone()).is_ok() {
                                    status_msg = format!("Copied to clipboard: {}", copy_str);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
```

---

### Task 4: Implement Web Live Visualizer Server (`physure-cli/src/web.rs`)

**Files:**
- Create: `D:\Projects\physure\physure-cli\src\web.rs`

- [ ] **Step 1: Create `web.rs` with embedded HTML GUI and HTTP server**

```rust
use std::collections::HashMap;
use tiny_http::{Response, Server};
use physure_script::value::PhsValue;

pub fn start_web_server(code: &str, vars: &HashMap<String, PhsValue>) -> Result<(), Box<dyn std::error::Error>> {
    let server = Server::http("127.0.0.1:3000").map_err(|e| format!("{}", e))?;
    println!("\x1b[1;32m🚀 Physure Web Visualizer running at http://localhost:3000\x1b[0m");
    let _ = open::that("http://localhost:3000");

    let mut rows_html = String::new();
    for (k, v) in vars {
        rows_html.push_str(&format!(
            "<tr><td class='font-mono text-cyan-400'>{}</td><td class='font-mono text-amber-300'>{}</td><td><button onclick=\"navigator.clipboard.writeText('{} = {}')\" class='px-2 py-1 bg-slate-700 hover:bg-cyan-600 text-xs rounded text-white'>Copy</button></td></tr>",
            k, v, k, v
        ));
    }

    let html_content = format!(r#"
<!DOCTYPE html>
<html lang="en" class="dark">
<head>
    <meta charset="UTF-8">
    <title>Physure Visualizer</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16.8/dist/katex.min.css">
</head>
<body class="bg-slate-950 text-slate-100 min-h-screen p-8">
    <div class="max-w-5xl mx-auto space-y-6">
        <header class="flex justify-between items-center border-b border-slate-800 pb-4">
            <div>
                <h1 class="text-3xl font-bold bg-gradient-to-r from-cyan-400 to-blue-500 bg-clip-text text-transparent">Physure Script Dashboard</h1>
                <p class="text-slate-400 text-sm">Interactive Physical Quantity Inspector</p>
            </div>
            <span class="px-3 py-1 bg-cyan-950 border border-cyan-800 text-cyan-400 text-xs rounded-full">v0.2.4 Live</span>
        </header>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-xl">
                <h2 class="text-lg font-semibold text-cyan-400 mb-3">PHS Source Code</h2>
                <pre class="bg-slate-950 p-4 rounded-lg font-mono text-sm text-slate-300 overflow-x-auto">{}</pre>
            </div>

            <div class="bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-xl">
                <h2 class="text-lg font-semibold text-emerald-400 mb-3">Quantities & Variables</h2>
                <table class="w-full text-left text-sm border-collapse">
                    <thead>
                        <tr class="border-b border-slate-800 text-slate-400">
                            <th class="pb-2">Variable</th>
                            <th class="pb-2">Value</th>
                            <th class="pb-2">Action</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-slate-800/50">
                        {}
                    </tbody>
                </table>
            </div>
        </div>
    </div>
</body>
</html>
    "#, code, rows_html);

    for request in server.incoming_requests() {
        let response = Response::from_string(&html_content)
            .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap());
        let _ = request.respond(response);
    }
    Ok(())
}
```

---

### Task 5: Integration in `physure-cli/src/main.rs` & CLI Options

**Files:**
- Modify: `D:\Projects\physure\physure-cli\src\main.rs`

- [ ] **Step 1: Wire `--tui` and `--web` flags into `main.rs`**
- [ ] **Step 2: Run `cargo test --workspace`**
- [ ] **Step 3: Execute `cargo run --bin phs -- D:\Projects\test_physure\1_cargas.phs` to verify rich default output.**
