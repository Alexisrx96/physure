"""Provides a base class for symbolic entities that involve exponents.

This module contains the `BaseExponentEntity` class, which implements the
common arithmetic logic (multiplication, division, exponentiation) for objects
that are represented by a dictionary of bases and their exponents. Both
`Dimension` and `CompoundUnit` inherit from this class to share this
fundamental behavior, promoting code reuse and consistency.
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import singledispatchmethod
from typing import TYPE_CHECKING, Any

from physure.core.formatting import to_superscript

if TYPE_CHECKING:
    from typing_extensions import Self

    from physure.domain.notation.typing import ExponentsDict


@dataclass(frozen=True)
class BaseExponentEntity:
    """Base class for entities with exponents."""

    exponents: ExponentsDict

    def __new__(cls, exponents: ExponentsDict) -> Self:
        """Create or retrieve a cached ExponentEntity instance."""
        normalized = {k: v for k, v in exponents.items() if v}
        instance = super().__new__(cls)
        object.__setattr__(instance, "exponents", normalized)
        return instance

    def __init__(self, exponents: ExponentsDict) -> None:
        """Initializes the entity with a dictionary of exponents."""
        pass

    def __mul__(self, other: Any) -> Self:
        """Multiplies two exponent entities together."""
        if isinstance(other, (int, float, complex)):
            return self
        if not hasattr(other, "exponents"):
            return NotImplemented

        new_exponents = self.exponents.copy()
        for key, exp in other.exponents.items():
            new_exponents[key] = new_exponents.get(key, 0) + exp
        return type(self)(new_exponents)

    def __truediv__(self, other: Any) -> Self:
        """Divides one exponent entity by another."""
        if isinstance(other, (int, float, complex)):
            return self
        if not hasattr(other, "exponents"):
            return NotImplemented

        new_exponents = self.exponents.copy()
        for key, exp in other.exponents.items():
            new_exponents[key] = new_exponents.get(key, 0) - exp
        return type(self)(new_exponents)

    def __pow__(self: Self, power: float) -> Self:
        """Raises the exponent entity to a given power."""
        return type(self)({k: v * power for k, v in self.exponents.items()})

    def __eq__(self, other: object) -> bool:
        """Checks equality between two exponent entities."""
        if not isinstance(other, BaseExponentEntity):
            return NotImplemented
        return self.exponents == other.exponents

    def __hash__(self) -> int:
        """Returns a hash value for the exponent entity."""
        return hash(frozenset(self.exponents.items()))

    def __repr__(self) -> str:
        """Detailed representation for debugging."""
        return str(self.exponents)

    def __str__(self) -> str:
        """Returns a string representation of the entity."""
        numerator, denominator = [], []
        # Sort alphabetically for a deterministic order
        for unit, exp in sorted(self.exponents.items()):
            formatted = (
                f"{unit}{to_superscript(abs(exp)) if abs(exp) != 1 else ''}"
            )
            (numerator if exp > 0 else denominator).append(formatted)
        n = "·".join(numerator)
        d = "·".join(denominator)
        if "·" in d:
            d = f"({d})"
        if d and n:
            return f"{n}/{d}"
        if d and not n:
            return f"1/{d}"
        if n and not d:
            return n
        return "1"

    @singledispatchmethod
    def __rtruediv__(self: Self, other: complex) -> Self:
        """Implements the reflected division operator.

        This is used for returning the inverse of the entity.
        """
        return type(self)({u: -exp for u, exp in self.exponents.items()})
