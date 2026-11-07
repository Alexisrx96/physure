# GUM uncertainty compliance report

**Status:** technical audit, not a metrological certification  
**Date:** 2026-08-26  
**Code revision:** `ea37c8c89deaef86e3f335d5011411050cfe7d30`

## 1. Purpose and scope

This report compares Physure's uncertainty handling with:

- [JCGM 100:2008 (GUM)](https://www.bipm.org/doi/10.59161/JCGM100-2008E), particularly Type A/B evaluation, the law of propagation, correlations, expanded uncertainty, and reporting;
- [JCGM 101:2008](https://www.bipm.org/doi/10.59161/JCGM101-2008), for propagation of distributions using Monte Carlo;
- [JCGM 102:2011](https://www.bipm.org/doi/10.59161/JCGM102-2011), for measurement models with multiple output quantities;
- the asymmetric-uncertainty work proposed by [@jamontanac in issue #46](https://github.com/Alexisrx96/physure/issues/46) and partially incorporated through [PR #61](https://github.com/Alexisrx96/physure/pull/61), based on Barlow, Brazzale, and Volobouev, [*Asymmetric Errors*](https://arxiv.org/abs/2411.15499).

The word **compliant** is used here only in a limited technical sense: a formula or behavior agrees with an applicable section of GUM under its assumptions. It does not imply that Physure alone produces a traceable, accredited, or certified measurement result.

### Design principle

The project aims to be **compatible and interoperable with GUM, not restricted exclusively to GUM**. GUM defines a metrological baseline that Physure must be able to represent, propagate, validate, and report without information loss. On top of that baseline, the engine may provide additional methods—such as propagation of distributions, Monte Carlo, the unscented transform, asymmetric moments, intervals, or domain-specific models—when they are more appropriate.

An extension outside the classical GUM workflow is acceptable when it:

1. explicitly declares its model, assumptions, and parameters;
2. is not presented as GUM when it is not;
3. preserves provenance and avoids silent degradation;
4. can produce, where meaningful, an interoperable metrological projection: estimate, standard uncertainty, covariance, and clearly defined coverage;
5. provides numerical validation or appropriate references for its own method.

## 2. Executive conclusion

Physure correctly implements the core **GUM first-order law of propagation of standard uncertainty** for scalar Gaussian quantities, including dependencies that share the same source. It does not yet implement the complete metrological process required to describe, calculate, and report a fully GUM-compliant measurement result.

| Area | Status | Conclusion |
|---|---|---|
| Symmetric standard uncertainty | Partially compliant | `x +/- u` is interpreted directly as a Gaussian standard deviation; this is correct only when the user already supplies a standard uncertainty. |
| First-order linear propagation | Compliant under assumptions | Addition, subtraction, multiplication, division, powers, and differentiable functions use the correct sensitivity coefficients. |
| Correlation | Partially compliant | Shared sources and independence are represented, but PHS cannot declare an arbitrary covariance or correlation coefficient. |
| Type A and Type B evaluation | Not implemented | The evaluation method, observations, assigned distribution, instrument resolution, and degrees of freedom are not retained. |
| Expanded uncertainty | Not implemented | The main flow has no `k`, coverage probability, effective degrees of freedom, or Welch–Satterthwaite calculation. |
| Nonlinearity | Partial | Monte Carlo and sigma points exist, but there is no validated automatic method selection or general linearization-adequacy diagnostic. |
| Input distributions | Not compliant yet | A symmetric scalar input is built from a mean and standard deviation; rectangular, triangular, Student's t, lognormal, Poisson, and other distributions cannot be declared. |
| Asymmetric uncertainty | Partial core support | The core propagates variance and third moment using dimidiated and Fechner models, but the public PHS/Python workflow remains incomplete. |
| GUM reporting | Partial | Presentation rounding exists, but the uncertainty budget, measurand definition, method, `k`, coverage, and degrees of freedom are missing. |
| Cross-language parity | Incomplete | Rust, PHS, Python, WASM, and Java do not preserve all the same capabilities; Java loses uncertainty in some transcendental functions. |

The recommended project description is therefore:

> **Physure provides first-order GUM-compatible propagation of standard uncertainty with provenance tracking, together with an extensible framework for methods beyond GUM. Its nonlinear-distribution and asymmetric backends remain experimental, and a complete GUM-interoperable evaluation and reporting workflow is still missing.**

## 3. Current semantics of symmetric uncertainty

The expression:

```phs
4.5 +/- 0.05 cm
```

is converted into a quantity with mean `4.5 cm` and standard deviation `0.05 cm`. The value after `+/-` is not interpreted as a generic interval half-width, instrument tolerance, or expanded uncertainty: it is passed directly to `GaussianBackend::new(mean, std_dev)`.

Main paths:

- grammar: [`physure-script/src/phs.pest`](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/phs.pest);
- AST construction: [`physure-script/src/parser/quantities.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/parser/quantities.rs);
- quantity construction: [`physure-core/src/quantity.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/quantity.rs);
- default backend: [`physure-core/src/uncertainty/gaussian.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/uncertainty/gaussian.rs).

This behavior is compatible with GUM when `0.05 cm` is already the standard uncertainty `u(x)`. If it represents something else, it must be converted manually. Examples:

| Declared information | Required conversion before constructing the quantity |
|---|---|
| Rectangular half-width `a` | `u = a / sqrt(3)` |
| Triangular half-width `a` | `u = a / sqrt(6)` |
| Expanded uncertainty `U`, coverage factor `k` | `u = U / k` |
| Experimental standard deviation of the mean | Must be calculated from the observations and their degrees of freedom |

Physure does not retain which conversion was applied. Two quantities with the same `u` are indistinguishable even if one came from a Type A evaluation and the other from a Type B certificate.

### `sigma` does not declare the metrological type

The form `target +/- k sigma` creates a `SigmaBound` intended for comparisons. It does not declare that the second number is the quantity's standard deviation. Its implementation is in [`physure-script/src/interpreter/expressions.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/interpreter/expressions.rs) and [`physure-script/src/builtins/core.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/builtins/core.rs).

## 4. Implemented law of propagation

For a measurement model `y = f(x_1, ..., x_n)`, GUM uses, to first order:

```text
u²(y) = sum_i sum_j c_i c_j u(x_i, x_j)
c_i   = partial f / partial x_i
```

Physure represents each source with an identifier and a coefficient. When combining quantities, it applies the chain rule per source:

```text
coefficient_new(source) = J_a coefficient_a(source)
                        + J_b coefficient_b(source)

u(y) = sqrt(sum_source coefficient_new(source)²)
```

This reproduces the covariance-matrix result for dependencies expressible through shared sources. The effective implementation is in [`physure-core/src/uncertainty/lineage.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/uncertainty/lineage.rs), with backend dispatch in [`physure-core/src/uncertainty/trait_def.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/uncertainty/trait_def.rs).

### 4.1 Binary operations

Let `a` and `b` be estimates, `u_a` and `u_b` their standard uncertainties, and `u(a,b)` their covariance.

| Operation | Sensitivity coefficients | First-order combined variance |
|---|---|---|
| `a + b` | `(1, 1)` | `u_a² + u_b² + 2u(a,b)` |
| `a - b` | `(1, -1)` | `u_a² + u_b² - 2u(a,b)` |
| `a * b` | `(b, a)` | `b²u_a² + a²u_b² + 2ab u(a,b)` |
| `a / b` | `(1/b, -a/b²)` | `(u_a/b)² + (a u_b/b²)² - 2a u(a,b)/b³` |

For independent sources, `u(a,b) = 0`. For two expressions derived from the same source, the coefficients of that source combine algebraically. Therefore, `x - x` cancels and `x + x` has uncertainty `2u(x)`, not `sqrt(2)u(x)`.

The `uncorrelated` mode deliberately discards provenance and combines in quadrature. It is correct only when independence is a defensible assumption.

### 4.2 Powers and functions

| Model | Sensitivity used |
|---|---|
| `y = x^n` | `n x^(n-1)` |
| `sin(x)` | `cos(x)` |
| `cos(x)` | `-sin(x)` |
| `tan(x)` | `1 + tan²(x)` |
| `exp(x)` | `exp(x)` |
| `ln(x)` | `1/x` |
| `tanh(x)` | `1 - tanh²(x)` |

These expressions are consistent with GUM when the function is differentiable at the evaluation point and the linear approximation adequately represents the output distribution.

### 4.3 Unit conversions

A linear conversion `y = cx` scales uncertainty by `|c|`. For affine conversions, such as Celsius to kelvin, the offset changes the estimate but not its uncertainty. This behavior is metrologically correct.

## 5. Correlation and source traceability

`Lineage` retains identified contributions. The same source may reappear with different sensitivities and cancel or reinforce itself. This is more accurate than treating every occurrence as an independent measurement.

Limitations:

1. PHS has no syntax for `rho(x,y)` or an input covariance matrix.
2. A new source receives a new identifier; two physically correlated measurements do not become correlated merely because they share a name or value.
3. Mixed backends may lose or reduce provenance information. With `Moments`, operand order can still determine whether a mixed operation is accepted or rejected.
4. JCGM 102 requires retaining the covariance matrix between multiple outputs when they will be used together. Physure retains scalar provenance internally, but does not yet expose a complete GUM vector result with `U_y` as a reportable artifact.

## 6. Nonlinearity and alternative backends

### 6.1 Gaussian

This is the default backend and applies first-order linearization. The normal shape assumed by the constructor must not be confused with a universal GUM requirement: the law of propagation uses estimates, standard uncertainties, and covariances; the full distribution becomes especially important for nonlinearity and coverage.

### 6.2 Monte Carlo

The backend transforms samples operation by operation, which is conceptually compatible with JCGM 101. However:

- scalar quantities are initialized from a mean and standard deviation, currently using a normal distribution;
- there is no metrological API for assigning other PDFs to each input;
- JCGM 101-style convergence criteria, numerical tolerance, and coverage validation are missing;
- division may produce `NaN` or infinite samples without a structured metrological diagnostic;
- the result does not contain a reproducible record of seed, trial count, and configuration.

It should therefore be considered a **useful numerical backend**, not a complete JCGM 101 implementation.

### 6.3 Unscented

Unary functions transform sigma points, but several binary operations fall back to Jacobians and `Lineage`. The name `unscented` does not currently imply a complete multivariate unscented transform from input to output.

### 6.4 Method selection

Manual configuration through `propagation_mode` exists, but this audit found no general effective path that detects curvature and automatically switches from linear GUM propagation to Monte Carlo. Documentation claiming automatic switching should be treated as aspirational until code and validation demonstrate it.

## 7. `@jamontanac`'s asymmetric-uncertainty track

### 7.1 Origin and mathematical decision

[Issue #46](https://github.com/Alexisrx96/physure/issues/46), opened by `@jamontanac`, proposes not combining positive and negative half-widths independently. That rule artificially preserves asymmetry when many sources are added and conflicts with the expected trend toward symmetry for sums of independent contributions.

The proposal stores:

- the shift between the quoted value and the mean;
- variance;
- third central moment.

The `(sigma_minus, sigma_plus)` half-widths are converted to moments on input and reconstructed for reporting. Under a local transformation with sensitivity `J`:

```text
variance' = J² variance
third'    = J³ third
```

The cubic sign preserves reversal of asymmetry when the derivative is negative.

### 7.2 What PR #61 incorporated

[PR #61](https://github.com/Alexisrx96/physure/pull/61), merged as `a825ff3`, primarily incorporated:

- `MomentsBackend` with per-source provenance (`MomentLineage`);
- propagation of mean, variance, and third moment;
- separate shape strategies in [`physure-core/src/uncertainty/shapes.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/uncertainty/shapes.rs);
- the equal-area dimidiated distribution as the default;
- the equal-height Fechner distribution as an alternative;
- first-order addition, subtraction, multiplication, division, powers, and functions;
- explicit rejection of incompatible shapes or unrepresentable asymmetries;
- partial Python bindings for moments;
- tests for half-width recovery, skew reversal, and the tendency of sums toward symmetry.

Technical credit belongs to **Jose Alejandro Montaña (`@jamontanac`)**. The informal identifier `@jamontac` does not match the account listed as the issue and PR author.

### 7.3 Relationship to GUM

This work complements GUM but does not by itself establish GUM compliance:

- retaining variance and covariance preserves the central quantity required by the GUM law of propagation;
- retaining the third moment preserves information not required or fully described by first-order GUM;
- approximating a distribution with a dimidiated or Fechner family is a modeling decision that must be declared and validated;
- for non-Gaussian or strongly nonlinear results, JCGM 101 recommends propagation of distributions rather than only three moments;
- PDF errors and intervals derived from a likelihood are not interchangeable. Issue #46 correctly identifies that the kind must be declared when constructing the quantity.

### 7.4 What remains incomplete

1. **PHS:** accepts syntax such as `12.3 +/- (0.5, 0.4) pb`, but the interpreter and code generators reject it through `asymmetric_refusal()` to avoid silently losing the third moment.
2. **High-level Python:** the core and bindings expose `MomentsBackend`, but `uncertainty_model("moments")` is not yet a general workflow; some constructions raise `NotImplementedError`.
3. **Semantic API:** there is no stable distinction between a PDF error, likelihood error, coverage interval, and tolerance limit.
4. **Central value:** a coherent public API is still needed to distinguish the quoted value, mean, and median/mode after convolution shifts the center.
5. **Arrays and tensors:** there is no general storage for third moment and its dependence structure across NumPy, JAX, and PyTorch.
6. **Asymmetric correlation:** `MomentLineage` covers core sources, but there is no complete public interface for arbitrary matrices or cross-moments.
7. **Mixed backends:** a `Moments` quantity on the left may preserve skew while the reverse operation is rejected; availability is not commutative.
8. **Serialization and reporting:** a stable schema, evaluable PHS format, versioned JSON, and uniform `x +sigma_plus -sigma_minus` presentation are missing.
9. **Coverage:** reconstructed half-widths depend on the selected shape and must not automatically be presented as an interval with universal coverage probability.
10. **Reference validation:** reproducible cases against the examples and reference implementations from *Asymmetric Errors*, plus comparison with Monte Carlo, are still needed.

## 8. Concrete limitations and defects in the current code

### High priority

1. **Power at zero mean:** [`propagate_pow`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/uncertainty/trait_def.rs) returns zero uncertainty for every base with zero mean and a positive exponent. This is incorrect for exponent `1` and may be misleading when the first-order term vanishes but higher-order dispersion remains.
2. **Non-finite Jacobian:** [`Lineage::scale`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/uncertainty/lineage.rs) converts non-finite sensitivities into exact provenance. Cases such as `ln(0)` or singular derivatives can report zero uncertainty instead of a domain error.
3. **Unvalidated domains:** logarithms, fractional powers, tangent near poles, and Monte Carlo division require consistent validation.
4. **Loss in Java:** `sin`, `cos`, and `tan` in [`physure-java/src/main/java/com/physure/Quantity.java`](https://github.com/Alexisrx96/physure/blob/main/physure-java/src/main/java/com/physure/Quantity.java) discard uncertainty.

### Medium priority

5. **`abs` at zero:** sensitivity `1` is used even though the function is not differentiable there and the output distribution may be folded and asymmetric.
6. **Mixed backends:** the information exposed by `UncertaintyBackend` is reduced to mean and standard deviation; a foreign backend cannot observe the full third moment or provenance.
7. **Nonlinearity without diagnostics:** there is no quantitative indicator of first-order adequacy or automatic comparison with Monte Carlo.
8. **Monte Carlo reproducibility:** seed, sampling, convergence, and tolerance metadata are missing.

### Presentation and governance

9. **`+/-` notation:** GUM warns that standard and expanded uncertainty may be confused. Physure should explicitly label `u`, `U`, `k`, and coverage once the metrological model exists.
10. **Terminology:** sigma, tolerance, standard uncertainty, PDF error, and confidence interval must remain distinct concepts in both syntax and API.
11. **Historical documentation:** claims of “100% compliant” or automatic method selection should be replaced with verifiable states tied to executable tests.

### Rounding of reported values

Physure implements uncertainty-aware, GUM-style rounding for ordinary scalar display in
[`gum_round`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/quantity.rs). This is presentation logic: it does not reduce
the precision of the value or uncertainty stored by the calculation.

By default:

1. the standard uncertainty is rounded to one significant digit, or to two when rounding it
   to one digit produces a leading digit of `1`;
2. the reported value is rounded to the same decimal place as the rounded uncertainty; and
3. the unit is printed after the coordinated value-and-uncertainty pair.

For example, the hydraulic-pump model

```phs
pressure = 6.0 +/- 0.1 bar
flow = 120 +/- 3 L / min
efficiency = 0.82 +/- 0.02
input_power = pressure * flow / efficiency
input_power => kW
```

has an internal result of approximately `1.4634 kW` with a standard uncertainty of
`0.0566 kW`. Its default report is therefore:

```text
1.46 +/- 0.06 kW
```

The `@precision(N)` decorator overrides the default rule for an uncertain value: `N` selects
the number of significant digits retained in the uncertainty, and the value is still rounded
to the matching decimal place. For an exact value, the same decorator instead selects decimal
places. This difference must be stated in user-facing documentation because `precision` does
not have identical semantics for exact and uncertain quantities.

The PHS function `round(x, n)` is different from display rounding. It changes the stored mean
to `n` decimal places but preserves the existing uncertainty and unit. For example,
`round(9.81 +/- 0.05 m/s^2, 1)` retains a mean of `9.8` and a standard uncertainty of `0.05`,
which is displayed as `9.80 +/- 0.05 m/s^2`. It does not round the uncertainty distribution or
make the result exact.

This behavior covers coordinated reporting of a value and its already-evaluated standard
uncertainty. It does **not** model rounding, digitization, or instrument resolution as a new
uncertainty contribution. If a device has resolution increment `d` and a rectangular
quantization model is appropriate, a common Type B evaluation is

```text
u_resolution = d / sqrt(12)
```

That component must currently be declared and combined manually. Physure does not infer it
from the number of displayed decimals, `round()`, or `@precision`. A future metrological model
should represent the resolution, assumed distribution, divisor, source, and degrees of freedom
explicitly so that the contribution appears in the uncertainty budget.

The implementation is therefore compatible with the GUM reporting principle that the value
and uncertainty should be rounded consistently, but it is not yet a complete GUM reporting
workflow. The automatic one-or-two-digit policy is a project convention within the broader
GUM guidance, not the only rounding policy permitted by GUM.

## 9. Missing GUM elements

To produce a defensible result under GUM, Physure must represent at least:

### 9.1 Input metrological model

- measurand identifier and definition;
- estimate and unit;
- uncertainty component;
- Type A or Type B evaluation;
- assigned distribution and its parameters;
- derived standard uncertainty;
- degrees of freedom;
- documentary or experimental source;
- applied correction and its uncertainty;
- correlations or covariances with other inputs.

### 9.2 Combination and method adequacy

- uncertainty budget with sensitivity coefficients;
- explicit covariance matrix or an equivalent exportable representation;
- verification of linearity and approximation validity;
- support for higher-order terms where appropriate;
- explicit selection between linear GUM and propagation of distributions;
- convergence and validation for Monte Carlo.

### 9.3 Expanded uncertainty and reporting

- effective degrees of freedom using Welch–Satterthwaite where applicable;
- coverage factor `k`;
- coverage probability;
- coverage interval, including asymmetric intervals;
- rounding rules for value and uncertainty;
- reporting of method, assumptions, correlations, and dominant contributions;
- reproducible serialization of the budget and result.

## 10. Recommended roadmap

The roadmap uses GUM as a **minimum interoperability contract**, not a scientific ceiling. The internal model should allow a quantity to retain information richer than `(value, standard deviation)` and project it onto that view only when a consumer requests a GUM report.

### Phase 1 — correct the existing core

1. Fix powers at zero mean and non-finite sensitivities.
2. Unify domain errors across Gaussian, Moments, Monte Carlo, and Unscented.
3. Fix uncertainty loss in Java.
4. Add cross-language parity tests for every operation.

### Phase 2 — minimum metrological model

Introduce an immutable structure associated with each source, without inflating every derived result with duplicated text:

```text
UncertaintyComponent
  evaluation: type_a | type_b
  distribution: normal | rectangular | triangular | student_t | custom
  declared_parameter
  standard_uncertainty
  degrees_of_freedom
  source_id
  reference
```

Arithmetic should continue to operate on compact coefficients; metadata lives once per source and is consulted for budgets and reports.

### Phase 3 — correlation and coverage

1. API for covariance, correlation, and input matrices.
2. Welch–Satterthwaite and coverage factors.
3. Distinct `standard` and `expanded` results without ambiguously overloading `+/-`.
4. Exportable uncertainty budget.

### Phase 4 — complete the asymmetric track

1. Define `pdf_error` versus `likelihood_error`.
2. Complete construction, propagation, serialization, and presentation in Python and PHS.
3. Clearly expose quoted value, mean, and median/mode.
4. Add arrays and third-order dependence only when a validated representation and use case exist.
5. Validate Moments against Barlow et al.'s references and Monte Carlo.

### Phase 5 — normative validation

1. Reproduce examples from the annexes of JCGM 100.
2. Reproduce validation cases from JCGM 101 and JCGM 102.
3. Publish tolerances, seeds, and reference artifacts.
4. Generate a compliance matrix from executable tests rather than manual declarations.

### Phase 6 — extensions beyond GUM

1. Define a common interface for uncertainty methods with declared capabilities, without forcing every distribution to look Gaussian.
2. Retain rich results—samples, quantiles, moments, intervals, or sets—alongside their GUM projection where mathematically valid.
3. Allow domain-specific models and future methods without modifying `Quantity` for every case.
4. Require every backend to declare what it preserves: mean, variance, covariance, skew, tails, coverage, dependence, and traceability.
5. Compare methods using reference cases rather than selecting linear GUM, Monte Carlo, or another method by name alone.

## 11. Proposed criteria for compliance claims

Physure should not claim “GUM compliant” without qualification, nor use that label to exclude more general methods. Recommended levels are:

| Level | Requirement |
|---|---|
| **GUM-LPU compatible** | Correct standard uncertainties, sensitivity coefficients, and covariances under the first-order approximation. |
| **GUM reporting capable** | Additionally: budget, degrees of freedom, `k`, coverage, rounding, and measurand metadata. |
| **JCGM 101 validated** | Explicit input distributions, convergent Monte Carlo, and published validation cases. |
| **JCGM 102 validated** | Verified multiple outputs and output covariance matrix. |
| **Asymmetric experimental** | Asymmetric moments and families are available without claiming universal coverage. |
| **Extended uncertainty model** | A method beyond GUM with documented assumptions, capabilities, validation, and GUM projection where applicable. |

In the audited state, Physure primarily reaches **GUM-LPU compatible** for the scalar Gaussian backend in Rust/PHS/Python, subject to the domain defects identified above. Asymmetric support should be labeled **experimental**.

## 12. References

1. JCGM, [JCGM 100:2008 — Guide to the Expression of Uncertainty in Measurement](https://www.bipm.org/doi/10.59161/JCGM100-2008E).
2. JCGM, [JCGM 101:2008 — Propagation of distributions using a Monte Carlo method](https://www.bipm.org/doi/10.59161/JCGM101-2008).
3. JCGM, [JCGM 102:2011 — Extension to any number of output quantities](https://www.bipm.org/doi/10.59161/JCGM102-2011).
4. Roger Barlow, Alessandra R. Brazzale, and Igor Volobouev, [*Asymmetric Errors*](https://arxiv.org/abs/2411.15499).
5. `@jamontanac`, [Proposal: support asymmetric uncertainties — issue #46](https://github.com/Alexisrx96/physure/issues/46).
6. `@jamontanac`, [Asymmetric moment propagation — PR #61](https://github.com/Alexisrx96/physure/pull/61).
