"""Test suite for the lexer module."""

import unittest

from measurekit.domain.notation.lexer import (
    TokenType,
    UnitToken,
    generate_tokens,
    parse_superscript,
    to_subscript,
    to_superscript,
)


class TestSuperscriptSubcript(unittest.TestCase):
    """Tests for superscript and subscript conversion functions."""

    def test_to_superscript(self):
        """Test converting numbers to superscript."""
        self.assertEqual(to_superscript(123), "¹²³")
        self.assertEqual(to_superscript(-45), "⁻⁴⁵")
        self.assertEqual(to_superscript("0.5"), "⁰⋅⁵")
        self.assertEqual(to_superscript(0), "⁰")
        self.assertEqual(to_superscript("123.456"), "¹²³⋅⁴⁵⁶")

    def test_to_subscript(self):
        """Test converting numbers to subscript."""
        self.assertEqual(to_subscript(123), "₁₂₃")
        self.assertEqual(to_subscript("456"), "₄₅₆")
        self.assertEqual(to_subscript("0"), "₀")
        self.assertEqual(to_subscript(789), "₇₈₉")
        # Subscript doesn't handle decimal points or negative signs
        self.assertNotIn("-", to_subscript(-123))
        self.assertEqual(to_subscript("1.23"), "₁₂₃")
        self.assertNotIn(".", to_subscript("1.23"))

    def test_parse_superscript(self):
        """Test parsing superscript back to integer or float."""
        self.assertEqual(parse_superscript("¹²³"), 123)
        self.assertEqual(parse_superscript("⁻⁴⁵"), -45)
        self.assertEqual(parse_superscript("⁰"), 0)

        # Test parsing decimal superscripts
        self.assertEqual(parse_superscript("⁰⋅⁵"), 0.5)
        self.assertEqual(parse_superscript("¹⋅²³"), 1.23)

        # Invalid superscript should return 0
        self.assertEqual(parse_superscript("abc"), 0)
        self.assertEqual(parse_superscript(""), 0)


class TestTokenGeneration(unittest.TestCase):
    """Tests for the token generation function."""

    def test_generate_basic_tokens(self):
        """Test generating tokens for basic unit expressions."""
        # Test a simple unit
        tokens = list(generate_tokens("m"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "m"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

        # Test multiple units
        tokens = list(generate_tokens("m kg s"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "m"),
                UnitToken(TokenType.UNIT, "kg"),
                UnitToken(TokenType.UNIT, "s"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

    def test_generate_tokens_with_superscripts(self):
        """Test generating tokens with superscript measurekit.notation."""
        tokens = list(generate_tokens("m²"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "m"),
                UnitToken(TokenType.SUP, "²"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

        tokens = list(generate_tokens("kg⁻¹"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "kg"),
                UnitToken(TokenType.SUP, "⁻¹"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

    def test_generate_tokens_with_operations(self):
        """Test generating tokens with mathematical operations."""
        # Test multiplication
        tokens = list(generate_tokens("m·s"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "m"),
                UnitToken(TokenType.MUL, "·"),
                UnitToken(TokenType.UNIT, "s"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

        # Test alternative multiplication
        tokens = list(generate_tokens("m*s"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "m"),
                UnitToken(TokenType.MUL, "*"),
                UnitToken(TokenType.UNIT, "s"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

        # Test division
        tokens = list(generate_tokens("m/s"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "m"),
                UnitToken(TokenType.DIV, "/"),
                UnitToken(TokenType.UNIT, "s"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

        # Test exponent notation
        tokens = list(generate_tokens("m^2"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "m"),
                UnitToken(TokenType.EXP, "^"),
                UnitToken(TokenType.NUMBER, "2"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

    def test_generate_tokens_with_parentheses(self):
        """Test generating tokens with parentheses grouping."""
        tokens = list(generate_tokens("(m/s)"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.LPAREN, "("),
                UnitToken(TokenType.UNIT, "m"),
                UnitToken(TokenType.DIV, "/"),
                UnitToken(TokenType.UNIT, "s"),
                UnitToken(TokenType.RPAREN, ")"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

        # With superscript outside parentheses
        tokens = list(generate_tokens("(m/s)²"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.LPAREN, "("),
                UnitToken(TokenType.UNIT, "m"),
                UnitToken(TokenType.DIV, "/"),
                UnitToken(TokenType.UNIT, "s"),
                UnitToken(TokenType.RPAREN, ")"),
                UnitToken(TokenType.SUP, "²"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

    def test_generate_tokens_complex_expressions(self):
        """Test generating tokens for complex unit expressions."""
        tokens = list(generate_tokens("kg·m²·s⁻²"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "kg"),
                UnitToken(TokenType.MUL, "·"),
                UnitToken(TokenType.UNIT, "m"),
                UnitToken(TokenType.SUP, "²"),
                UnitToken(TokenType.MUL, "·"),
                UnitToken(TokenType.UNIT, "s"),
                UnitToken(TokenType.SUP, "⁻²"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

        # With mixed notation
        tokens = list(generate_tokens("kg*m^2/s^2"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "kg"),
                UnitToken(TokenType.MUL, "*"),
                UnitToken(TokenType.UNIT, "m"),
                UnitToken(TokenType.EXP, "^"),
                UnitToken(TokenType.NUMBER, "2"),
                UnitToken(TokenType.DIV, "/"),
                UnitToken(TokenType.UNIT, "s"),
                UnitToken(TokenType.EXP, "^"),
                UnitToken(TokenType.NUMBER, "2"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

    def test_error_handling(self):
        """Test error handling for invalid characters."""
        with self.assertRaises(ValueError) as cm:
            list(generate_tokens("m@s"))
        self.assertIn("Unexpected character '@'", str(cm.exception))

        with self.assertRaises(ValueError) as cm:
            list(generate_tokens("kg#m"))
        self.assertIn("Unexpected character '#'", str(cm.exception))

        with self.assertRaises(ValueError) as cm:
            list(generate_tokens("?"))
        self.assertIn("Unexpected character '?'", str(cm.exception))

    def test_handling_special_characters(self):
        """Test handling of special characters in unit names."""
        # Test units with special characters like Ω (Ohm) and µ (micro)
        tokens = list(generate_tokens("Ω"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "Ω"),
                UnitToken(TokenType.EOF, ""),
            ],
        )

        tokens = list(generate_tokens("µm"))
        self.assertEqual(
            tokens,
            [
                UnitToken(TokenType.UNIT, "µm"),
                UnitToken(TokenType.EOF, ""),
            ],
        )


if __name__ == "__main__":
    unittest.main()
