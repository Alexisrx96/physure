import math

import pytest

from measurekit.ext.chemistry.species import (
    ATOMIC_WEIGHTS,
    Species,
    parse_formula,
)


def test_parse_simple_formula():
    assert parse_formula("H2O") == {"H": 2, "O": 1}


def test_parse_unicode_subscript_formula():
    assert parse_formula("H₂O") == {"H": 2, "O": 1}
    assert parse_formula("C₆H₁₂O₆") == {"C": 6, "H": 12, "O": 6}
    assert parse_formula("Ca(NO₃)₂") == {"Ca": 1, "N": 2, "O": 6}


def test_parse_no_count_defaults_to_one():
    assert parse_formula("NaCl") == {"Na": 1, "Cl": 1}


def test_parse_nested_parens():
    assert parse_formula("Ca(NO3)2") == {"Ca": 1, "N": 2, "O": 6}
    assert parse_formula("Fe3(PO4)2") == {"Fe": 3, "P": 2, "O": 8}


def test_parse_leading_group_and_trailing_elements():
    # ammonium sulfate: parenthesized group with a multiplier, followed by
    # a bare element and a bare-element-with-count.
    assert parse_formula("(NH4)2SO4") == {"N": 2, "H": 8, "S": 1, "O": 4}


def test_parse_multiple_nested_groups():
    assert parse_formula("Al2(SO4)3") == {"Al": 2, "S": 3, "O": 12}
    assert parse_formula("Ca3(PO4)2") == {"Ca": 3, "P": 2, "O": 8}


def test_parse_group_followed_by_more_bare_elements():
    # potassium ferrocyanide: bare element, then a multiplied group,
    # followed by more bare elements sharing atoms with the group.
    assert parse_formula("K4Fe(CN)6") == {"K": 4, "Fe": 1, "C": 6, "N": 6}


def test_parse_large_subscript_no_other_elements():
    assert parse_formula("C60") == {"C": 60}  # buckminsterfullerene


def test_unknown_element_raises():
    with pytest.raises(ValueError, match="Unknown element"):
        parse_formula("Xx2O")


def test_malformed_formula_raises():
    with pytest.raises(ValueError, match="Unbalanced parentheses"):
        parse_formula("H2O)")


def test_unclosed_paren_raises():
    with pytest.raises(ValueError, match="Invalid formula"):
        parse_formula("(H2O")


def test_lowercase_start_raises():
    with pytest.raises(ValueError, match="Invalid formula"):
        parse_formula("h2o")


def test_unsupported_bracket_notation_raises():
    # square brackets (complex-ion notation) aren't in the supported
    # grammar -- must fail loudly rather than silently mis-parsing.
    with pytest.raises(ValueError, match="Invalid formula"):
        parse_formula("K4[Fe(CN)6]")


def test_empty_formula_raises():
    with pytest.raises(ValueError, match="Invalid formula"):
        parse_formula("")


@pytest.mark.parametrize(
    ("formula", "known_molar_mass"),
    [
        ("H2O", 18.015),
        ("CO2", 44.009),
        ("NaCl", 58.44),
        ("C6H12O6", 180.156),  # glucose
        ("H2SO4", 98.079),  # sulfuric acid
        ("NaHCO3", 84.007),  # sodium bicarbonate
        ("Al2(SO4)3", 342.15),  # aluminum sulfate
        ("CaCO3", 100.087),  # calcium carbonate / limestone
        ("NH4NO3", 80.043),  # ammonium nitrate
        ("KMnO4", 158.034),  # potassium permanganate
        ("(NH4)2SO4", 132.14),  # ammonium sulfate
        ("Ca3(PO4)2", 310.177),  # calcium phosphate
        ("C60", 720.64),  # buckminsterfullerene
        ("K4Fe(CN)6", 368.35),  # potassium ferrocyanide
    ],
)
def test_molar_mass_matches_published_value(formula, known_molar_mass):
    computed = Species(formula).molar_mass.to("g/mol").magnitude
    assert math.isclose(computed, known_molar_mass, rel_tol=2e-4)


def test_molar_mass_uncertainty_is_quadrature_of_atomic_uncertainties():
    water = Species("H2O")
    _, h_std = ATOMIC_WEIGHTS["H"]
    _, o_std = ATOMIC_WEIGHTS["O"]
    expected_variance = (2 * h_std) ** 2 + (1 * o_std) ** 2
    assert math.isclose(
        water.molar_mass.uncertainty, expected_variance**0.5, rel_tol=1e-9
    )


def test_molar_mass_uncertainty_scales_with_atom_count():
    # glucose has many more atoms than water -- the tabulated per-element
    # uncertainty must compound with atom count, not just total mass.
    glucose = Species("C6H12O6")
    _, c_std = ATOMIC_WEIGHTS["C"]
    _, h_std = ATOMIC_WEIGHTS["H"]
    _, o_std = ATOMIC_WEIGHTS["O"]
    expected_variance = (6 * c_std) ** 2 + (12 * h_std) ** 2 + (6 * o_std) ** 2
    assert math.isclose(
        glucose.molar_mass.uncertainty, expected_variance**0.5, rel_tol=1e-9
    )


def test_species_repr_contains_formula_and_molar_mass():
    water = Species("H2O")
    assert "H2O" in repr(water)
    assert "molar_mass" in repr(water)
