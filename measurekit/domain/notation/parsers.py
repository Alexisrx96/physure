"""A recursive descent parser for symbolic unit and dimension expressions.

This module provides a NotationParser that transforms a stream of tokens from
the lexer into a structured entity representing a physical unit or dimension.
It follows a simple grammar to handle multiplication, division, and
exponentiation of symbolic entities.

The grammar is defined as follows:
- expr  : term ((MUL | DIV) term)*
- term  : factor
- factor: (UNIT | LPAREN expr RPAREN) (EXP NUMBER | SUP)? | NUMBER

This structure allows the parser to correctly handle operator precedence and
nested expressions. The parser is designed to be generic and can work with any
entity class that adheres to the ExponentEntityProtocol.
"""

from __future__ import annotations

from collections.abc import Iterator

from measurekit.domain.notation.lexer import (
    TokenType,
    UnitToken,
    parse_superscript,
)
from measurekit.domain.notation.protocols import ExponentEntityProtocol
from measurekit.domain.notation.token_buffer import TokenBuffer


class NotationParser:
    """A parser that builds a symbolic entity from a stream of tokens.

    This class takes an iterator of tokens and an entity class (like
    CompoundUnit or Dimension) and constructs an instance of that class by
    parsing the tokens according to a defined grammar.

    Attributes:
        tokens (TokenBuffer): A buffer to allow lookahead in the token stream.
        entity_cls (type[ExponentEntityProtocol]): The class to use for
                                                  creating the final parsed
                                                  entity.
    """

    def __init__(
        self,
        tokens: Iterator[UnitToken],
        entity_cls: type[ExponentEntityProtocol],
    ) -> None:
        """Initializes the parser with a token stream and an entity class.

        Args:
            tokens: An iterator that yields UnitToken objects.
            entity_cls: A class that conforms to the ExponentEntityProtocol,
                        used to build the final object.
        """
        self.tokens = TokenBuffer(tokens)
        self.entity_cls = entity_cls

    @property
    def current(self) -> UnitToken:
        """Returns the current token from the buffer without consuming it."""
        return self.tokens.current()

    def eat(self, token_type: TokenType) -> UnitToken:
        """Consumes the current token if it matches the expected type.

        If the current token matches the `token_type`, the buffer is advanced.
        If not, a ValueError is raised.

        Args:
            token_type: The expected type of the current token.

        Returns:
            The consumed token.

        Raises:
            ValueError: If the current token's type does not match the
                        expected type.
        """
        token = self.current
        if token.type == token_type:
            self.tokens.advance()
            return token
        else:
            raise ValueError(f"Expected {token_type}, but got {token.type}")

    def parse(self) -> ExponentEntityProtocol:
        """Parses the entire token stream and returns the final entity.

        This is the main entry point for the parser. It parses the root
        expression and ensures that no unexpected tokens are left at the end.

        Returns:
            An instance of the entity class representing the parsed expression.

        Raises:
            ValueError: If there are unexpected tokens remaining after the main
                        expression has been parsed.
        """
        result = self.expr()
        # After parsing, we should be at the end of the input.
        if self.current.type != TokenType.EOF:
            raise ValueError(
                "Unexpected token at the end of the "
                f"expression: {self.current.type}"
            )
        return result

    def expr(self) -> ExponentEntityProtocol:
        """Parses a full expression with multiplication and division."""
        if self.current.type == TokenType.EOF:
            return self.entity_cls({})
        result = self.term()
        # Handle a sequence of multiplications or divisions.
        while self.current.type in (TokenType.MUL, TokenType.DIV):
            op = self.eat(self.current.type)
            if op.type == TokenType.MUL:
                result *= self.term()
            else:
                result /= self.term()
        return result

    def term(self) -> ExponentEntityProtocol:
        """Parses a 'term', which in this grammar is just a 'factor'.

        The grammar is simplified so that exponentiation (handled in `factor`)
        has higher precedence than multiplication/division (handled in `expr`).
        """
        return self.factor()

    def factor(self) -> ExponentEntityProtocol:
        """Parses a factor, the highest precedence element in the grammar.

        A factor can be a unit, a parenthesized expression, or the number '1'.
        It also handles any attached exponents.
        """
        token = self.current

        if token.type == TokenType.UNIT:
            self.eat(TokenType.UNIT)
            original_value = token.value
            unit_value = original_value
            exponent_value = None

            # Attempt to split the token into a unit name and an embedded
            # exponent
            # (e.g., "m2" -> "m", "2" or "s-1" -> "s", "-1").
            split_index = -1
            for i, char in enumerate(original_value):
                # An exponent starts with a digit or a hyphen
                # (but not at the beginning).
                if i > 0 and (char.isdigit() or char == "-"):
                    split_index = i
                    break

            if split_index != -1:
                exponent_value = original_value[split_index:]
                unit_value = original_value[:split_index]

            # Create an entity for the base unit part.
            base_unit = self.entity_cls({unit_value: 1})

            # If an embedded exponent was found, try to apply it.
            if exponent_value is not None:
                try:
                    return base_unit ** int(exponent_value)
                except ValueError:
                    # If parsing the exponent fails (e.g., "m-s"), fall back to
                    # treating the entire original token ("m-s") as a single
                    # unit name.
                    return self.entity_cls({original_value: 1})

            # If no embedded exponent, check for a standard exponent
            # (e.g., "^2" or "²").
            exponent = self._parse_exponent()
            return base_unit**exponent if exponent is not None else base_unit

        if token.type == TokenType.LPAREN:
            self.eat(TokenType.LPAREN)
            result = self.expr()  # Recursively parse the nested expression.
            self.eat(TokenType.RPAREN)
            # Check for an exponent attached to the closing parenthesis.
            exponent = self._parse_exponent()
            return result**exponent if exponent is not None else result

        # Handle dimensionless quantities represented by the number '1'.
        if token.type == TokenType.NUMBER and token.value == "1":
            self.eat(TokenType.NUMBER)
            # Handle cases like "1/s".
            if self.current.type == TokenType.DIV:
                self.eat(TokenType.DIV)
                return self.entity_cls({}) / self.factor()
            return self.entity_cls({})

        # If the token is not a unit, parenthesis, or '1', it's an error.
        raise ValueError(f"Unexpected token: {token.type} ({token.value})")

    def _parse_exponent(self) -> int | float | None:
        """Parses an optional exponent following a factor.

        Handles both superscript (e.g., ²) and caret (e.g., ^2) notations.

        Returns:
            The numeric value of the exponent, or None if no exponent is found.

        Raises:
            ValueError: If a caret operator is not followed by a number.
        """
        # Case 1: Superscript exponent (e.g., ², ⁻¹, etc.).
        if self.current.type == TokenType.SUP:
            token = self.eat(TokenType.SUP)
            return parse_superscript(token.value)
        # Case 2: Caret exponent (e.g., ^2, **2).
        elif self.current.type == TokenType.EXP:
            self.eat(TokenType.EXP)
            if self.current.type == TokenType.NUMBER:
                token = self.eat(TokenType.NUMBER)
                return int(token.value)
            # If there's a caret but no number, it's a syntax error.
            raise ValueError(
                "Expected number after exponent operator,"
                f" got {self.current.type}"
            )
        # Case 3: No exponent found.
        return None
