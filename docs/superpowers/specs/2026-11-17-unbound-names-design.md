# An unbound name is an error (audit T5)

Date: 2026-11-17. Branch: `docs/post-release-0.4.0`. Decision of the owner on 2026-11-17: **an error everywhere**.

## 1. The problem

`eval_identifier` (`physure-script/src/interpreter/expressions.rs`) resolves a bare name in this order: a binding in `env`, `pi`/`π` and the catalog constants, a unit symbol, and then, as the last branch, **the name as text** (`foo` is the string `"foo"`). A typo or a missing definition therefore becomes a value that travels until some later operation refuses it, far from the cause. Two special messages already exist to patch the two places where this is most visible (`=>` and a condition: "`x` has no value ...").

The generated JavaScript already throws `ReferenceError` for the same name, so the interpreter and its own transpiler disagree (ROADMAP, T5).

## 2. Measurement (2026-11-17)

A temporary trace on the last branch, run over the suites of `physure-script`, `physure-cli` and `physure-lsp` (the trace was reverted; nothing of it is committed): 362 fallbacks.

| Source | Names | What it is |
|---|---|---|
| format words | `base`, `frac`, `ifrac`, `.2f`, `.3f`, `.1e`, `.2e` (about 60) | the only fallback a working program relies on |
| partial documents (`interpreting_a_partial_document_never_panics`) | `x`, `a`, `b`, `of`, `use`, `wher`, `sol`, ... | tests that nothing panics on half-typed text |
| lower-case `true` / `false` | 29 | tests that a condition needs a `Bool` |
| deliberate error cases | `fooz`, `zz_unit`, `KPa`, `Km`, `Kg`, `foobar` | the program is meant to be refused |
| programs the checker flags | `x`, `v`, `a`, `fur`, `r` | `no_program_the_interpreter_runs_gets_an_error` and `the_checker_finds_most_of_...` |

No test and no documented program uses an unbound name as a text value on purpose. The symbolic functions take quoted strings (`deriv("x^3", "x")`).

## 3. The rule

1. A name with no binding, no `pi`/constant, and no unit symbol is an error: `` `foo` has no value ``. The last branch of `eval_identifier` becomes that error.
2. The two patches that detect "a name that evaluated to its own text" (`eval_binary_expr` for `=>`, `eval_condition`) are deleted: the general error replaces them. A condition still requires a `Bool` (`while true` says that `true` has no value, and that the Bool literals are `True` and `False`: keep that wording, it is the useful one).
3. **Format words are not names.** `base`, `frac`, `ifrac` and `.Nf`/`.Ne`/`.Ng` are read from the syntax of the format spec, not evaluated. `apply_format_spec` already receives the spec as text. The first task of the plan finds the exact path by which these words reach `eval_identifier` today and cuts it there.
4. Text interpolation keeps its rule: a `{...}` span that does not evaluate stays as the raw placeholder. An unbound name inside a span is now such a failure, so `"{F_e}"` prints `{F_e}` as it does today for other failures. No exception is added.
5. Unchanged: quoted strings are text; symbolic functions take strings; `unit_symbol_as_quantity` (a free unit symbol is the unit, which makes a unit-bearing equation string survive its round trip); `env` bindings win over everything.

## 4. Agreement with the other tools

* **Checker** (`check.rs`, code for "neither a variable nor a unit"): unchanged in what it reports. The invariant stays: the checker may miss a program, it never flags one the interpreter accepts. Because the interpreter now refuses strictly more, the checker's job gets easier, not harder; the plan adds a test that every program the checker flags with that code is also refused by the interpreter.
* **JavaScript/TypeScript codegen**: already `ReferenceError`. The ROADMAP line is closed. Rust and Java codegen are checked for the same agreement (an unbound name must not compile, or must be refused at generation).
* **Language server**: uses the checker; a half-typed word is an error diagnostic instead of text. `interpreting_a_partial_document_never_panics` keeps its meaning (no panic) and only its expected outcome is `Err`.
* **Bindings** (Python, WASM, Java) and `phs eval`: run the interpreter; they get the error with no code change. Their tests are searched for programs that relied on the fallback.

## 5. Tests

* New: an unbound name in each position (statement, operand, call argument, condition, `=>` target value, range, `for` iterable) is the `has no value` error; a typo in a variable name names the typo.
* New: every format word still formats exactly as before (the existing format tests stay unchanged and green).
* Rewritten: the 29 lower-case `true`/`false` tests and the two special messages, to the general message with the same Bool hint.
* Measurement as a test of the claim "no document relies on the fallback": run every `.phs` file and every ```` ```phs ```` block of `docs/` and of the guide before and after; the set of programs that change from success to error must be empty, or each one is read and fixed (a tutorial that worked by chance is a bug in the tutorial).
* The suites of the three crates, `physure` core, WASM and Java parity tests, and the Python suite stay green.

## 6. Risks

* A tutorial or user program that works today by accident with a free name. Mitigation: the before/after run of section 5.
* Host values: values a host gives (`PhyEquation.callWith`, Python `Quantity` arguments) are locals bound in `env` (reserved-names phase 3), so they are not affected.
* A symbolic result re-evaluated by the interpreter that contains a free symbol (the `x`, `a`, `b` of the trace may come from there). The plan's second task attributes each probe name of section 2 to its source before the change, so a legitimate round trip is not broken; if one is found, it is bound (a local) instead of being tolerated.

## 7. Not in this change

* Making unit names (`m`, `s`, `kg`) reserved (decided against in the reserved-names design).
* The symbolic engine's handling of free symbols.
* The unit holes (`max(1 m, 1 s)`, `min(1 m, 2)`, `vector(1 m, 2 s)`).
* Any version bump or merge.
