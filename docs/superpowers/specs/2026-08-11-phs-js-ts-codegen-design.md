# PHS → JS/TS codegen target

## Context

`physure-script`'s `codegen` module already transpiles PHS programs to Python, Rust, and Java (`physure-script/src/codegen/{python,rust,java}.rs`, dispatched via `Target` in `mod.rs`). Each target assumes a real runtime library is available to the generated code: Python imports the published `physure` PyPI package, Java imports the published `com.physure.*` Maven classes, Rust uses the `physure_core` crate.

This adds two more targets — `Target::JavaScript` and `Target::TypeScript` — completing PHS's "JS/TS support" alongside a separate, independent effort to build the `physure-wasm` binding crate, published to npm as `physure` (tracked separately; not part of this spec).

## Decision: assume the `physure` npm package exists

Generated JS/TS code assumes an npm package named `physure` is on hand, exporting a `Quantity` class with the same surface as `physure-java`'s (published Maven artifactId `physure`) hand-written runtime:

- `Quantity.of(value: number, unit: string): Quantity`
- `Quantity.withUncertainty(value: number, uncertainty: number, unit: string): Quantity`
- `.add(other)` / `.subtract(other)` / `.multiply(other)` / `.divide(other)` / `.pow(exponent)` / `.convertTo(unit: string)`
- `.toString()`

Building that package is out of scope here — it's a hard requirement the eventual `physure-wasm` binding crate must satisfy (its Rust crate name stays `physure-wasm`, but it publishes to npm as `physure`, same directory-vs-published-name split as `physure-core`→`physure` on crates.io and `physure-java`→`physure` on Maven). This spec is codegen-only, in the same spirit as the existing Rust/Java/Python targets: it emits well-formed source and is verified with string-assertion tests, not by executing the output against a live runtime.

## Architecture

Add one new file, `physure-script/src/codegen/js.rs`, with:

```rust
pub struct JsTranspiler {
    pub typed: bool, // false = plain JS, true = TypeScript
}
```

This mirrors `JavaTranspiler`'s `class_name` field — one struct, one `CodeGenerator` impl, a flag that changes a handful of emission points (type annotations on function signatures and `const` declarations).

`Target` in `mod.rs` gains two variants:

```rust
pub enum Target {
    Python,
    Rust,
    Java,
    JavaWithClass(String),
    JavaScript,
    TypeScript,
}
```

`transpile()` routes both through `compile_equations_to_functions` first — the same pass Rust and Java already use, which runs the interpreter once to learn each equation's `(lhs, rhs)` and rewrites named-argument equation calls (`eq5(I = -1.6mA, V = -6.3V)`) into real generated functions. This means JS/TS needs no runtime equation-solving support, same as Rust/Java.

Shared helpers already in `mod.rs` are reused as-is: `split_interpolated` (string interpolation), `expr_to_unit_string` (unit literals for `.convertTo`), `range_is_not_transpilable` (ranges have no JS/TS equivalent, same as Rust/Java).

Identifier casing: PHS's `snake_case` names become `camelCase`, using the same conversion Java's transpiler already does. Whether that helper is extracted to `mod.rs` as a shared `pub(crate) fn` or duplicated in `js.rs` is an implementation-time call, not a design one.

## Emission shape

**Imports** — both flavors use ESM:

```js
// JavaScript
import { Quantity } from "physure";
```

```ts
// TypeScript
import { Quantity } from "physure";
```

**Values** — `Quantity.of(75, "kg")`, or `Quantity.withUncertainty(75, 0.5, "kg")` when the literal carries uncertainty (same shape as Java's `Quantity.withUncertainty(75.0, 0.5, "kg")`, minus the forced `.0` — JS/TS numeric literals don't need it).

**Functions** — plain JS:

```js
function kineticEnergy(m, v) {
    return m.multiply(v);
}
```

TypeScript adds annotations:

```ts
function kineticEnergy(m: Quantity, v: Quantity): Quantity {
    return m.multiply(v);
}
```

**Binary ops** — same method-call mapping Java uses: `+`/`-`/`*`/`/` → `.add`/`.subtract`/`.multiply`/`.divide`, `=>` (unit conversion) → `.convertTo("unit")`, `^` → `.pow(n)` for a literal exponent, or `.pow(other.getValue())` when the exponent is itself an expression — matching Java's `getValue()` naming, since the assumed `physure` npm package's API mirrors Java's runtime surface throughout. Ranges error via `range_is_not_transpilable()`.

**Top-level statements** — each assignment prints its value (matching Python/Java's behavior of proving the script ran), using a template literal: `` console.log(`m: ${m}`); `` — idiomatic JS/TS, and template literals call `toString()` on interpolated values automatically, same effect as Java's concatenation without needing it.

## CLI wiring

`physure-cli/src/main.rs`'s `handle_transpile` gets two more arms, following the exact existing pattern for `python`/`java`:

- `--target js` / `--target javascript` → `Target::JavaScript`, default output extension `.js`
- `--target ts` / `--target typescript` → `Target::TypeScript`, default output extension `.ts`
- Extension inference from `--output foo.js` / `foo.ts` when `--target` is omitted, same as the existing `.py`/`.java` inference.

Help text (`main.rs` usage strings) and the CLI's generated markdown docs (`md.push_str` block around line 577) get the two new target names added alongside `python`/`rust`/`java`.

## Testing

- Unit tests in `js.rs`, mirroring `java.rs`'s: function-def transpilation, quantity-with-uncertainty transpilation — one set for JS, one for TS (or parametrized over `typed`).
- Both new targets added to the two shared loop-based tests already in `mod.rs`:
  - `where_bindings_are_inlined_instead_of_emitted_as_a_let_call` (extend the `for target in [...]` array)
  - `named_argument_equation_call_compiles_to_a_real_function` (add JS/TS assertions alongside the existing Rust/Java ones)
- A CLI-level test (or manual check) confirming `phs transpile script.phs --target ts -o out.ts` writes a `.ts` file and the console message names "TypeScript".

## Out of scope

- The `physure-wasm` binding crate itself (separate spec).
- Executing/compiling generated JS/TS output in CI (no target runtime exists yet to run it against).
- npm publishing/packaging concerns — not this spec's problem until the `physure` npm package exists.
