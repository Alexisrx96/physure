from collections.abc import Iterator
from typing import (
    Union,
)

from notation.lexer import (
    TokenType,
    UnitToken,
    parse_superscript,
)
from notation.protocols import ExponentEntityProtocol
from notation.token_buffer import TokenBuffer


class NotationParser:
    def __init__(
        self,
        tokens: Iterator["UnitToken"],
        entity_cls: type[ExponentEntityProtocol],
    ) -> None:
        self.tokens = TokenBuffer(tokens)
        self.entity_cls = entity_cls

    @property
    def current(self) -> "UnitToken":
        return self.tokens.current()

    def eat(self, token_type: "TokenType") -> "UnitToken":
        token = self.current
        if token.type == token_type:
            self.tokens.advance()
            return token
        else:
            raise ValueError(f"Expected {token_type}, but got {token.type}.")

    def parse(self) -> ExponentEntityProtocol:
        result = self.expr()
        if self.current.type != TokenType.EOF:
            raise ValueError(
                "Unexpected token at the end of the "
                f"expression: {self.current.type}"
            )
        return result

    def expr(self) -> ExponentEntityProtocol:
        result = self.term()
        while self.current.type in (TokenType.MUL, TokenType.DIV):
            op = self.eat(self.current.type)
            result = (
                result * self.term()
                if op.type == TokenType.MUL
                else result / self.term()
            )
        return result

    def term(self) -> ExponentEntityProtocol:
        result = self.factor()
        if self.current.type == TokenType.EXP:
            self.eat(TokenType.EXP)
            exponent = int(self.eat(TokenType.NUMBER).value)
            result = result**exponent
        if (
            self.current.type == TokenType.UNIT
            and self.current.value[-1].isdigit()
        ):
            exponent = int(self.current.value[-1])
            self.eat(TokenType.UNIT)
            result = result**exponent
        return result

    def factor(self) -> ExponentEntityProtocol:
        token = self.current

        if token.type == TokenType.UNIT:
            self.eat(TokenType.UNIT)
            # Check if the token value contains digits at the
            # end (like "m2" or "s-1")
            unit_value = token.value
            exponent_value = None

            # Extract unit and exponent if the token has a numeric suffix
            if any(c.isdigit() for c in unit_value) or "-" in unit_value:
                # Find where the numeric part starts
                for i, char in enumerate(unit_value):
                    if char.isdigit() or (
                        char == "-"
                        and i > 0
                        and i < len(unit_value) - 1
                        and unit_value[i + 1].isdigit()
                    ):
                        exponent_value = unit_value[i:]
                        unit_value = unit_value[:i]
                        break

            # Create the base unit
            base_unit = self.entity_cls({unit_value: 1})

            # Apply the embedded exponent if found
            if exponent_value is not None:
                try:
                    return base_unit ** int(exponent_value)
                except ValueError:
                    pass  # If parsing fails, fall back to normal behavior

            # Normal exponent handling
            exponent = self._parse_exponent()
            return base_unit**exponent if exponent is not None else base_unit

        if token.type == TokenType.LPAREN:
            self.eat(TokenType.LPAREN)
            result = self.expr()
            self.eat(TokenType.RPAREN)
            exponent = self._parse_exponent()
            return result**exponent if exponent is not None else result

        if token.type == TokenType.NUMBER and token.value == "1":
            self.eat(TokenType.NUMBER)
            if self.current.type == TokenType.DIV:
                self.eat(TokenType.DIV)
                return self.entity_cls({}) / self.factor()
            return self.entity_cls({})

        raise ValueError(f"Unexpected token: {token.type} ({token.value})")

    def _parse_exponent(self) -> Union[int, float, None]:
        if self.current.type == TokenType.SUP:
            token = self.eat(TokenType.SUP)
            return parse_superscript(token.value)
        elif self.current.type == TokenType.EXP:
            self.eat(TokenType.EXP)
            if self.current.type == TokenType.NUMBER:
                token = self.eat(TokenType.NUMBER)
                return int(token.value)
            raise ValueError(
                "Expected number after exponent operator,"
                f" got {self.current.type}"
            )
        return None
