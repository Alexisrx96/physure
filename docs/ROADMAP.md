# 🗺️ Physure Master Development Roadmap

**Central Status & Subsystem Progress Tracker**

Physure is a zero-dependency, unit-aware, dimension-correct scientific computing ecosystem built on a high-performance Rust core (`physure_core`) with multi-language bindings (Python, C, Java, WebAssembly) and standalone DSL (`phs`).

This document serves as the **single source of truth** for tracking project-wide progress, sub-roadmaps, subsystem milestones, and architectural goals across the entire workspace.

---

## 📊 Master Subsystem Progress Dashboard

| Subsystem / Sub-roadmap | Status | Progress | Target Path / Sub-roadmap Document | Key Modules |
|---|---|---|---|---|
| **1. Core Physics & Unit Engine** | ✅ Complete | 100% | `physure-core/src/`, `physure-python/physure/domain/measurement/` | Base units, dimensions, quantities, JIT/AOT |
| **2. Uncertainty & Covariance Engine** | ✅ Complete | 95% | `physure-core/src/uncertainty/`, `physure-python/physure/domain/uncertainty/` | GUM covariance, Monte Carlo, affine arithmetic |
| **3. Chemistry & Reaction Tracking** | 🚧 Python ✅ / Rust core ✅ / PHS + bindings Planned | 60% | [Chemistry Roadmap](chemistry_roadmap.md) | `physure.ext.chemistry` → `physure_core::chemistry` + PHS builtins |
| **4. Symbolic Math & CAS Engine** | 🚧 Phase 1 Done / Phase 2–4 In Progress | 35% | [Symbolic Math Roadmap](symbolic_math_roadmap.md) | `physure_core::cas`, Gröbner, Risch, ODEs, SAT |
| **5. Diagramming Engine (`physure.diagrams`)**| 📋 Planned | 10% | [Diagramming Engine Roadmap](diagrams_roadmap.md) | Free-body diagrams, vectors, circuits, TikZ/SVG |
| **6. Scientific Companion & Metrology** | 🚧 In Progress | 40% | [Scientific Companion Roadmap](scientific_roadmap.md) | Buckingham $\pi$, IAPWS-IF97, EKF, Polars/Xarray |
| **7. Metrological Rigor & Physical Reality Gaps**| 🚧 In Progress | 17% | [Scientific Gaps & Rigor](scientific_gaps_and_rigor.md) | Welch-Satterthwaite, DAEs, Real EOS, Intervals |
| **8. PHS Language Readiness (LAB-READY)** | ✅ Complete | 100% | [Language Readiness Roadmap](language_readiness_roadmap.md) | `physure-script` loops/concurrency, `physure-cli` debugger, `DebugHook` |
| **9. Reproducibility & Audited Use (`phs conf`)** | 🚧 Phase 1 (core: 1a file model and seal, 1b engine identity, 1c resolution, sealed loading, freeze/update/seal operations, 1d entry listing and semantic diff) and Phase 2 (the eight `phs conf` commands, JSON output, exit codes) done; witnesses (Phase 3) next | 40% | [Design spec](https://github.com/Alexisrx96/physure/blob/main/docs/superpowers/specs/2026-11-07-conf-freeze-design.md), [User guide](conf_freeze.md) | Frozen configuration, engine pin, witnesses, `phs conf freeze/diff/update` |

---

## 🗺️ Sub-Roadmap Registry

> 🧭 **Active execution plan:** [Implementation Plan — Chemistry Core Port, then CAS Phase 2](implementation_plan.md). The sub-roadmaps below define *what* and *why*; that document defines *how* and *in what order*.

For detailed architectural designs, mathematical specifications, API proposals, and paper citations for each domain, consult the dedicated sub-roadmaps:

1. 🧮 **[Symbolic Math & Advanced CAS Engine Roadmap](symbolic_math_roadmap.md)**
   * Multivariate polynomials, Gröbner bases ($F_4/F_5$), Partial fraction decomposition, Taylor/Laurent series, Risch integrator, `dsolve`, `pdesolve`, symbolic linear algebra (Bareiss, Jordan form), number theory, and DPLL SAT solver.
   * Companion research: **[Symbolic Math Scientific Research](symbolic_math_research.md)**.

2. 🧪 **[Chemistry & Physical-Chemical Reaction Tracking Roadmap](chemistry_roadmap.md)**
   * **Phases 1–4** (✅): Chemical formula parsing, IUPAC molar masses, substance-aware mass-to-moles equivalencies, reaction balancing, stoichiometry yield calculator, and thermochemistry/kinetics helpers.
   * **Phases 5–8** (🚧): Rust core chemistry module (`physure_core::chemistry`), PHS DSL native builtins (`species()`, `balance()`, `arrhenius()`), multi-reaction networks with kinetic ODE integration, chemical equilibrium solver (NASA CEA), and cross-language bindings (C/FFI, WASM, JNI).

3. 📐 **[Parametric Physical & Scientific Diagramming Engine Roadmap](diagrams_roadmap.md)**
   * Physically parameter-driven diagramming (`physure.diagrams`), free-body diagrams, particle trajectories with uncertainty cones/ellipses, electrical schematics, field lines, and SVG/TikZ/Matplotlib/ASCII exporters.

4. 🔬 **[Scientific Companion & Metrology Roadmap](scientific_roadmap.md)**
   * Buckingham $\pi$ theorem nullspace solver, GUM uncertainty propagation, IAPWS-IF97 steam tables, Polars/Xarray unit accessories, and unit-aware extended Kalman filters (EKF).

5. ⚖️ **[Scientific Gaps, Metrological Rigor & Reality Analysis](scientific_gaps_and_rigor.md)**
   * GUM Welch-Satterthwaite effective degrees of freedom ($\nu_{\text{eff}}$), Peng-Robinson / IAPWS real gas equations of state, 3D anisotropic tensors (ISO 80000-4), Pantelides DAE index reduction, and IEEE 1788 verified interval arithmetic.

6. 📜 **[Paper Compliance, Validation & Theoretical Reconciliation](paper_compliance_and_validation.md)**
   * Codebase vs. paper compliance matrix, computational limitations, theoretical contradictions resolution (GUM 1st order vs Monte Carlo, Interval vs Affine, Buchberger vs F4/F5), and open `[TODO: Paper Compliance Verification]` registry.

7. 🧵 **[PHS Language Readiness Roadmap](language_readiness_roadmap.md)**
   * `for`/`while` loops, `rayon`-backed data-parallelism and `parallel_map`, and an interpreter `DebugHook` powering a `phs debug` CLI debugger (DAP adapter staged as a stretch goal) — the path to the LAB-READY milestone.

8. 🔒 **[`phs conf`: freezing the whole configuration](https://github.com/Alexisrx96/physure/blob/main/docs/superpowers/specs/2026-11-07-conf-freeze-design.md)**
   * For audited settings: the data (conf sections, unit systems, constants), the engine (pinned by version, commit, build and binary hash) and the environment are frozen into a sealed `physure.frozen.conf` with a hash-chained history; `phs conf freeze / diff / update / seal / verify` change it only deliberately. Design approved 2026-11-07; Phases 1a (file model and seal), 1b (engine identity and its pinning policies) and 1c (resolution, sealed loading that fails closed for every consumer, the freeze/update/seal operations) are done, and so are 1d (the registry entry listing) and Phase 2 (the eight `phs conf` commands with `--json` schema 1 and exit codes 0/1/2/3, `verify --expect`, a compare-and-swap for `update`/`seal`, the `phs upgrade` hint; [user guide](conf_freeze.md)); witnesses (Phase 3) are next; the Python and Java bindings refuse while a frozen file is in force until Phase 5 wires them.

---

## 🏁 Consolidated Master Milestones

```mermaid
timeline
    title Physure Master Development Milestones
    Done : Rust Core Engine : phs conf Commands (Freeze, Verify, Diff, Update, Seal, History) : Rust Core Chemistry Module (formulas, molar mass, balancing, thermo) : Frozen Atomic-Weight Editions (CIAAW 2024) : Correlated Uncertainty & Covariance : Chemistry Python Extension (Species & Reactions) : Native Symbolic AST & Regression : PHS LAB-READY (loops, concurrency, debugger, incremental LSP, compiled exports, decorators) : Rust-Owned Unit Algebra (A1) : GUM Welch-Satterthwaite : Angle Mode
    Now : PHS DSL Chemistry Builtins : Python Delegates to the Core Chemistry : phs conf (Freezing the Whole Configuration, witnesses (Phase 3)) : Buckingham Pi Nullspace Solver
    Next : CAS Exact-Arithmetic Placement (S0) : Multivariate Polynomials & Gröbner Bases (F4/F5) : Partial Fraction Decomposition : Risch Integration Engine : Analytical ODE/PDE Solvers (dsolve/pdesolve) : Real Gas EOS
    Later : Advanced Reaction Networks & Equilibrium : Cross-Language Chemistry Bindings : Bareiss Symbolic Matrices & Forms : Parametric Free-Body Diagrams (physure.diagrams) : DAE Pantelides Index Reduction : Anisotropic 3D Tensors : Verified Interval Arithmetic (IEEE 1788) : DPLL SAT Solver & Noether Symmetries
```

---

## 🔍 Subsystem Milestone Matrix

### Subsystem 1: Core Physics & Unit Engine (100% ✅)
* [x] Base unit dictionary, prefix system, and SI rational unit vectors in Rust.
* [x] Zero-copy Python bindings via PyO3 (`physure._core.Quantity`).
* [x] `torch.compile` and `jax.jit` zero-overhead tracer compatibility.
* [x] Standalone PHS DSL interpreter (`physure-cli`, `physure-script`).
* [x] Unit algebra, conversion and dimensions owned by the Rust core; Python delegates (A1 program).
* [x] Angle mode: `rad` and `sr` read as 1 for conversions, never for sums or comparisons.
* [x] PHS LAB-READY: loops, concurrency, breakpoints, incremental LSP, compiled exports, decorators (Tracks A–F; the DAP adapter and cross-file incrementality remain as non-blocking follow-ons).

### Subsystem 2: Chemistry & Physical-Chemistry (50% 🚧)
* [x] **Phase 1–4 Python Extension** (`physure.ext.chemistry`): Species, molar equivalency, reaction balancer, thermo/kinetics.
* [ ] **Phase 5**: Rust core chemistry module (`physure_core::chemistry`) — periodic table, recursive descent formula parser, exact rational RREF balancer (✅ done in the core); PyO3 bindings (not done).
* [ ] **Phase 6**: PHS DSL native chemistry builtins — `species()`, `molar_mass()`, `balance()`, `arrhenius()`, `gibbs()`, `mass_to_moles()`, `moles_to_mass()`.
* [ ] **Phase 7**: Advanced reaction networks — multi-reaction stoichiometric matrix, kinetic ODE integration (RK4), chemical equilibrium solver (NASA CEA algorithm).
* [ ] **Phase 8**: Cross-language chemistry bindings — C/FFI (`physure.h`), WebAssembly (`wasm-bindgen`), Java/JNI.

### Subsystem 3: Symbolic Math & CAS Engine (35% 🚧)
* [x] **Phase 1**: Base `Expr` AST, basic simplification, derivatives, heuristic integration, AOT physics compilation, dimensional symbolic regression.
* [ ] **Phase 2**: Multivariate polynomials, Gröbner bases (Buchberger/F4/F5), Hermite partial fraction decomposition, Taylor/Laurent series.
* [ ] **Phase 3**: Risch decision-procedure integrator, analytical ODE solver (`dsolve`), PDE solver (`pdesolve`).
* [ ] **Phase 4**: Symbolic linear algebra (Bareiss fraction-free det, Jordan/Smith canonical forms), number theory, combinatorics, and DPLL SAT solver.

### Subsystem 4: Scientific & Metrological Rigor Gaps (17% 🚧)
* [x] **Welch-Satterthwaite $\nu_{\text{eff}}$**: Effective degrees of freedom and Student-$t$ coverage factor $k_p$ (GUM §4.2, G.4). Core, PHS (`dof` clause and builtins), Python, WebAssembly and Java (see `docs/superpowers/specs/2026-11-01-welch-satterthwaite-design.md` and, for Java's quantities keeping their sources, `docs/superpowers/specs/2026-11-02-java-provenance-design.md`).
* [ ] **Real Equations of State**: Peng-Robinson, Redlich-Kwong, and IAPWS-IF97 steam formulation.
* [ ] **Anisotropic Physical Tensors**: 2nd and 4th-order tensors for stress-strain ($\boldsymbol{\sigma}, \boldsymbol{\varepsilon}$) and dielectric permittivity (ISO 80000-4).
* [ ] **Verified Interval Arithmetic**: IEEE 1788 directed interval bounds to prevent floating-point rounding errors.
* [ ] **DAE Pantelides Index Reduction**: Structural index reduction and initialization for coupled physical systems.
* [ ] **Noether Symmetry Derivation**: Symbolic derivation of conserved quantities from Lagrangian/Hamiltonian symmetries.

### Subsystem 5: Parametric Diagramming Engine (`physure.diagrams`) (10% 📋)
* [ ] **Phase 1**: Free Body Diagrams (FBD) on inclined planes, weight/normal/friction force scaling, SVG & Matplotlib renderers.
* [ ] **Phase 2**: Particle kinematics, coordinate transforms, uncertainty error cones, and covariance ellipses.
* [ ] **Phase 3**: Electrical circuit schematics, 2D field line tracing, thermodynamic $P-V$ work integrals.
* [ ] **Phase 4**: Chemistry reaction profiles, VSEPR geometries, and electrochemical cells.
* [ ] **Phase 5**: LaTeX TikZ exporter and terminal ASCII renderer.

### Subsystem 6: Reproducibility & Audited Use (40% 🚧)
* [x] **Frozen atomic-weight editions**: named, immutable CIAAW editions with a fingerprint test, tables of your own, and the edition report (`atomic_weight_edition()`).
* [x] **Phase 1 — core**: **1a done** (`physure_core::frozen`: SHA-256, canonical form, frozen-file model, hash-chained history, seal and verifier); **1b done** (`physure_core::frozen::engine`: the engine identity from a `build.rs` with a tested commit rule, the four policies `binary`/`build`/`release`/`record`, the strict `[Engine]` block, the `[Environment]` record, and each history entry recording the engine block it sealed); **1c done** (`physure_core::frozen::{active, load, ops}`: resolution from the host's explicit path, `PHYSURE_CONF`, discovery and `PHYSURE_REQUIRE_FROZEN`; sealed loading that replays only the file's sources, strictly; the catalog hooks fail closed for the core, PHS, the CLI and the language server, `ensure_valid()` with exit codes 1/2/3, one configuration per process; the per-process caches; `freeze`, `plan_update`, `plan_seal` and `write_atomic`; Python and Java refuse until Phase 5); **1d done** (`physure_core::frozen::entries`: every catalog as a sorted list of entries covering all 18 fields of the registry, the constants and the settings with their compiled defaults, and `diff`/`summary` of two such lists; a test mutates every field of `UnitRegistry` and checks the entries notice, and the destructuring without `..` makes a new field a compile error).
* [x] **Phase 2 — CLI**: `phs conf` `status`, `show`, `freeze`, `verify`, `diff`, `update`, `seal`, `history` (`physure-cli/src/conf.rs`) over a report layer in the core (`physure_core::frozen::{report, origin}`: `inspect`, `compare`, `history_rows`, `entries_with_origin`, `overrides_contribution`); `--json` with `"schema": 1`; exit codes 0/1/2/3; `verify --expect` against a seal recorded elsewhere; `update`/`seal` confirm, and refuse to write a file changed since they read it; `phs conf` is dispatched before any other argument handling and exempt from the pre-command check; `phs upgrade` suggests `phs conf diff`. User guide: [Freezing the configuration](conf_freeze.md).
* [ ] **Phase 3 — witnesses** (next): built-in reference calculations and user scripts, `freeze --witness`, `verify --run`.
* [ ] **Phase 4 — data migration**: code-resident numbers (currency rates, Monte Carlo defaults, Python chemistry tables) move into the catalog; a public table of what is and is not frozen.
* [ ] **Phase 5 — bindings**: Python, WebAssembly and Java honour the frozen file (until then Python and Java refuse while one is in force).

---

## 🤝 Maintenance & Updating Guidelines

1. **Central Progress Verification**: When completing a task or PR in any subsystem, update both the target sub-roadmap and this Master Roadmap's progress table.
2. **Relative Linking**: All documentation links MUST remain relative (e.g. `symbolic_math_roadmap.md`) to guarantee portability across GitHub, MkDocs, and local clones.
3. **Paper Citations**: Any new algorithm or physical model added to the roadmap must include peer-reviewed scientific paper citations with public DOI/web links in [`symbolic_math_research.md`](symbolic_math_research.md) or [`scientific_gaps_and_rigor.md`](scientific_gaps_and_rigor.md).
