# Physure Performance Benchmarks

Performance measurements of **`physure`** 0.3.0 (the unit and uncertainty engine lives in Rust, `physure-core`). This is the only benchmarks page in the repository: every table says which script measured it and how to reproduce it.

---

## 💻 Measurement environment

| | |
| :--- | :--- |
| **Date** | 2026-10-30 |
| **Commit** | `efac063` (after 0.3.0 was published) |
| **Machine** | Intel Core i5-13420H (8 cores, 12 threads), 15.7 GB RAM, laptop **in use** |
| **OS** | Windows 11 (10.0.26300) |
| **Python** | CPython 3.14.6 |
| **Rust** | `rustc 1.97.1`, `release` profile (`lto = true`, `codegen-units = 1`) |
| **PyTorch** | 2.13.0, CPU only (no CUDA and no C++ compiler, so `torch.compile` with `inductor` could not be measured) |

**How to read the numbers.** The machine was not isolated: the same row of section 6 went from 11 to 15 µs between two batches an hour apart, without a line of code changing. So each table gives the **minimum** of several repetitions (what bursts of background noise cannot make worse) and, where it matters, the **median**. Compare values within the same table, or repeat the measurement on your machine; do not take an absolute value as a guarantee. The ratios between versions on this page were measured by alternating both versions in the same session.

---

## ⚡ 1. Import and first use

The `physure` 0.3.0 wheel from PyPI, measured from a neutral directory (not from `physure-python/`, which contains the project's package and would be imported instead).

| Metric | Value | How |
| :--- | :--- | :--- |
| Interpreter start (`python -c pass`) | **49.6 ms** (min 44.8) | mean of 30 processes |
| Cold `python -c "import physure"` | **68.5 ms ± 5.4** (min 61.4) | mean of 30 processes; the difference from the start is **≈ 19 ms** (≈ 17 ms with the minimums) |
| `import physure` inside a process | **5.8 ms** median (min 4.9) | 6 rounds |
| First `Q_(1.0, "m")` after the import | **222 ms** median (min 204) | builds the default unit system (the whole catalog) |

Against 0.2.4 in the same session: in-process import 5.8 ms (min 5.4) and first `Q_` 241 ms (min 227). 0.3.0 is not slower on either.

---

## 📈 2. Lineage growth

`benchmarks/benchmark_lineage.py`: consecutive additions of quantities with uncertainty. Minimum \[median\] of 7 runs.

| Operations | Correlated mode | Uncorrelated mode |
| :---: | :---: | :---: |
| **1,000 additions** | **46 ms** \[58 ms\] | **44 ms** \[69 ms\] |
| **5,000 additions** | **242 ms** \[327 ms\] | **205 ms** \[267 ms\] |

---

## 🧮 3. Vectorized uncertainty propagation

`benchmarks/benchmark_propagation.py`: arrays of $N$ elements. Minimum \[median\] of 7 runs.

| $N$ | Correlated mode | Uncorrelated mode | Memory increase (correlated / uncorrelated) |
| :---: | :---: | :---: | :---: |
| **100** | **3 ms** \[5 ms\] | **1 ms** \[1 ms\] | +79.6 MB / 0 MB (SciPy sparse is loaded the first time) |
| **1,000** | **26 ms** \[34 ms\] | **27 ms** \[35 ms\] | +0.1 MB / +0.2 MB |
| **5,000** | **0.66 s** \[0.80 s\] | **0.66 s** \[0.83 s\] | +2.2 MB / +1.1 MB |

The script measures each configuration once per process; the maximum of the 7 runs reached 2.4 s at $N = 5{,}000$, which is why the minimum is given.

---

## 🔥 4. Tensors (PyTorch, CPU)

`benchmarks/benchmark_comparison.py`, parts 1 and 2: addition of two tensors of $10^6$ elements, 100 iterations, median of 9 batches.

| Execution mode | Time per iteration | Versus pure PyTorch |
| :--- | :---: | :---: |
| **Pure PyTorch** | **0.180 ms** | $1.00\times$ |
| **Physure, eager mode** (`q1 + q2`) | **1.567 ms** | $8.7\times$ |
| **Physure `@torch.compile`** | not measured | needs a C++ compiler (MSVC) for `inductor` |

The same experiment with the published wheels, alternating versions in the same session (6 rounds, best of each): 0.2.4 → 1.51 ms and 0.3.0 → 1.46 ms, with a 0.15 ms baseline in both. The cost of eager mode **did not change with A1**.

Earlier versions of this page gave $3.1\times$ for eager mode and $1.2\times$ for `@torch.compile`, measured on another machine (the page said Linux x86_64). They do not reproduce here: on CPU, eager mode costs about 1.4 ms more than the bare addition for $10^6$ elements, and that is an open line of work, not a result of this version.

---

## 🦀 5. Rust micro-benchmarks (`physure-core`, Criterion)

`cargo bench -p physure --bench core_benchmarks`. Measured without the FFI bridge; central time of the Criterion interval.

| Micro-benchmark | Time | What it measures |
| :--- | :---: | :--- |
| **`unit_mul`** | **170.9 ns** | Product of two `RationalUnit` values (exponents and exact scale). |
| **`unit_div`** | **167.1 ns** | Quotient of two `RationalUnit` values. |
| **`unit_mul_prefixed`** | **200.2 ns** | Product with a prefixed unit. |
| **`unit_div_prefixed`** | **294.8 ns** | Quotient with a prefixed unit. |
| **`quantity_convert_prefixed`** | **390.3 ns** | Conversion of a quantity with a prefixed unit (`convert_to`). |
| **`quantity_add_scalar`** | **175.7 ns** | Scalar addition with dimensional validation. |
| **`quantity_mul_scalar`** | **194.7 ns** | Scalar product with dimensional accumulation. |
| **`covariance_propagate`** | **9.48 µs** | Sparse covariance propagation by blocks. |
| **`parse_compound_unit`** | **218 µs** | Reading `kg*m/s^2` with `Parser::parse_expression`: every call copies the catalog registry. |
| **`parse_prefixed_compound_unit`** | **185 µs** | Reading `km/h`, the same. |

The previous version of this table included `compiled_symbolic_eval`; that benchmark no longer exists (the symbolic benchmark was removed when the symbolic engine moved to `physure-script`), and the prefixed-unit, conversion and parsing ones are new. The old figures in this section are not comparable with these: they came from another machine and from before `RationalUnit` carried exact rational scales.

---

## 🧪 6. Unit and quantity operations in Python

`benchmarks/bench_unit_ops.py`, the table that the A1 quality gates use (no row may be more than 10 % slower than the previous version, measured by alternating both in the same session). Minimum \[median\] of 8 runs, in nanoseconds.

| Operation | Minimum \[median\] |
| :--- | :---: |
| `Q_(5.0, 'm')` (unit as text) | 15,379 \[15,853\] |
| `Q_(5.0, units.m)` | 14,979 \[15,905\] |
| `5 * units.m` | 15,391 \[16,472\] |
| `units.m * units.s` | 909 \[956\] |
| `units.m / units.s` | 905 \[1,021\] |
| `units.m ** 2` | 674 \[710\] |
| `q1 + q2` | 19,341 \[21,629\] |
| `q1 * q2` | 20,723 \[21,542\] |
| `q1.to('km')` | 22,645 \[24,171\] |
| `q_speed.to('km/h')` | 25,159 \[27,309\] |
| `array(1000).to('km')` | 35,055 \[44,221\] |

With the machine less busy these same rows came out between 20 % and 30 % lower (`Q_` 11.3 µs, `q1 + q2` 15 µs): that is the noise described above.

---

## 🚀 Reproducing the benchmarks

```bash
cd physure/physure-python

# build the extension in release mode
uv run maturin develop --release

# sections 2, 3 and 4 (each script measures once: repeat them and keep the minimum)
uv run --no-sync python benchmarks/benchmark_lineage.py
uv run --no-sync python benchmarks/benchmark_propagation.py
uv run --no-sync python benchmarks/benchmark_comparison.py   # needs a C++ compiler for the @torch.compile part

# section 6
uv run --no-sync python benchmarks/bench_unit_ops.py results.json

# section 5 (from the repository root)
cargo bench -p physure --bench core_benchmarks
```

For section 1, always measure from a directory that does not contain the `physure` package (the `physure-python/` directory contains it, and `python -c` would import it instead of the installed wheel).
