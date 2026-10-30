# physure-wasm — WebAssembly/JS/TS binding crate

## Context

`physure-core` is wrapped by three language bindings today: `physure-python` (PyO3), `physure-java` (JNI, published to Maven Central), and none yet for JS/TS. `physure-core/Cargo.toml`'s header comment already names the fourth as `physure-wasm` ("All FFI wrappers (physure-python, physure-wasm, physure-java) depend on this"), and the roadmap lists WebAssembly (`wasm-bindgen`) as a planned cross-language binding.

This is independent of, but interlocks with, the separate PHS→JS/TS codegen target spec (`2026-08-11-phs-js-ts-codegen-design.md`): that spec assumes a `physure` npm package exports a `Quantity` class with a specific method surface. This spec delivers that package — published as `physure` on npm, same as the crate's directory being `physure-core/` while the crates.io package is `physure`, and the Java module's directory being `physure-java/` while the Maven artifactId is `physure`. The Rust crate itself keeps the name `physure-wasm` (workspace member, `Cargo.toml` package name) since `physure` is already taken within the workspace by `physure-core`.

## Scope

Full parity with `physure-java`'s public surface: `Quantity` arithmetic/conversion, `UnitRegistry` introspection and PHS evaluation, and `PhyFunction` for stateful custom functions (deriv/integral/solve/call), per the approved "full implementation" scope.

## Crate

New Cargo workspace member `physure-wasm/`, added to the root `Cargo.toml` `members` list.

```toml
[package]
name = "physure-wasm"
crate-type = ["cdylib", "rlib"]

[dependencies]
physure-core = { workspace = true }
physure-script = { workspace = true }
wasm-bindgen = "0.2"
js-sys = "0.3"
console_error_panic_hook = "0.1"

[dev-dependencies]
wasm-bindgen-test = "0.3"
```

Unlike `physure-java`, there is no hand-written high-level wrapper layer analogous to `Quantity.java`/`NativeEngine.java`: `#[wasm_bindgen]` structs and their `impl` blocks *are* the JS-facing API, and `wasm-bindgen` generates the `.js` glue and `.d.ts` typings directly from the Rust source. There's also no `initRegistry`/`destroyRegistry` handle-pair boilerplate — wasm-bindgen-managed objects are freed via JS's normal GC/finalization, so ownership is expressed with ordinary Rust struct fields.

## API surface

### `Quantity`

Thin `#[wasm_bindgen]` wrapper around `physure_core::Quantity`.

- `new Quantity(value: number, unit: string)` — constructor (`#[wasm_bindgen(constructor)]`), delegates to `physure_core::Quantity::new`.
- `static Quantity.withUncertainty(value: number, uncertainty: number, unit: string): Quantity` — delegates to `Quantity::with_uncertainty`.
- `.add(other: Quantity): Quantity`, `.subtract(other: Quantity): Quantity`, `.multiply(other: Quantity): Quantity`, `.divide(other: Quantity): Quantity` — delegate to `add`/`sub`/`mul`/`div`.
- `.pow(exponent: number): Quantity` — delegates to `pow`.
- `.convertTo(unit: string): Quantity` — delegates to `to` (parses the target unit string against the default registry, matching Java's `Quantity.to`).
- `.toString(): string`.
- Getters: `.value: number` (canonical/mean magnitude), `.uncertainty: number`, `.unit: string`.

These names match exactly what the JS/TS codegen spec already emits, so generated code needs no changes once this ships.

### `UnitRegistry`

Wraps `physure_core::UnitRegistry` plus a `physure_script::PhsInterpreter` for the evaluate/deriv/integral/solve methods — held as ordinary struct fields (no handle indirection, unlike `NativeEngine`'s `jlong` pointers).

- `new UnitRegistry()` — loads the embedded default `physure.conf`, same as `physure_core::units::conf::DEFAULT_PHYSURE_CONF`.
- `static UnitRegistry.fromContent(content: string): UnitRegistry` — loads the default config, then applies `content` as an override, same two-step process `initRegistryFromContent` uses in the JNI binding.
- **No `fromPath` variant.** `wasm32-unknown-unknown` has no filesystem; a browser can't open an arbitrary path, and even under Node, wasm-bindgen's generated glue doesn't wire up `std::fs`. Callers who want a custom config file read it themselves (`fs.readFileSync` in Node, `<input type="file">` or `fetch` in a browser) and pass the resulting string to `fromContent`.
- `.getUnitExponents(expr: string): Map<string, number>` — built with `js_sys::Map`, mirroring `getUnitExponents`'s `Map<String, Integer>`.
- `.getUnitScale(expr: string): number`.
- `.getCategories(): Map<string, string[]>` — `js_sys::Map` of category name to `js_sys::Array` of unit symbols.
- `.evaluate(source: string): string` — runs one or more PHS statements through the interpreter and returns their string results, same behavior as `evaluateExpression`.
- `.deriv(expr: string, wrt: string): string`, `.integral(expr: string, wrt: string): string`, `.solve(equation: string, wrt: string): string` — symbolic operations, matching `UnitRegistry.java`'s `deriv`/`integral`/`solve`.

### `PhyFunction`

Wraps a name, a reference to the registry's interpreter, and a PHS function body — mirrors `PhyFunction.java`.

- `new PhyFunction(registry: UnitRegistry, name: string, body: string)`.
- `.call(...args: (Quantity | string)[]): Quantity` — accepts either `Quantity` objects or PHS-literal strings per argument, same dual calling convention Java's `call(String...)`/`call(Quantity...)` overloads provide (TypeScript expresses this as a union parameter type rather than true overloading).
- `.deriv(wrt: string): PhyFunction`, `.integral(wrt: string): PhyFunction`, `.solve(wrt: string): PhyFunction`.
- `.add(other: PhyFunction): PhyFunction`, `.subtract(other: PhyFunction): PhyFunction`, `.multiply(other: PhyFunction): PhyFunction`, `.divide(other: PhyFunction): PhyFunction`, `.compose(inner: PhyFunction): PhyFunction`.
- `.getParams(): string[]`, `.getName(): string`.

`PhyEquation` (the raw, non-callable equation object in Java, used for `add`/`sub`/`getLhs`/`getRhs` on unevaluated equations) is not part of v1 — `PhyFunction` covers the stateful, callable use case the README's usage example actually demonstrates. Add it later if a concrete need for the raw-equation form shows up.

## Errors

A `From<PhysureError> for JsValue` (or an equivalent manual conversion) builds a real `js_sys::Error` carrying the same `Display` message Rust/Python/Java already produce. Every fallible export returns `Result<T, JsValue>`, so `instanceof Error` works on the JS/TS side and nothing fails silently — consistent with the project's "raise, don't silently drop" rule. `console_error_panic_hook` is installed in a `#[wasm_bindgen(start)]` init function so an unexpected Rust panic still surfaces a readable message in the browser/Node console instead of an opaque `unreachable executed`.

## Build & packaging

`wasm-pack build` produces one portable `.wasm` binary — no per-platform build matrix, unlike `physure-java`'s five-target native-library matrix. To serve both bundler users (Vite/Webpack/Rollup — the majority of npm consumers) and plain Node without a bundler, CI builds twice:

- `wasm-pack build --target bundler --out-dir pkg-bundler`
- `wasm-pack build --target nodejs --out-dir pkg-node`

A hand-assembled `package.json` at the package root wires both outputs together via conditional `exports`:

```json
{
  "name": "physure",
  "exports": {
    ".": {
      "node": "./pkg-node/physure_wasm.js",
      "import": "./pkg-bundler/physure_wasm.js",
      "types": "./pkg-bundler/physure_wasm.d.ts"
    }
  }
}
```

Same `.wasm` bytes underneath both variants; only the JS glue differs (sync `require`-based init for Node vs async `fetch`-based init for bundlers, which wasm-pack handles automatically per target).

## Publishing

Mirrors `.github/workflows/java-release.yml`'s structure, in a new `wasm-release.yml`:

- Triggered by a `wasm-vX.Y.Z` tag (version-checked against `physure-wasm/Cargo.toml`, same pattern as `check-version` in the Java workflow).
- Single `ubuntu-latest` job: `wasm-pack build` for both targets (no platform matrix needed), assemble `package.json`, `npm publish`.
- Auth via an `NPM_TOKEN` repository secret, gated behind an `npm` GitHub Environment (mirroring the Java workflow's `maven-central` environment). One-time setup (repo owner): create an npm account/org, generate an automation access token, add it as the `NPM_TOKEN` secret in the `npm` environment. No GPG signing step — npm doesn't require it.

## Testing

`#[wasm_bindgen_test]` tests, run via `wasm-pack test --node`, covering:

- `Quantity` arithmetic and `.convertTo` between compatible units, and that mismatched/incompatible units throw a catchable JS error rather than silently combining magnitudes.
- `UnitRegistry.getUnitExponents`/`.getUnitScale`/`.getCategories`/`.evaluate` against known unit expressions.
- `PhyFunction.call`/`.deriv`/`.integral`/`.solve` against the same `kinetic_energy` example the `physure-java` README uses, for direct behavioral parity.

These run in an actual JS engine (Node, via `wasm-pack test --node`), unlike the codegen target's string-assertion-only tests — this is the first place generated/hand-written JS/TS code against this API actually executes.

## Out of scope

- `PhyEquation` (raw, non-callable equation wrapper) — deferred until a concrete need appears.
- Browser-based (`--headless --chrome`) test runs — Node-based `wasm_bindgen_test` is sufficient for v1 coverage; add browser tests if browser-specific behavior (e.g. `fetch`-based init) needs verification later.
- Actually publishing the first release — this spec produces the workflow and package; running it requires the one-time npm token setup and a deliberate tag push, both left to the repo owner.
