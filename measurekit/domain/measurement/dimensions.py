"""Defines the `Dimension` class for representing physical dimensions.

A physical dimension is a fundamental property of a quantity, such as Length,
Mass, or Time. This module provides the `Dimension` class, which represents
these concepts as a combination of base dimensions raised to certain powers
(e.g., Velocity is Length/Time or L¹·T⁻¹). This class is a cornerstone of the
library, enabling it to check for dimensional consistency in calculations.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import ClassVar, Final, cast

from typing_extensions import Self

from measurekit.application.parsing import (
    parse_unit_string,
    to_superscript,
)
from measurekit.domain.exceptions import DimensionError
from measurekit.domain.notation.base_entity import BaseExponentEntity
from measurekit.domain.notation.protocols import ExponentEntityProtocol
from measurekit.domain.notation.typing import ExponentsDict

_DIMENSION_NAME_REGISTRY: dict[Dimension | None, str] = {}

DIMENSIONLESS = None
_PREFIX_DENYLIST: set[str] = set()

# Canonical Basis for SI plus supplementary (Angle, Money):
# [Length, Mass, Time, Current, Temperature, Amount, Luminous, Angle, Money]
SI_ORDER: Final[tuple[str, ...]] = (
    "L",
    "M",
    "T",
    "I",
    "O",
    "N",
    "J",
    "A",
    "$",
)
SI_INDICES: Final[dict[str, int]] = {s: i for i, s in enumerate(SI_ORDER)}
DIM_COUNT: Final[int] = len(SI_ORDER)
ZERO_VECTOR: Final[tuple[int, ...]] = (0,) * DIM_COUNT


def block_prefixes_for_dimension_symbol(symbol: str) -> None:
    """Adds a dimension symbol to the prefix denylist."""
    _PREFIX_DENYLIST.add(symbol)


def prefixes_allowed_for_dimension_symbol(symbol: str) -> bool:
    """Checks if a dimension symbol is allowed to have prefixes."""
    return symbol not in _PREFIX_DENYLIST


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
class Dimension(BaseExponentEntity):
    """Represents a physical dimension using a fixed-length vector.

    Canonical Order:
    [Length, Mass, Time, Current, Temperature, Amount, Luminous]
    Symbols: [L, M, T, I, O, N, J]

    Attributes:
    ----------
    _vector : Tuple[int, ...]
        Tuple of integers storing the exponents of the base dimensions.
    """

    _vector: tuple[int, ...]
    _analytical_representation: str = field(init=False, repr=False)
    _display_exponents: dict = field(init=False, repr=False)

    _cache: ClassVar[dict[tuple[int, ...], Dimension]] = {}
    _base_dimensions: ClassVar[list[str]] = list(SI_ORDER)

    def __new__(
        cls, exponents: ExponentsDict | tuple[int, ...] | None = None
    ) -> Self:
        """Creates or retrieves a cached Dimension instance."""
        if isinstance(exponents, tuple):
            vector = exponents
        elif exponents is None:
            vector = ZERO_VECTOR
        else:
            vector = cls._normalize_exponents(exponents)

        if vector in cls._cache:
            return cast(Self, cls._cache[vector])

        # Initialize instance without calling BaseExponentEntity.__new__
        # to avoid its default dictionary-based logic.
        instance = super(BaseExponentEntity, cls).__new__(cls)

        analytical_rep, display_exp_dict = cls._calculate_representation(
            vector
        )

        object.__setattr__(instance, "_vector", vector)
        object.__setattr__(
            instance, "_analytical_representation", analytical_rep
        )
        object.__setattr__(instance, "_display_exponents", display_exp_dict)
        # Backward compatibility:
        # set the 'exponents' field from BaseExponentEntity
        object.__setattr__(instance, "exponents", display_exp_dict)

        cls._cache[vector] = cast(Dimension, instance)
        return cast(Self, instance)

    @classmethod
    def _calculate_representation(
        cls, vector: tuple[int, ...]
    ) -> tuple[str, dict[str, int]]:
        """Calculates the analytical string and display dict for a vector."""
        normalized = {SI_ORDER[i]: v for i, v in enumerate(vector) if v != 0}

        if not normalized:
            analytical_rep = "Dimensionless"
        else:
            parts = []
            for k in SI_ORDER:
                idx = SI_INDICES[k]
                exp = vector[idx]
                if exp == 0:
                    continue
                parts.append(k if exp == 1 else f"{k}{to_superscript(exp)}")
            analytical_rep = "·".join(parts)

        display_exp_dict = {k: int(v) for k, v in normalized.items()}
        return analytical_rep, display_exp_dict

    @classmethod
    def _normalize_exponents(cls, exponents: ExponentsDict) -> tuple[int, ...]:
        """Converts an exponents dictionary into a canonical vector."""
        v_list = [0] * DIM_COUNT
        for k, v in exponents.items():
            if k in SI_INDICES:
                v_list[SI_INDICES[k]] = int(v)
            elif v != 0:
                raise DimensionError(f"Unknown base dimension symbol: {k}")
        return tuple(v_list)

    def __init__(
        self, exponents: ExponentsDict | tuple[int, ...] | None = None
    ):
        """Initializes the dimension. Logic is handled in __new__."""
        pass

    def __hash__(self) -> int:
        """Returns a hash value for the dimension based on its vector."""
        return hash(self._vector)

    def __eq__(self, other: object) -> bool:
        """Checks equality by comparing vector representations."""
        if isinstance(other, Dimension):
            return self._vector == other._vector
        if isinstance(other, BaseExponentEntity):
            # Fallback for mixed comparisons if necessary
            return self.exponents == other.exponents
        return NotImplemented

    # --- Vector Arithmetic ---

    def __mul__(self, other: ExponentEntityProtocol) -> Dimension:
        """Multiplies two dimensions by adding their exponent vectors."""
        if not isinstance(other, Dimension):
            # If multiplying by something else (e.g. CompoundUnit),
            # let it handle it
            return NotImplemented
        new_vector = tuple(a + b for a, b in zip(self._vector, other._vector))
        return Dimension(new_vector)

    def __truediv__(self, other: ExponentEntityProtocol) -> Dimension:
        """Divides two dimensions by subtracting their exponent vectors."""
        if not isinstance(other, Dimension):
            return NotImplemented
        new_vector = tuple(a - b for a, b in zip(self._vector, other._vector))
        return Dimension(new_vector)

    def __pow__(self, power: float) -> Dimension:
        """Raises a dimension to a power by scaling its exponent vector."""
        if not isinstance(power, (int, float)):
            return NotImplemented
        # Dimensions typically have integer exponents
        new_vector = tuple(int(v * power) for v in self._vector)
        return Dimension(new_vector)

    # --- Properties for SI Base Dimensions ---

    @property
    def length(self) -> int:
        """Exponent of the Length dimension."""
        return self._vector[0]

    @property
    def mass(self) -> int:
        """Exponent of the Mass dimension."""
        return self._vector[1]

    @property
    def time(self) -> int:
        """Exponent of the Time dimension."""
        return self._vector[2]

    @property
    def current(self) -> int:
        """Exponent of the Electric Current dimension."""
        return self._vector[3]

    @property
    def temperature(self) -> int:
        """Exponent of the Thermodynamic Temperature dimension."""
        return self._vector[4]

    @property
    def amount(self) -> int:
        """Exponent of the Amount of Substance dimension."""
        return self._vector[5]

    @property
    def luminous(self) -> int:
        """Exponent of the Luminous Intensity dimension."""
        return self._vector[6]

    @property
    def analytical_representation(self) -> str:
        """Returns the pre-calculated analytical dimension description."""
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
        return (
            "<Dimension: "
            f"{self.analytical_representation} {self._display_exponents}>"
        )

    @property
    def is_dimensionless(self) -> bool:
        """Checks if the dimension is dimensionless."""
        return all(v == 0 for v in self._vector)

    @classmethod
    def set_base_dimensions(cls, bases: list[str]):
        """Sets the base dimensions that the system will recognize.

        Note: The vector representation strictly follows the SI order.
        If a different system is used, this method will update the registry,
        but the vector internally will still map known SI symbols.
        """
        cls._base_dimensions = bases

    @classmethod
    def from_string(cls, dim_str: str) -> Dimension:
        """Creates a Dimension object from a string."""
        # Use memoized parser eventually, or just use get_dimension
        return get_dimension(dim_str)


def get_dimension(unit_expression: str) -> Dimension:
    """Returns a Dimension object parsed from a unit expression string."""
    return cast(Dimension, parse_unit_string(unit_expression, Dimension))
