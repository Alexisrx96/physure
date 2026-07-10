import math

import pytest

from measurekit.ext.chemistry.species import Species, parse_formula


def test_parse_simple_formula():
    assert parse_formula("H2O") == {"H": 2, "O": 1}


def test_parse_no_count_defaults_to_one():
    assert parse_formula("NaCl") == {"Na": 1, "Cl": 1}


def test_parse_nested_parens():
    assert parse_formula("Ca(NO3)2") == {"Ca": 1, "N": 2, "O": 6}
    assert parse_formula("Fe3(PO4)2") == {"Fe": 3, "P": 2, "O": 8}


def test_unknown_element_raises():
    with pytest.raises(ValueError, match="Unknown element"):
        parse_formula("Xx2O")


def test_malformed_formula_raises():
    with pytest.raises(ValueError, match="Unbalanced parentheses"):
        parse_formula("H2O)")


def test_species_molar_mass():
    water = Species("H2O")
    assert math.isclose(
        water.molar_mass.to("g/mol").magnitude, 18.015, rel_tol=1e-4
    )
    assert water.molar_mass.uncertainty > 0
