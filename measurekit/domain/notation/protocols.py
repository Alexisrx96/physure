"""Protocols for symbolic entities with exponents.

Defines the ExponentEntityProtocol which specifies the required interface
for entities with exponents.
"""

from __future__ import annotations

from typing_extensions import Protocol

from measurekit.domain.notation.typing import ExponentsDict


class ExponentEntityProtocol(Protocol):
    """Protocol for entities that have exponents.

    This protocol defines the required methods and properties for any class
    that represents an entity with exponents, such as Dimension or
    CompoundUnit.
    """

    @property
    def exponents(self) -> ExponentsDict:
        """Returns the exponents dictionary representing the entity."""
        ...

    def __init__(self, exponents: ExponentsDict) -> None:
        """Initializes the entity with a dictionary of exponents."""
        ...

    def __mul__(self, other: ExponentEntityProtocol) -> ExponentEntityProtocol:
        """Multiplies two exponent entities together."""
        ...

    def __truediv__(
        self, other: ExponentEntityProtocol
    ) -> ExponentEntityProtocol:
        """Divides one exponent entity by another."""
        ...

    def __pow__(self, power: float) -> ExponentEntityProtocol:
        """Raises the exponent entity to a given power."""
        ...

    def __eq__(self, other: object) -> bool:
        """Checks equality between two exponent entities."""
        ...

    def __hash__(self) -> int:
        """Returns a hash value for the exponent entity."""
        raise TypeError
