# An unbound name is an error (audit T5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A bare name with no binding, no `pi`/constant and no unit symbol is the error `` `foo` has no value `` in the interpreter, instead of evaluating to the text `"foo"`.

**Architecture:** The last branch of `eval_identifier` becomes the error. The only working program that relied on the fallback is the format spec (`x: base`, `x: .2f`), which the parser builds as `format(x, Identifier(spec))`; it is read as text before the change. The two patches that detected "a name that evaluated to its own text" disappear (the condition keeps its Bool hint). `check::explains` learns the new wording so the report does not say one problem twice.

**Tech Stack:** Rust (`physure-script`, tests in `physure-script/tests`), the spec `docs/superpowers/specs/2026-11-17-unbound-names-design.md`.

**Conventions for every task (from this repository):**
* Run every cargo command with `CARGO_TARGET_DIR=D:/Projects/physure/target` (bash). NEVER `cargo clean` it.
* Repo files are CRLF: edit with the Edit tool, or Python in default text mode; chain `&& git commit` so a failed edit cannot commit half of it; no `sed -i` on source files.
* English in code, docs and commits. Commit message ends with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
* Known Windows lock `Acceso denegado (os error 5)` in some `physure-cli` suites: re-run that suite alone.
* Python is not touched by this plan, but Task 5 rebuilds and runs it: `cd physure-python && unset VIRTUAL_ENV && CARGO_TARGET_DIR=D:/Projects/physure/target uv run --no-sync maturin develop --release`, verify `_core.__file__` is under the worktree, then `uv run --no-sync pytest -q`.

---

### Task 1: The guard over the documented programs (passes now, catches reliance later)

**Files:**
* Create: `physure-script/tests/docs_programs_need_no_unbound_names.rs`

The test runs every PHS program of the documentation and checks that none fails with a `has no value` error. Today it passes (the programs either work or fail for other reasons); after Task 3 any program that worked only through the fallback fails here by name.

- [ ] **Step 1: Find the programs and the evaluation entry point**

Read how `physure-script/tests/check_corpus.rs` and `physure-script/tests/report.rs` collect programs and evaluate them (`physure_script::report::evaluate`, `physure_script::interpreter::eval_phs`). Collect: `docs/tutorials/*.phs`, `docs/tutorials/phs_primer.md` blocks fenced as ```` ```phs ````, every other ```` ```phs ```` block under `docs/**/*.md` and `README.md` files of the workspace, and the guide `PHYSURE_LANGUAGE_GUIDE.md` blocks. Paths are relative to `env!("CARGO_MANIFEST_DIR")/..`.

- [ ] **Step 2: Write the test**

```rust
//! No documented program works only because an unbound name evaluates to its own text (audit T5).

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Every ```phs block of a Markdown text.
fn blocks(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in markdown.lines() {
        match (&mut current, line.trim_start()) {
            (None, l) if l.starts_with("```phs") => current = Some(String::new()),
            (Some(block), l) if l.starts_with("```") => {
                out.push(std::mem::take(block));
                current = None;
            }
            (Some(block), _) => {
                block.push_str(line);
                block.push('\n');
            }
            _ => {}
        }
    }
    out
}

fn files(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if !["target", "node_modules", ".git", ".claude", "site"].contains(&name) {
                files(&path, extension, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            out.push(path);
        }
    }
}

/// Programs shown on purpose with a name that has no value (the documentation of the error itself).
const SHOWS_THE_ERROR: &[&str] = &["has no value"];

#[test]
fn no_documented_program_relies_on_an_unbound_name() {
    let mut programs: Vec<(String, String)> = Vec::new();
    let mut phs = Vec::new();
    files(&root(), "phs", &mut phs);
    for path in phs {
        programs.push((path.display().to_string(), std::fs::read_to_string(&path).unwrap()));
    }
    let mut markdown = Vec::new();
    files(&root(), "md", &mut markdown);
    for path in markdown {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for (i, block) in blocks(&text).into_iter().enumerate() {
            programs.push((format!("{} block {i}", path.display()), block));
        }
    }
    assert!(programs.len() > 50, "found only {} programs: the search is broken", programs.len());

    let mut reliant = Vec::new();
    for (origin, source) in &programs {
        if SHOWS_THE_ERROR.iter().any(|shown| source.contains(shown)) {
            continue;
        }
        // use the same entry point as the report tests; replace this call with the one found in Step 1
        let outcome = physure_script::report::evaluate(source);
        let said = format!("{outcome:?}");
        if said.contains("has no value") {
            reliant.push(format!("{origin}: {}", said.lines().next().unwrap_or("")));
        }
    }
    assert!(reliant.is_empty(), "programs that rely on an unbound name:\n{}", reliant.join("\n"));
}
```

If `report::evaluate` has another signature, adapt the call (it must return something whose `Debug` or `Display` contains the messages of the findings); keep the rest.

- [ ] **Step 3: Run it, it must pass today**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test docs_programs_need_no_unbound_names`
Expected: PASS (the count assertion proves it found programs). If it fails today, a documented program already says `has no value`: read it; if it is not an intended example, report it instead of changing the test.

- [ ] **Step 4: Mutation check**

Temporarily change the check string to `"is not"` or inject a program with `while true`: the test must fail. Revert.

- [ ] **Step 5: Commit**

```bash
git add physure-script/tests/docs_programs_need_no_unbound_names.rs
git commit -m "test(script): no documented program relies on an unbound name (audit T5)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Format words are read as text, not evaluated

**Files:**
* Modify: `physure-script/src/interpreter/expressions.rs` (`eval_call`, the loop that evaluates `args`)
* Test: `physure-script/tests/format_specs_are_not_names.rs` (create)

The parser (`physure-script/src/parser/expressions.rs`, `parse_base_expr`) builds `format(left, Expr::Identifier(spec))`. Today the spec is evaluated through `eval_identifier` and works only because an unbound name is its own text; a variable named `base` would change its meaning. Do NOT change the AST (the code generators read it).

- [ ] **Step 1: Write the failing tests**

```rust
use physure_script::interpreter::eval_phs;

fn shows(source: &str) -> String {
    eval_phs(source).unwrap().to_string()
}

#[test]
fn a_format_word_formats_even_when_a_variable_has_its_name() {
    assert_eq!(shows("base = 3\n2 kohm: base"), shows("2 kohm: base"));
    assert_eq!(shows("frac = 7\n1.5: frac"), "3/2");
}

#[test]
fn the_format_words_still_format() {
    assert_eq!(shows("1.5: frac"), "3/2");
    assert_eq!(shows("1.5: ifrac"), "1 1/2");
    assert_eq!(shows("3.14159: .2f"), "3.14");
    assert_eq!(shows("12345.678: .2e"), "1.23e4");
}
```

Adjust the expected texts to what the current implementation prints (run the same programs without the `base = 3` line first and copy their output: the point of the test is that a variable no longer changes the result).

- [ ] **Step 2: Run, expect the first test to FAIL**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test format_specs_are_not_names`
Expected: `a_format_word_formats_even_when_a_variable_has_its_name` fails (the variable replaces the spec).

- [ ] **Step 3: Implement**

In `eval_call`, where the positional arguments are evaluated (`for arg in args { arg_vals.push(self.eval_expr(arg, env)?); }`), read the spec of `format` as its own text:

```rust
let mut arg_vals = Vec::new();
for (i, arg) in args.iter().enumerate() {
    // the spec of `x: base` is a word of the format, not a name to look up
    if name == "format" && i == 1 {
        if let Expr::Identifier(spec) = arg {
            arg_vals.push(PhsValue::String(spec.clone()));
            continue;
        }
    }
    arg_vals.push(self.eval_expr(arg, env)?);
}
```

`format` is a core function, so a user function cannot take the name (reserved names): the special case never hides a user call.

- [ ] **Step 4: Run the new tests and the whole `physure-script` suite**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked"`
Expected: all PASS (this task changes no behaviour except the shadowing case).

- [ ] **Step 5: Commit** (`fix(script): a format word is read as text, not looked up as a name (audit T5)`)

---

### Task 3: The rule

**Files:**
* Modify: `physure-script/src/interpreter/expressions.rs` (`eval_identifier`, `eval_binary_expr` convert patch, `eval_short_circuit` assert patch, `eval_condition`)
* Modify: `physure-script/src/check.rs` (`explains`, doc comments of the module that say "evaluates to its own text")
* Modify: `physure-script/src/interpreter/helpers.rs` (the doc of `KnownCondition::Unbound`)
* Modify (tests whose expected text changes): `physure-script/src/interpreter/tests.rs`, `physure-script/tests/{check_conditions,comparison_semantics,language_semantics,silent_results,zero_and_conditions,report,lsp_never_panics}.rs` and whatever else fails
* Create: `physure-script/tests/unbound_names.rs`

- [ ] **Step 1: Write the failing tests (`tests/unbound_names.rs`)**

```rust
use physure_script::interpreter::eval_phs;

fn refused(source: &str) -> String {
    match eval_phs(source) {
        Ok(value) => panic!("{source:?} gave {value}, expected an error"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn a_name_with_no_value_is_an_error_in_every_position() {
    for source in [
        "foo",
        "foo + 1",
        "1 + foo",
        "2 * foo",
        "sin(foo)",
        "x = foo",
        "if foo > 1 then 1 else 2",
        "for i in foo { i }",
        "foo .. 3",
        "[foo, 2]",
        "foo => m",
    ] {
        let message = refused(source);
        assert!(message.contains("`foo` has no value"), "{source:?}: {message}");
    }
}

#[test]
fn a_typo_in_a_variable_names_the_typo() {
    assert!(refused("speed = 3 m/s\nspeeed * 2 s").contains("`speeed` has no value"));
}

#[test]
fn what_has_a_value_is_not_unbound() {
    // a variable, a constant, a unit, a function parameter, a name bound by `where`
    assert!(eval_phs("v = 2\nv + 1").is_ok());
    assert!(eval_phs("pi * 2").is_ok());
    assert!(eval_phs("5 m + 2 m").is_ok());
    assert!(eval_phs("fn f(a) = a * 2\nf(3)").is_ok());
    assert!(eval_phs("x = 2 * y where y = 1").is_ok());
}

#[test]
fn a_condition_that_is_a_name_still_says_which_literals_are_bool() {
    let message = refused("while true {\n  1\n}");
    assert!(message.contains("`true` has no value"), "{message}");
    assert!(message.contains("True and False"), "{message}");
}

#[test]
fn text_stays_text_and_a_failing_span_stays_raw() {
    assert_eq!(eval_phs("\"foo\"").unwrap().to_string(), "foo");
    assert_eq!(eval_phs("\"{foo}\"").unwrap().to_string(), "{foo}");
}
```

If a source in the first test is not valid PHS (for example the list syntax), replace it with the closest valid construct (read `physure-script/src/parser` or the guide); keep one case per position named in the spec (statement, operand, call argument, condition, `=>` value, range, `for` iterable).

- [ ] **Step 2: Run, expect FAIL** (`foo` currently evaluates to the text).

- [ ] **Step 3: Implement the rule**

In `eval_identifier`, replace the last branch:

```rust
} else {
    return Err(PhysureError::Generic(format!("`{name}` has no value")));
}
```

(`eval_identifier` already returns `PhysureResult`; the other branches are inside `Ok(if ... )`: restructure to `Ok(match ...)` or return early, whichever reads best.)

In `eval_binary_expr`, delete the block that detects `(Expr::Identifier(name), PhsValue::String(text))` for `=>` (the general error now arrives from `self.eval_expr(left, env)?` before it).

In `eval_condition`, keep the hint for a bare name that has no value:

```rust
pub(crate) fn eval_condition(&self, expr: &Expr, env: &HashMap<String, PhsValue>, context: &str) -> PhysureResult<bool> {
    let value = match self.eval_expr(expr, env) {
        Err(error) if matches!(expr, Expr::Identifier(name) if error.to_string() == format!("`{name}` has no value")) => {
            let Expr::Identifier(name) = expr else { unreachable!() };
            return Err(PhysureError::Generic(format!(
                "`{name}` has no value; the Bool literals are True and False ({context} needs a Bool)"
            )));
        }
        other => other?,
    };
    condition_of(&value, context)
}
```

Simplify the `("assert", [Expr::Identifier(name), ..])` arm of `eval_short_circuit`: it re-checks the message prefix; since the error now propagates it can return `None` for non-errors only — read it, delete what became dead, keep the message `` `true` has no value `` of `assert(true)`.

In `check.rs` `explains`, the `unknown-name` finding is now explained by either runtime wording:

```rust
"unknown-unit" | "unknown-name" => between(finding_message, '\'').is_some_and(|name| {
    runtime_message.contains(&format!("Unknown unit '{name}'")) || runtime_message.contains(&format!("`{name}` has no value"))
}),
```

Update the module doc comments that say a name "evaluates to its own text" (`check.rs` top, `helpers.rs` `KnownCondition::Unbound`, `interpreter/expressions.rs` doc of `eval_identifier`).

- [ ] **Step 4: Run the new tests, expect PASS; then run everything and sort the fallout**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script -p physure-cli -p physure-lsp --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked|failures:" `

Expected failures and what to do with each class (read each, never loosen blindly):
  * tests that expected the old text of an unbound name (`Unknown unit 'foo'`, `cannot convert a String`, the two special `has no value` forms): rewrite to the general message, keeping the Bool hint for conditions;
  * `a_flagged_line_is_not_said_twice` / `a_second_problem_on_a_flagged_line_is_reported_too` in `tests/report.rs`: if a program is now said twice, `explains` needs the new wording for that code (fix `explains`, not the test); if a second problem became hidden, tighten;
  * `tests/docs_programs_need_no_unbound_names.rs` (Task 1): a documented program that worked by chance is a bug in that documentation: fix the program (define the name, or quote the text), never the test;
  * `interpreting_a_partial_document_never_panics`: must stay green (it checks no panic); if it asserts an outcome, the expectation becomes `Err`;
  * any program in a test that used an unbound name as text on purpose: this plan's spec says there are none; report the one you find instead of keeping the fallback.

- [ ] **Step 5: Mutation checks**

(a) make the last branch of `eval_identifier` return the text again: `tests/unbound_names.rs` must fail; (b) remove the `explains` change: the report guard must fail. Revert both.

- [ ] **Step 6: Commit** (`fix(script): a name with no value is an error, not its own text (audit T5)`)

---

### Task 4: The checker and the interpreter agree

**Files:**
* Test: `physure-script/tests/unbound_names.rs` (append) or `tests/check.rs`

- [ ] **Step 1: Write the agreement test**

For a list of programs where the checker reports `unknown-name` (take them from `tests/check.rs` / `tests/check_corpus.rs`, grep `unknown-name`), assert the interpreter refuses each with a message naming the name; and for a list of programs the interpreter accepts that contain names with values (a variable defined later in a function body, a `for` variable, a parameter, `pi`, a unit used as a name), assert `check_source` reports no `unknown-name`. This is the standing invariant: the checker may miss a program, it never flags one the interpreter accepts.

- [ ] **Step 2: Run; if the checker flags an accepted program, fix the checker** (`physure-script/src/check.rs`, the rule around the `unknown-name` code), never the test.

- [ ] **Step 3: Commit** (`test(script): the checker and the interpreter agree on a name with no value (audit T5)`)

---

### Task 5: Every consumer

**Files:** none unless a failure points at one.

- [ ] **Step 1:** Rust: `cargo test -p physure-script -p physure-cli -p physure-lsp --no-fail-fast` and `cargo test -p physure --no-fail-fast`; report both totals.
- [ ] **Step 2:** Python: rebuild the extension in this worktree (command in the conventions), verify `_core.__file__`, run `uv run --no-sync pytest -q`; grep `physure-python/tests` and `physure-python/physure` for PHS programs that use a bare word as text (`grep -rn "evaluate\|run_phs" physure-python/tests | head`) and read any hit.
- [ ] **Step 3:** WASM: `cd physure-wasm && wasm-pack test --node` and the golden JS test (`node --test tests/js/generated.test.mjs` after `wasm-pack build --target nodejs --out-dir pkg-node`); Java: `cargo build --release -p physure-java` then `cd physure-java && mvn -B test -Dphysure.native.dir=D:/Projects/physure/target/release`. Report totals; a failure from a program with a bare word is fixed in that test (define the name), a failure that is not about names is reported.
- [ ] **Step 4:** Code generators: for each target (`js`, `ts`, `rust`, `java`, `python`) generate a one-line program with an unbound name (`phs transpile`) and record in the report what each does (JS: `ReferenceError` at run time; others: compile error / refused / emitted as is). If one of them silently accepts it, add ONE open line to `docs/ROADMAP.md` section 3 per such target; do not fix here.
- [ ] **Step 5:** The language server: a document with a half-typed word gives an error diagnostic; confirm with the `physure-lsp` tests (already in Step 1) and report.

---

### Task 6: Documentation

**Files:** `CHANGELOG.md`, `docs/ROADMAP.md` (lines about T5: "An unbound name evaluates to a string of itself" and "An unbound name is a `ReferenceError` in the generated JavaScript"), `ROADMAP.md` if it mentions it, `docs/conf_coverage.md` line about T5, `docs/superpowers/audits/2026-11-14-code-resident-numbers.md` (the T5 row and the line "T5 ... language semantics, for the owner"), `PHYSURE_LANGUAGE_GUIDE.md` and `docs/` pages that say a name with no value is its own text (grep `its own text`, `own name`, `unbound`), `physure-cli/src/main.rs` guide text.

- [ ] **Step 1:** CHANGELOG `[Unreleased]`: a **Changed** bullet: an unbound name is now the error `` `x` has no value `` instead of its own text; the format words (`: base`, `: .2f`, `: frac`) are read as text and are no longer affected by a variable of the same name; `check::explains` knows the new wording. List the commits of Tasks 1-4 by hash. State the breaking change plainly: a program that used a free word as text now fails (quote it).
- [ ] **Step 2:** ROADMAP: tick the T5 items; close the JavaScript line (the interpreter now agrees); in the fixed-list sentence add T5. Audit row T5: mark resolved with the commit hashes (same style as the T11 row). `docs/conf_coverage.md`: update or remove the T5 row (read it first: it says "not frozen"; keep the row's meaning true).
- [ ] **Step 3:** The guide: run `phs doc --save` if the guide text derives from `physure-cli/src/main.rs` and you changed it; the sync test in `physure-cli` must stay green. Grep the docs for statements that an unbound name is text and fix each.
- [ ] **Step 4:** `cd physure-python && uv run --no-sync mkdocs build --strict` (the 3 `Div ... unclosed` warnings are old); never link source files with relative paths.
- [ ] **Step 5:** Commit (`docs: the changelog, the roadmap and the audit record T5`).
