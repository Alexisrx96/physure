"""Test suite for the parsers module."""

import unittest

from notation.base_entity import BaseExponentEntity
from notation.lexer import TokenType, UnitToken, generate_tokens
from notation.parsers import NotationParser


class TestNotationParser(unittest.TestCase):
    """Tests for the NotationParser class."""

    def test_initialization(self):
        """Test parser initialization."""
        tokens = generate_tokens("m")
        parser = NotationParser(tokens, BaseExponentEntity)

        # Check the token buffer works
        self.assertEqual(parser.current.type, TokenType.UNIT)
        self.assertEqual(parser.current.value, "m")

    def test_eat_method(self):
        """Test the eat method."""
        tokens = generate_tokens("m")
        parser = NotationParser(tokens, BaseExponentEntity)

        # Eat the current token when it matches the expected type
        token = parser.eat(TokenType.UNIT)
        self.assertEqual(token.type, TokenType.UNIT)
        self.assertEqual(token.value, "m")

        # Parser should advance to EOF
        self.assertEqual(parser.current.type, TokenType.EOF)

        # Eating a token that doesn't match should raise ValueError
        with self.assertRaises(ValueError):
            parser.eat(TokenType.MUL)

    def test_parse_simple_units(self):
        """Test parsing simple unit expressions."""
        # Single unit
        tokens = generate_tokens("m")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"m": 1})

        # Multiple units with multiplication
        tokens = generate_tokens("m·s")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"m": 1, "s": 1})

        # Unit with superscript exponent
        tokens = generate_tokens("m²")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"m": 2})

        # Unit with caret exponent
        tokens = generate_tokens("m^2")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"m": 2})

    def test_parse_complex_expressions(self):
        """Test parsing complex unit expressions."""
        # Units with division
        tokens = generate_tokens("m/s")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"m": 1, "s": -1})

        # Multiple units with mixed operations
        tokens = generate_tokens("kg·m/s²")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"kg": 1, "m": 1, "s": -2})

        # Mixed notation styles
        tokens = generate_tokens("kg*m^2/s^2")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"kg": 1, "m": 2, "s": -2})

    def test_parse_with_parentheses(self):
        """Test parsing expressions with parentheses."""
        # Simple parentheses
        tokens = generate_tokens("(m)")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"m": 1})

        # Parentheses with operations inside
        tokens = generate_tokens("(m/s)")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"m": 1, "s": -1})

        # Parentheses with exponent
        tokens = generate_tokens("(m/s)²")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"m": 2, "s": -2})

        # Nested parentheses
        tokens = generate_tokens("(m·(kg/s))")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"m": 1, "kg": 1, "s": -1})

    def test_parse_with_unity(self):
        """Test parsing expressions with unity (1) term."""
        # Unity in numerator
        tokens = generate_tokens("1")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {})

        # Unity in denominator with unit
        tokens = generate_tokens("1/s")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"s": -1})

        # More complex expression with unity
        tokens = generate_tokens("kg·1/s")
        parser = NotationParser(tokens, BaseExponentEntity)
        result = parser.parse()
        self.assertEqual(result.exponents, {"kg": 1, "s": -1})

    def test_error_handling(self):
        """Test error handling in parsing."""
        # Invalid token at the end
        tokens = [
            UnitToken(TokenType.UNIT, "m"),
            UnitToken(TokenType.UNIT, "kg"),  # Invalid, expecting operator
            UnitToken(TokenType.EOF, ""),
        ]
        parser = NotationParser(iter(tokens), BaseExponentEntity)
        with self.assertRaises(ValueError) as cm:
            parser.parse()
        self.assertIn("Unexpected token at the end", str(cm.exception))

        # Invalid factor
        tokens = [
            UnitToken(TokenType.DIV, "/"),  # Invalid start
            UnitToken(TokenType.EOF, ""),
        ]
        parser = NotationParser(iter(tokens), BaseExponentEntity)
        with self.assertRaises(ValueError) as cm:
            parser.parse()
        self.assertIn("Unexpected token", str(cm.exception))

        # Unclosed parenthesis
        tokens = generate_tokens("(m")
        parser = NotationParser(tokens, BaseExponentEntity)
        with self.assertRaises(ValueError):
            parser.parse()

    def test_parse_exponent_method(self):
        """Test the _parse_exponent helper method."""
        # Superscript exponent
        tokens = [
            UnitToken(TokenType.UNIT, "m"),
            UnitToken(TokenType.SUP, "²"),
            UnitToken(TokenType.EOF, ""),
        ]
        parser = NotationParser(iter(tokens), BaseExponentEntity)
        parser.eat(TokenType.UNIT)  # Advance to the superscript
        exponent = parser._parse_exponent()
        self.assertEqual(exponent, 2)

        # Caret exponent
        tokens = [
            UnitToken(TokenType.UNIT, "m"),
            UnitToken(TokenType.EXP, "^"),
            UnitToken(TokenType.NUMBER, "3"),
            UnitToken(TokenType.EOF, ""),
        ]
        parser = NotationParser(iter(tokens), BaseExponentEntity)
        parser.eat(TokenType.UNIT)  # Advance to the caret
        exponent = parser._parse_exponent()
        self.assertEqual(exponent, 3)

        # No exponent
        tokens = [
            UnitToken(TokenType.UNIT, "m"),
            UnitToken(TokenType.MUL, "·"),  # Not an exponent
            UnitToken(TokenType.EOF, ""),
        ]
        parser = NotationParser(iter(tokens), BaseExponentEntity)
        parser.eat(TokenType.UNIT)  # Advance to the multiplication
        exponent = parser._parse_exponent()
        self.assertIsNone(exponent)


if __name__ == '__main__':
    unittest.main()
