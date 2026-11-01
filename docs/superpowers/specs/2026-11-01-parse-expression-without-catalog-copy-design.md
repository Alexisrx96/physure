# Parsing a unit without copying the catalog — design

**Date:** 2026-11-01
**Status:** approved by the maintainer ("Do it", 2026-11-01), after the maintainer chose "A first, then D" and "same semantics, no copy".
**Base:** local `main` @ `7786740` (PR #71)
**Origin:** a ranked list of three bottlenecks from the maintainer. Each was checked against the code and a profile before design; only the first part of the first is built here (see "What was checked").

## What was checked

| Claim | Finding |
|---|---|
| The catalog is cloned in `Parser::parse_expression` (218 µs) | **True.** With no `units::with_catalog` scope, `parse_expression` calls `conf::build_registry_from_conf()`, which clones the embedded registry (parsed once, in a `OnceLock`) and `stat`s `./physure.conf` on every call. `Quantity::new("m/s")` goes through the same path. |
| A `LazyLock` makes the first `Q_()` (222 ms) almost instant | **No.** The embedded catalog is already lazy. A profile of the first `Q_(1.0, "m")` puts nearly all of it in Python module imports and in the system build in `application/startup.py`. That is piece B, not this one. |
| Eager overhead: 8.7x on 10^6-element tensors, ~19 µs on scalars | Scalars reproduce (15–18 µs per `+ - * /`). On tensors the extra 1.4 ms at 10^6 elements is ~1.4 ns per element: work proportional to N, not a dispatch cost, so a `#[pyclass]` operator would not touch it. Piece C, after profiling. |
| Welch–Satterthwaite is the one metrological gap | **True and unbuilt** (`docs/scientific_gaps_and_rigor.md` §1.1). Piece D, next. |

## Decision

| Question | Decision | Rejected |
|---|---|---|
| What happens to a `physure.conf` in the working directory | **Same semantics, no copy.** It is looked for on each call. No file: the embedded catalog is read by reference. A file: the merged registry is built as today. | A cache keyed by the file's `(path, mtime, size)` (global state to reset); reading it once per process (a file created or edited after first use would be ignored, and anything that changes directory would see a frozen catalog) |

## Design

`units::conf` gets one function:

```rust
pub fn with_conf_registry<R>(f: impl FnOnce(&UnitRegistry) -> R) -> R
```

- If `./physure.conf` is a file, it calls `build_registry_from_conf()` and gives `f` the merged registry it returns (the rare path, exactly as before).
- Otherwise it gives `f` `&embedded_catalog().0`: no copy.

`Parser::parse_expression` uses it in the arm where there is no catalog scope. `build_registry_from_conf()` stays, for the callers that need an owned registry. No cache, no global state beyond the existing `OnceLock`, nothing for the `clean_state` fixture to reset.

Callers that build a registry once per program (`phs check`, `report`, `inspect`) are left alone: they pay the copy once, not once per operation.

## Limit

The `stat` of `./physure.conf` stays, because it is what keeps the override live. If it turns out to be most of the time that is left, that is reported and the semantics are discussed before they change.

## Verification

- A test that two calls to `with_conf_registry`, with no `physure.conf` in the working directory, see the **same** `&UnitRegistry`: zero copy, proved by address and not by a timing.
- The existing unit-parser tests, unchanged.
- An integration test in its own binary (changing the working directory is process-wide, so it cannot share a binary with parallel tests): a `physure.conf` in the working directory that redefines a unit still takes effect, and goes away when the file does.
- `cargo bench -p physure --bench core_benchmarks -- parse_` before and after; the two rows of `docs/benchmarks.md` (§5) take the new figures. A target of 20 µs for `kg*m/s^2` is a target, not a promise.

## Not in this piece

The first `Q_()` (B), the eager overhead (C), and Welch–Satterthwaite (D, designed next).

## Result

Built as designed on `perf/parse-expression-no-copy`. Criterion (`--warm-up-time 1 --measurement-time 3`, one machine, one session):

| Benchmark | Before | After | Change |
|---|---|---|---|
| `parse_compound_unit` (`kg*m/s^2`) | 130.1 µs | 6.09 µs | -95.3% (21x) |
| `parse_prefixed_compound_unit` (`km/h`) | 128.5 µs | 6.12 µs | -95.4% (21x) |

The 20 µs target is met with a margin of 3x, and the `stat` of `./physure.conf` that stays is part of the 6 µs, so the semantics did not have to be reopened. `cargo test` for `physure`, `physure-script`, `physure-cli` and `physure-lsp` is unchanged.
