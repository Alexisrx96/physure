"""Protocols, type aliases, and a base class for exponent entities."""

from __future__ import annotations

from dataclasses import dataclass
from functools import singledispatchmethod
from typing import Any, Protocol

from physure.core.formatting import to_superscript

# Exponents dictionary type alias: maps base unit or dimension symbol to power/exponent
ExponentsDict = dict[str, Any]


class ExponentEntityProtocol(Protocol):
    """Protocol for entities that have exponents (such as Dimension or CompoundUnit)."""

    @property
    def exponents(self) -> ExponentsDict:
        """Mapping of base symbol to exponent."""
        ...

    def __init__(self, exponents: ExponentsDict) -> None: ...

    def __mul__(
        self, other: ExponentEntityProtocol
    ) -> ExponentEntityProtocol: ...

    def __truediv__(
        self, other: ExponentEntityProtocol
    ) -> ExponentEntityProtocol: ...

    def __pow__(self, power: float) -> ExponentEntityProtocol: ...

    def __eq__(self, other: object) -> bool: ...

    def __hash__(self) -> int:
        raise TypeError


@dataclass(frozen=True)
class BaseExponentEntity:
    """Base class for entities represented by a dictionary of exponents."""

    exponents: ExponentsDict

    def __new__(cls, exponents: ExponentsDict) -> Any:
        """Create the instance, dropping zero exponents."""
        normalized = {k: v for k, v in exponents.items() if v}
        instance = super().__new__(cls)
        object.__setattr__(instance, "exponents", normalized)
        return instance

    def __init__(self, exponents: ExponentsDict) -> None:
        # No-op: __new__ already builds the frozen instance above; a dataclass
        # would otherwise generate an __init__ that clobbers it.
        pass

    def __mul__(self, other: Any) -> Any:
        if isinstance(other, (int, float, complex)):
            return self
        if not hasattr(other, "exponents"):
            return NotImplemented

        new_exponents = self.exponents.copy()
        for key, exp in other.exponents.items():
            new_exponents[key] = new_exponents.get(key, 0) + exp
        return type(self)(new_exponents)

    def __truediv__(self, other: Any) -> Any:
        if isinstance(other, (int, float, complex)):
            return self
        if not hasattr(other, "exponents"):
            return NotImplemented

        new_exponents = self.exponents.copy()
        for key, exp in other.exponents.items():
            new_exponents[key] = new_exponents.get(key, 0) - exp
        return type(self)(new_exponents)

    def __pow__(self, power: float) -> Any:
        return type(self)({k: v * power for k, v in self.exponents.items()})

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, BaseExponentEntity):
            return NotImplemented
        return self.exponents == other.exponents

    def __hash__(self) -> int:
        return hash(frozenset(self.exponents.items()))

    def __repr__(self) -> str:
        return str(self.exponents)

    def __str__(self) -> str:
        numerator, denominator = [], []
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
    def __rtruediv__(self, other: complex) -> Any:
        return type(self)({u: -exp for u, exp in self.exponents.items()})
