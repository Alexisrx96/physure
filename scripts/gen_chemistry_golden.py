"""Writes tests/golden/chemistry_*.tsv.

Run with an environment that has physure installed (the main checkout's venv works), from the
repository root:

    PYTHONUTF8=1 physure-python/.venv/Scripts/python.exe scripts/gen_chemistry_golden.py

The molar masses are the atomic weights physure.conf selects (`[Chemistry] atomic_weights`, the alias
`standard`, that is the edition `standard-2024` = the CIAAW Standard Atomic Weights 2024, with the definitions
of `[AtomicWeights]` on top, none in the shipped file) combined by the Python formula parser: the weights are read from the
conf and the formulas are parsed by `physure.ext.chemistry`, so neither comes from the Rust code
under test. The reading rules of the conf are written out again here, not imported from the core
(and are independent of scripts/gen_atomic_weights_conf.py, which writes the sections): the second
column of `Sym = weight, U` is the half-width CIAAW publishes, read as a uniform distribution, so
the standard uncertainty is U / sqrt(3) unless the line ends in `, standard` (then the number is
taken as given); `Sym = [a, b]` is an interval, with weight (a + b) / 2 and standard uncertainty
(b - a) / sqrt(12). The reactions file still freezes what `physure.ext.chemistry` answered before
the core took it over. The core's `tests/chemistry_golden.rs` holds the Rust port to both. Formulas and equations on which the two
are meant to differ (hydrates, a zero count) are deliberately not in the lists.
"""

from __future__ import annotations

import math
from pathlib import Path

from physure.ext.chemistry.reaction import Reaction
from physure.ext.chemistry.species import parse_formula

ROOT = Path(__file__).resolve().parent.parent
GOLDEN = ROOT / "tests" / "golden"
CONF = ROOT / "physure-core" / "src" / "units" / "physure.conf"

COMPOUNDS = [
    "H2O", "H₂O", "CO2", "C6H12O6", "C₆H₁₂O₆", "Ca(NO3)2", "NaCl", "Mg3(Fe(CN)6)2",
    "Al2(SO4)3", "C8H18", "NH4NO3", "Fe2O3", "(NH4)2SO4", "CH3(CH2)4CH3",
    "K4Fe(CN)6", "Ca3(PO4)2", "C12H22O11", "NaHCO3", "H2SO4", "UF6",
]

REACTIONS = [
    "H2 + O2 -> H2O",
    "C8H18 + O2 -> CO2 + H2O",
    "Fe2O3 + C -> Fe + CO2",
    "2 H2 + 5 O2 -> 9 H2O",
    "N2 + 3 H2 ⇌ 2 NH3",
    "N2 + H2 <=> NH3",
    "N2 + H2 = NH3",
    "N2 + H2 → NH3",
    "C₂H₆ + O₂ -> CO₂ + H₂O",
    "Al + HCl -> AlCl3 + H2",
    "KMnO4 + HCl -> KCl + MnCl2 + H2O + Cl2",
    "C6H12O6 + O2 -> CO2 + H2O",
    "Ca(OH)2 + H3PO4 -> Ca3(PO4)2 + H2O",
]

REFUSED = [
    "H2 + O2 -> H2O + H2O2",
    "H2 -> O2",
    "H2 + H2O -> O2",
    "H2 + O2",
    "H2 -> O2 -> H2O",
    "H2 + -> H2O",
    "H2 + Xx -> H2",
    "H2O) -> H2",
]


ALIASES = {"standard": "standard-2024", "abridged": "abridged-2024"}  # what physure.conf documents


def read_line(line: str) -> tuple[str, tuple[float, float]]:
    """`Sym = weight, U [, standard]` or `Sym = [a, b]` -> (symbol, (weight, standard uncertainty))."""
    symbol, rest = (part.strip() for part in line.split("=", 1))
    if rest.startswith("["):
        assert rest.endswith("]"), line
        low, high = (float(x) for x in rest[1:-1].split(","))
        assert 0 < low < high, line
        return symbol, ((low + high) / 2, (high - low) / math.sqrt(12))
    weight, number, *kind = (field.strip() for field in rest.split(","))
    assert kind in ([], ["interval"], ["standard"]), line
    std = float(number) if kind == ["standard"] else float(number) / math.sqrt(3)
    return symbol, (float(weight), std)


def atomic_weights() -> dict[str, tuple[float, float]]:
    """symbol -> (weight, standard uncertainty) of the table physure.conf selects.

    `[Chemistry] atomic_weights` names an edition or an alias (`standard` is `standard-2024`); the
    table is the section `[AtomicWeights:<edition>]`, and the lines of `[AtomicWeights]` (the
    definitions of the user, none in the shipped file) are applied on top of it. A line
    `symbol = weight, U` gives U / sqrt(3) (uniform distribution over the CIAAW interval weight +- U);
    `symbol = weight, u, standard` gives u; `symbol = [a, b]` gives the midpoint and (b - a) / sqrt(12).
    """
    tables: dict[str, dict[str, tuple[float, float]]] = {}
    definitions: dict[str, tuple[float, float]] = {}
    selection = "standard"
    section = None
    for raw in CONF.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith(("#", ";")):
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1]
            if section.startswith("AtomicWeights:"):
                tables.setdefault(section.removeprefix("AtomicWeights:"), {})
        elif section == "AtomicWeights":
            symbol, value = read_line(line)
            definitions[symbol] = value
        elif section is not None and section.startswith("AtomicWeights:"):
            symbol, value = read_line(line)
            tables[section.removeprefix("AtomicWeights:")][symbol] = value
        elif section == "Chemistry":
            key, value = (part.strip() for part in line.split("=", 1))
            assert key == "atomic_weights", line
            selection = value
    return {**tables[ALIASES.get(selection, selection)], **definitions}


def molar_mass_rows() -> list[str]:
    weights = atomic_weights()
    rows = []
    for formula in [*weights, *COMPOUNDS]:
        mass = variance = 0.0
        for element, count in parse_formula(formula).items():
            weight, std = weights[element]
            mass += count * weight
            variance += (count * std) ** 2
        rows.append(f"{formula}\t{mass!r}\t{variance**0.5!r}")
    return rows


def reaction_rows() -> list[str]:
    rows = []
    for equation in REACTIONS:
        r = Reaction.from_string(equation)
        react = ",".join(map(str, r.reactant_coeffs))
        prod = ",".join(map(str, r.product_coeffs))
        rows.append(f"{equation}\t{int(r.reversible)}\t{react}\t{prod}")
    for equation in REFUSED:
        try:
            Reaction.from_string(equation)
        except ValueError:
            rows.append(f"{equation}\tERROR\t\t")
        else:
            raise SystemExit(f"{equation!r} was expected to be refused")
    return rows


def write(name: str, header: str, rows: list[str]) -> None:
    text = f"# {header}\n" + "\n".join(rows) + "\n"
    (GOLDEN / name).write_text(text, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    write(
        "chemistry_molar_mass.tsv",
        "formula, molar mass (g/mol), standard uncertainty (CIAAW half-width / sqrt(3), or (b - a) / sqrt(12) for an interval, combined per element in quadrature); the CIAAW Standard Atomic Weights 2024 of physure.conf combined by the formula parser of physure.ext.chemistry, by scripts/gen_chemistry_golden.py",
        molar_mass_rows(),
    )
    write(
        "chemistry_reactions.tsv",
        "equation, reversible, reactant coefficients, product coefficients (ERROR when refused); by scripts/gen_chemistry_golden.py",
        reaction_rows(),
    )
