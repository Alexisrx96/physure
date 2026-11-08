# Standard atomic weights and their uncertainty

The chemistry module of the Rust core ([Phase 5 of the chemistry roadmap](chemistry_roadmap.md)) takes
its atomic weights from `physure.conf`. This page says exactly what those numbers are, how the
uncertainty that CIAAW publishes is turned into the standard uncertainty physure uses, how a molar mass
combines them, how to choose the table that is read and how to keep a calculation reproducible, what was
checked, and where the model stops being exact. Every statement here is a statement of a cited source, a
property of the data file, or an arithmetic result that is worked out in the text.

## 1. What the data are

A *table* of atomic weights is a section `[AtomicWeights:<edition>]` of `physure.conf`, named by its
*edition*. The file ships two, and both come from the *Atomic Weights 2021* report (Prohaska et al.,
*Pure Appl. Chem.* 94(5), 573-600 (2022),
[doi:10.1515/pac-2019-0603](https://doi.org/10.1515/pac-2019-0603)) with the 2024 revisions of Gd, Lu and
Zr by the CIAAW. Both hold all 118 elements, in g/mol:

| Edition | What it is | Source |
| --- | --- | --- |
| `standard-2024` (the default) | the **CIAAW Standard Atomic Weights 2024**, with the intervals kept as intervals | [ciaaw.org/atomic-weights.htm](https://ciaaw.org/atomic-weights.htm) |
| `abridged-2024` (an option) | the **CIAAW abridged standard atomic weights 2024**, five significant digits | [ciaaw.org/abridged-atomic-weights.htm](https://ciaaw.org/abridged-atomic-weights.htm) |

**A shipped edition is frozen.** Once released it is never edited: a new CIAAW release becomes a new
edition (for example `standard-2026`) next to the old ones. Section 6 says why and how that is enforced.

Each page was transcribed as printed (digit groups, footnote letters and dashes included) into
[`tests/golden/ciaaw_standard_atomic_weights_2024.tsv`](https://github.com/Alexisrx96/physure/blob/main/tests/golden/ciaaw_standard_atomic_weights_2024.tsv)
and
[`tests/golden/ciaaw_abridged_atomic_weights_2024.tsv`](https://github.com/Alexisrx96/physure/blob/main/tests/golden/ciaaw_abridged_atomic_weights_2024.tsv).
The block of `physure.conf`
([`[AtomicWeights:standard-2024]`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/physure.conf#L705)
and the sections that follow) is **generated** from those two files by
[`scripts/gen_atomic_weights_conf.py`](https://github.com/Alexisrx96/physure/blob/main/scripts/gen_atomic_weights_conf.py)
and is never edited by hand. Besides the two tables the block has the (empty) section `[AtomicWeights]`
for your own definitions and `[Chemistry]`, which selects the table that is read (section 5). A line of a
table looks like one of these:

```ini
Na = 22.98976928, 0.00000002     # standard-2024; CIAAW prints 22.989 769 28(2)
H  = [1.00784, 1.00811]          # standard-2024; CIAAW prints [1.007 84, 1.008 11]
Tc = 98, 0                       # either edition; CIAAW prints a dash: no standard atomic weight
H  = 1.0080, 0.0002              # abridged-2024; CIAAW prints 1.0080 and a "±" of 0.0002
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
applies exactly that to every line of every table:

| Printed by CIAAW | Weight stored | Standard uncertainty stored |
| --- | --- | --- |
| `x(n)`, `U` = `n` units of the last digit shown | `x` | `U / sqrt(3)` |
| `[a, b]` | `(a + b) / 2` | `(b - a) / sqrt(12)` (the same rule with half-width `(b - a) / 2`) |
| the abridged `x` and `±` | `x` | `± / sqrt(3)` |
| a dash | the mass number (section 7) | `0` |
| `Sym = value, u, standard` (your own conf) | `value` | `u`, as given |

**Worked example, hydrogen** (`standard-2024`). CIAAW prints the interval `[1.007 84, 1.008 11]`.

- weight: `(1.00784 + 1.00811) / 2 = 1.007975`
- `b - a = 0.00027`, so `u = 0.00027 / sqrt(12) = 7.794e-5`
- the same number from the half-width: `0.000135 / sqrt(3) = 7.794e-5`

**Worked example, water** (`standard-2024`). Oxygen is the interval `[15.99903, 15.99977]`, so its weight is
`15.9994` and `u(O) = 0.00074 / sqrt(12) = 2.1362e-4`. With the combination rule of section 4:

- `M(H2O) = 2 * 1.007975 + 15.9994 = 18.01535 g/mol`
- `u^2 = (2 * 7.794e-5)^2 + (2.1362e-4)^2 = 2.430e-8 + 4.563e-8 = 6.993e-8`, so `u(M) = 2.6445e-4 g/mol`

That is `M(H2O) = 18.01535 ± 0.00026 g/mol` (standard uncertainty). Section 5.4 gives the same molecule
under `abridged-2024`.

## 4. How a molar mass combines its atoms

For a molecular weight IUPAC gives `u^2(Mr) = sum over elements E of nu_E^2 * u^2(Ar(E))`: an element
contributes its stoichiometric coefficient `nu_E` times its standard uncertainty, and different elements
are uncorrelated. physure follows this exactly, which has two consequences that are easy to get wrong:

- the atoms of one element are **one quantity**, not independent copies. In `H2O` the two hydrogens add as
  `2 u(H)`, not as `sqrt(2) u(H)`;
- the molar mass of an element is one stable uncertainty source in the Rust core, so two species that share
  an element are correlated through it, and a molar mass cancels against itself on a round trip
  (`moles_to_mass(mass_to_moles(m))` keeps the uncertainty of `m`).

**Worked example, 50 g of CO2** (`standard-2024`). Carbon is the interval `[12.0096, 12.0116]`: weight
`12.0106`, `u(C) = 0.0020 / sqrt(12) = 5.7735e-4`. With `u(O)` from section 3:

- `M(CO2) = 12.0106 + 2 * 15.9994 = 44.0094 g/mol`
- `u^2(M) = (5.7735e-4)^2 + (2 * 2.1362e-4)^2 = 3.333e-7 + 1.825e-7 = 5.159e-7`, so `u(M) = 7.1824e-4 g/mol`
  (had the two oxygens been taken as independent, it would be `6.516e-4`, which is wrong)
- for a mass of `50.0 ± 0.1 g`: `n = 50 / 44.0094 = 1.13612 mol`
- relative uncertainties: `0.1 / 50 = 2.0e-3` for the mass and `7.1824e-4 / 44.0094 = 1.632e-5` for the molar
  mass, combined `sqrt((2.0e-3)^2 + (1.632e-5)^2) = 2.000067e-3`
- `u(n) = 1.13612 * 2.000067e-3 = 0.00227 mol`

So `n = 1.13612 ± 0.00227 mol`; the molar mass contributes `1.9e-5 mol` of it and the weighing nearly all.

These are first-order (GUM) propagations of the standard uncertainties; see section 10.

## 5. Choosing the table

### 5.1 The two shipped editions and what each "±" is

- **`standard-2024`** holds the standard atomic weights proper. For the 70 elements with a single value the
  "±" is the half-width of the interval that CIAAW evaluated; for the 14 interval elements (Ar, B, Br, C, Cl,
  H, Pb, Li, Mg, N, O, Si, S, Tl) the table gives the interval itself, read as in section 3.
- **`abridged-2024`** holds the abridged table, "best abridged to five digits" in the Commission's
  recommendation of 2013. Its "±" is, in the words of Prohaska et al. (2022), section 7 (rules i-iii of the
  abridged table), "a conservative value … a simplified measure of the reliability of the abridged values".
  The standard uncertainty derived from it (`± / sqrt(3)`) is therefore conservative too.

### 5.2 Why `standard-2024` is the default

Using the abridged "±" as a half-width would put a rounding convention where the Commission's evaluation
should be. For the elements whose weight has more than five significant digits the abridged "±" is just the
place value of the last rounded digit, and for the interval elements the single abridged value "does not
correspond necessarily to the midpoints of the intervals" (Prohaska et al. 2022, §7). Two examples from the
data:

- **Na**: abridged `22.990 ± 0.001`; the published value is `22.989 769 28(2)`, that is `U = 0.00000002`, fifty
  thousand times smaller than the abridged `0.001`.
- **Ar**: the abridged table gives the single value `39.95 ± 0.16`; the published datum is the interval
  `[39.792, 39.963]`, whose midpoint is `39.8775` and whose standard uncertainty is `0.0494`.

The abridged table is still published by CIAAW as a table of standard atomic weights, so it is available as
an option: select it when a calculation must be compared with a document that quotes the abridged values.

### 5.3 The selector, the names and the aliases

The table that is read is chosen in the `[Chemistry]` section of any conf, of the working directory or of a
unit system:

```ini
[Chemistry]
atomic_weights = abridged-2024
```

- The value is an **edition name** or an **alias**. The aliases are `standard` (the newest standard edition
  shipped with this release: `standard-2024`) and `abridged` (`abridged-2024`). The shipped file selects
  `standard`.
- **An alias moves when a release ships a newer edition of its kind; a pinned name never changes meaning.**
  Every release that moves an alias says so in the CHANGELOG. Section 6 explains why that matters.
- An edition name is lower-case ASCII letters, digits and `-`, `_`, `.`, starts with a letter, and has at
  most 64 characters. Any other name, in `atomic_weights = ...` or in a section header, is a problem and the
  text is refused. A table cannot be called `standard` or `abridged`: the alias would shadow it and it
  could never be selected, so the section is refused.
- `[Chemistry]` has one key. An unknown key, an empty value or an ill-formed one is a problem; the last
  selection wins; as for every definition, a text with a problem is refused as a whole and nothing of it is
  applied.
- The selection is checked when the weights are used, not when it is read, so the choice and the table may
  come from different texts, in any order. If the selected edition is defined by no section, every use of
  the atomic weights fails with an error that names the edition; nothing is taken from another table.

### 5.4 Your definitions: one rule

The section `[AtomicWeights]` (no suffix) holds **your** definitions of elements. They override or add
elements **on top of whichever table is selected, in any order**, and the shipped file leaves the section
empty. The lookup of an element is exactly:

1. your definitions (`[AtomicWeights]`), then
2. the selected table,

and nothing else: there is no fallback from one table to another. Your definitions win over the selected
table and over nothing else. A definition does not depend on which table is selected, nor on whether the
selection comes before or after it, nor on whether they are in the same text:

```ini
[AtomicWeights]
Fe = 56.0, 0.3        # stored as 56.0 with the standard uncertainty 0.3 / sqrt(3) = 0.1732

[Chemistry]
atomic_weights = abridged-2024
```

Here `Fe` is `56.0` with `0.1732` under `abridged-2024`, and it would be the same under `standard-2024` or
under a table of your own; every other element is read from the selected table. A later definition of the
same symbol replaces the earlier one.

### 5.5 The same molecule under both editions

Water, computed by a script and checked by hand. Under `standard-2024` (section 3):
`M = 2 * 1.007975 + 15.9994 = 18.01535 g/mol`, `u = 0.00026 g/mol`.

Under `abridged-2024`: H is `1.0080 ± 0.0002` and O is `15.999 ± 0.001`, so

- `M = 2 * 1.0080 + 15.999 = 18.015 g/mol`
- `u(H) = 0.0002 / sqrt(3) = 1.1547e-4` and `u(O) = 0.001 / sqrt(3) = 5.7735e-4`
- `u^2 = (2 * 1.1547e-4)^2 + (5.7735e-4)^2 = 5.333e-8 + 3.333e-7 = 3.867e-7`, so `u(M) = 6.218e-4 g/mol`

| Edition | `M(H2O)` / g/mol | `u(M)` / g/mol |
| --- | --- | --- |
| `standard-2024` | `18.01535` | `0.00026` |
| `abridged-2024` | `18.015` | `0.00062` |

The two molar masses differ by `0.00035 g/mol`, less than the abridged standard uncertainty. The abridged
uncertainty is `2.35` times the standard one: that is the conservatism of the abridged "±", **not a better
or worse measurement**.

## 6. Reproducibility and audited use

**Why.** In a regulated or audited setting a calculation is validated against one version of the data, and
changing that version can take months of paperwork. A library upgrade must therefore never change the atomic
weights a calculation uses unless its owner chooses so, and the owner must be able to state which table
produced a result.

**The policy.**

- A released edition is **frozen**: its numbers never change. A new CIAAW release is added as a **new
  edition** with a new name, next to the old ones.
- A fingerprint test enforces it (section 9): if either shipped table changes by a single bit, the test
  fails with a message that says to add a new edition instead.
- Only the **aliases** (`standard`, `abridged`) move, when a release ships a newer edition, and the CHANGELOG
  of that release says so.

**What to do.**

1. *Pin the edition name* in your conf: `atomic_weights = standard-2024`. A pinned name never changes
   meaning, whatever release of the library you run.
2. Or carry *a complete table of your own* under your own name, validated by you:

   ```ini
   [AtomicWeights:acme-2026-03]
   # illustrative numbers, invented for this example: the values of your own validated table go here
   H = 1.00794, 0.00007
   C = 12.0107, 0.0008
   O = 15.9994, 0.0003

   [Chemistry]
   atomic_weights = acme-2026-03
   ```

   Nothing is then taken from ours: an element that your table lacks is an error that names the element
   and the edition (`no atomic weight for 'Na' in edition 'acme-2026-03'`; for a formula,
   `unknown element 'Na' in edition 'acme-2026-03'`), never a silent value from another table. Your own
   `[AtomicWeights]` definitions still apply on top of it (section 5.4).
3. *State the table in your report.* `chemistry::atomic_weight_edition()` returns the table that produced the
   weights of the catalog in scope, and `UnitRegistry::atomic_weight_edition()` does the same for a
   registry: the resolved edition (`standard-2024`, so an alias is reported as the edition it stood for),
   followed by ` + N definitions` when your definitions are not empty (`standard-2024 + 2 definitions`;
   `acme-2026-03`). A report can quote it.
4. *Assess a change before adopting it.* To see what moving to another edition would do to a calculation,
   evaluate the same calculation twice, each time against a registry with its own selection (the embedded
   catalog extended with `[Chemistry] atomic_weights = <edition>`, run inside `units::with_catalog`, which
   is how the tests of `atomic_weights_editions.rs` and the chemistry modules select a table), and compare
   the two results with the tolerance your procedure allows.

**The limits of this guarantee.** It covers the **atomic weights only**. The other data of `physure.conf`
(physical constants, unit definitions, prefixes) are versioned with the library release, not by edition. A
setting that needs those frozen as well must also pin the version of the library, or carry a `physure.conf`
of its own.

## 7. Elements with no standard atomic weight

For Tc, Pm, Po, At, Rn, Fr, Ra, Ac and Np to Og (34 elements) CIAAW prints a dash, in both tables. The
tables hold the mass number of a long-lived isotope with an uncertainty of **0**. That is a convention of
this file, **not a CIAAW value**: the mass numbers were taken over from the Python package this module
replaced (and are the last column of the TSVs, which say so; the two TSVs agree on all 34). A molar mass
built from one of these elements therefore has no uncertainty from the element; define the mass of the
isotope you mean in `[AtomicWeights]` (section 8) when that matters.

## 8. Overriding in your own conf

A conf of your own, or a unit system, can replace an element, add one, or provide a whole table; a later
definition wins and every section is read strictly (a bad line refuses the whole text and nothing is
applied). All the atomic-weight sections share one grammar. Three forms of a line:

```ini
[AtomicWeights]
# an element, with the half-width of its interval: stored as 0.5 / sqrt(3)
Fe = 56.0, 0.5
# an interval: stored as its midpoint and (b - a) / sqrt(12)
H = [1.00784, 1.00811]
# an isotope with its own standard uncertainty (`, standard`: the number is stored as given)
D = 2.01410177812, 0.00000000012, standard
```

With the last line, `D2O` parses (under any table); without a definition of `D` it is refused. A weight
must be positive and finite, a number finite and not negative, and an interval needs `0 < a < b` and takes
no third field. These definitions apply on top of whichever table is selected (section 5.4); to carry a
whole table of your own, put the same lines in `[AtomicWeights:<your-edition>]` and select it (section 6).
The parser is
[`read_atomic_weight`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/conf.rs#L276)
and the selector is
[`read_chemistry`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/conf.rs#L309).

## 9. Verification performed

- **Transcription.** Each TSV was cross-checked against the CIAAW page before use. Standard: 118 rows,
  14 intervals (H, Li, B, C, N, O, Mg, Si, S, Cl, Ar, Br, Tl, Pb) and 34 dashes; the 70 elements printed as a
  single value round (to five significant figures, half-up) to the values of the abridged table, and the
  abridged value of 12 of the 14 interval elements lies inside the interval. Abridged: 118 rows and the same
  34 dashes; the 34 mass numbers of the two TSVs, and those of the Python package, are equal.
- **The conf is held to the tables.**
  [`physure-core/tests/atomic_weights_ciaaw.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-core/tests/atomic_weights_ciaaw.rs)
  derives the expected `(weight, standard uncertainty)` of every row of each TSV with its own small parser
  (nothing shared with the conf reader or the generator) and compares it with the embedded catalog to
  `1e-12` relative (mass numbers exactly). It also asserts the shape (118 rows; standard: 14 intervals and
  34 dashes; abridged: 34 dashes), `H = [1.007 84, 1.008 11]` and `Na = 22.989 769 28(2)` in the standard
  table, `H = 1.0080 ± 0.0002` and `Pb = 207.2 ± 1.1` in the abridged one, that the 34 mass numbers agree, and
  that every single standard value rounds half-up to its abridged value.
- **The editions are frozen.** The same file holds one *fingerprint* per shipped edition: the FNV-1a 64-bit
  hash of a canonical text with one `Sym=<weight bits>,<standard-uncertainty bits>;` entry per element in
  symbol order (the bits are the IEEE-754 binary64 pattern as 16 lower-case hex digits). The constants in
  the test were computed from the TSVs by a separate Python script, not from the Rust code, and the Rust
  test recomputes them from the conf: `standard-2024` is `0xc92e208bc767774b` and `abridged-2024` is
  `0xac3e57bf770452a6`. A change to a published edition, by accident or on purpose, fails the test; its
  message says to add a new edition instead.
- **The reader and the selector.** `atomic_weights_conf.rs` tests the grammar (every form and every refused
  line, in each kind of section) and `atomic_weights_editions.rs` tests the editions: the defaults, alias
  and pinned name agreeing for all 118 elements, definitions winning over the standard, the abridged and a
  custom table in every order and across several texts, a complete table of your own with no fallback, the
  selected-but-undefined edition, the refused texts, and the edition report.
- **The molar masses.**
  [`tests/golden/chemistry_molar_mass.tsv`](https://github.com/Alexisrx96/physure/blob/main/tests/golden/chemistry_molar_mass.tsv)
  holds the molar mass of the 118 elements and 20 compounds under the default edition, computed by
  [`scripts/gen_chemistry_golden.py`](https://github.com/Alexisrx96/physure/blob/main/scripts/gen_chemistry_golden.py)
  with a third, separate reader of the conf (selector, tables and definitions included) and the Python
  formula parser; the Rust core is held to it to `1e-12`. The expected numbers in the unit tests are written
  by hand from the TSVs.
- **Three discrepancies, explained.** Against the Python package's 2013 table the only values that moved by
  more than their half-width are **O** (`15.999` in Python; now the interval midpoint `15.9994` of
  `[15.99903, 15.99977]`), **Tl** (`204.38`; now `204.3835`, the midpoint of `[204.382, 204.385]`) and **Lu**
  (`174.9668`; now `174.96669(5)`, a 2024 revision). For O and Tl the abridged value lies just *below* the
  interval, which its "±" covers. **Yb** is `173.045(10)` in the standard table and `173.05` in the abridged
  table (rounded half-up); that is a rounding, not a disagreement.

To check the generator: `python scripts/gen_atomic_weights_conf.py` rewrites the block and changes nothing
when the two TSVs and the block already agree.

## 10. Known limits

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
  it, and it needs a value of its own (section 8).
- **The table is revised.** CIAAW revises the table every two years, so a new release of the data is a new
  edition: its TSV and its section must be added and regenerated together (add the TSV, extend the
  generator, add the edition's tests and fingerprint), and the frozen editions stay as they are.
- **One edition per calculation.** The stable uncertainty source of an element is identified by its symbol,
  not by edition, so quantities computed with different editions that are then combined in one expression
  would share one source per element. Compare editions by evaluating the calculation twice (section 6), not
  by mixing the results.
- **What is frozen.** Only the atomic weights are versioned by edition (section 6). The chemistry module is
  reachable from Rust; Python and PHS do not use it yet.

## 11. References

- Prohaska, T. et al. "Standard atomic weights of the elements 2021 (IUPAC Technical Report)." *Pure Appl.
  Chem.* 94(5), 573-600 (2022). [doi:10.1515/pac-2019-0603](https://doi.org/10.1515/pac-2019-0603)
- Possolo, A., van der Veen, A. M. H., Meija, J., Hibbert, D. B. *Pure Appl. Chem.* 90(2), 395-424 (2018).
  [doi:10.1515/pac-2016-0402](https://doi.org/10.1515/pac-2016-0402)
- van der Veen, A. M. H. et al. IUPAC *Guidelines for the use of atomic weights*, published as
  "Interpretation and use of standard atomic weights".
  [doi:10.1515/pac-2017-1002](https://doi.org/10.1515/pac-2017-1002)
- CIAAW, Standard Atomic Weights 2024: [ciaaw.org/atomic-weights.htm](https://ciaaw.org/atomic-weights.htm)
- CIAAW, Abridged Standard Atomic Weights 2024:
  [ciaaw.org/abridged-atomic-weights.htm](https://ciaaw.org/abridged-atomic-weights.htm)
