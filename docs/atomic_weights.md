# Standard atomic weights and their uncertainty

The chemistry module of the Rust core ([Phase 5 of the chemistry roadmap](chemistry_roadmap.md)) takes
its atomic weights from the `[AtomicWeights]` section of `physure.conf`. This page says exactly what
those numbers are, how the uncertainty that CIAAW publishes is turned into the standard uncertainty
physure uses, how a molar mass combines them, what was checked, and where the model stops being exact.
Every statement here is a statement of a cited source, a property of the data file, or an arithmetic
result that is worked out in the text.

## 1. What the data are

The section is the **CIAAW Standard Atomic Weights 2024**
([ciaaw.org/atomic-weights.htm](https://ciaaw.org/atomic-weights.htm)): the table of the *Atomic Weights
2021* report (Prohaska et al., *Pure Appl. Chem.* 94(5), 573-600 (2022),
[doi:10.1515/pac-2019-0603](https://doi.org/10.1515/pac-2019-0603)) with the 2024 revisions of Gd, Lu and
Zr by the CIAAW. All 118 elements are present, in g/mol.

The page was transcribed as printed into
[`tests/golden/ciaaw_standard_atomic_weights_2024.tsv`](https://github.com/Alexisrx96/physure/blob/main/tests/golden/ciaaw_standard_atomic_weights_2024.tsv)
(digit groups, footnote letters and the dashes included). The section of `physure.conf`
([`[AtomicWeights]`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/physure.conf#L705))
is **generated** from that file by
[`scripts/gen_atomic_weights_conf.py`](https://github.com/Alexisrx96/physure/blob/main/scripts/gen_atomic_weights_conf.py)
and is never edited by hand. A line looks like one of these:

```ini
Na = 22.98976928, 0.00000002     # CIAAW prints 22.989 769 28(2)
H  = [1.00784, 1.00811]          # CIAAW prints [1.007 84, 1.008 11]
Tc = 98, 0                       # CIAAW prints a dash: no standard atomic weight
```

## 2. What the published uncertainty means

The number in parentheses (or the width of an interval) is **not** a standard uncertainty and **not** an
expanded uncertainty in the sense of the GUM.

- Prohaska et al. (2022), sections 6 and 7: the uncertainty `U[Ar°(E)]` is such that the atomic weights of
  normal materials "are expected to lie between Ar°(E) − U and Ar°(E) + U with great certitude". The older
  format `192.217(2)` was abandoned because it "suggests that the stated uncertainty is a standard
  uncertainty".
- Possolo, van der Veen, Meija and Hibbert, *Pure Appl. Chem.* 90(2), 395-424 (2018),
  [doi:10.1515/pac-2016-0402](https://doi.org/10.1515/pac-2016-0402): it is a "decisional expanded
  uncertainty" that "differs essentially from the expanded uncertainty defined in the GUM".

So the published `U` is the half-width of an interval in which the atomic weight of normal materials is
expected to lie, and it carries no coverage factor. Treating it as a standard uncertainty (as the Python
package did before) overstates every uncertainty by a factor of `sqrt(3)`, because the standard uncertainty
of a uniform distribution of half-width `U` is `U / sqrt(3)` (section 3).

## 3. The model: a uniform distribution

Prohaska et al. write that "the uniform distribution on these intervals … can be used as a simple and
practically useful model", and the IUPAC guidelines (van der Veen et al., *Interpretation and use of
standard atomic weights*, [doi:10.1515/pac-2017-1002](https://doi.org/10.1515/pac-2017-1002)) turn it into
a rule: for an interval `[a, b]` the standard uncertainty is `u(Ar) = (b - a) / sqrt(12)`; their own example
says "the half-width of the interval is 0.01 and hence the standard uncertainty is 0.01/√3 = 0.006". physure
applies exactly that to every line of the section:

| Printed by CIAAW | Weight stored | Standard uncertainty stored |
| --- | --- | --- |
| `x(n)`, `U` = `n` units of the last digit shown | `x` | `U / sqrt(3)` |
| `[a, b]` | `(a + b) / 2` | `(b - a) / sqrt(12)` (the same rule with half-width `(b - a) / 2`) |
| a dash | the mass number (section 6) | `0` |
| `Sym = value, u, standard` (your own conf) | `value` | `u`, as given |

**Worked example, hydrogen.** CIAAW prints the interval `[1.007 84, 1.008 11]`.

- weight: `(1.00784 + 1.00811) / 2 = 1.007975`
- `b - a = 0.00027`, so `u = 0.00027 / sqrt(12) = 7.794e-5`
- the same number from the half-width: `0.000135 / sqrt(3) = 7.794e-5`

**Worked example, water.** Oxygen is the interval `[15.99903, 15.99977]`, so its weight is `15.9994` and
`u(O) = 0.00074 / sqrt(12) = 2.1362e-4`. With the combination rule of section 4:

- `M(H2O) = 2 * 1.007975 + 15.9994 = 18.01535 g/mol`
- `u^2 = (2 * 7.794e-5)^2 + (2.1362e-4)^2 = 2.430e-8 + 4.563e-8 = 6.993e-8`, so `u(M) = 2.6445e-4 g/mol`

That is `M(H2O) = 18.01535 ± 0.00026 g/mol` (standard uncertainty).

## 4. How a molar mass combines its atoms

For a molecular weight IUPAC gives `u^2(Mr) = sum over elements E of nu_E^2 * u^2(Ar(E))`: an element
contributes its stoichiometric coefficient `nu_E` times its standard uncertainty, and different elements
are uncorrelated. physure follows this exactly, which has two consequences that are easy to get wrong:

- the atoms of one element are **one quantity**, not independent copies. In `H2O` the two hydrogens add as
  `2 u(H)`, not as `sqrt(2) u(H)`;
- the molar mass of an element is one stable uncertainty source in the Rust core, so two species that share
  an element are correlated through it, and a molar mass cancels against itself on a round trip
  (`moles_to_mass(mass_to_moles(m))` keeps the uncertainty of `m`).

**Worked example, 50 g of CO2.** Carbon is the interval `[12.0096, 12.0116]`: weight `12.0106`,
`u(C) = 0.0020 / sqrt(12) = 5.7735e-4`. With `u(O)` from section 3:

- `M(CO2) = 12.0106 + 2 * 15.9994 = 44.0094 g/mol`
- `u^2(M) = (5.7735e-4)^2 + (2 * 2.1362e-4)^2 = 3.333e-7 + 1.825e-7 = 5.159e-7`, so `u(M) = 7.1824e-4 g/mol`
  (had the two oxygens been taken as independent, it would be `6.516e-4`, which is wrong)
- for a mass of `50.0 ± 0.1 g`: `n = 50 / 44.0094 = 1.13612 mol`
- relative uncertainties: `0.1 / 50 = 2.0e-3` for the mass and `7.1824e-4 / 44.0094 = 1.632e-5` for the molar
  mass, combined `sqrt((2.0e-3)^2 + (1.632e-5)^2) = 2.000067e-3`
- `u(n) = 1.13612 * 2.000067e-3 = 0.00227 mol`

So `n = 1.13612 ± 0.00227 mol`; the molar mass contributes `1.9e-5 mol` of it and the weighing nearly all.

These are first-order (GUM) propagations of the standard uncertainties; see section 9.

## 5. Why the full table and not the abridged one

CIAAW also publishes an *abridged* table, and an earlier version of this section used it. It was replaced
because of what Prohaska et al. (2022), section 7 (rules i-iii of the abridged table), say about it: its
"±" is "a conservative value … a simplified measure of the reliability of the abridged values", and for the
elements whose weight has more than five significant digits it is just the place value of the last rounded
digit. For the interval elements the single abridged value "does not correspond necessarily to the midpoints
of the intervals". Using the abridged "±" as a half-width would put a rounding convention where the
Commission's evaluation should be. Two examples from the data:

- **Na**: abridged `22.990 ± 0.001`; the published value is `22.989 769 28(2)`, that is `U = 0.00000002`, fifty
  thousand times smaller than the abridged `0.001`.
- **Ar**: the abridged table gives the single value `39.95 ± 0.16`; the published datum is the interval
  `[39.792, 39.963]`, whose midpoint is `39.8775` and whose standard uncertainty is `0.0494`.

## 6. Elements with no standard atomic weight

For Tc, Pm, Po, At, Rn, Fr, Ra, Ac and Np to Og (34 elements) CIAAW prints a dash. The section holds the
mass number of a long-lived isotope with an uncertainty of **0**. That is a convention of this file, **not a
CIAAW value**: the mass numbers were taken over from the Python package this module replaced (and are the
last column of the TSV, which says so). A molar mass built from one of these elements therefore has no
uncertainty from the element; replace the line with the mass of the isotope you mean (section 7) when
that matters.

## 7. Overriding in your own conf

A conf of your own, or a unit system, can replace an element or add one; a later definition wins and the
section is read strictly (a bad line refuses the whole text and nothing is applied). Three forms:

```ini
[AtomicWeights]
# an element, with the half-width of its interval: stored as 0.5 / sqrt(3)
Fe = 56.0, 0.5
# an interval: stored as its midpoint and (b - a) / sqrt(12)
H = [1.00784, 1.00811]
# an isotope with its own standard uncertainty (`, standard`: the number is stored as given)
D = 2.01410177812, 0.00000000012, standard
```

With the last line, `D2O` parses; without a definition of `D` it is refused. A weight must be positive and
finite, a number finite and not negative, and an interval needs `0 < a < b` and takes no third field. The
parser is
[`read_atomic_weight`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/conf.rs#L250).

## 8. Verification performed

- **Transcription.** The TSV was cross-checked against the CIAAW page before use: 118 rows, 14 intervals
  (H, Li, B, C, N, O, Mg, Si, S, Cl, Ar, Br, Tl, Pb) and 34 dashes. The 70 elements printed as a single
  value round (to five significant figures, half-up) to the values of the CIAAW abridged table, and the
  abridged value of 12 of the 14 interval elements lies inside the interval. The 34 mass numbers equal those
  of the Python package.
- **The conf is held to the table.**
  [`physure-core/tests/atomic_weights_ciaaw.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-core/tests/atomic_weights_ciaaw.rs)
  derives the expected `(weight, standard uncertainty)` of every row of the TSV with its own small parser
  (nothing shared with the conf reader or the generator) and compares it with the embedded catalog to
  `1e-12` relative (mass numbers exactly). It also asserts the 118 / 14 / 34 shape, `H = [1.007 84, 1.008 11]`
  and `Na = 22.989 769 28(2)`. The reader itself is tested in `atomic_weights_conf.rs` (the forms above and
  every refused line).
- **The molar masses.**
  [`tests/golden/chemistry_molar_mass.tsv`](https://github.com/Alexisrx96/physure/blob/main/tests/golden/chemistry_molar_mass.tsv)
  holds the molar mass of the 118 elements and 20 compounds, computed by
  [`scripts/gen_chemistry_golden.py`](https://github.com/Alexisrx96/physure/blob/main/scripts/gen_chemistry_golden.py)
  with a third, separate reader of the conf and the Python formula parser; the Rust core is held to it to
  `1e-12`. The expected numbers in the unit tests are written by hand from the TSV.
- **Three discrepancies, explained.** Against the Python package's 2013 table the only values that moved by
  more than their half-width are **O** (`15.999` in Python; now the interval midpoint `15.9994` of
  `[15.99903, 15.99977]`), **Tl** (`204.38`; now `204.3835`, the midpoint of `[204.382, 204.385]`) and **Lu**
  (`174.9668`; now `174.96669(5)`, a 2024 revision). For O and Tl the abridged value lies just *below* the
  interval, which its "±" covers. **Yb** is `173.045(10)` in the table and `173.05` in the abridged table
  (rounded half-up); that is a rounding, not a disagreement.

To check the generator: `python scripts/gen_atomic_weights_conf.py` rewrites the section and changes nothing
when the TSV and the section already agree.

## 9. Known limits

- **First-order propagation.** The uncertainty of a molar mass is the GUM first-order combination of
  standard uncertainties; no higher-order term or correlation between different elements is modelled.
- **Uniform distribution is a model.** IUPAC itself calls it "a simple and practically useful model"; it
  is chosen for its simplicity, and nothing in the sources says that the weights of normal materials are
  distributed uniformly within the interval.
- **Monoisotopic elements.** For these, the basis of the published `U` is a coverage factor of `k = 6` on
  the atomic-mass evaluation (Prohaska et al., section 6). IUPAC's recommendation is nevertheless to assign
  the uniform distribution to every standard atomic weight, which is what is done here, so these standard
  uncertainties are on the conservative side.
- **Normal materials only.** The interval is where the atomic weight of *normal materials* is expected to
  lie (section 2). For a material outside that class the standard atomic weight is not a statement about
  it, and it needs a value of its own (section 7).
- **The table is revised.** CIAAW revises the table every two years, so the TSV and the section must be
  regenerated together (edit the TSV, run the generator, run the tests).

## 10. References

- Prohaska, T. et al. "Standard atomic weights of the elements 2021 (IUPAC Technical Report)." *Pure Appl.
  Chem.* 94(5), 573-600 (2022). [doi:10.1515/pac-2019-0603](https://doi.org/10.1515/pac-2019-0603)
- Possolo, A., van der Veen, A. M. H., Meija, J., Hibbert, D. B. *Pure Appl. Chem.* 90(2), 395-424 (2018).
  [doi:10.1515/pac-2016-0402](https://doi.org/10.1515/pac-2016-0402)
- van der Veen, A. M. H. et al. IUPAC *Guidelines for the use of atomic weights*, published as
  "Interpretation and use of standard atomic weights".
  [doi:10.1515/pac-2017-1002](https://doi.org/10.1515/pac-2017-1002)
- CIAAW, Standard Atomic Weights 2024: [ciaaw.org/atomic-weights.htm](https://ciaaw.org/atomic-weights.htm)
