# Physure Roadmap

*Last reviewed: 2026-11-16. Last release: 0.4.0. Anything landed since is listed under
"Unreleased" in the [changelog](https://github.com/Alexisrx96/physure/blob/main/CHANGELOG.md).*

Physure is a calculation engine that keeps units and uncertainties correct. It is a Rust core
(`physure-core`), a small language for calculations (PHS, `physure-script`), and thin bindings
over the core: Python, WebAssembly, Java, the `phs` command line and a language server.

This page says what we are building toward, what we are working on, and in what order. It is the
only place that tracks status. The sub-roadmaps linked at the end hold the designs, not the
status.

---

## Goals

**Physure is for the calculation you have to defend.** A result should have the right units, a
stated uncertainty, and a way for someone else to reproduce it exactly, later, on another
machine.

Four goals follow from that, in order of importance:

1. **No silently wrong answer.** A dimension, a conversion factor or an uncertainty is never
   dropped. When an operation cannot keep them, it is an error. This covers the language too: a
   comparison, a condition or a default that could hide a mistake is refused, not guessed.
2. **Results that can be reproduced and audited.** The configuration is frozen and sealed, the
   engine is pinned, reference calculations are re-run to prove nothing moved, randomness is
   seeded, and every number that affects a result has a known origin.
3. **One implementation, every surface.** Physics and unit logic exist once, in the Rust core.
   PHS, Python, WebAssembly, Java, the CLI, the language server and the agent tools (`phs mcp`)
   delegate to it. A feature is not finished while one surface computes it differently, or
   cannot reach it.
4. **Domain breadth on that foundation.** Chemistry, thermodynamic properties and symbolic
   mathematics are added through the same pipeline, so a new domain is as trustworthy as a unit
   conversion. Breadth never comes before the first three goals.

## How we prioritise

Work is taken in this order. An item higher in the list interrupts one lower down.

1. **A defect that gives a wrong answer without saying so.**
2. **Work already in progress.** Finish before starting something else.
3. **Publishing what is done.** Finished work that is not released helps nobody.
4. **Parity.** Closing the gap between what the core can do and what each surface exposes.
5. **New domains**, bounded ones first.

Work without a finish line is not scheduled. It is listed under [Not planned](#not-planned) with
what would bring it back.

## What "done" means

A feature is done when all of these hold:

1. It lives in the Rust core, with exact arithmetic wherever the mathematics is exact.
2. Its numbers are catalog data, so `phs conf` can freeze them. None stay hidden in code.
3. Its reference values are witnesses, each checked against an independent published source.
4. PHS exposes it, and `phs check` knows its dimensions.
5. Python, WebAssembly and Java delegate to the core, held by the shared conformance tests.
6. The guide, the changelog and this page say so.

---

## Where things stand

| Area | State | Open work |
|---|---|---|
| **Units, dimensions, conversion** | Done. The Rust core owns the unit algebra; Python keeps no implementation of its own. | The catalog's constants carry no uncertainty. |
| **Uncertainty** | Done: GUM propagation with covariance, Monte Carlo, unscented, asymmetric pairs, effective degrees of freedom and coverage factors. | Open audit findings; the asymmetric-uncertainty proposal ([#46](https://github.com/Alexisrx96/physure/issues/46)). |
| **PHS language** | Lab-ready: loops, concurrency, debugger, compiled exports, decorators. Its semantics were tightened after the November audit. | A stability line; open audit findings. |
| **Tools and agent surface** | Done: static dimension checking (`phs check`), sandboxed evaluation (`phs eval`, `phs unit`), the MCP server (`phs mcp`), the language server. | A machine-readable language specification. |
| **Bindings** | Python, WebAssembly and Java wrap the core and pass the shared conformance tests. | They refuse to run under a frozen configuration until Phase 5. |
| **Reproducibility (`phs conf`)** | Phases 1 to 4 done: sealed file, engine pin, the eight commands, witnesses, and the numbers that lived in code moved into the catalog, with a [coverage table](conf_coverage.md). | Phase 5 not started. |
| **Chemistry** | The Python extension and the Rust core are done. | PHS builtins; Python still runs its own copy; reaction networks. |
| **Symbolic mathematics** | Phase 1 only: derivatives, heuristic integrals, series, simple ODEs and small matrices, on floating-point coefficients. | Engine defects, then exact coefficients, then polynomials. |
| **Physical models** | Welch–Satterthwaite done. | Buckingham π, real-gas equations of state, steam tables: not started. |
| **Diagrams** | Not started. | Parked. See [Not planned](#not-planned). |

---

## Now

In order. Nothing under [Next](#next) starts before these are finished.

### 1. Finish Phase 4b of the configuration freeze *(done)*

Every number a user would want to pin, audit or override moves out of the code and into the
catalog, so a frozen file governs it. What cannot move is listed in a public coverage table, so
nobody believes something is frozen when it is not.

- [x] Settings for the Monte Carlo sample count and seed, the `while` ceiling, the asymmetric
      shape and the default degrees of freedom. Monte Carlo becomes deterministic by default
      (seed 0), with random draws as an explicit choice.
- [x] The "did you mean" hint for an unknown unit is deterministic.
- [x] Display rules are catalog settings, and printed text is covered by witnesses.
- [x] Standard enthalpies and entropies of formation are catalog data in the core.
- [x] Exchange rates are shipped catalog text, not Python literals.
- [x] The coverage table (`docs/conf_coverage.md`), with a test that fails when it drifts.

Source: the
[audit of code-resident numbers](https://github.com/Alexisrx96/physure/blob/main/docs/superpowers/audits/2026-11-14-code-resident-numbers.md).

### 2. Publish the next release *(done: 0.4.0, 2026-11-16)*

0.4.0 shipped everything since 0.3.0, including the breaking changes to PHS: operator
precedence, a zero needs a unit, a condition must be a Bool, `pi` and the catalog constants are
reserved names, Monte Carlo is seeded by default, and a module runs in its own scope. The
changelog opens with what to change when upgrading from 0.3.x.

- [x] The changelog covers the work since `v0.3.0`. It was checked by feature (chemistry, angle
      mode, the configuration freeze phases, witnesses, degrees of freedom, the PHS tools, the
      bindings), not commit by commit.
- [x] The Python, Rust, Java and WebAssembly packages are published (PyPI, crates.io, Maven
      Central, npm).
- [x] The release workflows run end to end. The bumps of Python, core and Java started their
      releases by dispatch. The npm one did not at first: `wasm-release.yml` had no dispatch
      trigger, and it took four attempts. That is fixed.
- [ ] npm 0.4.0 has no README or keywords: they were committed after it was published. They
      appear with the next npm version.

### 3. Close the open audit findings

The November audit found defects beyond the ones already fixed. The ones that can give a wrong
answer come first:

- [x] The symbolic engine dropped comparisons: `integral("x < 0.5", "x")` answered `x^2/2` (T2). The parser stopped at the first token it did not know and dropped the rest, so it now refuses any text it did not read, and names a comparison.
- [x] The symbolic parser read `and`, `or` and `not` as symbols and multiplied them: `simplify("x and y")` was `and * x * y` and `deriv("x not y", "x")` was `not * y`. Fixed in phase 2 of the reserved names.
- [ ] Reserved names that no program can bind (design: `docs/superpowers/specs/2026-11-16-reserved-names-design.md`). Phases 1 (the table, the interpreter, the imports) and 2 (`phs check`, the symbolic parser, the language server) are done. Phase 3 (hosts, the generated section of the language guide) is open. Known gaps recorded in the spec §11: the Java equation route, plugins, two wildcard imports with a shared name.
- [x] `use * from matrix` panicked; it is an alias of `array` now.
- [x] Only the first call of a user function is inlined in a symbolic expression (T3); arguments were not substituted either. Fixed in Phase 4b.
- [ ] An unbound name evaluates to a string of itself instead of being an error (T5).
- [x] Covariance pruning read a missing variance as an absolute `1.0`, whatever the unit (T7). A block whose variances are not both stored is kept now. It was latent: no public call sequence reaches it, and pruning is off by default.
- [x] `e` meant Euler's number in a symbolic string and was refused elsewhere (T13). It is a name like any other in both now, and `e^x` in a symbolic text is refused (write `exp(x)`). The same change fixed a silent wrong answer: a name ending in `e` before `^` was cut into the name and `e`, so `deriv("time^2", "time")` answered 0.
- [ ] A compiled export without contracts has no way to report a failure (T15).
- [ ] The JavaScript and TypeScript targets emit calls the binding does not have (T19).

Then the ones that fail loudly or only read badly: T1 (reporting half), T4, T8, T9, T11, T16
and T21 (T18 and T20 are fixed). The full list, with reproductions, is in section 11.2 of the audit.

---

## Next

In order, after "Now".

### 4. Phase 5 of the configuration freeze: the bindings

Python, WebAssembly and Java honour a frozen configuration. Today Python and Java refuse to run
while one is in force, so the audit features do not reach most users. Includes the two items
deferred from Phase 4: the Python default coverage probability, and the unit display tables
derived from the catalog.

**Done when** the same frozen file gives the same witnesses through every binding.

### 5. Chemistry on every surface

- PHS builtins: `species()`, `molar_mass()`, `balance()`, `arrhenius()`, `gibbs()`
  ([Phase 6](chemistry_roadmap.md)).
- Python delegates to the core, and its 681 lines of independent chemistry are deleted.
- A sourced thermochemistry edition with uncertainties. The first edition keeps the legacy
  Python numbers unchanged and is named as such.

**Done when** a chemistry result is identical in PHS and Python and is covered by a witness.

### 6. Constants with uncertainties

None of the 411 constants in the catalog carries an uncertainty, and the catalog grammar has no
way to write one (audit T14). For a tool about uncertainty, the gravitational constant should not
be exact.

**Done when** the CODATA constants carry their published standard uncertainties and propagate
like any measurement.

### 7. A stability line for PHS

The language changed in breaking ways in November. Users need to know what will not change
again.

**Done when** there is a language reference, a conformance suite that every code-generation
target passes, and a written deprecation policy.

### 8. Documentation for users

The documentation site is mostly roadmaps and research notes. It needs task guides: an
uncertainty budget from start to finish, checking a script with `phs check`, freezing a
configuration for an audit, and using Physure from an agent through `phs mcp`.

---

## Later

Each item is bounded and has a published reference to test against. Each starts with a short
specification that states its done-criterion, and goes through the pipeline under
[What "done" means](#what-done-means).

| Item | Done when | Design |
|---|---|---|
| Buckingham π in the core | Dimensionless groups come from an exact rational nullspace, in PHS and Python. Today only a neural-network layer in Python does this. | [Scientific companion](scientific_roadmap.md) |
| Real-gas equations of state | Peng–Robinson and Redlich–Kwong reproduce published values. | [Scientific gaps](scientific_gaps_and_rigor.md) |
| Steam tables (IAPWS-IF97) | The standard's own verification tables pass as witnesses. | [Scientific companion](scientific_roadmap.md) |
| Exact coefficients in the symbolic engine | Numeric leaves are exact rationals, not `f64`. Everything below depends on this. | [Symbolic mathematics](symbolic_math_roadmap.md) |
| Multivariate polynomials and Gröbner bases | Canonical forms and Buchberger's algorithm on exact coefficients. | [Symbolic mathematics](symbolic_math_roadmap.md) |
| Partial fractions, series, fraction-free determinants | Exact results, replacing today's floating-point ones. | [Symbolic mathematics](symbolic_math_roadmap.md) |
| Reaction kinetics | Multi-reaction networks integrated as ODEs, with units. | [Chemistry, Phase 7](chemistry_roadmap.md) |
| Chemistry in WebAssembly and Java | The conformance tests cover chemistry. | [Chemistry, Phase 8](chemistry_roadmap.md) |
| Debug adapter and cross-file incremental analysis | The two follow-ons left open by the language-readiness work. | [Language readiness](language_readiness_roadmap.md) |
| An independent external validator | A separate project re-derives results as a black box and reports disagreements. | [Design](https://github.com/Alexisrx96/physure/blob/main/docs/superpowers/specs/2026-10-30-external-scientific-validator-design.md) |

### Needs scoping first

These are worth doing but have no finish line as written. Each needs a bounded definition before
it can be scheduled.

| Item | What has to be decided |
|---|---|
| Risch integration | Which case. The logarithmic and exponential case is bounded; the full algorithm is not. |
| `dsolve` and `pdesolve` | A named list of equation classes. |
| Verified interval arithmetic (IEEE 1788) | Which conformance level, and whether it is an evaluation mode. This is the strongest form of goal 1: a result with a guaranteed enclosure. |
| Chemical equilibrium (NASA CEA) | Where the thermodynamic data comes from, and under what licence. |
| DAE index reduction (Pantelides) | An equation-system model, which does not exist yet. |
| Anisotropic tensors | Which ranks and which operations. |
| Kalman filtering, Polars and xarray accessors | Whether they belong in the core or stay Python extras. |
| Free-body and trajectory diagrams | Whether a drawing feature belongs in a calculation engine at all. |
| Faster Gröbner bases (F4/F5) | A benchmark that shows Buchberger is too slow for a real use. |

## Not planned

Parked on purpose. They stay in the sub-roadmaps as designs.

| Item | Why | What would bring it back |
|---|---|---|
| SAT solver, number theory, combinatorics | General-purpose mathematics, far from units and uncertainty. | A physical-modelling feature that needs one. |
| Jordan, Smith and Frobenius canonical forms | Same. | Same. |
| Noether symmetry derivation | A research topic, with no bounded specification. | A specification with reference problems. |
| Circuit schematics, field lines, VSEPR geometries, TikZ export | Drawing, not calculation. | Demand from users, after the first diagrams exist. |
| A curated formula repository | Parked since the language-readiness work. | An explicit decision to start it. |
| Unit objects resolved at `torch.compile` trace time | Measured: the graph breaks are not the bottleneck. | A benchmark that shows otherwise. |
| Extracting the symbolic engine into its own crate | Measured: no binding could drop the interpreter. | A binding that can. |

---

## Done

**Core and units**

- The Rust core owns the unit algebra, conversion and dimensions; Python delegates (0.3.0).
- Exact scales, temperatures as points and intervals, logarithmic units, exact fractional powers.
- Angle mode: the radian and the steradian read as 1 in conversions, never in sums or comparisons.
- One rule for how a unit is written, and it reads back.

**Uncertainty**

- GUM propagation with covariance, Monte Carlo, unscented and asymmetric uncertainties.
- Effective degrees of freedom, coverage factors and expanded uncertainty (Welch–Satterthwaite)
  in the core, PHS, Python, WebAssembly and Java
  ([design](https://github.com/Alexisrx96/physure/blob/main/docs/superpowers/specs/2026-11-01-welch-satterthwaite-design.md)).
- A quantity keeps the sources of its uncertainty, in Java too.

**PHS language and tools**

- Lab-ready: loops, parallel loops, a debugger, incremental language server, compiled exports,
  decorators ([language readiness](language_readiness_roadmap.md)).
- Static dimension checking: `phs check` finds a dimension mismatch without running the program.
- Agent surface: a sandboxed interpreter, `phs eval`, `phs unit`, and the `phs mcp` server.
- Language semantics after the audit: arithmetic binds tighter than conversion and comparison, a
  zero needs a unit, a condition is a Bool, `pi` and the catalog constants are global, one-sided
  limits.

**Reproducibility**

- `phs conf`, Phases 1 to 4: a sealed frozen file with a hash-chained history, the engine pinned
  by version, commit, build and binary hash, the eight commands with JSON output and exit codes,
  and witnesses (63 built-in reference calculations plus the user's own scripts, compared bit for
  bit); the numbers that lived in code are catalog settings and sections, listed in the
  [coverage table](conf_coverage.md) ([user guide](conf_freeze.md),
  [design](https://github.com/Alexisrx96/physure/blob/main/docs/superpowers/specs/2026-11-07-conf-freeze-design.md)).
- Frozen atomic-weight editions (CIAAW 2024) ([guide](atomic_weights.md)).
- The audit of numbers that live in code, and the fixes for the defects it found.

**Chemistry**

- The Python extension: species, molar equivalency, reaction balancing, thermochemistry and
  kinetics helpers.
- The Rust core module: formula parser, molar mass with uncertainty, exact rational balancing,
  Arrhenius, Gibbs and Clausius–Clapeyron.

**Bindings and project**

- Java and WebAssembly are held to the core by shared golden conformance tests, on every
  platform the Java library ships for.
- First use of the Python package is fast, and parsing a unit no longer copies the catalog.
- 0.4.0 is published on PyPI, crates.io, Maven Central and npm. The release pipeline starts from
  the version bump, and the four bump workflows have run in a real release.

---

## Sub-roadmaps and designs

These hold designs, mathematics and references. Status lives on this page only.

| Document | Covers |
|---|---|
| [Chemistry](chemistry_roadmap.md) | Species, reactions, thermochemistry, the port to the core and PHS. |
| [PHS language readiness](language_readiness_roadmap.md) | Loops, concurrency, debugger, language server, exports, decorators. |
| [Symbolic mathematics](symbolic_math_roadmap.md) and its [research notes](symbolic_math_research.md) | The computer-algebra design, Phases 1 to 4. |
| [Scientific companion](scientific_roadmap.md) | Buckingham π, steam tables, data-frame accessors, ODEs, Kalman filtering. |
| [Scientific gaps and rigor](scientific_gaps_and_rigor.md) | Equations of state, tensors, interval arithmetic, DAEs, symmetries. |
| [Paper compliance and validation](paper_compliance_and_validation.md) | Where the code follows, or departs from, the cited literature. |
| [Diagramming engine](diagrams_roadmap.md) | Parametric physical diagrams (parked). |
| [Configuration freeze: design](https://github.com/Alexisrx96/physure/blob/main/docs/superpowers/specs/2026-11-07-conf-freeze-design.md) | The frozen file, the engine pin, witnesses, the phases. |
| [Audit of code-resident numbers](https://github.com/Alexisrx96/physure/blob/main/docs/superpowers/audits/2026-11-14-code-resident-numbers.md) | Every number that lives in code, and the open findings. |

## Keeping this page current

1. **Update it in the same branch as the work.** When a phase lands or a new subsystem is
   designed, edit this page before the branch merges. The sub-roadmap changes only if the design
   changed.
2. **One page.** `ROADMAP.md` at the repository root only points here. Do not copy status into
   it or into a sub-roadmap.
3. **No percentages.** An item is done, in progress, or not started. A new item needs a
   "done when".
4. **Links.** Other pages under `docs/` are linked relatively. Anything outside it, including
   `docs/superpowers/` and source files, uses the full
   `https://github.com/Alexisrx96/physure/blob/main/<path>` form, because the documentation
   build fails on a relative link that leaves the docs tree.
5. **Citations.** A new algorithm or physical model needs a published reference in its
   sub-roadmap before it moves out of "Needs scoping first".
