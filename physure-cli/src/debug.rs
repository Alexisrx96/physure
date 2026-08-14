//! `phs debug <script.phs> [--break-fn name] [--break N[:cond]]`
//!
//! A stdin-driven debugger REPL, following the same shape as `main.rs`'s existing `run_repl`
//! (plain `read_line` loop, no new CLI dependency) and `export.rs`'s subcommand-module
//! convention (`run_debug(args)`, dispatched from `main.rs` via `if args[1] == "debug"`).

#[derive(Debug, Clone, PartialEq)]
pub enum DebuggerCommand {
    Print(String),
    Inspect(String),
    Locals,
    Globals,
    Backtrace,
    Continue,
    Step,
    Next,
    Finish,
    Unknown(String),
}

pub fn parse_command(line: &str) -> DebuggerCommand {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("print ") {
        return DebuggerCommand::Print(rest.trim().to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("inspect ") {
        return DebuggerCommand::Inspect(rest.trim().to_string());
    }
    match trimmed {
        "locals" => DebuggerCommand::Locals,
        "globals" => DebuggerCommand::Globals,
        "backtrace" => DebuggerCommand::Backtrace,
        "continue" | "c" => DebuggerCommand::Continue,
        "step" | "s" => DebuggerCommand::Step,
        "next" | "n" => DebuggerCommand::Next,
        "finish" => DebuggerCommand::Finish,
        other => DebuggerCommand::Unknown(other.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BreakpointSpec {
    Line(usize),
    Conditional(usize, String),
    FunctionEntry(String),
}

/// Parses one `--break` flag value: `"42"` -> a line breakpoint, `"42:v > 100 m/s"` -> a
/// conditional one (the condition text is parsed into a real `Expr` later, once a script is
/// loaded and `crate::parser::parse_phs` is available -- this function only splits the flag
/// text, so it's testable without a script or an interpreter in hand).
pub fn parse_break_flag(value: &str) -> Option<BreakpointSpec> {
    if let Some((line_str, cond)) = value.split_once(':') {
        line_str.trim().parse::<usize>().ok().map(|l| BreakpointSpec::Conditional(l, cond.trim().to_string()))
    } else {
        value.trim().parse::<usize>().ok().map(BreakpointSpec::Line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_print_and_inspect_with_their_argument() {
        assert_eq!(parse_command("print v"), DebuggerCommand::Print("v".to_string()));
        assert_eq!(parse_command("inspect G"), DebuggerCommand::Inspect("G".to_string()));
    }

    #[test]
    fn parses_bare_commands_and_short_aliases() {
        assert_eq!(parse_command("locals"), DebuggerCommand::Locals);
        assert_eq!(parse_command("backtrace"), DebuggerCommand::Backtrace);
        assert_eq!(parse_command("c"), DebuggerCommand::Continue);
        assert_eq!(parse_command("n"), DebuggerCommand::Next);
    }

    #[test]
    fn parses_a_plain_line_breakpoint() {
        assert_eq!(parse_break_flag("42"), Some(BreakpointSpec::Line(42)));
    }

    #[test]
    fn parses_a_conditional_breakpoint() {
        assert_eq!(
            parse_break_flag("42:v > 100 m/s"),
            Some(BreakpointSpec::Conditional(42, "v > 100 m/s".to_string()))
        );
    }
}
