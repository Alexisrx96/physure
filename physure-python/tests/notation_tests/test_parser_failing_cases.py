"""Test suite for specific failing cases in the NotationParser using pytest."""

import pytest

from physure.domain.notation.base_entity import BaseExponentEntity
from physure.domain.notation.lexer import (
    TokenType,
    UnitToken,
    generate_tokens,
)
from physure.domain.notation.parsers import NotationParser


def test_factor_value_error_fallback():
    """Test fallback when an embedded exponent cannot be parsed."""
    mock_tokens = iter(
        [
            UnitToken(TokenType.UNIT, "m-s"),
            UnitToken(TokenType.EOF, ""),
        ]
    )
    parser = NotationParser(mock_tokens, BaseExponentEntity)
    result = parser.parse()
    assert result.exponents == {"m-s": 1}


def test_exponent_not_followed_by_number_raises_error():
    """Test error when caret is not followed by a number."""
    tokens = generate_tokens("m^s")
    parser = NotationParser(tokens, BaseExponentEntity)
    with pytest.raises(
        ValueError, match="Expected number after exponent operator, got UNIT"
    ):
        parser.parse()
