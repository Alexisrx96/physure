# PHS Execution Context — Proposal

> **Date:** 2026-08-02
> **Status:** Proposed — not implemented, nothing scheduled
> **Affects:** `physure-script` (grammar, interpreter, codegen), `physure-cli` (REPL), `physure-lsp`
> **Does not affect:** `physure-core` — the primitive it needs already exists

---

## 1. The problem

Python can say *how* a calculation should be done, scoped to where it matters:

```python
with physure.propagation_mode("uncorrelated"):
    ...
with physure.uncertainty_model("moments"):
    ...
with physure.use_system("imperial"):
    ...
```

PHS cannot say any of it. The only channel a script has is `[Settings]` in a
`physure.conf` sitting in the working directory, which means:

1. **A script cannot declare what it needs.** `x - x` is `0 m` or `0 ± 1.41 m`
   depending on a file the script never mentions and the reader may not have. The
   script is not wrong and the conf is not wrong; the pair is unreproducible.
2. **The setting is all-or-nothing.** A conf applies to every line of every script
   run from that directory. There is no way to drop provenance tracking for one
   expensive loop and keep it everywhere else, which is the whole reason the
   `uncorrelated` opt-out exists.
3. **The transpilers cannot carry it.** `phs transpile` emits Python, Rust and
   Java that reproduce the arithmetic but not the conditions it ran under. A
   script whose conf said `uncorrelated` transpiles to Python that computes
   correlated, silently, with the same numbers on the page and different numbers
   coming out.
4. **New knobs have nowhere to go.** `uncertainty_model("moments")` exists in
   Python and has no PHS spelling, so `12.3 +/- (0.5, 0.4) pb` parses but has no
   scope in which it could ever be evaluated. Asymmetric propagation cannot land
   in PHS before this does.

The gap is not that the core lacks a mechanism. `physure_core::uncertainty::mode`
already carries the mode, and `mode::scoped(...)` already gives a per-thread
override that restores on drop — which is precisely the primitive an interpreter
needs. What is missing is a PHS-level notion of "the conditions this expression is
being evaluated under", and a way to write it down.

---

## 2. What a context holds

| Knob | Python | PHS today | Where it is read |
|---|---|---|---|
| `propagation_mode` | `propagation_mode(...)` | conf only, process-wide | `Lineage::combine`, `Quantity::new_scalar` |
| `uncertainty_model` | `uncertainty_model(...)` | — | `Uncertainty.from_standard` (Python only) |
| unit system | `use_system("imperial")` | — | nothing in Rust reads a system |
| `default_output`, `auto_simplify`, `readable_representation` | conf | conf, never read by Rust | nothing |
| unlocked builtins (`use solve from calc`) | — | per-program, already scoped | `PhsInterpreter.unlocked_builtins` |

The last row matters: PHS already has one thing that behaves like a context — the
domain-gated builtins a `use` statement unlocks. It is stored on the interpreter
and lives for the program. A context is the same idea generalised to settings
rather than names, which is an argument for building it the same way rather than
inventing a second mechanism beside it.

---

## 3. Proposed model

### 3.1 The value

```rust
/// The conditions an expression is evaluated under.
#[derive(Clone, PartialEq)]
pub struct PhsContext {
    pub propagation: PropagationMode,
    pub uncertainty_model: UncertaintyModel,
    pub unit_system: Option<String>,
    // room for the formatting settings when something reads them
}
```

`PhsInterpreter` gains `context: PhsContext` plus a stack for nesting. Entering a
scope pushes a modified copy and calls `physure_core::uncertainty::mode::scoped`,
whose guard is held for as long as the scope is on the stack; leaving pops and the
guard restores. The interpreter never touches the process-wide setter, so a `phs`
run and an embedding host cannot fight over it.

### 3.2 Resolution order

```
enclosing `with` block  →  file-level `set`  →  physure.conf [Settings]  →  built-in default
```

Identical in shape to Python's `ContextVar → system settings → default`, which is
what keeps the two languages from needing separate explanations.

### 3.3 Surface syntax

Two forms, one mechanism. **Recommended minimum is the first**; the second is the
extension once the first is proven.

**(a) File-level declaration — solves problems 1, 3 and 4:**

```phs
set propagation_mode = "uncorrelated"
set uncertainty_model = "moments"

x = 10.0 +/- 1.0 m
x - x            # 0.0 ± 1.41 m, and the script says why
```

Applies from the statement to the end of its scope (file, or a function body).
One statement per line, so the REPL — which parses line by line, `physure-cli/src/main.rs:87`
— needs no changes, and the LSP sees a normal statement.

**(b) Scoped block — solves problem 2:**

```phs
with propagation_mode = "uncorrelated":
    heavy_sum(samples)
```

Reuses the existing indented `block_body` rule that `assignment_fn` already uses,
so the grammar cost is one rule. This is the form that cannot be evaluated
line-by-line, so it is the one that forces REPL work — hence second.

**Rejected alternatives**

- *Postfix `under`, mirroring the existing `where`* (`expr under propagation_mode = "uncorrelated"`).
  Composes with `where` and is REPL-safe. Rejected as the primary form only
  because a setting that changes how numbers combine reads better as a
  declaration than as a modifier hanging off the end of one expression — but it
  is the closest thing to the language's existing grain, and worth revisiting.
- *`use uncorrelated from uncertainty`*, riding on the import syntax. Rejected:
  `use ... from` binds names, and overloading it to set values would make the same
  statement mean two different kinds of thing.
- *A magic comment / shebang pragma* (`#!propagation uncorrelated`). Rejected: a
  comment that changes results is exactly the silent dependency this proposal is
  trying to remove.

---

## 4. The transpiler obligation

This is the part that is not negotiable and should be settled before any syntax is
chosen. A context that the interpreter honours and the transpilers drop is worse
than no context at all: today a reader can at least see the conf, whereas
transpiled code would carry no trace of the conditions it was written under.

| Target | Equivalent | Action |
|---|---|---|
| Python | `with physure.propagation_mode(...)` | emit it |
| Rust | `let _g = physure::uncertainty::mode::scoped(...)` | emit it |
| Java | none exists | **refuse**, following `QuantityNode::asymmetric_refusal` |

The refusal path already has a shape in this codebase: one method on the node that
returns `Option<String>`, called by the interpreter and all three code generators,
so a new target cannot forget it. Any context work should extend that method
rather than add a second refusal mechanism.

---

## 5. Semantics to settle

- **Does a context cross a function call?** Recommended: yes, dynamically, like
  Python's `ContextVar` — a function called inside an `uncorrelated` block runs
  uncorrelated. A function that needs otherwise pins it with its own `set` in the
  body, which shadows. The lexical alternative (a function runs under the context
  it was *defined* in) is safer for library authors and more surprising for
  everyone else.
- **What does a context do to a value that outlives it?** Nothing. A `Quantity`
  built under `monte_carlo` keeps its samples after the block ends; the context
  decides how values are *made* and *combined*, not what they are.
- **Does `set` in an imported module leak to the importer?** Recommended: no. An
  import that silently changed the caller's propagation mode would be the worst
  version of problem 1.
- **What happens on an unknown value?** Refuse at parse time. `PropagationMode`'s
  `FromStr` already produces the message; the conf path only warns because it has
  no error channel, and a script does.

---

## 6. Phasing

1. `PhsContext` on the interpreter, populated from the conf, read where the
   process-wide mode is read today. No syntax, no user-visible change — but it
   removes the global from the PHS path.
2. `set` statements, plus Python and Rust codegen and the Java refusal.
3. `with` blocks, plus REPL and LSP support for multi-line scopes.
4. `uncertainty_model` through the same channel, when asymmetric propagation
   lands and there is something for `"moments"` to select.

Step 1 is worth doing on its own merits even if no syntax is ever added.

---

## 7. Open questions for the user

1. Is `set` the right keyword? It is not currently reserved, and reserving it
   breaks any script using `set` as a variable name.
2. Should the unit system be part of this at all? Nothing in Rust reads a unit
   system today, so including it means building that first — a much larger job
   than the uncertainty knobs.
3. Should a context be *required* for anything, or always optional with a conf
   fallback? Requiring a script to declare its propagation mode would make every
   script reproducible, and would break every existing one.
