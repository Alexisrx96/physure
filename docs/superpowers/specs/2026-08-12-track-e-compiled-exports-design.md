# Track E — Compiled Export Artifacts Design Spec (`.proto` + `.md` + `.dll`/`.so`)

**Date**: 2026-08-12
**Status**: Approved
**Subsystem**: `physure-script` (grammar, AST, codegen), `physure-cli` (new `export` subcommand)

---

## 1. Overview & Goals

Track E turns an already-`export`ed PHS function into a portable, cross-language-callable
artifact bundle: an interop contract (`.proto`), human-readable documentation (`.md`), and
optionally a compiled native library (`.dll`/`.so`/`.dylib`). Scoped to one function at a time —
the curated multi-formula "formula repository" idea stays parked, out of scope.

Three things ride along on this track that don't exist in PHS today and are prerequisites, not
side effects:

1. **`///` doc comments** — a new first-class comment form attached to a `FunctionDefNode`, the
   source the `.md` generator renders from.
2. **`.proto`/`.md` generation is mandatory**, not one of several `--target` choices. Every
   `phs export` run produces both; the compiled native artifact is the only optional part.
3. **Contract propagation** — `@requires`/`@ensures`/`@range` (Track F, already merged) must be
   enforced inside the compiled shim too, or a `.dll`/`.so` consumer who never sees the PHS source
   has no way to know a contract failed. This was explicitly left as "Track E's job" by the Track F
   plan's scope boundary.

---

## 2. `///` Doc Comments — Grammar & AST

### 2.1 Problem

`COMMENT` today is silent and swallows `//` and `#` indiscriminately:

```pest
COMMENT = _{ ("//" | "#") ~ (!"\n" ~ !"\r" ~ ANY)* }
```

`///` is a prefix match against the `"//"` branch, so a `///` line is discarded exactly like a
plain `//` comment today — the text never reaches the AST. This must change without breaking any
existing `//`-comment script.

### 2.2 Grammar

```pest
COMMENT = _{ ("//" ~ !"/" ~ (!"\n" ~ !"\r" ~ ANY)*) | ("#" ~ (!"\n" ~ !"\r" ~ ANY)*) }

doc_comment = @{ "///" ~ (!NEWLINE ~ ANY)* }

// One or more `///` lines immediately above the definition they document. Stacks outside
// decorators — source order is docs, then decorators, then the def:
//   /// Computes kinetic energy.
//   @stable
//   fn kinetic_energy(m, v) = 0.5 * m * v^2
documented_stmt = { (doc_comment ~ _nl)+ ~ (decorated_stmt | function_def | assignment_fn | assignment) }

stmt = { import_stmt | export_stmt | documented_stmt | decorated_stmt | function_def
        | assignment_fn | assignment | guard_if_stmt | return_stmt | while_stmt | raw_block | expr }
```

`("//" ~ !"/" ~ ...)` means a bare `//x` still matches the silent `COMMENT` branch (the `!"/"`
lookahead only rejects when the *next* character is another `/`), so every existing `//`-comment
script is unaffected. `///x` now fails the `COMMENT` rule and falls through to `_is_stmt_start`,
where `documented_stmt` picks it up explicitly. `doc_comment` is atomic (`@`) so its captured span
is the raw comment text, not reconstructed from sub-token concatenation.

### 2.3 AST

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDefNode {
    pub name: String,
    pub params: Vec<String>,
    pub param_units: Vec<Option<String>>,
    pub body_stmts: Vec<Statement>,
    #[serde(default)]
    pub decorators: Vec<DecoratorNode>,
    #[serde(default)]
    pub doc: Option<String>,   // consecutive `///` lines, newline-joined, `///` prefix stripped
}
```

`AssignmentNode` does **not** get a `doc` field — Track E only compiles/documents functions, so a
doc comment on a constant assignment has no consumer yet. Adding it now would be dead weight;
revisit if a future track needs it.

### 2.4 Parser

Same shape as Track F's `parse_decorated_stmt`: a new `parse_documented_stmt` in `parser.rs`
collects consecutive `doc_comment` pairs (stripping the `///` prefix and one leading space if
present, per line), parses the wrapped `decorated_stmt | function_def | assignment_fn | assignment`
via the existing functions, and — only for `Statement::FunctionDef` — sets `node.doc = Some(joined)`.
A doc comment stacked on an `Assignment` is accepted syntactically (so `documented_stmt` doesn't
need a separate grammar path per target) but the text is simply dropped with no error — consistent
with "doc comments are opt-in documentation, not a correctness invariant," unlike unit dropping.

### 2.5 Test

Parse round-trip: single-line and multi-line `///` block attaches to `FunctionDefNode.doc` in
source order; `///` stacked above `@decorators` above `fn` parses with both `doc` and `decorators`
populated; a bare `//`/`#` comment continues to parse as before (regression coverage for the
`COMMENT` rule change).

---

## 3. Codegen — `.proto` Generator

New `physure-script/src/codegen/proto.rs::ProtoGenerator`, implementing the existing
`CodeGenerator` trait (`fn generate_program(&self, program: &Program) -> Result<String, CodegenError>`)
— same shape as `python.rs`/`rust.rs`/`java.rs`/`js.rs`. The CLI builds a synthetic
`Program { statements: vec![Statement::FunctionDef(node.clone())] }` containing only the exported
function and calls `generate_program` on it, so all five codegen backends share one calling
convention.

Message shape, derived from `param_units` and whether the function carries any
`@requires`/`@ensures` decorator (`@range` has already been lowered into `@requires` by Track F's
parser pass by the time codegen ever sees `node.decorators` — see §6 for what that means for
message content):

```proto
message KineticEnergyRequest {
  double m = 1;
  double v = 2;
}

message KineticEnergyResponse {
  double value = 1;
  bool ok = 2;      // present only if the function has >=1 @requires/@ensures
  string error = 3; // present only if the function has >=1 @requires/@ensures
}

service KineticEnergyService {
  rpc Compute(KineticEnergyRequest) returns (KineticEnergyResponse);
}
```

Text templating only — no `prost`/protobuf runtime dependency, matching the existing codegen
backends' style. Message/service names are the function name in PascalCase.

**Test**: text-shape assertions (same style as existing `codegen/tests.rs`) for a plain function
and a decorated one, confirming the `ok`/`error` fields appear only when contracts are present.

---

## 4. Codegen — `.md` Generator

New `physure-script/src/codegen/md.rs::MdGenerator`, same `CodeGenerator`-trait /
synthetic-`Program` calling convention as `proto.rs`.

```markdown
# kinetic_energy

Computes kinetic energy.

## Signature

`kinetic_energy(m, v) -> Quantity`

| Parameter | Unit |
| :-- | :-- |
| `m` | *(none declared)* |
| `v` | m/s |

## Stability

`@stable`

## Preconditions

- `v must be >= the @range lower bound`
- `v must be <= the @range upper bound`

## Postconditions

- `result must be positive`
```

- **Description** section is the joined `doc` text; if `node.doc` is `None`, the section is
  omitted entirely (not an error — see §2.4).
- **Stability** section renders `@stable`/`@experimental` if present, omitted otherwise.
- **Preconditions**/**Postconditions** list each `@requires`/`@ensures` decorator's message
  argument (the second arg) verbatim, one per bullet, in decorator source order. Omitted if empty.
- **`@range` display, deliberate simplification**: Track F's `lower_range` fully desugars
  `@range(v, min, max)` into two independent `@requires` decorators at parse time and does not
  retain the original `(param, min, max)` tuple anywhere in the AST (confirmed by reading
  `physure-script/src/decorators.rs` — `lower_range` returns `Vec<DecoratorNode>` with no
  provenance marker). Reconstructing "valid range: [0, 10] m/s" as a single line would require
  either threading a source tag through Track F's already-merged, already-tested lowering, or
  string-matching each `@requires` message against the `"@range lower/upper bound"` suffix
  `lower_range` happens to generate today — both add real complexity for a formatting nicety. This
  design instead renders each lowered `@requires` as its own precondition bullet, using the
  self-describing message `lower_range` already produces. Less pretty than a single structured
  range line, but accurate and requires zero changes to merged Track F code. Revisit only if a
  future consumer genuinely needs the structured triple back (not just prettier docs).

**Test**: text-shape assertions mirroring `proto.rs`'s — a plain function, one with only `///`
docs, one with only decorators, one with both, and one with none of the optional sections (bare
signature table only).

---

## 5. Codegen — Native FFI Shim

Implemented as a new method on the existing `RustTranspiler` (`physure-script/src/codegen/rust.rs`),
not a new file — it reuses `generate_function_def`'s already-transpiled function body directly and
only adds a wrapping shim, so it belongs next to what it wraps rather than duplicating the
`Expr`-to-Rust logic in a sibling module.

```rust
impl RustTranspiler {
    /// `pub fn generate_export_shim(&self, node: &FunctionDefNode) -> Result<String, CodegenError>`
    // 1. Emits the ordinary transpiled function via `generate_function_def` (unchanged, private).
    // 2. Emits an `extern "C"` wrapper: flat `f64` params, one per `node.params`, each converted
    //    via `Quantity::new(param, "<node.param_units[i] or \"\">")?`.
    // 3. If `node.decorators` contains no `@requires`/`@ensures` (a `@stable`-only or bare
    //    function): wrapper returns a bare `f64` via `.value.mean()` — metadata-only decorators
    //    never trigger the struct-return shape.
    // 4. If it contains at least one `@requires`/`@ensures`: wrapper returns
    //    `#[repr(C)] pub struct <Name>Result { value: f64, ok: bool }`,
    //    running each `@requires` check (params bound) before the body call and each `@ensures`
    //    check (result bound under `result`) after, matching `interpreter.rs`'s
    //    `check_requires`/`check_ensures` semantics exactly — same condition `Expr`, same
    //    evaluation order, so compiled and interpreted pass/fail can never diverge by construction
    //    rather than by two independently-written condition evaluators. On the first failing
    //    check, stores the decorator's message (evaluated the same way `check_requires`/
    //    `check_ensures` do) into a `thread_local!` `String`, and returns `{ value: 0.0, ok: false }`.
    //    A `#[no_mangle] pub extern "C" fn <name>_last_error() -> *const c_char` reads that
    //    thread-local — same idiom as `errno`/`GetLastError`, valid until the next call on the
    //    same thread.
}
```

Condition `Expr`s are already ordinary `FunctionCall`s under the hood (`op_>`, etc. — Track F's own
note), so no new expression-to-Rust-source machinery is needed; `generate_expr` (already used for
the function body) handles them unchanged.

**Test**: text-shape assertions — bare-`f64` shape for an undecorated function, struct-return shape
with the right number of requires/ensures checks in the right order for a decorated one.

---

## 6. `physure-cli export` Subcommand

New `physure-cli/src/export.rs`, sibling to `scaffold.rs`.

```
phs export <script.phs> --fn <name> [--native] [-o <dir>]
```

1. Parse the script (`physure_script::parser::parse_phs`). Find the `FunctionDefNode` named
   `--fn` among `program.statements`; error + `exit(1)` if absent.
2. Confirm a matching `Statement::Export { symbol: <name>, .. }` exists in the same program; error
   + `exit(1)` if the function exists but was never `export`ed — Track E reuses the existing
   `export` statement as the compile-target marker, it does not invent new annotation syntax.
3. Always write `<out>/<fn>.proto` (via `ProtoGenerator`) and `<out>/<fn>.md` (via `MdGenerator`).
   `<out>` defaults to the script's own directory, overridable with `-o` — created with
   `fs::create_dir_all` if it doesn't exist yet, matching `scaffold.rs`'s existing `write_file`.
4. If `--native` is passed:
   a. Generate the shim source via `RustTranspiler::generate_export_shim`.
   b. Scaffold `<out>/<fn>_export/{Cargo.toml, src/lib.rs}`:
      - `Cargo.toml`: `[lib] crate-type = ["cdylib"]`, and
        `physure-core = { path = "<baked-in-absolute-path>", package = "physure" }`.
      - The absolute path is computed once, at `phs` build time, as
        `concat!(env!("CARGO_MANIFEST_DIR"), "/../physure-core")` — the same relationship
        `physure-cli/Cargo.toml` itself already has to `physure-core` (`path = "../physure-core",
        package = "physure"`), just baked into the generated file instead of Cargo's own
        dependency graph. This makes every `phs export --native` run, from any output directory,
        resolve back to the exact `physure-core` this `phs` binary was built from — no publishing,
        no vendoring, no drift from the "single source of truth" unit logic in `physure-core`.
   c. Run `cargo build --release` via `std::process::Command` inside `<out>/<fn>_export/`. On
      failure, print cargo's stderr verbatim and `exit(1)` — no attempt to reinterpret it.
   d. Copy the resulting `target/release/{lib<fn>_export.dll|.so|.dylib}` to
      `<out>/<fn>.{dll|so|dylib}` (extension by `cfg!(target_os)`, mirroring `scaffold.rs`'s
      existing `EXT` selection in its shell templates).

**Rejected alternatives for the `physure-core` dependency** (recorded so this isn't re-litigated):
publishing `physure-core` to a registry (blocked on a publishing decision nobody has made) and
vendoring a copy of `Quantity`/unit logic into the generated crate (directly violates this
project's single-source-of-truth invariant for unit correctness — the exact class of drift
`CLAUDE.md` already warns about).

**Test**: CLI-level tests for the error paths (`--fn` missing, function not `export`ed, unknown
flag). A `--native` round-trip integration test (slower — real `cargo build --release` — gated
behind a feature or `#[ignore]`d and run explicitly in CI, not on every `cargo test`): compile a
known exported function, load the `.dll`/`.so` with `libloading` (new `physure-cli` dev-dependency,
never shipped), call it, and assert the result matches the interpreter's own evaluation of the same
function within floating-point tolerance. A second case exports a function carrying
`@requires`/`@ensures`, calls it with a valid and a contract-violating input, and asserts
`.ok`/`<fn>_last_error()` match the interpreter's own pass/fail for the same inputs.

---

## 7. Scope Boundaries

- **In scope**: `///` doc comments (grammar/AST/parser), `.proto` generator, `.md` generator,
  native FFI shim generator with `@requires`/`@ensures` propagation, `phs export` CLI subcommand,
  scaffold-and-build pipeline, all tests described above.
- **Out of scope, parked**: a curated multi-formula repository, a hosted on-demand build service,
  a generated docs *site* (this track produces one `.md` file per exported function, not a site).
  `.dylib` output on macOS happens for free via the same `cargo build --release` mechanism, though
  not separately tested here (no macOS CI runner in this workspace today).
- **Explicitly not touched**: Track F's `lower_range`/`validate_decorators`/interpreter enforcement
  — reused as-is. Track F's own scope boundary already forbade this plan from adding
  decorator-aware behavior to the *existing* Python/Java/Rust/JS transpilers; that still holds —
  only the new `generate_export_shim` method is decorator-aware, `generate_function_def` and the
  other three language backends are untouched.
- **`@export` as sugar for the compile trigger**: still an open, deferred question per the roadmap
  (§8, "Open, deferred"); this plan does not decide it — `export` (the existing statement) remains
  the only trigger.

---

## 8. Example Script This Plan Makes Valid

```phs
/// Computes the kinetic energy of a moving mass.
/// `m` is mass, `v` is velocity, both must be positive.
@stable
@range(v, 0.0 m/s, 2.998e8 m/s)
@ensures(result > 0.0 J, "kinetic energy must be positive")
fn kinetic_energy(m, v) = 0.5 * m * v^2

export kinetic_energy as kinetic_energy
```

```bash
phs export orbit_sim.phs --fn kinetic_energy --native -o dist/
# writes dist/kinetic_energy.proto, dist/kinetic_energy.md, dist/kinetic_energy.dll (or .so)
```
