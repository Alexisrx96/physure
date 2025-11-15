"""Test suite for specific failing cases in the NotationParser."""

import unittest

from measurekit.domain.notation.base_entity import BaseExponentEntity
from measurekit.domain.notation.lexer import (
    TokenType,
    UnitToken,
    generate_tokens,
)
from measurekit.domain.notation.parsers import NotationParser


class TestNotationParserFailingCases(unittest.TestCase):
    """
    Provides specific tests for expected failure branches.
    """

    def test_factor_value_error_fallback(self):
        """
        Covers the `except ValueError` block in `factor()` by manually
        feeding the parser a token that the lexer would normally not produce.
        This simulates a scenario where an embedded exponent cannot be parsed
        as an int.
        """
        # We simulate a hypothetical lexer that produces a single
        # "m-s" UNIT token.
        mock_tokens = iter(
            [
                UnitToken(TokenType.UNIT, "m-s"),
                UnitToken(TokenType.EOF, ""),
            ]
        )
        parser = NotationParser(mock_tokens, BaseExponentEntity)

        # The parser will try to parse '-s' as an exponent, fail, and then
        # fall back to treating the *entire* original token "m-s" as the unit.
        result = parser.parse()
        self.assertEqual(result.exponents, {"m-s": 1})

    def test_exponent_not_followed_by_number_raises_error(self):
        """
        Covers the ValueError in `_parse_exponent` when a caret operator '^'
        is not followed by a valid number.
        """
        tokens = generate_tokens("m^s")  # 's' is a UNIT, not a NUMBER
        parser = NotationParser(tokens, BaseExponentEntity)
        with self.assertRaisesRegex(
            ValueError, "Expected number after exponent operator, got UNIT"
        ):
            parser.parse()


if __name__ == "__main__":
    unittest.main()
