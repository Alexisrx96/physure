# Reserved names: words a program can never rebind — design

**Status:** design approved section by section by the user on 2026-11-16; this document is the written spec awaiting their review.
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
| Constant | `pi`, `π` and the `[Constants]` of the catalog in force | exists in `globals.rs`; moves under the same function and the same message shape |

The function names are **not typed twice**. `reserved.rs` takes them from the builtin tables, and a drift test fails when a builtin is added or removed without the table following. The exact list is whatever those tables contain on the day of the plan; the table above names the classes.

A module function is reserved by what is imported: `use * from calc` reserves every name of `calc`, `use solve from calc` reserves `solve` only, and an alias (`use solve as sv from calc`) reserves `sv` and leaves `solve` free.

## 4. The function

```rust
pub enum Reserved { Keyword, StatementWord, Literal, CoreFunction, ModuleFunction(&'static str), Constant }
pub fn kind(name: &str, imported: &Imported) -> Option<Reserved>
```

`Imported` is the set of module names a program has brought in. `kind` is pure; the catalog in force is read only for the constants, as `globals.rs` does today.

## 5. Where it is enforced

1. **Interpreter.** `refuse_binding` and `refuse_global_bindings` in `globals.rs` already check an assignment, a function name, a parameter, a `for` variable, a `where` binding and an imported name, once per statement and before it runs. They call `reserved::kind` instead of the constants test alone. The guards `!env.contains_key("assert")` and `!env.contains_key(name)` of `eval_short_circuit` can no longer be false once those names cannot be bound, and are deleted.
2. **Importing over a variable.** `use * from calc` when the program already holds a variable named `limit` is an error that names the variable. A name bound before the import and a name bound after it are both refused, so a reserved name never has two meanings in one program.
3. **`phs check`.** The same check before the program runs, in `refuse_global_bindings` of `check.rs`. The classes that are new report the code `reserved-name`; a constant keeps `constant-shadowing`.
4. **Symbolic-string parser** (`physure-script/src/symbolic/parser.rs`). A reserved name without parentheses is not a symbol: `and`, `or`, `not`, `true`, `inf`, `sin` as a bare word are an error that names the class. With parentheses a function name is a call, as now. This closes the `and * x * y` finding.
5. **Hosts.** A value a host gives a program goes through `PhsInterpreter::run_program_with`, which already calls `refuse_binding` ("a value given by the host"); the Python, WebAssembly and Java bindings get the new check from that call and surface its error as their own. Phase 3 tests it through each binding.

## 6. Messages

The error names the class and the way out, in the shape of today's `shadowing_error`:

> `max` is a function of PHS and cannot be bound as a variable; choose another name
> `use` is a word of the language (it starts an import) and cannot be bound as a parameter of `f`; choose another name
> `limit` is a function of `calc`, which this program imports, and cannot be bound as a variable; choose another name

A grammar keyword keeps the parse error it has today (it never reaches the check); improving that message is out of scope.

## 7. Stability policy

Adding a name to a reserved class is a breaking change: it goes in the changelog under "What to change". New functions are placed in a module before the core, so that a program that does not import the module is not affected. The list is documented (§8), and a name leaves a class only by a changelog entry.

## 8. Documentation

`PHYSURE_LANGUAGE_GUIDE.md` gets a "Reserved names" section generated from the table, and a test fails when the section and the table differ. The CHANGELOG entry of the release lists what to change for a program that bound a name that is now reserved.

## 9. Phases

1. **The table and the interpreter.** `reserved.rs`, the drift test, the binding check in `globals.rs`, the import-over-a-variable error, the deletion of the `assert` special case, the table-driven test. Measure first how many tests and examples of the repository bind a name that becomes reserved (`max = ...`, `fn round`, a redefined `assert`) and migrate them.
2. **The checker and the symbolic parser.** The `phs check` diagnostic and the refusal of a reserved bare word in a symbolic text.
3. **Hosts and documentation.** The check in the Python, WebAssembly and Java binding of a name, the generated section of the guide and its test, the changelog entry.

## 10. Testing

* A table-driven test: every reserved name, in every form that binds (assignment, function name, parameter, `for`, `where`, import alias, host), is an error with the class named; `m`, `s`, `kg`, `e`, `base`, `x` still bind.
* The drift test of §3, in both directions.
* The module rule: `limit = 1` is accepted before an import of `calc` that does not bring it, refused after `use * from calc`, accepted after `use solve from calc`.
* The symbolic texts of §5.4, and a unit test that `deriv("sin(x)", "x")` still reads `sin` as a call.
* `phs check` and the interpreter agree on every case of the table (the pattern of `tests/zero_and_conditions.rs`).

## 11. Risks and what is deliberately left alone

* **Programs that break.** A program that defines `fn max(...)` (which never ran: the builtin answered every call) or binds a variable `min` or `round` is an error where it used to run. That is the point of the change, and the changelog says it.
* **The weakest part of the case.** For a variable, a statement word (`use = 3` works today) and a lower-case literal (`true = 3` works today), the harm is confusion and a clearer parse of partial documents, not a wrong answer. They are reserved because the user asked for words that can never be rebound, and a class can be dropped from §3 without touching the rest.
* **Unit names are not reserved.** `m = 5 kg` and `t = 3 s` stay valid. The unit lookup still prefers a variable to a unit, as `globals.rs` documents.
* **Not in this spec:** reserving the words of a format, the improvement of the parse error of a keyword, rename and completion in the language server, and the unbound-name rule of audit T5 (a separate decision, measured on 2026-11-16: 435 fallbacks in 38 tests, most of them format words and partial documents).
