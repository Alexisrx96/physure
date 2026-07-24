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
    widgets::{Block, Borders, Paragraph, Row, Table, TableState},
    Terminal,
};
use physure_script::value::{PhsValue, PlotData};
use arboard::Clipboard;
use crate::step::ExecutionStep;

pub fn run_tui(code: &str, steps: &[ExecutionStep], _vars: &HashMap<String, PhsValue>) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut table_state = TableState::default();
    if !steps.is_empty() {
        table_state.select(Some(0));
    }
    let mut code_scroll: u16 = 0;
    let mut status_msg = String::from("Nav: ▲/▼ (Select Step) | w/s (Scroll Code) | 'c' (Copy) | 'q' (Quit)");

    let code_lines: Vec<&str> = code.lines().collect();

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(12),
                    Constraint::Length(3),
                ])
                .split(f.area());

            let header = Paragraph::new("Physure TUI Dashboard v0.2.4 — Interactive Physical Computation")
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::ALL).title("Physure Interactive Inspector"));
            f.render_widget(header, chunks[0]);

            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
                .split(chunks[1]);

            // Code View Panel
            let code_visible_lines: Vec<&str> = code_lines.iter().skip(code_scroll as usize).copied().collect();
            let code_block = Paragraph::new(code_visible_lines.join("\n"))
                .style(Style::default().fg(Color::Gray))
                .block(Block::default().borders(Borders::ALL).title(format!("PHS Script (Line {})", code_scroll + 1)));
            f.render_widget(code_block, main_chunks[0]);

            // Right Panel: Split into Table & Inspection Card
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(main_chunks[1]);

            // Table of Calculation Steps
            let rows: Vec<Row> = steps
                .iter()
                .map(|step| {
                    let val_str = step.value.to_string();
                    let truncated_val = if val_str.len() > 30 { format!("{}...", &val_str[..27]) } else { val_str };
                    Row::new(vec![step.label.clone(), step.expr_code.clone(), truncated_val])
                })
                .collect();

            let table = Table::new(rows, [Constraint::Percentage(25), Constraint::Percentage(35), Constraint::Percentage(40)])
                .header(Row::new(vec!["Label", "Expression", "Value"]).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
                .highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD))
                .highlight_symbol("▸ ")
                .block(Block::default().borders(Borders::ALL).title("Calculations & Steps"));

            f.render_stateful_widget(table, right_chunks[0], &mut table_state);

            // Inspection Detail Card
            let selected_idx = table_state.selected().unwrap_or(0);
            let detail_text = if let Some(step) = steps.get(selected_idx) {
                match &step.value {
                    PhsValue::Quantity(q) => {
                        format!(
                            "Target: {}\nExpr: {}\nValue: {}\nUnit: {}\nRepr: {}",
                            step.label, step.expr_code, q, q.unit.__repr__(), q.to_string()
                        )
                    }
                    PhsValue::Plot(PlotData { title, ascii, .. }) => {
                        format!("Plot: {}\n{}", title, ascii)
                    }
                    PhsValue::String(s) if step.is_display_text => {
                        format!("Doc Block:\n{}", s)
                    }
                    _ => {
                        format!("Target: {}\nExpr: {}\nEvaluated: {}", step.label, step.expr_code, step.value)
                    }
                }
            } else {
                String::from("No step selected")
            };

            let detail_card = Paragraph::new(detail_text)
                .style(Style::default().fg(Color::Green))
                .block(Block::default().borders(Borders::ALL).title("Selected Step Details"));
            f.render_widget(detail_card, right_chunks[1]);

            let footer = Paragraph::new(status_msg.as_str())
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().borders(Borders::ALL).title("Keyboard Actions"));
            f.render_widget(footer, chunks[2]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Down => {
                        if !steps.is_empty() {
                            let next = match table_state.selected() {
                                Some(i) => (i + 1) % steps.len(),
                                None => 0,
                            };
                            table_state.select(Some(next));
                        }
                    }
                    KeyCode::Up => {
                        if !steps.is_empty() {
                            let prev = match table_state.selected() {
                                Some(i) => if i == 0 { steps.len() - 1 } else { i - 1 },
                                None => 0,
                            };
                            table_state.select(Some(prev));
                        }
                    }
                    KeyCode::Char('w') | KeyCode::PageUp => {
                        if code_scroll > 0 {
                            code_scroll -= 1;
                        }
                    }
                    KeyCode::Char('s') | KeyCode::PageDown => {
                        if (code_scroll as usize) + 1 < code_lines.len() {
                            code_scroll += 1;
                        }
                    }
                    KeyCode::Char('c') => {
                        if let Some(idx) = table_state.selected() {
                            if let Some(step) = steps.get(idx) {
                                let copy_str = format!("{} = {}", step.label, step.value);
                                if let Ok(mut cb) = Clipboard::new() {
                                    if cb.set_text(copy_str.clone()).is_ok() {
                                        status_msg = format!("Copied to clipboard: {}", copy_str);
                                    }
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
