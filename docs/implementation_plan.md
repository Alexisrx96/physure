# 🧭 Implementation Plan — Chemistry Core Port, then CAS Phase 2

**Status:** Approved sequencing, not yet started
**Order:** Part A (Chemistry → Rust core + PHS) **first**, then Part B (Symbolic CAS Phase 2)
**Parent roadmaps:** [Chemistry Roadmap](chemistry_roadmap.md) · [Symbolic Math Roadmap](symbolic_math_roadmap.md) · [Master ROADMAP](ROADMAP.md)

This document is the *how* and the *order*. The sub-roadmaps remain the *what* and the *why*.

---

## 0. Prerequisite finding — the CAS is not where the roadmaps say it is

`docs/ROADMAP.md` and `docs/symbolic_math_roadmap.md` place the symbolic engine at
`physure_core::cas::{polynomial, groebner, rational, series}`. **That module does not exist.**

The real engine is `physure-script/src/symbolic/` (~4 200 lines: `ast.rs`, `expr.rs`, `diff.rs`,
`integrate.rs`, `series.rs`, `solve.rs`, `ode.rs`, `transforms.rs`, `sym_matrix.rs`, `factor.rs`),
built on `enum Node` (`ast.rs:7`) whose numeric leaf is `Number(f64)` — no polynomial
representation, no exact arithmetic.

Two consequences:

1. **Chemistry is unaffected.** `physure-core/src/chemistry/` is reachable from both
   `physure-script` (which depends on `physure-core`) and `physure-python`. No decision needed.
2. **CAS Phase 2 is blocked on a placement decision.** Gröbner bases and Hermite partial fractions
   need exact rational coefficients; they cannot simply extend `Node`. Resolving this is task
   **S0** below, and it is why Chemistry goes first.

---

# Part A — Chemistry to the Rust core and PHS

Implements Phases 5–6 of [chemistry_roadmap.md](chemistry_roadmap.md). Phases 7–8 (multi-reaction
networks, NASA CEA equilibrium, C-FFI / WASM / JNI) are **out of scope**.

Source of truth for the port: `physure-python/physure/ext/chemistry/` (669 lines, already tested by
`physure-python/tests/ext/test_chemistry_integration.py`). The Python behaviour is the spec; the
Rust port must not change results.

## A1 — `physure-core/src/chemistry/{mod,elements,formula,species}.rs`

| Item | Detail |
|---|---|
| `elements.rs` | The 118 IUPAC 2021 standard atomic weights as a sorted `static [(&str, (f64, f64))]` + `binary_search_by_key`. Port the table verbatim from `species.py:27`. |
| `formula.rs` | Recursive-descent parser: nested parentheses, multi-digit subscripts, Unicode subscripts, hydrate dots (`·`, `*`). Port `parse_formula` (`species.py:171`) and the `subscript_to_ascii` normalisation. |
| `species.rs` | `Composition(BTreeMap<String, u32>)`; `Species { formula, composition }`; `molar_mass() -> (f64, f64)` — mass sum, uncertainty combined in quadrature weighted by atom count (GUM/JCGM 100:2008 §5.1). |

**Deliberate simplifications**

- **No `phf`.** 118 entries do not justify a new build-dependency; a sorted static slice with binary
  search is `O(log n)`, zero-allocation, zero-dep. Revisit only if a benchmark shows the lookup.
- **No `ChemistryError`.** Reuse the existing `PhysureError` variants.
- **Reject, don't guess.** Ionic charges (`SO4^2-`) and aggregation states (`(aq)`) are not parsed by
  the Python implementation either. Return an explicit error naming the unsupported token rather
  than silently dropping it — a wrong composition is a wrong molar mass.

**Tests:** molar masses of `H2O`, `C6H12O6`, `Ca(NO3)2`, `CuSO4·5H2O` against the IUPAC table,
values *and* uncertainties. Malformed formulas (`H2O)`, `Xx3`, empty) must error.

## A2 — `physure-core/src/chemistry/reaction.rs`

Port `_rref` (`reaction.py:42`) and `_balance` (`reaction.py:74`) using `Ratio<i64>` —
`num-rational` is already a workspace dependency, so no new deps.

- `parse_equation` accepts all five separators the Python regex accepts: `->`, `=`, `<=>`, `→`, `⇌`,
  and preserves the reversible flag.
- Exact arithmetic is mandatory: float RREF mis-rounds large stoichiometries (octane combustion).
- Overflow in the LCM/GCD normalisation must return an error, never wrap.

**Tests:** octane combustion (`2 C8H18 + 25 O2 -> 16 CO2 + 18 H2O`), `Fe2O3 + C -> Fe + CO2`
(→ 2/3/4/3), and an under-determined system that **must** fail with "expected exactly one degree of
freedom" rather than pick an arbitrary solution.

## A3 — `physure-core/src/chemistry/thermo.rs`

`arrhenius`, `gibbs_free_energy`, `clausius_clapeyron` operating on `Quantity` with the unit
registry, so the dimensional checks and the gas constant come from the core rather than being
re-derived per binding. Port from `thermo_kinetics.py:53-99`.

`standard_enthalpy` / `standard_entropy` stay in Python for now — they are lookup tables, not
algorithms, and nothing in PHS needs them yet.

**Tests:** `arrhenius` against a hand-computed `k`; `gibbs` reproducing −237.1 kJ/mol for the
water-formation example in the roadmap; dimensional mismatch (e.g. `Ea` in kelvin) must raise.

## A4 — PHS domain `chem`

| File | Change |
|---|---|
| `physure-script/src/builtins.rs:72` | `domain_members`: add `"chem" => Some(&["species", "molar_mass", "composition", "balance", "arrhenius", "gibbs", "clausius_clapeyron", "mass_to_moles", "moles_to_mass"])` |
| `physure-script/src/builtins.rs:89` | `eval_domain_builtin_with_kwargs`: add the `"chem" => eval_chem_builtin(...)` arm |
| `physure-script/src/builtins.rs` | New `eval_chem_builtin`, following the shape of `eval_calc_builtin` |
| `physure-script/src/value.rs:15` | `PhsValue::Species(Species)` and `PhsValue::Reaction(BalancedReaction)`, with `Display` (`"2 H2 + O2 -> 2 H2O"`) and field access through the existing property syntax |
| `physure-script/src/interpreter.rs:328-334` | Map the new domain name — see the bug below |

> ⚠️ **Latent bug to fix in the same PR.** `domain_members` matches `"array" | "matrix"`
> (`builtins.rs:76`), but `resolve_use` (`interpreter.rs:329-334`) only maps `"calc" | "plot" |
> "array"` and falls into `unreachable!("domain_members returned Some for unknown domain")`.
> So `use dot from matrix` panics the interpreter today. Adding `chem` touches exactly this match —
> replace the hand-written mapping with one that cannot drift out of sync with `domain_members`.

**Target syntax** (already spec'd in `chemistry_roadmap.md` §6.2):

```phs
use balance, molar_mass, mass_to_moles from chem

M_water = molar_mass("H2O")            # 18.015 +/- 0.001 g/mol
sample  = 50.0 +/- 0.1 g
n_co2   = mass_to_moles(sample, "CO2")
rxn     = balance("Fe2O3 + C -> Fe + CO2")
```

**Tests:** an end-to-end `.phs` script per builtin; `use nope from chem` must produce the existing
"no such function" error, not a panic.

## A5 — Python delegates to the core

- PyO3 bindings in `physure-python/src/lib.rs` (registered in the `#[pymodule(name = "_core")]` at
  `lib.rs:1700`). **Not** in `physure-core` — that crate must never depend on `pyo3`.
- `physure/ext/chemistry/species.py` and `reaction.py` try `from physure._core import ...` and keep
  the pure-Python implementation as fallback, matching the pattern used elsewhere in the package.
- `tests/ext/test_chemistry_integration.py` must pass identically on both paths — parametrise on
  core availability so the fallback is actually exercised, not just present.

## Part A acceptance

- [ ] `cargo test -p physure` green, chemistry module ≥ 80 % line coverage
- [ ] `uv run pytest` green with and without the compiled core
- [ ] `phs` script from §6.2 of the chemistry roadmap runs and prints the documented values
- [ ] `use dot from matrix` no longer panics
- [ ] ROADMAP subsystem 3 moved from 50 % to Phases 5–6 ✅

---

# Part B — Symbolic CAS Phase 2

Implements Phase 2 of [symbolic_math_roadmap.md](symbolic_math_roadmap.md). Ordered so that the
first user-visible win (correct `expand`, then real rational integration) lands before the
theory-heavy work.

## S0 — Placement decision (blocking, ~1 day)

Create `physure-core/src/cas/` and put polynomials, rational functions and Gröbner there, with a
`Node ↔ Poly` bridge staying in `physure-script/src/symbolic/`.

Rationale: "the Rust core comes first" — code in `physure-script` is invisible to the Python, Java
and WASM bindings. Fix the incorrect module paths in both roadmaps in the same PR (see §0).

## S1 — `cas/poly/`: the multivariate ring

- `Monomial(Vec<u32>)` over an ordered variable list.
- `MonomialOrder { Lex, GrLex, GrevLex }`.
- `MultiPoly(BTreeMap<Monomial, Ratio<i128>>)`: add, multiply, multivariate division with remainder,
  leading term/monomial/coefficient.
- **Coefficients are `Ratio<i128>`, and overflow is an error, not a wrap.** No `num-bigint` until a
  real computation overflows — that is the trigger to revisit, and the error message will say so.
- **No Karatsuba/FFT multiplication.** Schoolbook until a benchmark says otherwise.

## S2 — `Node ↔ MultiPoly` bridge (first visible value)

- `MultiPoly::from_node` returns `None` cleanly when the expression is not polynomial.
- `f64 → Ratio` conversion accepts only exactly-representable rationals; anything else falls back to
  the current heuristic path. Never silently round a coefficient.
- Route `expand()` and `simplify()` through the canonical form instead of `factor.rs` heuristics.

## S3 — Univariate: square-free and GCD

Yun's square-free factorisation; GCD via **subresultant PRS**. Rational root finding.

**Not** EZGCD or modular GCD — substantially more code, no use case in physure that the subresultant
algorithm fails to serve.

## S4 — Hermite partial fractions ← *the actual payoff of Part B*

- `cas/rational.rs`: Hermite reduction (derivative part + square-free log-derivative part), then full
  PFD over ℝ (linear + irreducible quadratic factors).
- New `apart()` builtin in the `calc` domain.
- Wire into `integrate_div` (`physure-script/src/symbolic/integrate.rs:422`) so rational functions
  integrate properly instead of falling through the current special cases.

This is the prerequisite for the Risch integrator (Phase 3) and it is where Part B earns its cost.

## S5 — Gröbner bases

Buchberger's algorithm with the Gebauer-Möller criteria; `groebner()` and `solve_system()` builtins
in the `calc` domain. Elimination-ideal test suite per roadmap milestone **M2.2**.

> **F4/F5 is explicitly out of scope.** It is a sparse linear-algebra engine over finite fields —
> months of work — and no physure workload currently exists that Buchberger cannot finish. It gets
> reconsidered when a concrete benchmark times out, not before.

## Part B acceptance

- [ ] `expand((x+y)^5)` and friends canonical and exact
- [ ] `apart()` matches textbook decompositions; `integrate(1/(x^2-1), x)` correct
- [ ] Gröbner elimination test suite green
- [ ] Roadmap module paths corrected; symbolic subsystem moved past 35 %

---

## Out of scope, and the trigger to reconsider

| Skipped | Add when |
|---|---|
| `phf` for the periodic table | a profile shows element lookup on a hot path |
| `num-bigint` for CAS coefficients | an `i128` overflow error fires on a real computation |
| A dedicated `ChemistryError` | `PhysureError` variants stop describing a failure accurately |
| Chemistry Phases 7–8 (networks, CEA, FFI/WASM/JNI) | Phases 5–6 are shipped and a user needs multi-reaction systems |
| F4/F5 Gröbner | a Buchberger run on a real system does not terminate in reasonable time |
| EZGCD / modular GCD | subresultant PRS becomes the measured bottleneck |
| Karatsuba/FFT polynomial multiplication | high-degree benchmarks justify it |
| `standard_enthalpy`/`standard_entropy` tables in Rust | PHS needs them |

## Quality gates (both parts)

Per `CLAUDE.md`: `uv run ruff check .` and `ruff format --check` clean; coverage ≥ 80 %; Python
3.11–3.14 green; SonarQube gate green on new code; doctests runnable. `maturin develop` is required
after any change under `physure-core/src/`, and `import physure` must stay inside the ~0.5 s budget.
