# notation/lexer.py
"""Lexical analyzer for scientific notations.

It provides some common notations for physical quantities and units, such as
superscript and subscript notations for exponents and units.
"""

import re
from collections.abc import Generator
from dataclasses import dataclass
from enum import Enum, auto

# Superscript and Subscript Mapping
_SUPERSCRIPT_CHARS = "⁰¹²³⁴⁵⁶⁷⁸⁹⋅⁻"
_SUPERSCRIPT_MAP = str.maketrans("0123456789.-", _SUPERSCRIPT_CHARS)
_SUPERSCRIPT_REVERSE_MAP = str.maketrans(_SUPERSCRIPT_CHARS, "0123456789.-")
_SUBSCRIPT_MAP = str.maketrans("0123456789-", "₀₁₂₃₄₅₆₇₈₉₋")
_SUBSCRIPT_REVERSE_MAP = str.maketrans("₀₁₂₃₄₅₆₇₈₉₋", "0123456789-")


def subscript_to_ascii(s: str) -> str:
    """Normalize Unicode subscript digits to ASCII.

    Example:
        >>> subscript_to_ascii("H₂O")
        'H2O'
    """
    return s.translate(_SUBSCRIPT_REVERSE_MAP)


def to_superscript(n: str | float) -> str:
    """Convert an integer to its superscript representation."""
    result = str(n).translate(_SUPERSCRIPT_MAP)
    # Remove characters that weren't translated
    return "".join(c for c in result if c in _SUPERSCRIPT_CHARS)


def to_subscript(n: str | float) -> str:
    """Convert an integer to its subscript representation."""
    result = str(n).translate(_SUBSCRIPT_MAP)
    # Remove characters that weren't translated
    return "".join(c for c in result if c in "₀₁₂₃₄₅₆₇₈₉₋")


def parse_superscript(sup: str) -> int | float:
    """Convert a superscript number to an integer."""
    try:
        return int(sup.translate(_SUPERSCRIPT_REVERSE_MAP))
    except ValueError:
        try:
            return float(sup.translate(_SUPERSCRIPT_REVERSE_MAP))
        except ValueError:
            return 0


# Token Types
class TokenType(Enum):
    """Enumeration of possible token types in a unit notation string."""

    UNIT = auto()
    SUP = auto()
    MUL = auto()
    DIV = auto()
    EXP = auto()
    NUMBER = auto()
    LPAREN = auto()
    RPAREN = auto()
    EOF = auto()

    def __str__(self):
        """Returns the name of the token type."""
        return self.name


# Token Representation
@dataclass(frozen=True)
class UnitToken:
    """A single token parsed from a unit notation string."""

    type: TokenType
    """The type of the token."""
    value: str
    """The value of the token."""


# Lexer for Notation Parsing
_TOKEN_SPEC = [
    ("NUMBER", r"[+-]?\d+(\.\d*)?([eE][+-]?\d+)?"),
    ("UNIT_WITH_EXP", r"[a-zA-Z_°Ωµ$₀₁₂₃₄₅₆₇₈₉]+[-]?[0-9]+"),
    ("UNIT", r"[a-zA-Z_°Ωµ$₀₁₂₃₄₅₆₇₈₉]+"),
    ("SUP", r"[⁰¹²³⁴⁵⁶⁷⁸⁹⁻]+"),
    ("EXP", r"\*\*|\^"),
    ("MUL", r"[\*·]"),
    ("DIV", r"/"),
    ("LPAREN", r"\("),
    ("RPAREN", r"\)"),
    ("SKIP", r"\s+"),
    ("MISMATCH", r"."),
]
_TOKEN_MAP = {
    "NUMBER": TokenType.NUMBER,
    "UNIT_WITH_EXP": TokenType.UNIT,
    "UNIT": TokenType.UNIT,
    "SUP": TokenType.SUP,
    "MUL": TokenType.MUL,
    "DIV": TokenType.DIV,
    "EXP": TokenType.EXP,
    "LPAREN": TokenType.LPAREN,
    "RPAREN": TokenType.RPAREN,
}
_REGEX = re.compile(
    "|".join(f"(?P<{name}>{pattern})" for name, pattern in _TOKEN_SPEC)
)


def generate_tokens(input_string: str) -> Generator[UnitToken, None, None]:
    """Generate a sequence of tokens from the given input string.

    Args:
        input_string: The string to tokenize.

    Yields:
        A sequence of tokens, where each token is a :class:`UnitToken`.

    Raises:
        ValueError: If an unrecognized character is encountered.
    """
    for match in _REGEX.finditer(input_string):
        kind = match.lastgroup
        value = match.group()
        if kind in _TOKEN_MAP:
            yield UnitToken(_TOKEN_MAP[kind], value)
        elif kind == "SKIP":
            continue
        elif kind == "MISMATCH":
            raise ValueError(f"Unexpected character {value!r}")
    yield UnitToken(TokenType.EOF, "")
