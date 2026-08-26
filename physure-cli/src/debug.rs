//! `phs debug <script.phs> [--break-fn name] [--break N[:cond]]`
//!
//! A stdin-driven debugger REPL, following the same shape as `main.rs`'s existing `run_repl`
//! (plain `read_line` loop, no new CLI dependency) and `export.rs`'s subcommand-module
//! convention (`run_debug(args)`, dispatched from `main.rs` via `if args[1] == "debug"`).

use std::io::{self, Write};
use std::process;
use std::sync::Arc;

use physure_script::debug::{Breakpoint, DebugAction, DebugContext, DebugHook};
use physure_script::inspect::{inspect, ScopeKind};
use physure_script::{parse_phs, PhsInterpreter};

use crate::rich::RichRenderer;

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
}

/// Parses one `--break` flag value: `"42"` -> a line breakpoint, `"42:v > 100 m/s"` -> a
/// conditional one (the condition text is parsed into a real `Expr` later, once a script is
/// loaded and `crate::parser::parse_phs` is available -- this function only splits the flag
/// text, so it's testable without a script or an interpreter in hand).
pub fn parse_break_flag(value: &str) -> Option<BreakpointSpec> {
    if let Some((line_str, cond)) = value.split_once(':') {
        line_str.trim().parse::<usize>().ok()
            .filter(|&l| l > 0)
            .map(|l| BreakpointSpec::Conditional(l, cond.trim().to_string()))
    } else {
        value.trim().parse::<usize>().ok().filter(|&l| l > 0).map(BreakpointSpec::Line)
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

    #[test]
    fn rejects_a_zero_line_breakpoint() {
        // Line 0 is never a real source line (parser.rs's line_col() is 1-based), and it's the
        // sentinel synthesized/composed functions get for "unknown line" (see
        // physure-script/src/interpreter.rs's function-composition sites) -- accepting it as a
        // literal breakpoint would spuriously pause on every call to such a function.
        assert_eq!(parse_break_flag("0"), None);
    }

    #[test]
    fn rejects_a_zero_line_conditional_breakpoint() {
        assert_eq!(parse_break_flag("0:x > 1"), None);
    }
}

struct CliDebugHook {
    registry: physure_core::UnitRegistry,
}

impl DebugHook for CliDebugHook {
    fn on_statement(&self, ctx: &DebugContext) -> DebugAction {
        let frame_desc = ctx
            .call_stack
            .last()
            .map(|f| format!(", in {}() called from line {}", f.fn_name, f.call_site_line))
            .unwrap_or_default();
        println!("Paused at line {}{}", ctx.line, frame_desc);

        loop {
            print!("> ");
            if io::stdout().flush().is_err() {
                return DebugAction::Continue;
            }
            let mut line = String::new();
            if io::stdin().read_line(&mut line).is_err() || line.is_empty() {
                return DebugAction::Continue;
            }
            match parse_command(&line) {
                DebuggerCommand::Continue => return DebugAction::Continue,
                DebuggerCommand::Step => return DebugAction::StepInto,
                DebuggerCommand::Next => return DebugAction::StepOver,
                DebuggerCommand::Finish => return DebugAction::StepOut,
                DebuggerCommand::Locals => {
                    let Some(frame) = ctx.call_stack.last() else {
                        println!("(no locals at global scope)");
                        continue;
                    };
                    for name in &frame.declared {
                        if let Some(val) = ctx.env.get(name) {
                            RichRenderer::render_variable_card(name, val, None);
                        }
                    }
                }
                DebuggerCommand::Globals => {
                    let local_names: std::collections::HashSet<&str> = ctx
                        .call_stack
                        .last()
                        .map(|f| f.declared.iter().map(String::as_str).collect())
                        .unwrap_or_default();
                    for (name, val) in ctx.env {
                        if !local_names.contains(name.as_str()) {
                            RichRenderer::render_variable_card(name, val, None);
                        }
                    }
                }
                DebuggerCommand::Backtrace => {
                    for (depth, frame) in ctx.call_stack.iter().rev().enumerate() {
                        println!("  #{depth} {} (called from line {})", frame.fn_name, frame.call_site_line);
                    }
                }
                DebuggerCommand::Print(expr_src) => {
                    match parse_phs(&expr_src) {
                        Ok(prog) if !prog.statements.is_empty() => {
                            let interp = PhsInterpreter::default();
                            let physure_script::Statement::Expr(e) = &prog.statements[0] else {
                                println!("error: not an expression");
                                continue;
                            };
                            match interp.eval_expr(e, ctx.env) {
                                Ok(v) => println!("{v}"),
                                Err(err) => println!("error: {err}"),
                            }
                        }
                        _ => println!("error: could not parse '{expr_src}'"),
                    }
                }
                DebuggerCommand::Inspect(name) => {
                    let Some(val) = ctx.env.get(&name) else {
                        println!("error: no variable named '{name}'");
                        continue;
                    };
                    let scope = ctx
                        .call_stack
                        .last()
                        .filter(|f| f.declared.contains(&name))
                        .map(|f| ScopeKind::Local { owner_fn: f.fn_name.clone(), frame_depth: ctx.call_stack.len() })
                        .unwrap_or(ScopeKind::Global);
                    let insp = inspect(&name, val, scope, &self.registry);
                    println!("{name}");
                    println!("  kind        : {:?}", insp.kind);
                    println!("  scope       : {:?}", insp.scope);
                    println!("  measure     : {:?}", insp.measure);
                    println!("  unit        : {:?}", insp.unit_display);
                    println!("  prefix      : {:?}", insp.prefix);
                    println!("  dimension   : {:?}", insp.dimension);
                    println!("  uncertainty : {:?}", insp.uncertainty);
                }
                DebuggerCommand::Unknown(cmd) => {
                    println!("unknown command '{cmd}' -- try print/inspect/locals/globals/backtrace/step/next/finish/continue");
                }
            }
        }
    }
}

pub fn run_debug(args: &[String]) {
    let script_path = match args.get(2) {
        Some(p) if !p.starts_with('-') => p.clone(),
        _ => {
            eprintln!("Usage: phs debug <script.phs> [--break-fn name] [--break N[:cond]]");
            process::exit(1);
        }
    };
    let code = match std::fs::read_to_string(&script_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not read '{}': {}", script_path, e);
            process::exit(1);
        }
    };
    let program = match parse_phs(&code) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: parse failed: {}", e);
            process::exit(1);
        }
    };

    let (registry, _) = physure_core::units::conf::build_registry_from_conf();
    let mut interp = PhsInterpreter::with_debug_hook(
        Arc::new(physure_script::resolver::FsModuleResolver::default()),
        Arc::new(CliDebugHook { registry }),
    );

    let mut i = 2;
    while i < args.len() {
        if args[i] == "--break-fn" {
            if let Some(name) = args.get(i + 1) {
                interp.add_breakpoint(Breakpoint::FunctionEntry(name.clone()));
            }
            i += 2;
        } else if args[i] == "--break" {
            let raw = args.get(i + 1);
            if let Some(spec) = raw.and_then(|v| parse_break_flag(v)) {
                match spec {
                    BreakpointSpec::Line(l) => interp.add_breakpoint(Breakpoint::Line(l)),
                    BreakpointSpec::Conditional(l, cond_src) => {
                        match parse_phs(&cond_src) {
                            Ok(prog) if !prog.statements.is_empty() => {
                                if let physure_script::Statement::Expr(e) = prog.statements.into_iter().next().unwrap() {
                                    interp.add_breakpoint(Breakpoint::Conditional(l, e));
                                }
                            }
                            _ => eprintln!("warning: could not parse breakpoint condition '{cond_src}'"),
                        }
                    }
                }
            } else {
                eprintln!(
                    "warning: could not parse breakpoint '{}' (line numbers must be 1 or greater)",
                    raw.map(String::as_str).unwrap_or("")
                );
            }
            i += 2;
        } else {
            i += 1;
        }
    }

    match interp.run_statements_with_lines(&program) {
        Ok(_) => println!("Program finished."),
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}
