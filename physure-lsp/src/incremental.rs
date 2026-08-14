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

use physure_script::ast::{Expr, ImportSpecifier};
use physure_script::debug::collect_declared;
use std::collections::HashSet;

/// What one top-level statement writes into `env` when it runs successfully, and what names
/// its evaluation reads from `env`. Purely a static AST walk -- no execution.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct StmtDeps {
    pub writes: Vec<String>,
    pub reads: HashSet<String>,
}

pub fn analyze(statements: &[Statement]) -> Vec<StmtDeps> {
    statements.iter().map(analyze_one).collect()
}

fn analyze_one(stmt: &Statement) -> StmtDeps {
    match stmt {
        Statement::Assignment(node) => {
            let mut reads = HashSet::new();
            collect_expr_reads(&node.value, &HashSet::new(), &mut reads);
            for d in &node.decorators {
                for arg in &d.args {
                    collect_expr_reads(arg, &HashSet::new(), &mut reads);
                }
            }
            StmtDeps { writes: vec![node.name.clone()], reads }
        }
        Statement::FunctionDef(node) => {
            let mut declared: HashSet<String> = node.params.iter().cloned().collect();
            for s in &node.body_stmts {
                collect_declared(s, &mut declared);
            }
            let mut reads = HashSet::new();
            // `check_ensures` (physure-script/src/interpreter.rs) binds a synthetic `result`
            // name into a clone of the local env before evaluating an `@ensures` condition;
            // treat it as local to every decorator on this function (not just `@ensures`) so
            // it never counts as an outer read -- `validate_decorators` already forbids a
            // param literally named `result`, so this can't hide a real dependency.
            let mut decorator_locals = declared.clone();
            decorator_locals.insert("result".to_string());
            for d in &node.decorators {
                for arg in &d.args {
                    collect_expr_reads(arg, &decorator_locals, &mut reads);
                }
            }
            for s in &node.body_stmts {
                collect_stmt_reads(s, &declared, &mut reads);
            }
            StmtDeps { writes: vec![node.name.clone()], reads }
        }
        Statement::Import(node) => {
            let writes = match &node.specifier {
                ImportSpecifier::Symbols(syms) => syms
                    .iter()
                    .map(|s| s.alias.clone().unwrap_or_else(|| s.name.clone()))
                    .collect(),
                // Wildcard/ModuleAlias: statically unknowable which names get bound -- under-
                // report rather than guess, same rule collect_declared already uses.
                ImportSpecifier::Wildcard | ImportSpecifier::ModuleAlias(_) => Vec::new(),
            };
            StmtDeps { writes, reads: HashSet::new() }
        }
        Statement::While { cond, body, .. } => {
            // §4.4: verified against the interpreter that a while body's writes always leak
            // to the top level, unconditionally -- no "did this name already exist" filter.
            let mut declared = HashSet::new();
            for s in body {
                collect_declared(s, &mut declared);
            }
            // Reads are NOT scoped by `declared` the way a function's params/locals are: a
            // while loop introduces no fresh child scope (§4.4 confirmed the body executes
            // directly against the shared top-level env, unlike a function call's separate
            // env clone), so even a name the body itself assigns must still count as reading
            // whatever came from before the loop -- most importantly `cond`'s own use of a
            // name the body reassigns (`i` in `while i < 5 { i = i + 1 }` reads the pre-loop
            // `i`; it is not something the body "locally" owns the way a param would be).
            let mut reads = HashSet::new();
            collect_expr_reads(cond, &HashSet::new(), &mut reads);
            for s in body {
                collect_stmt_reads(s, &HashSet::new(), &mut reads);
            }
            StmtDeps { writes: declared.into_iter().collect(), reads }
        }
        Statement::Expr(expr) | Statement::Return(expr) => {
            let mut reads = HashSet::new();
            collect_expr_reads(expr, &HashSet::new(), &mut reads);
            StmtDeps { writes: Vec::new(), reads }
        }
        Statement::GuardReturn { cond, value } => {
            let mut reads = HashSet::new();
            collect_expr_reads(cond, &HashSet::new(), &mut reads);
            collect_expr_reads(value, &HashSet::new(), &mut reads);
            StmtDeps { writes: Vec::new(), reads }
        }
        Statement::Export(_) => StmtDeps::default(),
    }
}

/// Reads inside a nested statement (a function body or while body), relative to the
/// enclosing statement's already-computed `locals` set. Deliberately does not add a nested
/// FunctionDef's own params as further-local: over-reporting a param name as a "read" is
/// harmless (nothing at the top level is ever named after a stranger's parameter, and if it
/// coincidentally is, the worst case is one extra safe re-run) -- ponytail: known ceiling,
/// ok to leave as-is; upgrade only if this over-reporting is ever observed to matter.
fn collect_stmt_reads(stmt: &Statement, locals: &HashSet<String>, out: &mut HashSet<String>) {
    match stmt {
        Statement::Assignment(node) => {
            collect_expr_reads(&node.value, locals, out);
            for d in &node.decorators {
                for arg in &d.args {
                    collect_expr_reads(arg, locals, out);
                }
            }
        }
        Statement::FunctionDef(node) => {
            // Same `result`-binding exemption as `analyze_one`'s FunctionDef arm -- see its
            // comment for why (`check_ensures` in interpreter.rs).
            let mut decorator_locals = locals.clone();
            decorator_locals.insert("result".to_string());
            for d in &node.decorators {
                for arg in &d.args {
                    collect_expr_reads(arg, &decorator_locals, out);
                }
            }
            for s in &node.body_stmts {
                collect_stmt_reads(s, locals, out);
            }
        }
        Statement::Import(_) => {}
        Statement::While { cond, body, .. } => {
            collect_expr_reads(cond, locals, out);
            for s in body {
                collect_stmt_reads(s, locals, out);
            }
        }
        Statement::Expr(expr) | Statement::Return(expr) => collect_expr_reads(expr, locals, out),
        Statement::GuardReturn { cond, value } => {
            collect_expr_reads(cond, locals, out);
            collect_expr_reads(value, locals, out);
        }
        Statement::Export(_) => {}
    }
}

fn collect_expr_reads(expr: &Expr, locals: &HashSet<String>, out: &mut HashSet<String>) {
    match expr {
        Expr::Quantity(_) => {}
        Expr::Identifier(name) => {
            if name.starts_with('`') || (name.contains('{') && name.contains('}')) {
                collect_template_reads(name, locals, out);
            } else if !locals.contains(name) {
                out.insert(name.clone());
            }
        }
        Expr::Str(text) => collect_template_reads(text, locals, out),
        Expr::BinaryOp { left, right, .. } => {
            collect_expr_reads(left, locals, out);
            collect_expr_reads(right, locals, out);
        }
        Expr::FunctionCall { name, args, kwargs } => {
            // `where`/`let` desugars to this internal 3-arg pseudo-call: args[0] is a bare
            // name local to args[2] only, evaluated against the *outer* scope for args[1]
            // (§4.3). Not a real callable -- "let" itself must not be treated as a read.
            if name == "let" && args.len() == 3 {
                if let Expr::Identifier(bound_name) = &args[0] {
                    collect_expr_reads(&args[1], locals, out);
                    let mut inner = locals.clone();
                    inner.insert(bound_name.clone());
                    collect_expr_reads(&args[2], &inner, out);
                    return;
                }
            }
            // §4.1: the callee's name is a bare String field, not a nested Identifier --
            // resolution goes through env.get(name) exactly like an Identifier read does.
            if !locals.contains(name) {
                out.insert(name.clone());
            }
            for arg in args {
                collect_expr_reads(arg, locals, out);
            }
            for (_, arg) in kwargs {
                collect_expr_reads(arg, locals, out);
            }
        }
        Expr::ForExpr { var, iterable, body } => {
            collect_expr_reads(iterable, locals, out);
            let mut inner = locals.clone();
            inner.insert(var.clone());
            collect_expr_reads(body, &inner, out);
        }
    }
}

/// §4.5: scans `text` for every `{...}` span the same way `interpolate` does at eval time,
/// parses each as PHS source, and folds in whatever it reads. A span that fails to parse
/// contributes nothing, matching `interpolate`'s own behavior of leaving it untouched.
fn collect_template_reads(text: &str, locals: &HashSet<String>, out: &mut HashSet<String>) {
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else { break };
        let expr_str = rest[..end].trim();
        rest = &rest[end + 1..];
        if let Ok(prog) = physure_script::parser::parse_phs(expr_str) {
            for stmt in &prog.statements {
                collect_stmt_reads(stmt, locals, out);
            }
        }
    }
}

/// Returns a clone of `stmt` with every FunctionDefNode/While `body_lines` field cleared, so
/// statement equality is insensitive to line-number shifts caused by edits elsewhere in the
/// document -- a byte-identical function or while statement shouldn't register as "changed"
/// just because something above it in the file moved it to a different line. `Statement`
/// itself carries no line info (that lives out-of-band in `Program.lines`/`DocState.lines`),
/// but `FunctionDefNode.body_lines` and `Statement::While`'s `body_lines` do, and recurse into
/// nested bodies too (a nested FunctionDef/While inside a function body has its own).
fn normalize_lines(stmt: &Statement) -> Statement {
    match stmt {
        Statement::FunctionDef(node) => {
            let mut node = node.clone();
            node.body_lines = Vec::new();
            node.body_stmts = node.body_stmts.iter().map(normalize_lines).collect();
            Statement::FunctionDef(node)
        }
        Statement::While { cond, body, .. } => Statement::While {
            cond: cond.clone(),
            body: body.iter().map(normalize_lines).collect(),
            body_lines: Vec::new(),
        },
        other => other.clone(),
    }
}

/// Longest common prefix length, then longest common suffix length (not overlapping the
/// prefix), by structural equality. Everything between the two matched regions -- on the old
/// side and the new side independently, since insertions/deletions change list length -- is
/// the changed span (spec §3 step 2). O(n), safe in the over-inclusive direction for any edit
/// this doesn't perfectly isolate (e.g. a whole statement moving from top to bottom).
fn diff_bounds(old: &[Statement], new: &[Statement]) -> (usize, usize) {
    let old: Vec<Statement> = old.iter().map(normalize_lines).collect();
    let new: Vec<Statement> = new.iter().map(normalize_lines).collect();
    let max_prefix = old.len().min(new.len());
    let mut prefix = 0;
    while prefix < max_prefix && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let max_suffix = old.len().min(new.len()) - prefix;
    let mut suffix = 0;
    while suffix < max_suffix && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix] {
        suffix += 1;
    }
    (prefix, suffix)
}

use std::collections::HashMap;

/// Result of diffing an old statement list against a new one: which new-list indices need
/// re-running, the common-prefix length (statements before it can never be dirty by
/// construction -- everything they read resolves within the unchanged prefix), and every
/// name touched by the changed span on either side (needed to invalidate stale `env` entries
/// before re-running -- see `apply_change`).
pub struct DirtyAnalysis {
    pub dirty: HashSet<usize>,
    pub prefix: usize,
    pub touched_names: HashSet<String>,
}

pub fn compute_dirty(old: &[Statement], new: &[Statement]) -> DirtyAnalysis {
    let (prefix, suffix) = diff_bounds(old, new);
    let old_mid = &old[prefix..old.len() - suffix];
    let new_mid_end = new.len() - suffix;
    let new_mid = &new[prefix..new_mid_end];

    // Union of both sides: a write that's purely deleted (old side only) still needs every
    // downstream reader re-resolved, which an index-only check over the new list would miss.
    let mut touched_names: HashSet<String> = HashSet::new();
    for stmt in old_mid.iter().chain(new_mid.iter()) {
        touched_names.extend(analyze_one(stmt).writes);
    }

    let deps = analyze(new);
    let mut dirty = HashSet::new();
    let mut last_writer: HashMap<String, usize> = HashMap::new();

    for (i, d) in deps.iter().enumerate() {
        // Statements before `prefix` are unchanged content whose reads resolve entirely
        // within the equally-unchanged prefix -- never dirty, regardless of a same-named
        // write appearing later in the changed span.
        if i >= prefix {
            let in_changed_span = i < new_mid_end;
            let touches = d.reads.iter().any(|n| touched_names.contains(n));
            let depends_on_dirty = d
                .reads
                .iter()
                .any(|n| last_writer.get(n).map_or(false, |w| dirty.contains(w)));
            if in_changed_span || touches || depends_on_dirty {
                dirty.insert(i);
            }
        }
        for name in &d.writes {
            last_writer.insert(name.clone(), i);
        }
    }

    DirtyAnalysis { dirty, prefix, touched_names }
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

    use physure_script::parser::parse_phs;

    fn stmts(src: &str) -> Vec<Statement> {
        parse_phs(src).unwrap().statements
    }

    #[test]
    fn assignment_writes_its_name_and_reads_its_expression() {
        let s = stmts("y = x + 1");
        let deps = analyze_one(&s[0]);
        assert_eq!(deps.writes, vec!["y".to_string()]);
        assert!(deps.reads.contains("x"));
    }

    #[test]
    fn function_def_reads_a_global_used_only_in_its_body_and_writes_its_own_name() {
        // The body's `g` must show up as a read of the *FunctionDef* statement -- nothing
        // else in the script mentions `g` at the top level. Confirms the "expression tree
        // includes nested body statements" reading of the spec (§4.1).
        let s = stmts("fn compute(m) = m * g");
        let deps = analyze_one(&s[0]);
        assert_eq!(deps.writes, vec!["compute".to_string()]);
        assert!(deps.reads.contains("g"));
        assert!(!deps.reads.contains("m"), "param must not read as a global");
    }

    #[test]
    fn function_call_name_counts_as_a_read_not_just_its_args() {
        // Expr::FunctionCall.name is a bare String, not an Expr::Identifier -- a walker that
        // only visits Identifier nodes would miss this (§4.1).
        let s = stmts("result = compute(2.0)");
        let deps = analyze_one(&s[0]);
        assert!(deps.reads.contains("compute"));
    }

    #[test]
    fn while_writes_every_body_assigned_name_unconditionally() {
        // Verified against the real interpreter (spec §4.4) that while-body writes always
        // leak, regardless of whether the name existed before the loop -- no filtering here.
        let s = stmts("while i < 5 { i = i + 1\nbrand_new = 99 }");
        let deps = analyze_one(&s[0]);
        let mut writes = deps.writes.clone();
        writes.sort();
        assert_eq!(writes, vec!["brand_new".to_string(), "i".to_string()]);
        assert!(deps.reads.contains("i"), "cond and body must read the pre-loop i");
    }

    #[test]
    fn where_bound_name_does_not_read_as_an_outer_dependency() {
        // `expr where a = value` desugars to let(a, value, expr) -- `a` is local to the
        // `let`'s body argument only (§4.3).
        let s = stmts("y = a * 2 where a = 3 m");
        let deps = analyze_one(&s[0]);
        assert!(!deps.reads.contains("a"), "where-bound name must not read as a global");
    }

    #[test]
    fn where_value_expr_still_reads_normally() {
        let s = stmts("y = a * 2 where a = base_value");
        let deps = analyze_one(&s[0]);
        assert!(deps.reads.contains("base_value"));
    }

    #[test]
    fn template_string_interpolation_reads_the_interpolated_name() {
        // interpolate() parses {expr} as real PHS source and evaluates it against env --
        // skipping this would under-count reads for any statement using string
        // interpolation (§4.5).
        let s = stmts(r#"msg = "v is {v * 2}""#);
        let deps = analyze_one(&s[0]);
        assert!(deps.reads.contains("v"));
    }

    #[test]
    fn ensures_result_binding_does_not_read_as_an_outer_dependency() {
        let s = stmts("@ensures(result > 0.0, \"must be positive\")\nfn small(m) = m");
        let deps = analyze_one(&s[0]);
        assert!(!deps.reads.contains("result"), "result is bound locally by @ensures, not an outer read");
    }

    #[test]
    fn import_symbols_write_their_aliased_or_own_names() {
        let s = stmts("use solve as slv, deriv from calc");
        let deps = analyze_one(&s[0]);
        let mut writes = deps.writes.clone();
        writes.sort();
        assert_eq!(writes, vec!["deriv".to_string(), "slv".to_string()]);
    }

    #[test]
    fn diff_bounds_finds_a_single_inserted_statement() {
        let old = stmts("a = 1\nb = 2\nc = 3");
        let new = stmts("a = 1\nx = 9\nb = 2\nc = 3");
        assert_eq!(diff_bounds(&old, &new), (1, 2));
    }

    #[test]
    fn diff_bounds_finds_a_single_edited_statement() {
        let old = stmts("a = 1\nb = 2\nc = 3");
        let new = stmts("a = 1\nb = 5\nc = 3");
        assert_eq!(diff_bounds(&old, &new), (1, 1));
    }

    #[test]
    fn diff_bounds_of_identical_lists_covers_everything() {
        let old = stmts("a = 1\nb = 2");
        let new = stmts("a = 1\nb = 2");
        assert_eq!(diff_bounds(&old, &new), (2, 0));
    }

    #[test]
    fn diff_bounds_handles_empty_old_list() {
        let old: Vec<Statement> = Vec::new();
        let new = stmts("a = 1\nb = 2");
        assert_eq!(diff_bounds(&old, &new), (0, 0));
    }

    #[test]
    fn diff_bounds_ignores_line_number_shifts_in_function_bodies() {
        let old = stmts("fn f(x) = x + 1\nresult = f(2)");
        let new = stmts("extra = 0\nfn f(x) = x + 1\nresult = f(2)");
        // The function and the call site are byte-identical content, just shifted down by
        // one line by the inserted statement above them -- diff_bounds must still recognize
        // them as an unchanged suffix instead of treating the line-number shift as a change.
        assert_eq!(diff_bounds(&old, &new), (0, 2));
    }

    #[test]
    fn editing_an_unread_statement_dirties_only_itself() {
        // Roadmap-mandated execution-count test: a statement whose result nothing downstream
        // reads, edited, re-executes alone.
        let old = stmts("a = 1\nb = 2\nc = 3");
        let new = stmts("a = 1\nb = 99\nc = 3");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([1]));
    }

    #[test]
    fn editing_the_first_of_two_writes_reruns_only_correctly_scoped_dependents() {
        // Roadmap-mandated rebinding-correctness test: x written twice, y and z read
        // in between/after. Editing the *first* x must not touch the second write (x = 2,
        // a fresh write reading nothing) but must touch both the direct and transitive
        // readers of the first write.
        let old = stmts("x = 1\ny = x\nx = 2\nz = y");
        let new = stmts("x = 5\ny = x\nx = 2\nz = y");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([0, 1, 3]));
    }

    #[test]
    fn editing_a_global_only_a_called_functions_body_reads_propagates_to_the_call_site() {
        // §4.1: g never appears in the call site's own text -- only inside compute's body.
        let old = stmts("g = 9.8\nfn compute(m) = m * g\nresult = compute(2.0)");
        let new = stmts("g = 10.0\nfn compute(m) = m * g\nresult = compute(2.0)");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([0, 1, 2]));
    }

    #[test]
    fn unrelated_statements_stay_clean() {
        let old = stmts("a = 1\nb = 2\nc = 3");
        let new = stmts("a = 1\nb = 2\nc = 3");
        let result = compute_dirty(&old, &new);
        assert!(result.dirty.is_empty());
    }

    #[test]
    fn a_deleted_write_dirties_its_former_readers() {
        // touched_names must come from *both* sides of the changed span: the write to x
        // disappears entirely (statement removed), so whatever used to read it needs
        // re-resolving even though nothing at its new position writes x anymore.
        let old = stmts("x = 1\ny = x");
        let new = stmts("y = x");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([0]));
    }

    #[test]
    fn statement_after_a_while_loop_reruns_when_the_loops_body_write_changes() {
        // §4.4: c reads i, which the while loop (re)assigns -- edit the loop body, c must
        // re-run too, regardless of whether i existed before the loop.
        let old = stmts("i = 0\nwhile i < 5 { i = i + 1 }\nc = i");
        let new = stmts("i = 0\nwhile i < 5 { i = i + 2 }\nc = i");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([1, 2]));
    }

    #[test]
    fn editing_a_variable_used_only_in_a_template_string_dirties_that_statement() {
        // §4.5
        let old = stmts("v = 1\nmsg = \"v is {v * 2}\"");
        let new = stmts("v = 2\nmsg = \"v is {v * 2}\"");
        let result = compute_dirty(&old, &new);
        assert_eq!(result.dirty, HashSet::from([0, 1]));
    }
}
