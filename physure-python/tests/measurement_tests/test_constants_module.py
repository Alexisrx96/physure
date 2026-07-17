"""Tests for the physure.constants ergonomic re-export module."""

import pytest

from physure import constants, get_current_system


@pytest.fixture
def system():
    return get_current_system()


@pytest.mark.parametrize(
    ("alias", "canonical"),
    sorted(constants._ALIASES.items()),
)
def test_alias_matches_canonical_constant(system, alias, canonical):
    aliased = getattr(constants, alias)
    canonical_q = system.get_constant(canonical)
    assert aliased.magnitude == canonical_q.magnitude
    assert aliased.unit == canonical_q.unit


def test_unknown_attribute_raises_attribute_error():
    with pytest.raises(AttributeError):
        _ = constants.not_a_real_constant


def test_dir_lists_all_aliases():
    assert set(dir(constants)) == set(constants.__all__)
    assert "hbar" in dir(constants)
