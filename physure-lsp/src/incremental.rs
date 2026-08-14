use physure_script::ast::Statement;
use physure_script::interpreter::PhsInterpreter;
use tower_lsp::lsp_types::Diagnostic;

/// Everything Track D persists for one open document across edits: the last successfully
/// parsed statement list (diffed against on the next change), the interpreter whose `env`
/// carries forward instead of being rebuilt from scratch, and each statement's last-known
/// diagnostic so an unchanged statement doesn't need to re-run to keep reporting it.
pub struct DocState {
    pub statements: Vec<Statement>,
    pub lines: Vec<usize>,
    pub interp: PhsInterpreter,
    pub diagnostics: Vec<Option<Diagnostic>>,
}

impl DocState {
    pub fn empty() -> Self {
        DocState {
            statements: Vec::new(),
            lines: Vec::new(),
            interp: PhsInterpreter::default(),
            diagnostics: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_doc_state_has_no_statements_or_diagnostics() {
        let state = DocState::empty();
        assert!(state.statements.is_empty());
        assert!(state.diagnostics.is_empty());
    }
}
