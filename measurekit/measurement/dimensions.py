# measurekit/measurement/dimensions.py
"""This module defines the Dimension class and related utility functions.

A Dimension represents a physical dimension as a mapping of base symbols
(e.g., L, M, T) to their integer exponents. It provides methods for
dimensional analysis through arithmetic operations. The module also includes
a registry for naming dimensions and functions for parsing dimension strings.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import ClassVar, cast

from measurekit.notation.lexer import generate_tokens, to_superscript
from measurekit.notation.parsers import (
    NotationParser,
)
from measurekit.notation.typing import ExponentsDict

_DIMENSION_NAME_REGISTRY: dict[Dimension | None, str] = {}

DIMENSIONLESS = None
_PREFIX_BLOCKLIST: set[str] = set()


def block_prefixes_for_dimension_symbol(symbol: str) -> None:
    """Adds a dimension symbol to the prefix blocklist."""
    _PREFIX_BLOCKLIST.add(symbol)


def prefixes_allowed_for_dimension_symbol(symbol: str) -> bool:
    """Checks if a dimension symbol is allowed to have prefixes."""
    return symbol not in _PREFIX_BLOCKLIST


def register_dimension(dimension: Dimension, name: str):
    """Registers a descriptive name for a Dimension instance.

    This function populates the central registry so that dimensions
    can be represented in a more human-readable way.

    Args:
        dimension: The Dimension object to name (e.g., Dimension({'L': 1})).
        name: The human-readable name (e.g., "Length").
    """
    _DIMENSION_NAME_REGISTRY[dimension] = name


@dataclass(frozen=True)
class Dimension:
    """Represents a physical dimension as a mapping of base symbols (e.g., L, M, T) to their exponents.

    Attributes
    ----------
    exponents : ExponentsDict
        Dictionary mapping base dimension symbols to their exponents.

    Class Attributes
    ---------------
    _cache : dict[tuple, Dimension]
        Cache for unique Dimension instances.
    _base_dimensions : list[str]
        List of recognized base dimension symbols.

    Methods
    -------
    __mul__(other: Dimension) -> Dimension
        Multiplies two Dimension objects.
    __truediv__(other: Dimension) -> Dimension
        Divides two Dimension objects.
    __pow__(power: float) -> Dimension
        Raises the Dimension to a power.
    is_dimensionless() -> bool
        Checks if the dimension is dimensionless.
    set_base_dimensions(bases: list[str])
        Sets the base dimensions for the system.
    from_string(dim_str: str) -> Dimension
        Creates a Dimension from a string representation.
    """

    _cache: ClassVar[dict[tuple, Dimension]] = {}
    _base_dimensions: ClassVar[list[str]] = []
    exponents: ExponentsDict

    __slots__ = (
        "exponents",
        "_analytical_representation",
        "_display_exponents",
    )

    def __new__(cls, exponents: ExponentsDict) -> Dimension:
        normalized = {k: float(v) for k, v in exponents.items() if v != 0}
        key = tuple(sorted(normalized.items()))
        if key in cls._cache:
            return cls._cache[key]

        instance = super().__new__(cls)
        object.__setattr__(instance, "exponents", normalized)

        # Pre-calculate the analytical representation
        if not normalized:
            analytical_rep = "Dimensionless"
        else:
            parts = []
            for k, exp in sorted(normalized.items()):
                display_exp = int(exp) if exp == int(exp) else exp
                if display_exp == 1:
                    parts.append(k)
                else:
                    parts.append(f"{k}{to_superscript(display_exp)}")
            analytical_rep = "·".join(parts)

        # Pre-calculate the display exponents dictionary
        display_exp_dict = {
            k: int(v) if v == int(v) else v for k, v in normalized.items()
        }

        object.__setattr__(
            instance, "_analytical_representation", analytical_rep
        )
        object.__setattr__(instance, "_display_exponents", display_exp_dict)

        cls._cache[key] = instance
        return instance

    def __init__(self, exponents: ExponentsDict) -> None:
        pass

    def __mul__(self, other: Dimension) -> Dimension:
        new_exponents = {**self.exponents}
        for key, exp in other.exponents.items():
            new_exponents[key] = new_exponents.get(key, 0) + exp
        return Dimension(new_exponents)

    def __truediv__(self, other: Dimension) -> Dimension:
        new_exponents = {**self.exponents}
        for key, exp in other.exponents.items():
            new_exponents[key] = new_exponents.get(key, 0) - exp
        return Dimension(new_exponents)

    def __pow__(self, power: float) -> Dimension:
        return Dimension({k: v * power for k, v in self.exponents.items()})

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Dimension):
            return NotImplemented
        return self.exponents == other.exponents

    def __hash__(self) -> int:
        return hash(tuple(sorted(self.exponents.items())))

    def __rtruediv__(self, other: complex) -> Dimension:
        """Allows division where a scalar is divided by a Dimension (inverse)."""
        return Dimension({k: -v for k, v in self.exponents.items()})

    @property
    def analytical_representation(self) -> str:
        """Returns the pre-calculated analytical description of the dimension."""
        return self._analytical_representation

    @property
    def name(self) -> str | None:
        """Returns the registered descriptive name for the dimension."""
        return _DIMENSION_NAME_REGISTRY.get(self)

    def __str__(self) -> str:
        """The main representation of the dimension is its analytical form."""
        return self.analytical_representation

    def __repr__(self) -> str:
        """Detailed representation for debugging with pre-calculated values."""
        registered_name = self.name
        if registered_name:
            return (
                f"<Dimension: {self.analytical_representation} "
                f"({registered_name}) {self._display_exponents}>"
            )
        return f"<Dimension: {self.analytical_representation} {self._display_exponents}>"

    def is_dimensionless(self) -> bool:
        """Checks if the dimension is dimensionless."""
        return not self.exponents

    @classmethod
    def set_base_dimensions(cls, bases: list[str]):
        """Sets the base dimensions that the system will recognize.

        This method should be called during system initialization.

        Args:
            bases (list[str]): A list of the base dimension symbols.
                               e.g., ['L', 'M', 'T', 'I', 'O', 'J', 'N']
        """
        cls._base_dimensions = bases

    @classmethod
    def from_string(cls, dim_str: str) -> Dimension:
        """Creates a Dimension object from a string.

        Args:
            dim_str (str): The string representing the dimension.
                           e.g., "L", "T", "L·T⁻²", "M*L^2 / T^2"

        Returns:
            Dimension: A new corresponding Dimension object.
        """
        tokens = generate_tokens(dim_str)

        parser = NotationParser(tokens, cls)  # type: ignore

        parsed_dim = parser.parse()
        return cast(Dimension, parsed_dim)


def get_dimension(unit_expression: str) -> Dimension:
    """Returns a Dimension object parsed from a unit expression string.

    Args:
        unit_expression (str): The string representing the unit expression.

    Returns:
        Dimension: The corresponding Dimension object.
    """
    tokens = generate_tokens(unit_expression)
    parser = NotationParser(tokens, Dimension)
    return cast(Dimension, parser.parse())
