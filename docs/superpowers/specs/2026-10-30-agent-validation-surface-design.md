# A validation surface for agents and language models (N5) — design

**Date:** 2026-10-30
**Status:** approved by the maintainer ("do it", 2026-10-30) after the design below was presented in chat; N5a and N5b merged into local main; N5c (`phs mcp`) built on `feat/phs-mcp`. N5 is complete.
**Base:** local `main` @ `b3e83f4`
**Follows:** the draft `2026-10-30-draft-llm-validation-surface.md` (next step N5 of `2026-10-30-post-a1-inventory.md`), which builds on `docs/LLM_SLM_SYNTAX_EXPOSURE_SPEC.md`.

## Goal

Give an agent a deterministic way to ask "is this equation dimensionally right, and what is its value with units and uncertainty?" and get a structured answer instead of free text, so a model checks its work against the engine instead of recalling numbers. The engine must be safe to run on text an outside caller wrote.

## Decisions (maintainer, 2026-10-30)

| Question | Decision | Rejected |
|---|---|---|
| Which surface | **CLI and an MCP server, both now** (`phs eval`, `phs unit`, `phs check -`, `phs mcp`) | CLI only; documenting `phs --daemon`/`phs serve` |
| Where the safe subset is enforced | **In the interpreter** (a sandboxed constructor in `physure-script`), so every surface and binding gets the same guarantee | A pre-scan and a thread timeout in the CLI |
| Monte Carlo and determinism | **Seeded**: a scoped seed in `physure-core`; the same input gives the same output | Refuse Monte Carlo in safe mode; leave it and document it |

## What the code says (the reasons for the sandbox)

Four things a PHS program can do that an outside caller must not be able to do, each verified in the source:

1. **Read files.** The default interpreter resolves `import a.b` against `./a/b.phs` (`PhsInterpreter::default` builds an `FsModuleResolver::default()`, base `.`; `resolver.rs`).
2. **Load native code.** `PhsInterpreter::with_base_dir(dir)` sets `plugin_base_dir`, and a `use` of a name in `<dir>/ext/` dlopens a library (`statements.rs::resolve_use`, `plugin.rs`). With no base dir nothing is loaded.
3. **Write files.** The 3D mesh export builtin writes `std::fs::write(&filename, ...)` (`builtins/plot.rs`, the only `std::fs` write reachable from a builtin).
4. **Kill the process.** `fn f(x) = f(x)` then `f(1)` overflows the native stack and aborts the whole process (checked with the release `phs`). Every user-function call goes through `call_function_node_at` (`interpreter/expressions.rs`) and nothing bounds the depth. The only loop bounds that exist are `max_loop_iterations` (ranges, `linspace`, `parallel_map`), `max_pipeline_steps`, a `while` cap of 10,000 iterations, and the parser's `MAX_NESTING_DEPTH` of 64.

Also relevant: Monte Carlo draws from `rand::thread_rng()` in two places (`physure-core/src/uncertainty/monte_carlo.rs`), so it is the only source of non-determinism. PHS text cannot select it (no script or test mentions it); the host's `physure.conf` or `set_propagation_mode` does, and the conf is read from the working directory, so an agent evaluation on a machine configured for Monte Carlo would not repeat. A `for` expression over `parallel_threshold` (10,000) elements and `parallel_map` run on rayon, which would reorder draws.

## The pieces

| | Piece | Crate | Lands as |
|---|---|---|---|
| **N5a** | Sandbox, call-depth limit, deadline, output cap, seeded Monte Carlo | `physure-script`, `physure-core` | its own branch |
| **N5b** | `phs eval`, `phs unit`, `phs check -`; the JSON report | `physure-script` (report types), `physure-cli` | its own branch, after N5a |
| **N5c** | `phs mcp`, a stdio MCP server | `physure-cli` | its own branch, after N5b |

Each stage gets its own plan (`docs/superpowers/plans/`) when it starts, and is mergeable on its own.

## N5a — the sandbox

`physure_script::Limits` (public, `Clone`, `Debug`):

| Field | Default (unrestricted) | Sandboxed |
|---|---|---|
| `max_call_depth: usize` | 1,000, **applies to every interpreter** | 200 |
| `deadline: Option<Instant>` | `None` | `Some(start + timeout)` |
| `max_output_bytes: Option<usize>` | `None` | `Some(1 MiB)` |
| `max_stack_bytes: Option<usize>` | `None` | `Some(48 MiB)` |
| `allow_files: bool` | `true` | `false` |
| `sequential: bool` | `false` | `true` |

- `PhsInterpreter` gets a `limits: Limits` field (default as above) and `PhsInterpreter::sandboxed(Limits)`: an in-memory resolver with no modules (an `import` of anything but a builtin domain fails like an unknown module), no base dir (no plugin ever loads), `allow_files = false`, `sequential = true`.
- **Call depth.** `call_function_node_at` counts depth in a thread-local with a guard that decrements on every exit path. Past `max_call_depth` it returns an error with a stable prefix (`call depth limit`).
- **Stack budget.** Depth alone does not bound the stack: measured with a probe, a PHS call costs **15 to 20 KB in release and about 100 KB in debug**, and a function body with 40 nested parentheses multiplies that. So `check_budget()` (run at each `eval_expr`) also compares the address of a local with a floor set when the statement started (`StackScope`, in `eval_statement_with_env`); past `max_stack_bytes` it errors with the prefix `stack limit`. `run_sandboxed(limits, f)` runs `f` on a 64 MiB thread so the 48 MiB budget has headroom for the native frames between two checks. The unrestricted default (1,000 deep, no budget) is a net for hosts with a big stack only: a 1 MiB main thread (Windows) overflows near 60 levels and 8 MiB near 400, as before. Running the CLI's scripts on a big-stack thread would close that and is not part of N5.
- **Deadline.** `check_budget()` is called at each statement, each user-function call, each `while` iteration and each expression; it compares `Instant::now()` against the deadline only when one is set. Past it: an error with the prefix `time limit`. It is cooperative: a single native operation that never returns to the interpreter is not interrupted; the one-process-per-call design of N5b/N5c is what bounds that (below).
- **Files.** The mesh-export builtin returns an error with the prefix `forbidden:` when `allow_files` is false. No other builtin touches the filesystem.
- **Sequential.** `sequential` makes the `for`-expression and `parallel_map` take their existing single-thread branch, the one a debug session already uses.
- **Output cap.** Applied where results are rendered into a report (N5b); `Limits` carries the number.
- **`physure-core`.** `physure_core::uncertainty::with_seed(seed, f)`: while `f` runs on this thread, the two `thread_rng()` sites draw from `StdRng::seed_from_u64(seed ^ counter * golden)` with a per-scope counter instead, so a sequential evaluation is reproducible. Outside a scope nothing changes.
- **Not a promise of the sandbox:** memory exhaustion by a single native allocation, and CPU spent inside one native call. N5b/N5c bound those by running each evaluation in a child process killed after a hard timeout.

## N5b — the JSON

`physure_script::report`: `Report { ok, results, diagnostics }`, `serde::Serialize`.

```json
{ "ok": true,
  "results":     [ { "line": 3, "kind": "quantity", "text": "9.81 m/s^2",
                     "value": 9.81, "uncertainty": 0.02, "unit": "m/s^2", "dimension": "L·T⁻²" } ],
  "diagnostics": [ { "line": 5, "severity": "error", "code": "dimension-mismatch", "message": "..." } ] }
```

- `kind` is one of `quantity`, `number`, `bool`, `string`, `vector`, `matrix`, `equation`, `function`, `range`, `plot`; `text` is always present; a quantity also carries `value`, `uncertainty`, `unit` (from `Quantity::display_parts()`, so the three agree) and `dimension` (the core's formula text).
- Diagnostics merge the checker's findings (N2), parse errors and runtime errors. `code` is the checker's code, or `syntax`, `runtime`, `limit` (depth, time or output) or `forbidden`. `ok` is false when any diagnostic has severity `error`.
- `phs eval <file|-> [--seed N] [--timeout-ms N]`: always sandboxed, always JSON, default timeout 5 s, output cap 1 MiB, default seed fixed. No flag turns the sandbox off.
- `phs unit <symbol>`: prints `UnitRegistry::facts` for the symbol as JSON (dimension, kind, alias, simplified) and exits 1 with an error object for an unknown unit.
- `phs check -`: reads the program from stdin.
- `phs <script.phs>` is unchanged.

## N5c — the MCP server

- `phs mcp`: JSON-RPC 2.0 over stdio, newline-delimited, hand-written on `serde_json` (no SDK dependency). `initialize` negotiates the protocol version; `tools/list`, `tools/call`, `resources/list`, `resources/read`; unknown methods get a JSON-RPC error.
- Tools, all marked read-only: `evaluate(source, seed?)` and `check(source)` each run the program in a **child process** (`phs eval -` / `phs check -`, the current executable), killed after a hard timeout; `convert(value, from, to)` and `describe_unit(symbol)` call the core in-process, since they run no user code, and never build PHS text from their inputs.
- Resource `phs://language`: the `phs doc` Markdown.
- A crash or a hang in an evaluation is reported as a tool error; the server stays up.

## Verification (acceptance)

- A documented JSON shape, with tests that parse it for success, for a dimension error, for a syntax error and for an unknown unit.
- Sandbox refusals, each its own test: an `import` of a file, a plugin, the mesh export, runaway recursion (an error, no crash, **in a debug build**), the deadline, the output cap.
- Determinism: two runs with the same seed give identical output, different seeds differ, and a run outside a seed scope is unchanged.
- MCP: handshake, `tools/list`, `tools/call` over in-memory streams, and one test that runs the real binary and checks that a killed child is reported without stopping the server.
- No change in the behaviour of `phs <script.phs>`, except that runaway recursion now stops with an error.

## What it does not do

- No network transport for MCP, no `phs serve` changes, no OS memory limits (Windows job objects), no machine-readable grammar or unit-list file (the draft's open question: not in v1).
- The sandbox does not defend against a malicious *host*: it is a set of refusals for text from outside, not an OS-level jail.
