# Effective degrees of freedom, coverage factor and expanded uncertainty (Welch–Satterthwaite) — design

**Date:** 2026-11-01
**Status:** approved by the maintainer ("Proceed", 2026-11-01) after five decisions (table below). Built in four stages, D1 to D4.
**Base:** local `main` @ `7786740` (PR #71). Independent of `perf/parse-expression-no-copy`.
**Origin:** the third item of a ranked list of bottlenecks; the only one of the three that adds a capability. The gap is documented in `docs/scientific_gaps_and_rigor.md` §1.1, `docs/ROADMAP.md` and the GUM compliance note (§2: "Expanded uncertainty: not implemented"; §10, Phases 2 and 3).

## Decisions (maintainer, 2026-11-01)

| Question | Decision | Rejected |
|---|---|---|
| Reach | **Scalars through the lineage** (the model every binding shares) | scalars and arrays together; core only |
| Declaring ν | **A new literal syntax in PHS**, plus functions elsewhere | functions only; Python and Rust only |
| The PHS syntax | **A keyword after the value**: `10.003 +/- 0.002 dof 9 V` (`ν 9`, `dof inf`) | a parenthesised group `(dof 9)`, which collides with the asymmetric pair `(0.5, 0.4)` |
| Using a fractional ν_eff | **Truncate, as JCGM 100:2008 G.4.1**; ν_eff is reported untruncated | truncate by default with a switch; always the fractional value |
| Bindings | **All five**: Rust, PHS, Python, WASM, Java | Rust, PHS and Python first; Rust and PHS only |

## What the code has and lacks

Every term of a `Lineage` is `(source id, c_i · u_i)`: the contribution of one independent source to the standard uncertainty of the value. That is the numerator of Welch–Satterthwaite, `ν_eff = u_c⁴ / Σ (c_i u_i)⁴ / ν_i`, already computed. Because a lineage merges shared sources by id, the formula is valid for correlated inputs too: it sums over independent sources, not over the inputs of the model.

Missing: the degrees of freedom ν_i of each source, a Student-t quantile (`physure-core` has no special functions and no statistics crate), and any way to declare, read or report them. `docs/uncertainty-gum-compliance.md` §3 notes that two quantities with the same `u` are indistinguishable whether they came from observations or from a certificate; ν is the first piece of that evaluation metadata to be kept, in the shape the compliance note's Phase 2 asks for (metadata once per source, arithmetic on compact coefficients).

## Design

### 1. Model (`physure-core::uncertainty`)

- **A side table** `source id -> ν`, process-wide (`RwLock<HashMap<u32, f64>>`). Only finite ν are stored; a missing entry is ν = ∞. Ids are never reused, so an entry cannot belong to another source. Nothing is added to a lineage term and the arithmetic does not change.
- `Lineage::effective_dof()`: `∞` when no source of the lineage has a finite ν (and for an exact quantity); otherwise `u_c⁴ / Σ_{ν_i finite} c_i⁴ / ν_i`.
- **Declaring.** `Quantity::with_dof(ν)` needs a Gaussian quantity with **exactly one source**; ν must be positive (or ∞). A source's ν can be declared once: the same value again is a no-op, a different one is an error, so a later call cannot change a source that other copies of the value share. A quantity with several sources, an exact one, or one of another model is an error.
- `Quantity::from_samples(&[f64], unit)`: Type A. Mean, `s/√n`, ν = n − 1, n ≥ 2; all samples equal gives an exact quantity.
- `effective_dof`, `coverage_factor(p)` and `expanded(p)` exist for the Gaussian model; the Moments, Monte Carlo and unscented models, and anything not scalar, **raise** with the reason. They never return a number that was not computed.

### 2. Student-t in the core (`physure-core::math::stats`)

`ln_gamma`, the regularized incomplete beta (continued fraction) and gamma, and `student_t_two_sided(p, ν)`: the `k_p` with `P(|T_ν| < k_p) = p`, found by bracketing and bisection on the two-sided tail, with the normal quantile for ν = ∞. No new dependency.

`coverage_factor(p) = student_t_two_sided(p, max(1, floor(ν_eff)))` (ν_eff = ∞: the normal quantile, 1.960 for 95 %). `expanded(p)` returns a struct `Expanded { value, standard_uncertainty, expanded_uncertainty, coverage_factor, coverage_probability, dof_eff }`: a distinct result, never a `±` that could be read as a standard uncertainty.

### 3. PHS

- **Literal**: `uncertainty = { uncertainty_op ~ (uncertainty_pair | uncertainty_val) ~ dof_clause? }`, `dof_clause = { ("dof" | "ν") ~ number }`. `inf` is the number grammar's infinity. Before or after the unit, like the uncertainty itself. With an asymmetric pair it is a parse error; so is a non-positive number.
- **Builtins**: `dof(x)` (ν_eff), `coverage_factor(x, p)`, `expanded(x, p)` (U as a quantity with the unit of `x`), `from_samples(vector)`.
- The static checker validates the clause and treats the builtins' results as dimensionless (`dof`, `coverage_factor`) and as the unit of `x` (`expanded`). The language server and `phs doc` learn the syntax and the builtins.

### 4. Python, WASM, Java

- **Python**: `Q_(v, unit, uncertainty=u, dof=9)`, `Q_.from_samples(values, unit)`, `q.dof`, `q.coverage_factor(p=0.95)`, `q.expanded(p=0.95)` (a frozen result object with a readable `str`, e.g. `10.0030 ± 0.0059 V (k=2.12, p=95 %)`), all in the Rust core: no runtime dependency. Array magnitudes and Python-lineage mode raise. `Lineage.__reduce__` carries the ν of its sources, because ids are per process.
- **WASM**: `withDof`, `fromSamples`, `dof`, `coverageFactor(p)`, `expandedUncertainty(p)`. **Java**: `Quantity.withDof`, `fromSamples`, `effectiveDof`, `coverageFactor`, `expanded`.
- `tests/golden/uncertainty_dof.tsv` freezes the answers; the core, Java and WASM check themselves against it, as for `unit_dimension_scale.tsv`.

## Validation

1. **JCGM 100:2008 Annex H.1** (end-gauge calibration), built through the public API from the Guide's budget, as reproduced in the GTC documentation (<https://gtc.readthedocs.io/en/stable/API/GUM_H1.html>; the ISO copy of the Guide answers 403 from the environment the spec was written in). Contributions `|c_i| u(x_i)` in nm and ν: 25 (18), 5.8 (24), 3.9 (5), 6.7 (8), 16.599 (2), 2.88679 (50). Evaluated independently with `scipy` for this spec:

   | Result | Computed | Published in the Guide |
   |---|---|---|
   | `u_c` | 31.664 nm | 32 nm |
   | `ν_eff` | 16.752 | 16.7 (cut at one decimal) |
   | `k` for 95 % (ν = 16) | 2.1199 | 2.12 |
   | `U` for 95 % | 67.12 nm | 0.07 µm |
   | `k` for 99 % (ν = 16) | 2.9208 | 2.92 |
   | `U` for 99 % | 92.48 nm | 0.09 µm |

   The test asserts the computed column to 1e-9 relative and the published column to its printed digits. `scientific_gaps_and_rigor.md` asks for 1e-4 against the Guide; the Guide prints three digits, so 1e-4 is only reachable against the computed value.
2. **The quantile**: a grid of ν (1 to 10⁴ and ∞) and p (0.5 to 0.999) against `scipy.stats.t.ppf`, to 1e-9 relative, in `tests/golden/student_t_two_sided.tsv` written by `scripts/gen_student_t_golden.py` (scipy is a development tool there, not a dependency), and the Guide's Table G.2 values.
3. **Properties**: the ν of independent sources combine as the formula says; a shared source is counted once (`x - x` has an exact result); a sum of equal independent sources of ν each has ν_eff = 2ν; infinite ν anywhere it does not matter leaves ν_eff untouched.

## Staging

| Stage | Content | Done when |
|---|---|---|
| D1 | The core: the side table, `effective_dof`, `student_t_two_sided`, the `Quantity` methods, `Expanded`, the H.1 test and the quantile grid | `cargo test -p physure` passes with the H.1 and grid tests |
| D2 | PHS: the literal, the builtins, the checker, the LSP, `phs doc`, the language spec | `cargo test -p physure-script -p physure-cli -p physure-lsp`; the H.1 example written in PHS |
| D3 | Python | `pytest` with the H.1 example in Python; doctests |
| D4 | WASM and Java, the golden file | the Java matrix and the WASM tests |

Each stage ends with a commit, a CHANGELOG entry and green tests. Nothing is pushed or merged without the maintainer's word.

## Not in this piece

- ν_eff for arrays or the covariance stores (numpy / torch / jax); Python-lineage mode.
- Type B distributions and a ν derived from a reliability; declaring correlations.
- Asymmetric coverage intervals and the Monte Carlo coverage of JCGM 101.
- The `physure.ext.metrology.MetrologyQuantity` sketched in `scientific_roadmap.md` §2.2: `with_dof` and `from_samples` replace it.

## Risks

- The side table never frees an entry. Only sources that declare a finite ν are stored, so the growth is bounded by what a program declares.
- Java and WASM need native builds and the CI matrix, so D4 is the slowest stage to verify.
- A hidden choice in `coverage_factor`: ν_eff below 1 uses ν = 1. The Guide does not say; the choice is documented and tested.
