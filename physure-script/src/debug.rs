use std::collections::HashSet;
use crate::value::PhsValue;
use std::collections::HashMap;

/// Implemented by a debugger front end (the CLI in Track C, a DAP adapter later) to observe
/// and control interpreter execution. `None` on `PhsInterpreter` costs nothing — every call
/// site checks `is_none()` before doing any work to build a `DebugContext`.
pub trait DebugHook: Send + Sync {
    fn on_statement(&self, ctx: &DebugContext) -> DebugAction;
}

pub struct DebugContext<'a> {
    pub line: usize,
    pub call_stack: &'a [StackFrame],
    pub env: &'a HashMap<String, PhsValue>,
}

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub fn_name: String,
    pub call_site_line: usize,
    /// Params plus every name first assigned in this function's `body_stmts` — computed once,
    /// statically, from the `FunctionDefNode` when the frame is pushed. Lets the debugger
    /// distinguish "local to this call" from "visible because `call_function_node` clones the
    /// whole caller-side env" without re-walking values at every pause.
    pub declared: HashSet<String>,
}

impl StackFrame {
    pub fn new(func: &crate::ast::FunctionDefNode, call_site_line: usize) -> Self {
        let mut declared: HashSet<String> = func.params.iter().cloned().collect();
        for stmt in &func.body_stmts {
            collect_declared(stmt, &mut declared);
        }
        StackFrame { fn_name: func.name.clone(), call_site_line, declared }
    }
}

pub fn collect_declared(stmt: &crate::ast::Statement, declared: &mut HashSet<String>) {
    use crate::ast::{ImportSpecifier, Statement};
    match stmt {
        Statement::Assignment(node) => {
            declared.insert(node.name.clone());
        }
        Statement::FunctionDef(node) => {
            declared.insert(node.name.clone());
        }
        Statement::While { body, .. } => {
            for s in body {
                collect_declared(s, declared);
            }
        }
        // Mirrors the name-binding logic in `interpreter::resolve_use`: `Symbols` binds each
        // symbol under its alias if given, otherwise its own name -- statically determinable
        // from the AST alone, same as `resolve_use` does at runtime. `Wildcard` (`use * from
        // ...`) binds every export of the resolved module/domain, which is NOT knowable without
        // actually resolving it -- `declared` deliberately under-reports for a wildcard import
        // rather than guessing; a debugger consulting it should treat "not in `declared`" as
        // "unknown", not "definitely a global", for a function with a wildcard import in its
        // body. `ModuleAlias` binds nothing today because `resolve_use` always errors on it
        // ("Module aliases not yet supported by interpreter"), so there is nothing to add.
        Statement::Import(node) => {
            if let ImportSpecifier::Symbols(syms) = &node.specifier {
                for sym in syms {
                    declared.insert(sym.alias.clone().unwrap_or_else(|| sym.name.clone()));
                }
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugAction {
    Continue,
    StepInto,
    StepOver,
    StepOut,
    /// Currently unreachable from any shipped `DebugHook` implementation (no CLI command maps
    /// to it) -- mapped to the same "stop at the next checkpoint" behavior as `StepInto` in
    /// `interpreter.rs`'s `debug_checkpoint` (both become `StepMode::Into`). Reserved for a
    /// future async/DAP-style pause request that can interrupt execution from outside the
    /// current call, which is not something the synchronous CLI hook needs.
    Pause,
}

#[derive(Debug, Clone)]
pub enum Breakpoint {
    Line(usize),
    Conditional(usize, crate::ast::Expr),
    /// Matches by comparing the checkpoint's innermost `call_stack` frame's `fn_name` against
    /// this name -- which means it fires on *every* statement executed inside that function's
    /// frame, not only the first one. There is no separate "just entered" bit tracked anywhere,
    /// so a multi-statement function pauses once per statement per call, not once per call.
    /// Callers that want a true once-per-call pause need to track call-count themselves (e.g.
    /// by comparing `call_stack` depth/identity across hook invocations).
    FunctionEntry(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AssignmentNode, FunctionDefNode, Statement};
    use crate::ast::Expr;

    #[test]
    fn declared_includes_names_bound_by_an_in_body_explicit_import() {
        use crate::ast::{ImportNode, ImportSpecifier, ImportSymbol};

        let func = FunctionDefNode {
            name: "f".to_string(),
            params: vec![],
            param_units: vec![],
            body_stmts: vec![
                Statement::Import(ImportNode {
                    path: "calc".to_string(),
                    specifier: ImportSpecifier::Symbols(vec![
                        ImportSymbol { name: "solve".to_string(), alias: None },
                        ImportSymbol { name: "deriv".to_string(), alias: Some("d".to_string()) },
                    ]),
                }),
                Statement::Return(Expr::Identifier("solve".to_string())),
            ],
            body_lines: vec![1, 2],
            decorators: Vec::new(),
            doc: None,
        };
        let frame = StackFrame::new(&func, 1);
        assert!(frame.declared.contains("solve"), "unaliased import name missing: {:?}", frame.declared);
        assert!(frame.declared.contains("d"), "aliased import should be declared under its alias: {:?}", frame.declared);
        assert!(!frame.declared.contains("deriv"), "aliased import should not also be declared under its original name: {:?}", frame.declared);
    }

    #[test]
    fn declared_includes_params_and_body_assignments_not_globals() {
        let func = FunctionDefNode {
            name: "f".to_string(),
            params: vec!["a".to_string()],
            param_units: vec![None],
            body_stmts: vec![
                Statement::Assignment(AssignmentNode {
                    name: "b".to_string(),
                    value: Expr::Identifier("a".to_string()),
                    decorators: Vec::new(),
                }),
                Statement::Return(Expr::Identifier("b".to_string())),
            ],
            body_lines: vec![1, 2],
            decorators: Vec::new(),
            doc: None,
        };
        let frame = StackFrame::new(&func, 10);
        assert!(frame.declared.contains("a"));
        assert!(frame.declared.contains("b"));
        assert!(!frame.declared.contains("some_global"));
        assert_eq!(frame.fn_name, "f");
        assert_eq!(frame.call_site_line, 10);
    }
}
