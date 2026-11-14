# Audit: numbers that live in code (configuration freeze, Phase 4)

**Date:** 2026-11-14 (research done on 2026-11-13). **Base:** `main` at `fd41d1c`. **Branch:** `feat/conf-freeze-4`.
**Scope:** every crate and language that can produce a number a user sees: `physure-core`, `physure-script`, `physure-cli`,
`physure-lsp`, `physure-wasm`, `physure-java`, `physure-python` (Rust and the `physure/` package).
**Spec:** `docs/superpowers/specs/2026-11-07-conf-freeze-design.md` §2, §3, §8, §11 (Phase 4), §13.
**Status (2026-11-14, after the audit):** every flagged defect (B8, B10, B14, B16-B20, B22) and B11 are **fixed on branch fix/audit-defects**
(plan `docs/superpowers/plans/2026-11-14-audit-defects.md`); the commits are named in §4. That track found further
defects, listed in §11. The rest of this file is the audit as written; only the status cells and notes are new.

**Method.** I ran a literal extractor over every non-test source file (Rust `#[cfg(test)] mod` and `tests.rs` excluded,
Python `tests/` and doctests excluded) and then searched for `unwrap_or(`, `Default`, `const`, `env::var`, `rand`, `seed`,
`rayon`, `1e-` and tolerance keywords. I read each hit before classifying it. Probes ran from the scratchpad, never from
the worktree:

* `phs eval` from this worktree, built into a scratch target directory (release build of `fd41d1c`);
* Python from the main checkout's virtualenv. Its source is the same commit, and its `_core.pyd` is an earlier local build.

Every `file:line` below was read at `fd41d1c`.

**Classes**

* **A, data.** Something a user or an auditor could legitimately want to pin or override. It is a candidate to move into the catalog.
* **B, algorithm internal.** It stays in the engine, where the engine pin and the witnesses protect it.
* **C, binding-only data.** It must be migrated or removed when that binding delegates to the core.
* **D, not a result.** UI, styling, buffers. Only the categories are listed.

**Witness column.** "yes" means a case of the built-in battery (`physure-core/src/frozen/battery/`, 47 cases) would
change its recorded bits if the number changed. "no" means nothing records it today.

---

## 1. Summary

### 1.1 Counts

| Class | Total | core | script | python (Rust + `physure/`) | java | wasm | cli | lsp |
|---|---|---|---|---|---|---|---|---|
| **A** (move to the catalog) | **10** | 6 | 1 | 3 | 0 | 0 | 0 | 0 |
| **B** (stays in the engine) | **25** | 16 | 9 | 0 | 0 | 0 | 0 | 0 |
| **C** (binding-only) | **13** | 1 | 0 | 11 | 1 | 0 | 0 | 0 |
| **D** (not a result, categories) | **9** | 2 | 2 | 2 | 1 | 0 | 2 | 0 |

`physure-wasm` and `physure-lsp` hold no number that affects a result: both delegate to the core and to PHS. The CLI
holds only display labels and limits, but it is the surface where the seed default is decided (§6.3).

Of the 25 B items, 9 are flagged as **defects** (B8, B10, B14, B16, B17, B18, B19, B20, B22). Each one is unit-dependent
or gives a wrong answer without an error, so a frozen file would freeze a wrong answer. They are not catalog material.
They are listed so that whoever writes the Phase 4 plan can decide whether to fix them before freezing users' validations
onto them.

**Status:** all 9 are **fixed on branch fix/audit-defects**, and B11 with them (the same absolute tolerance as B10, not flagged here):
**0 of the 25 B items is an open defect.** The class totals above do not change: a fixed item still lives in the
engine. B17's tolerance became the `[Settings]` key `approx_rel_tol` (so it is also covered by the catalog, §9). The
new findings of §11 (B26 and T1-T12) are not code-resident numbers and are not counted in this table.

### 1.2 Answer to the spec's list (§2)

| Spec item | Finding |
|---|---|
| Exchange rates `EUR = 1.08`, `MXN = 0.059` | Confirmed (A2). They are Python-only. No network fetch exists; the rates are literals. |
| `samples.unwrap_or(1000)` | Confirmed (A1). The number is written in **three** places: `physure-core/src/quantity.rs:244`, `physure-python/src/lib.rs:999` and `physure-python/physure/domain/measurement/quantity.py:576`. |
| `imperial.conf` / `international.conf` read by Python only | Confirmed, but **they hold no number that affects a result**. They carry only `[System] name` and `base_units`, and their own comments say no code reads `base_units`. For Phase 4 the item is empty. They matter in Phase 5 as *sources*. |
| Python `STANDARD_ENTHALPY_FORMATION`, `STANDARD_ENTROPY` | Confirmed (A3). There are 8 species each, with no uncertainty, no phase and no stated standard state. |
| **Not in the spec** | The most consequential finding is **C1**: Python's own atomic-weight table, `ATOMIC_WEIGHTS` with 118 elements. It **disagrees with the catalog** for 25 elements, and it treats the half-widths as standard uncertainties. The Python and core molar masses of water differ (§7, S1). |

---

## 2. Top 15 findings by user impact

| # | ID | What | Why it ranks here |
|---|---|---|---|
| 1 | A10 / §6.3 | Monte Carlo draws are **unseeded** on every surface except `phs eval`, MCP and the witnesses | Under `propagation_mode = monte_carlo`, `phs file.phs` printed the input `x = 10 +/- 1 m` as `10.01 ± 0.98 m`, then `10.1 ± 1.0 m`, then `10.00 ± 0.98 m` on three runs. A frozen file cannot make that reproducible. |
| 2 | C1 | Python `ATOMIC_WEIGHTS` differs from `[AtomicWeights:standard-2024]` for 25 elements, and uses half-widths as standard uncertainties | Python `Species("H2O").molar_mass` gives `18.015 ± 0.00108 g/mol`. The core gives `18.01535 ± 0.000264`. Freezing the catalog does nothing for Python chemistry. |
| 3 | B14 | The covariance store drops entries with `abs(value) <= 1e-12` (absolute, in base-unit²) | Python numpy array `[1, 2] m ± 1e-7 m`, then `x + x` gives **uncertainty `[0, 0]`**. In `mm` the same physical case keeps its uncertainty. The uncertainty is dropped silently and the result depends on the unit. |
| 4 | A1 | Default Monte Carlo sample count 1000, written 3 times | It changes every Monte Carlo result. It is witnessed (`unc.montecarlo_seed_1`), but no key exists. |
| 5 | B16 | PHS `==` is `abs(l - r) < 1e-9` on base-SI magnitudes | `1 pm == 2 pm` is **true**. `1 nm == 2 nm` is false. |
| 6 | B19 | PHS `limit()` probes at `+1e-8` (or `±1e7`), returns 0 below `1e-5` and ±inf above `1e6` | `limit("0.000001*x","x",1)` gives `0.0`. `limit("2000000 + x","x",1)` gives `inf`. |
| 7 | B20 | PHS numeric `integral` uses 100 panels of 3-point Gauss–Legendre with no error estimate | `integral("1/sqrt(x)","x",0,1)` gives `1.97508560641784` (exact: 2). There is no warning. |
| 8 | A2 | Exchange rates are Python literals | `100 EUR => USD` gives `108.0`. The rates are undated, and the release changes them. |
| 9 | B17 / B18 | PHS `≈` is an absolute `1e-3` in base SI. `x == t ± kσ` uses `u = 0.05` (base SI) when `x` is exact, and compares in mixed scales | `1 mm ≈ 1.9 mm` is true. `1 km ≈ 1.0009 km` is false. `10.0 mm == 10.04 mm +/- 1 sigma` is true. |
| 10 | A6 | Display rules: 15 significant digits, scientific notation below `1e-4` or from `1e16`, the GUM 1-or-2-figure uncertainty rule | They govern every printed or reported number (CLI, HTML/LaTeX reports, Java `toString`, WASM). The witnesses store bits, so a change in a printed report is never detected. |
| 11 | A3 | Python thermochemical tables | They have no uncertainty and no phase or standard state. The core has no equivalent. |
| 12 | B8 | Monte Carlo standard deviation uses `ddof = 0` (`samples.std(0.0)`) | JCGM 101:2008 (7.6), which the backend claims to implement, divides by `M − 1`. The result is low by a factor of `sqrt(999/1000)`. It is witnessed, but it is a standards disagreement. |
| 13 | A9 | The PHS `while` cap of 10 000 is hard-coded | Its sibling `max_loop_iterations` (1 000 000) is a `[Settings]` key. A frozen file cannot change the cap, and a script that needs more errors (verified). |
| 14 | C4 / C11 | Python and core displays differ | Python `str(Q_(10,'V',uncertainty=1))` is `(10.0 ± 1) V`, and the Monte Carlo output is `(10.0524997… ± 0.98482587…) m`. The core prints `10.0 ± 1.0 m`. |
| 15 | A5 | Python alone defaults the coverage probability to `p = 0.95` | The core, PHS, WASM and Java require `p` explicitly. |

---

## 3. Class A: data, candidates for the catalog (sorted by impact)

| ID | file:line | Literal and meaning | Affects (example, result observed) | Catalog key today | Witness |
|---|---|---|---|---|---|
| A10 | `physure-core/src/uncertainty/monte_carlo.rs:28-35`; seed defaults `physure-script/src/report.rs:14` (`DEFAULT_SEED = 0`), `physure-cli/src/mcp.rs:255` | **The absence of a seed.** Outside a `with_seed` scope, `StdRng::from_rng(thread_rng())` draws OS entropy. | Every Monte Carlo result from `phs file.phs`, the REPL, the TUI, the web view, `phs serve`, the LSP, WASM, Java and Python. Observed: three runs gave three different displayed inputs (§2, #1). | none | `unc.montecarlo_seed_1` uses its own seed; unseeded behaviour is unwitnessable |
| A1 | `physure-core/src/quantity.rs:244` (`samples.unwrap_or(1000)`); duplicated at `physure-python/src/lib.rs:999`, `physure-python/physure/domain/measurement/quantity.py:576` | `1000`: draws per new Monte Carlo quantity | Every Monte Carlo mean and u. `phs eval mc.phs --seed 0` with `y = (10 ± 1 m)²` gives `100.77165846524241 ± 20.33282727052625`. | none | **yes**: `unc.montecarlo_seed_1` |
| A2 | `physure-python/physure/ext/currency.py:16-41` (USD 1.0 at line 18, EUR `1.08` at 28, MXN `0.059` at 37); `$` dimension at line 10 | Exchange rates relative to USD | `enable_currency(); Q_(100,'EUR').to('USD')` gives `108.0 USD`. `Q_(100,'MXN').to('USD')` gives `5.8999999999999995 USD`. | none (`physure.conf:144` has an empty `# --- Currency ---` header). `international.conf` names `$:USD` in `base_units` (unread). | no |
| A6 | `physure-core/src/quantity.rs:75` and `:145` (scientific below `1e-4`, from `1e16`), `:85` (15 significant digits), `:150-153` (GUM: 1 figure, 2 when the leading digit is 1), `:49` (14-digit cut before fractions), `:41` (`MAX_DENOM = 10_000`, `frac` format only) | Display precision rules | Every printed or reported number. `1/3` prints `0.333333333333333`. `0.00009999 m` prints `9.999e-5 m`, and `0.0001 m` prints `0.0001 m`. `10.0 ± 0.15 m` prints `10.0 ± 0.2 m`. | `readable_representation`, `default_output` exist but **nobody reads them** (§5) | no (witnesses store bits, and `phs-witness-1` omits the text) |
| A3 | `physure-python/physure/ext/chemistry/thermo_kinetics.py:25-44` | ΔfH° (kJ/mol) and S° (J/(mol·K)) for 8 species at 298.15 K, e.g. `"H2O": -285.8` and `69.9` | `standard_enthalpy_formation("H2O")` gives `-285.8 kJ/mol`, and every Gibbs computation built from it | none | no |
| A5 | `physure-python/physure/domain/measurement/degrees_of_freedom.py:134`, `:154` | `p = 0.95`, the default coverage probability | `x.expanded()` and `x.coverage_factor()` in Python only. The core, PHS (`dof.rs:56,63`, arity 2), WASM and Java take `p` explicitly. | none | `unc.expanded_k_p95` passes 0.95 explicitly |
| A9 | `physure-script/src/interpreter/statements.rs:246` | `10_000`: the `while` iteration ceiling | `i = 0; while i < 20000 { i = i + 1 }` errors: "while loop did not converge after 10000 iterations". A larger cap returns a value. | none. Its sibling `max_loop_iterations` is in `[Settings]` | no |
| A4 | `physure-core/src/uncertainty/shapes.rs:299` (`ShapeKind::DEFAULT = Dimidiated`) | Which distribution an asymmetric `(σ⁻, σ⁺)` means (equal-area or Fechner) | Every result with an asymmetric uncertainty, e.g. PHS `x = 12.3 +/- (0.5, 0.4)`: its moments and the pair read back | none | no (no asymmetric case) |
| A7 | `physure-core/src/uncertainty/dof.rs:61` (`unwrap_or(f64::INFINITY)`) | An uncertainty with no declared degrees of freedom has ν = ∞ | `coverage_factor`, `expanded` and `ν_eff` of any budget with undeclared inputs (a normal k instead of Student-t) | none | partly: `unc.gum_h1_*` declare every ν |
| A8 | `physure-core/src/units/rational.rs:526-547` (16 hard-coded derived symbols: N, J, W, Pa, C, Hz, N/C, V, Ω, F, S, Wb, T, H, lx, kat) and `:567-588` (8 prefixes k, M, G, T, m, **µ**, n with tolerances) | Unit **names** in code, a parallel of the catalog's `[Units]`/`[Prefixes]` | Unit text via `RationalUnit::__repr__` when no display name exists (error texts, the `unit` of bindings that call `__repr__`). **It disagrees with the catalog**: code prints `µ`, and `physure.conf:87` defines `micro = u, 1e-6`. | `[Prefixes]` and `[Units]` hold the data; the code does not read it | no |

## 4. Class B: algorithm internals that stay in the engine (sorted by impact; ⚠ = defect)

| ID | file:line | Literal and meaning | Affects | Witness |
|---|---|---|---|---|
| B14 ⚠ **fixed on branch fix/audit-defects** (`6125152`; see also X6 and X7 in §11) | `physure-core/src/covariance/store.rs:44`, `:55`, `:189` | `1e-12`: covariance entries with an absolute value at or below it are not stored | Python numpy-array quantities (`vectorized_uncertainty.py:307-320`). Probe: `Q_(np.array([1.,2.]),'m',uncertainty=np.array([1e-7,1e-7]))`; `x + x` gives uncertainty `[0. 0.]`. With `u = 1e-5` the result is `[2e-05 2e-05]`. In `mm` with `u = 1e-5` it is kept. | no |
| B16 ⚠ **fixed on branch fix/audit-defects** (`53a0ac0`, `abc60bb`) | `physure-script/src/builtins/core.rs:386` (`==`), `:257` (`!=`) | absolute `1e-9` on base-SI magnitudes | `1 pm == 2 pm` is True, and `1 nm == 2 nm` is False (probe). | no |
| B19 ⚠ **fixed on branch fix/audit-defects** (`b69f22c`, `8c249df`) | `physure-script/src/builtins/calc.rs:387-389` (probe `±1e7`, `+1e-8`), `:400` (`< 1e-5` gives 0), `:403` (`> 1e6` gives ±inf) | Numeric limit heuristic | `limit("0.000001*x","x",1)` gives `0.0`. `limit("2000000 + x","x",1)` gives `inf`. `limit("1/(x - 1e-9)","x",0)` gives `inf` (true value −1e9). | no |
| B20 ⚠ **fixed on branch fix/audit-defects** (`13e4eab`, `cef665e`, `e9462f1`) | `physure-script/src/builtins/calc.rs:575`, `:624` (100 panels), `:629-640` (3-point Gauss–Legendre, `0.7745966692414834`), `:601-612` (improper ends at `1 − 1e-6`, cut at `1 − 1e-7`); `:568` (an evaluation error becomes `0.0`) | Numeric quadrature with no error estimate, used when symbolic integration fails | `integral("1/sqrt(x)","x",0,1)` gives `1.97508560641784` (true value 2). `exp(-x^2)` over `[0, 1]` gives `0.7468241328124272` (correct). The integrand's unit and uncertainty are dropped (`q.value.mean()`). | no |
| B17 ⚠ **fixed on branch fix/audit-defects** (`0e10eb9`, `abc60bb`) | `physure-script/src/builtins/core.rs:183` | `≈` is an absolute `1e-3` in base SI | `1 mm ≈ 1.9 mm` is True, and `1 km ≈ 1.0009 km` is False (probe). | no |
| B18 ⚠ **fixed on branch fix/audit-defects** (`abc60bb`) | `physure-script/src/builtins/core.rs:382` | `0.05`, the default u for an exact left operand in `x == t ± kσ`. It mixes `l.std_dev()` (own unit) with canonical magnitudes. | `10.0 mm == 10.04 mm +/- 1 sigma` is True (0.04 mm against 0.05 m). | no |
| B10 ⚠ **fixed on branch fix/audit-defects** (`53a0ac0`) | `physure-core/src/quantity.rs:30` | `Quantity: PartialEq` uses an absolute `1e-9` | Rust API `==` (bindings use their own comparisons, §C6) | no |
| B22 ⚠ **fixed on branch fix/audit-defects** (`53a0ac0`, `abc60bb`) | `physure-script/src/interpreter/helpers.rs:70-71` | truthiness `abs(x) > 1e-15` on the raw mean | `if 1e-16 m` is false, while `if 1e-16 km` is true | no |
| B8 ⚠ **fixed on branch fix/audit-defects** (`cb93ba2`) | `physure-core/src/uncertainty/monte_carlo.rs:87` | `samples.std(0.0)`, population standard deviation | Every Monte Carlo u. JCGM 101 (7.6) uses `1/(M−1)`. | **yes** (`unc.montecarlo_seed_1`; its value changes with the fix: `0.5112074131685965` → `0.5114632087378533`) |
| B7 | `physure-core/src/uncertainty/monte_carlo.rs:32` | seed mixing `seed ^ n·0x9E3779B97F4A7C15`; `StdRng` (ChaCha12), `rand_distr::Normal` | Seeded Monte Carlo draws. They can move with a `rand` dependency bump (the battery documents this). | **yes** |
| B6 | `physure-core/src/uncertainty/unscented.rs:40-46` | `n = 1`, `λ = 3 − n` (κ = 2, α = 1, β = 0), weights 2/3, 1/6, 1/6 | Unscented results | **yes** (`unc.unscented_square`) |
| B5 | `physure-core/src/uncertainty/expanded.rs:40-46` | ν_eff truncated to an integer, at least 1 (GUM G.4.1) | `coverage_factor`, `expanded` | **yes** (`unc.expanded_k_p95`) |
| B1 | `physure-core/src/math/stats.rs:14-16` | `MAX_ITERATIONS = 3000`, `EPSILON = 1e-16`, `TINY = 1e-300` (continued fractions) | Student-t and normal quantiles | **yes** (`stat.*`) |
| B2 | `physure-core/src/math/stats.rs:25-35`, `:41` | Lanczos coefficients (g = 7, n = 9) | ln Γ, and therefore every k_p | **yes** |
| B3 | `physure-core/src/math/stats.rs:171-189` | Bracket doubling up to `1e300`; 200 bisections; stop at `1e-15·high` | Every k_p | **yes** |
| B4 | `physure-core/src/math/stats.rs:21`, `:212-215` | `NORMAL_LIMIT = 1e6`: above it, the normal quantile plus the Cornish–Fisher term | k_p for ν > 10⁶ | **no** (the battery's largest ν is 100) |
| B11 **fixed on branch fix/audit-defects** with B10 (`53a0ac0`; not flagged as a defect in this audit, but it is the same absolute tolerance: `assert(1 fm, 1000 fm)` passed) | `physure-core/src/quantity.rs:548-549` | `assert` tolerances `REL 1e-9`, `ABS 1e-12` | PHS `assert` pass or fail (WASM, Java and Python `phs_assert`) | no (a user witness can) |
| B12 | `physure-core/src/quantity.rs:190`; `physure-core/src/units/rational.rs:338`, `:452`, `:454`, `:481` | `abs(scale − 1) < 1e-9` decides "plain number" and display-name inheritance | Which unit text is shown and whether a scale is folded into the value | indirect (`units.*`) |
| B9 | `physure-core/src/uncertainty/shapes.rs:58`, `:60`, `:132`, `:172`, `:263` | `1/√(2π)`, maximum skewness 1.6406, `RATIO_MAX = 1e12`, slack of 4·ε | Asymmetric pairs ↔ moments | no |
| B13 | `physure-core/src/units/exact.rs:29`, `:33` | `MAX_EXPONENT = 400`, `2^53`: where exact scales give up | Exact conversions | indirect (`units.*`) |
| B15 | `physure-core/src/covariance/pruning.rs:10`; mirrored at `physure-python/src/lib.rs:791`, `physure-python/physure/backends/torch/autograd_store.py:36-37` | `max_age 100`, `corr_threshold 1e-6`, `enabled false` | Python covariance pruning (off by default) | no |
| B21 | `physure-script/src/builtins/calc.rs:316` (series order `6`), `physure-script/src/builtins/array.rs:96` (`linspace` with `50` points, same as Python `ext/helpers.py:74`), `physure-script/src/builtins/core.rs:150` (`format` precision `6`), `physure-script/src/builtins/plot.rs:226` (3-D grid `25`) | API defaults of builtins | `linspace(0, 1)` has 50 elements (probe). `series("exp(x)","x")` is kept to x⁶ (probe). | no |
| B23 | `physure-script/src/symbolic/ode.rs:68`, `:114` (`1e-12`); `physure-script/src/symbolic/ast.rs:433` (`1e-9`); `physure-script/src/symbolic/display.rs:14` (`1e15`); `physure-script/src/check.rs:802` (denominators up to 12, `1e-9`, static checker only) | Symbolic-engine epsilons | `dsolve` branch choice (repeated root), symbolic printing | no |
| B24 | `physure-script/src/limits.rs:9-19` (call depth 1000 or 200 when sandboxed, stack 48 MiB, output 1 MiB); `physure-script/src/parser/expressions.rs:17` (128); `physure-script/src/parser/mod.rs:20` (64); `physure-cli/src/conf_witness.rs:52`; witness deadline 60 s | Sandbox and parser ceilings: a script either errors or doesn't, values are unaffected | error versus value | no |
| B25 | `physure-core/src/settings.rs:6`, `:46`, `:100` | `parallel_threshold 10000`, `max_loop_iterations 1000000`, `max_pipeline_steps 1000` | Already **covered**: `[Settings]` keys, and the compiled default equals the catalog | n/a |

## 5. Class C: binding-only data (owner phase in brackets)

| ID | file:line | What | Agreement with the core or catalog | Owner |
|---|---|---|---|---|
| C1 | `physure-python/physure/ext/chemistry/species.py:27-146` | `ATOMIC_WEIGHTS`: 118 elements, labelled "IUPAC 2021". The `U` column is used as a **standard** uncertainty (`species.py:225-228`). | **Disagrees**: 25 element means differ from `standard-2024` (H, Li, B, C, N, O, F, Mg, Al, S, Cl, Ar, Sc, Mn, Y, Zr, Rh, Gd, Ho, Tm, Lu, Hf, Au, Tl, Pb). The catalog's grammar divides a half-width by √3 and turns an interval into `(b−a)/√12`. Python does neither. Water: Python 18.015 ± 0.00108, core 18.01535 ± 0.000264. | chemistry plan 3 (delete; delegate) |
| C2 | `physure-python/physure/domain/measurement/equivalencies.py:36-37`, `:69` | `c`, `h`, `k_B` literals | Agree (exact SI-2019 values) | Phase 5 (read the catalog constants) |
| C3 | `physure-core/src/units/registry.rs:737-815` (`build_default_si`, `build_default_imperial`), exposed as `UnitRegistry.default_si()` and `default_imperial()` in `physure-python/src/lib.rs:378-385` | A hard-coded parallel unit table (about 30 units and aliases) that bypasses the catalog | Values agree. It is a subset, and no Python code calls it. | Phase 5 (remove) |
| C4 | `physure-python/src/lib.rs:999`; `physure-python/physure/domain/measurement/quantity.py:576` | Copies of the default of 1000 samples | Agree today; they would drift from A1 | with A1 (Python passes `None`) |
| C5 | `physure-python/physure/domain/measurement/_arithmetic_mixin.py:908` and `_backend_mixin.py:112` (`h = 1e-7`, **absolute**); `equivalencies.py:137` (`dx = 1e-8` relative); `physure-python/physure/core/autograd.py:133` (`epsilon = 1e-5`) | Python finite-difference steps for uncertainty propagation through functions | No core equivalent (the core uses analytic derivatives). The absolute step is unit-dependent. | Phase 5 (delegate) |
| C6 | `physure-python/physure/ext/helpers.py:20-21` and `physure-python/src/lib.rs:727` (`rel_tol = 0.1`, `abs_tol = 1e-5`); `physure-python/physure/domain/measurement/quantity.py:418` (`1e-9`/`1e-12`); `physure-python/physure/backends/*` `allclose` (`1e-5`/`1e-8`); `physure-java/src/main/java/com/physure/Quantity.java:331` (`approxEquals`, absolute `1e-6`) | Equality tolerances | Five different definitions of "equal" across the surfaces. `helpers.approx_eq(100 m, 109 m)` is **True** (probe). | Phase 5 |
| C7 | `physure-python/physure/domain/measurement/base_entity.py:32`, `definitions_text.py:57` | `limit_denominator(1000)` when an exponent becomes a fraction | The core turns a float exponent into a rational differently (`Rational64::from_f64`, `quantity.rs:377`). The PHS parser rationalises literals before that (`(8 m^3)^(1/3)` gives `2.0 m`). | Phase 5 |
| C8 | `physure-python/physure/domain/measurement/vectorized_uncertainty.py:319` | `size < 1000` switches between dense and sparse covariance | performance only, unless it crosses B14 | Phase 5 |
| C9 | `physure-python/physure/ext/symbolic_regression.py:56`, `:325-328`, `:336` | leaf probability 0.35, complexity 10, population 40, generations 25, `random.Random(seed=None)` | Python-only; **unseeded** by default | Phase 5 (document; seed) |
| C10 | `physure-python/physure/nn/layers.py:68`, `:115`, `:173`, `:200` | `torch.randn(...)*0.1` (global torch RNG), `log(abs(X) + 1e-9)` | Python-only | Phase 5 |
| C11 | `physure-python/physure/domain/measurement/quantity.py:844-850` | `__str__` prints the raw float repr, with no GUM rounding | **Differs** from the core display (`10.0 ± 1.0 V` versus `(10.0 ± 1) V`) | Phase 5 (call `gum_round`) |
| C12 | `physure-python/physure/application/startup.py:195-210`; `physure-python/physure/domain/measurement/system.py:98-103` | Python merges the embedded conf, then the system file, then **the working directory's `physure.conf`** with `configparser`. It keeps `[Constants]` in its own dict, which `get_constant` and Python chemistry read. | Python's constants and settings can differ from the core registry's: a system file's `[Constants]` never reaches the core. Python refuses to run while a frozen file is in force (`default_conf`, `lib.rs:1911-1917`), so this does not break a freeze today. | Phase 5 |
| C13 | `physure-java/src/main/java/com/physure/Quantity.java:331` | listed under C6 for Java | n/a | Phase 5 |

## 6. Cross-cutting records

### 6.1 Catalog drift between Python and the core (reads)

* **Keys nobody reads.** `[Settings] default_output`, `auto_simplify`, `default_system`, `readable_representation`,
  `verbose`, `mkml_recursion_limit` (the core lists them in `CORE_UNUSED_SETTINGS`, `physure-core/src/units/conf.rs:177`;
  Python stores them and reads only `propagation_mode` and `angle_mode`), and `[System] base_units` in the system files.
  An auditor reading `auto_simplify = true` would believe it does something. Recommendation: say "inert" in the coverage
  table, or remove them.
* **Python reads, the core does not:** a system file's `[Settings]` and `[Constants]` (C12), and the system files
  themselves (they carry only `name` and inert `base_units`).
* **The core reads, Python does not:** `[AtomicWeights*]`, `[Chemistry]` (Python has its own table, C1), and
  `[Categories]`.
* **Code parallels of catalog data:** A8 (derived-symbol and prefix tables, which disagree on `µ`/`u`), C2 and C3.

### 6.2 Inputs read from the environment or files at runtime

| Where | What | Effect on results |
|---|---|---|
| `physure-core/src/units/conf.rs:1319` | the working directory's `physure.conf` (lenient) | **yes**: units, constants, settings (governed while a frozen file is in force) |
| `physure-core/src/frozen/active.rs:19-20`, `:205-222` | `PHYSURE_CONF`, `PHYSURE_REQUIRE_FROZEN`, the working directory and its parents | which configuration is in force (by design) |
| `physure-python/physure/application/startup.py:203` | the working directory's `physure.conf`, a second reading through `configparser` | **yes**, Python only (C12) |
| PHS `use` (`physure-script/src/interpreter/statements.rs:268-275`) | `.phs` modules and native plugins `<base_dir>/ext/<stem>.<dll>` | **yes**: code that is outside the frozen file entirely. The sandbox blocks it for witnesses. This needs a line in the coverage table. |
| `physure-cli/src/config.rs:30-43` | `lang` from `./physure.conf`, `../`, `~/.config/physure/`, `~/physure.conf`, `~/.physure.conf` | labels only (D) |
| `physure-cli/src/conf.rs:868` etc. | the history `actor` | informational |
| `physure-python/physure/core/dispatcher.py:16` | `PHYSURE_DEBUG` | runtime checks only |
| `physure-cli/src/html.rs:535`, `physure-cli/src/protocol.rs:31` | `PHS_NO_OPEN`, the data directory | UI |
| Build time: `physure-core/src/frozen/engine.rs:77-83`, `physure-cli/src/main.rs:80` | `PHYSURE_BUILD_COMMIT`, `PHS_BUILD_SHA` | engine identity |
| Network | none at runtime in any library or binding (`ureq` appears only in `server.rs` tests and `phs upgrade`). Currency is **not** fetched. | n/a |

Python also delegates to whatever scipy, numpy, torch and jax versions are installed: `solve_ivp` and `curve_fit`
defaults in `application/solver_service.py` and `fit_service.py`. JAX arrays default to float32 because the backend
never enables `jax_enable_x64` (`backends/jax_backend.py:162`). These versions are environment, recorded by Phase 5.

### 6.3 Randomness

| Generator | Default seed | Fixed? |
|---|---|---|
| Core Monte Carlo (`monte_carlo.rs:28-35`) | none (OS entropy through `thread_rng`) | Seeded only inside `with_seed`: `phs eval` (seed 0, `report.rs:14`), MCP `evaluate` (0), user witnesses (0), battery (1). **Unseeded:** `phs <file>`, REPL, TUI, `--web`/`--html`, `phs serve`, LSP, WASM, Java, Python. |
| Seeded draws on rayon workers | the doc at `monte_carlo.rs:13-15` says workers are not seeded | Only reachable when `Limits.sequential` is false (not in sandboxed runs) |
| Python `symbolic_regression` | `random.Random(None)` | no |
| Python `nn.layers` | the global torch RNG; JAX `key` comes from the caller | no or caller's choice |
| Unscented, correlated and uncorrelated modes | none | deterministic |

### 6.4 Dependence on the floating-point environment

* **Platform libm.** `exp`, `ln`, `powf`, `sin`, `tanh`, `log10` in Rust call the target's libm: UCRT, glibc, musl, or
  Rust's `libm` on wasm32. Lanczos and ln Γ, Arrhenius and the logarithmic units all depend on it, so last bits can
  differ between targets. The per-platform witness run is the detector, as §8 intends.
* **No `mul_add`/FMA and no fast-math.** Grep found no `target-cpu` and no `.cargo` rustflags
  (`physure-core/.cargo/config.toml` sets only a PyO3 environment variable). The release profile is `lto = true`,
  `codegen-units = 1` (`Cargo.toml:61-63`). LLVM does not contract floating point without fast-math, so builds are
  deterministic per target.
* **Parallel evaluation.** PHS `for` at or above `parallel_threshold`, and `parallel_map`
  (`physure-script/src/interpreter/expressions.rs:451-466`, `physure-script/src/builtins/core.rs:328-346`): the collect
  is ordered and there is no parallel float reduction, so values do not depend on the thread count. **But** the workers
  re-open only the angle mode. A thread-local `with_seed`, `uncertainty::mode::scoped` or `units::with_catalog`
  (`physure-core/src/units/scope.rs:18-42`, used by WASM) is **not** seen inside the loop body. On a host that scopes a
  catalog and lets loops go parallel, the body reads the process catalog. I have not verified this on WASM, where rayon
  may not spawn threads.
* **Reductions.** `ndarray` `mean` and `std` over a fixed length are deterministic. In Python, numpy, BLAS and torch
  reductions depend on the environment.

### 6.5 Class D categories (not results)

1. HTML/three.js viewer styling, camera and lighting (`physure-core/src/plotting.rs:500-770`); the colormap
   (`plotting.rs:873-874`).
2. Mesh and glTF encodings: float32 buffers, `{:.4}` bounds, STL header (`plotting.rs:190-398`), and the same again in
   Python (`physure-python/physure/plotting.py:1275-1486`).
3. PHS SVG and ASCII plot geometry (`physure-script/src/builtins/plot.rs:387-600`).
4. Python matplotlib layout (`physure-python/physure/plotting.py`).
5. Buffer capacities and base64 tables (core, CLI).
6. CLI labels and the i18n language (`physure-cli/src/config.rs`), TUI and terminal widths.
7. MCP and JSON-RPC codes, timeouts and sleeps (`physure-cli/src/mcp.rs`, `physure-script/src/plugin.rs`).
8. Inspection caps: `MAX_INSPECTED_ELEMENTS = 10`, `MAX_UNIT_LEN = 200` (`physure-script/src/inspect.rs:51`,
   `physure-script/src/report.rs:295`).
9. Hashes (FNV in `physure-python/src/lib.rs:1710`) and Java `hashCode`.

---

## 7. Surprises

* **S1. Python and the core give different molar masses** (C1). This is the biggest silent disagreement in the library:
  25 elements, different uncertainty conventions, and a table label ("IUPAC 2021", with a 2013 reference) that matches
  neither shipped edition.
* **S2. A silent drop of uncertainty** in the covariance store (B14). This violates invariant #1 of `CLAUDE.md`, and the
  numpy probe shows it. *(Fixed on branch fix/audit-defects, `6125152`.)*
* **S3. A code table disagrees with the catalog.** `si_prefix_for_scale` writes `µ`; the catalog's micro is `u` (A8).
* **S4. The Monte Carlo standard deviation is the population form** (B8), although the doc cites JCGM 101, which uses
  `M − 1`. *(Fixed on branch fix/audit-defects, `cb93ba2`.)*
* **S5. Two loop ceilings.** `max_loop_iterations = 1000000` is configurable; the `while` cap of 10 000 is a hidden const
  in PHS (A9).
* **S6. Defaults that differ between Python and the core:**
  * `p = 0.95` exists only in Python (A5);
  * `approx_eq` defaults to `rel_tol = 0.1` in Python (`lib.rs:727`, `helpers.py:21`), against the core `assert`'s `1e-9`
    (since the defects track, the core's `assert(a, b)` is `phs_eq`, relative `1e-12`; the Python default is unchanged);
  * Python `__str__` does not round the uncertainty (C11);
  * Python finite differences, against analytic derivatives in the core (C5).
* **S7. The unit-system files the spec names carry no result-affecting number**: only an inert `base_units`.
* **S8. Catalog data that drops uncertainties.** The catalog is data, not code, but the audit met it.
  * `[Constants]` holds CODATA values **without their uncertainties**: `newtonian_constant_of_gravitation = 6.6743e-11`
    is shown as exact (CODATA u_r = 2.2e-5).
  * Two near-duplicate constants disagree:
    `bohr_magneton_in_inverse_meter_per_tesla = 46.686447719` and `..._meters_per_tesla = 46.68644814`
    (`physure-core/src/units/physure.conf:299-300`).

  Both deserve their own task.

---

## 8. Proposed migration set (Phase 4)

These rules apply to every item:

* **Compiled default.** Spec §13 requires that a compiled default reproduce the previous behaviour bit for bit.
* **How to add a key.** A new `[Settings]` key follows the existing path: a `Setting` variant in `units/conf.rs`, an
  `EffectiveSettings` field, `reset_to_defaults()` in `settings.rs`, and a thread-local `scoped_*` guard for the battery.
* **Python.** Python cannot honour a frozen file before Phase 5 (`default_conf` refuses). A Python-only item can have its
  *data* moved to a catalog text in Phase 4, but it stays "not frozen" in `docs/conf_coverage.md` until Phase 5.

### Priority 1: build in Phase 4

**M1. Monte Carlo samples (A1, C4).**

* **Key:** `[Settings] monte_carlo_samples = 1000`.
* **Read:** `Quantity::new_scalar` uses `samples.unwrap_or_else(crate::settings::monte_carlo_samples)`. Python
  (`lib.rs:999`, `quantity.py:576`) passes `None` when the caller gave no count, so the core decides. A frozen file's
  `[Settings]` governs it through `ProcessSettings::apply`.
* **Compatibility:** bit-identical (1000).
* **Battery:** pin `unc.montecarlo_seed_1` with a scoped `samples = 1000`, as the propagation mode is pinned, so that the
  witness keeps testing the engine (open question Q2).
* **Tests:**
  * a frozen file with `monte_carlo_samples = 2000` changes `phs eval` output for a seeded Monte Carlo script;
  * an unfrozen run and a run of a file that omits the key both give the bits recorded today;
  * `entries` lists the key.

**M2. Seed policy (A10).**

* **Key:** `[Settings] monte_carlo_seed = none` (default `none` is today's behaviour; an integer seeds every top-level
  evaluation).
* **Read:** the entry points that evaluate (`phs <file>`, REPL statement, `serve` request, LSP, WASM, Java, Python
  `Q_`) wrap evaluation in `with_seed(setting)` when it is an integer. The CLI's `--seed` and `phs eval`'s default 0 take
  precedence, as host choices.
* **Compatibility:** identical with the default `none`.
* **Tests:**
  * the same script run twice with the setting gives identical bits, and without it differs;
  * a frozen file with a seed reproduces across processes.

  This needs a decision (Q1), because "seed per evaluation" versus "per process" changes what is reproducible.

**M3. Coverage of the `while` cap (A9).**

* **Key:** `[Settings] max_while_iterations = 10000`.
* **Read:** `settings::max_while_iterations()`, in the same pattern as `max_loop_iterations`. It is fixed at the compiled
  default for witnesses, as the other ceilings are (§9).
* **Compatibility:** identical.
* **Test:** a conf with `max_while_iterations = 30000` lets the 20 000-iteration loop finish, and the default still
  errors at 10 000.

**M4. Exchange rates (A2), data only.**

* **Where:** a shipped conf text `currency.conf` (not the default catalog: rates are not physics and would put `EUR`
  into every frozen file):
  ```
  [Dimensions]
  money = $, noprefix
  [Units]
  USD = 1.0, $, noprefix, [USD, dollar, dollars, dolar]
  EUR = 1.08, $, noprefix, [EUR, euro, euros]
  MXN = 0.059, $, noprefix, [MXN, peso, pesos]
  ```
  The core accepts `$` since stage 2a.
* **Read:** `enable_currency()` becomes `system.extend_core(<text>)`. In Phase 5 a system file becomes a source and the
  frozen file records it.
* **Compatibility:** **not bit-identical**. The core's exact scale gives `100 MXN => USD` = `5.9` where the Python float
  gives `5.8999999999999995`. This is an improvement, but it needs a CHANGELOG entry.
* **Before Phase 5?** The data moves now. Freezing it waits for Phase 5.
* **Test:** the conversions above against the text, and an alias-collision check (`add-unit` skill) for USD, EUR, MXN,
  dollar and peso.

**M5. Thermochemical tables (A3).**

* **Sections** follow the edition pattern of `[AtomicWeights]`: `[Thermochemistry:starter-2026]` with
  `H2O(l) = -285.8, 69.9` (ΔfH° in kJ/mol, S° in J/(mol·K), at 298.15 K and 1 bar, stated in the section comment),
  `[Chemistry] thermochemistry = starter-2026`, and a user override section `[Thermochemistry]`.
* **Read:** a core reader `UnitRegistry::standard_enthalpy_formation(species)` through `try_with_catalog_in_scope`.
* **Compatibility:** identical numbers. Keys gain a phase, so Python's `"H2O"` maps to `H2O(l)` and needs an alias.
* **Before Phase 5?** The core reader and the data move now. Python delegates in chemistry plan 3.
* **Tests:**
  * a `chem.dhf_h2o` witness;
  * an override in a frozen file changes it;
  * the Python table equals the catalog until it is deleted.

  Q4 asks whether uncertainties should be added.

### Priority 2: worth doing in Phase 4 if time allows

**M6. Asymmetric shape default (A4).**

* **Key:** `[Settings] asymmetric_shape = dimidiated`.
* **Read:** a process value consulted where `ShapeKind::DEFAULT` is today (`moments.rs:83`, `:93`, `:112`, `:402`,
  `:426`, `:490`).
* **Compatibility:** identical.
* **Tests:** add the battery case `unc.asymmetric_sum`; `fechner` in a frozen file changes it.

**M7. Display rules (A6).**

* **Keys:** `[Settings] display_significant_digits = 15`, `display_scientific_below = 1e-4`,
  `display_scientific_from = 1e16`, `uncertainty_digits = gum` (`gum` is the 1-or-2 rule; an integer forces a count, as
  `@precision` does).
* **Read:** `format_float` and `gum_round` read the settings once per call.
* **Compatibility:** identical.
* **Tests:**
  * a golden table of `format_float` and `gum_round` outputs, unchanged at default;
  * a frozen file with `display_significant_digits = 6` changes `phs` text output.

  Because witnesses do not record text, the coverage table must say plainly that printed text is protected only by these
  keys and the engine pin.

**M8. Default degrees of freedom (A7).** `[Settings] default_dof = inf`. Identical. Test: `inf` and `50` give different
`coverage_factor` values for an undeclared input. This depends on Q5.

**M9. Default coverage probability (A5).** Either remove the Python default (`p` required, as in every other binding),
or add `[Settings] coverage_probability = 0.95` that bindings read when `p` is omitted. The setting reaches Python only in
Phase 5. Recommendation: remove the default; there is then no data to freeze.

**M10. Unit display tables (A8).** No new key: derive the symbol and prefix from the catalog in scope (`[Prefixes]`, and
the unit whose recipe matches) instead of the two match tables. This **changes text** (`µ` becomes `u`) and needs a
CHANGELOG entry. Test: the repr of an anonymous `1e-6` force unit equals the catalog's prefix symbol.

### Not migrated (and why)

* **C1, Python `ATOMIC_WEIGHTS`.** The catalog already holds the data. Delete it in chemistry plan 3 and list it as
  "Python chemistry: not frozen" until then.
* **C2, C3.** These are removals (Phase 5).
* **B16–B20, B22.** These are defects, not data. They need engine fixes (relative tolerances, an error estimate, refusal
  instead of guessing). A key would freeze a wrong answer. *(Fixed on branch fix/audit-defects with B8, B10, B11 and B14;
  the only key the fixes added is `approx_rel_tol`, the relative tolerance of `≈`.)*

---

## 9. Left in the engine (class B) and how it is protected

| Protection | Items |
|---|---|
| Engine pin **and** a battery witness | B1, B2, B3, B5, B6, B7, B8 (fixed; the witness value changed) (and A1 until M1) |
| Engine pin, indirect witness (`units.*`, `const.*`) | B12, B13 |
| Engine pin only, **no witness** | B4 (ν > 10⁶), B9 (asymmetric shapes), B10 (fixed), B11 (fixed), B14 (fixed), B15, B16 (fixed), B18 (fixed), B19 (fixed), B20 (fixed), B21, B22 (fixed), B23, B24 |
| Engine pin only, no witness, **defect** | none open: B14, B16, B17, B18, B19, B20 and B22 are **fixed on branch fix/audit-defects** (§4) |
| Already covered by `[Settings]` | B25; B17 (its tolerance is now `approx_rel_tol`, a frozen entry; the rule itself is engine) |

Cheap witnesses that would close gaps:

* `stat.t_p95_nu2e6` (B4);
* `unc.asymmetric_sum` (B9 and A4);
* PHS-level cases, as built-in user witnesses or a new `phs.*` family, for `==`, `≈`, `limit`, numeric `integral` and
  `linspace` (B16–B21). The core battery cannot reach PHS builtins. Now that these are fixed, such cases would pin the
  corrected behaviour (the tests of the defects track pin it in the test suite, not in a frozen file).

---

## 10. Open questions for the owner

1. **Q1. Seeds (M2).** Should a frozen file be allowed to evaluate Monte Carlo unseeded at all? Options:
   1. `monte_carlo_seed` defaults to `none` (today's behaviour);
   2. a frozen file **requires** a seed whenever `propagation_mode = monte_carlo`;
   3. seed per evaluation, or per process.
2. **Q2. Battery and settings.** Should battery cases pin `monte_carlo_samples` (and the future `asymmetric_shape`)
   thread-locally, as they pin the propagation mode, so a witness tests the engine and not the file's own settings? Or
   should they read the file, so an override shows in its witness?
3. **Q3. Currency.** Ship exchange rates at all? They are not physical constants and go stale. They could ship as a
   dated, opt-in `currency.conf` (proposed), or with no rates at all, leaving the user to define them.
4. **Q4. Thermochemistry.** Phase, standard-state pressure (1 bar against 1 atm), uncertainties (NIST quotes, e.g.
   ΔfH°(H2O, l) = −285.83 ± 0.04 kJ/mol), and the edition name. Is 0.1 kJ/mol rounding acceptable as "starter" data?
5. **Q5. Degrees of freedom.** Is "undeclared ν means ∞" the right default for a metrology library, or should `expanded`
   refuse when an input has no declared ν?
6. **Q6. Display as a result.** Do audited users treat printed text as the result? If so, M7 belongs in priority 1, and
   `phs-witness-1` could also record the display text.
7. **Q7. Defects before freezing.** Fix B14 and B16–B20/B22 before Phase 4 ships? Freezing validations on top of them
   would make every later fix an "unannounced alteration" for frozen users. **Answered (2026-11-14):** fixed first, in
   their own track, before Phase 4b; done on branch fix/audit-defects (§4, §11).
8. **Q8. Inert settings.** Remove `default_output`, `auto_simplify`, `default_system`, `readable_representation`,
   `verbose`, `mkml_recursion_limit` and `[System] base_units`, or keep them and mark them inert in the coverage table?
9. **Q9. Catalog constants without uncertainties, and the duplicate Bohr-magneton entries (S8).** Is that in scope for
   Phase 4's "data" audit, or a separate catalog task?
10. **Q10. Plugins and modules.** PHS `use` loads `.phs` modules and native plugins from disk, and neither is in the
    frozen file. Should `verify --run` or evaluation under a frozen file refuse them, or record their hashes?
11. **Q11. Parallel loops and thread-local scopes (§6.4).** Should rayon workers re-open the seed, the propagation-mode
    scope and the catalog scope, as they re-open the angle mode?

---

## 11. Findings of the defects track

Added on 2026-11-14 by the defects track (branch `fix/audit-defects`, plan
`docs/superpowers/plans/2026-11-14-audit-defects.md`). Its adversarial review and its implementers probed beyond the
audit's list. Every probe below was run with `phs eval -`: the "before" values on the `conf-freeze-3` debug build, the
"after" values on the branch. The commit messages hold the full probe lists.

### 11.1 Found and fixed on the branch

| ID | What | Before | After | Commit |
|---|---|---|---|---|
| X1 | `call_by_name` held the lock of the unlocked builtins while a calc builtin ran, so a calc builtin that called another one waited forever | `integral("sqrt(x)","x",0,1)`, `integral("tan(x)","x",0,3)`, `integral("abs(x-1/3)","x",0,1)` did not stop | `0.666666666666667`, refused, refused | `13e4eab` |
| X2 | `integral` bounds: an unknown name or `pi` read as `0`, and the unit of a quantity bound dropped | `integral("sin(x)","x",0,pi)` `0.0`; `integral("x","x","a",1)` `0.5`; `integral("x","x",0 m,2 m)` `2.0` | `2.0`; an error; `2 m^2` | `13e4eab` |
| X3 | The integrand did not see the variables of the program | `v = 3; integral("v*x","x",0,2)` `0.0` | `6` | `13e4eab` |
| X4 | The symbolic definite integral was returned without checking that the antiderivative is continuous on the interval; the symbolic engine read a comparison as its left side | `integral("x^(-2)","x",-1,1)` `-2.0`; `integral("sec(x)^2","x",0,3)` `-0.1425`; `integral("x < 0.5","x",0,1)` `0.5` | refused (a closed form is returned only when the quadrature confirms it) | `cef665e` |
| X5 | The improper-interval mapping inverted `x = 2t/(1 − t²)` but integrated under `x = t/(1 − t²)` | `integral("exp(-x)","x",1,"inf")` `0.606530659712631`; `integral("1/x^2","x",1,"inf")` `1.99999799999899` | `0.36787944117144…`; `1.0` | `cef665e` |
| X6 | An input that appears twice in one operation added only its first Jacobian to the cross covariance (`cov(x + x, x) = var(x)`) | Python arrays, `u = 1e-3 m`: `(x + x) - x` `1.73e-3`; `(x + x) - 2*x` `2e-3` | `1e-3`; `0` | `23cd3a1` |
| X7 | B14 also dropped every Jacobian entry at or below `1e-12` | `x * 1e-13`, `u = 1e-3 m`: `[0, 0]` | `1e-16` | `6125152` |
| X8 | Operand pairs that have no comparison answered `False`; strings were ordered | `vector(1,2) == vector(1,2)`, `1 m == "a"`, `1 m < 20 m +/- 1 sigma` `False`; `"b" < "a"` `True` | type errors naming both operand types | `abc60bb` |
| X9 | The sigma-bound target literal carried `k` as its uncertainty | `y = 10.04 mm +/- 2 sigma` displayed `10 ± 2 mm ± 2.0σ` | `10.04 mm ± 2.0σ` | `abc60bb` |
| X10 | The dimensionless-zero exception of comparisons applied to temperature points and levels | `-10 degC > 0` `True`; `5 m > 0 dB` `True` | unit-mismatch errors (`1 m > 0` is still `True`) | `53a0ac0` |
| X11 | `limit` probed one side only, at `p + 1e-8`, read any failure, non-number or unknown point as `0`, did not see the call's scope, and dropped the unit and uncertainty of the point | `limit("abs(x)/x","x",0)` `1.0`; `limit("q*x","x",1)` `0.0`; `limit("x","x",2 m)` `2.00000001` | refused; refused; `2.0 m` | `b69f22c` |
| X12 | The LSP hover called `limit` symbolic; there is no symbolic limit | "Computes symbolic limit" | says what the numeric method does | `8c249df` |
| X13 | Monte Carlo accepted fewer than two samples, and Python replaced a bad `samples` by the default | `M = 0`: mean `0.0`, std NaN; `M = 1`: std `0.0`; Python `samples = -1` ran with 1000 | refused (`DomainError`; Python `ValueError`, `OverflowError` for `-1`) | `cb93ba2` |
| X14 | The test helper `embedded_catalog_problems()` applied the embedded catalog's `[Settings]` to the process outside the settings lock, behind an intermittent failure of `parses_max_pipeline_steps_from_settings_section` | a locked test read back `1000` after writing `42` | the helper reads without applying | `3d9f905` |

### 11.2 Open (not fixed on the branch)

| ID | file:line | What | Probe (branch build) | Decision needed |
|---|---|---|---|---|
| B26 | `physure-script/src/phs.pest:125` (`comp_expr = { term ~ (_is_compare ~ op_compare ~ term)* }`), `:135` | **Comparison binds tighter than `+` and `-`.** `0.1 + 0.2 == 0.3` is parsed as `0.1 + (0.2 == 0.3)` | `0.1 + 0.2 == 0.3` and `0.1 m + 0.2 m == 0.3 m`: "Invalid operand types for binary operation" (before and after the track). Every test of the track parenthesises its sums; the guide says to. | Language semantics, for the owner. The grammar was **not** changed. |
| T1 | `physure-script/src/check.rs:745`; `physure-script/src/report.rs:184` | The static checker flags a valid sign test as a dimension mismatch, and `phs eval` hides the runtime error of any line the checker flagged | `1 m > 0`: the runtime answers `True`, the checker reports "cannot compare L with dimensionless", so `phs eval` gives `ok: false`. On a flagged line the interpreter's own error is dropped ("say it once"), so a different runtime error there is never shown. | Teach the checker the dimensionless-zero exception (linear units only, as the runtime now does); report a runtime error whose text differs from the checker's. |
| T2 | the symbolic engine (`physure-script/src/symbolic/`) | Comparisons are dropped silently by the symbolic engine | `integral("x < 0.5","x")` (indefinite) answers `x^2/2`. The definite form is now refused (X4) because the quadrature sees a Bool. | Refuse a comparison in symbolic input. |
| T3 | `physure-script/src/builtins/mod.rs:88-112` (`preprocess_symbolic_expression`, `inline_call`) | Inlines only the **first** call of each user function, up to its first closing parenthesis, and does not substitute the arguments | read at file:line, not probed | Inline every call with its arguments, or refuse user functions in symbolic input. |
| T4 | the runtime builtins | PHS has no runtime `sec`, `csc`, `cot` or `erf`, while the symbolic engine integrates them | `sec(1)`, `csc(1)`, `cot(1)`, `erf(1)`: "Undefined function". `integral("sec(x)^2","x",0,1)` is refused: "the antiderivative tan(x) cannot be confirmed … Undefined function 'sec'" (true value `tan 1`). | Add the four functions (core first). |
| T5 | the interpreter's name lookup | An unbound name evaluates to a string of itself | `foo` gives `foo`; `pi` gives `pi`; `x = pi * 2` is flagged by the checker ("'pi' is neither a variable nor a unit"). Before the track `pressure > 0 Pa` answered `False` silently (now a type error, X8). | Whether an unbound name should be an error. |
| T6 | `physure-script/src/interpreter/helpers.rs:89-90` | Truthiness of strings and other types is unchanged by B22: the strings `"true"`, `"True"` and `"1"` are true, every other string is false, and every other type (vector, matrix, function, ...) is false | `if "true" then 1 else 0` and `if "1" then 1 else 0` give `1.0` | Ask the owner: should a non-Bool, non-numeric condition be an error? |
| T7 | `physure-core/src/covariance/store.rs:202-203` (`prune_weak_correlations`) | The B15 pruning reads a missing variance norm as `unwrap_or(1.0)`, an absolute value in whatever unit | read at file:line; pruning is off by default | Skip (or refuse) a block whose variances are unknown. |
| T8 | `physure-core/src/covariance/store.rs` (`numpy_to_csr`) | Since X7 a Jacobian entry is dropped only when exactly zero, so a Jacobian full of rounding noise stays dense: a storage and time risk for large arrays, not a correctness one | not measured | Measure on a large array; if needed, a relative rule per row. |
| T9 | `physure-core/src/quantity.rs:265`, `:270` | `Quantity::new_scalar` panics on a Monte Carlo sample count below 2 while `try_new_scalar` returns the error (84 callers kept the old signature; no caller passes a user's count) | read at file:line | Move callers that can receive a user's count to `try_new_scalar`. |
| T10 | `physure-script/src/builtins/limit.rs` | `limit` is two-sided only; there is no syntax for a one-sided limit | `limit("1/x","x",0)` is refused because the sides disagree | Language syntax, for the owner. |
| T11 | `physure-script/src/builtins/limit.rs:382-390` (`first_step`) | The extrapolation starts at `h = 1/8` **in the variable's unit**, so a borderline refusal can depend on the unit of the point | read at file:line; the accepted values in the test suite are unit-independent | Scale `h0` to the point's magnitude or prove the refusals unit-independent. |
| T12 | `physure-script/src/builtins/definite_integral.rs`, `limit.rs` | `pi` and the catalog constants resolve as names only inside `integral` and `limit` texts and bounds; elsewhere PHS has no such identifiers (T5) | `integral("sin(x)","x",0,pi)` gives `2.0`, while `pi` alone gives the text `pi` | Question for the owner: make `pi` and the catalog constants global identifiers? |

The binding disagreements the track recorded (Python `Quantity.__eq__` exact after conversion, `PyQuantity.__eq__` same
unit and exact mean, `approx_eq` defaults `0.1` / `1e-5`, the Python code generator's `approx_eq` comparing in the left
operand's unit, Java `approxEquals` absolute `1e-6`, the Java code generator's `==` as `.equals`, no comparison API in
WASM, no `≈` mapping in the Rust code generator) are C6 items of §5, owned by Phase 5 and the binding-contract work.
