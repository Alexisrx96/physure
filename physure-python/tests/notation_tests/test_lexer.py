"""Test suite for superscript, subscript, and formatting utilities."""

from physure.core.formatting import (
    parse_superscript,
    subscript_to_ascii,
    to_subscript,
    to_superscript,
)


def test_to_superscript():
    """Test converting numbers to superscript."""
    assert to_superscript(123) == "¹²³"
    assert to_superscript(-45) == "⁻⁴⁵"
    assert to_superscript("0.5") == "⁰⋅⁵"
    assert to_superscript(0) == "⁰"
    assert to_superscript("123.456") == "¹²³⋅⁴⁵⁶"


def test_to_subscript():
    """Test converting numbers to subscript."""
    assert to_subscript(123) == "₁₂₃"
    assert to_subscript("456") == "₄₅₆"
    assert to_subscript("0") == "₀"
    assert to_subscript(789) == "₇₈₉"
    assert "-" not in to_subscript(-123)
    assert to_subscript("1.23") == "₁₂₃"
    assert "." not in to_subscript("1.23")


def test_subscript_to_ascii():
    """Test normalizing Unicode subscript digits back to ASCII."""
    assert subscript_to_ascii("H₂O") == "H2O"
    assert subscript_to_ascii("C₆H₁₂O₆") == "C6H12O6"
    assert subscript_to_ascii("CO₋₁") == "CO-1"
    assert subscript_to_ascii("H2O") == "H2O"


def test_parse_superscript():
    """Test parsing superscript back to integer or float."""
    assert parse_superscript("¹²³") == 123
    assert parse_superscript("⁻⁴⁵") == -45
    assert parse_superscript("⁰") == 0
    assert parse_superscript("⁰⋅⁵") == 0.5
    assert parse_superscript("¹⋅²³") == 1.23
    assert parse_superscript("abc") == 0
    assert parse_superscript("") == 0
