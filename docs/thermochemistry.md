# Standard thermochemical data

The standard enthalpy of formation (ΔfH°, kJ/mol) and the standard molar entropy (S°, J/(mol·K)) of a species,
at 298.15 K and 1 bar, are **catalog data**: tables in `physure.conf`, named by edition, selected in
`[Chemistry]`, overridable in a conf of your own, and copied, sealed and witnessed by a frozen file
([conf_freeze.md](conf_freeze.md)). They follow the pattern of the atomic weights
([atomic_weights.md](atomic_weights.md)).

**Read this first.** The one edition this release ships, `legacy-python-1`, is **not** a reference table. It is
the starter table the Python package carried, kept number for number so that nothing changes when the data move
into the catalog: 8 species, textbook values rounded to one decimal, **no uncertainty**, from **mixed sources**.
Section 3 compares every number with the reference tables. A better-sourced edition, with uncertainties, is
future work.

## 1. The sections

| Section | What it holds |
| --- | --- |
| `[Thermochemistry:<edition>]` | a table: one line `species = ΔfH°, S°` per species, every species with its phase. The shipped one is [`legacy-python-1`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/physure.conf#L748). |
| `[Thermochemistry]` | your definitions, empty in the shipped file ([source](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/physure.conf#L767)): they replace or add species of whichever edition is selected |
| `[Chemistry] thermochemistry = <edition>` | which table is read ([source](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/physure.conf#L1166)); the shipped file selects `legacy-python-1`. There is no alias: the name is the edition |

A line is `species = enthalpy, entropy`: two finite numbers, ΔfH° in kJ/mol and S° in J/(mol·K), at 298.15 K and
1 bar (100 000 Pa). Anything else (one number, three, a word, `inf`) is a problem
([`read_thermochemistry`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/conf.rs#L1098)).

Edition names follow the rule of the atomic weights: lower-case ASCII letters, digits and `-`, `_`, `.`,
starting with a letter, at most 64 characters. Names that start with `legacy-`, `standard-` or `abridged-` are
reserved for the editions of this library, present and future, for every kind of edition: a text of yours that
defines `[Thermochemistry:legacy-python-1]` (a shipped edition is frozen) or `[Thermochemistry:legacy-python-2]`
is a problem. Only the catalog embedded in the library defines them.

How a text is read is the rule of the rest of the conf: the definitions of a unit system and every source of a
frozen file are read strictly (a problem refuses the text as a whole); the `physure.conf` of the working
directory leniently (a bad line is reported on stderr as `physure.conf: <problem>; the line is ignored` and the
rest applies). A species written twice in one section of one text is a problem in every reading.

## 2. Species, phases and how a name is found

A species is a **formula followed by its phase**: `H2O(l)`, `CO2(g)`, `NaCl(cr)`, `Ca(OH)2(s)`. The phases are
`(l)` liquid, `(g)` gas, `(s)` solid, `(cr)` crystalline and `(aq)` aqueous. The trailing `(phase)` is read
**before** the formula (a formula parser would read the `(s)` of `Ca(OH)2(s)` as a group): a trailing group of
lower-case letters is a phase and must be one of these five; the rest must be written as a formula (element
symbols, counts that do not start with 0, balanced parentheses). See
[`split_species`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/registry.rs#L165).

- In a table `[Thermochemistry:<edition>]` every species carries its phase.
- **There is no alias table.** A name written without its phase (`H2O`) is the **unique** phase of that formula
  in the data in force: `H2O` is `H2O(l)` in `legacy-python-1`. In a table that has `H2O(l)` and `H2O(g)`, `H2O`
  is an error that names both phases; write the one you mean.
- A definition of yours written without a phase (`H2O = -285.83, 69.95` in `[Thermochemistry]`) is placed by the
  same rule against the selected edition, so it replaces the edition's `H2O(l)` instead of silently losing to it.
  A formula the edition has in several phases, or in none, must be written with its phase, and two definitions
  that are one species (`H2O` and `H2O(l)`) are an error. These are errors when the data are read
  ([`thermochemistry_table`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/registry.rs#L255)),
  because the selection may come from another text; a frozen file whose sources together cannot be read this way
  is refused when it is loaded.
- A species the data lack is an error that names the edition (and the phases it has of that formula, if any).

## 3. The edition `legacy-python-1`

What it is: the table of the Python package
([`thermo_kinetics.py`](https://github.com/Alexisrx96/physure/blob/main/physure-python/physure/ext/chemistry/thermo_kinetics.py#L25)),
with the phase each value belongs to written out. A test reads the literals of that Python file and holds the
edition to them bit for bit (`physure-core/tests/thermochemistry.rs`); a Python test holds the Python table to the
catalog until chemistry plan 3 replaces the table by the core's reader.

| Species | ΔfH° (kJ/mol), this edition | reference ΔfH° | S° (J/(mol·K)), this edition | reference S° |
| --- | --- | --- | --- | --- |
| H2O(l) | -285.8 | CODATA -285.830 ± 0.040 | 69.9 | CODATA 69.95 ± 0.03 |
| CO2(g) | -393.5 | CODATA -393.51 ± 0.13 | 213.7 | CODATA 213.785 ± 0.010 (NBS-82: 213.74) |
| CH4(g) | -74.8 | JANAF -74.87 | 186.3 | JANAF 186.25 |
| NH3(g) | -46.1 | CODATA -45.94 ± 0.35 (NBS-82: -46.11) | 192.4 | CODATA 192.77 ± 0.05 (NBS-82: 192.45) |
| NaCl(cr) | -411.2 | JANAF -411.12 (NBS: -411.153) | 72.1 | JANAF 72.11 |
| O2(g) | 0 | 0 (reference state) | 205.2 | CODATA 205.152 ± 0.005 |
| H2(g) | 0 | 0 (reference state) | 130.7 | CODATA 130.680 ± 0.003 |
| N2(g) | 0 | 0 (reference state) | 191.6 | CODATA 191.609 ± 0.004 |

What the comparison says:

- **The phases are right**: liquid water for -285.8 (the gas is -241.826), crystalline sodium chloride, the
  elements in their reference states at 0.
- **The sources are mixed.** Most numbers are the CODATA or JANAF value rounded to one decimal, but the entropy
  of CO2 (213.7) is not a rounding of CODATA's 213.785: it matches NBS-82's 213.74; and the enthalpy of NH3
  (-46.1) is NBS-82's -46.11, not CODATA's -45.94.
- **NH3(g) S° is a known outlier**: 192.4 against CODATA's 192.77 ± 0.05, about seven standard uncertainties
  away (it is NBS-82's 192.45 rounded).
- **There is no uncertainty**: every value is read as exact, so a result built from these numbers carries none of
  the uncertainty of the data. CODATA's ± for H2O(l) ΔfH° is 0.040 kJ/mol, for NH3(g) 0.35.

**Provenance of the reference column, and what was verified.** The reference values were first recalled by the
reviewer of the plan from CODATA Key Values for Thermodynamics (1989), NIST-JANAF (Chase 1998) and the NBS tables
(1982), without the documents at hand. In this work:

- the **CODATA** rows (H2O(l), CO2(g), NH3(g), O2(g), H2(g), N2(g)) were checked on 2026-11-15 against the
  online table "CODATA Key Values for Thermodynamics" (codata.info, standard pressure 100 000 Pa), and agree with
  the table above. CH4 and NaCl are not in that table;
- **CH4(g)** and **NaCl(cr)** were checked on 2026-11-15 against the NIST Chemistry WebBook, whose entries cite
  Chase 1998 (NIST-JANAF): CH4(g) ΔfH° -74.87, S° (1 bar) 186.25; NaCl(cr) ΔfH° -411.12, S° 72.11, all without
  an uncertainty on that page. The reviewer had recalled -74.873 ± 0.34 for CH4 and 72.13 for NaCl; the WebBook
  gives -74.87 and 72.11;
- the **NBS-82** numbers (213.74, -46.11, 192.45, -411.153) were **not** verified: the NBS tables (Wagman et al.,
  J. Phys. Chem. Ref. Data 11, Suppl. 2, 1982) were not consulted. They are the reviewer's recollection, quoted to
  explain where the edition's numbers come from;
- the checks read web pages, not the printed primary publications (Cox, Wagman and Medvedev, *CODATA Key Values
  for Thermodynamics*, 1989; Chase, *NIST-JANAF Thermochemical Tables*, 4th ed., 1998).

None of this changes the edition: it is frozen, and it keeps the numbers the Python package had. A
better-sourced edition, with each species' source and its uncertainty (and more species and phases), would be a
**new** edition under a new name, selected with `[Chemistry] thermochemistry`; it is future work.

## 4. Reading the data

The core reads them through the catalog in scope (the configuration in force, a frozen file included):

- `chemistry::standard_enthalpy(species)`: a `Quantity` in kJ/mol;
- `chemistry::standard_entropy(species)`: a `Quantity` in J/(mol·K);

both through
[`UnitRegistry::thermochemistry_of`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/units/registry.rs#L296)
([source](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/chemistry/thermo.rs#L77)). Unicode
subscripts read as digits (`H₂O`).

**Not yet:** the Python package still answers `standard_enthalpy` and `standard_entropy` from its own table (equal
to this edition, held so by a test) until chemistry plan 3 makes it delegate; it does not honour a frozen file or
your `[Thermochemistry]` definitions. PHS has no chemistry builtins, so the data have no language surface.

## 5. Frozen files and witnesses

A frozen file copies the sections like the rest of the catalog; `phs conf diff` and `phs conf show
thermochemistry` list them as `thermochemistry` (your definitions, by the key you wrote),
`thermochemistry_table` (`edition/species`), `thermochemistry_edition` (the number of species) and
`thermochemistry_selection`. Three built-in witnesses read the data in force
([source](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/frozen/battery/chemistry.rs#L89)):
`chem.dhf_h2o_l` (-285.8 kJ/mol), `chem.entropy_h2o_l` (69.9 J/(mol·K)) and `chem.gibbs_formation_h2o_l`, the
Gibbs energy of formation of liquid water, H2(g) + ½ O2(g) → H2O(l), as ΔG = ΔH − TΔS from the table's entries
(ΔS = 69.9 − 130.7 − 205.2/2 = −163.4 J/(mol·K), ΔG = −237.08 kJ/mol). A definition of yours changes exactly
these three. The older witness `chem.gibbs_298` does not read the table: its ΔS is a literal, −163.2, and an id
never changes what it computes.

A file frozen before the data moved into the catalog has no `[Chemistry] thermochemistry` and still loads; the
readers then say that no edition is selected.
