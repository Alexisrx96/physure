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

### Changed
- `Quantity.java` now uses `List<Quantity>` instead of raw `double[]` for `QuantityVector` interactions; added `mul()`, `div()`, `sub()` shorthand aliases.

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
