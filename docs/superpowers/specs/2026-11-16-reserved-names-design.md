# Reserved names: words a program can never rebind — design

**Status:** design approved section by section by the user on 2026-11-16. Phases 1, 2 and 3 are implemented (plans `2026-11-16-reserved-names-phase-1.md`, `-phase-2.md` and `2026-11-17-reserved-names-phase-3.md`).
**Scope of this spec:** the set of reserved names, one function that decides, and where it is enforced (interpreter, static checker, symbolic-string parser, hosts). Each phase of §9 gets its own implementation plan.

## 1. Why

Today PHS reserves two things: the keywords of the grammar (`for while let if then else where return not and or True False`, rule `keyword` of `physure-script/src/phs.pest`) and `pi`, `π` and the 411 constants of the catalog (`physure-script/src/globals.rs`: "shadowing is an error"). Everything else can be rebound without a word, and the November 2026 audit work kept finding the same family of silent mistakes:

| Finding | What happened |
|---|---|
| Probe of 2026-11-16 | A program may assign `sin = 3`, `deriv = 1`, `assert = 2`, `max`, `format`, `print`, `use`, `from`, `fn`, `export`, `import`, `as`, `in`, `true`, `false`, `None`, `inf`, `oo`. Each is accepted. For a variable the call still reaches the builtin (`sin = 3` then `sin(0)` is `0.0`, and an `assert(1 m == 2 m)` after `assert = 2` still fails), so the harm of a variable is confusion (`sin` is `3.0` and a function at once), not a wrong call. The silent cases are the two rows below. |
| T2 follow-up | The symbolic parser reads `and`, `or` and `not` as symbols and multiplies them: `simplify("x and y")` is `and * x * y`. |
| T5 | `true` (lower case) is the text `"true"`, so `while true` is an error only because the text is not a Bool. |
| A function the call never reaches | `call_by_name` (`interpreter/expressions.rs`) asks the core builtins first, then the unlocked module functions, then the program's functions. So `fn max(a, b) = 100` is accepted and `max(1, 2)` is `2.0`: the program's function is never called. After `use * from calc`, `fn limit(a) = 7` is accepted and `limit(1)` runs the builtin and fails with its own arity error. |
| A value the name never reaches | `inf = 3` is accepted and `inf` still reads as infinity, because the infinity literal wins over the binding. |
| T13 | `e` meant Euler's number in one path and nothing in the other, and a name ending in `e` was cut in two. |

The principle is the one `globals.rs` already states for `pi`: a scientist who writes `pi = 3` has made a mistake, not a choice. This spec extends it to the words of the language itself, and gives the symbolic-string parser the same list.

## 2. What the user decided

* **Reserved, never rebindable:** the builtin functions, the statement words and the literals.
* **Not reserved:** unit names (`m`, `s`, `kg` are the variables of physics), the words of a format (`base`, `frac`), `e`, single letters.
* **Scope of the builtin functions:** the functions of the core are reserved always; the functions of a module (`calc`, `array`, `plot`) are reserved while the module is imported.

## 3. The reserved set

One table, in `physure-script/src/reserved.rs`, is the source of truth.

| Class | Names | State |
|---|---|---|
| Keyword | `for while let if then else where return not and or True False` | exists; stays in the grammar |
| Statement word | `use from import export as fn in` | new. The words the grammar uses (`import_stmt`, `export_stmt`, `function_def`, `for_expr`). `def` is not a word of PHS and is not reserved |
| Literal | `true false None none inf infinity oo` | new. `inf`, `infinity` and `oo` are the infinity literals of the `number` rule; `None` is a value kind; the lower-case `true`, `false` and `none` are reserved so that they cannot be confused with `True`, `False` and `None`. `NaN` is not a literal of the language and is not reserved |
| Core function | every name `eval_core_builtin` answers (`abs approx assert ceil cos exact_assert exp floor format ln log max min parallel_map round sin sqrt tan uncertainty vector dof coverage_factor expanded from_samples ...`) | new; always |
| Module function | every name the table of a module answers (`calc`: `deriv integral limit series solve ...`; `array`: `norm det dot cross transpose ...`; `plot`: `plot plot3d ...`) | new; while imported |
| Constant | `pi`, `π` and the `[Constants]` of the catalog in force | stays in `globals.rs` (`refuse_constant_binding`), called from `refuse_binding` after the static classes |

The core function names are typed once, in `reserved.rs`, and a drift test reads the arms of `eval_core_builtin` and fails when a builtin is added or removed without the table following. The module functions are not typed at all: they come from `domain_members` through what the interpreter has unlocked.

A module function is reserved by what is imported: `use * from calc` reserves every name of `calc`, `use solve from calc` reserves `solve` only, and an alias (`use solve as sv from calc`) reserves `sv` and leaves `solve` free.

## 4. The function

```rust
pub enum Reserved { Keyword, StatementWord, Literal, CoreFunction }
pub fn static_kind(name: &str) -> Option<Reserved>
```

`static_kind` answers for the keywords, the statement words, the literals and the core functions, and is pure. The module functions depend on the program: the interpreter's `module_domain(name) -> Option<&'static str>` answers the module whose function the program imported under that name. The constants are checked by `globals.rs` (`refuse_constant_binding`), against the catalog in force.

## 5. Where it is enforced

1. **Interpreter.** `refuse_binding` and `refuse_global_bindings` in `globals.rs` already check an assignment, a function name, a parameter, a `for` variable, a `where` binding and an imported name, once per statement and before it runs. `refuse_binding` calls `reserved::static_kind`, and `refuse_global_bindings` also asks the interpreter's `module_domain`. The guards `!env.contains_key("assert")` and `!env.contains_key(name)` of `eval_short_circuit` can no longer be false once those names cannot be bound, and are deleted. The variables of an equation call (`eq(sigma=5, A=2)`) are local to the call and are refused only when they are a keyword, a literal, `pi` or a catalog constant. A value a host gives is treated the same way.
2. **Importing over a variable.** `use * from calc` when the program already holds a variable named `limit` is an error that names the variable. A name bound before the import and a name bound after it are both refused, so a reserved name never has two meanings in one program. Only the names the program holds when the import runs (variables and functions) are looked at: a parameter or a local of a function defined before the import is local to that function and never shadows a call, so it is left alone. A name held by the program's host (a function it registered) or by a plugin counts as held.
3. **`phs check`.** The pre-pass of `check.rs` (`refuse_reserved_bindings`) reads the top-level statements in order, keeping what the imports have unlocked and the names the program holds (assignments and functions; a loop body is left out because it may never run), and reports with the code `reserved-name` a reserved word, literal or core function bound in any form, a module function bound after its import, an import over a held name and a remap. A constant keeps `constant-shadowing`. The decision about an import is `reserved::plan_import`, the function the interpreter calls too. The checker may miss what the interpreter refuses (an import nested in a function or a loop body, a name a `.phs` import binds, an assignment inside a loop), and it does not report what the interpreter accepts.
4. **Symbolic-string parser** (`symbolic/parser.rs`). A word PHS reads as something other than a name is not a bare symbol: the keywords (`and`, `or`, `not`, `if`...), the literals (`true`, `inf`...) and the functions the parser itself knows (`sin`, `log`, `deriv`, `integral`...) are an error that names the class; with parentheses a function is a call, as before. The other core-function names (`sigma`, `max`, `min`, `dof`...) and the statement words stay symbols: they are plausible physical symbols, and the variables of an equation call with those names are allowed too. This narrows the first draft of this item, which listed every reserved name. `sec` also stays a symbol: it is the unit second.
5. **Hosts.** A value a host gives a program (`run_program_with`) and an argument of an equation call are local to the run and never hide a call, so `refuse_local_binding` refuses only a keyword, a literal, `pi` and the catalog's constants; a name like `sigma` or `max` is accepted. A function a host registers (`register_fn`, which returns a `Result` now) or a plugin installs is refused when its name is reserved (`refuse_function_name`: the static classes and the functions of the imported modules), because a call would reach the builtin first. The Python binding raises `ValueError`.

## 6. Messages

The error names the class and the way out, in the shape of today's `shadowing_error`:

> `max` is a function of PHS and cannot be bound as a variable; choose another name
> `use` is a word of the language and cannot be bound as a parameter of `f`; choose another name
> `limit` is a function of `calc`, which this program imports, and cannot be bound as a variable; choose another name

A grammar keyword keeps the parse error it has today (it never reaches the check); improving that message is out of scope.

## 7. Stability policy

Adding a name to a reserved class is a breaking change: it goes in the changelog under "What to change". New functions are placed in a module before the core, so that a program that does not import the module is not affected. The list is documented (§8), and a name leaves a class only by a changelog entry.

## 8. Documentation

`PHYSURE_LANGUAGE_GUIDE.md` gets a "Reserved names" section generated from the table, and a test fails when the section and the table differ. The CHANGELOG entry of the release lists what to change for a program that bound a name that is now reserved.

## 9. Phases

1. **The table and the interpreter (done).** `reserved.rs`, the drift test, the binding check in `globals.rs`, the import-over-a-variable error, the deletion of the `assert` special case, the table-driven test. Measure first how many tests and examples of the repository bind a name that becomes reserved (`max = ...`, `fn round`, a redefined `assert`) and migrate them.
2. **The checker and the symbolic parser (done).** The `phs check` diagnostic and the refusal of a reserved bare word in a symbolic text.
3. **Hosts and documentation (done).** The host-registration check (Python `register_function`; WebAssembly and Java have no registration API), the values a host gives (Java `PhyEquation.callWith` goes through `run_program_with`), the generated section of the guide and its test, the changelog entry.

## 10. Testing

* A table-driven test: every reserved name, in every form that binds (assignment, function name, parameter, `for`, `where`, import alias), is an error with the class named; `m`, `s`, `kg`, `e`, `base`, `x` still bind. A value a host gives and an equation-call argument are refused only for a keyword, a literal, `pi` and a constant; a function a host registers is refused for any reserved name.
* The drift test of §3, in both directions.
* The module rule: `limit = 1` is accepted before an import of `calc` that does not bring it, refused after `use * from calc`, accepted after `use solve from calc`.
* The symbolic texts of §5.4, and a unit test that `deriv("sin(x)", "x")` still reads `sin` as a call.
* `phs check` and the interpreter agree on every case of the table (the pattern of `tests/zero_and_conditions.rs`).

## 11. Risks and what is deliberately left alone

* **Programs that break.** A program that defines `fn max(...)` (which never ran: the builtin answered every call) or binds a variable `min` or `round` is an error where it used to run. That is the point of the change, and the changelog says it.
* **The weakest part of the case.** For a variable, a statement word (`use = 3` works today) and a lower-case literal (`true = 3` works today), the harm is confusion and a clearer parse of partial documents, not a wrong answer. They are reserved because the user asked for words that can never be rebound, and a class can be dropped from §3 without touching the rest.
* **`in` is also the inch symbol.** A variable cannot be named `in`; a quantity written with the unit (`2 in`) is not a binding and is unaffected.
* **Unit names are not reserved.** `m = 5 kg` and `t = 3 s` stay valid. The unit lookup still prefers a variable to a unit, as `globals.rs` documents.
* **The language server replays the imports.** When the old or the new text of a document has a `use`, `apply_change` forgets what was unlocked (`PhsInterpreter::reset_imports`) and runs the imports of the new text again, in order, interleaved with the changed statements, as a fresh run would.
* **Inside a module body the module functions are not reserved.** What `use` unlocks belongs to the whole interpreter, so a `.phs` module that is imported would be refused for binding `limit` just because its importer ran `use * from calc`. The module-function check is therefore off while a module body runs (`module_domain` returns nothing while a module loads); the static classes still apply there. The cost: a module that imports `calc` itself and then binds `limit` is accepted. Still open: it needs what is unlocked kept per module scope.
* **Not in this spec:** reserving the words of a format, the improvement of the parse error of a keyword, rename and completion in the language server, and the unbound-name rule of audit T5 (a separate decision, measured on 2026-11-16: 435 fallbacks in 38 tests, most of them format words and partial documents).
* **A host value can make an import fail.** A host value named `limit` plus a program `use * from calc` is refused as "already bound in the program or by its host": the name was bound by the host, not by the program. It is loud, and the Java equation texts rarely contain `use`.
* **A plugin `use` inside an imported `.phs` module skips the module-function part of the check.** `module_domain` returns nothing while a module loads, so `refuse_function_name` only applies the static classes there.
* **The binding suites were run on one machine only.** On 2026-11-17 the Python suite (1761 passed), the Java suite (226 on JDK 21, with the native library built from the branch in release mode, as CI does) and the WebAssembly suite (93 under Node 24, `wasm-pack test --node`) all passed; CI runs Java 8 to 25 on five platforms. The Java route (`PhyEquation.callWith`) with a variable named `sigma` has its own test (`FunctionStrictTest`) as well as the Rust-level test of `run_program_with`. An `ext/*.py` function named like a core function now fails the Python loader (`phs_loader.load_ext_functions`) with the `ValueError` of `register_function`.
* **The variables of `integral` and `limit` get only the static classes.** `integral("x", "limit", 0, 1)` after `use * from calc` is allowed: that variable is local.
* **Two wildcard imports that share a name** (`use * from calc` then `use * from array` share `gradient`, `sym_det`, `sym_trace`, `sym_charpoly`, `sym_eigenvalues`, `sym_transpose`) keep the older behaviour: the later import wins. Refusing it would break programs that import both. Still open.
* **The checker finds the imports among the bindings by the text of their description** (`what.starts_with(IMPORT_BINDING)` in `globals.rs` and `check.rs`); an enum for what binds a name is still to do.
* **The language server's `env` can hold a name a later line binds.** Open `use solve from calc
limit = 5`, then change line 1 to `use * from calc`: `limit = 5` stays clean and in `env`, so the import is refused on line 1 with "already bound" while a fresh run reports line 2.
* **Statements that call a module function do not re-run when the `use` narrows.** A wildcard import writes no name in the dependency analysis, so `r = limit(...)` after `use * from calc`, edited to `use solve from calc`, keeps its old value and gets no diagnostic.
* **A PHS function that uses `inf` and is differentiated with `f'`** (`fn f(x) = x * inf`, then `f'(2)`) now errors: its derivative is computed as symbolic text and `inf` is refused there.
