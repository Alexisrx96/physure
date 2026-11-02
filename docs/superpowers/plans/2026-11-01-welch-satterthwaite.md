# Welch–Satterthwaite — plan

**Spec:** `docs/superpowers/specs/2026-11-01-welch-satterthwaite-design.md`
**Branch:** `feat/welch-satterthwaite` (worktree `.claude/worktrees/welch-satterthwaite`), off `main` @ `7786740`.
**Commit rule:** every commit ends with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; nothing is pushed or merged without the maintainer's word. Chat, docs, commits and comments are English.
**Method:** red test first, the smallest change that turns it green, then the next. Stages D2 to D4 get their detailed tasks when they start, from the names D1 settles.

## Stage D1 — the core

Files: create `physure-core/src/math/stats.rs`, `physure-core/src/uncertainty/dof.rs`, `physure-core/src/uncertainty/expanded.rs`, `tests/golden/student_t_two_sided.tsv`, `scripts/gen_student_t_golden.py`, `physure-core/tests/effective_dof.rs`, `physure-core/tests/student_t_golden.rs`; modify `math/mod.rs`, `uncertainty/mod.rs`, `lib.rs`.

### Task 1: the quantile

- [x] `scripts/gen_student_t_golden.py`: for ν in 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 30, 40, 60, 100, 300, 1000, 10000, ∞ and p in 0.5, 0.683, 0.9, 0.95, 0.954, 0.99, 0.997, 0.999, write `nu<TAB>p<TAB>k` with 17 significant digits from `scipy.stats.t.ppf((1 + p) / 2, nu)` (`norm.ppf` for ∞) to `tests/golden/student_t_two_sided.tsv`. Run it.
- [x] `physure-core/tests/student_t_golden.rs`: read the TSV, call `physure_core::math::stats::student_t_two_sided(p, nu)`, assert relative error < 1e-9 for each row; assert the Guide's table values (ν = 16: 2.12 and 2.92 to 3 digits). Run: it fails to compile.
- [x] `math/stats.rs`: `ln_gamma` (Lanczos), `reg_inc_beta`, `reg_lower_gamma`, `normal_two_sided`, `student_t_two_sided(p, nu) -> PhysureResult<f64>` (`p` outside (0, 1) or `nu` ≤ 0 or NaN is a `DomainError`). Run the test: it passes.

### Task 2: the table and ν_eff

- [x] `physure-core/tests/effective_dof.rs`, first test `a_source_without_a_declared_dof_has_an_infinite_one`; then `the_dof_of_independent_sources_combine_by_welch_satterthwaite` (two sources, hand-computed); `a_shared_source_is_counted_once`; `a_sum_of_equal_independent_sources_has_twice_the_dof`; `infinite_dof_leaves_the_others_alone`. They fail to compile.
- [x] `uncertainty/dof.rs`: the table, `declare_dof(id, nu)`, `dof_of(id)`; `impl Lineage { pub fn effective_dof(&self) -> f64 }`. Tests pass.

### Task 3: the `Quantity` API

- [x] Tests: `with_dof_declares_the_dof_of_the_one_source`, `with_dof_twice_with_the_same_value_is_a_no_op`, `with_dof_with_another_value_is_refused`, `with_dof_refuses_a_derived_an_exact_and_a_non_gaussian_quantity`, `with_dof_refuses_zero_negative_and_nan`, `from_samples_is_type_a` (mean, `s/√n`, ν = n − 1; n < 2 refused; equal samples exact), `coverage_factor_truncates_the_dof` (ν_eff 16.75 uses 16; ν_eff 0.4 uses 1; ν_eff ∞ is 1.95996), `expanded_reports_every_part`.
- [x] `uncertainty/expanded.rs`: `Expanded`; `impl Quantity { with_dof, from_samples, effective_dof, coverage_factor, expanded }`. Export `Expanded` from `uncertainty` and the crate root. Tests pass.

### Task 4: the Guide

- [x] `a_h1_end_gauge_calibration_reproduces_the_guide` in `effective_dof.rs`: six independent quantities with the budget of the spec, summed through the public API; assert the computed column to 1e-9 and the published column to its digits (`u_c` rounds to 32 nm, ν_eff cut at one decimal is 16.7, `k` is 2.12 and 2.92, `U` is 0.07 µm and 0.09 µm).

### Task 5: close the stage

- [x] `cargo test -p physure`; `cargo clippy -p physure` shows nothing new in the files touched.
- [x] `CHANGELOG.md` `[Unreleased]` → Added: the core API (what, why, the H.1 result). `docs/scientific_gaps_and_rigor.md` §1.1: the TODO becomes a pointer to the test, status line updated; `docs/ROADMAP.md` tick left for D4 (the feature is complete when all five bindings have it).
- [x] Commit the spec and plan, then D1.

## Stage D2 — PHS (done)

- [x] Core: `Quantity::from_samples_in(samples, unit)`, so that a program with a unit in hand needs no unit string.
- [x] `QuantityNode.dof` (`#[serde(default)]`) and `QuantityNode::dof_refusal(target)`; the 24 literal constructions get `dof: None`.
- [x] Grammar `dof_op`, `dof_value` (with a sign, so that `dof -2` is refused as what it is), `dof_clause`, last in `uncertainty`. Parser: `check_dof` refuses an asymmetric pair, a `sigma` bound and a non-positive number. 5 parser tests.
- [x] Interpreter: `eval_quantity_literal` applies `with_dof`. Builtins `dof`, `coverage_factor`, `expanded`, `from_samples` in `builtins/dof.rs`. 11 tests in `physure-script/tests/degrees_of_freedom.rs`, among them the Guide's H.1 program and the tutorial file.
- [x] Checker: `dof` and `coverage_factor` are dimensionless, `expanded` keeps the unit (1 test in `tests/check.rs`).
- [x] Generators: Python and Rust carry the clause, Java and JavaScript/TypeScript refuse it (`tests/codegen_dof.rs`). The new builtins are written as ordinary calls by every target, like the existing ones; Python gets them in D3 (`physure.ext.helpers`).
- [x] Language server: completion and hover for the four builtins and for `dof`. Docs: the language specification, the primer (section 3) and `docs/tutorials/dof_example.phs`.

## Stage D3 — Python (done)

- [x] Core, so that the rules live once: `Lineage::declare_dof` (the single-source rule) and `declared_dofs`, `coverage_factor_for(p, nu_eff)` (the truncation), `type_a(samples)`; `Quantity::with_dof`, `coverage_factor` and `from_samples_in` call them.
- [x] PyO3: `Lineage.effective_dof`, `declare_dof`, `declared_dofs`, `from_terms(terms, dofs)` and `__reduce__` carrying the dofs; module functions `coverage_factor`, `type_a_evaluation`, `gum_round`.
- [x] Python: `physure/domain/measurement/degrees_of_freedom.py` (`DegreesOfFreedomMixin`: `with_dof`, `dof`, `coverage_factor`, `expanded`; frozen `ExpandedUncertainty` with a GUM-rounded text), `Quantity.from_input(..., dof=)` and the `dof=` keyword of the factories, `Q_.from_samples`.
- [x] Arrays, `python_lineage()` and the sampling modes raise `NotImplementedError`.
- [x] Found by the tests: `Quantity.to` minted a new source (see the CHANGELOG); fixed in `_convert_by_plan`.
- [x] Helpers for exported scripts (`physure.ext.helpers`), imported by the Python target; the Guide's program transpiled and run.
- [x] 14 tests in `tests/test_degrees_of_freedom.py` and the doctests; the whole suite passes.

## Stage D4

Planned in the spec ("Staging"). Its tasks are written here when the stage starts.
