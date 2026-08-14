# Track C — Breakpoints Design Spec (`DebugHook` + CLI debugger)

**Date**: 2026-08-13
**Status**: Approved
**Subsystem**: `physure-script` (AST, Interpreter, Builtins), `physure-core` (UnitRegistry), `physure-cli`

---

## 1. Overview & Goals

Track C adds a debug-hook mechanism to the PHS interpreter and a `phs debug script.phs` CLI
debugger consuming it, per [docs/language_readiness_roadmap.md §5](../../language_readiness_roadmap.md).
Scoped to the roadmap's non-stretch checklist items:

1. **`DebugHook` trait + `Vec<StackFrame>` call-stack tracking**, wired into the interpreter's
   statement and loop-iteration dispatch, zero cost when no hook is set.
2. **Line, conditional, and function-entry breakpoints.**
3. **`Inspection`**: full decomposition of a paused variable — measure, kind, scope, unit,
   prefix, dimension, uncertainty — not just a `Display` string.
4. **`phs debug script.phs` CLI debugger**: `print`, `inspect`, `locals`, `globals`,
   `backtrace`, `break`/`break fn`, `step`/`next`/`finish`/`continue`.
5. **`parallel_map` sequential-fallback rule**: a breakpoint set while debugging forces
   `parallel_map` off the `rayon` pool for that session.

**Explicitly out of scope**: the DAP adapter (`physure-lsp` or a new `physure-dap` crate) — a
stretch item per the roadmap, not required for LAB-READY, and not designed here.

**Not in the roadmap's original sketch, added during brainstorming**: a source-location
foundation (§3 below). The roadmap's `DebugContext { line: usize, .. }` and `--break 42` assume
every `Statement` already knows what line it's on. It doesn't — `ast.rs`/`parser.rs` carry no
position information at all today. This is a genuine prerequisite, not a documentation gap.

---

## 2. Scope Decomposition (for parallel implementation)

Five independently-plannable sub-tracks, sharing this one spec. **C0 is the only hard
dependency** — everything else reads line numbers or the call stack it introduces:

```
C0 (source locations) ──┬──> C1 (DebugHook + StackFrame) ──> C3 (breakpoints) ──┐
                         ├──> C2 (Inspection)                                   ├──> Integration
                         └──> C4 (phs debug CLI, stubbed against a fake hook)  ──┘
```

C1, C2, and C4 touch disjoint files and can be worked in parallel once C0 lands. C3 starts once
C1 lands (it needs `DebugContext`). C2 barely needs C0 at all — it only reads a `PhsValue` /
`Quantity`, not a line number — so it can start immediately, in parallel with C0 itself, and
only needs to wait on nothing.

---

## 3. Sub-track C0 — Source-location plumbing

**Problem, confirmed by reading the code**: `Statement` (`ast.rs`) has no line field, and no
wrapper type attaches one. `parser.rs` calls pest's `pair.line_col()` today only for
parse-error messages — the value is never stored on the AST it produces.

**Rejected approach**: wrapping every `Statement` as `{ line: usize, kind: StatementKind }`
(renaming the enum) is the "obviously correct" compiler-textbook fix, but `Statement::` is
matched or constructed at 201 sites across 14 files (`grep -c` confirmed). That blast radius is
disproportionate to what breakpoints actually need.

**Chosen approach**: additive parallel `Vec<usize>` fields, matching this codebase's existing
convention for backward-compatible AST growth (`FunctionDefNode.decorators`/`.doc`, both
`#[serde(default)]`):

```rust
pub struct Program {
    pub statements: Vec<Statement>,
    #[serde(default)]
    pub lines: Vec<usize>,       // lines[i] is the source line of statements[i]
}

pub struct FunctionDefNode {
    // ...unchanged...
    pub body_stmts: Vec<Statement>,
    #[serde(default)]
    pub body_lines: Vec<usize>,  // body_lines[i] is the source line of body_stmts[i]
}

pub enum Statement {
    // ...unchanged except While...
    While {
        cond: Expr,
        body: Vec<Statement>,
        #[serde(default)]
        body_lines: Vec<usize>,
    },
}
```

`Program` and `FunctionDefNode` are structs, so adding a field there breaks nothing — every
existing `for stmt in &program.statements` keeps compiling untouched. `Statement::While` is the
one variant whose payload is inline in the enum, so it's the one real touch point: `grep` finds
22 `Statement::While` sites across 8 files, of which ~10 destructure `{ cond, body }` without
`..` and need a mechanical update (add `..` or bind `body_lines`); the other ~12 already use
`{ .. }` / `{ body, .. }` and need no change. Rust's exhaustiveness checking flags every site
that needs touching — this is compiler-guided, not exploratory.

**Populating it**: `parser.rs`'s `parse_statement` (the single funnel function every `Rule::stmt`
match goes through, confirmed at `parser.rs:55`) captures `pair.line_col().0` once per call.
The three call sites that build a `Vec<Statement>` (top-level program, a function body, a
`while` body) collect the parallel `Vec<usize>` alongside it in the same loop.

**Test**: parse a multi-line script with a top-level statement, a function with a two-statement
body, and a `while` with a two-statement body; assert `program.lines`, the function's
`body_lines`, and the `while`'s `body_lines` each match the source line numbers by hand-counting
the test script.

---

## 4. Sub-track C1 — `DebugHook` + call stack

```rust
pub trait DebugHook: Send + Sync {
    fn on_statement(&self, ctx: &DebugContext) -> DebugAction;
}
pub struct DebugContext<'a> {
    pub line: usize,
    pub call_stack: &'a [StackFrame],
    pub env: &'a HashMap<String, PhsValue>,
}
pub struct StackFrame {
    pub fn_name: String,
    pub call_site_line: usize,
    pub declared: HashSet<String>,
}
pub enum DebugAction { Continue, StepInto, StepOver, StepOut, Pause }
```

Matches the roadmap's sketch exactly (§5) — no changes needed there, only to how it's wired in:

- `PhsInterpreter` gains two fields: `debug_hook: Option<Arc<dyn DebugHook>>` and
  `call_stack: Arc<Mutex<Vec<StackFrame>>>` — `Mutex`, matching the existing
  `plugin_state`/`unlocked_builtins`/`dynamic_externals` fields, **not** `RefCell`: Track B's
  `for`-expression and `parallel_map` rayon paths already require `&PhsInterpreter: Send + Sync`
  at compile time (the closure runs on multiple worker threads), and that bound is checked
  regardless of whether a hook is actually set at runtime — `RefCell` would break both of
  Track B's already-shipped parallel paths, not just misbehave when contended. §7's
  sequential-fallback rule means the mutex is never actually contended once debugging is active,
  but the type still has to satisfy `Sync` unconditionally. A new `with_debug_hook(resolver,
  hook)` constructor sets `debug_hook`; `Default`/`new` leave it `None`.
- **One choke point, reached from two places.** Today, `call_function_node`'s per-statement loop
  special-cases `Statement::Return`/`GuardReturn` with its own `break`, bypassing
  `eval_statement_with_env` (which already *does* handle those two variants, for every other
  caller — top-level via `eval_statement`, and `while`-loop bodies). This is a pre-existing
  near-duplication, not something C1 introduces, but it means a hook inserted only inside
  `eval_statement_with_env` would never fire on a function's final `return` — the single most
  common place a user would want to pause. Fix: a shared private
  `fn debug_checkpoint(&self, line: usize) -> PhysureResult<()>`, called from the top of
  `eval_statement_with_env` **and** from the top of `call_function_node`'s loop body (before its
  `match stmt`). Both call sites already have a line available once C0 lands (`self` doesn't
  need one — the caller passes the statement's `line`/`body_lines[i]`).
- `debug_checkpoint` is `if self.debug_hook.is_none() { return Ok(()) }` as its first line —
  zero allocation, zero lock, when debugging isn't active. This satisfies the roadmap's "no-op
  by default" requirement directly.
- **Loop iterations are checkpoints too** (roadmap §5): the `Statement::While` arm in
  `eval_statement_with_env` and the (already-existing, unrelated) `Expr::ForExpr` sequential
  path both call `debug_checkpoint` once per iteration's body statement — which they already do
  for free, since both funnel through `eval_statement_with_env` per statement. No extra call
  site needed beyond the one already covering every statement.
- `StackFrame::declared` is computed once, statically, when `call_function_node` pushes a frame:
  `func.params.iter().cloned().chain(assignment/functiondef targets in func.body_stmts)`. This
  needs no new physure-core work — the `FunctionDefNode` is already in hand at the point the
  frame is pushed.
- `StepOver`/`StepOut` are implemented by comparing `call_stack.len()` at the point `Step*` was
  issued against the current depth on each subsequent `debug_checkpoint` call: `StepOver`
  resumes `Continue` until depth `<= saved_depth`, `StepOut` until depth `< saved_depth`,
  matching the roadmap's description of this as "the one real piece of interpreter plumbing this
  track adds."

**Test**: a fake `DebugHook` that records every `(line, call_stack.len())` it's called with;
run a script with a function call and a `while` loop; assert the recorded sequence matches hand
counting, including that the function's `return` statement is recorded (regression test for the
choke-point gap above).

---

## 5. Sub-track C2 — `Inspection` (independent of C0/C1)

```rust
pub enum ScopeKind {
    Global,
    Local { owner_fn: String, frame_depth: usize },
}
pub struct Inspection {
    pub name: String,
    pub kind: ValueKind,               // Scalar | Vector(len) | Matrix(rows, cols)
                                        // | Function | Equation | Bool | String
    pub scope: ScopeKind,
    pub measure: Option<f64>,
    pub unit_display: Option<String>,
    pub prefix: Option<(String, f64)>,
    pub dimension: Vec<(String, i64, i64)>,
    pub uncertainty: Option<UncertaintySummary>,
    pub detail: ValueDetail,
}
pub struct UncertaintySummary { pub std_dev: f64, pub backend: String }
```

Two deliberate deviations from the roadmap's literal sketch, found by checking what
`physure-core` actually wires up at runtime rather than what a same-named type happens to define
elsewhere:

- **`dimension`**: the roadmap names `DimVector`/`SI_ORDER` (`[L, M, T, I, Θ, N, J, A, $]`,
  `physure-core/src/units/dimension.rs`). That module is dead code — `grep` confirms it's
  referenced only by its own tests and `UnitDefinition`, and `UnitDefinition` is itself
  referenced nowhere outside its own file. The real conf-loading path
  (`units/conf.rs::registry.add_base_unit`) builds `RationalUnit`s directly and never
  constructs a `DimVector`. The **live** dimension representation is
  `RationalUnit.dimensions: SmallVec<[(String, (i64,i64)); 4]>`, keyed by whatever base-unit
  symbols `physure.conf` actually registers (`"m"`, `"kg"`, `"s"`, `"A"`, …) — not abstract SI
  letters. `Inspection.dimension` is `quantity.unit.dimensions` directly: `Vec<(String, i64,
  i64)>` of *registered base-unit symbol* → (numerator, denominator). Zero new physure-core
  code, and arguably more informative than an opaque letter code.
- **`prefix`**: reliably recoverable only for a single, non-compound unit that already carries a
  `display_name` (e.g. `"km"`). `UnitRegistry::get_unit` (`registry.rs:142-179`) already
  contains an inline loop that strips a known prefix symbol and resolves the remainder — this
  gets factored out into a small new
  `UnitRegistry::split_prefix(&self, name: &str) -> Option<(String, f64, String)>` (prefix
  symbol, factor, remainder symbol), reused by both `get_unit` and the new `Inspection` code —
  one code path, not two independently-written ones. For a compound unit (`"km/h"`) or any
  `RationalUnit` synthesized by arithmetic rather than looked up by name, `display_name` is
  `None` and so is `prefix` — an honest `None`, not a wrong guess.
- **`unit_display`**: `RationalUnit::__repr__()` — the same rendering `builtins.rs`'s value
  formatter already uses for `print`/`format`. There is no "as written" source text to recover
  in general (a computed unit like the result of `5 m/s * 2` was never literally written), so
  the best-effort pretty-print is the honest answer, consistent with what the rest of the
  language already shows the user.
- **`uncertainty`**: `Quantity.value.mean()`/`.std_dev()` and
  `UncertaintyBackend::get_model_name()` (already implemented by every backend —
  `"gaussian"`, `"monte_carlo"`, `"unscented"`, …) are read directly. No new backend work.
- **`scope`**: `ScopeKind::Local` is determined by walking `DebugContext.call_stack`
  innermost-out for the first frame whose `declared` set contains the name being inspected
  (per §4); anything else present in `self.env` at the top level is `ScopeKind::Global`.

**Test**: build an `Inspection` for a scalar `Quantity` with a known unit/uncertainty and assert
every field; build one for `"km"` and assert `prefix == Some(("kilo".into(), 1e3))` (or
whatever `physure.conf`'s prefix table actually names it — read the conf, don't assume); build
one for `"km/h"` and assert `prefix == None`.

---

## 6. Sub-track C3 — Breakpoints (depends on C1; line kind depends on C0)

```rust
pub enum Breakpoint {
    Line(usize),
    Conditional(usize, Expr),   // parsed once when the breakpoint is set, evaluated per hit
    FunctionEntry(String),
}
```

`debug_checkpoint` (§4) checks the active breakpoint set before consulting the hook's last
`DebugAction`: a `Line`/`Conditional` breakpoint matches when `ctx.line` equals it (conditional
additionally evaluates its `Expr` via the existing `eval_expr(&self, expr, ctx.env)` — same
mechanism `@requires` already uses, no new expression machinery); a `FunctionEntry` breakpoint
matches on the frame-push that creates a new innermost `StackFrame` whose `fn_name` matches.

**Test**: a script with a function called from two places; set a `FunctionEntry` breakpoint on
it, run, assert it pauses on both calls; set a `Conditional` breakpoint on a `for`-loop-adjacent
line with a condition that's true only on one specific value, assert it pauses exactly once.

---

## 7. Sub-track C4 — `phs debug` CLI (independently buildable)

`physure-cli` already has everything this needs, established by existing code, not new
dependencies:

- A plain stdin read-loop (`run_repl`, `main.rs:56`) — `phs debug` follows the identical
  shape (prompt, `read_line`, dispatch on the trimmed line), no `rustyline`/`clap` addition.
- A subcommand-dispatch convention (`export.rs` + `if args[1] == "export" { export::run_export
  (&args); return; }` in `main.rs`) — `debug.rs` + `if args[1] == "debug"` follows it exactly.

`debug.rs::run_debug(args)`:
1. Parses `script.phs [--break-fn name] [--break N[:cond]]` (repeatable flags for multiple
   breakpoints, following the existing `get_flag_value`/`args.iter().any` helpers already used
   elsewhere in `main.rs`/`export.rs` — no new arg-parsing dependency).
2. Constructs a `PhsInterpreter::with_debug_hook(..., hook)` where `hook` is a small
   `CliDebugHook` implementing `DebugHook` by printing the pause banner and then blocking on
   another stdin read-loop for debugger commands (`print`/`inspect`/`locals`/`globals`/
   `backtrace`/`step`/`next`/`finish`/`continue`), returning the resulting `DebugAction`.
3. `locals` lists only the innermost `StackFrame.declared` names (with their current value from
   `ctx.env`); `globals` lists names present in `self.env` at the top level — i.e. exactly
   `ScopeKind::Global` from §5, resolving the reason these are split rather than one flat dump
   (the roadmap's own note: `ctx.env` is a full clone-and-overlay of everything visible at the
   call site, so an undifferentiated dump would print every global inside a one-line helper).
4. `inspect <name>` builds and pretty-prints an `Inspection` (§5); `print <name>`/`print <expr>`
   evaluates via the existing `eval_expr` and uses the existing `Display`-style formatter.

**Test**: this is the sub-track that can be built and unit-tested in isolation against a small
local fake `DebugHook`-consuming interface before C1/C3 land for real — command parsing
(`"break 42:v > 100 m/s"` → `Breakpoint::Conditional(42, ...)`) and output formatting are pure
functions, testable without an actual paused interpreter.

---

## 8. Integration

- `builtins.rs`'s `parallel_map` (just shipped in Track B) gains one check at its top:
  `if interpreter.debug_hook.is_some() { /* existing sequential .map() path */ } else {
  /* existing rayon par_iter path */ }` — the roadmap's explicit rule that a breakpoint inside a
  parallel closure isn't a coherent debugging experience, made real rather than just documented.
- **End-to-end scripted test**: a script with a named function called from a loop; set a
  `--break-fn` breakpoint; drive the CLI debugger's stdin loop programmatically (feed it a
  scripted sequence of commands via a piped/mocked stdin, matching however `physure-cli`'s
  existing tests — if any — already exercise `run_repl`); assert it pauses at the right line,
  `inspect` on a local decomposes measure/unit/prefix/dimension/uncertainty correctly, and
  `continue` resumes to completion.

---

## 9. Testing & Verification Strategy Summary

1. **C0**: parse-and-assert-line-numbers test (program/function/while, all three `lines` fields).
2. **C1**: fake-hook-records-checkpoints test, including the function-return regression case.
3. **C2**: `Inspection` field-by-field tests for a plain scalar, a `"km"`-unit scalar (prefix
   present), and a `"km/h"`-unit scalar (prefix absent).
4. **C3**: function-entry breakpoint hit twice; conditional breakpoint hit exactly once.
5. **C4**: command-parsing unit tests, independent of a live interpreter.
6. **Integration**: `parallel_map` sequential-fallback-when-debugging test; end-to-end scripted
   CLI-debugger test (roadmap's own checklist item).
