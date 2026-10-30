# PHS evaluation on a registry's own unit catalog — design

**Date:** 2026-10-30
**Status:** proposal, not implemented. Written at the maintainer's request after the strictness pass (`2026-10-30-binding-strictness-design.md`), whose "not fixed" list it closes.
**Affects:** `physure-core` (`units/parser.rs`, `units/conf.rs`), `physure-script` (`parser/quantities.rs`), the Java and WASM bindings. Does not change any public signature in the recommended option.

## The problem

A Java or WASM `UnitRegistry` can be built from a conf that adds or redefines units. `getUnitScale`, `getUnitExponents` and `getUnitPowers` answer from that catalog. `evaluate`, `solve`, `deriv`, `integral` and `PhyFunction` run the PHS interpreter, which resolves every unit through the **embedded** catalog, so:

| On a registry built from `furlong = 201.168, L, [fur]` and `km = 7.0, L, [km]` | |
|---|---|
| `getUnitScale("fur")` | 201.168 |
| `evaluate("5 fur")` | `Unknown unit 'fur'`: loud, but the registry has the unit |
| `getUnitScale("km")` | 7.0 |
| `evaluate("1 km")` | used 1000, silently (now refused, see below) |

The interim fix (commit `fix(java,wasm): a registry that redefines a unit of the catalog refuses PHS evaluation`) turns the silent case into an error and leaves the rest as it was. This document is about making `evaluate` use the registry's catalog, so that the guard can go.

## What was measured

An earlier estimate in the conversation counted call sites: 49 calls in 11 `physure-script` files that create quantities, parse unit text or build the default catalog (35 outside the transpilers and the tests), and about 90 callers of `parse_phs` outside the parser (65 in `physure-script`, 10 in the CLI, 6 in the language server, 5 in WASM, 3 in Java, 1 in Python), which would make an explicit "pass the registry everywhere" change large. Reading where the catalog is actually **chosen** gives a smaller picture:

- **In the core there is one choke point.** `Quantity::new` and `Quantity::with_uncertainty` (`quantity.rs:220`, `:809`) call `Parser::parse_expression`, which calls `build_registry_from_conf()` (`units/parser.rs:196`): a copy of the embedded catalog plus, if there is a `physure.conf` in the working directory, its definitions. Nothing else in the core resolves unit text through the default catalog outside tests.
- **In the script there is one more.** `parser/quantities.rs` has a private `unit_registry()` (a process-wide `OnceLock` of `build_registry_from_conf().0`) used twice: by `is_known_unit_symbol`, and by the `split_unit_expr` scan in `parse_quantity` that decides where a unit chain ends in `5 fur` or `2 m/s`. This is what decides, at parse time, whether a word after a number is a unit.
- The other call sites in the script (35 that create a quantity or parse unit text, outside the transpilers and the tests) reach the catalog **through** those two. They never choose one.

Two things seen on the way, relevant to the design:

1. `build_registry_from_conf()` clones the embedded catalog and checks for `physure.conf` in the current directory **on every call**, so every `Quantity::new` in a process pays a catalog copy and a file-system check, and a `physure.conf` dropped into the working directory changes how every unit text in the process is read. A registry scope would also be a place to stop doing that.
2. The interpreter keeps AST nodes with unit text and resolves them when it evaluates; a function defined through a registry is called through the same registry, so the resolution scope is the call, not the registry's lifetime.

## Options

### A. Pass the registry down

`parse_phs(code, &registry)`, a `registry` field on `PhsInterpreter`, and every `Quantity::new(value, text)` in the script becomes "parse the text with the interpreter's registry, then build the quantity". The explicit option; every caller of `parse_phs` changes (about 90 sites in five crates), plus the public API of a crate held to the strict coverage gate (90 % new coverage). It is also the shape that the `2026-08-02-phs-execution-context.md` proposal wants for settings, so it is the right place to end up, not the cheapest place to start.

### B. A scoped catalog (recommended)

```rust
// physure-core::units
pub fn with_catalog<R>(registry: &UnitRegistry, f: impl FnOnce() -> R) -> R
```

A thread-local stack of catalogs, set for the duration of `f` and restored by an RAII guard (so a panic or an early return cannot leave a catalog behind). `Parser::parse_expression` and the script's `unit_registry()` consult the top of the stack first, and fall back to what they do today. A binding wraps what it does with the registry (`parse_phs`, then `run_statement`) in `with_catalog(&handle.registry, ...)`. Nothing else changes: no signature, no caller, the CLI and the language server never set a scope and see no difference.

*For:* two places in the core and the script plus one wrapper per binding entry point (`evaluate`, `evaluateQuantity`, `PhyFunction` calls: about five in Java, five in WASM); the guard that refuses evaluation on a registry with redefinitions disappears; the scope is the right place to skip the per-call catalog copy (it already holds a `&UnitRegistry`).
*Against:* an implicit, thread-local context; it must never be held across an `await` or handed to another thread (the bindings' calls are synchronous and each is on one thread, so they never do); two registries in nested scopes are legal and the inner one wins, which has to be tested.

### C. Keep the guard

The interim state: correct, loud, and the custom units are usable for `getUnitScale` and friends only. Costs nothing more; leaves a registry's own catalog out of every calculation that is not a bare unit lookup.

## Recommendation

B, as one change in three commits (core scope + tests, script + tests, bindings and removal of the guard), with A kept as the direction for the execution-context work: when `parse_phs` takes a context, the scope becomes the context's catalog and the thread-local goes away.

### Semantics to settle in the change

- **Which catalog a scope sees**: exactly the registry given, not that registry plus `physure.conf` of the working directory (a registry built from a conf is already complete; the file in the directory is for processes that did not choose a catalog).
- **Nesting**: the innermost scope wins; leaving it restores the outer one; no scope is today's behaviour.
- **What a value remembers**: a `Quantity` carries its `RationalUnit`, which is self-contained, so a value computed under one catalog converts and prints under any (`to("km")` resolves the *target* text, under the caller's scope).
- **Functions and unit chains defined under a scope** keep the meaning they were parsed with; a registry's interpreter is always called under its own scope, so they are consistent.

## What changes in the bindings

- Java: `evaluateExpression`, `evaluateQuantity`, `getFunctionParams`'s callers and the interpreter-running calls wrap their body in the scope; `RegistryHandle.redefined` and `refuse_evaluation_on_redefined_units` are deleted; `CustomRegistryTest`'s refusal tests become "evaluates with the registry's meaning" tests.
- WASM: the same in `evaluate`, `PhyFunction::{new, call, evaluate_string, define}`; `RegistryState.redefined` and `check_evaluable` go.
- `UnitRegistry::redefinitions_of` can stay: it is a fact about a registry that a caller may want, and it costs nothing when unused.

## Tests (acceptance)

- `1 km` on a registry that defines `km = 7.0` is 7 m (the same answer as `getUnitScale("km")`), and `5 fur` on one that adds `furlong` is 1005.84 m, through `evaluate`, `solve`/`deriv` on an expression with units, and a `PhyFunction` call, in Java and WASM.
- Two registries with different `km` evaluated in turn, and nested in one thread (core-level test), each answer with its own.
- A scope left by a panic restores the outer catalog; no scope behaves as it does today (the existing 413 `physure-script` tests and the core suite are the regression test).
- A thread that has a scope does not leak it to another thread (a core test with two threads); concurrent Java threads on separate registries each see their own (the strict suite already runs eight).
- New code in `physure-core` and `physure-script` covered at 90 % or more (the "Physure Core Strict" gate).

## Size

Measured, not guessed, after the survey above: the core scope (about 40 lines and its tests), the script's `unit_registry()` (about 15 lines and tests), five entry points in each binding, the removal of the interim guard, and its tests: **about half a day**, most of it tests, of which the 90 % gate on the new core and script code is the bulk. The explicit option A is the large one, and is part of the execution-context work, not this one.

## Open questions

- Is a thread-local scope acceptable in `physure-core`, or should the core take the registry as a parameter (`Quantity::new_in(&registry, ...)`) and keep only the script's choice thread-local? The second removes the hidden context from the core's API at the price of about ten more call sites in the script.
- Should a registry built from a conf also stop reading `physure.conf` from the working directory? It does not today (the Java and WASM catalogs are built from the embedded text and the given conf only), so only the scope's fallback is in question.
