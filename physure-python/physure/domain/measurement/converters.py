"""Unit conversion strategies."""

import math
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from fractions import Fraction

_LN10 = math.log(10.0)


class UnitConverter(ABC):
    """Abstract base class for any unit conversion."""

    @abstractmethod
    def to_base(self, value: float) -> float:
        """Converts a value from this unit to the system's base unit."""
        pass

    @property
    @abstractmethod
    def is_linear(self) -> bool:
        """Returns True if the conversion is linear (y = ax)."""
        pass

    @abstractmethod
    def from_base(self, value: float) -> float:
        """Converts a value from the base unit to this unit."""
        pass

    def convert(self, value: float, from_base: bool) -> float:
        """Performs the generic conversion in the given direction."""
        return self.from_base(value) if from_base else self.to_base(value)

    def to_base_derivative(self, value: float) -> float:
        """d(to_base)/d(value), used to propagate uncertainty.

        Central-difference fallback for custom converters; subclasses
        override with exact derivatives.
        """
        h = 1e-7
        return (self.to_base(value + h) - self.to_base(value - h)) / (2 * h)

    def from_base_derivative(self, value: float) -> float:
        """d(from_base)/d(value), used to propagate uncertainty."""
        h = 1e-7
        return (self.from_base(value + h) - self.from_base(value - h)) / (
            2 * h
        )


@dataclass(frozen=True)
class LinearConverter(UnitConverter):
    """For most units: y = ax (e.g. meters to kilometers)."""

    scale: float
    # Exact rational value of the declared scale (from the .conf string),
    # or None when it is not exactly representable. Only consumed by
    # CompoundUnit._compound_factor_exact; all value math stays float.
    exact: Fraction | None = field(default=None, compare=False, repr=False)

    @property
    def is_linear(self) -> bool:
        """Returns True: plain scale conversion."""
        return True

    def to_base(self, value: float) -> float:
        """Converts value to base unit."""
        return value * self.scale

    def from_base(self, value: float) -> float:
        """Converts value from base unit."""
        return value / self.scale

    def to_base_derivative(self, value: float) -> float:
        """Exact derivative: constant scale."""
        return self.scale

    def from_base_derivative(self, value: float) -> float:
        """Exact derivative: constant 1/scale."""
        return 1.0 / self.scale


@dataclass(frozen=True)
class OffsetConverter(UnitConverter):
    """For units with an offset: y = ax + b (e.g. Celsius to Kelvin)."""

    scale: float
    offset: float

    @property
    def is_linear(self) -> bool:
        """Returns False: conversion has an offset."""
        return False

    def to_base(self, value: float) -> float:
        """Converts value to base unit."""
        return (value * self.scale) + self.offset

    def from_base(self, value: float) -> float:
        """Converts value from base unit."""
        return (value - self.offset) / self.scale

    def to_base_derivative(self, value: float) -> float:
        """Exact derivative: the offset vanishes."""
        return self.scale

    def from_base_derivative(self, value: float) -> float:
        """Exact derivative: the offset vanishes."""
        return 1.0 / self.scale


# Alias for backward compatibility
AffineConverter = OffsetConverter


@dataclass(frozen=True)
class LogarithmicConverter(UnitConverter):
    """For logarithmic units: y = factor * log10(x / reference).

    Specifically for Decibels (dB): dB = 10 * log10(P / P_ref) or
    20 * log10(V / V_ref).
    We store the factor (10 or 20) and the reference value.
    """

    factor: float
    reference: float = 1.0

    @property
    def is_linear(self) -> bool:
        """Returns False: conversion is logarithmic."""
        return False

    def to_base(self, value: float) -> float:
        """Converts logarithmic value to linear base value."""
        return self.reference * (10 ** (value / self.factor))

    def from_base(self, value: float) -> float:
        """Converts linear base value to logarithmic value."""
        # ponytail: `value: float` is a simplification for the common case;
        # callers may pass numpy arrays at runtime, which this branch
        # handles even though it's unreachable for the declared type.
        if isinstance(  # pyright: ignore[reportUnnecessaryIsInstance]
            value, (int, float)
        ):
            return self.factor * math.log10(value / self.reference)
        # Array input (numpy is only required on this path)
        import numpy as np

        return self.factor * np.log10(  # pyright: ignore[reportUnreachable]
            value / self.reference
        )

    def to_base_derivative(self, value: float) -> float:
        """Exact derivative: to_base(v) * ln(10) / factor."""
        return self.to_base(value) * _LN10 / self.factor

    def from_base_derivative(self, value: float) -> float:
        """Exact derivative: factor / (value * ln(10))."""
        return self.factor / (value * _LN10)
