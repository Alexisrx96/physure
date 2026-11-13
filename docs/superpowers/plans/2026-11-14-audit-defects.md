# Engine defects found by the Phase 4 audit — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the nine places where the engine gives a **wrong answer without an error** because a magic number is applied in the wrong unit, or a numerical method guesses instead of estimating its error. Rule of the project: *if an operation cannot preserve a dimension, a factor or an uncertainty, it raises; a wrong answer with confident units is worse than an exception.* This track runs **before** Phase 4b (data migration) so that nothing is frozen, witnessed or migrated on top of a wrong behaviour.

**Source:** `docs/superpowers/audits/2026-11-14-code-resident-numbers.md` §4 (class B items B8, B10, B14, B16, B17, B18, B19, B20, B22), each with file:line and a probe. Every probe in the audit must be **re-run before the change and after it**: the "before" run goes into the commit message of the fix, so the defect is documented as measured.

**Owner decisions already taken (2026-11-14):** defects are fixed first, in their own track; Monte Carlo gets a fixed default seed (that is Phase 4b, not here); printed text counts as a result (Phase 4b).

**Process rules of this project:** tests first (see the failing test, fix, see it pass); English everywhere; `CHANGELOG.md` (CRLF) gets a **Fixed** entry per behaviour change with the before/after of the probe, and every change of a result is called out as such; roadmaps untouched unless a phase changes; never push; encoding discipline for scripted edits (UTF-8, CRLF-aware, backslashes via `chr(92)`); `physure-core` gets no new dependency; PHS has no fallback: PHS semantics live only in `physure-script`.

---

## Principles that decide every item

1. **A tolerance is relative to the quantities compared, never absolute in a unit nobody chose.** The audit's common cause: `abs(a_SI − b_SI) < 1e-9` or `1e-3` on base-SI magnitudes makes `1 pm == 2 pm` true and `1 nm == 2 nm` false. Any comparison first brings both operands to one unit **exactly** (the registry's conversion, as `to` does), then compares in that unit.
2. **Equality is exact up to floating-point rounding, not a fuzzy zone.** `==` on quantities means "the same value up to the rounding of the conversion and of the arithmetic that produced them": a relative bound of `EQ_ULPS = 8` ulps of the larger magnitude (so `0.1 + 0.2 == 0.3` is true and `1 pm == 2 pm` is false), and exactly equal when both are zero. No absolute epsilon anywhere in `==`/`!=`.
3. **Approximate comparison is explicit and relative.** `≈` means `|a − b| <= rel_tol · max(|a|, |b|)` with `rel_tol` read from the catalog `[Settings] approx_rel_tol` (default `1e-3`, the order of magnitude the old absolute rule had on values near 1); both operands zero is true; operands of different dimension are an error as today. This is the only new catalog key in this track.
4. **A numerical method that cannot bound its error refuses.** `integral` and `limit` return a value only with a converged error estimate below their tolerance; otherwise they raise an error that names the method, the estimate reached and what to do (give the symbolic form, split the interval). They never return `0.0` or `inf` as a guess.
5. **Units and uncertainty are never dropped.** A numeric integral of a quantity-valued integrand keeps the integrand's unit times the variable's unit and propagates uncertainty (or raises if it cannot); it must not return a bare mean.
6. **Unit-independence is a test, not a hope.** Every fixed comparison gets a property test: scaling both operands by a unit change (`m` ↔ `mm` ↔ `km`, exact factors) never changes the result.
7. **Behaviour changes are loud.** Each fix says in the changelog which previous answers change; PHS programs that relied on the old fuzziness are found by running the repository's `.phs` corpus (tutorials, examples, tests) before and after, and any difference is explained in the commit.

---

## Tasks

### Task 1: comparisons in PHS and in the Rust `Quantity` (B16, B17, B18, B10, B22)

**Files:** `physure-script/src/builtins/core.rs` (`==` at ~386, `!=` at ~257, `≈` at ~183, the sigma-bound comparison at ~382), `physure-core/src/quantity.rs` (`PartialEq` at ~30; `assert` tolerances at ~548 are B11 and stay), `physure-script/src/interpreter/helpers.rs` (~70, truthiness), `physure-core/src/units/conf.rs` + `physure-core/src/settings.rs` (the `approx_rel_tol` setting: collected, validated, applied through `ProcessSettings`/`EffectiveSettings` like the other settings; frozen files carry it), `physure-core/src/frozen/entries.rs` (the setting appears in the entries), `physure-core/src/units/physure.conf` (`[Settings] approx_rel_tol = 1e-3`, with a comment).

Required behaviour:
- **`==` / `!=` (B16):** operands brought to the left operand's unit exactly; equal iff within `EQ_ULPS` of the larger magnitude or both exactly zero; dimension mismatch stays an error. `1 pm == 2 pm` → `false`; `1 nm == 2 nm` → `false`; `1 km == 1000 m` → `true`; `0.1 m + 0.2 m == 0.3 m` → `true`.
- **`≈` (B17):** principle 3. `1 mm ≈ 1.9 mm` → `false` (relative difference 0.9 > 1e-3); `1 km ≈ 1.0009 km` → `true`; unit-independent by construction.
- **Sigma-bound comparison `x == t ± kσ` (B18):** both sides converted to one unit before anything is compared; the combined standard uncertainty is `sqrt(u_left² + u_right²)` in that unit; when both are zero the comparison is the exact one of `==` (principle 2). The magic `0.05` default is removed. `10 mm == 10.04 mm ± 1σ` is then decided by `|10 − 10.04| <= 1 · u` in mm with `u` the right operand's σ — if the right operand has no σ the expression is a **type error** with a message saying a `±` operand is required (decide against the existing grammar and tests, and document).
- **`Quantity: PartialEq` (B10):** the same rule as `==` (principle 1 and 2) on the exact conversion; uncertainty is compared exactly as before (bitwise or as the current code does — read it and keep the semantics, only replace the absolute epsilon on the magnitude).
- **Truthiness (B22):** a quantity is false iff its magnitude is exactly zero (any unit); remove `abs(x) > 1e-15`. `if 1e-16 m` is true, `if 0 m` is false, `if 0 km` is false, `if -0.0 m` is false.

- [ ] **Step 1: tests first** — `physure-script/tests/comparison_semantics.rs`: every line of the table above as a PHS snippet through the same entry the CLI `phs eval` uses (compare the JSON `bool` result); the unit-independence property test (a small deterministic grid of magnitudes × unit scales `m`/`mm`/`km`/`µm`, both orders); `0.1 + 0.2 == 0.3`; non-finite operands (`NaN == NaN` false, `inf == inf` true by the same rule as IEEE: decide and document); the sigma-bound cases including the error case; `approx_rel_tol` honoured from a frozen file (activate one with `[Settings] approx_rel_tol = 1e-6` as `frozen_equivalence.rs` does, restore state) and invalid values (`0`, negative, `nan`, `abc`) refused strictly by the frozen replay and ignored-with-a-warning by the lenient reader, like the other numeric settings; core: `Quantity` equality tests in `physure-core/tests/` for `1 pm` vs `2 pm`; entries: the new setting is listed and `diff` notices a change.
- [ ] **Step 2:** run them; they fail where the audit says. **Step 3:** implement. **Step 4:** run the whole `physure-script`, `physure` and `physure-cli` suites and the `.phs` corpus before/after (`for f in tutorials/examples/tests *.phs`: record `phs eval` JSON before and after; list every difference in the commit message). **Step 5:** commit `fix(script,core): comparisons are relative and unit-independent (audit B10 B16 B17 B18 B22)`.

### Task 2: numeric integral and limit refuse instead of guessing (B20, B19)

**Files:** `physure-script/src/builtins/calc.rs` (`integral` numeric fallback at ~568-640, `limit` at ~387-405).

Required behaviour:
- **`integral` (B20):** when symbolic integration fails, integrate numerically with an **error-estimating** rule — tanh-sinh (double-exponential) quadrature on finite intervals (it handles integrable endpoint singularities such as `1/sqrt(x)` on `[0, 1]` and returns `2` to ~1e-14), with the estimate from the difference of successive levels, tolerance `1e-12` relative (`1e-15` absolute floor only to avoid division by zero when the integral is exactly zero), a hard cap on evaluations. Not converged → error naming the method, the estimate and the integral. Improper (infinite) intervals via a documented variable transformation or refused with a clear message (choose the simplest correct option and test it). An evaluation error of the integrand inside the interval is an **error**, never `0.0`. The integrand's unit × the variable's unit is the result's unit; uncertainty: if the integrand carries uncertainty, propagate through the quadrature weights or raise a clear error saying it is unsupported — never drop it silently.
- **`limit` (B19):** try symbolic first (as today); numeric fallback uses a **sequence of approach distances** (e.g. `h = 10^-k`, `k = 3..12`, both sides when the limit is two-sided as the current grammar defines) and accepts only when the sequence converges (successive differences decreasing to below `1e-9` relative) and agrees from both sides; a divergent sequence returns the signed infinity **only** when it diverges monotonically past a documented bound across at least three successive scales; anything else is an error ("the limit could not be determined numerically; give a symbolic form"). `limit("0.000001*x","x",1)` must be `1e-6`; `limit("2000000+x","x",1)` must be `2000001`; `limit("1/(x - 1e-9)","x",0)` must be `-1e9`; a true divergence such as `limit("1/x","x",0)` (two-sided) is an error or `inf` according to the current grammar — keep the documented behaviour of genuinely divergent cases and test it.

- [ ] **Step 1: tests first** — each audit probe as an expected value with a tolerance written in the test from the closed form: `∫₀¹ x^(-1/2) dx = 2`, `∫₀¹ exp(-x²) dx = 0.746824132812427…` (erf), `∫₀^π sin x dx = 2`, `∫₀¹ ln x dx = -1` (log singularity), an oscillatory case that does not converge within the cap (must raise), an integrand that errors inside the interval (must raise), a unit case (`∫ v dt` with `v` in `m/s`, `t` in `s`, gives `m`; and with `t` in `h` the unit is `m*h/s` or the exact conversion the existing unit algebra gives), the uncertainty rule (raise or propagate: whichever you implement, test it), and the three `limit` probes above plus `limit("sin(x)/x","x",0) = 1`, `limit("(1-cos(x))/x^2","x",0) = 0.5`.
- [ ] **Steps 2–5** as Task 1 (corpus before/after). Commit `fix(script): integral and limit refuse instead of guessing (audit B19 B20)`.

### Task 3: covariance pruning and the Monte Carlo deviation (B14, B8)

**Files:** `physure-core/src/covariance/store.rs` (~44, 55, 189), `physure-core/src/covariance/pruning.rs` (read it; B15 stays), `physure-core/src/uncertainty/monte_carlo.rs` (~87), `physure-python/src/lib.rs` only if it mirrors the threshold (audit B14/B15), Python tests under `physure-python/tests/` for the array case.

Required behaviour:
- **B14:** entries are never dropped by an **absolute** threshold. Drop only what is exactly zero, or what is negligible **relative to the standard deviations involved**: `|cov(i,j)| <= EPS_CORR · sqrt(var_i · var_j)` with `EPS_CORR = 1e-12` (a correlation coefficient bound: invariant under scaling a variable by any unit factor) and **never** a variance (diagonal) entry unless it is exactly zero. Test: `x + x` with `u = 1e-7 m`, `1e-5 m`, `1e-9 m`, and the same in `mm`, `km` → the result's uncertainty is exactly `2·u` in every case (before the fix `1e-7 m` gave `0`); the scaling-invariance property test (scale a variable by `10^k`, k in −9..9: the set of stored entries is identical); numpy arrays through the Python binding if the binding can be built in this environment (if `maturin`/`uv` cannot build, say so and cover it with a Rust-level test of the same store call sequence).
- **B8:** Monte Carlo standard uncertainty uses the **sample** standard deviation `1/(M−1)` (JCGM 101:2008 §7.6), `M = samples`; define the `M == 1` case (uncertainty `0`? error?) and document it. The mean is unchanged. Update every test that pinned the old value; the battery case `unc.montecarlo_seed_1` and the references in `physure-core/tests/battery_reference.rs` change deliberately: update them with the new value **derived independently** (compute it in the test from the sample set the generator produces, not by copying the engine's output), keep the 5-standard-error agreement with the exact product moments, and note in the changelog that the witness value of `unc.montecarlo_seed_1` changes with this release (a `verify --run` of a file frozen before this fix reports that witness as differing, which is the intended signal).

- [ ] **Steps 1–5** as above. Commit `fix(core): covariance entries are pruned relative to the deviations, and Monte Carlo uses the sample deviation (audit B8 B14)`.

### Task 4: changelog, audit status, verification

- [ ] **Step 1:** `CHANGELOG.md` (CRLF): under `[Unreleased]` a **Fixed** group (create the heading once if missing: check first) with one bullet per item: the audit id, the probe, the answer before, the answer after, and who is affected; plus an **Added** bullet for the `approx_rel_tol` setting.
- [ ] **Step 2:** in `docs/superpowers/audits/2026-11-14-code-resident-numbers.md` mark B8, B10, B14, B16-B20, B22 as **fixed in this track** with the commit hashes (edit the table; do not rewrite history), and update the 'Left in the engine' section.
- [ ] **Step 3:** full verification: `cargo test -p physure -p physure-cli -p physure-script -p physure-lsp`, `cargo check --workspace` (WASM crate included), `cargo clippy -p physure -p physure-script --all-targets` (nothing new in touched files), `cd physure-python && uv run mkdocs build --strict` if docs changed, and the Python suite for the touched binding code if buildable. Commit `docs: audit defects fixed - changelog and audit status`.

---

## Self-review against the audit

| Audit item | Task | Behaviour after |
|---|---|---|
| B16 `==`/`!=` absolute 1e-9 in SI | 1 | relative 8 ulps after exact conversion; `1 pm == 2 pm` false |
| B17 `≈` absolute 1e-3 | 1 | relative `approx_rel_tol` (catalog) |
| B18 sigma-bound `0.05` mixing units | 1 | one unit, quadrature of σ, no magic default |
| B10 `Quantity: PartialEq` 1e-9 | 1 | same rule as `==` |
| B22 truthiness 1e-15 raw mean | 1 | exact zero test |
| B20 numeric integral, no error estimate, drops unit/uncertainty | 2 | tanh-sinh with estimate; refuses; keeps unit |
| B19 limit heuristics | 2 | convergence-checked sequence; refuses |
| B14 covariance store drops <= 1e-12 absolute | 3 | relative to deviations; never drops a variance |
| B8 Monte Carlo population deviation | 3 | sample deviation (JCGM 101 §7.6) |

Left for Phase 4b (data migration): `monte_carlo_samples`, `monte_carlo_seed` (fixed default 0, `random` opt-in), `max_while_iterations`, currency as `currency.conf`, `[Thermochemistry:<edition>]`, display-rule keys and a printed-text witness, default degrees of freedom; Python's atomic-weight table is deleted in chemistry plan 3.
