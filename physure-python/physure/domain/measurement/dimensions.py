"""Defines the `Dimension` class for representing physical dimensions.

A physical dimension is a fundamental property of a quantity, such as Length,
Mass, or Time. This module provides the `Dimension` class, which represents
these concepts as a combination of base dimensions raised to certain powers
(e.g., Velocity is Length/Time or L¹·T⁻¹). This class is a cornerstone of the
library, enabling it to check for dimensional consistency in calculations.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from fractions import Fraction
from types import MappingProxyType
from typing import TYPE_CHECKING, Any, ClassVar, Final, cast

from physure.core.formatting import to_superscript

if TYPE_CHECKING:
    from typing import Self

from physure._core import RationalUnit
from physure.domain.measurement.base_entity import (
    BaseExponentEntity,
    ExponentEntityProtocol,
    ExponentsDict,
)
from physure.domain.measurement.base_entity import (
    exact_power as _pair,
)

_DIMENSION_NAME_REGISTRY: dict[Dimension | None, str] = {}

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


def register_dimension(dimension: Dimension, name: str):
    """Registers a descriptive name for a Dimension instance.

    This function populates the central registry so that dimensions
    can be represented in a more human-readable way.

    Args:
        dimension: The Dimension object to name (e.g., Dimension({'L': 1})).
        name: The human-readable name (e.g., "Length").
    """
    _DIMENSION_NAME_REGISTRY[dimension] = name


_RANK: Final[dict[str, int]] = {symbol: i for i, symbol in enumerate(SI_ORDER)}


def _symbol_order(item: tuple[str, Any]) -> tuple[int, str]:
    """Sorts the SI symbols in their canonical order, then the others by name."""
    return (_RANK.get(item[0], len(_RANK)), item[0])


@dataclass(frozen=True)
class Dimension(BaseExponentEntity):
    """A physical dimension: base dimensions to rational powers.

    The powers are exact (a velocity is L·T⁻¹, a noise density L²·T^(1/2)...)
    and the algebra is the core's: the dimension is held as a unit over the
    dimension symbols, and multiplying, dividing and raising it to a power are
    the operations of the core on it.

    Canonical Order:
    [Length, Mass, Time, Current, Temperature, Amount, Luminous, Angle, Money]
    Symbols: [L, M, T, I, O, N, J, A, $]; any other symbol follows by name.

    Attributes:
    ----------
    exponents : dict[str, int | Fraction]
        The non-zero powers by symbol, in canonical order.
    """

    _key: tuple[tuple[str, tuple[int, int]], ...]
    _unit: RationalUnit = field(repr=False)
    _analytical_representation: str = field(init=False, repr=False)

    # one instance per set of powers, found by the canonical key or by the
    # items that were given (so the usual call does no arithmetic at all)
    _cache: ClassVar[dict[tuple, Dimension]] = {}
    _base_dimensions: ClassVar[list[str]] = list(SI_ORDER)

    def __new__(cls, exponents: ExponentsDict | None = None) -> Self:
        """Creates or retrieves a cached Dimension instance."""
        items = tuple(exponents.items()) if exponents else ()
        try:
            found = cls._cache.get(items)
        except TypeError:  # an unhashable power: no shortcut
            items = None
            found = None
        if found is not None:
            return cast("Self", found)

        key = tuple(
            sorted(
                (
                    (symbol, pair)
                    for symbol, value in (exponents or {}).items()
                    if (pair := _pair(value))[0] != 0
                ),
                key=_symbol_order,
            )
        )
        instance = cls._cache.get(key)
        if instance is None:
            # Initialize instance without calling BaseExponentEntity.__new__
            # to avoid its default dictionary-based logic.
            instance = super(BaseExponentEntity, cls).__new__(cls)
            object.__setattr__(instance, "_key", key)
            object.__setattr__(instance, "_unit", RationalUnit(dict(key)))
            object.__setattr__(
                instance,
                "_analytical_representation",
                cls._calculate_representation(key),
            )
            # read-only: the instance is shared by everything that builds it
            object.__setattr__(
                instance,
                "exponents",
                MappingProxyType(
                    {
                        symbol: n if d == 1 else Fraction(n, d)
                        for symbol, (n, d) in key
                    }
                ),
            )
            cls._cache[key] = cast("Dimension", instance)
        if items is not None:
            cls._cache[items] = cast("Dimension", instance)
        return cast("Self", instance)

    @classmethod
    def _calculate_representation(
        cls, key: tuple[tuple[str, tuple[int, int]], ...]
    ) -> str:
        """Calculates the analytical string of a dimension."""
        if not key:
            return "Dimensionless"
        parts = []
        for symbol, (n, d) in key:
            if (n, d) == (1, 1):
                parts.append(symbol)
            elif d == 1:
                parts.append(f"{symbol}{to_superscript(n)}")
            else:
                parts.append(f"{symbol}^({n}/{d})")
        return "·".join(parts)

    @classmethod
    def _of(cls, unit: RationalUnit) -> Dimension:
        """The dimension that a unit over dimension symbols stands for."""
        return cls(unit.dimensions)

    # ponytail: real initialization happens in __new__'s Flyweight cache;
    # this __init__ is intentionally a no-op.
    def __init__(  # pyright: ignore[reportMissingSuperCall]
        self, exponents: ExponentsDict | None = None
    ):
        """Initializes the dimension. Logic is handled in __new__."""

    def __reduce__(self) -> tuple[Any, ...]:
        """Pickles the powers; the flyweight finds the instance again."""
        return (type(self), (dict(self.exponents),))

    def __hash__(self) -> int:
        """Returns a hash value for the dimension based on its powers."""
        return hash(self._key)

    def __eq__(self, other: object) -> bool:
        """Checks equality by comparing the powers."""
        if isinstance(other, Dimension):
            return self._key == other._key
        if isinstance(other, BaseExponentEntity):
            # Fallback for mixed comparisons if necessary
            return self.exponents == other.exponents
        return NotImplemented

    # --- Algebra of the core ---

    def __mul__(self, other: ExponentEntityProtocol) -> Dimension:
        """Multiplies two dimensions by adding their powers."""
        if not isinstance(other, Dimension):
            return NotImplemented  # type: ignore[return-value]
        return Dimension._of(self._unit * other._unit)

    def __truediv__(self, other: ExponentEntityProtocol) -> Dimension:
        """Divides two dimensions by subtracting their powers."""
        if not isinstance(other, Dimension):
            return NotImplemented  # type: ignore[return-value]
        return Dimension._of(self._unit / other._unit)

    def __pow__(self, power: float) -> Dimension:
        """Raises a dimension to a power by scaling its powers."""
        # ponytail: defensive runtime guard against misuse by callers that
        # ignore the type hint (e.g. dynamically-typed callers); the
        # `power: float` annotation makes this unreachable statically.
        if not isinstance(  # pyright: ignore[reportUnnecessaryIsInstance]
            power, (int, float, Fraction)
        ):
            return NotImplemented  # pyright: ignore[reportUnreachable]
        return Dimension._of(self._unit ** _pair(power))

    # --- Properties for SI Base Dimensions ---

    @property
    def length(self) -> int:
        """Exponent of the Length dimension."""
        return self.exponents.get("L", 0)

    @property
    def mass(self) -> int:
        """Exponent of the Mass dimension."""
        return self.exponents.get("M", 0)

    @property
    def time(self) -> int:
        """Exponent of the Time dimension."""
        return self.exponents.get("T", 0)

    @property
    def current(self) -> int:
        """Exponent of the Electric Current dimension."""
        return self.exponents.get("I", 0)

    @property
    def temperature(self) -> int:
        """Exponent of the Thermodynamic Temperature dimension."""
        return self.exponents.get("O", 0)

    @property
    def amount(self) -> int:
        """Exponent of the Amount of Substance dimension."""
        return self.exponents.get("N", 0)

    @property
    def luminous(self) -> int:
        """Exponent of the Luminous Intensity dimension."""
        return self.exponents.get("J", 0)

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
                f"({registered_name}) {self.exponents}>"
            )
        return (
            f"<Dimension: {self.analytical_representation} {self.exponents}>"
        )

    @property
    def is_dimensionless(self) -> bool:
        """Checks if the dimension is dimensionless."""
        return not self._key

    @classmethod
    def set_base_dimensions(cls, bases: list[str]):
        """Sets the base dimensions that the system will recognize."""
        cls._base_dimensions = bases

    @classmethod
    def from_string(cls, dim_str: str) -> Dimension:
        """Creates a Dimension object from a string."""
        # Use memoized parser eventually, or just use get_dimension
        return get_dimension(dim_str)


def get_dimension(unit_expression: str) -> Dimension:
    """Returns a Dimension object parsed from a unit expression string."""
    from physure.application.parsing import parse_unit_string

    return cast("Dimension", parse_unit_string(unit_expression, Dimension))
