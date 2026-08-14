# Track D — Incremental LSP Evaluation Design Spec

**Date**: 2026-08-14
**Status**: Approved
**Subsystem**: `physure-lsp`

---

## 1. Overview & Goals

Track D replaces `physure-lsp`'s current full-file reparse-and-re-execute on every keystroke with
per-statement, dependency-aware incremental re-evaluation, per
[docs/language_readiness_roadmap.md §6](../../language_readiness_roadmap.md). Scope is a single
open document, in `physure-lsp` only — no cross-file incrementality, no `salsa` adoption (both
explicitly out of scope per the roadmap; a hand-rolled graph is enough for PHS's dependency
structure).

**Confirmed in the current code** (`physure-lsp/src/main.rs::on_change` /
`analyze`, [main.rs:409-500](https://github.com/Alexisrx96/physure/blob/main/physure-lsp/src/main.rs#L409-L500)): every keystroke
builds a fresh `PhsInterpreter::default()` and calls `run_statement` for every statement in the
file, with no caching. Diagnostics are the *only* consumer of this execution today — `analyze`
returns `Vec<Diagnostic>` and nothing else is read off the interpreter. Track D's job is to make
that same diagnostic output cheaper to keep up to date, not to add new LSP features.

---

## 2. Architecture

`Backend.documents: RwLock<HashMap<Url, String>>` becomes `RwLock<HashMap<Url, DocState>>`:

```rust
struct DocState {
    statements: Vec<Statement>,           // last successfully-parsed Program.statements
    lines: Vec<usize>,                    // parallel to statements, for diagnostic ranges
    interp: PhsInterpreter,               // persisted across edits — env carries forward
    diagnostics: Vec<Option<Diagnostic>>, // parallel to statements; None = that statement is clean
}
```

All new logic — the diff, the read/write analysis, the dirty-set sweep — goes in a new
`physure-lsp/src/incremental.rs`, independently unit-testable without a real LSP client. `main.rs`
(already 875 lines covering completion, hover, and diagnostics) keeps only orchestration: `on_change`
calls `incremental::apply_change(&mut doc_state, new_text)` instead of today's `analyze(&text)`.

`did_close` is added (there is no handler for it today, so `documents` never evicts an entry for
the lifetime of the LSP process — a pre-existing leak that a bigger per-doc `DocState` makes worth
fixing alongside this work, not a Track D feature in its own right).

---

## 3. Algorithm

Replaces the current "reparse everything, run everything" body of `on_change`:

1. **Reparse** the buffer via the existing `parse_phs_with_lines` (cheap — parsing isn't the cost
   being cut, re-*execution* is). On parse failure: publish the single parse-error diagnostic as
   today, and **leave `DocState` untouched** — a syntactically invalid buffer can't be diffed
   meaningfully, so the next successful parse resumes incremental diffing from the last known-good
   state rather than the broken one.
2. **Diff** `old.statements` against `new_statements` by common-prefix / common-suffix trim: walk
   both lists from the front while elements are structurally equal (`Statement: PartialEq`,
   already derived), then from the back (non-overlapping with the prefix). Everything between the
   two matched regions is the **changed span** — on the old side and the new side independently,
   since insertions/deletions change list length.
   ```
   old: [A, B, C]              new: [A, X, B, C]
   prefix match: [A]  (len 1)  suffix match: [B, C]  (len 2)
   changed span (old): []      changed span (new): [X]
   ```
   This is O(n), needs no new dependency, and is exact for the common case (a single edited,
   inserted, or deleted statement). It does not optimally detect a whole statement moving from the
   top of the file to the bottom — that degrades to treating more as changed than strictly
   necessary, which is always the *safe* direction (re-running too much costs time; re-running too
   little produces stale diagnostics, which the roadmap calls out as worse than today's baseline).
3. **`touched_names`** = every name written by any statement in the changed span, old side *or*
   new side. Union of both sides matters: a write that's purely *deleted* (present in the old span,
   absent from the new one) still needs every downstream reader of that name to re-resolve — old-side
   membership is what catches that case, since a pure index-based check over the new list alone
   would never see the deletion.
4. **Single forward sweep** over `new_statements`, `i` from `0` to `len - 1`, maintaining a running
   `last_writer: HashMap<String, usize>` (nearest-preceding-write index per name, rebuilt fresh
   every change — cheap, it's an AST walk, no execution):
   - Statement `i` is **dirty** if: `i` is in the new-side changed span, OR its read set intersects
     `touched_names`, OR any name it reads resolves (via `last_writer`, which only ever points
     backward) to an already-dirty statement.
   - Because reads only ever resolve to earlier writes, this one left-to-right pass is sufficient —
     no separate fixpoint loop needed.
   - Statements before the changed span can never be dirty by construction (everything they could
     read resolves within the unchanged prefix, which is identical to what already ran).
5. **Re-run** only the dirty statements, in file order, against `doc_state.interp` (the persisted
   interpreter, not a fresh one). Every non-dirty statement keeps its cached
   `diagnostics[i]`. Publish the full accumulated diagnostics list (LSP requires the complete set
   each time, not a diff).

---

## 4. Correctness subtleties

Two behaviors the roadmap's original sketch didn't spell out, both found by reading the actual
interpreter rather than assumed from the AST shape. Both matter because under-counting a
dependency produces stale diagnostics — explicitly called out in the roadmap as the failure mode
worse than not doing this track at all.

### 4.1 A function call is a read of its own name, not of an `Identifier` node

`Expr::FunctionCall { name: String, args, kwargs }` stores the callee's name as a bare `String`
field, not a nested `Expr::Identifier`. Resolution goes through `env.get(name)`
([interpreter.rs:849](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/interpreter.rs#L849)), exactly the same
lookup an `Identifier` read uses. A read-collector that only walks `Expr::Identifier` nodes would
silently miss every function call as a dependency edge.

**Fix**: the read-collector treats `FunctionCall.name` as an implicit read, in addition to
walking `args`/`kwargs` for their own reads.

Once fixed, the transitive case resolves itself with no extra machinery, because a
`Statement::FunctionDef`'s "expression tree" (per the roadmap's own definition of reads —
"every `Expr::Identifier` in its expression tree") already includes its nested `body_stmts`'
expressions:

```
g = 9.8                     # writes g
fn compute(m) = m * g       # reads g (body expr), writes "compute"
result = compute(2.0)       # reads "compute" (the FunctionCall.name fix)
```

Editing `g` dirties the `FunctionDef` statement (its body reads `g`) → that dirties `compute` as a
touched name → `last_writer["compute"]` points at the now-dirty `FunctionDef` statement → the call
site becomes dirty too, by rule 4's ordinary forward propagation. This also falls directly out of
`call_function_node`'s documented dynamic-scoping model (Track C's spec: `local_env = env.clone()`
at the *call site*, not the definition site) — there is no separate "closure capture" to model.

The read-collector walks the *entire* subtree of a top-level statement recursively — `BinaryOp`
children, `FunctionCall` args/kwargs, `ForExpr` iterable/body, decorator args, and (for
`FunctionDef`/`While`) every nested statement's own expressions — then subtracts the statement's
own locally-declared names, reusing the exact `collect_declared` logic Track C already built
([debug.rs:39](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/debug.rs#L39): params ∪ names assigned anywhere in
`body_stmts`) rather than re-deriving it.

### 4.2 Stale success on a broken rewrite

The persisted `env` is exactly that — persisted. If a statement that previously wrote `x`
successfully is edited into something that now fails, simply calling `run_statement` and ignoring
the `Err` would leave the *old* `x` sitting in `env` for every downstream reader, silently wrong —
a fresh full re-run (today's behavior) would never have had that stale value to begin with.

**Fix**: before re-running any dirty statement, remove the name(s) it is expected to write
(`Assignment.name`, `FunctionDef.name`, each bound name in an `Import`'s `Symbols` specifier) from
`env` first. A failing rewrite this time then correctly leaves that name undefined, matching what a
fresh interpreter would produce, and downstream readers fall back to the same behavior the
interpreter already has for an unbound identifier (unit-symbol fallback, then string-literal
fallback — see [interpreter.rs:685-700](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/interpreter.rs#L685-L700)) rather
than a silently-preserved old numeric value.

### 4.3 `where`/`let`-local names must not read as cross-statement dependencies

`where` clauses desugar to an internal `let(name, value_expr, body_expr)` pseudo-call
([interpreter.rs:718](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/interpreter.rs#L718)). The read-collector must
special-case this form: `value_expr`'s reads count normally (evaluated in the outer scope), but
`name` is locally bound for the walk of `body_expr` only, and must not be treated as depending on
some unrelated earlier top-level write of the same name. Chained `where a = 1, b = a + 1` desugars
to nested `let`s and falls out of the same rule recursively.

### 4.4 A `While` statement's writes are conditional on what already existed

Track A's scoping rule (roadmap §3): a `while` body's assignments persist to the enclosing scope
only for a name that **already existed there before the loop**; a name first assigned inside the
loop stays loop-local and never leaks out. So a top-level `Statement::While`'s write-set is not
simply "every name assigned in its body" (that would over-write the graph with names nothing
outside the loop can ever actually see) — it's the intersection of that set with whatever the
top-level scan has already seen written by an *earlier* statement.

This falls out of the same left-to-right sweep used for step 4's `last_writer` map with no extra
pass: when the sweep reaches a `While` statement, a body-assigned name counts as one of its writes
only if `last_writer` already has an entry for that name at that point. A body-assigned name with
no prior entry is loop-local — excluded from the graph entirely, since nothing outside the loop can
legitimately depend on it. The `While` statement's *reads* are computed the same way as a
`FunctionDef`'s (§4.1): the full recursive walk of `cond` and every body statement's expressions,
including the `FunctionCall.name` fix, minus names first assigned inside the loop itself.

---

## 5. Testing

Per the roadmap's checklist, both driven by an execution-count counter per statement in the test
harness (not wall-clock timing):

1. **Execution-count test**: a script with one statement whose result nothing downstream reads;
   edit it; assert only that one statement re-executed.
2. **Rebinding-correctness test**: two writes to the same name with reads in between; edit the
   *first* write; assert only the correctly-scoped dependents re-run (not the second, unrelated
   write to the same name).

Plus two targeted at the subtleties in §4:

3. A global read only inside a called function's body (never at the call site's own top-level
   expression) still propagates to the call site when the global changes (§4.1).
4. A statement that previously wrote a value successfully, edited into a form that now errors,
   removes that value for downstream readers rather than leaving it stale (§4.2).
5. A name first assigned inside a `while` body (no prior top-level write) does not dirty any
   statement outside the loop when edited; a `while` body reassignment of a name that *does*
   already exist at top level does propagate to statements after the loop that read it (§4.4).

---

## 6. Out of scope

- Cross-file incrementality via `FsModuleResolver` — a real follow-on, not designed here (per the
  roadmap).
- Adopting `salsa` — the roadmap's own build-vs-adopt call: start hand-rolled, revisit only if the
  query graph grows complex enough to justify it.
- Any new LSP-visible feature (hover-on-value, inline evaluation results, etc.) — Track D is a
  performance/responsiveness change to the existing diagnostics pipeline, not a new capability.
