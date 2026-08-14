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

fn collect_declared(stmt: &crate::ast::Statement, declared: &mut HashSet<String>) {
    use crate::ast::Statement;
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
        _ => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugAction {
    Continue,
    StepInto,
    StepOver,
    StepOut,
    Pause,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AssignmentNode, FunctionDefNode, Statement};
    use crate::ast::Expr;

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
