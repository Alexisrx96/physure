"""Test suite for unit parsing using the mandatory Rust core parser."""

import pytest

from physure._core import parse_unit_expression


def test_parse_simple_units():
    """Test parsing simple unit expressions."""
    assert parse_unit_expression("m").dimensions == {"m": (1, 1)}
    assert parse_unit_expression("m·s").dimensions == {
        "m": (1, 1),
        "s": (1, 1),
    }
    assert parse_unit_expression("m²").dimensions == {"m": (2, 1)}
    assert parse_unit_expression("m^2").dimensions == {"m": (2, 1)}


def test_parse_complex_expressions():
    """Test parsing complex unit expressions."""
    assert parse_unit_expression("m/s").dimensions == {
        "m": (1, 1),
        "s": (-1, 1),
    }
    assert parse_unit_expression("kg·m/s²").dimensions == {
        "kg": (1, 1),
        "m": (1, 1),
        "s": (-2, 1),
    }
    assert parse_unit_expression("kg*m^2/s^2").dimensions == {
        "kg": (1, 1),
        "m": (2, 1),
        "s": (-2, 1),
    }


def test_parse_with_parentheses():
    """Test parsing expressions with parentheses."""
    assert parse_unit_expression("(m)").dimensions == {"m": (1, 1)}
    assert parse_unit_expression("(m/s)").dimensions == {
        "m": (1, 1),
        "s": (-1, 1),
    }
    assert parse_unit_expression("(m/s)²").dimensions == {
        "m": (2, 1),
        "s": (-2, 1),
    }
    assert parse_unit_expression("(m·(kg/s))").dimensions == {
        "kg": (1, 1),
        "m": (1, 1),
        "s": (-1, 1),
    }


def test_parse_with_unity():
    """Test parsing expressions with unity (1) term."""
    assert parse_unit_expression("1").dimensions == {}
    assert parse_unit_expression("1/s").dimensions == {"s": (-1, 1)}
    assert parse_unit_expression("kg·1/s").dimensions == {
        "kg": (1, 1),
        "s": (-1, 1),
    }


def test_parser_error_handling():
    """Test error handling in native parsing."""
    with pytest.raises(ValueError, match="Parse error"):
        parse_unit_expression("/")

    with pytest.raises(ValueError, match="Parse error"):
        parse_unit_expression("(m")
