use super::*;
use crossterm::event::{KeyEvent, KeyModifiers};
use physure_core::quantity::Quantity;
use physure_core::units::RationalUnit;
use ratatui::backend::TestBackend;
use std::cell::RefCell;
use std::rc::Rc;

fn step(label: &str, expr: &str, value: PhsValue) -> ExecutionStep {
    ExecutionStep {
        label: label.to_string(),
        expr_code: expr.to_string(),
        latex_expr: String::new(),
        value,
        is_display_text: false,
        precision_override: None,
    }
}

fn joules(value: f64) -> PhsValue {
    PhsValue::Quantity(Quantity::new_scalar(value, 0.0, RationalUnit::base("J"), None, None))
}

fn steps() -> Vec<ExecutionStep> {
    vec![
        step("E", "m * c^2", joules(625.0)),
        step("n", "3", PhsValue::Number(3.0)),
        step("ok", "1 < 2", PhsValue::Bool(true)),
    ]
}

/// A dashboard whose clipboard is a list of what was copied, and whether copying works.
fn dashboard(step_count: usize, copying: bool) -> (DashboardState, Rc<RefCell<Vec<String>>>) {
    let copies = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&copies);
    let state = DashboardState::with_clipboard(
        step_count,
        Box::new(move |text| {
            sink.borrow_mut().push(text.to_string());
            copying
        }),
    );
    (state, copies)
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

/// The text on the screen of a terminal that is not one: each row of the buffer on a line.
fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    buffer
        .content()
        .chunks(buffer.area.width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn drawn(code: &[&str], steps: &[ExecutionStep], state: &mut DashboardState, width: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, 30)).unwrap();
    terminal.draw(|f| draw_dashboard(f, code, steps, state)).unwrap();
    screen(&terminal)
}

#[test]
fn the_card_describes_the_step_by_the_kind_of_value_it_has() {
    assert_eq!(detail_text(None), "No step selected");

    let steps = steps();
    let quantity = detail_text(Some(&steps[0]));
    assert!(quantity.starts_with("Target: E\nExpr: m * c^2\nValue: "), "{quantity}");
    assert!(quantity.contains("Unit: J") && quantity.contains("Repr: "), "{quantity}");
    assert_eq!(detail_text(Some(&steps[1])), "Target: n\nExpr: 3\nEvaluated: 3.0");

    let plot = step(
        "p",
        "plot(x)",
        PhsValue::Plot(PlotData {
            title: "Speed".to_string(),
            x_unit: "s".to_string(),
            y_unit: "m".to_string(),
            ascii: "/\\_".to_string(),
            svg: String::new(),
        }),
    );
    assert_eq!(detail_text(Some(&plot)), "Plot: Speed\n/\\_");

    let mut text = step("doc", "", PhsValue::String("A paragraph.".to_string()));
    assert_eq!(detail_text(Some(&text)), "Target: doc\nExpr: \nEvaluated: A paragraph.");
    text.is_display_text = true;
    assert_eq!(detail_text(Some(&text)), "Doc Block:\nA paragraph.");
}

#[test]
fn a_new_dashboard_starts_on_the_first_step_at_the_top_of_the_code() {
    // the system clipboard is not touched until a step is copied
    let state = DashboardState::new(2);
    assert_eq!((state.table_state.selected(), state.code_scroll), (Some(0), 0));
    assert!(state.status_msg.starts_with("Nav:"));
    assert_eq!(DashboardState::new(0).table_state.selected(), None);
}

#[test]
fn the_selection_moves_around_the_steps_and_stays_put_without_any() {
    let (mut state, _) = dashboard(3, true);
    assert_eq!(state.table_state.selected(), Some(0));
    state.select_next(3);
    state.select_next(3);
    assert_eq!(state.table_state.selected(), Some(2));
    state.select_next(3);
    assert_eq!(state.table_state.selected(), Some(0), "past the last step comes the first");
    state.select_previous(3);
    assert_eq!(state.table_state.selected(), Some(2), "before the first step comes the last");
    state.select_previous(3);
    assert_eq!(state.table_state.selected(), Some(1));

    state.table_state.select(None);
    state.select_previous(3);
    assert_eq!(state.table_state.selected(), Some(0));
    state.table_state.select(None);
    state.select_next(3);
    assert_eq!(state.table_state.selected(), Some(0));

    let (mut empty, _) = dashboard(0, true);
    assert_eq!(empty.table_state.selected(), None);
    empty.select_next(0);
    empty.select_previous(0);
    assert_eq!(empty.table_state.selected(), None);
}

#[test]
fn keys_move_the_selection_scroll_the_code_and_ask_to_leave() {
    let steps = steps();
    let (mut state, _) = dashboard(steps.len(), true);

    assert!(state.handle_key(KeyCode::Down, &steps, 5));
    assert_eq!(state.table_state.selected(), Some(1));
    assert!(state.handle_key(KeyCode::Up, &steps, 5));
    assert_eq!(state.table_state.selected(), Some(0));

    // the code scrolls down to its last line and no further, and up to its first
    for _ in 0..10 {
        assert!(state.handle_key(KeyCode::Char('s'), &steps, 5));
    }
    assert_eq!(state.code_scroll, 4);
    assert!(state.handle_key(KeyCode::PageUp, &steps, 5));
    assert_eq!(state.code_scroll, 3);
    assert!(state.handle_key(KeyCode::PageDown, &steps, 5));
    assert_eq!(state.code_scroll, 4);
    for _ in 0..10 {
        assert!(state.handle_key(KeyCode::Char('w'), &steps, 5));
    }
    assert_eq!(state.code_scroll, 0);

    // a key that means nothing changes nothing
    assert!(state.handle_key(KeyCode::Char('x'), &steps, 5));
    assert_eq!((state.table_state.selected(), state.code_scroll), (Some(0), 0));

    assert!(!state.handle_key(KeyCode::Char('q'), &steps, 5));
    assert!(!state.handle_key(KeyCode::Esc, &steps, 5));
}

#[test]
fn copying_puts_the_selected_step_on_the_clipboard_and_says_so() {
    let steps = steps();
    let (mut state, copies) = dashboard(steps.len(), true);
    state.handle_key(KeyCode::Down, &steps, 1);
    assert!(state.handle_key(KeyCode::Char('c'), &steps, 1));
    assert_eq!(*copies.borrow(), ["n = 3.0"]);
    assert_eq!(state.status_msg, "Copied to clipboard: n = 3.0");
}

#[test]
fn nothing_is_said_copied_when_the_clipboard_refuses_or_nothing_is_selected() {
    let steps = steps();
    let (mut refusing, tried) = dashboard(steps.len(), false);
    let before = refusing.status_msg.clone();
    refusing.handle_key(KeyCode::Char('c'), &steps, 1);
    assert_eq!(tried.borrow().len(), 1);
    assert_eq!(refusing.status_msg, before);

    let (mut empty, copies) = dashboard(0, true);
    empty.handle_key(KeyCode::Char('c'), &[], 1);
    assert!(copies.borrow().is_empty());
    // a selection past the steps there are
    let (mut stale, copies) = dashboard(steps.len(), true);
    stale.table_state.select(Some(9));
    stale.handle_key(KeyCode::Char('c'), &steps, 1);
    assert!(copies.borrow().is_empty());
}

#[test]
fn the_dashboard_shows_the_code_the_steps_and_the_selected_one() {
    let steps = steps();
    let (mut state, _) = dashboard(steps.len(), true);
    let code = ["m = 5 kg", "E = m * c^2", "n = 3"];
    let shown = drawn(&code, &steps, &mut state, 120);
    for expected in [
        "Physure TUI Dashboard v",
        "Physure Interactive Inspector",
        "PHS Script (Line 1)",
        "E = m * c^2",
        "Calculations & Steps",
        "Label",
        "Expression",
        "m * c^2",
        "Selected Step Details",
        "Target: E",
        "Keyboard Actions",
        "Nav: ▲/▼",
    ] {
        assert!(shown.contains(expected), "{expected:?} is not on the screen:\n{shown}");
    }

    state.handle_key(KeyCode::Char('s'), &steps, code.len());
    state.handle_key(KeyCode::Down, &steps, code.len());
    let scrolled = drawn(&code, &steps, &mut state, 120);
    assert!(scrolled.contains("PHS Script (Line 2)") && !scrolled.contains("m = 5 kg"), "{scrolled}");
    assert!(scrolled.contains("Target: n"), "{scrolled}");
}

#[test]
fn the_dashboard_of_a_script_without_steps_says_so() {
    let (mut state, _) = dashboard(0, true);
    let shown = drawn(&[], &[], &mut state, 100);
    assert!(shown.contains("No step selected"), "{shown}");
}

#[test]
fn a_long_value_is_cut_to_thirty_characters_wherever_the_cut_falls() {
    // the cut at the 27th byte is inside a two-byte character: it must be cut at characters, not at bytes
    let long = format!("{}{}", "x".repeat(26), "²".repeat(20));
    let short = "y".repeat(30);
    let steps = vec![
        step("long", "a", PhsValue::String(long)),
        step("short", "b", PhsValue::String(short.clone())),
    ];
    let (mut state, _) = dashboard(steps.len(), true);
    let shown = drawn(&["a"], &steps, &mut state, 220);
    let cut = format!("{}²...", "x".repeat(26));
    assert!(shown.contains(&cut), "{shown}");
    assert!(shown.contains(&short), "a value of thirty characters is shown whole:\n{shown}");
}

#[test]
fn the_loop_draws_acts_on_the_keys_and_stops_when_asked() {
    let steps = steps();
    let (mut state, copies) = dashboard(steps.len(), true);
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let released = KeyEvent::new_with_kind(KeyCode::Down, KeyModifiers::NONE, KeyEventKind::Release);
    let mut events = vec![
        None,                         // nothing came within the wait
        Some(Event::Resize(100, 30)), // not a key
        Some(press(KeyCode::Down)),
        Some(Event::Key(released)),
        Some(press(KeyCode::Char('c'))),
        Some(press(KeyCode::Char('q'))),
        Some(press(KeyCode::Down)), // never read: the loop has left
    ]
    .into_iter();

    run_dashboard(&mut terminal, &["code"], &steps, &mut state, || Ok(events.next().unwrap())).unwrap();

    assert_eq!(state.table_state.selected(), Some(1), "a key pressed acts once, not again when it is released");
    assert_eq!(*copies.borrow(), ["n = 3.0"]);
    assert_eq!(events.len(), 1);
    assert!(screen(&terminal).contains("Copied to clipboard: n = 3.0"));
}

#[test]
fn a_terminal_that_fails_ends_the_loop_with_the_error() {
    let (mut state, _) = dashboard(0, true);
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let failed = run_dashboard(&mut terminal, &[], &[], &mut state, || Err(io::Error::other("the terminal is gone")));
    assert_eq!(failed.unwrap_err().to_string(), "the terminal is gone");
}
