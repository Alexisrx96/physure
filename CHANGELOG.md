# Changelog

All notable changes to the **Physure** ecosystem are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- **PhyEquation & PhyFunction architecture** with equation arithmetic, callable equations, and multi-language transpilation parity tests across PHS, Rust, Python, and Java 8+.
- **Infinity support** (`inf`, `-inf`, `∞`, `-∞`, `infinity`, `oo`) in PHS grammar, limits, and improper integrals.
- **Advanced vector calculus operators**: high-order derivatives `diff(f, var, order)`, gradient `grad`, divergence `div`, curl `curl`, laplacian `laplacian`.
- **`QuantityVector` & `QuantityMatrix`** (Order 1 & Order 2 tensors) with dot product, cross product, norm, transpose, matrix multiplication, and determinant — all with physical unit propagation.
- **Pure Rust N-Dimensional native plotting engine** (0 third-party dependencies):
  - `plot3d(expr, title)`: 3D surface plots with isometric projection and HSL depth shading.
  - `plot_field(u_expr, v_expr, title)`: 2D vector field arrow plots with magnitude color scaling.
  - `plot_nd(matrix, title)`: N-D parallel coordinates visualization.
- **Strict tensor type separation**: `Quantity` (scalar), `QuantityVector` (vector), `QuantityMatrix` (matrix) as first-class citizens in Rust, PHS, Python, and Java 8+.
- **`PhsValue::Matrix`** variant in `physure-script` with full exporter support (JSON, CSV, Python).
- **Java 8+ classes** `com.physure.QuantityVector` and `com.physure.QuantityMatrix` with idiomatic Java collections API.
- **Python classes** `physure.QuantityVector` and `physure.QuantityMatrix` exposed via PyO3.
- **`abs` accepts a quantity**, returning the same unit and the same uncertainty — folding a magnitude to its absolute value moves where a measurement sits, not how well it is known. It delegates to the core's `UncertaintyValue::propagate_function`, so a Monte Carlo or unscented backend is not silently downgraded to Gaussian.
- **`exp`, `ln` and `log` accept a dimensionless quantity**, propagating its uncertainty, and reject a dimensioned one with a clear error. They used to refuse every quantity ("exp expects a number"), so `exp(0.5 +/- 0.01)` could not be evaluated at all, while `ln(5 m)` — a physics error, since a power series can only be summed over terms that share a unit — was the sort of thing the tool is supposed to catch.

### Changed
- `Quantity.java` now uses `List<Quantity>` instead of raw `double[]` for `QuantityVector` interactions; added `mul()`, `div()`, `sub()` shorthand aliases.
- **BREAKING (PHS)**: local bindings moved from `let name = value in body` to a postfix `where` clause — `body where name = value[, name2 = value2]`. A later binding can use an earlier one. `let` stays a reserved word with no rule behind it, so the old form fails to parse instead of quietly meaning "let times inches".

### Removed
- **`physure.ext.grammar`**, the Python reimplementation of the PHS language (1655 lines), and its test module. Only Rust implements PHS now: `physure.repl` (`python -m physure`, `physure repl`) evaluates through `physure._core.Interpreter` and reports an install hint if the native engine is missing. Startup for `python -m physure "500 N / 2 m^2 => kPa"` drops from the Python `UnitSystem` build to ~0.09 s.

### Fixed
- **`QuantityVector::norm()` reported an uncertainty exactly √2 too small.** It squared the vector through `dot(self, self)`, and `mul` treats its two operands as statistically independent, so each component contributed σ(xᵢ²) = √2·xᵢσᵢ instead of the correct 2·xᵢσᵢ. Every component is now squared with `pow(2.0)`, whose analytic derivative knows it is the same variable, which gives the GUM result σ = √(Σ (xᵢ/|v|)²σᵢ²) and preserves the component's uncertainty backend instead of collapsing it to Gaussian. For `(1 ± 0.09, 2 ± 0.06, 2 ± 0.18) m` the norm now reads `3 ± 0.13 m`, not `3 ± 0.0919 m`.
- **A unit symbol that the script also binds as a variable is now rejected instead of guessed.** `g = 9.81 m / s ^ 2` followed by `f = 10.0 kg * g` read `g` as the gram unit and produced a mass-squared quantity where the author meant 98.1 N; with `t = 3.0 s` bound, `5 m / t` meant metre per *tonne*. The check runs once over the parsed program, so the interpreter and all three transpile targets are covered, and the error names the token and both rewrites: `(10 kg) * g` for the variable, `10 kg * "g"` for the unit. Only a token following an explicit `*` or `/` is a candidate — `10 m` is a quantity literal by grammar, so binding `m` does not poison every later literal.
- **`floor` and `ceil` keep the uncertainty**, like `round` now does. Both rebuilt the quantity with a zero standard deviation, so `floor(9.81 +/- 0.05 m/s^2)` printed as an exact `9.0 m / s ^ 2`. The mean is moved by adding an exact offset, which slides the whole distribution and leaves a Monte Carlo or unscented backend intact.
- **`sin`, `cos` and `tan` are dimensionally checked and keep the uncertainty.** They returned a bare number built from the mean — unit and uncertainty both discarded — and applied themselves to anything, so `sin(9.81 m/s^2)` answered -0.379 as though metres per second squared were radians. The argument must now be an angle (converted to radians through its own scale, so `sin(90 deg)` is 1) or a dimensionless value read as radians; the result is dimensionless and the sigma comes from the derivative.
- **A degree is a plane angle again.** `physure.conf` declares `radian` and `steradian` against the same dimension symbol, and the Rust loader let the last one win, so `deg`, `arcmin` and `arcsec` were registered as scaled *steradians*: `90 deg => rad` failed as a unit mismatch while `1 deg + 1 sr` was accepted.
- **Inches are usable again**: `in` was a grammar keyword taken by `let ... in`, so `12 in`, `12 in => cm` and `1.5 in^2` were parse errors and only the `inch` alias worked.
- `physure._core.Quantity.__str__` renders the measurement (`0.25 kPa`) by delegating to the core's `Display`, instead of repeating `__repr__` (`Quantity(0.25, kPa)`); the REPL and `print()` now read like the `phs` CLI.
- Local bindings now transpile to real code in the Python, Rust and Java targets. They used to be emitted as a call to an undefined `let(...)` function, so the generated file did not compile.
- **Uncertainties survive formatting, rounding and `repr`.** A format spec (`g:.2f`) printed the mean alone, `round(q, n)` rebuilt the quantity with a zero standard deviation, and `physure._core.Quantity.__repr__` omitted the uncertainty — an uncertain measurement looked exact in all three.
- **A percent uncertainty is relative again**: `9.81 +/- 0.5% m/s^2` was parsed as ±0.5 instead of ±0.049, a spread twenty times too wide. A percentage applied to a magnitude that is only known at run time is now rejected rather than guessed.
- Transpiled files stamp the compiler's real version. All three targets hardcoded `v0.2.4` while the workspace was on `0.2.3`, so a generated file named a compiler that had never produced it; the banner now comes from `env!("CARGO_PKG_VERSION")`.
- The `physure` crate README's usage example imported `use physure::{...}`, which does not compile. The package is `physure` but its library target is `physure_core`, and Cargo derives the import path from the latter. The README is not wired in with `include_str!`, so no doctest ever caught it.
- `Debug for Quantity` (Rust) prints `std_dev`, so a failing assertion about an uncertain quantity no longer reports what looks like a different measurement.

---

## [0.2.3] - 2026-07-26

**Tags:** `v0.2.3`, `core-v0.2.3`, `java-v0.2.3`

### Added
- **PHS CLI i18n/LaTeX fixes** and cross-platform installer improvements (#40).
- **Auto-loading of `ext/*.py` functions** into PhysureScript interpreters.
- **Native `.rs` plugin FFI** with rich value types and hot-reload support.
- **`phs new-plugin` scaffold** command in CLI.
- **Domain-gated builtins** and lazy plugin/ext loading.
- **PHS Language Server Protocol (LSP)**:
  - Context-aware `use`/`from` autocompletion.
  - Surface `use`/`from` domain-gated builtins in completions and hover.
  - Auto-pop the `use`/`from` suggestion widget on space.
- **`physure-lsp` binary** shipped alongside `phs`, with native `linux-aarch64` builds.
- **`physure-java` Maven Central packaging** setup, bump to 0.2.3.

### Fixed
- Forward file path in daemon protocol for `use`/`from` resolution.
- Remove lazy Python ext domain loader (`use ... from <py-stem>`).
- Restore `physure` as crates.io package name for `physure-core`.

---

## [0.2.2] - 2026-07-20

**Tags:** `v0.2.2`, `py-core-v0.2.2`

### Added
- **Native PHS Rust core engine**, standalone `phs` CLI binary, and PyO3 FFI bindings (`v0.2.1-phs`).
- **Combined `py-core-v*` tag** to release to both PyPI and Crates.io simultaneously.

### Fixed
- Restore environment to `crates-io` in `core-release.yml`.
- Track `physure-core/src/bin/phs.rs` by fixing `.gitignore`.

---

## [0.2.1] - 2026-07-19

**Tags:** `v0.2.1`, `core-v0.2.1`, `py-core-v0.2.1`

### Added
- **Physure logo assets** with theme-aware logos in all READMEs.
- Brand-colored badge set with live PyPI, crates.io and CI tags.
- **MKML grammar extensions**: `linspace`, `plot` support, and text interpolation.
- Rust-native `approx_eq`, `sqrt` and DSL helper exports.
- `RationalUnit::base()` method in core engine.

### Fixed
- Reject bare number addition with dimensioned quantities in symbolic engine.

---

## [0.2.0] - 2026-07-18

**Tag:** `v0.2.0`

### Changed
- **Complete rebranding** from MeasureKit to **Physure**.
- **Monorepo Cargo Workspace** restructure: `physure-core`, `physure-script`, `physure-python`.
- Simplified `physure-python` package layout to flat `physure-python/physure`.

### Added
- **Complete decoupling of `physure-core` from PyO3** — pure Rust core with no Python dependencies.
- **Q⁷ mathematical foundation core**: `DimVector` 7-dimensional vector space, Dual AD, 2nd-order Hessian propagation, and Intervals.
- **Expanded symbolic engine**: general power differentiation `u(x)^v(x)`, algebraic factorization, and advanced integration rules.
- **Full Rust-first migration**: unit parser, pre-baked registry, converters, `DimVector`, `UnitDefinition`, and Hessian propagation — all in Rust.
- `CompoundUnit` becomes a thin wrapper over Rust `RationalUnit`.
- Native Rust `Expr` PyO3 class for symbolic expressions.
- Transcendental function delegation (sin, cos, tan, exp, log, tanh) to Rust with uncertainty propagation.
- **Unit-aware `curve_fit`** and friendly constant name resolution.
- Comprehensive performance benchmarks report (`BENCHMARKS.md`).
- Multi-platform release wheels CI pipeline.
- Pinned GitHub Releases on version tags.

### Fixed
- PEP 639 license metadata for PyPI validation.
- Zero-exponent parser guard for digit-suffixed alias names (`a0`, `tau0`).
- Non-integer dimension exponent rejection from native parser.

### Removed
- Python lexer and recursive-descent parser (replaced entirely by Rust core parser engine).
- `physure/domain/notation` module (consolidated into measurement module).
- Rust-fallback illusion stubs.

---

## [0.1.9] - 2026-07-14

**Tag:** `v0.1.9`

### Added
- **MKML notation layer** improvements:
  - Unicode subscript digits in chemical formulas and reaction terms.
  - Equilibrium arrow (`⇌`, `<=>`) with reversible flag.
  - `×` and `÷` as multiplication/division operators.
  - `√` / `sqrt(...)` prefix operator.
- **MKML math functions**: `round`, `floor`, `ceil`, `min`, `max`, `sin`, `cos`, `tan`, `exp`, `log`, `ln` via dispatch table.
- **MKML user-defined functions**: definitions and calls, recursion with configurable depth limit, optional typed parameters, `let...in` local bindings, display-text blocks.
- **Ternary operator** and comparison operators in MKML grammar.

### Fixed
- CompoundUnit recipe lookup keyed by every alias, not just the canonical symbol.
- Hide unit for dimensionless quantities in display.
- Log-domain-error tests made version-agnostic.

---

## [0.1.8] - 2026-07-10

**Tag:** `v0.1.8`

### Added
- **Automated release pipeline**: bump workflow with pinned GitHub Actions (#33).
- **Rust core engine** with PyO3 0.23 migration, `__torch_dispatch__`, custom Triton kernel for covariance updates.
- **Cross-correlation storage**, pruning config, and autograd covariance store.
- **Arrow IPC export** and pickle state support in Rust core.
- **Symbolic module**: `SymbolicExpression.compile` for optimized numeric execution.
- **MKML (MeasureKit Markup Language) grammar interpreter** for MeasureNote-style notes (#10).
- **Unit-aware terminal REPL** (`python -m measurekit` / `measurekit repl`) (#12).
- **Equivalencies**, currency extensions, expanded unit/constant catalog, PINN example (#14).
- **Exact rational conversion factors** — float only at output (#23).
- PEP 561 `py.typed` marker and typing stubs.
- Optional SymEngine acceleration for symbolic operations.
- Python 3.13/3.14 support (PyO3 0.23→0.25).
- **Zero required runtime dependencies** (#9).
- First-use performance improvement: 4.8s → 0.5s (lazy torch/scipy, single system build, REPL pre-warm) (#18).
- WASM/Pyodide build support via GitHub Action.
- `UnknownUnitError` with `difflib` suggestions.
- `save_state` / `load_state` for application persistence.
- Covariance store pruning and non-linear fallback.
- Comprehensive README overhaul with grammar ext, REPL, install extras, units reference (#13).

### Changed
- Core install slimmed from ~130 MB to ~5 MB.
- Crate-type changed to `staticlib` for Pyodide compatibility.

### Fixed
- 29 SonarQube violations resolved, coverage raised 60→84%.
- `gal` redefinition resolved (gallon keeps `gal`, galileo gets `Gal`) (#17).
- Correctness batch: angles, uncertainty Jacobians, roots, recipes, entry points (#6).

---

## [0.0.3] - 2025-10-01

**Tag:** `0.0.3`

### Changed
- Documentation and formatting improvements across multiple modules and tests.
- Enhanced `SymbolicQuantity` class with improved unit handling and operator overloading.
- Context management for active unit system and enhanced `UnitSystem` class.
- Improved `QuantityFactory` initialization and global default system management.

---

## [0.0.2] - 2025-09-24

**Tag:** `0.0.2`

### Changed
- Refactored MeasureKit's unit handling and initialization system.
- Improved documentation across measurement modules.
- Simplified `Dimension` class implementation.
- Enhanced type hinting and simplified arithmetic operations in `Quantity` and `CompoundUnit`.

### Fixed
- Replace `ValueError` with `IncompatibleUnitsError` in integration and measurement tests.

---

## [0.0.1] - 2025-09-16

**Tag:** `0.0.1`

### Added
- Initial release of MeasureKit (pre-Physure):
  - `Quantity` class with magnitude, uncertainty, and unit handling.
  - `CompoundUnit` for composite unit expressions.
  - Notation module with lexer and parser for unit string expressions.
  - Dynamics module with ODE solver and unit awareness.
  - `Function` class with immutability and validation.
  - Configuration management with singleton pattern.
  - `Uncertainty` class for managing measurement uncertainty.
