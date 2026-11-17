# Reserved names, phase 1: the table and the interpreter — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A program can no longer bind a statement word, a literal or a core function of PHS, nor a function of a module it imports, and says so before it runs.

**Architecture:** One module, `physure-script/src/reserved.rs`, holds the tables and the messages. `globals::refuse_binding` (static classes plus the catalog constants) and `globals::refuse_global_bindings` (once per statement, with a lookup of the module functions the program has imported) call it. `unlock_domain` refuses an import that would replace a name the program already holds. Phases 2 (checker, symbolic parser) and 3 (hosts, guide) are separate plans.

**Tech Stack:** Rust (`physure-script` crate), the `eval_phs` test entry point.

**Spec:** `docs/superpowers/specs/2026-11-16-reserved-names-design.md` (§3 set, §5.1-5.2 enforcement, §6 messages, §9 phase 1).

**Working notes (read once):**
- Work on the branch `docs/post-release-0.4.0`, in the worktree `.claude/worktrees/post-release-040`. Do not touch `main`.
- Build in the shared target so no second multi-GB `target/` appears: prefix every cargo command with `CARGO_TARGET_DIR=D:/Projects/physure/target`.
- Source and docs files are CRLF in the working tree. Edit them with the editor tool, or with Python in default text mode (`open(p, encoding='utf-8')`), never `newline=''`, and chain a script and its `git commit` with `&&`.
- Measured before writing this plan (2026-11-16): none of the 4 `.phs` files, the documentation examples, nor the PHS programs in the Rust tests binds a statement word, a literal or a core function as a variable, a parameter or a loop variable. Every one of the 41 names binds today in each of the 9 forms of Task 2 without error. Task 5 re-checks this by running every suite.
- As built, the module doc says `static_kind` where this plan's first draft said `kind`; Tasks 2-4 grew review fixes recorded in the git history (commits after each task).

---

### Task 1: The table, its messages and its drift tests

**Files:**
- Create: `physure-script/src/reserved.rs`
- Modify: `physure-script/src/lib.rs` (add `pub mod reserved;` after `pub mod globals;`)

- [ ] **Step 1: Write the module with its tests**

Create `physure-script/src/reserved.rs`:

```rust
//! The names a program can never bind (spec `docs/superpowers/specs/2026-11-16-reserved-names-design.md`).
//!
//! Classes decided by the user: the keywords of the grammar, the statement words, the literals, the functions of
//! the core, and, while the program imports them, the functions of a module (`calc`, `array`, `plot`). The
//! constants of the catalog (`pi`, `π`, `[Constants]`) are reserved by [`crate::globals`]. Not reserved: unit
//! names, the words of a format, `e`, single letters.
//!
//! `kind` answers for the classes that do not depend on the program. A module function is reserved by what the
//! interpreter has unlocked, which only the interpreter knows (`PhsInterpreter::module_domain`).

/// The class of a reserved name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reserved {
    /// A keyword of the grammar (`phs.pest`, rule `keyword`). The grammar refuses it before any check runs.
    Keyword,
    /// A word the grammar reads in a statement: `use`, `import`, `fn`...
    StatementWord,
    /// A value the language names: `true`, `None`, `inf`...
    Literal,
    /// A function `eval_core_builtin` answers.
    CoreFunction,
}

/// The rule `keyword` of the grammar, word for word.
pub const KEYWORDS: [&str; 13] =
    ["for", "while", "let", "if", "then", "else", "where", "return", "not", "and", "or", "True", "False"];

/// The words of `import_stmt`, `export_stmt`, `function_def` and `for_expr`. `def` is not a word of PHS.
pub const STATEMENT_WORDS: [&str; 7] = ["use", "from", "import", "export", "as", "fn", "in"];

/// `True` and `False` are keywords; the lower-case forms and `None` are reserved so that they are never a second,
/// different thing. `inf`, `infinity` and `oo` are the infinity literals of the rule `number`. `NaN` is not a
/// literal of the language.
pub const LITERALS: [&str; 7] = ["true", "false", "None", "none", "inf", "infinity", "oo"];

/// The names `eval_core_builtin` answers, without the `op_*` names the operators are rewritten to.
pub const CORE_FUNCTIONS: [&str; 27] = [
    "abs", "approx", "assert", "ceil", "cos", "coverage_factor", "dof", "exact_assert", "exp", "expanded", "floor",
    "format", "from_samples", "if_then_else", "ln", "log", "max", "min", "parallel_map", "round", "sigma", "sin",
    "sqrt", "tan", "ternary", "uncertainty", "vector",
];

/// The class of `name` among the classes that do not depend on the program, `None` when it is free.
pub fn static_kind(name: &str) -> Option<Reserved> {
    if KEYWORDS.contains(&name) {
        Some(Reserved::Keyword)
    } else if STATEMENT_WORDS.contains(&name) {
        Some(Reserved::StatementWord)
    } else if LITERALS.contains(&name) {
        Some(Reserved::Literal)
    } else if CORE_FUNCTIONS.contains(&name) {
        Some(Reserved::CoreFunction)
    } else {
        None
    }
}

impl Reserved {
    fn describe(self) -> &'static str {
        match self {
            Reserved::Keyword | Reserved::StatementWord => "a word of the language",
            Reserved::Literal => "a literal of the language",
            Reserved::CoreFunction => "a function of PHS",
        }
    }
}

/// The error for binding the reserved `name` as `what` ("a variable", "a parameter of `f`", ...).
pub fn binding_error(name: &str, what: &str, kind: Reserved) -> String {
    format!("`{name}` is {} and cannot be bound as {what}; choose another name", kind.describe())
}

/// The error for binding `name`, a function of the module `domain` that the program imports, as `what`.
pub fn module_binding_error(name: &str, domain: &str, what: &str) -> String {
    format!("`{name}` is a function of `{domain}`, which this program imports, and cannot be bound as {what}; choose another name")
}

/// The error for importing the function `name` of `domain` over a name the program already holds.
pub fn module_import_error(name: &str, domain: &str) -> String {
    format!("`{name}` is a function of `{domain}`, which this program imports, and `{name}` is already bound in the program; choose another name for it")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut words: Vec<String>) -> Vec<String> {
        words.sort();
        words
    }

    #[test]
    fn the_keywords_are_those_of_the_grammar() {
        let pest = include_str!("phs.pest");
        let line = pest.lines().find(|l| l.starts_with("keyword")).expect("the rule `keyword`");
        let inside = line.split('(').nth(1).and_then(|rest| rest.split(')').next()).expect("the alternatives");
        let in_grammar: Vec<String> = inside.split('|').map(|w| w.trim().trim_matches('"').to_string()).collect();
        assert_eq!(sorted(in_grammar), sorted(KEYWORDS.iter().map(|w| w.to_string()).collect()));
    }

    #[test]
    fn the_core_functions_are_the_arms_of_eval_core_builtin() {
        let source = include_str!("builtins/core.rs");
        let start = source.find("pub fn eval_core_builtin(").expect("the dispatcher");
        let end = start + source[start..].find("_ => Ok(None),").expect("its last arm");
        let mut arms = Vec::new();
        for line in source[start..end].lines() {
            let line = line.trim_start();
            if !line.starts_with('"') {
                continue;
            }
            for part in line.split("=>").next().unwrap_or("").split('|') {
                let name = part.trim().trim_matches('"');
                if !name.starts_with("op_") {
                    arms.push(name.to_string());
                }
            }
        }
        assert_eq!(sorted(arms), sorted(CORE_FUNCTIONS.iter().map(|w| w.to_string()).collect()));
    }

    #[test]
    fn the_statement_words_and_the_infinity_literals_are_read_by_the_grammar() {
        let pest = include_str!("phs.pest");
        for word in STATEMENT_WORDS.iter().chain(["inf", "infinity", "oo"].iter()) {
            assert!(pest.contains(&format!("\"{word}\"")), "the grammar no longer reads `{word}`");
        }
    }

    #[test]
    fn no_name_is_in_two_classes() {
        let all: Vec<&str> = KEYWORDS.iter().chain(&STATEMENT_WORDS).chain(&LITERALS).chain(&CORE_FUNCTIONS).copied().collect();
        let mut unique = all.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(all.len(), unique.len());
    }

    #[test]
    fn a_name_is_free_unless_a_class_has_it() {
        assert_eq!(static_kind("max"), Some(Reserved::CoreFunction));
        assert_eq!(static_kind("use"), Some(Reserved::StatementWord));
        assert_eq!(static_kind("inf"), Some(Reserved::Literal));
        assert_eq!(static_kind("while"), Some(Reserved::Keyword));
        for free in ["m", "s", "kg", "e", "base", "frac", "x", "limit", "def", "NaN"] {
            assert_eq!(static_kind(free), None, "{free}");
        }
    }

    #[test]
    fn the_messages_name_the_class_and_the_way_out() {
        assert_eq!(
            binding_error("max", "a variable", Reserved::CoreFunction),
            "`max` is a function of PHS and cannot be bound as a variable; choose another name"
        );
        assert_eq!(
            binding_error("use", "a parameter of `f`", Reserved::StatementWord),
            "`use` is a word of the language and cannot be bound as a parameter of `f`; choose another name"
        );
        assert_eq!(
            module_binding_error("limit", "calc", "a variable"),
            "`limit` is a function of `calc`, which this program imports, and cannot be bound as a variable; choose another name"
        );
    }
}
```

Add to `physure-script/src/lib.rs`, right after the line `pub mod globals;`:

```rust
pub mod reserved;
```

- [ ] **Step 2: Run the tests**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --lib reserved::`
Expected: 6 passed. If `the_core_functions_are_the_arms_of_eval_core_builtin` fails, the message prints both lists: a builtin was added or removed since this plan was written, so make `CORE_FUNCTIONS` (and its length) match the arms exactly.

- [ ] **Step 3: Commit**

```bash
git add physure-script/src/reserved.rs physure-script/src/lib.rs
git commit -m "feat(script): the table of reserved names, its messages and its drift tests

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: A program cannot bind a statement word, a literal or a core function

**Files:**
- Create: `physure-script/tests/reserved_names.rs`
- Modify: `physure-script/src/globals.rs` (`refuse_binding`, lines 101-111)

- [ ] **Step 1: Write the failing test**

Create `physure-script/tests/reserved_names.rs`:

```rust
//! Reserved names (spec `docs/superpowers/specs/2026-11-16-reserved-names-design.md`): the statement words, the
//! literals and the functions of the core can never be bound, in any form that binds, and the names that are not
//! reserved still bind.

use physure_script::interpreter::eval_phs;
use physure_script::reserved::{CORE_FUNCTIONS, LITERALS, STATEMENT_WORDS};
use physure_script::{parse_phs, PhsInterpreter, PhsValue};

/// The message with which `program` is refused.
fn refused(program: &str) -> String {
    match eval_phs(program) {
        Ok(values) => panic!("{program:?} should be refused, it gave {:?}", values.last().map(|v| v.to_string())),
        Err(e) => e.to_string(),
    }
}

/// The text of the value of the last statement of `program`.
fn value(program: &str) -> String {
    let values = eval_phs(program).unwrap_or_else(|e| panic!("{program:?} failed: {e}"));
    values.last().expect("a value").to_string()
}

/// Every way a program binds a name, with `name`.
fn bindings_of(name: &str) -> Vec<String> {
    vec![
        format!("{name} = 3"),
        format!("f({name}) = {name} * 2"),
        format!("fn g({name}) = {name}"),
        format!("for {name} in 0..3 {{ 1 }}"),
        format!("x = 2 * y where y = 1, {name} = 3"),
        format!("{name}(x) = x"),
        // inside a body that never runs
        format!("fn h(x) = x * a where {name} = 1, a = 2"),
        format!("fn k(x) = for {name} in 0..3 {{ x }}"),
        format!("use solve as {name} from calc"),
    ]
}

#[test]
fn a_statement_word_a_literal_or_a_core_function_cannot_be_bound_in_any_form() {
    for name in STATEMENT_WORDS.iter().chain(&LITERALS).chain(&CORE_FUNCTIONS) {
        for program in bindings_of(name) {
            let message = refused(&program);
            assert!(
                message.contains(&format!("`{name}` is")) && message.contains("cannot be bound as") && message.contains("choose another name"),
                "{program:?}: {message}"
            );
        }
    }
}

#[test]
fn the_message_names_the_class() {
    assert!(refused("max = 3").contains("`max` is a function of PHS and cannot be bound as a variable"));
    assert!(refused("use = 3").contains("`use` is a word of the language and cannot be bound as a variable"));
    assert!(refused("inf = 3").contains("`inf` is a literal of the language and cannot be bound as a variable"));
    assert!(refused("fn f(round) = round").contains("cannot be bound as a parameter of `f`"));
}

#[test]
fn a_name_that_is_not_reserved_still_binds() {
    // unit names are the variables of physics; `e`, the words of a format and a function of a module are free
    // until the module is imported
    for name in ["m", "s", "kg", "e", "base", "frac", "x", "limit", "NaN"] {
        assert_eq!(value(&format!("{name} = 3\n{name}")), "3.0", "{name}");
    }
}

#[test]
fn a_value_a_host_gives_cannot_be_a_reserved_name() {
    let program = parse_phs("1").expect("a program");
    let error = PhsInterpreter::default()
        .run_program_with(&program, vec![("max".to_string(), PhsValue::Number(1.0))])
        .unwrap_err()
        .to_string();
    assert!(error.contains("`max` is a function of PHS and cannot be bound as a value given by the host"), "{error}");
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test reserved_names`
Expected: `a_statement_word_a_literal_or_a_core_function_cannot_be_bound_in_any_form`, `the_message_names_the_class` and `a_value_a_host_gives_cannot_be_a_reserved_name` FAIL (`"... should be refused, it gave ..."`); `a_name_that_is_not_reserved_still_binds` passes.

- [ ] **Step 3: Make `refuse_binding` ask the table first**

In `physure-script/src/globals.rs`, replace the body of `refuse_binding`:

```rust
pub fn refuse_binding(name: &str, what: &str) -> PhysureResult<()> {
    if PI_NAMES.contains(&name) {
        return Err(PhysureError::Generic(shadowing_error(name, what, None)));
    }
    let value = with_catalog_in_force(|registry| registry.constants_meta.get(name).map(|meta| meta.value.clone()))?;
    match value {
        Some(value) => Err(PhysureError::Generic(shadowing_error(name, what, Some(&value)))),
        None => Ok(()),
    }
}
```

with:

```rust
pub fn refuse_binding(name: &str, what: &str) -> PhysureResult<()> {
    if let Some(kind) = crate::reserved::static_kind(name) {
        return Err(PhysureError::Generic(crate::reserved::binding_error(name, what, kind)));
    }
    if PI_NAMES.contains(&name) {
        return Err(PhysureError::Generic(shadowing_error(name, what, None)));
    }
    let value = with_catalog_in_force(|registry| registry.constants_meta.get(name).map(|meta| meta.value.clone()))?;
    match value {
        Some(value) => Err(PhysureError::Generic(shadowing_error(name, what, Some(&value)))),
        None => Ok(()),
    }
}
```

Also update the module documentation of `globals.rs`: in the paragraph that begins `**Shadowing is an error.**`, add the sentence `The statement words, the literals and the functions of the core are refused the same way (see [`crate::reserved`]).` after its first sentence's list.

- [ ] **Step 4: Run the new tests and the neighbours**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test reserved_names --test global_constants`
Expected: all pass (the constants keep their messages: `refuse_binding` only returns early for the new classes).

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/globals.rs physure-script/tests/reserved_names.rs
git commit -m "feat(script): a statement word, a literal or a core function cannot be bound

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: A function of an imported module is reserved from the import on

**Files:**
- Modify: `physure-script/src/globals.rs` (`refuse_global_bindings`, lines 179-192)
- Modify: `physure-script/src/interpreter/statements.rs` (the two calls at lines 94 and 261; a new method `module_domain` next to `unlock_domain`)
- Test: `physure-script/tests/reserved_names.rs`

- [ ] **Step 1: Write the failing tests**

Append to `physure-script/tests/reserved_names.rs`:

```rust
#[test]
fn a_function_of_a_module_is_free_until_the_module_is_imported() {
    assert_eq!(value("limit = 5\nlimit"), "5.0");
    assert_eq!(value("fn series(a) = a + 1\nseries(1)"), "2.0");
}

#[test]
fn a_function_of_an_imported_module_cannot_be_bound() {
    for program in [
        "use * from calc\nlimit = 5",
        "use * from calc\nfn series(a) = a",
        "use * from calc\nfn f(solve) = solve",
        "use * from calc\nfor deriv in 0..3 { 1 }",
        "use * from array\nnorm = 1",
        "use * from plot\nplot = 1",
    ] {
        let message = refused(program);
        assert!(message.contains("is a function of `") && message.contains("which this program imports"), "{program:?}: {message}");
    }
}

#[test]
fn only_what_is_imported_is_reserved() {
    // `use solve from calc` brings `solve` and nothing else of calc
    assert_eq!(value("use solve from calc\nlimit = 5\nlimit"), "5.0");
    assert!(refused("use solve from calc\nsolve = 1").contains("`solve` is a function of `calc`"));
    // an alias reserves the alias and leaves the name free
    assert_eq!(value("use solve as sv from calc\nsolve = 2\nsolve"), "2.0");
    assert!(refused("use solve as sv from calc\nsv = 1").contains("`sv` is a function of `calc`"));
}

#[test]
fn importing_a_module_twice_is_not_a_binding() {
    assert_eq!(value("use * from calc\nuse * from calc\n1"), "1.0");
    assert_eq!(value("use solve from calc\nuse solve from calc\n1"), "1.0");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test reserved_names a_function_of an only_what importing_a_module`
Expected: `a_function_of_an_imported_module_cannot_be_bound` and `only_what_is_imported_is_reserved` FAIL (the programs run); the other two pass.

- [ ] **Step 3: Give `refuse_global_bindings` a lookup of the imported module functions**

In `physure-script/src/globals.rs`, replace `refuse_global_bindings` (the whole function with its doc comment, lines 179-192) with:

```rust
/// Refuses `statement` when it binds a reserved name anywhere inside it: a global identifier of the catalog in
/// force, a statement word, a literal, a function of the core, or a function of a module the program has imported
/// (`module_domain` answers the module of such a name). Checked once for a statement before it runs, so a loop
/// does not pay for it at every iteration and a body that never runs is checked too. An `import` statement is not
/// checked against the module functions: running `use * from calc` twice binds nothing new, and
/// `PhsInterpreter::unlock_domain` refuses an import over a name the program holds.
pub(crate) fn refuse_global_bindings(
    statement: &Statement,
    module_domain: &dyn Fn(&str) -> Option<&'static str>,
) -> PhysureResult<()> {
    let mut bound = Vec::new();
    bindings_of(statement, &mut bound);
    if bound.is_empty() {
        return Ok(());
    }
    let is_import = matches!(statement, Statement::Import(_));
    for (name, what) in bound {
        refuse_binding(&name, &what)?;
        if !is_import {
            if let Some(domain) = module_domain(&name) {
                return Err(PhysureError::Generic(crate::reserved::module_binding_error(&name, domain, &what)));
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Pass the lookup from the interpreter**

In `physure-script/src/interpreter/statements.rs`:

Replace both calls

```rust
crate::globals::refuse_global_bindings(stmt)?;
```

(line 94, inside `run_statements_with_lines`, and line 261, inside `eval_statement_in`) with

```rust
crate::globals::refuse_global_bindings(stmt, &|name| self.module_domain(name))?;
```

and add this method to the same `impl` block, right before `unlock_domain`:

```rust
    /// The module (`calc`, `plot`, `array`) whose function `name` the program has imported under that name.
    pub(crate) fn module_domain(&self, name: &str) -> Option<&'static str> {
        self.unlocked_builtins.lock().unwrap_or_else(|e| e.into_inner()).get(name).map(|(domain, _)| *domain)
    }
```

- [ ] **Step 5: Run the tests**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test reserved_names`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/globals.rs physure-script/src/interpreter/statements.rs physure-script/tests/reserved_names.rs
git commit -m "feat(script): a function of an imported module cannot be bound

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: An import cannot replace a name the program already holds

**Files:**
- Modify: `physure-script/src/interpreter/statements.rs` (`resolve_use` line 317-321, `unlock_domain` lines 347-374)
- Test: `physure-script/tests/reserved_names.rs`

- [ ] **Step 1: Write the failing tests**

Append to `physure-script/tests/reserved_names.rs`:

```rust
#[test]
fn an_import_cannot_replace_a_name_the_program_already_holds() {
    for program in [
        "limit = 5\nuse * from calc",
        "fn series(a) = a\nuse * from calc",
        "solve = 1\nuse solve from calc",
        "sv = 1\nuse solve as sv from calc",
    ] {
        let message = refused(program);
        assert!(message.contains("is a function of `calc`") && message.contains("already bound in the program"), "{program:?}: {message}");
    }
    // a name the module does not bring stays free, before and after
    assert_eq!(value("x = 1\nuse solve from calc\nx"), "1.0");
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test reserved_names an_import_cannot_replace`
Expected: FAIL (`"... should be refused, it gave ..."`).

- [ ] **Step 3: Refuse the import over a bound name**

In `physure-script/src/interpreter/statements.rs`, in `resolve_use`, change

```rust
            self.unlock_domain(node, members)?;
```

to

```rust
            self.unlock_domain(node, members, env)?;
```

and change `unlock_domain` to take the environment and check each name before it unlocks anything:

```rust
    /// `use calc`, `use plot` or `use array`: makes the named members (or all of them) callable. A member whose
    /// name the program already holds (a variable or a function) is refused, so that a reserved name never has
    /// two meanings in one program.
    fn unlock_domain(&self, node: &crate::ast::ImportNode, members: &[&str], env: &HashMap<String, PhsValue>) -> PhysureResult<()> {
        use crate::ast::ImportSpecifier;

        let domain: &'static str = match node.path.as_str() {
            "calc" => "calc",
            "plot" => "plot",
            "array" => "array",
            _ => unreachable!("domain_members returned Some for unknown domain"),
        };
        let mut unlocked = self.unlocked_builtins.lock().unwrap_or_else(|e| e.into_inner());
        let held = |name: &str| -> PhysureResult<()> {
            if env.contains_key(name) {
                return Err(PhysureError::Generic(crate::reserved::module_import_error(name, domain)));
            }
            Ok(())
        };
        match &node.specifier {
            ImportSpecifier::Wildcard => {
                for &member in members {
                    held(member)?;
                }
                for &member in members {
                    unlocked.insert(member.to_string(), (domain, member.to_string()));
                }
            }
            ImportSpecifier::Symbols(syms) => {
                for sym in syms {
                    if !members.contains(&sym.name.as_str()) {
                        return Err(PhysureError::Generic(format!("no such function '{}' in domain '{}'", sym.name, node.path)));
                    }
                    let call_name = sym.alias.as_deref().unwrap_or(&sym.name).to_string();
                    held(&call_name)?;
                    unlocked.insert(call_name, (domain, sym.name.clone()));
                }
            }
            ImportSpecifier::ModuleAlias(_alias) => return Err(module_aliases_unsupported()),
        }
        Ok(())
    }
```

(`env` is the second parameter of `resolve_use` and is a `&mut HashMap`; passing `env` where `&HashMap` is expected reborrows it, so the call compiles unchanged.)

- [ ] **Step 4: Run the tests**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test reserved_names`
Expected: all pass. If the compiler rejects passing `env` (a `&mut`) to the new parameter, write `&*env` at the call.

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/interpreter/statements.rs physure-script/tests/reserved_names.rs
git commit -m "feat(script): an import cannot replace a name the program already holds

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Delete the guards that can no longer be false, and run everything

**Files:**
- Modify: `physure-script/src/interpreter/expressions.rs` (`eval_short_circuit`, lines 340-366)

- [ ] **Step 1: Delete the two guards**

In `eval_short_circuit`, change

```rust
            ("ternary" | "if_then_else", [cond, then, otherwise]) if !env.contains_key(name) => {
```

to

```rust
            ("ternary" | "if_then_else", [cond, then, otherwise]) => {
```

and change

```rust
            ("assert", [Expr::Identifier(name), ..]) if args.len() <= 2 && !env.contains_key("assert") => {
```

to

```rust
            ("assert", [Expr::Identifier(name), ..]) if args.len() <= 2 => {
```

(`ternary`, `if_then_else` and `assert` are core functions now, so `env` cannot hold them: no assignment, parameter, `where`, `for`, import or host value can bind them.)

- [ ] **Step 2: Run every suite that runs PHS**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script -p physure-cli -p physure-lsp --no-fail-fast 2>&1 | grep -E '^test result|FAILED|panicked'`
Expected: every `test result` line is `ok`. The pre-plan scan found no program that binds a reserved name, so no failure is expected. If a test fails with `cannot be bound as`, the program in that test binds a name that is now reserved: rename the variable in the test (for example `min` to `lowest`, `max` to `highest`) and say which test it was in the commit message.

- [ ] **Step 3: Commit**

```bash
git add physure-script/src/interpreter/expressions.rs
git commit -m "refactor(script): the guards for a variable named assert or ternary cannot be false any more

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Changelog and roadmap

**Files:**
- Modify: `CHANGELOG.md` (the `### Changed` list of `## [Unreleased]`)
- Modify: `docs/ROADMAP.md` (the list of section 3, "Close the open audit findings")

- [ ] **Step 1: Add the changelog entry**

In `CHANGELOG.md`, under `## [Unreleased]` / `### Changed`, add as the last bullet (the commit hash is the one of Task 2):

```markdown
- **A program can no longer bind a statement word, a literal or a core function, nor a function of a module it imports** (spec `docs/superpowers/specs/2026-11-16-reserved-names-design.md`, phase 1; commit `<hash of Task 2>`). `use`, `from`, `import`, `export`, `as`, `fn` and `in`; `true`, `false`, `None`, `none`, `inf`, `infinity` and `oo`; the 27 functions of the core (`abs`, `max`, `min`, `round`, `floor`, `ceil`, `sqrt`, `sin`, `cos`, `tan`, `exp`, `ln`, `log`, `format`, `assert`, `vector`, `uncertainty`, `sigma`, `dof`, ...); and, from the `use` on, the functions of `calc`, `array` or `plot` that the program imports (`use * from calc` reserves all of `calc`, `use solve from calc` reserves `solve`, `use solve as sv from calc` reserves `sv`) are refused as a variable, a parameter, a function name, a `for` variable, a `where` binding, an import alias or a value a host gives, with an error that names the class: "`max` is a function of PHS and cannot be bound as a variable; choose another name". An import that would replace a name the program already holds is refused too. Before, the call always reached the builtin, so `fn max(a, b) = 100` was accepted and never called, and `inf = 3` was accepted while `inf` stayed infinity. **What to change:** rename a variable, parameter or function that has one of these names. Unit names (`m`, `s`, `kg`), `e` and the words of a format (`base`, `frac`) are not reserved. Phases 2 (`phs check` and the symbolic parser) and 3 (hosts and the guide) are open.
```

Replace `<hash of Task 2>` with the 7-character hash printed by `git log --format=%h -1 --grep "cannot be bound"` for the Task 2 commit.

- [ ] **Step 2: Add the roadmap line**

In `docs/ROADMAP.md`, in section 3's list, after the line that begins `- [ ] The symbolic parser reads `and`, `or` and `not` as symbols`, add:

```markdown
- [ ] Reserved names that no program can bind (design: `docs/superpowers/specs/2026-11-16-reserved-names-design.md`). Phase 1 is done: the table, the interpreter, the imports. Phase 2 (`phs check` and the symbolic parser, which closes the `and`, `or`, `not` item above) and phase 3 (hosts, the generated section of the language guide) are open.
```

- [ ] **Step 3: Build the docs and commit**

Run: `cd physure-python && uv run mkdocs build --strict 2>&1 | tail -1`
Expected: `INFO    -  Documentation built in ...`

```bash
git add CHANGELOG.md docs/ROADMAP.md
git commit -m "docs: the changelog and the roadmap record phase 1 of the reserved names

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-review (run against the spec)

- **§3 the set:** keywords (table + grammar drift), statement words, literals, core functions (table + dispatcher drift), module functions (Task 3, from `unlocked_builtins`, which `domain_members` fills), constants (unchanged, ordering in `refuse_binding`). Not reserved: unit names, `e`, `base`, `frac`, `NaN`, `def` (Task 1 `a_name_is_free_unless_a_class_has_it`, Task 2 `a_name_that_is_not_reserved_still_binds`).
- **§5.1 interpreter:** Tasks 2, 3. **§5.2 import over a variable:** Task 4. **§5.5 hosts:** the host test of Task 2 (`run_program_with` already calls `refuse_binding`). The guards of §5.1 are deleted in Task 5.
- **§6 messages:** the three shapes of the spec are pinned in Task 1's `the_messages_name_the_class_and_the_way_out` and Task 3/4 tests.
- **§9 phase 1 "measure first":** done before this plan (see the working notes); Task 5 re-confirms with every suite.
- **Out of this plan (phases 2 and 3):** `phs check` diagnostic `reserved-name`, the symbolic-text refusal, the Python/WebAssembly/Java check through each binding, the generated guide section and its test.
- **Names used across tasks:** `reserved::{Reserved, KEYWORDS, STATEMENT_WORDS, LITERALS, CORE_FUNCTIONS, static_kind, binding_error, module_binding_error, module_import_error}`, `PhsInterpreter::module_domain`, `globals::refuse_global_bindings(statement, module_domain)`: each is defined in the task that introduces it and used with the same signature later.
