use std::collections::HashMap;
use std::io;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Row, Table, TableState},
    Frame, Terminal,
};
use physure_script::value::{PhsValue, PlotData};
use crate::step::ExecutionStep;

/// Puts a text on the clipboard; false when it could not.
type SetClipboard = Box<dyn FnMut(&str) -> bool>;

/// The clipboard of the system. A machine without one (a server, a session over SSH) just does not copy.
fn system_clipboard(text: &str) -> bool {
    arboard::Clipboard::new().is_ok_and(|mut clipboard| clipboard.set_text(text).is_ok())
}

/// What the dashboard remembers between frames and keys: the selected step, how far the code is scrolled, and
/// the line of text at the bottom.
struct DashboardState {
    table_state: TableState,
    code_scroll: u16,
    status_msg: String,
    set_clipboard: SetClipboard,
}

impl DashboardState {
    fn new(step_count: usize) -> Self {
        Self::with_clipboard(step_count, Box::new(system_clipboard))
    }

    fn with_clipboard(step_count: usize, set_clipboard: SetClipboard) -> Self {
        let mut table_state = TableState::default();
        if step_count > 0 {
            table_state.select(Some(0));
        }
        DashboardState {
            table_state,
            code_scroll: 0,
            status_msg: String::from("Nav: ▲/▼ (Select Step) | w/s (Scroll Code) | 'c' (Copy) | 'q' (Quit)"),
            set_clipboard,
        }
    }

    fn select_next(&mut self, step_count: usize) {
        if step_count == 0 {
            return;
        }
        let next = match self.table_state.selected() {
            Some(i) => (i + 1) % step_count,
            None => 0,
        };
        self.table_state.select(Some(next));
    }

    fn select_previous(&mut self, step_count: usize) {
        if step_count == 0 {
            return;
        }
        let prev = match self.table_state.selected() {
            Some(0) => step_count - 1,
            Some(i) => i - 1,
            None => 0,
        };
        self.table_state.select(Some(prev));
    }

    fn copy_selected(&mut self, steps: &[ExecutionStep]) {
        let Some(step) = self.table_state.selected().and_then(|idx| steps.get(idx)) else {
            return;
        };
        let copy_str = format!("{} = {}", step.label, step.value);
        if (self.set_clipboard)(&copy_str) {
            self.status_msg = format!("Copied to clipboard: {}", copy_str);
        }
    }

    /// Applies a key to the state; false when the key asks to leave.
    fn handle_key(&mut self, key: KeyCode, steps: &[ExecutionStep], code_line_count: usize) -> bool {
        match key {
            KeyCode::Char('q') | KeyCode::Esc => return false,
            KeyCode::Down => self.select_next(steps.len()),
            KeyCode::Up => self.select_previous(steps.len()),
            KeyCode::Char('w') | KeyCode::PageUp => self.code_scroll = self.code_scroll.saturating_sub(1),
            KeyCode::Char('s') | KeyCode::PageDown => {
                if (self.code_scroll as usize) + 1 < code_line_count {
                    self.code_scroll += 1;
                }
            }
            KeyCode::Char('c') => self.copy_selected(steps),
            _ => {}
        }
        true
    }
}

/// The text of the card under the table: the selected step, as the kind of value it has.
fn detail_text(step: Option<&ExecutionStep>) -> String {
    let Some(step) = step else {
        return String::from("No step selected");
    };
    match &step.value {
        PhsValue::Quantity(q) => {
            format!(
                "Target: {}\nExpr: {}\nValue: {}\nUnit: {}\nRepr: {}",
                step.label, step.expr_code, q, q.unit.__repr__(), q
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
}

/// The table of calculation steps, a value cut to 30 characters.
fn step_rows(steps: &[ExecutionStep]) -> Vec<Row<'static>> {
    steps
        .iter()
        .map(|step| {
            let val_str = step.value.to_string();
            let truncated_val = if val_str.chars().count() > 30 {
                format!("{}...", val_str.chars().take(27).collect::<String>())
            } else {
                val_str
            };
            Row::new(vec![step.label.clone(), step.expr_code.clone(), truncated_val])
        })
        .collect()
}

fn draw_dashboard(f: &mut Frame<'_>, code_lines: &[&str], steps: &[ExecutionStep], state: &mut DashboardState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(f.area());

    let header = Paragraph::new(format!(
        "Physure TUI Dashboard v{} — Interactive Physical Computation",
        env!("CARGO_PKG_VERSION")
    ))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("Physure Interactive Inspector"));
    f.render_widget(header, chunks[0]);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(chunks[1]);

    // Code View Panel
    let code_visible_lines: Vec<&str> = code_lines.iter().skip(state.code_scroll as usize).copied().collect();
    let code_block = Paragraph::new(code_visible_lines.join("\n"))
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL).title(format!("PHS Script (Line {})", state.code_scroll + 1)));
    f.render_widget(code_block, main_chunks[0]);

    // Right Panel: Split into Table & Inspection Card
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(main_chunks[1]);

    // Table of Calculation Steps
    let table = Table::new(step_rows(steps), [Constraint::Percentage(25), Constraint::Percentage(35), Constraint::Percentage(40)])
        .header(Row::new(vec!["Label", "Expression", "Value"]).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
        .highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD))
        .highlight_symbol("▸ ")
        .block(Block::default().borders(Borders::ALL).title("Calculations & Steps"));

    f.render_stateful_widget(table, right_chunks[0], &mut state.table_state);

    // Inspection Detail Card
    let selected_idx = state.table_state.selected().unwrap_or(0);
    let detail_card = Paragraph::new(detail_text(steps.get(selected_idx)))
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL).title("Selected Step Details"));
    f.render_widget(detail_card, right_chunks[1]);

    let footer = Paragraph::new(state.status_msg.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Keyboard Actions"));
    f.render_widget(footer, chunks[2]);
}

/// Draws the dashboard and applies the keys `next_event` brings, until one asks to leave. `next_event` waits a
/// moment for the terminal and says what came, if anything. A key that is released is not a key pressed: Windows
/// terminals report both, and a key would otherwise act twice.
fn run_dashboard<B: Backend>(
    terminal: &mut Terminal<B>,
    code_lines: &[&str],
    steps: &[ExecutionStep],
    state: &mut DashboardState,
    mut next_event: impl FnMut() -> io::Result<Option<Event>>,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| draw_dashboard(f, code_lines, steps, state))?;

        if let Some(Event::Key(key)) = next_event()? {
            if key.kind != KeyEventKind::Release && !state.handle_key(key.code, steps, code_lines.len()) {
                return Ok(());
            }
        }
    }
}

pub fn run_tui(code: &str, steps: &[ExecutionStep], _vars: &HashMap<String, PhsValue>) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = DashboardState::new(steps.len());
    let code_lines: Vec<&str> = code.lines().collect();

    let result = run_dashboard(&mut terminal, &code_lines, steps, &mut state, || {
        if event::poll(std::time::Duration::from_millis(100))? {
            event::read().map(Some)
        } else {
            Ok(None)
        }
    });

    // the terminal is given back even when the dashboard failed
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(result?)
}

#[cfg(test)]
mod tests;
