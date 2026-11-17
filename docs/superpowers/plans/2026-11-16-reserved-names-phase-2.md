# Reserved names, phase 2: the checker, the symbolic parser and the language server — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `phs check` reports a reserved-name binding before the program runs, a symbolic text refuses the words PHS reads as something other than a name, and the language server forgets what a deleted `use` unlocked.

**Architecture:** The decision about what an import unlocks, or why it is refused, moves from `PhsInterpreter::unlock_domain` into one pure function of `reserved.rs` (`plan_import`), so the interpreter and the checker give the same answer. The checker's pre-pass walks the top-level statements in order, keeping what has been unlocked and what is held. The symbolic parser asks `reserved::static_kind` for a bare name. The language server clears and replays the imports of the new text when an import is added, changed or removed.

**Tech Stack:** Rust (`physure-script`, `physure-lsp`).

**Spec:** `docs/superpowers/specs/2026-11-16-reserved-names-design.md` (§5.3 checker, §5.4 symbolic parser, §11 the language server bullet, §9 phase 2). Phase 1 is plan `2026-11-16-reserved-names-phase-1.md` and is done.

**Working notes (read once):**
- Work on the branch `docs/post-release-0.4.0`, in the worktree `.claude/worktrees/post-release-040`. Do not touch `main`. Do not push.
- Prefix every cargo command with `CARGO_TARGET_DIR=D:/Projects/physure/target` (shared target, never a second `target/`).
- Rust, `CHANGELOG.md` and `docs/*.md` files are CRLF in the working tree: edit them with the editor tool, or with Python in default text mode (`open(p, encoding='utf-8')`, never `newline=''`) chained with `&&` before `git commit`.
- One deviation from spec §5.4, decided while planning: the symbolic parser refuses as a bare symbol the words PHS reads as something other than a name (the keywords, the literals, and the functions the symbolic parser itself knows: `sin`, `cos`, ..., `deriv`, `integral`). It does NOT refuse the other core-function names (`sigma`, `max`, `min`, `dof`, `round`...) or the statement words: they are plausible physical symbols, and the variables of an equation call with those names are already allowed (phase 1). Task 5 writes this into the spec.

---

### Task 1: One function decides what an import unlocks

**Files:**
- Modify: `physure-script/src/reserved.rs` (new items before `#[cfg(test)] mod tests`, and new tests inside it)
- Modify: `physure-script/src/interpreter/statements.rs` (`unlock_domain`)

This is a refactor: behaviour must not change. The 16 tests of `tests/reserved_names.rs` and the module tests are the guard.

- [ ] **Step 1: Write the tests for the new function**

Inside `mod tests` of `physure-script/src/reserved.rs` add:

```rust
    use crate::ast::{ImportNode, ImportSpecifier, ImportSymbol};

    fn import(path: &str, specifier: ImportSpecifier) -> ImportNode {
        ImportNode { path: path.to_string(), specifier }
    }

    fn symbol(name: &str, alias: Option<&str>) -> ImportSymbol {
        ImportSymbol { name: name.to_string(), alias: alias.map(str::to_string) }
    }

    const MEMBERS: [&str; 3] = ["limit", "solve", "deriv"];

    #[test]
    fn a_path_names_a_module_exactly_when_the_builtins_do() {
        for path in ["calc", "plot", "array", "matrix", "core", "nope", ""] {
            assert_eq!(domain_of(path).is_some(), crate::builtins::domain_members(path).is_some(), "{path}");
        }
        assert_eq!(domain_of("matrix"), Some("array"));
    }

    #[test]
    fn a_wildcard_unlocks_every_member_and_a_named_import_only_what_it_names() {
        let none = |_: &str| false;
        let unlocked = Unlocked::new();
        let all = plan_import(&import("calc", ImportSpecifier::Wildcard), "calc", &MEMBERS, &none, &unlocked).unwrap();
        assert_eq!(all.len(), 3);
        let named = import("calc", ImportSpecifier::Symbols(vec![symbol("solve", Some("sv"))]));
        let one = plan_import(&named, "calc", &MEMBERS, &none, &unlocked).unwrap();
        assert_eq!(one, vec![("sv".to_string(), ("calc", "solve".to_string()))]);
    }

    #[test]
    fn an_import_over_a_held_name_or_a_remap_is_refused_and_a_repeat_is_not() {
        let held = |name: &str| name == "limit";
        let mut unlocked = Unlocked::new();
        let wildcard = import("calc", ImportSpecifier::Wildcard);
        let error = plan_import(&wildcard, "calc", &MEMBERS, &held, &unlocked).unwrap_err();
        assert!(matches!(&error, ImportError::Reserved(m) if m.contains("already bound in the program")), "{error:?}");

        unlocked.insert("limit".to_string(), ("calc", "solve".to_string()));
        let none = |_: &str| false;
        let error = plan_import(&wildcard, "calc", &MEMBERS, &none, &unlocked).unwrap_err();
        assert!(matches!(&error, ImportError::Reserved(m) if m.contains("already a function of `calc`")), "{error:?}");
        let remap = import("calc", ImportSpecifier::Symbols(vec![symbol("deriv", Some("limit"))]));
        assert!(plan_import(&remap, "calc", &MEMBERS, &none, &unlocked).is_err());
        let repeat = import("calc", ImportSpecifier::Symbols(vec![symbol("solve", Some("limit"))]));
        assert!(plan_import(&repeat, "calc", &MEMBERS, &none, &unlocked).is_ok());

        let missing = import("calc", ImportSpecifier::Symbols(vec![symbol("nope", None)]));
        let error = plan_import(&missing, "calc", &MEMBERS, &none, &Unlocked::new()).unwrap_err();
        assert_eq!(error, ImportError::NoSuchFunction("no such function 'nope' in domain 'calc'".to_string()));
    }
```

- [ ] **Step 2: Run them to see them fail to compile**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --lib reserved::`
Expected: compile errors (`domain_of`, `plan_import`, `Unlocked`, `ImportError` not found).

- [ ] **Step 3: Write the function**

In `physure-script/src/reserved.rs`, add at the top of the file (after the module doc, before `pub enum Reserved`):

```rust
use std::collections::HashMap;

use crate::ast::{ImportNode, ImportSpecifier};
```

and, before `#[cfg(test)] mod tests`, add:

```rust
/// What a program has unlocked from the modules: the name it calls a function by, and the module and the
/// function's own name behind it.
pub type Unlocked = HashMap<String, (&'static str, String)>;

/// The module a path of `use` names (`matrix` is another name of `array`), `None` for anything else.
pub fn domain_of(path: &str) -> Option<&'static str> {
    match path {
        "calc" => Some("calc"),
        "plot" => Some("plot"),
        "array" | "matrix" => Some("array"),
        _ => None,
    }
}

/// Why an import of a module is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// The module has no such function.
    NoSuchFunction(String),
    /// A name it would unlock is already held by the program, or already unlocked as another function.
    Reserved(String),
}

impl ImportError {
    /// The text of the error.
    pub fn message(&self) -> &str {
        match self {
            ImportError::NoSuchFunction(message) | ImportError::Reserved(message) => message,
        }
    }
}

/// What `use` of the module `domain` (whose functions are `members`) unlocks: the name a function is called by and
/// the function behind it, for each name of the import; or why it is refused. `held` says whether the program
/// already holds a variable or a function of a name, `unlocked` is what it has imported so far. Nothing is unlocked
/// unless every name of the import is accepted. A module alias (`use calc as c`) unlocks nothing here: the caller
/// refuses it.
pub fn plan_import(
    node: &ImportNode,
    domain: &'static str,
    members: &[&str],
    held: &dyn Fn(&str) -> bool,
    unlocked: &Unlocked,
) -> Result<Vec<(String, (&'static str, String))>, ImportError> {
    let mut imports = Vec::new();
    match &node.specifier {
        ImportSpecifier::Wildcard => {
            for &member in members {
                if held(member) {
                    return Err(ImportError::Reserved(module_import_error(member, domain)));
                }
                // an alias of this module on the same name is a remap; another module's function of that name
                // is replaced by the later import, as it always was
                if let Some((other, canonical)) = unlocked.get(member) {
                    if *other == domain && canonical != member {
                        return Err(ImportError::Reserved(module_remap_error(member, domain)));
                    }
                }
                imports.push((member.to_string(), (domain, member.to_string())));
            }
        }
        ImportSpecifier::Symbols(syms) => {
            for sym in syms {
                if !members.contains(&sym.name.as_str()) {
                    return Err(ImportError::NoSuchFunction(format!("no such function '{}' in domain '{}'", sym.name, node.path)));
                }
                let call_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                if held(&call_name) {
                    return Err(ImportError::Reserved(module_import_error(&call_name, domain)));
                }
                let wanted = (domain, sym.name.clone());
                if unlocked.get(&call_name).is_some_and(|already| *already != wanted) {
                    return Err(ImportError::Reserved(module_remap_error(&call_name, domain)));
                }
                imports.push((call_name, wanted));
            }
        }
        ImportSpecifier::ModuleAlias(_) => {}
    }
    Ok(imports)
}
```

- [ ] **Step 4: Make `unlock_domain` call it**

In `physure-script/src/interpreter/statements.rs`, replace the whole of `unlock_domain` (from its doc comment, which starts "`use calc`, `use plot` or `use array`: makes the named members", to its closing brace) with:

```rust
    /// `use calc`, `use plot` or `use array`: makes the named members (or all of them) callable. What an import may
    /// unlock is decided by [`crate::reserved::plan_import`], the function the checker asks too: a member whose name
    /// the program already holds (a variable or a function) is refused, so is one that would remap a name the
    /// program already imported as another function, and nothing is unlocked unless every name is accepted.
    fn unlock_domain(&self, node: &crate::ast::ImportNode, members: &[&str], env: &HashMap<String, PhsValue>) -> PhysureResult<()> {
        if let crate::ast::ImportSpecifier::ModuleAlias(_) = &node.specifier {
            return Err(module_aliases_unsupported());
        }
        let domain = crate::reserved::domain_of(&node.path).expect("domain_members returned Some for unknown domain");
        let mut unlocked = self.unlocked_builtins.lock().unwrap_or_else(|e| e.into_inner());
        let imports = crate::reserved::plan_import(node, domain, members, &|name| env.contains_key(name), &unlocked)
            .map_err(|error| PhysureError::Generic(error.message().to_string()))?;
        unlocked.extend(imports);
        Ok(())
    }
```

- [ ] **Step 5: Run everything that guards the behaviour**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --no-fail-fast 2>&1 | grep -E '^test result|FAILED|panicked'`
Expected: all `ok` (the three new tests included, `reserved_names` 16 and `module_scope` unchanged).

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/reserved.rs physure-script/src/interpreter/statements.rs
git commit -m "refactor(script): one function decides what an import of a module unlocks

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `phs check` reports a reserved binding before the program runs

**Files:**
- Modify: `physure-script/src/globals.rs` (make `IMPORT_BINDING` `pub(crate)`)
- Modify: `physure-script/src/check.rs` (`check_program`'s call, `refuse_global_bindings`, the list of codes in the doc of `Diagnostic`)
- Test: `physure-script/tests/reserved_names.rs` (append)

- [ ] **Step 1: Write the failing tests**

Add to the `use` lines at the top of `physure-script/tests/reserved_names.rs`: `use physure_script::check::{check_source, Severity};` (merge with existing `physure_script` imports if they exist, no duplicates). Append:

```rust
/// The diagnostics of `program` that carry the code `reserved-name`: `(line, message)`.
fn reserved_diagnostics(program: &str) -> Vec<(usize, String)> {
    check_source(program)
        .unwrap_or_else(|e| panic!("{program:?}: {e}"))
        .into_iter()
        .filter(|d| d.code == "reserved-name" && d.severity == Severity::Error)
        .map(|d| (d.line, d.message))
        .collect()
}

#[test]
fn the_checker_says_what_the_interpreter_says_for_every_reserved_name_and_form() {
    for name in STATEMENT_WORDS.iter().chain(&LITERALS).chain(&CORE_FUNCTIONS) {
        for program in bindings_of(name) {
            let found = reserved_diagnostics(&program);
            assert!(
                found.iter().any(|(line, message)| *line == 1 && message.contains(&format!("`{name}` is"))),
                "{program:?}: {found:?}"
            );
            // and it is the same sentence the interpreter gives
            let said = refused(&program);
            assert!(found.iter().any(|(_, message)| *message == said), "{program:?}: checker {found:?}, interpreter {said:?}");
        }
    }
}

#[test]
fn the_checker_follows_the_imports_in_order() {
    for (program, line) in [
        ("use * from calc\nlimit = 5", 2),
        ("use solve as sv from calc\nsv = 1", 2),
        ("limit = 5\nuse * from calc", 2),
        ("use * from calc\nuse solve as limit from calc", 2),
        ("use * from calc\nfn f(solve) = solve", 2),
    ] {
        let found = reserved_diagnostics(program);
        assert!(found.iter().any(|(l, _)| *l == line), "{program:?}: {found:?}");
    }
}

#[test]
fn the_checker_does_not_report_what_the_interpreter_accepts() {
    for program in [
        "limit = 5\nlimit",
        "use solve from calc\nlimit = 5",
        "use solve as sv from calc\nsolve = 2",
        "use * from calc\nuse * from calc\n1",
        "use solve as sv from calc\nuse solve as sv from calc\n1",
        "m = 5 kg\ne = 2",
        "eq = \"F = sigma * A\"\neq(sigma=5, A=2)",
    ] {
        assert_eq!(reserved_diagnostics(program), vec![], "{program:?}");
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test reserved_names the_checker`
Expected: `the_checker_says_what_the_interpreter_says...` and `the_checker_follows_the_imports_in_order` FAIL (no `reserved-name` diagnostic exists); `the_checker_does_not_report...` passes.

- [ ] **Step 3: Expose the import marker**

In `physure-script/src/globals.rs` change `const IMPORT_BINDING: &str = "a name imported from";` to `pub(crate) const IMPORT_BINDING: &str = "a name imported from";`.

- [ ] **Step 4: Rewrite the checker's pre-pass**

In `physure-script/src/check.rs`, in `check_program`, change `checker.refuse_global_bindings(program);` to `checker.refuse_reserved_bindings(program);`. Replace the method `refuse_global_bindings` (with its doc comment, "A binding of `pi`, `π` or a constant of the catalog anywhere in a statement ... top-level statement that holds it.") by:

```rust
    /// A binding of a reserved name anywhere in a statement (function bodies included) is an error, as the
    /// interpreter says before the statement runs (`crate::globals`, `crate::reserved`): `pi`, `π` and the
    /// constants of the catalog (`constant-shadowing`), a statement word, a literal, a function of the core, and a
    /// function of a module the program has imported by then (`reserved-name`). The top-level statements are read
    /// in order, keeping what the imports have unlocked and the names the program holds, so an import over a held
    /// name or a remap is reported too. Each finding is at the line of the top-level statement that holds it.
    fn refuse_reserved_bindings(&mut self, program: &Program) {
        let mut unlocked = crate::reserved::Unlocked::new();
        let mut held: HashSet<String> = HashSet::new();
        for (index, statement) in program.statements.iter().enumerate() {
            self.line = program.lines.get(index).copied().unwrap_or(0);
            let mut bound = Vec::new();
            crate::globals::bindings_of(statement, &mut bound);
            for (name, what) in bound {
                if let Some(kind) = crate::reserved::static_kind(&name) {
                    self.report(Severity::Error, "reserved-name", crate::reserved::binding_error(&name, &what, kind));
                } else if crate::globals::is_global_in(&name, self.registry) {
                    let value = self.registry.constants_meta.get(&name).map(|meta| meta.value.clone());
                    let message = crate::globals::shadowing_error(&name, &what, value.as_deref());
                    self.report(Severity::Error, "constant-shadowing", message);
                } else if !what.starts_with(crate::globals::IMPORT_BINDING) {
                    if let Some((domain, _)) = unlocked.get(&name) {
                        self.report(Severity::Error, "reserved-name", crate::reserved::module_binding_error(&name, domain, &what));
                    }
                }
            }
            if let Statement::Import(import) = statement {
                if let (Some(domain), Some(members)) = (crate::reserved::domain_of(&import.path), crate::builtins::domain_members(&import.path)) {
                    match crate::reserved::plan_import(import, domain, members, &|name| held.contains(name), &unlocked) {
                        Ok(imports) => unlocked.extend(imports),
                        Err(crate::reserved::ImportError::Reserved(message)) => self.report(Severity::Error, "reserved-name", message),
                        // the interpreter says so when it runs the import; the checker has no code for it
                        Err(crate::reserved::ImportError::NoSuchFunction(_)) => {}
                    }
                }
            }
            collect_assigned(std::slice::from_ref(statement), &mut held);
        }
        self.line = 0;
    }
```

(`HashSet` is already imported in `check.rs`; if the compiler asks for other imports, add them. Keep the same order of diagnostics for the constants as before: they are still reported in statement order.) In the doc of `Diagnostic` (the list "`dimension-mismatch`, `unknown-unit`, ... `constant-shadowing`, `not-a-bool`"), add `` `reserved-name` ``.

- [ ] **Step 5: Run the tests**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test reserved_names --test global_constants --test zero_and_conditions`
Expected: all pass. Then the whole crate and the crates that use the checker: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script -p physure-cli -p physure-lsp --no-fail-fast 2>&1 | grep -E '^test result|FAILED|panicked'` — all `ok`. If an existing test fails because a program it checks binds a reserved name, STOP and report with the test and message.

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/globals.rs physure-script/src/check.rs physure-script/tests/reserved_names.rs
git commit -m "feat(script): phs check reports a reserved name before the program runs

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: A symbolic text refuses the words PHS reads as something other than a name

**Files:**
- Modify: `physure-script/src/symbolic/parser.rs` (`parse_identifier`, a new private function)
- Test: `physure-script/tests/builtins_calc_array_plot.rs` (append)

- [ ] **Step 1: Write the failing test**

Append to `physure-script/tests/builtins_calc_array_plot.rs`:

```rust
/// The follow-up of audit T2: the symbolic parser read `and`, `or` and `not` as symbols and multiplied them
/// (`simplify("x and y")` was `and * x * y`, `deriv("x not y", "x")` was `not * y`), and a function written without
/// its parentheses (`sin * x`) as a symbol called `sin`.
#[test]
fn a_word_the_language_reads_is_not_a_symbol_of_a_symbolic_text() {
    for (text, word) in [
        ("x and y", "and"),
        ("x or y", "or"),
        ("not x", "not"),
        ("if x", "if"),
        ("x where y", "where"),
        ("True * x", "True"),
        ("true * x", "true"),
        ("x * inf", "inf"),
        ("sin * x", "sin"),
        ("x * log", "log"),
    ] {
        let message = calc_refused(&format!(r#"simplify("{text}")"#));
        assert!(message.contains(&format!("`{word}`")), "{text}: {message}");
    }
    // a function with its parentheses, and a word that is only a name, stay what they were
    assert!(calc(r#"simplify("sin(x) * 2")"#).contains("sin(x)"));
    assert_eq!(calc(r#"deriv("sigma^2", "sigma")"#), "2 * sigma");
    assert!(calc(r#"deriv("max * x", "x")"#).contains("max"));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test builtins_calc_array_plot a_word_the_language`
Expected: FAIL ("should be refused, it gave ..."). If one of the sample texts is refused by the lexer or the grammar already with a message that does not name the word, report which, and remove that sample (keep the others).

- [ ] **Step 3: Refuse the bare word**

In `physure-script/src/symbolic/parser.rs`, in `parse_identifier`, change its last line `Ok(Node::Symbol(name.to_string()))` to:

```rust
        refuse_bare_word(name)?;
        Ok(Node::Symbol(name.to_string()))
```

and add, next to `unary_function` (at file level):

```rust
/// A word PHS reads as something other than a name is not a symbol of a symbolic text: `x and y` was `and * x * y`,
/// and a function without its parentheses (`sin * x`) a symbol called `sin`. The words that are only plausible names
/// (`sigma`, `max`, `dof`, a statement word) stay symbols, as the variables of an equation call do.
fn refuse_bare_word(name: &str) -> PhysureResult<()> {
    use crate::reserved::{static_kind, Reserved};
    let message = match static_kind(name) {
        Some(Reserved::Keyword) => format!("`{name}` is a word of the language and is not a symbol of a symbolic text; use another name"),
        Some(Reserved::Literal) => format!("`{name}` is a literal of the language and is not a symbol of a symbolic text; use another name"),
        _ if unary_function(name).is_some() || matches!(name, "deriv" | "integral") => {
            format!("`{name}` is a function: write it with its argument in parentheses, `{name}(x)`")
        }
        _ => return Ok(()),
    };
    Err(PhysureError::Generic(message))
}
```

- [ ] **Step 3b: Check the assertions the test makes about the symbols that stay**

If `deriv("sigma^2", "sigma")` or `deriv("max * x", "x")` print differently from the expected text (for example the order of a product), check by hand that the derivative is mathematically right (d/dσ σ² = 2σ; d/dx (max·x) = max) and fix the expected string, not the code; tell me.

- [ ] **Step 4: Run the test and everything that reads symbolic texts**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script -p physure-cli -p physure-lsp --no-fail-fast 2>&1 | grep -E '^test result|FAILED|panicked'`
Expected: all `ok`. If an existing test uses one of the refused words as a symbol (`inf`, `oo`, `true` ... inside a symbolic text), STOP and report the test, the text and the message: do not edit it.

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/symbolic/parser.rs physure-script/tests/builtins_calc_array_plot.rs
git commit -m "fix(script): a word the language reads is not a symbol of a symbolic text

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The language server forgets what a deleted `use` unlocked

**Files:**
- Modify: `physure-script/src/interpreter/statements.rs` (new public method next to `module_domain`)
- Modify: `physure-lsp/src/incremental.rs` (`apply_change`, and tests in its `mod tests`)

- [ ] **Step 1: Write the failing tests**

In `mod tests` of `physure-lsp/src/incremental.rs` add:

```rust
    #[test]
    fn deleting_an_import_frees_the_names_it_unlocked() {
        let first = apply_change(None, "use * from calc\n");
        assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
        let second = apply_change(Some(first.state), "limit = 5\n");
        assert!(second.diagnostics.is_empty(), "limit is free again: {:?}", second.diagnostics);
    }

    #[test]
    fn narrowing_an_import_frees_the_rest_of_the_module() {
        let first = apply_change(None, "use * from calc\n");
        let second = apply_change(Some(first.state), "use solve from calc\nlimit = 5\n");
        assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    }

    #[test]
    fn an_import_that_stays_keeps_its_names_reserved_through_an_unrelated_edit() {
        let first = apply_change(None, "use * from calc\nx = 1\n");
        let second = apply_change(Some(first.state), "use * from calc\nx = 2\nlimit = 5\n");
        assert_eq!(second.diagnostics.len(), 1, "{:?}", second.diagnostics);
        assert!(second.diagnostics[0].message.contains("`limit` is a function of `calc`"), "{:?}", second.diagnostics);
    }
```

(`diagnostics` and `message` are the fields of the `Diagnostic` type this file returns in `ChangeOutcome`; if the field that holds the text has another name, use it.)

- [ ] **Step 2: Run them to see them fail**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-lsp deleting_an_import narrowing_an_import an_import_that_stays`
Expected: the first two FAIL (a diagnostic says `limit` is a function of `calc`); the third passes.

- [ ] **Step 3: Give the interpreter a way to forget its imports**

In `physure-script/src/interpreter/statements.rs`, right after `module_domain`, add:

```rust
    /// Forgets every function a `use` unlocked. A host that keeps one interpreter alive across edits of a text (the
    /// language server) calls it before it replays the imports of the new text.
    pub fn reset_imports(&self) {
        self.unlocked_builtins.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
```

- [ ] **Step 4: Replay the imports when one was added, changed or removed**

In `physure-lsp/src/incremental.rs`, in `apply_change`, right after the loop that removes `touched_names` from `state.interp.env` and before the loop `for (i, stmt) in new_statements.iter().enumerate()`, add:

```rust
    // What a `use` unlocked is kept by the interpreter for as long as the document lives. When the old or the new
    // text has an import, forget it and run the imports of the new text again (they are idempotent): a name the old
    // text imported is free again, and one the new text imports is reserved from the first dirty statement on.
    let is_import = |statement: &Statement| matches!(statement, Statement::Import(_));
    if old_statements.iter().chain(&new_statements).any(is_import) {
        state.interp.reset_imports();
        for statement in new_statements.iter().filter(|s| is_import(s)) {
            // a refused import shows its error when it is dirty, or keeps the one it already had
            let _ = state.interp.run_statement(statement);
        }
    }
```

- [ ] **Step 5: Run the tests**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-lsp -p physure-script --no-fail-fast 2>&1 | grep -E '^test result|FAILED|panicked'`
Expected: all `ok`.

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/interpreter/statements.rs physure-lsp/src/incremental.rs
git commit -m "fix(lsp): a deleted or narrowed use frees the names it had unlocked

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Spec, changelog and roadmap

**Files:**
- Modify: `docs/superpowers/specs/2026-11-16-reserved-names-design.md` (§5 items 3 and 4, §11, §9)
- Modify: `CHANGELOG.md` (`### Changed`, and `### Fixed`, under `## [Unreleased]`)
- Modify: `docs/ROADMAP.md`

- [ ] **Step 1: The spec says what phase 2 did**

In §5 of the spec:
- Item 3 (`phs check`): replace its text with "**`phs check`.** The pre-pass of `check.rs` (`refuse_reserved_bindings`) reads the top-level statements in order, keeping what the imports have unlocked and the names the program holds, and reports with the code `reserved-name` a reserved word, literal or core function bound in any form, a module function bound after its import, an import over a held name and a remap. A constant keeps `constant-shadowing`. The decision about an import is `reserved::plan_import`, the function the interpreter calls too."
- Item 4 (symbolic parser): replace its text with "**Symbolic-string parser** (`symbolic/parser.rs`). A word PHS reads as something other than a name is not a bare symbol: the keywords (`and`, `or`, `not`, `if`...), the literals (`true`, `inf`...) and the functions the parser itself knows (`sin`, `log`, `deriv`, `integral`...) are an error that names the class; with parentheses a function is a call, as before. The other core-function names (`sigma`, `max`, `min`, `dof`...) and the statement words stay symbols: they are plausible physical symbols, and the variables of an equation call with those names are allowed too. This narrows the first draft of this item, which listed every reserved name."
In §11 delete the bullet that begins "**The language server keeps one interpreter per open document" and add: "* **The language server replays the imports.** When the old or the new text of a document has a `use`, `apply_change` forgets what was unlocked and runs the imports of the new text again before the dirty statements."

- [ ] **Step 2: The changelog**

Under `## [Unreleased]`, in `### Changed`, add as the last bullet:

```markdown
- **`phs check` reports a reserved name before the program runs** (phase 2 of the reserved names; commit `<hash of Task 2>`). The checker reads the top-level statements in order and reports, with the code `reserved-name`, a statement word, a literal or a core function bound in any form, a function of an imported module bound after its `use`, an import over a name the program holds, and an import that would remap one; it says what the interpreter says. The decision about an import is one function (`reserved::plan_import`) that the interpreter calls too.
```

and in `### Fixed` add:

```markdown
- **A symbolic text no longer reads the words of the language as symbols** (commit `<hash of Task 3>`). `simplify("x and y")` was `and * x * y`, `deriv("x not y", "x")` was `not * y`, and `sin * x` a product with a symbol called `sin`. The keywords (`and`, `or`, `not`, `if`, ...), the literals (`true`, `inf`, ...) and the functions the symbolic parser knows (`sin`, `log`, `deriv`, `integral`, ...) are an error when written bare; `sigma`, `max`, `min`, `dof` and the other names stay symbols. **What to change:** a symbolic text that used one of these words as a symbol, rename it.
- **The language server forgets what a deleted `use` unlocked** (commit `<hash of Task 4>`). After the user deleted or narrowed `use * from calc` in the editor, the old names stayed reserved in that document until it was reopened, and `limit = 5` was reported as an error.
```

Replace each `<hash of Task N>` with the 7-character hash of that task's commit (`git log --format=%h -1 --grep '<subject words>'`).

- [ ] **Step 3: The roadmap**

In `docs/ROADMAP.md`: change the line that begins `- [ ] The symbolic parser reads `and`, `or` and `not` as symbols` to start with `- [x]` and past tense ("The symbolic parser read `and`, `or` and `not` as symbols ... Fixed in phase 2 of the reserved names."); in the line `- [ ] Reserved names that no program can bind`, replace "Phase 1 is done: the table, the interpreter, the imports. Phase 2 (`phs check` and the symbolic parser, which closes the `and`, `or`, `not` item above, plus resetting what an import unlocks in the language server) and phase 3 (hosts, the generated section of the language guide) are open." with "Phases 1 (the table, the interpreter, the imports) and 2 (`phs check`, the symbolic parser, the language server) are done. Phase 3 (hosts, the generated section of the language guide) is open." Keep the rest of that line (the known gaps) as it is.

- [ ] **Step 4: Build the docs and commit**

Run: `cd physure-python && uv run mkdocs build --strict 2>&1 | tail -1` — expected `INFO    -  Documentation built in ...`.
Check line endings: `python -c "b=open('CHANGELOG.md','rb').read(); print(b.count(b'\n')-b.count(b'\r\n'))"` prints 0, same for `docs/ROADMAP.md` and the spec.

```bash
git add CHANGELOG.md docs/ROADMAP.md docs/superpowers/specs/2026-11-16-reserved-names-design.md
git commit -m "docs: the spec, the changelog and the roadmap record phase 2 of the reserved names

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-review (against the spec)

- **§5.3 checker:** Task 2 (static classes, module functions by order, import over a held name, remap; the agreement test with the interpreter's own sentence).
- **§5.4 symbolic parser:** Task 3, with the deviation recorded in the working notes and written into the spec by Task 5.
- **§11 language-server bullet and the phase 2 item of §9:** Task 4 and Task 5.
- **§10 phase-2 bullets:** the symbolic texts and `deriv("sin(x)", "x")` still reading `sin` as a call (Task 3's last assertions), `phs check` and the interpreter agree on every case of the table (Task 2's first test).
- **Out of this plan (phase 3):** hosts (the Java equation route, plugins, host-registered externals), the generated section of the language guide and its test.
- **Names used across tasks:** `reserved::{Unlocked, domain_of, ImportError, plan_import}` (Task 1), `globals::IMPORT_BINDING` made `pub(crate)` and `Checker::refuse_reserved_bindings` (Task 2), `refuse_bare_word` (Task 3), `PhsInterpreter::reset_imports` (Task 4); each is defined in the task that introduces it and used with the same signature later.
