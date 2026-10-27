# A1 Stage 0 — divergence report

**Date:** 2026-10-27
**Measured on:** local `main` after the repo-hygiene work (Python and Rust share one catalog)
**Tests:** `physure-python/tests/core/test_unit_algebra_parity.py`, golden file `physure-python/tests/core/golden/unit_semantics.json`
**Design:** `2026-10-27-a1-rust-owned-unit-algebra-design.md`

## Method

Rust is the oracle. For every name in the generated catalog that Python can resolve, the test converts 1.0 and 25.0 of that unit to two reference units of the same Python dimension, once with Python (`Q_(v, a).to(b)`) and once with the PHS `=>` operator running inside `physure-core` (`physure._core.evaluate_phs_native`). Magnitudes are compared with `==`. The references of a dimension are its two shortest symbols **that Rust knows**: one reference is not enough, because a dimension can hold several kinds of quantity (angles and solid angle; ratios and logarithmic units), and a reference of one kind hides the others. With a single reference (`dB`) the whole dimensionless group, `percent` and `ppm` included, was silently not compared.

## Scale

| | |
|---|---|
| Names in the generated catalog index | 6158 |
| Names Python itself cannot resolve | 3720 |
| Dimension groups | 34 |
| Differences | 8877 |

## The five classes of difference

| Class | Comparisons | What happens | Resolved by |
|---|---|---|---|
| `rust-rejects-long-prefix-name` | 8732 (2183 symbols) | Python accepts a long prefix name with a long unit name (`kilometer`, `attoampere`). Rust only accepts a symbol prefix with a symbol (`km`, `kA`) and answers "Unknown unit". | Stage 1: the core must accept the prefix names the catalog already defines. |
| `last-bit-difference` | 87 (46 symbols, among them `acre`, `gal`/`gallon`, `psi`, `lb`/`pound`, `oz`, `inch`, `eV`, `BTU`, `coulomb`, `degC`, `degF`, `celsius`, `fahrenheit`, `ppm`, `rad`) | The results differ by a few units in the last place. Python converts with exact `Fraction` scales and a single rounding; Rust stores scales as `f64`. One compound conversion shows it too: `g/cm^3` to `kg/m^3` is `1000.0` in Python and `999.9999999999999` in Rust. A direct example: `1 ppm => percent` is `0.0001` in Python and `9.999999999999999e-05` in Rust. | Stage 1: exact rational scales in the core. |
| `dimensionless-kinds-merged-by-python` | 38 (19 symbols) | Python files angles (`rad`, `deg`, `arcmin`, `arcsec`), solid angle (`sr`), plain ratios (`percent`, `ppm`, `unity`) and `pH` under one dimension, so it converts `arcmin` to `sr`. Rust answers "Unit mismatch". | Open question for Stage 1: Rust's stricter behaviour looks right; Python's looks like a bug. |
| `rust-rejects-delta-temperature` | 12 (3 symbols: `delta_celsius`, `degC_delta`, `delta_degC`) | Python converts a temperature difference in Celsius to kelvin one to one. Rust says "Unit mismatch". | Stage 1: decide and implement the rule in the core. |
| `rust-does-not-know-unit` | 8 (2 symbols: `dB`, `decibel`) | The PHS registry does not know the logarithmic units that the catalog and Python define. | Stage 1: register the logarithmic units in the core. |

A sixth finding is not a difference between the two sides but a defect of the generated catalog: 3720 of its 6158 names (for example `attoA`, `attoBTU`) are a long prefix name glued to a unit symbol, and Python itself raises `UnknownUnitError` for them. Stage 2 removes the generated catalog.

## What agrees

- Every unit that both sides resolve converts identically, bit for bit, except the 46 symbols above.
- 16 of 17 representative compound conversions agree exactly (`km/h`, `kg*m/s^2`, `J/s`, `kWh`, `psi`, `mmHg`, `L/min`, `eV`, `cal`, `BTU/h`, `hp`, `degC`, `degF`, `mol/L`, `ft*lbf`, `N*m`).
- Both sides reject conversions between incompatible dimensions (33 cross-dimension pairs checked).

## What Rust cannot answer yet

`kind` (absolute or delta), `is_linear`, `simplify` and alias formatting have no Rust counterpart. Their current Python results for 32 units and compounds are frozen in the golden file, which becomes the specification for Stage 1.

## How to use this

Each class has a strict `xfail` test (or a recorded set, for the last-bit differences). When Stage 1 or 2 closes a gap, the matching test turns into an unexpected pass, which fails the suite until its `xfail` marker is removed. `KNOWN_LAST_BIT` and `KNOWN_COMPOUND_LAST_BIT` become empty when the core has exact scales.
