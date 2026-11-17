# Reserved names, phase 3: hosts, plugins and the language guide — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A value a host gives or an equation call names is local and may be named like a function (`sigma`); a host or a plugin cannot install a function under a reserved name; the language guide lists the reserved names from the table and a test keeps the committed guide in sync; the Python binding is built and its suite run against all of it.

**Architecture:** One new rule, "local bindings", in `globals.rs` (`refuse_local_binding`): a name bound for one run only (a value a host gives, an argument of an equation call) never shadows a call, so only what the language reads as something else (keyword, literal) and `pi`/the catalog constants are refused. A new `PhsInterpreter::refuse_function_name` (static classes + the functions of imported modules) guards `register_fn` and `install_dynamic_externals`. The guide section is built in `generate_language_docs_md()` from the same tables and compared with the committed file by a CLI test.

**Tech Stack:** Rust (`physure-script`, `physure-cli`, `physure-python`), PyO3/maturin, pytest.

**Spec:** `docs/superpowers/specs/2026-11-16-reserved-names-design.md` (§5.5 hosts, §8 documentation, §9 phase 3, §11 the gaps of the Java route and the plugins). Phases 1 and 2 are done (plans `2026-11-16-reserved-names-phase-1.md` and `-phase-2.md`).

**Working notes (read once):**
- Work on the branch `docs/post-release-0.4.0`, in the worktree `.claude/worktrees/post-release-040`. Do not touch `main`. Do not push.
- Prefix every cargo command with `CARGO_TARGET_DIR=D:/Projects/physure/target` (shared target, never a second `target/`).
- Rust, `CHANGELOG.md` and `docs/*.md` files are CRLF in the working tree: edit with the editor tool, or Python in default text mode chained with `&&` before `git commit`. `PHYSURE_LANGUAGE_GUIDE.md` is written by `phs doc --save` (LF); do not hand-edit it.
- A change of criterion, decided while planning: phase 1 refused a value a host gives when it was named like a core function, and the final review of phase 1 found that this breaks the Java binding (`PhyEquation.callWith` binds the measured variables of an equation by name, so `F = sigma * A` with a measurement for `sigma` fails). A local never shadows a call (a call reaches the builtin first), so Task 1 narrows the rule for host values and equation-call arguments to what the language reads as something else. Task 5 writes it into the spec and the changelog.
- Known and accepted when this plan was written: the Java and WASM suites could not be run here (no JNI build, no `wasm-pack`); only a Rust-level test covers the Java route. (That was wrong: JDK 21, Maven, Node 24 and `wasm-pack` were installed, and both suites were run on 2026-11-17; see spec §11.)

---

### Task 1: A value a host gives, and an equation-call argument, is a local

**Files:**
- Modify: `physure-script/src/globals.rs` (new `refuse_local_binding`)
- Modify: `physure-script/src/interpreter/statements.rs` (`run_program_with`)
- Modify: `physure-script/src/interpreter/expressions.rs` (the equation-call argument check, one call)
- Modify: `physure-script/tests/reserved_names.rs` (replace two phase-1 tests, add one)

- [ ] **Step 1: Write the failing test and replace the two obsolete ones**

In `physure-script/tests/reserved_names.rs`, DELETE the tests `a_value_a_host_gives_cannot_be_a_reserved_name` and `a_value_a_host_gives_cannot_be_a_function_of_an_imported_module`, and add:

```rust
#[test]
fn a_value_a_host_gives_is_local_and_may_be_named_like_a_function() {
    // a measurement of an equation called from Java is bound by the name of its variable: `sigma`, `max`, `dof`...
    let program = parse_phs("sigma * 2").expect("a program");
    let value = PhsInterpreter::default()
        .run_program_with(&program, vec![("sigma".to_string(), PhsValue::Number(5.0))])
        .expect("sigma is local to the run");
    assert_eq!(value.to_string(), "10.0");

    // also after the module that has a function of that name is imported
    let interpreter = PhsInterpreter::default();
    interpreter.run_program_with(&parse_phs("use * from calc\n1").expect("a program"), vec![]).expect("the import runs");
    let value = interpreter
        .run_program_with(&parse_phs("limit + 1").expect("a program"), vec![("limit".to_string(), PhsValue::Number(1.0))])
        .expect("limit is local to the run");
    assert_eq!(value.to_string(), "2.0");
}

#[test]
fn a_value_a_host_gives_cannot_be_a_word_the_language_reads_or_a_constant() {
    for name in ["inf", "True", "pi"] {
        let error = PhsInterpreter::default()
            .run_program_with(&parse_phs("1").expect("a program"), vec![(name.to_string(), PhsValue::Number(1.0))])
            .unwrap_err()
            .to_string();
        assert!(error.contains(&format!("`{name}` is")) && error.contains("cannot be bound as a value given by the host"), "{name}: {error}");
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test reserved_names a_value_a_host`
Expected: `a_value_a_host_gives_is_local_and_may_be_named_like_a_function` FAILS (`sigma` is refused as a function of PHS); `a_value_a_host_gives_cannot_be_a_word_the_language_reads_or_a_constant` passes.

- [ ] **Step 3: Write the rule**

In `physure-script/src/globals.rs` add, right after `refuse_constant_binding`:

```rust
/// An error when `name` cannot be bound as a LOCAL of one run: a value a host gives (`run_program_with`) or an
/// argument of an equation call (`eq(sigma=5, A=2)`). A local never shadows a call (a call reaches the builtin
/// first), so the name of a function of PHS or of a module is fine for it, and `sigma`, `max` and `dof` are physical
/// symbols. What the language reads as something else is not: a keyword or a literal would be ignored (`inf = 3`),
/// and `pi` and the constants of the catalog are refused as everywhere.
pub fn refuse_local_binding(name: &str, what: &str) -> PhysureResult<()> {
    use crate::reserved::Reserved;
    if let Some(kind @ (Reserved::Keyword | Reserved::Literal)) = crate::reserved::static_kind(name) {
        return Err(PhysureError::Generic(crate::reserved::binding_error(name, what, kind)));
    }
    refuse_constant_binding(name, what)
}
```

- [ ] **Step 4: Use it for host values and equation-call arguments**

In `physure-script/src/interpreter/statements.rs`, `run_program_with`: READ the loop at the start of the function (it calls `crate::globals::refuse_binding(name, "a value given by the host")?` and then refuses a name for which `self.module_domain(name)` is `Some`, and the comment above it). Replace that comment and loop with:

```rust
        // a value a host gives is local to the run and never shadows a call: only what the language reads as
        // something else (a keyword, a literal), `pi` and the catalog's constants are refused
        for (name, _) in &bindings {
            crate::globals::refuse_local_binding(name, "a value given by the host")?;
        }
```

In `physure-script/src/interpreter/expressions.rs`, in the equation call (the line `crate::globals::refuse_constant_binding(kwarg_name, &format!("an argument of the equation `{name}`"))?;`), change `refuse_constant_binding` to `refuse_local_binding`.

- [ ] **Step 5: Run the tests**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --no-fail-fast 2>&1 | grep -E '^test result|FAILED|panicked'`
Expected: all `ok` (`reserved_names` passes; `global_constants` still refuses `pi` and constants as host values and equation arguments).

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/globals.rs physure-script/src/interpreter/statements.rs physure-script/src/interpreter/expressions.rs physure-script/tests/reserved_names.rs
git commit -m "fix(script): a value a host gives is a local: only a keyword, a literal, pi or a constant is refused

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: A host or a plugin cannot install a function under a reserved name

**Files:**
- Modify: `physure-script/src/interpreter/statements.rs` (new `refuse_function_name`, `install_dynamic_externals`)
- Modify: `physure-script/src/interpreter/mod.rs` (`register_fn`)
- Modify: callers of `register_fn`: `physure-script/src/interpreter/tests.rs` (~line 882), `physure-script/tests/sandbox.rs` (~line 164), `physure-python/src/lib.rs` (`register_function`, ~line 1355)
- Test: `physure-script/tests/reserved_names.rs`, `physure-script/src/interpreter/tests.rs`

- [ ] **Step 1: Write the failing tests**

Append to `physure-script/tests/reserved_names.rs`:

```rust
#[test]
fn a_host_function_cannot_take_a_reserved_name() {
    let mut interpreter = PhsInterpreter::default();
    for name in ["max", "use", "inf", "pi"] {
        let error = interpreter.register_fn(name, |_| Ok(PhsValue::Number(1.0))).unwrap_err().to_string();
        assert!(error.contains(&format!("`{name}` is")) && error.contains("cannot be bound as a function a host registers"), "{name}: {error}");
    }
    interpreter.register_fn("double", |_| Ok(PhsValue::Number(2.0))).expect("a free name registers");
    // a function of a module is reserved once the program imports the module
    interpreter.run_program_with(&parse_phs("use * from calc\n1").expect("a program"), vec![]).expect("the import runs");
    let error = interpreter.register_fn("limit", |_| Ok(PhsValue::Number(1.0))).unwrap_err().to_string();
    assert!(error.contains("`limit` is a function of `calc`"), "{error}");
}
```

In `mod tests` of `physure-script/src/interpreter/tests.rs` add (use the imports that file already has; add `use std::collections::HashMap; use std::sync::Arc; use crate::ast::ImportSpecifier;` locally inside the test if they are not at the top):

```rust
    #[test]
    fn a_plugin_cannot_install_a_function_under_a_reserved_name() {
        let interp = PhsInterpreter::default();
        let mut functions: HashMap<String, ExternalFn> = HashMap::new();
        functions.insert("max".to_string(), Arc::new(|_: &[PhsValue]| Ok(PhsValue::Number(1.0))));
        functions.insert("helper".to_string(), Arc::new(|_: &[PhsValue]| Ok(PhsValue::Number(2.0))));
        let error = interp.install_dynamic_externals(&ImportSpecifier::Wildcard, "m", &functions).unwrap_err().to_string();
        assert!(error.contains("`max` is a function of PHS") && error.contains("a function of the plugin `m`"), "{error}");
        assert!(interp.dynamic_externals.lock().unwrap().is_empty(), "nothing is installed when one name is refused");
        functions.remove("max");
        interp.install_dynamic_externals(&ImportSpecifier::Wildcard, "m", &functions).expect("free names install");
        assert!(interp.dynamic_externals.lock().unwrap().contains_key("helper"));
    }
```
(`ExternalFn` is the type of the functions of `dynamic_externals`; import it from `crate::interpreter` if the file does not already. If its exact closure signature differs, adapt the closures minimally.)

- [ ] **Step 2: Run them to see them fail**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --test reserved_names a_host_function` and `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --lib a_plugin_cannot`
Expected: compile errors (`register_fn` returns `()`, `install_dynamic_externals` is private to its module): that is the red.

- [ ] **Step 3: The guard**

In `physure-script/src/interpreter/statements.rs`, right after `reset_imports`, add:

```rust
    /// Refuses `name` as the name of a function a host registers or a plugin installs: a core builtin and an
    /// imported module function answer a call first, so such a function would never be called, and `pi`, the
    /// catalog's constants and the words of the language are not function names either.
    pub(crate) fn refuse_function_name(&self, name: &str, what: &str) -> PhysureResult<()> {
        crate::globals::refuse_binding(name, what)?;
        if let Some(domain) = self.module_domain(name) {
            return Err(PhysureError::Generic(crate::reserved::module_binding_error(name, domain, what)));
        }
        Ok(())
    }
```

In the same file, make `install_dynamic_externals` `pub(crate)` and add, at its start (before it takes the `dynamic` lock), a check of every name it would install, so that nothing is installed when one is refused:

```rust
        let names: Vec<String> = match specifier {
            ImportSpecifier::Wildcard => functions.keys().cloned().collect(),
            ImportSpecifier::Symbols(syms) => syms.iter().map(|sym| sym.alias.clone().unwrap_or_else(|| sym.name.clone())).collect(),
            ImportSpecifier::ModuleAlias(_alias) => return Err(module_aliases_unsupported()),
        };
        for name in &names {
            self.refuse_function_name(name, &format!("a function of the plugin `{module}`"))?;
        }
```
(`use crate::ast::ImportSpecifier;` is already at the top of that function; put the new block after it. The existing `match` below stays as it is.)

In `physure-script/src/interpreter/mod.rs` change `register_fn`:

```rust
    /// Registers a host function under `name`, callable from PHS source like any builtin.
    /// Takes precedence over user-defined PHS functions but not over builtins. A name that is reserved (a core
    /// function, a word of the language, a function of a module the program imported, `pi` or a constant) is
    /// refused: a call would reach the builtin first and the function would never run.
    pub fn register_fn<F>(&mut self, name: impl Into<String>, f: F) -> PhysureResult<()>
    where
        F: Fn(&[PhsValue]) -> PhysureResult<PhsValue> + Send + Sync + 'static,
    {
        let name = name.into();
        self.refuse_function_name(&name, "a function a host registers")?;
        self.externals.insert(name, Arc::new(f));
        Ok(())
    }
```
(READ the current function first: keep its generic bounds exactly.)

- [ ] **Step 4: Update the callers**

- `physure-script/src/interpreter/tests.rs` (`test_register_fn_dispatch`) and `physure-script/tests/sandbox.rs` (the `probe` registration): append `.unwrap()` to the `register_fn(...)` call (`.expect("a free name")`).
- `physure-python/src/lib.rs`, `register_function`: change its return type to `PyResult<()>`, and make it return `self.inner.register_fn(name, move |args| { ... }).map_err(|e| <the exception the neighbouring methods of this file raise for a `PhysureError`>)`; look at how other `#[pymethods]` of `PyInterpreter` convert a `PhysureError` (for example `PyValueError::new_err(e.to_string())` or a helper like `to_py_err`) and use the same. Keep the closure body exactly as it is.

- [ ] **Step 5: Run the tests**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-script --no-fail-fast 2>&1 | grep -E '^test result|FAILED|panicked'` — all `ok`. Then `CARGO_TARGET_DIR=D:/Projects/physure/target cargo check -p physure-python 2>&1 | tail -3` — expect `Finished` (the Python crate compiles; it is built and tested in Task 4).

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/interpreter/statements.rs physure-script/src/interpreter/mod.rs physure-script/src/interpreter/tests.rs physure-script/tests/sandbox.rs physure-script/tests/reserved_names.rs physure-python/src/lib.rs
git commit -m "feat(script): a host or a plugin cannot install a function under a reserved name

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: The language guide lists the reserved names, and a test keeps it in sync

**Files:**
- Modify: `physure-cli/src/main.rs` (`generate_language_docs_md`)
- Modify (generated): `PHYSURE_LANGUAGE_GUIDE.md` (by `phs doc --save`, not by hand)
- Test: `physure-cli/tests/cli_commands.rs`

Today the committed `PHYSURE_LANGUAGE_GUIDE.md` is byte-for-byte what `phs doc` prints (checked 2026-11-17, after normalising CRLF).

- [ ] **Step 1: Write the failing tests**

In `physure-cli/tests/cli_commands.rs`, next to `the_language_guide_is_printed_or_saved`, add (reuse the helpers of that file: `TempDir`, `run_in`, `stdout`):

```rust
#[test]
fn the_committed_language_guide_is_what_phs_doc_prints() {
    let dir = TempDir::new("guide");
    let printed = stdout(&run_in(dir.path(), &["doc"]));
    let committed = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("PHYSURE_LANGUAGE_GUIDE.md"))
        .expect("the committed guide");
    let normal = |text: &str| text.replace("\r\n", "\n").trim().to_string();
    assert!(
        normal(&printed) == normal(&committed),
        "PHYSURE_LANGUAGE_GUIDE.md is out of date: run `phs doc --save` from the repository root and commit the file"
    );
}

#[test]
fn the_guide_lists_every_reserved_name_and_every_module_function() {
    let dir = TempDir::new("reserved");
    let printed = stdout(&run_in(dir.path(), &["doc"]));
    let section = printed.split("### 1.7 Reserved names").nth(1).expect("the section 1.7 of the guide");
    let section = section.split("\n## ").next().unwrap_or(section);
    use physure_script::reserved::{CORE_FUNCTIONS, KEYWORDS, LITERALS, STATEMENT_WORDS};
    for name in KEYWORDS.iter().chain(&STATEMENT_WORDS).chain(&LITERALS).chain(&CORE_FUNCTIONS) {
        assert!(section.contains(&format!("`{name}`")), "the guide does not list `{name}`");
    }
    for domain in ["calc", "plot", "array"] {
        for name in physure_script::builtins::domain_members(domain).expect("a module") {
            assert!(section.contains(&format!("`{name}`")), "the guide does not list `{name}` of {domain}");
        }
    }
}
```
(`physure-cli` must be able to use `physure_script`; it already depends on it. If `physure_script::reserved` or `builtins::domain_members` is not reachable from the integration tests of that crate, tell me instead of making things public.)

- [ ] **Step 2: Run them to see them fail**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-cli --test cli_commands the_guide the_committed`
Expected: `the_guide_lists_every_reserved_name_and_every_module_function` FAILS ("the section 1.7 of the guide"); `the_committed_language_guide_is_what_phs_doc_prints` passes (it is the guard for the next steps).

- [ ] **Step 3: Generate the section from the tables**

In `physure-cli/src/main.rs`, in `generate_language_docs_md`, find where the last subsection of section 1 ends (`### 1.6 Settings that change a run`) and the next `## 2.` begins; add the new subsection right after 1.6's text and before section 2. Build it from the tables, with this content (write the Rust with `md.push_str(&format!(...))`, and a small private helper `fn code_list(names: &[&str]) -> String` that joins the names as `` `a`, `b`, `c` `` in the order given, sorted and de-duplicated for the module lists):

```
### 1.7 Reserved names

A program can never bind a reserved name: not as a variable, a parameter, a function name, a `for` variable, a `where` binding or an import alias. The refusal comes before the program runs (`phs check` reports it with the code `reserved-name`) and says what the name is and what to do:

| Class | Names |
| :--- | :--- |
| Keywords | <KEYWORDS> |
| Statement words | <STATEMENT_WORDS> |
| Literals | <LITERALS> |
| Functions of the core | <CORE_FUNCTIONS> |
| Functions of `calc` | <domain_members("calc")> |
| Functions of `array` | <domain_members("array")> |
| Functions of `plot` | <domain_members("plot")> |
| Constants | `pi`, `π` and every constant of the catalog (section 1.5) |

```phs
max = 3          # error: `max` is a function of PHS and cannot be bound as a variable; choose another name
use = 3          # error: `use` is a word of the language and cannot be bound as a variable; choose another name
```

* The functions of a module are reserved from the `use` that imports them: `use * from calc` reserves all of `calc`, `use solve from calc` reserves `solve`, and `use solve as sv from calc` reserves `sv` and leaves `solve` free. Importing a function over a name the program already holds is an error too.
* Not reserved: unit names (`m`, `s`, `kg`), `e`, the words of a format (`base`, `frac`).
* A value a host gives a program, and the arguments of an equation call (`eq(sigma=5, A=2)`), are local to the run and never hide a call, so they may be named like a function (`sigma`, `max`, `dof`); a keyword, a literal, `pi` and the constants are still refused there.
* A host function or a plugin function cannot be installed under a reserved name: a call would reach the builtin first.
* In a symbolic text (`deriv`, `integral`, `solve`, ...), the keywords, the literals and the functions the symbolic parser knows (`sin`, `log`, `deriv`, `integral`, ...) are not symbols; `sigma`, `max`, `min`, `dof` and `sec` are.
```
(The names of the table cells come from `physure_script::reserved::{KEYWORDS, STATEMENT_WORDS, LITERALS, CORE_FUNCTIONS}` and `physure_script::builtins::domain_members`; do not type them. `array` is also called `matrix`; list only `array`.) At the end of the paragraph of 1.5 that says shadowing is an error, add the sentence "Section 1.7 lists the other names that can never be bound."

- [ ] **Step 4: Regenerate the committed guide**

From the repository root of the worktree: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo build -p physure-cli` then `D:/Projects/physure/target/debug/phs.exe doc --save`. This rewrites `PHYSURE_LANGUAGE_GUIDE.md`. Check `git diff --stat PHYSURE_LANGUAGE_GUIDE.md` shows only the new section and the one sentence of 1.5 (a few dozen added lines, nothing else changed).

- [ ] **Step 5: Run the tests and the docs build**

Run: `CARGO_TARGET_DIR=D:/Projects/physure/target cargo test -p physure-cli --no-fail-fast 2>&1 | grep -E '^test result|FAILED|panicked'` — all `ok` (if `conf_witness` fails with `Acceso denegado (os error 5)` it is a known Windows lock in parallel: re-run that suite alone). `cd physure-python && uv run mkdocs build --strict 2>&1 | tail -1` — `Documentation built`.

- [ ] **Step 6: Commit**

```bash
git add physure-cli/src/main.rs physure-cli/tests/cli_commands.rs PHYSURE_LANGUAGE_GUIDE.md
git commit -m "docs(cli): the language guide lists the reserved names from the table, and a test keeps it in sync

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The Python binding is built and its suite run against all of it

**Files:**
- Modify: `physure-python/tests/` (one test for `register_function`, in the existing test file for the PHS interpreter binding)

This is also the first run of the Python suite against the work of this branch (audit item T21, Python half).

- [ ] **Step 1: Find the test file and write the failing test**

Run `grep -rn "register_function" physure-python/tests physure-python/physure | head` to find where `PyInterpreter.register_function` (exposed as the PHS interpreter of the `physure` package) is tested; add to that file:

```python
def test_a_host_function_cannot_take_a_reserved_name():
    interpreter = <the way that file builds the interpreter>
    with pytest.raises(ValueError, match="cannot be bound as a function a host registers"):
        interpreter.register_function(lambda *args: 1.0, "max")
    interpreter.register_function(lambda *args: 2.0, "double")  # a free name registers
```
Adapt the exception class to the one Task 2 raises (`ValueError` or the repository's own error class: read what you wrote in `register_function`). If no test file covers `register_function`, create `physure-python/tests/ext/test_register_function.py` following the conventions of its neighbours (imports, fixtures).

- [ ] **Step 2: Build the extension in THIS worktree and see the test fail then pass**

Run from `physure-python/` of the worktree (memory: `maturin develop` can contaminate another venv, so check first): `cd physure-python && echo "VIRTUAL_ENV=$VIRTUAL_ENV"` (must be empty or this worktree's `.venv`), then `CARGO_TARGET_DIR=D:/Projects/physure/target uv run maturin develop --release 2>&1 | tail -5`, and verify the module that is imported is this worktree's: `uv run python -c "import physure._core as c; print(c.__file__)"` must print a path under `...\.claude\worktrees\post-release-040\physure-python\`. Then `uv run pytest <the file> -q` — the new test passes. (Before the Task 2 change it would fail with "no exception raised"; do not rebuild an old commit to prove it.)

- [ ] **Step 3: Run the whole Python suite**

Run `uv run pytest -q 2>&1 | tail -25` (use a long timeout; run it in the background if it may take more than 10 minutes). Record the totals. For each failure, decide whether it relates to the reserved-names/checker/symbolic work of this branch (a name now refused, a symbolic text with a word now refused, `register_function`, `phs check` output) or is independent (an environment or pre-existing problem: say how you know, for instance by reading the failing assertion). Do NOT fix unrelated failures; fix a related one only if the fix is a test or a one-line source change that the plan would obviously want, and tell me exactly what you changed.

- [ ] **Step 4: Commit**

```bash
git add physure-python/tests
git commit -m "test(python): a host function cannot take a reserved name

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
(Do not add `.venv`, `site/` or build outputs; check `git status` first.)

---

### Task 5: Spec, changelog and roadmap

**Files:**
- Modify: `docs/superpowers/specs/2026-11-16-reserved-names-design.md` (§5 item 5, §9, §11)
- Modify: `CHANGELOG.md` (`### Changed`)
- Modify: `docs/ROADMAP.md`

- [ ] **Step 1: The spec says what phase 3 did**

- §5 item 5 (hosts): replace its text with "**Hosts.** A value a host gives a program (`run_program_with`) and an argument of an equation call are local to the run and never hide a call, so `refuse_local_binding` refuses only a keyword, a literal, `pi` and the catalog's constants; a name like `sigma` or `max` is accepted. A function a host registers (`register_fn`, which returns a `Result` now) or a plugin installs is refused when its name is reserved (`refuse_function_name`: the static classes and the functions of the imported modules), because a call would reach the builtin first. The Python binding raises its error."
- §5 item 1's last sentence (about equation calls) stays; add after it: "A value a host gives is treated the same way."
- §9: mark phase 3 "(done)" like the other two phases.
- §11: DELETE the two bullets that begin `* **Hosts bind names by the same call.**` and `* **Plugins and external functions are not checked.**`; DELETE the clause about hosts in §2 or §5 that contradicts the new rule if any remains (search for "host value" / "value a host gives"). Add a bullet: "* **The Java and WebAssembly suites were not run against this work** (no JNI build, no `wasm-pack` here); the Java route (`PhyEquation.callWith`) is covered by a Rust-level test of `run_program_with` with a variable named `sigma`."
- Header: the Status line says phases 1 and 2 are implemented; change to "Phases 1, 2 and 3 are implemented (plans `...-phase-1.md`, `-phase-2.md` and `-phase-3.md`)."

- [ ] **Step 2: The changelog**

Under `## [Unreleased]`:
- In the phase-1 bullet ("**A program can no longer bind a statement word, a literal or a core function ..."), the list of refused forms ends with "an import alias or a value a host gives" and a later sentence says Phases 2 and 3: replace "or a value a host gives" with "or an import alias" (remove the host clause) and replace the last sentence ("Phase 2 (...) is in the entries below; phase 3 (hosts and the guide) is open." or whatever it says now) with "Phases 2 and 3 are in the entries below."
- In `### Changed` add as the last bullet:
```markdown
- **A value a host gives and an equation-call argument are local, and a host or a plugin cannot install a function under a reserved name** (phase 3 of the reserved names; commits `<hash of Task 1>`, `<hash of Task 2>`). `run_program_with` and `eq(sigma=5, A=2)` bind a name for one run, and a local never hides a call, so a name like `sigma`, `max` or `dof` is accepted there (it was refused for a host value in the entries above, which broke the Java `PhyEquation.callWith` for an equation with a variable named `sigma`); a keyword, a literal, `pi` and the catalog's constants are still refused. `PhsInterpreter::register_fn` returns a `Result` and refuses a reserved name, as does the import of a native plugin that exports one, because a call would reach the builtin first and the function would never run; the Python `register_function` raises the error. **What to change:** an embedder that calls `register_fn` adds `?` or `.unwrap()`; a function registered as `max` or `sin` needs another name.
- **The language guide lists the reserved names** (commit `<hash of Task 3>`). `PHYSURE_LANGUAGE_GUIDE.md` has a section 1.7 generated from the table (`phs doc`), and a test of `physure-cli` fails when the committed guide is not what `phs doc` prints.
```
Replace each `<hash of Task N>` with the 7-character hash of that task's commit (`git log --format=%h -1 --grep '<words of its subject>'`).

- [ ] **Step 3: The roadmap**

In `docs/ROADMAP.md`, in the line `- [ ] Reserved names that no program can bind`: change `- [ ]` to `- [x]`, and replace its text after the design link with: "Done in three phases: the table and the interpreter, `phs check`, the symbolic parser and the language server, and the hosts, the plugins and the language guide (section 1.7). Known gaps recorded in the spec §11: two wildcard imports with a shared name, what is unlocked per module scope, the language server's stale `env` and narrowed imports, the enum for what binds a name." (Read the line first and keep its link.)

- [ ] **Step 4: Build the docs and commit**

Run `cd physure-python && uv run mkdocs build --strict 2>&1 | tail -3` (`Documentation built`, no WARNING about links). Line endings: `python -c "b=open(p,'rb').read(); print(b.count(b'\n')-b.count(b'\r\n'))"` prints 0 for `CHANGELOG.md`, `docs/ROADMAP.md` and the spec.

```bash
git add CHANGELOG.md docs/ROADMAP.md docs/superpowers/specs/2026-11-16-reserved-names-design.md
git commit -m "docs: the spec, the changelog and the roadmap record phase 3 of the reserved names

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-review (against the spec)

- **§5.5 hosts:** Task 1 (host values, equation arguments), Task 2 (`register_fn`, plugins, the Python error), Task 4 (the Python binding built and tested). Java: a Rust-level test (Task 1) because the Java suite cannot run here; recorded in the spec by Task 5.
- **§8 documentation:** Task 3 (section 1.7 generated from the table, the sync test, the 1.5 pointer) and Task 5 (changelog).
- **§11 gaps closed:** the Java equation route (Task 1), plugins and external functions (Task 2). Gaps that stay: two wildcard imports with a shared name, unlocked names per module scope, the language server's stale `env`, the enum for what binds a name, the Java/WASM suites.
- **Names across tasks:** `globals::refuse_local_binding` (Task 1; used by `run_program_with` and the equation call), `PhsInterpreter::refuse_function_name` (Task 2; used by `register_fn` and `install_dynamic_externals`), `register_fn(...) -> PhysureResult<()>` (Task 2; callers updated in the same task).
