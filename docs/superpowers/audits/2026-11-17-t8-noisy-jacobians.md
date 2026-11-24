# T8: does a Jacobian full of rounding noise make the covariance store dense and slow?

**Verdict: no code change. T8 is closed as not needed.** Measured 2026-11-17 on the branch `docs/post-release-0.4.0`, release build, one variable of size `n` with a diagonal variance. The audit row (`2026-11-14-code-resident-numbers.md`, T8) said "not measured".

## What the noise costs, when it is present

`propagate` of an `n×n` Jacobian. The noise cases are a band of width 3 plus entries of 1e-17..1e-15 of the band at every other position. "Chain" is ten propagates, each one fed by the last.

| n | Jacobian | one propagate | chain of 10 | stored entries (one / chain) |
|---|---|---|---|---|
| 1000 | band | 0.0009 s | 0.061 s | 9 k / 1.4 M |
| 1000 | band + noise | 0.29 s | 20.6 s | 2.0 M / 65 M |
| 4000 | band | 0.013 s | 0.55 s | 36 k / 5.5 M |
| 4000 | band + noise | 26.0 s | not run | 32 M (512 MB) |
| 4000 | dense, exact | 25.1 s | not run | 32 M (512 MB) |

Noise costs exactly what a dense Jacobian costs: the time grows as `n³` (the sparse sandwich of an effectively dense matrix). About 2000 times slower and 700 times more memory at `n = 4000`.

## Why it is not worth a fix

1. **Nothing in the real API sends a noisy Jacobian to the store.** The only in-tree caller of `_core.propagate` is the Python `CovarianceStore.update_from_propagation`. Its Jacobians are built structurally and analytically (`identity_operator`, `diagonal_operator`, `broadcast_operator`): exact diagonals, never noise. The only source of noise is `Uncertainty.propagate` (jax / torch autograd; measured at `n = 300`: `inv(A)@(A@x)` gave 84,578 of 90,000 entries non-zero, `fft`→`ifft` 88,463), and nothing in `physure/` calls it. The Rust core, PHS, WebAssembly, Java and the CLI never call `propagate`. Every real chain through the Python API (`x*x`, `x/3.1*3.1 - x`, `np.sin`, a ten-step chain) stayed exactly diagonal at `n = 4000`.
2. **The rule the audit proposed does not work.** "Drop `J[i][j]` when `|J[i][j]| <= eps * max_j |J[i][j]|`":
   - with `eps = f64::EPSILON` it keeps 72 % of the entries at `n = 4000` (the noise is above 2.2e-16), so it does not sparsify;
   - with `eps = 1e-12` it restores the band (nnz 36 k, 0.03 s) but ignores the input variances: a row `[1, 1e-13]` with σ = `[1, 1e13]` has two equal contributions, the rule drops one, and the standard uncertainty goes from 1.414 to 1.000, **29 % too low**. That is a silently dropped uncertainty, the same family as audit B14.

## If T8 is ever reopened

The rule has to weigh each entry by its input's standard deviation: drop `J_ij` when `|J_ij|·σ_j <= eps · max_k |J_ik|·σ_k`, the maximum over all the inputs of the row, with `σ_j = sqrt(Σ_jj)` read from the store and `eps ≈ 1e-12`. It is unit-invariant (a change of input unit scales `J_ij` and `σ_j` inversely, a change of output unit scales the row), still drops exact zeros, keeps every entry when the row maximum is NaN or infinite, and bounds the change by about `n·eps` relative. The B14 tests in `physure-core/tests/covariance_relative_pruning.rs` pass under it; new tests would be the σ-weighted counterexample above and a noisy `n = 1000` band. The trigger would be a caller that feeds the store a computed Jacobian (for example `Uncertainty.propagate` getting users).

## Found on the way (not T8)

- **`np.mean(x)` and `x.dot(x)` return an uncertainty of 0.0 without a warning** (`x = [1, 2, 3] ± 0.1 m`: `np.mean(x)` is `2.0 m` with uncertainty `0.0`, where the answer is 0.1/√3 ≈ 0.058 m; `x.dot(x)` is `14.0 m²` with uncertainty `0.0`). Confirmed by running it. `cross` is the same (`_backend_mixin.py` `__array_function__`; `quantity.py` "Uncertainty ignored for now"). A wrong answer with confident units. `np.sum`, `x.sum()`, `@` and `np.dot` raise instead, which is loud and fine.
- The Python path densifies each diagonal Jacobian to `n×n` before the FFI call: 128 MB per operand at `n = 4000`, and about 0.3 s per elementwise operation. This, not noise, is the real cost of the path today.
- `AutogradPropagator._compute_finite_diff` (`autograd.py`) perturbs every element at once, so for a function that is not elementwise it returns a wrong, diagonal-only Jacobian. The path is unused.
