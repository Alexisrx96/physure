"""Test suite for the parsers module using pytest."""

import pytest

from measurekit.domain.notation.base_entity import BaseExponentEntity
from measurekit.domain.notation.lexer import (
    TokenType,
    UnitToken,
    generate_tokens,
)
from measurekit.domain.notation.parsers import NotationParser


def test_initialization():
    """Test parser initialization."""
    tokens = generate_tokens("m")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.current.type == TokenType.UNIT
    assert parser.current.value == "m"


def test_eat_method():
    """Test the eat method."""
    tokens = generate_tokens("m")
    parser = NotationParser(tokens, BaseExponentEntity)
    token = parser.eat(TokenType.UNIT)
    assert token.type == TokenType.UNIT
    assert token.value == "m"
    assert parser.current.type == TokenType.EOF
    with pytest.raises(ValueError, match=r"Expected .* but got .*"):
        parser.eat(TokenType.MUL)


def test_parse_simple_units():
    """Test parsing simple unit expressions."""
    tokens = generate_tokens("m")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"m": 1}

    tokens = generate_tokens("m·s")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"m": 1, "s": 1}

    tokens = generate_tokens("m²")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"m": 2}

    tokens = generate_tokens("m^2")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"m": 2}


def test_parse_complex_expressions():
    """Test parsing complex unit expressions."""
    tokens = generate_tokens("m/s")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"m": 1, "s": -1}

    tokens = generate_tokens("kg·m/s²")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"kg": 1, "m": 1, "s": -2}

    tokens = generate_tokens("kg*m^2/s^2")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"kg": 1, "m": 2, "s": -2}


def test_parse_with_parentheses():
    """Test parsing expressions with parentheses."""
    tokens = generate_tokens("(m)")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"m": 1}

    tokens = generate_tokens("(m/s)")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"m": 1, "s": -1}

    tokens = generate_tokens("(m/s)²")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"m": 2, "s": -2}

    tokens = generate_tokens("(m·(kg/s))")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"m": 1, "kg": 1, "s": -1}


def test_parse_with_unity():
    """Test parsing expressions with unity (1) term."""
    tokens = generate_tokens("1")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {}

    tokens = generate_tokens("1/s")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"s": -1}

    tokens = generate_tokens("kg·1/s")
    parser = NotationParser(tokens, BaseExponentEntity)
    assert parser.parse().exponents == {"kg": 1, "s": -1}


def test_parser_error_handling():
    """Test error handling in parsing."""
    tokens = [
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.UNIT, "kg"),
        UnitToken(TokenType.EOF, ""),
    ]
    parser = NotationParser(iter(tokens), BaseExponentEntity)
    with pytest.raises(ValueError, match="Unexpected token at the end"):
        parser.parse()

    tokens = [
        UnitToken(TokenType.DIV, "/"),
        UnitToken(TokenType.EOF, ""),
    ]
    parser = NotationParser(iter(tokens), BaseExponentEntity)
    with pytest.raises(ValueError, match="Unexpected token"):
        parser.parse()

    tokens = generate_tokens("(m")
    parser = NotationParser(tokens, BaseExponentEntity)
    with pytest.raises(
        ValueError, match=r"Expected .*RPAREN.*, but got .*EOF"
    ):
        parser.parse()


def test_parse_exponent_method():
    """Test the _parse_exponent helper method."""
    tokens = [
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.SUP, "²"),
        UnitToken(TokenType.EOF, ""),
    ]
    parser = NotationParser(iter(tokens), BaseExponentEntity)
    parser.eat(TokenType.UNIT)
    assert parser._parse_exponent() == 2

    tokens = [
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.EXP, "^"),
        UnitToken(TokenType.NUMBER, "3"),
        UnitToken(TokenType.EOF, ""),
    ]
    parser = NotationParser(iter(tokens), BaseExponentEntity)
    parser.eat(TokenType.UNIT)
    assert parser._parse_exponent() == 3

    tokens = [
        UnitToken(TokenType.UNIT, "m"),
        UnitToken(TokenType.MUL, "·"),
        UnitToken(TokenType.EOF, ""),
    ]
    parser = NotationParser(iter(tokens), BaseExponentEntity)
    parser.eat(TokenType.UNIT)
    assert parser._parse_exponent() is None
