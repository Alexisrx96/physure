"""Test suite for the lexer module using pytest."""

import pytest

from measurekit.domain.notation.lexer import (
    TokenType,
    UnitToken,
    generate_tokens,
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
    # Subscript doesn't handle decimal points or negative signs
    assert "-" not in to_subscript(-123)
    assert to_subscript("1.23") == "₁₂₃"
    assert "." not in to_subscript("1.23")


def test_subscript_to_ascii():
    """Test normalizing Unicode subscript digits back to ASCII."""
    assert subscript_to_ascii("H₂O") == "H2O"
    assert subscript_to_ascii("C₆H₁₂O₆") == "C6H12O6"
    assert subscript_to_ascii("CO₋₁") == "CO-1"
    assert subscript_to_ascii("H2O") == "H2O"  # already ASCII: no-op


def test_parse_superscript():
    """Test parsing superscript back to integer or float."""
    assert parse_superscript("¹²³") == 123
    assert parse_superscript("⁻⁴⁵") == -45
    assert parse_superscript("⁰") == 0
    assert parse_superscript("⁰⋅⁵") == 0.5
    assert parse_superscript("¹⋅²³") == 1.23
    assert parse_superscript("abc") == 0
    assert parse_superscript("") == 0


def test_generate_basic_tokens():
    """Test generating tokens for basic unit expressions."""
    tokens = list(generate_tokens("m"))
    assert tokens == [
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.EOF, ""),
    ]

    tokens = list(generate_tokens("m kg s"))
    assert tokens == [
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.UNIT, "kg"),
        UnitToken(TokenType.UNIT, "s"),
        UnitToken(TokenType.EOF, ""),
    ]


def test_generate_tokens_with_superscripts():
    """Test generating tokens with superscript notation."""
    tokens = list(generate_tokens("m²"))
    assert tokens == [
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.SUP, "²"),
        UnitToken(TokenType.EOF, ""),
    ]

    tokens = list(generate_tokens("kg⁻¹"))
    assert tokens == [
        UnitToken(TokenType.UNIT, "kg"),
        UnitToken(TokenType.SUP, "⁻¹"),
        UnitToken(TokenType.EOF, ""),
    ]


def test_generate_tokens_with_operations():
    """Test generating tokens with mathematical operations."""
    tokens = list(generate_tokens("m·s"))
    assert tokens == [
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.MUL, "·"),
        UnitToken(TokenType.UNIT, "s"),
        UnitToken(TokenType.EOF, ""),
    ]

    tokens = list(generate_tokens("m*s"))
    assert tokens == [
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.MUL, "*"),
        UnitToken(TokenType.UNIT, "s"),
        UnitToken(TokenType.EOF, ""),
    ]

    tokens = list(generate_tokens("m/s"))
    assert tokens == [
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.DIV, "/"),
        UnitToken(TokenType.UNIT, "s"),
        UnitToken(TokenType.EOF, ""),
    ]

    tokens = list(generate_tokens("m^2"))
    assert tokens == [
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.EXP, "^"),
        UnitToken(TokenType.NUMBER, "2"),
        UnitToken(TokenType.EOF, ""),
    ]


def test_generate_tokens_with_parentheses():
    """Test generating tokens with parentheses grouping."""
    tokens = list(generate_tokens("(m/s)"))
    assert tokens == [
        UnitToken(TokenType.LPAREN, "("),
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.DIV, "/"),
        UnitToken(TokenType.UNIT, "s"),
        UnitToken(TokenType.RPAREN, ")"),
        UnitToken(TokenType.EOF, ""),
    ]


def test_generate_tokens_complex_expressions():
    """Test generating tokens for complex unit expressions."""
    tokens = list(generate_tokens("kg·m²·s⁻²"))
    assert tokens == [
        UnitToken(TokenType.UNIT, "kg"),
        UnitToken(TokenType.MUL, "·"),
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.SUP, "²"),
        UnitToken(TokenType.MUL, "·"),
        UnitToken(TokenType.UNIT, "s"),
        UnitToken(TokenType.SUP, "⁻²"),
        UnitToken(TokenType.EOF, ""),
    ]


def test_lexer_error_handling():
    """Test error handling for invalid characters."""
    with pytest.raises(ValueError, match="Unexpected character '@'"):
        list(generate_tokens("m@s"))

    with pytest.raises(ValueError, match="Unexpected character '#'"):
        list(generate_tokens("kg#m"))


def test_handling_special_characters():
    """Test handling of special characters in unit names."""
    tokens = list(generate_tokens("Ω"))
    assert tokens == [
        UnitToken(TokenType.UNIT, "Ω"),
        UnitToken(TokenType.EOF, ""),
    ]

    tokens = list(generate_tokens("µm"))
    assert tokens == [
        UnitToken(TokenType.UNIT, "µm"),
        UnitToken(TokenType.EOF, ""),
    ]
