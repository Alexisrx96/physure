# PHS Language Readiness Roadmap — Loops, Concurrency, Breakpoints, Incremental LSP, Compiled Exports & Decorators

**Status: ✅ LAB-READY reached — Tracks A/B/C/D/E/F all merged** (Track C's DAP adapter stretch item and Track D's cross-file incrementality follow-on remain open, both explicitly non-blocking)

> 🗺️ **Master Progress Tracker**: This document is a sub-roadmap of the [Master Development Roadmap](ROADMAP.md).

This document defines the path to **LAB-READY**: the point at which PHS scripts can express real
iterative/convergence workflows (loops), get real speedups on lab-scale data (concurrency), and can
be stepped through and inspected while debugging (breakpoints) — usable both by the physure team for
production scripts and by external lab/research users who expect an interactive debugging experience.

LAB-READY is a milestone on the way to the v1.0 LTS bar (frozen grammar, cross-target codegen
equivalence, fuzzing, plugin sandboxing — tracked separately), not a replacement for it.

---

## 1. Executive Summary

Six subsystems, developed as **independent, parallel tracks** rather than a strict sequence — each
has its own exit criteria and can merge on its own timeline:

- **Track A — Loops**: a `for`-expression (vectorized, functional) and a `while`-statement
  (imperative, for convergence loops like Newton's method) added to the PHS grammar, interpreter, and
  all three codegen targets (Python/Java/Rust).
- **Track B — Concurrency**: transparent data-parallelism (`rayon`) under existing array builtins and
  the new `for`-expression, plus an explicit `parallel_map(fn, vector)` builtin for user-defined
  functions across independent work items (parameter sweeps, Monte Carlo trials).
- **Track C — Breakpoints**: a `DebugHook` trait in the interpreter that both a new CLI debugger
  (`phs debug script.phs`) and, later, a DAP adapter over `physure-lsp` can consume — built once,
  staged across two consumers instead of duplicated.
- **Track D — Incremental LSP Evaluation**: replace `physure-lsp`'s current full-file
  reparse-and-re-execute on every keystroke with a per-statement dependency graph, so editing one
  variable only re-runs the statements that actually depend on it.
- **Track E — Compiled Export Artifacts**: turn an already-`export`ed PHS function into a
  standalone `.proto` interface contract and a compiled `.dll`/`.so`, reusing the existing Rust
  codegen backend plus a new FFI shim and build-scaffold step. Scoped tightly to the compile step
  for one function at a time — a curated multi-formula library/repository with hosted builds and
  generated docs is a related but explicitly out-of-scope future idea, not part of this track.
- **Track F — Decorators**: a generic `@name(args)` annotation syntax attachable to a function or
  variable, resolved against a registry so unknown names are a hard error, not a silent no-op. Track
  F itself adds zero runtime behavior — it's the shared parse-and-attach mechanism so future features
  (an `@export` alias for Track E, `@memoize`, `@deprecated`, a debugger breakpoint marker) each reuse
  one syntax instead of inventing a new keyword or statement per feature.

---

## 2. Core Architectural Approach

All three tracks touch the interpreter's execution model, so they share one design constraint: none
of them may compromise `physure-script`'s zero-FFI-dependency boundary from `physure-core`, and none
may weaken the "never silently drop a unit, conversion, or uncertainty" invariant — a parallel task
that panics on a unit mismatch must fail loudly, not disappear into a discarded thread result.

```mermaid
flowchart TD

subgraph track_a["Track A — Loops"]
  ast_loop["ast.rs: Expr::ForExpr, Statement::While"]
  parser_loop["parser.rs + .pest grammar rules"]
  interp_loop["interpreter.rs: tree-walk eval,\niteration cap"]
  codegen_loop["codegen/{python,java,rust}.rs:\nfor/while emitters"]
end

subgraph track_b["Track B — Concurrency"]
  rayon_dep["Cargo.toml: + rayon"]
  transparent["interpreter.rs: par_iter for\narray builtins + for-expr\nabove size threshold"]
  parallel_map["builtins.rs: parallel_map(fn, vector)\nfail-fast on first error"]
end

subgraph track_c["Track C — Breakpoints"]
  hook["interpreter.rs: DebugHook trait\non_statement(line, env) -> DebugAction"]
  cli_dbg["physure-cli: phs debug script.phs\n(breakpoints, step, inspect env)"]
  dap["physure-lsp (or new physure-dap):\nDAP adapter over DebugHook"]
end

ast_loop --> parser_loop --> interp_loop --> codegen_loop
rayon_dep --> transparent
rayon_dep --> parallel_map
interp_loop -.->|"loop iterations are debug-hook\ncheckpoints too"| hook
hook --> cli_dbg
hook --> dap

subgraph track_d["Track D — Incremental LSP Evaluation"]
  depgraph["lsp: per-statement dependency graph\n(reads/writes derived from AST, no execution)"]
  diff["lsp: on_change diffs old vs new\nstatement list against the graph"]
  rerun["lsp: re-run only changed statements\n+ their transitive dependents"]
end

parser_loop -.->|"same AST shape feeds\nstatement-level dependency edges"| depgraph
depgraph --> diff --> rerun

subgraph track_e["Track E — Compiled Export Artifacts"]
  proto_gen["codegen/proto.rs: ProtoGenerator\n(Request/Response + rpc Compute)"]
  ffi_shim["codegen/rust.rs output +\nnew FFI shim (flat f64, unit baked in)"]
  scaffold["cdylib scaffold + cargo build --release\n-> .dll / .so"]
end

ast_loop -.->|"Statement::Export names\nthe compile target"| proto_gen
codegen_loop -.->|"RustTranspiler function body\nreused, wrapped in shim"| ffi_shim
ffi_shim --> scaffold

subgraph track_f["Track F — Decorators"]
  decorator_grammar["phs.pest: decorator rule,\n\"@\" is unused today"]
  decorator_ast["ast.rs: DecoratorNode,\nFunctionDefNode/AssignmentNode.decorators"]
  decorator_registry["resolver.rs: known_decorators\nregistry, unknown name = hard error"]
end

decorator_grammar --> decorator_ast --> decorator_registry
decorator_registry -.->|"future sugar, e.g. @export\nas an alias for Statement::Export"| proto_gen
```

---

## 3. Track A — Loops

- **Grammar**: `Expr::ForExpr { var: String, iterable: Box<Expr>, body: Box<Expr> }` — evaluates to a
  `PhsValue::Vector` (one result per iteration), consuming a `Range` or `Vector`. `Statement::While {
  cond: Expr, body: Vec<Statement> }` for convergence loops that need mutable rebinding across
  iterations (`x = x + 1`).
- **Scoping decision**: a `while` body's assignments persist across iterations; a name assigned only
  inside the loop does not leak past it unless it already existed in the enclosing scope. (Confirm
  this matches user expectation before implementation — it's the one new semantic PHS doesn't have
  a precedent for.)
- **Safety**: `while` gets a configurable max-iteration cap so a non-converging script fails with a
  clear "loop did not converge after N iterations at line L" error instead of hanging.
- **Codegen**: `for`/`while` emitters added to all four transpile targets (Python, JavaScript/
  TypeScript, Java, Rust), with **execution-equivalence tests** (run the same script through the
  interpreter and through each transpiled target, assert matching numeric/uncertainty output) rather
  than the string-matching style of the current `codegen/tests.rs`.

## 4. Track B — Concurrency

- **Dependency**: add `rayon` to `physure-script/Cargo.toml` (not `tokio` — that's the LSP/web
  server's async I/O and is unrelated to compute parallelism).
- **Transparent parallelism**: array builtins (`gradient`, `trapz`, Monte Carlo uncertainty sampling)
  and the Track A `for`-expression switch to `rayon::par_iter` above a tunable size threshold. No PHS
  syntax changes.
- **`parallel_map(fn, vector)`**: explicit builtin for running a user-defined PHS function across a
  vector's elements on the `rayon` thread pool — for independent, expensive calls (parameter sweeps,
  Monte Carlo trials) that transparent parallelism can't reach.
- **Error semantics**: **fail-fast**. The first element that panics (unit mismatch, domain error)
  cancels the remaining work and surfaces which index failed and why. Collecting all per-element
  failures is a possible follow-up once Track A/C's error-handling story (catchable errors) exists,
  not a LAB-READY requirement.
- **Determinism test**: parallel and sequential paths must produce bit-identical results on the same
  input.

## 5. Track C — Breakpoints

- **`DebugHook` trait** (interpreter.rs):
  ```rust
  pub trait DebugHook: Send + Sync {
      fn on_statement(&self, ctx: &DebugContext) -> DebugAction;
  }
  pub struct DebugContext<'a> {
      pub line: usize,
      pub call_stack: &'a [StackFrame],  // fn name + call-site line, innermost last
      pub env: &'a HashMap<String, PhsValue>,  // the live, cloned-and-overlaid env visible here
  }
  pub struct StackFrame {
      pub fn_name: String,
      pub call_site_line: usize,
      pub declared: HashSet<String>,  // params ∪ names first assigned in this fn's body_stmts —
                                       // computed once from the AST when the frame is pushed
                                       // (static, like Track D's read/write analysis), not
                                       // re-derived by diffing HashMaps at every pause
  }
  pub enum DebugAction { Continue, StepInto, StepOver, StepOut, Pause }
  ```
  Called between statements and loop iterations. No-op by default — zero cost when not debugging.
  `StepOver`/`StepOut` require the interpreter to carry a `Vec<StackFrame>` through PHS function
  calls — today the Rust call stack and the PHS call stack are the same depth with nothing separate
  to inspect, so this is the one real piece of interpreter plumbing this track adds.

  **Why `declared` has to be tracked explicitly**: `call_function_node` builds each call's env by
  cloning whatever was visible at the call site and overlaying params
  ([interpreter/expressions.rs:444](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/interpreter/expressions.rs#L444)) — there is no parent-scope
  pointer to walk. So `ctx.env` inside a one-line helper contains the helper's params *and* every
  global, indistinguishably, in the same flat map. `StackFrame::declared` is what lets the debugger
  tell them apart without re-walking values: a name is local to a frame if it's a parameter or is
  the target of an `AssignmentNode`/`FunctionDef` somewhere in that function's `body_stmts` — both
  derivable statically from the `FunctionDefNode` the moment the frame is pushed, no execution
  needed.

- **Breakpoint kinds**: line breakpoints alone aren't enough once Track A loops exist — pausing on
  every one of 10,000 `for`-iterations is useless. Core kinds: **line** (`--break 42`),
  **conditional** (`--break 42:'v > 100 m/s'`, evaluated against the paused env before pausing), and
  **function-entry** (`--break-fn simulate`).

- **Inspection, not just `Display`**: a paused variable must decompose fully — measure, type, unit,
  prefix, dimension, and uncertainty are separate, explicit fields, not a formatted string. Grounded
  directly in what `physure-core` already exposes:

  ```rust
  pub enum ScopeKind {
      Global,                                    // present in `self.env` at the script's top level
      Local { owner_fn: String, frame_depth: usize },  // a param, or first assigned inside a
                                                         // call frame — found by walking
                                                         // `DebugContext.call_stack` innermost-out
                                                         // for the first frame whose `declared`
                                                         // set contains this name
  }
  pub struct Inspection {
      pub name: String,
      pub kind: ValueKind,             // Scalar | Vector(len) | Matrix(rows, cols)
                                        // | Function | Equation | Bool | String
      pub scope: ScopeKind,            // applies uniformly to vars and funcs — a nested
                                        // `FunctionDef` inside a function body is Local too
      pub measure: Option<f64>,        // the magnitude, when kind carries one
      pub unit_display: Option<String>,        // e.g. "km/h" as written
      pub prefix: Option<(String, f64)>,       // ("kilo", 1e3) — reverse-looked-up from
                                                // RationalUnit::scale against the active
                                                // UnitRegistry's registered prefixes
      pub dimension: Vec<(String, i64, i64)>,  // non-zero (symbol, numer, denom) pairs from
                                                // DimVector's SI_ORDER = [L, M, T, I, Θ, N, J, A, $]
      pub uncertainty: Option<UncertaintySummary>,  // std_dev + backend (gaussian/MC/unscented)
      pub detail: ValueDetail,         // Function -> params + declared param units;
                                        // Equation -> lhs/rhs symbolic forms;
                                        // Vector/Matrix -> per-element Inspection, truncated
                                        // for anything past a few elements with an indexed
                                        // drill-down (`inspect v[3]`)
  }
  ```

  `inspect <name>` is a distinct debugger command from `print <name>` (which stays a one-line
  `PhsValue::Display`-style value, useful for quick checks). `inspect` is the "decompose everything"
  command this track exists for.

- **CLI debugger** (`physure-cli`): `phs debug script.phs` — commands: `print <name>` /
  `print <expr>` (evaluates an arbitrary PHS expression in the paused scope, e.g. `print v => km/h`),
  `inspect <name>`, `locals` (only names in the innermost `StackFrame::declared` — params and
  first-assigned-here vars/functions), `globals` (names present in `self.env` at the top level,
  i.e. `ScopeKind::Global`), `backtrace` (the frame stack, each frame's `declared` names listed
  under it), `break <line>[:cond]` / `break fn <name>`, and `step`/`next`/`finish`/`continue`
  mapping to the four `DebugAction`s. Splitting `locals`/`globals` instead of one flat dump matters
  precisely because `ctx.env` is a full clone-and-overlay of everything visible at the call site
  ([interpreter/expressions.rs:444](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/interpreter/expressions.rs#L444)) — without the split, `locals`
  inside a one-line helper would print every global too. First consumer of `DebugHook` — proves the
  hook and `Inspection` design before DAP commits to them.

- **Interaction with Track A/B**: a breakpoint inside a `parallel_map` closure forces that call to
  **fall back to sequential execution** for the debug session — pausing one of N `rayon` worker
  threads while the rest race ahead isn't a coherent debugging experience, and silently ignoring the
  breakpoint would be worse. This is an explicit, documented rule, not emergent behavior.

- **DAP adapter** (`physure-lsp` or a new `physure-dap` crate): `setBreakpoints` (incl. the
  conditional-expression field), `stackTrace` (from `StackFrame`), `scopes` (DAP's own
  Local/Global distinction maps directly onto `ScopeKind` — one `scopes` response per frame's
  `declared` set plus one "Globals" pseudo-scope), `variables` (from `Inspection`, which is already
  shaped as an expandable tree — measure/unit/prefix/dimension/uncertainty as child variables),
  `continue`/`next`/`stepIn`/`stepOut`, `evaluate` (same code path as CLI `print <expr>`).
  **Staged, not blocking**: DAP is a stretch item and does not gate the LAB-READY milestone; the CLI
  debugger with `inspect` does.

---

## 6. Track D — Incremental LSP Evaluation

Confirmed directly in the current code: `physure-lsp`'s `on_change` builds a fresh
`PhsInterpreter::default()` and calls `run_statement` for every statement in the file, on every
keystroke, with no caching ([main.rs:409-447](https://github.com/Alexisrx96/physure/blob/main/physure-lsp/src/main.rs#L409-L447)). This track
replaces that with dependency-aware incremental re-evaluation, scoped to a single open document.

- **Dependency graph**: for each statement, its *writes* (the name an `AssignmentNode`/`FunctionDef`
  binds) and *reads* (every `Expr::Identifier` in its expression tree) are both derivable statically
  from the AST — no execution needed to build the graph.
- **Position-aware edges, not "everything after this line"**: PHS allows rebinding the same name
  later in the file, so a read at statement position *P* must resolve to the *nearest preceding*
  write of that name before *P*, not just "any statement that writes it." Get this wrong and an edit
  either re-runs too much (defeats the purpose) or too little (stale diagnostics — worse than the
  current full-rerun baseline).
- **On-change algorithm**: reparse the buffer (parsing itself is cheap — the cost being cut is
  re-*execution*), structurally diff the new statement list against the previous one to find
  changed/added/removed statements, walk the dependency graph forward from those to find every
  transitive dependent, and re-run only that set — in original file order, against a persisted
  `Environment` rather than a fresh one each time.
- **Scope for this track**: single-document, in-`physure-lsp`. Cross-file incrementality (edits to
  a `.phs` module invalidating documents that `import` it, via the `FsModuleResolver`) is a real
  follow-on but out of scope for LAB-READY — flagged here so it isn't lost, not committed to.
- **Build-vs-adopt**: `salsa` (the incremental-computation crate `rust-analyzer` itself uses for
  exactly this problem — full-file-reparse-on-keystroke) is the proven fit if the query graph grows
  complex later. Start with a hand-rolled graph for LAB-READY — PHS's dependency structure is far
  simpler than a type-checker's, and a bespoke `HashMap<String, StatementId>` is enough to prove the
  approach before pulling in a general-purpose incremental-query engine.
- **Test**: a script with one statement whose result nothing downstream reads; edit it; assert only
  that one statement re-executed (via an execution-count counter per statement in the test harness,
  not wall-clock timing). A second test proves the rebinding case: two writes to the same name with
  reads in between, edit the *first* write, assert only the correctly-scoped dependents re-run.

---

## 7. Track E — Compiled Export Artifacts (`.proto` + `.dll`/`.so`)

Scope: compile an already-`export`ed PHS function into a standalone, cross-language-callable
artifact. This is deliberately *not* the curated "formula repository" idea (a browsable library of
many documented formulas with hosted on-demand builds) — that stays parked for later; Track E is
only the single-function compile step it would eventually sit on top of.

- **Trigger**: the existing `Statement::Export`/`ExportNode` ([ast.rs:39](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/ast.rs#L39))
  already names which symbols are meant to be visible outside the script. Track E reuses it as the
  compile target list rather than inventing new annotation syntax.
- **`.proto` generation**: a new `codegen::proto::ProtoGenerator`, a sibling to `python.rs`/`rust.rs`/
  `java.rs` under the same `CodeGenerator` shape, emits one `Request`/`Response` message pair
  derived from `param_units` and the inferred return type, plus `rpc Compute(Request) returns
  (Response);`. This is text templating only — no `prost`/protobuf runtime dependency, since PHS
  never parses or serializes an actual protobuf wire message itself. The `.proto` is a portable
  contract for external consumers to generate bindings against in their own language; it is
  documentation-and-interop, not a network service PHS itself runs.
- **`.dll`/`.so` generation**: `RustTranspiler::generate_function_def` already emits a pure-Rust
  function, but its signature takes `Quantity` structs ([rust.rs:64](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/codegen/rust.rs#L64)),
  which is not `extern "C"`-safe. Track E adds an FFI shim: flat `f64` in, `f64` out, with each
  parameter's unit fixed at generation time (baked in, not passed at runtime — unit-safety is
  already a compile-time property once the export's declared units are chosen). The shim
  constructs a `Quantity` from the incoming `f64` plus that fixed unit, calls the transpiled
  function, and returns `.value.mean()`. The shim and generated function are scaffolded into a
  throwaway single-function crate (`Cargo.toml` with `crate-type = ["cdylib"]` — the same
  crate-type `physure-java`/`physure-python` already use at the workspace level), then built via
  `cargo build --release`. Windows produces a `.dll`, Linux a `.so` (the same mechanism produces a
  `.dylib` on macOS for free, though not separately requested).
- **Fallible exports (Track F `@requires`/`@ensures`/`@range`)**: a function carrying contract
  decorators can fail its checks at runtime, and a compiled-artifact consumer who never sees the PHS
  source still has to be able to detect that — silently returning a value as if the check passed
  would be exactly the kind of silent failure this project's invariants forbid. The C ABI can't
  throw, so the shim returns a plain `#[repr(C)]` struct instead of a bare `f64`:
  ```rust
  #[repr(C)]
  pub struct SimulateResult { pub value: f64, pub ok: bool }

  #[no_mangle]
  pub extern "C" fn simulate(v: f64, m0: f64) -> SimulateResult { /* ... */ }

  #[no_mangle]
  pub extern "C" fn simulate_last_error() -> *const std::os::raw::c_char { /* thread-local,
      valid until the next call on this thread — same idiom as errno/GetLastError */ }
  ```
  `.ok == false` means a `@requires`/`@ensures` check failed; the caller reads `simulate_last_error()`
  for which one and why. Functions with no contract decorators keep returning a bare `f64` — this
  struct-return shape only applies once a function actually has something that can fail.
- **CLI**: `phs export script.phs --fn simulate --target proto|native|all [-o <dir>]` writes
  `simulate.proto` and/or `simulate.dll`/`simulate.so` next to the script or to `-o`.
- **Explicitly out of scope for Track E**: a curated multi-formula library/registry, a hosted
  on-demand build service, or a generated docs site — the parked "formula repository" idea. Track E
  produces the artifacts; it does not host or catalog them.
- **Test**: round-trip — export a known function (e.g. `f(x) = x^2 * 1.0 m/s^2` from the Example
  Usage section), compile it to `.so`/`.dll` in CI, load it via `libloading` (a dev-dependency only,
  never shipped) from a test harness, call it with a known input, and assert the output matches the
  interpreter's own evaluation of the same function within floating-point tolerance. A second case
  exports a function carrying `@requires`/`@ensures`, calls it with both a valid and a
  contract-violating input, and asserts `.ok`/`simulate_last_error()` match the interpreter's own
  pass/fail for the same inputs.

---

## 8. Track F — Decorators

A generic `@name(args)` annotation mechanism attachable to a function or a variable, purely so future
tracks stop reinventing a bespoke keyword/statement every time they need to mark something. `@` is
completely unused in the grammar today ([phs.pest:1-3](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/phs.pest#L1-L3) — only
`#`/`//` are taken, as comment markers), so it's free to claim.

- **Grammar**: one new rule, tried before the bare alternatives it wraps:
  ```pest
  decorator      = { "@" ~ identifier ~ ("(" ~ (expr ~ ("," ~ expr)*)? ~ ")")? ~ _nl }
  decorated_stmt = { decorator+ ~ (function_def | assignment) }
  stmt           = { import_stmt | export_stmt | decorated_stmt | function_def | assignment_fn
                    | assignment | guard_if_stmt | return_stmt | raw_block | expr }
  ```
- **AST**: additive, not a new `Statement` variant — a `Vec<DecoratorNode>` field on the two node
  types decorators can apply to, `#[serde(default)]` for backward compatibility (matching the
  existing convention for `param_units`/`uncertainty_lower`):
  ```rust
  pub struct DecoratorNode {
      pub name: String,
      pub args: Vec<Expr>,
  }
  // on FunctionDefNode and AssignmentNode:
  #[serde(default)]
  pub decorators: Vec<DecoratorNode>,
  ```
- **Resolved, not silently ignored**: a `known_decorators: HashSet<&'static str>` registry (resolver.rs)
  is checked when a decorated statement is resolved. An unrecognized `@typo_name` is a hard resolve
  error, not a no-op — the same "never silently drop something" discipline the project already
  applies to units gets applied to metadata here too.
- **Semantics are opt-in per consumer, not built into Track F's mechanism**: parsing/attaching/name
  validation lives in Track F; the *behavior* behind each decorator name is implemented by whichever
  feature declares it. The mechanism is reused, not duplicated, per decorator.

### Phase 1 decorator catalog (approved, built alongside Track F)

Aimed squarely at the multi-disciplinary split this project runs on: some consumers only ever call
the compiled artifact (Track E) and need a stability guarantee; others own the physical/business
correctness of the formula and need to attach that correctness declaratively, next to the signature,
without touching the algorithm body or writing it into engineering's control flow by hand.

- **`@requires(cond, msg)` — precondition.** Evaluated against the call's local env (params already
  bound) before the function body runs. On failure, raises `PhysureError::ContractViolation { decorator,
  message }` (new variant, additive to `physure-core/src/error.rs`'s existing enum) — a distinct,
  identifiable error, not folded into the catch-all `Generic` variant.
- **`@ensures(cond, msg)` — postcondition.** Evaluated after the body produces a value, before it's
  returned, with the result bound under the name `result` in a local-env overlay so the condition can
  reference it (e.g. `@ensures(result > 0.0 J, "energy must be positive")`). Same failure path as
  `@requires`. *Caveat, low-priority*: if a function has a parameter literally named `result`, that
  parameter shadows the bound value inside `@ensures` — resolver flags this at resolve time rather
  than silently picking one.
- **`@range(param, min, max)` — parameter bound, structured.** Sugar over two `@requires`-equivalent
  checks (`param >= min`, `param <= max`), not a separate enforcement path — reuses `@requires`'s
  machinery exactly, so there's one code path to get right, not two. Kept as its own decorator (rather
  than requiring authors to spell out the two-sided check by hand) because the structured
  `(param, min, max)` tuple is also what Track E's `.proto`/`.md` generator and Track C's `inspect`
  reuse to display "valid range" — the sugar exists for the data shape, not just typing convenience.
- **`@stable` / `@experimental` — contract stability, no runtime check.** Pure metadata: Track E's
  `.proto`/`.md` generation surfaces which guarantee applies, so a consumer who only calls the
  compiled artifact knows whether its signature is safe to build tooling against. Resolver rejects a
  function carrying both on the same definition.
- **Propagation is mandatory, not optional**: `@requires`/`@ensures`/`@range` are transpiled into the
  Track E FFI shim exactly once, via the same `RustTranspiler::generate_expr` already used for the
  function body (comparisons already lower to plain `FunctionCall`s like `op_<`
  ([parser.rs:339](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/parser.rs#L339)), so no new expression machinery is needed) —
  see Track E's "Fallible exports" note. A contract that only the interpreter enforces and the
  compiled artifact silently ignores would defeat the reason these decorators exist.

**Explicitly out of scope for this catalog**: validations or decisions that depend on *when and how
data is gathered*, or on splitting a calculation across multiple steps/data sources per business
rule, or raising/warning based on ranges that are about *system integration* rather than the formula
itself. Those are dev-team-owned integration concerns, not formula-correctness concerns — a different
problem from what `@requires`/`@ensures`/`@range` solve, parked here so it isn't lost, not designed.

**Deprioritized, not scoped now** (illustrative only, no decision made): `@memoize`, `@deprecated`,
`@owner("team")`, a `@breakpoint` marker for Track C, `@export` as sugar for Track E's compile
trigger (the existing `export` statement is untouched either way).

- **Zero cost when unused**: `decorators: Vec<DecoratorNode>` is inert data on the AST node; nothing
  executes unless a consuming feature explicitly walks it.
- **Test**: parse round-trip (single and stacked decorators attach in source order to both a
  `FunctionDefNode` and an `AssignmentNode`); resolver rejects an unregistered decorator name; each
  Phase 1 decorator gets an interpreter-level pass/fail test plus the Track E FFI round-trip test
  described there.

---

## 9. Milestone Checklists

Tracks are independent — check off in any order, on any timeline.

- [x] **Track A: Loops** — see
      [`docs/superpowers/specs/2026-08-12-track-a-loops-design.md`](superpowers/specs/2026-08-12-track-a-loops-design.md)
      and [`docs/superpowers/plans/2026-08-12-track-a-loops.md`](superpowers/plans/2026-08-12-track-a-loops.md).
      Codegen emitters landed for all **four** transpile targets, not just the three named in this
      section's original sketch — Python, JavaScript/TypeScript, Rust, and Java. The execution-parity
      test suite's convergence-loop case (Newton's method) exposed real cross-backend bugs no prior
      test caught, all fixed in the same pass: the parser's desugared comparison calls (`op_<`, `op_>`,
      …) weren't translated to native operators by any backend; Rust and JS/TS redeclared rather than
      mutated a reassigned `while`-loop variable (both are block-scoped); Rust's arithmetic codegen
      moved rather than borrowed operands, breaking on `x + 2.0 / x`-style reuse; and `builtins.rs`'s
      `expr_to_string` plus the CLI's LaTeX renderer were both missing a `ForExpr` match arm.
  - [x] `Expr::ForExpr` grammar, AST, interpreter eval producing a `Vector` (pre-allocated with
        `Vec::with_capacity`, not grown one push at a time).
  - [x] `Statement::While` grammar, AST, interpreter eval with a 10,000-iteration cap and the scoping
        rule (assignments persist across iterations; a name assigned only inside the loop doesn't leak
        past it unless already bound in the enclosing scope).
  - [x] Python/JavaScript-TypeScript/Rust/Java codegen emitters for both constructs.
  - [x] Execution-equivalence tests (interpret vs. each transpiled target) for both constructs,
        including a convergence-loop (Newton's method) case.

- [x] **Track B: Concurrency** — see
      [`docs/superpowers/specs/2026-08-12-track-b-concurrency-design.md`](superpowers/specs/2026-08-12-track-b-concurrency-design.md)
      and [`docs/superpowers/plans/2026-08-12-track-b-concurrency.md`](superpowers/plans/2026-08-12-track-b-concurrency.md).
      One scope change from this section's original description, intentional and tracked
      below: `gradient`/`trapz` are **not** parallelized — `trapz` is a running accumulation
      that can't be parallelized without a parallel-reduce rewrite, and `gradient`'s
      per-element cost is too small for thread-dispatch to pay off. Threshold is configurable
      via `physure.conf`'s `[Settings] parallel_threshold` (default 10,000), not a bare
      constant or env var.
  - [x] Add `rayon` dependency to `physure-script`.
  - [x] Transparent `par_iter` parallelism for `for`-expression above `parallel_threshold`.
  - [x] `parallel_map(fn, vector)` builtin with fail-fast error semantics.
  - [x] Determinism test (parallel output == sequential output) and mid-batch-failure test.

- [x] **Track C: Breakpoints** — see
      [`docs/superpowers/specs/2026-08-13-track-c-breakpoints-design.md`](superpowers/specs/2026-08-13-track-c-breakpoints-design.md)
      and [`docs/superpowers/plans/2026-08-13-track-c-breakpoints.md`](superpowers/plans/2026-08-13-track-c-breakpoints.md).
      Three deviations from this section's original sketch, all found during implementation:
      - A prerequisite not in the original sketch, added during brainstorming: source-line
        tracking on `Program`/`FunctionDefNode`/`Statement::While` (none existed before this
        track), added as parallel `Vec<usize>` fields rather than wrapping every `Statement`
        variant, to keep the change additive.
      - Two `Inspection` fields deviate from the original sketch: `dimension` exposes
        `RationalUnit.dimensions` (registered base-unit symbols) directly rather than the
        unused `DimVector`/`SI_ORDER` scheme this section named; `prefix` is best-effort,
        resolvable only for a single non-compound unit symbol.
      - Found during the integration task's own review of Track B's parallel paths: the
        sequential-fallback rule applies to *both* rayon entry points Track B added —
        `parallel_map` and the `for`-expression's parallel branch above `parallel_threshold` —
        not just `parallel_map` as this section originally scoped it, since both share the same
        `call_stack`-corruption risk under concurrent debug checkpoints.
  - [x] `DebugHook` trait + `Vec<StackFrame>` call-stack tracking wired into the interpreter's
        statement/loop-iteration dispatch.
  - [x] Line, conditional, and function-entry breakpoints.
  - [x] `Inspection` struct: measure, `ValueKind`, `ScopeKind` (Global vs. Local + owning frame),
        unit, prefix (reverse-looked-up via `UnitRegistry::split_prefix`), dimension
        (`RationalUnit.dimensions`), uncertainty, and per-kind `ValueDetail` (function
        params/units, equation lhs/rhs, recursive Vector/Matrix elements).
  - [x] `StackFrame::declared` computed statically from each `FunctionDefNode` (params ∪ names
        assigned in `body_stmts`) when a call frame is pushed.
  - [x] `phs debug script.phs` CLI debugger: `print`, `inspect`, `locals`, `globals`,
        `backtrace`, `break`/`break fn`, step/next/finish/continue.
  - [x] `parallel_map` and `for`-expression sequential-fallback rule enforced whenever a debug
        hook is attached, not only when a breakpoint is set inside the parallel region.
  - [x] Scripted CLI-debugger test (set breakpoint, run, assert paused at right line, assert
        `inspect` output decomposes measure/unit/prefix/dimension/uncertainty correctly).
  - [ ] *(Stretch, non-blocking)* DAP adapter over the same `DebugHook`/`Inspection` types.

- [x] **Track D: Incremental LSP Evaluation** *(non-blocking for LAB-READY — editor responsiveness,
      not a language capability)* — see
      [`docs/superpowers/specs/2026-08-14-track-d-incremental-lsp-design.md`](superpowers/specs/2026-08-14-track-d-incremental-lsp-design.md)
      and [`docs/superpowers/plans/2026-08-14-track-d-incremental-lsp.md`](superpowers/plans/2026-08-14-track-d-incremental-lsp.md)
      for the design and executed plan. Several correctness subtleties were found and fixed
      during implementation and review, none present in the original sketch: a function call's
      callee name (`Expr::FunctionCall.name`) had to be treated as a read even though it's a bare
      `String`, not a nested `Identifier`, since resolution goes through `env.get(name)`; a
      `while` loop's body-assigned names were verified empirically to leak to the outer scope
      *unconditionally* (the original scoping-rule assumption in this section was wrong); a
      common-prefix statement (most notably a `FunctionDef`, whose free variables resolve lazily
      at call time, not definition time) can still be dirty and must not be exempted from
      dependency checks; a write-write ordering dependency exists that's distinct from the usual
      read-write kind — editing or deleting an earlier write to a name must also re-run a later,
      otherwise-unaffected write to the same name, or its value silently leaks through / never
      gets restored; and a decorator-conditional interpreter binding (`@ensures`'s synthetic
      `result`) must not be applied to decorators that don't actually bind it (`@requires`). A
      known, narrow, explicitly-documented gap remains for wildcard imports (`use * from mod`)
      whose target module changes — see the comment on `ImportSpecifier::Wildcard` in
      `incremental.rs` for the reasoning and upgrade path.
  - [x] Per-statement dependency graph (reads/writes from the AST via a pure walk, no execution
        needed to build it), position-aware for rebinding via a `last_writer` map built in one
        forward sweep.
  - [x] `on_change` diffs the statement list (common-prefix/suffix, line-number-insensitive) and
        re-runs only changed statements + transitive dependents, against a persisted
        `PhsInterpreter`.
  - [x] Execution-count test: editing an unread statement re-executes only itself (verified via
        exact dirty-index-set assertions, a stronger check than an execution counter).
  - [x] Rebinding-correctness test: editing an earlier write among two writes to the same name
        re-runs only the correctly-scoped dependents.
  - [ ] *(Follow-on, out of scope)* Cross-file incrementality via `FsModuleResolver`.

- [x] **Track E: Compiled Export Artifacts** *(non-blocking for LAB-READY — a distribution/interop
      capability, not a language execution capability)* — see
      [`docs/superpowers/specs/2026-08-12-track-e-compiled-exports-design.md`](superpowers/specs/2026-08-12-track-e-compiled-exports-design.md)
      and [`docs/superpowers/plans/2026-08-12-track-e-compiled-exports.md`](superpowers/plans/2026-08-12-track-e-compiled-exports.md)
      for the design and executed plan. Two scope changes from this section's original
      description, both intentional and tracked below:
      (1) `.proto` **and** `.md` generation are mandatory on every `phs export` run, not one of
      several `--target` choices — the CLI flag surface simplified to a single optional
      `--native`, in favor of a new `///` doc-comment language feature that feeds the `.md`;
      (2) contract propagation (`@requires`/`@ensures`) into the compiled shim, which Track F's
      own plan explicitly deferred to this track, is implemented via `generate_export_shim`
      reusing `RustTranspiler::generate_expr` — never a second, independently-written condition
      evaluator.
  - [x] `codegen::proto::ProtoGenerator`: Request/Response messages + `rpc Compute` per exported
        function, derived from `param_units`; `Response` gains `ok`/`error` fields only when the
        function carries `@requires`/`@ensures` (return-value unit is not statically known in PHS
        today, so the `Response.value` field is untyped `double` rather than unit-annotated).
  - [x] `///` doc-comment grammar/AST/parser (new prerequisite, not in the original sketch):
        `FunctionDefNode.doc: Option<String>`, rendered by `codegen::md::MdGenerator`.
  - [x] FFI shim generator (`RustTranspiler::generate_export_shim`): flat `f64` in/out, unit baked
        in at generation time, `@requires`/`@ensures` enforced via the shared `generate_expr` path,
        `<name>_last_error()` thread-local getter for the failing contract's message.
  - [x] Scaffold-and-build pipeline: throwaway `cdylib` crate, `cargo build --release`, artifact
        copied to the output dir; `physure-core` resolved via a `CARGO_MANIFEST_DIR`-derived
        absolute path baked into `phs` at its own compile time (not published, not vendored).
  - [x] `phs export` CLI subcommand (`--fn`, `--native`, `-o`; `.proto`/`.md` unconditional).
  - [x] Round-trip test: compiled `.so`/`.dll` output matches interpreter output for the same
        function and input, within floating-point tolerance, including a paired
        contract-violation case (`#[ignore]`d — real `cargo build --release`, run explicitly).
  - [ ] *(Parked, out of scope)* Curated multi-formula repository, hosted on-demand builds,
        generated docs site.

- [x] **Track F: Decorators** *(non-blocking for LAB-READY — a syntax/DRY foundation other tracks can
      adopt, not itself a language execution capability)* — see
      [`docs/superpowers/plans/2026-08-07-phs-decorators-track-f.md`](superpowers/plans/2026-08-07-phs-decorators-track-f.md).
      Two deviations from this section's original sketch: validation (unknown-name rejection,
      `@stable`/`@experimental` mutual exclusion, `@ensures`-vs-`result`-param collision) lives in a
      new `physure-script/src/decorators.rs` module rather than `resolver.rs`, modeled on the existing
      `validate_unit_shadowing` pass; and decorators ride directly on the existing `FunctionDefNode`/
      `AssignmentNode` structs (no new AST node type), so every consumer that already threads those
      structs — including `PhsValue::Function` — carries decorators for free.
  - [x] `decorator`/`decorated_stmt` grammar rules (`.pest`), tried before the bare `function_def`/
        `assignment` alternatives.
  - [x] `DecoratorNode` + `decorators: Vec<DecoratorNode>` on `FunctionDefNode` and `AssignmentNode`
        (`#[serde(default)]`, backward compatible).
  - [x] `KNOWN_DECORATORS` registry in `decorators.rs`; unregistered decorator name is a hard error.
  - [x] Parse round-trip test (single + stacked decorators, both node kinds).
  - [x] `PhysureError::ContractViolation { decorator, message }` variant added to
        `physure-core/src/error.rs`.
  - [x] `@requires(cond, msg)` / `@ensures(cond, msg)`: interpreter evaluation (params bound for
        `@requires`; `result` bound for `@ensures`) + validation check for a `result`-name collision.
  - [x] `@range(param, min, max)` implemented as sugar over two `@requires`-equivalent checks, with
        the structured tuple preserved for Track E/Track C consumption.
  - [x] `@stable` / `@experimental`: metadata only, confirmed inert at call time; validation rejects
        both on the same definition.
  - [x] `@requires`/`@ensures`/`@range` transpiled into the Track E FFI shim via
        `RustTranspiler::generate_expr` (see Track E's "Fallible exports" + `SimulateResult`-style
        struct return) — implemented as part of Track E's own plan, per the cross-reference already
        noted in Track E's section above.
  - [x] Interpreter pass/fail test per Phase 1 decorator; Track E FFI round-trip test (paired with
        Track E's own checklist item) confirms compiled and interpreted pass/fail agree.
  - [ ] *(Open, deferred)* Whether `@export` becomes sugar for Track E's compile-target marking —
        decided when Track E revisits it, not by Track F.
  - [ ] *(Parked, out of scope)* Dev-time/system-integration validation (data-gathering orchestration,
        multi-step business-rule-driven calc splitting, system-interaction range warnings) — a
        different problem from formula-correctness contracts, not designed here.

**LAB-READY is reached when Track A, Track B, and the non-stretch part of Track C are all merged with
tests green in `tests.yml` CI.** ✅ **Reached** — all three merged. Tracks D, E, and F improve editor
experience, distribution/interop, and future extensibility respectively; all three are also merged,
and none of them gate the milestone. The only items left open in this document are explicitly
non-blocking stretch/follow-on work: Track C's DAP adapter and Track D's cross-file incrementality.

---

## 10. Example Usage (once implemented)

```phs
# Track A: for-expression producing a Vector
temps = for t in 0.0 s .. 10.0 s {
    t^2 * 1.0 m/s^2
}

# Track A: while-statement for a convergence loop
guess = 1.0
guess = (guess + 2.0 / guess) / 2.0 while abs(guess^2 - 2.0) > 1e-9

# Track B: explicit parallel map over a parameter sweep
results = parallel_map(fn(k) = simulate(k), linspace(0.1, 10.0, 1000))

# Track F: contract decorators — domain-expert-owned validation, stacked in
# source order, kept separate from the algorithm body
@stable
@range(v, 0.0 m/s, 2.998e8 m/s)
@ensures(result > 0.0 J, "kinetic energy must be positive")
fn kinetic_energy(m, v) = 0.5 * m * v^2

@experimental
@requires(t > 0.0 K, "temperature must be positive")
fn boltzmann_factor(e, t) = exp(-e / (1.380649e-23 J/K * t))
```

```bash
# Track C: CLI debugger
phs debug orbit_sim.phs --break-fn simulate
> continue
Paused at line 42 (orbit_sim.phs), in simulate() called from line 58
> locals
  k       (param)              : 2.5e-3 N*s/m
  v       (assigned line 40)   : 7.67e3 m/s
> globals
  G       : 6.674e-11 m^3/(kg*s^2)
  simulate: Function(k, m0) -> Quantity
> inspect v
v
  kind        : Scalar
  scope       : Local { owner_fn: "simulate", frame_depth: 1 }
  measure     : 7.670e3
  unit        : km/h
  prefix      : kilo (10^3) on base unit "m"
  dimension   : L^1 T^-1        (L=1, M=0, T=-1, I=0, Θ=0, N=0, J=0, A=0, $=0)
  uncertainty : none
> inspect G
G
  kind        : Scalar
  scope       : Global
  measure     : 6.674e-11
  unit        : m^3/(kg*s^2)
  prefix      : none
  dimension   : L^3 M^-1 T^-2   (L=3, M=-1, T=-2, all others 0)
  uncertainty : none
> step
> print v
7.67e3 m/s
```

---

## 11. References

| Feature | Reference | Link |
| :--- | :--- | :--- |
| Data-parallelism model | `rayon` — data parallelism library for Rust | <https://docs.rs/rayon> |
| Debug protocol (Track C stretch) | Debug Adapter Protocol Specification | <https://microsoft.github.io/debug-adapter-protocol/> |
| Incremental computation (Track D) | `salsa` — the incremental-computation crate `rust-analyzer` uses to avoid full-file recompute on every keystroke | <https://github.com/salsa-rs/salsa> |
| IDL contract (Track E) | Protocol Buffers Language Guide (proto3) — text-templated `.proto` output, no runtime dependency | <https://protobuf.dev/programming-guides/proto3/> |
| FFI test loader (Track E, dev-only) | `libloading` — loads a `.dll`/`.so` at runtime for the round-trip test, never shipped | <https://docs.rs/libloading> |
