# Static dimensional checking of PHS (N2) — design

**Date:** 2026-10-30
**Status:** tiers T1 to T3 implemented on `feat/static-dimension-check`, after the maintainer's two decisions below; T4 (generated code without units) is not started.
**Base:** local `main` @ `5e6e266`
**Follows:** the draft `2026-10-30-draft-static-dimension-checking.md` (next step N2 of `2026-10-30-post-a1-inventory.md`).

## Decisions (maintainer, 2026-10-30)

| Question | Decision | Rejected |
|---|---|---|
| A unit or a name the checker does not know | **An error.** | A warning; silence |
| A function parameter with no unit annotation | **Inferred from the call sites now** (the draft's tier T3), not left unknown. | Unknown, never an error |

## What the interpreter does, which the checker has to match

Read from `interpreter/binary_ops.rs`, `interpreter/expressions.rs`, `builtins/core.rs`:

- A value is a `Quantity`, a `Number`, a `String`, a `Vector`, a `Range`, an `Equation` or a `Function`. A bare word that is not bound **is a string**, unless it is a unit symbol, in which case it is that unit as a quantity; a binding beats a unit. The string meets a quantity in `Mul`/`Div`/`Convert` as a unit to parse (so an unknown word is the runtime error "Unknown unit") and in `Add`/`Sub` as an unsupported operation.
- `Add`/`Sub` need the same dimension; `Mul`/`Div` combine; `Pow` needs a dimensionless constant exponent; `=>` needs the same dimension; a plain number does not scale an absolute temperature; two absolute temperatures do not add.
- An annotated parameter converts its argument to the declared unit (a dimension mismatch is an error), and `@implicit_units` gives a bare number the declared unit.
- The builtins delegate to the core's `Quantity` (`sqrt` halves the dimension, `sin`/`cos`/`tan`/`exp`/`ln`/`log` take an angle or a dimensionless value, `abs`/`floor`/`ceil`/`round` keep it), except `min` and `max`, which compare **base-unit magnitudes with no dimension check** at all.

## The design

`physure-script/src/check.rs` (no dependency on the interpreter's state; it uses the core's `UnitRegistry::facts` for units, `formula_text` for messages and `Rational64` for exponents).

- **Types.** `Unknown`, `Qty(dimension, absolute-temperature?, literal value?)`, `Bool`, and `Symbol(name)` for a word that is certainly text. A dimension is a map from dimension symbol to an exact fraction. The literal value is kept for dimensionless constants so that an exponent such as `(1/3)` or `(4 - 2)` is read exactly.
- **Soundness.** An error is reported only when the interpreter is certain to refuse: both sides known quantities of different dimension, a unit not in the catalog, a word that is neither bound, imported nor a unit meeting a quantity, a temperature rule broken. A name bound *anywhere* in the program (a later assignment, a parameter of another function, an import) is `Unknown` where it is read, because it may be bound by the time that line runs; a wildcard or module import makes every unbound name `Unknown`. A call the pass cannot resolve (a function not yet defined, an imported one, one with keywords) is `Unknown`. The one warning is `min`/`max` of different dimensions, which the interpreter accepts.
- **Functions.** A definition is registered and its body checked once for the declared dimensions (annotated parameters typed, the others `Unknown`); that gives the return type of a fully annotated function. A call checks each annotated argument against its declaration (conversion within a dimension is fine, `@implicit_units` lets a bare number through), then, if some parameter has no annotation, **checks the body again for the dimensions of the arguments** (T3): memoised by the function and the dimensions, with a guard against recursion and a depth of six. A mismatch found there is reported at the body's own line with ` (called as add(L, T) at line N)`.
- **Where.** The AST carries the line of each statement and no column, so a finding is on a line. In the language server it covers the line.
- **Surfaces.** `check_source(&str)` and `check_program(&Program, &UnitRegistry)`; `phs check <file>... [--json]` (exit 1 for an error, 2 for a file that cannot be read or parsed); the language server appends the findings to its syntax diagnostics (`source: physure-check`, `code`: `dimension-mismatch`, `unknown-unit`, `unknown-name`, `bad-argument`, `bad-exponent`, `temperature-points`, `mixed-extremum`).

## What was verified

- 41 tests in `tests/check.rs`: every rule above, each error also checked against the interpreter (**the interpreter refuses the same program**) and each clean program against the interpreter (**it runs**).
- A corpus test (`tests/check_corpus.rs`) takes every PHS program passed to `parse_phs(`/`eval_phs(` in the crate's own sources and tests (102 programs; 62 run) and requires **no error on any program the interpreter runs**: 0 false positives. Of the 2 programs of the corpus that the interpreter refuses for a dimension the checker reports 1. The other is `1 deg + 1 sr`: `facts` reports the same dimension symbol (`A`) for the degree, the radian and the steradian, and the core keeps the solid angle apart by a catalog flag (`own_kind`) that `facts` does not expose, so the checker takes them for the same dimension: a miss, never a false alarm. Reporting the flag in `UnitFacts` would close it.
- The tutorial `docs/tutorials/phs_primer.phs` has no finding.
- A check of the primer takes about 1.3 ms in a debug build, 870 bytes (the only `.phs` file in the repository, so no claim is made for large programs).
- `phs check` run end to end: exit codes 0, 1 and 2, text and `--json`.

## What it does not do (yet)

- **Columns.** A diagnostic covers a line.
- **T4.** Generated Rust and JS still carry `Quantity` and pay for units at run time; removing that cost needs a typed IR and is its own change.
- **Vectors, ranges, equations and strings are `Unknown`**: nothing is said about `[1 m, 2 s] + 1 kg`.
- **A refusal at a value** (a division by zero, a negative square root) is not a dimension and is not the checker's.
- **Arity** of a call and the keywords of a call are not checked.
- A conf or plugin the checker was not given is invisible to it: `check_source` reads the catalog the interpreter reads (the embedded one, plus a `physure.conf` in the working directory, or a `units::with_catalog` scope), so a unit that only a plugin adds is reported as unknown, as the interpreter would report it without the plugin.
- Not run in CI.
