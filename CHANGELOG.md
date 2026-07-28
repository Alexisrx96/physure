# Changelog

All notable changes to the **Physure** ecosystem (`physure-core`, `physure-script`, `physure-python`, `physure-java`) will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.2.4] - 2026-07-28

### Added
- **Pure Rust Native N-Dimensional Plotting Engine** (0 third-party dependencies):
  - `plot3d(expr, title)`: 3D Surface plots $z = f(x, y)$ with isometric projection, depth shading, and HSL color spectrum.
  - `plot_field(u_expr, v_expr, title)`: Vector field plots $\vec{F}(x, y) = (u, v)$ with directional SVG arrows and magnitude color scaling.
  - `plot_nd(matrix, title)`: N-Dimensional parallel coordinates visualization for high-dimensional datasets.
- **Explicit Tensor Separation (`Quantity`, `QuantityVector`, `QuantityMatrix`)**:
  - `Quantity`: Strictly Tensor of Order 0 (Scalar Physical Quantity with value, uncertainty, and unit).
  - `QuantityVector`: Strictly Tensor of Order 1 (Geometrical Vector with `.dot()`, `.cross()`, `.norm()`, `.unit_vector()`).
  - `QuantityMatrix`: Strictly Tensor of Order 2 (Linear Operator with `.matmul()`, `.transpose()`, `.det()`).
  - Native class exposures in Python (`physure.QuantityVector`, `physure.QuantityMatrix`) and Java 8+ (`com.physure.QuantityVector`, `com.physure.QuantityMatrix`).

### Changed
- Refactored `PhsValue` enum in `physure-script` to include dedicated `PhsValue::Matrix(QuantityMatrix)` variant.
- Updated multi-language transpiler parity suite to verify 100% execution across PHS Interpreter, Python, Java 8+, and Rust targets.

---

## [0.2.3] - 2026-07-28

### Added
- **Advanced Vector Calculus Operators**:
  - High-order derivatives `diff(f, var, order)` ($\frac{d^n}{dx^n} f$).
  - Gradient `grad(f, [vars])` ($\nabla f$).
  - Divergence `div([F], [vars])` ($\nabla \cdot \vec{F}$).
  - Curl / Rotor `curl([F], [vars])` ($\nabla \times \vec{F}$).
  - Laplacian `laplacian(f, [vars])` ($\nabla^2 f$).
- **Vector & Matrix Physical Algebra**:
  - `QuantityVector` with dot product ($\vec{v}_1 \cdot \vec{v}_2$), cross product ($\vec{v}_1 \times \vec{v}_2$), norm ($|\vec{v}|$), and unit vector ($\hat{v}$).
  - `QuantityMatrix` with matrix transpose ($A^T$), matrix multiplication ($A \cdot B$), and determinant ($\det(A)$) enforcing dimensional safety ($[u_A] \times [u_B]$).

---

## [0.2.2] - 2026-07-28

### Added
- **Infinity Support (`inf`, `-inf`, `∞`, `-∞`, `infinity`, `oo`)**:
  - Full grammar parsing for positive and negative infinity literals in PHS script.
  - Asymptotic limit evaluation (`limit(f, var, point)`).
  - Improper numerical integration over infinite intervals (`integral(f, var, -inf, inf)`) using domain transformation $x = \frac{t}{1 - t^2}$ and 15-point Gauss-Kronrod quadrature.

---

## [0.2.1] - 2026-07-28

### Added
- **Equation Arithmetic & Symbolic Substitution**:
  - Equation addition and subtraction (`eq1 + eq2`, `eq1 - eq2`).
  - Symbolic substitution (`eq1.substitute(symbol, eq2)`).
  - `.solve("var")` method returning callable `PhyEquation` objects across all target languages.

---

## [0.2.0] - 2026-07-28

### Added
- **Idiomatic Multi-Language Transpilation Architecture**:
  - Target support for **Python**, **Java 8+ (LTS)**, and **Rust**.
  - `PhyFunction` wrapper architecture for functions across all transpilation targets.
  - `PhyEquation` callable equation architecture in Python, Java 8+, and Rust.
  - Zero-dependency Java 8+ compatibility layer (`com.physure.*`).

---

## [0.1.9] - 2026-07-27

### Added
- **Uncertainty Backends & PyO3 CPyO3 Extension**:
  - Gaussian, Monte Carlo, and Unscented Transform (UT) uncertainty propagation backends.
  - Fast native CPyO3 Python extension (`physure._core`).
  - `Q_` shorthand factory for physical quantities in Python.

---

## [0.1.5] - 2026-07-25

### Added
- **PhysureScript (PHS) DSL Engine**:
  - Pest-based grammar parser (`phs.pest`), lexer, and AST.
  - Tree-walk interpreter (`PhsInterpreter`).
  - Dimensional checking and unit propagation in PHS expressions.

---

## [0.1.0] - 2026-07-20

### Added
- **Core Dimensional Engine (`physure-core`)**:
  - 7 SI base dimensions ($L, M, T, I, \Theta, N, J$) via `RationalUnit`.
  - `Quantity` type representing physical scalar values with units and uncertainty.
  - Automatic unit reduction, conversion (`to`), and SI base normalization.
