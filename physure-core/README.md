<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/Alexisrx96/physure/main/assets/logo-horizontal-dark.svg">
  <img src="https://raw.githubusercontent.com/Alexisrx96/physure/main/assets/logo-horizontal-light.svg" alt="physure" width="380">
</picture>

# physure 🦀

[![crates.io](https://img.shields.io/crates/v/physure?color=F59E0B&labelColor=18181A)](https://crates.io/crates/physure)
[![docs.rs](https://img.shields.io/docsrs/physure?labelColor=18181A)](https://docs.rs/physure)
[![CI](https://img.shields.io/github/actions/workflow/status/Alexisrx96/physure/tests.yml?branch=main&labelColor=18181A)](https://github.com/Alexisrx96/physure/actions/workflows/tests.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-F59E0B?labelColor=18181A)](https://github.com/Alexisrx96/physure/blob/main/LICENSE)

**Pure Rust physics engine: dimensional analysis with rational exponents, physical quantities, and uncertainty propagation.**

This crate is the core of the [physure](https://github.com/Alexisrx96/physure) project. It contains all the math and none of the FFI — no PyO3, no wasm-bindgen, no JNI. The [`physure` Python package](https://pypi.org/project/physure/) statically links this crate; future WASM/Java wrappers will do the same.

## Features

- **Exact dimensional analysis** — `RationalUnit` tracks dimension exponents as rationals, so `sqrt(m²)` is exactly `m`, never `m^0.9999`.
- **Uncertainty propagation** — Gaussian (first-order), Monte Carlo, and Unscented Transform backends, GUM-style.
- **Sparse covariance store** — track correlations between thousands of quantities with flat memory.
- **Symbolic expressions** — a small compiled expression engine (stack-evaluated, ~14 ns/eval).
- **Arrow IPC serialization** — cross-language data interchange.

## Usage

```toml
[dependencies]
physure = "0.2"
```

```rust
use physure::{Quantity, RationalUnit};

let metre = RationalUnit::new_from_dimensions([("m".to_string(), (1, 1))]);
let second = RationalUnit::new_from_dimensions([("s".to_string(), (1, 1))]);

// 10.0 ± 0.1 m  /  2.0 ± 0.05 s  ->  5 m/s with propagated uncertainty
let d = Quantity::new_scalar(10.0, 0.1, metre.clone(), None, None);
let t = Quantity::new_scalar(2.0, 0.05, second, None, None);
let v = d.div(&t).expect("compatible dimensions");
assert_eq!(v.value.mean(), 5.0);

// Dimension mismatches are errors, not silent wrong answers
assert!(d.add(&v).is_err());

// Rational exponents stay exact: sqrt(m^2) == m
let side = metre.mul(&metre).pow(num_rational::Rational64::new(1, 2));
assert_eq!(side, metre);
```

Run the full tour with `cargo run --example quickstart`.

## Performance

Criterion micro-benchmarks (see [BENCHMARKS.md](https://github.com/Alexisrx96/physure/blob/main/BENCHMARKS.md)):

| Operation | Time |
|---|---|
| Unit multiply / divide | ~54 ns |
| Scalar add with dimension check | ~40 ns |
| Compiled symbolic eval | ~14 ns |
| Sparse covariance propagation | ~7 µs |

```bash
cargo bench
```

## Development

```bash
git clone https://github.com/Alexisrx96/physure
cd physure
cargo test -p physure
```

The crate lives in the `physure-core/` directory of the workspace; the directory name is historical, the package name is `physure`.

## License

[MIT](https://github.com/Alexisrx96/physure/blob/main/LICENSE) — Irvin Torres
