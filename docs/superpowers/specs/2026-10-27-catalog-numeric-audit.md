# Numeric audit of `physure.conf`

**Date:** 2026-10-27. **File audited:** `physure-core/src/units/physure.conf` (the only catalog: Python, PHS and the Rust API all read it). **Scripts:** `catalog-audit/audit_units_and_constants.py` and `catalog-audit/audit_identities.py` in this folder; both re-run the numbers below.

## Why this audit exists

While deciding what the library should do with `arcmin → sr`, a check of the Rust oracle showed `1 deg => arcmin` returning `60.000043` instead of `60`. The project's first rule is that a wrong answer with confident units is worse than an exception, so the whole numeric content of the catalog was checked against the definition each value comes from.

## Result in one paragraph

**22 of the 69 numeric factors in `[Units]` that could be checked are off by more than a couple of units in the last place: 15 are truncated to 5 to 9 significant digits (worst factor: parsec, 7 × 10⁻⁶), 5 come from an older CODATA edition and 2 carry float noise. 189 of the 411 entries of `[Constants]` are cut to 7 significant digits (worst: 4.8 × 10⁻⁷) by a one-token bug in the generator. The file mixes two CODATA editions. Fifteen of 27 identities that must hold exactly (`1 deg = 60 arcmin`, `1 atm = 760 Torr`, `1 gal = 231 in³`, …) fail by 10⁻⁹ to 10⁻⁵ in the Rust core, and therefore in Python and PHS.** The coherent SI units (factor 1.0) and the exact imperial lengths and masses are correct.

## Method

- `[Units]`: each factor was compared, at 50 digits (`mpmath`), with the quantity it is defined as. Exact definitions (international inch, pound, `g₀`, US gallon = 231 in³, `π/180`, …) come from NIST SP 811 and the SI brochure; measured quantities come from `scipy.constants` (CODATA 2022). 69 entries were checked; the other 27 are coherent units whose factor 1.0 is correct by construction. A relative error above 4 × 10⁻¹⁶ (a couple of ulps of an `f64`) is reported.
- `[Constants]`: each entry was compared with `scipy.constants.physical_constants` through the generator's own name cleaning.
- Identities: 27 conversions whose exact result is known, evaluated by `physure._core.evaluate_phs_native` (the Rust core, which is what the Python package reads too).
- **Not covered:** dimensions of each unit, prefix handling, the logarithmic units, `[Categories]`, and the correctness of each alias.

## A. `[Units]` factors that are wrong

| Unit | In the catalog | Exact value | Relative error | Definition |
|---|---|---|---|---|
| parsec | 3.0857e16 | 3.085677581491367e16 | 7.3e-06 | 648000/π au; au = 149 597 870 700 m |
| btu | 1055.06 | 1055.05585262 | 3.9e-06 | BTU_IT (see D) |
| light_year | 9.4607e15 | 9460730472580800 | 3.2e-06 | c × Julian year |
| mmHg | 133.322 | 133.322387415 | 2.9e-06 | conventional mmHg |
| torr | 133.322 | 133.3223684210526 | 2.8e-06 | atm/760 |
| fluid_ounce | 2.95735e-5 | 2.95735295625e-5 | 1.0e-06 | gallon/128 |
| arcminute | 0.000290888 | 0.0002908882086657216 | 7.2e-07 | π/10800 |
| gallon_us | 0.00378541 | 0.003785411784 | 4.7e-07 | 231 in³ |
| slug | 14.5939 | 14.59390293720636 | 2.0e-07 | lbf·s²/ft |
| acre | 4046.856 | 4046.8564224 | 1.0e-07 | 43 560 ft² (international) |
| pound_force | 4.448222 | 4.4482216152605 | 8.7e-08 | lb × g₀ |
| psi | 6894.757 | 6894.757293168361 | 4.3e-08 | lbf/in² |
| arcsecond | 4.8481368e-6 | 4.84813681109536e-6 | 2.3e-09 | π/648000 |
| degree | 0.0174532925 | 0.0174532925199433 | 1.1e-09 | π/180 |
| horsepower | 745.699872 | 745.6998715822702 | 5.6e-10 | 550 ft·lbf/s |
| franklin, debye | …15044e-10, …15044e-30 | …1520e-10, …1520e-30 | 4.8e-15 | 1/(10c) C and 10⁻²¹/c C·m; about 20 ulps of float noise |

Edition drift (the same quantities, CODATA 2018 in `[Units]` and 2022 in scipy): `electron_mass` 1.4e-09, `bohr` 6.8e-10, `atomic_time` 2.9e-13, `hartree` 2.5e-13, `planck_charge` 4.4e-08 (see E).

Correct to the last bit: inch, foot, yard, mile, nautical mile, pound, ounce, tonne, minute, hour, day, year (Julian), hectare, liter, dyne, kilopond, erg, calorie (thermochemical), electron-volt, watt-hour, bar, atmosphere, the CGS units, `percent`, `ppm`, the temperature scales (5/9 as the nearest `f64`, zero point of °F to 9 × 10⁻¹⁷).

## B. `[Constants]`: cut to 7 digits by the generator

`physure-python/scripts/generate_constants_config.py` writes every constant with `abs(value) < 1e-4 or abs(value) > 1e4` as `f"{val:e}"`. Python's `e` format has six decimals by default, so those values keep **7 significant digits** (`6.674300e-11`, `1.054572e-34`, `8.854188e-12`). Values between 10⁻⁴ and 10⁴ are written with `repr` and keep all digits, which is why the problem is easy to miss.

- 411 constants: 221 have full precision, **189 are cut to 7 digits** (worst relative error 4.8e-07: `mo_x_unit`, `helion_mag_mom`, `kilogram_hertz_relationship`), 1 differs by edition (`electron_mass`).
- The public short names of `physure.constants` that read a cut entry: `sigma`, `eps0`, `epsilon_0`, `mu0`, `mu_0`, `m_p`, `m_n`, `u`, `a0`, `mu_B`, `Ryd`, `R_inf`. The SI-defining ones (`c`, `h`, `k`, `e`, `N_A`), `hbar`, `G`, `R`, `F`, `alpha`, `g` and `atm` are at full precision.
- The existing test `tests/measurement_tests/test_constants_codata.py` does not notice: it checks the SI-defining constants bit by bit, but compares the rest, and the derived relations (σ, μ_B, R∞, …), with `rel_tol` of 1e-6 and looser, which hides errors of 1e-7.

## C. What a user sees (evaluated by the Rust core)

| Conversion | Result | Exact | Relative error |
|---|---|---|---|
| 1 deg => arcmin | 60.000042971865454 | 60 | 7.2e-07 |
| 1 arcmin => arcsec | 59.999957096920205 | 60 | 7.2e-07 |
| 1 deg => arcsec | 3600.000004125296 | 3600 | 1.1e-09 |
| 180 deg => rad | 3.1415926499999998 | π | 1.1e-09 |
| 1 gal => fl_oz | 128.000067628113 | 128 | 5.3e-07 |
| 1 gal => in^3 | 230.99989113364055 | 231 | 4.7e-07 |
| 1 acre => ft^2 | 43559.99545332424 | 43560 | 1.0e-07 |
| 1 atm => Torr | 760.0021001785152 | 760 | 2.8e-06 |
| 1 psi => lbf/in^2 | 0.9999998709866548 | 1 | 1.3e-07 |
| 1 hp => ft*lbf/s | 549.9999527370215 | 550 | 8.6e-08 |
| 1 slug*ft/s^2 => lbf | 0.9999997122445776 | 1 | 2.9e-07 |
| 1 ly => m | 9460700000000000 | 9460730472580800 | 3.2e-06 |
| 1 pc => ly | 3.2615979790078957 | 3.2615637771674336 | 1.0e-05 |

Identities among the stored constants: `ε₀ μ₀ c² − 1 = −2.8e-08` (scipy's own values give 1.2e-12), `σ / (2π⁵k⁴/15h³c²) − 1 = −7.4e-08`, `α / (e²/4πε₀ħc) − 1 = 2.1e-08`.

Also visible, and fixed only by exact rational scales (A1 stage 1): `1 ft => in` gives `12.000000000000002`.

## D. Conventions that the catalog must state

The numbers are not wrong, they just do not say which unit they mean. Each needs a one-line comment in `physure.conf`, and the alias list should not suggest the other meaning:

| Unit | Catalog meaning | Other meanings that exist |
|---|---|---|
| btu | one value, `1055.06` | BTU_IT 1055.05585262, thermochemical 1054.35026444, ISO 1055.056, 59 °F about 1054.8 |
| acre | the international acre, truncated | US survey acre 4046.872609874252 |
| gallon | US liquid gallon (alias `gal`) | imperial gallon 4.54609 L |
| horsepower | mechanical, truncated | metric 735.49875 W, electrical 746 W |
| calorie | thermochemical, 4.184 J (correct) | IT calorie 4.1868 J |
| year | Julian, 365.25 d (correct) | tropical, calendar |
| mmHg and torr | both `133.322` | torr = atm/760 exactly; conventional mmHg is 1.4e-7 larger |

## E. Two CODATA editions in one file

`[Constants]` says "CODATA 2022 generated from scipy". The atomic and Planck units in `[Units]` and `electron_mass` in `[Constants]` are CODATA 2018 (`bohr = 5.29177210903e-11`; 2022 is `5.29177210544e-11`), and `tests/measurement_tests/test_constants_codata.py` says in its docstring that it checks the 2018 vintage. The differences are small (≤ 1.4e-9 except `planck_charge`, 4.4e-8) but a catalog should be from one edition. `electron_mass`, `electron_volt`, `planck_length`, `planck_mass`, `planck_temperature` and `planck_time` are defined in both sections.

## F. Recommended fix

One catalog-only change, independent of A1 (Python reads the same file), in this order:

1. **Generator:** replace `f"{val:e}"` with `repr(val)`; regenerate `[Constants]` once and keep the SI-defining constants exact.
2. **`[Units]`:** put the exact values of section A into the catalog (angles from `π/180`, `π/10800`, `π/648000` at full `f64` precision; the imperial units from their exact definitions).
3. **Edition:** choose CODATA 2022 for the whole file; update the atomic and Planck units and the docstring and expected values of `test_constants_codata.py`.
4. **Conventions:** add the comments of section D.
5. **Regression tests:** the reference table of `audit_units_and_constants.py` as a test of every `[Units]` factor (relative error ≤ 4e-16), the identities of section C (`pytest.approx(rel=1e-15)`), and `[Constants]` against `scipy.constants` at 1e-12 (`importorskip`).
6. **Consequences to expect:** the A1 Stage 0 parity test lists 46 symbols whose Python and Rust results differ in the last bit; changing the factors changes that list, so `KNOWN_LAST_BIT` and `tests/core/golden/unit_semantics.json` need regenerating in the same change. The generated unit reference documents need regenerating too (see the `add-unit` skill).

With exact rational scales (A1 stage 1) the identities of section C hold to the last bit; before that they hold to about 10⁻¹⁶, which is enough to remove every error in this audit.
