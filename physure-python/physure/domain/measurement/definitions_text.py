"""Writes what a unit system registers as the conf text the core reads.

A `UnitSystem` keeps its definitions twice: in Python objects and in the
registry of the core. The registry is built from conf text, so a unit that is
registered by hand is written as the line of the catalog that would define it.
Only what a catalog line can say is supported: a scale, a scale with a zero
point, or a level, on a dimension. Anything else is refused, and the caller
leaves the system as it was.
"""

from __future__ import annotations

import math
from fractions import Fraction
from typing import TYPE_CHECKING

from physure.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
    OffsetConverter,
)

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping

    from physure.domain.measurement.converters import UnitConverter
    from physure.domain.measurement.dimensions import Dimension
    from physure.domain.measurement.units import CompoundUnit

_SUPPORTED = (LinearConverter, OffsetConverter, LogarithmicConverter)
_UNWRITABLE = frozenset("=,[]\n\r")


def _name(text: str) -> str:
    """Returns `text` if a conf can hold it as a name, else raises."""
    if (
        not text
        or text != text.strip()
        or text[0] in "#;"
        or any(char in _UNWRITABLE for char in text)
    ):
        raise ValueError(f"{text!r} cannot be a name in a conf.")
    return text


def _number(value: float, exact: Fraction | None) -> str:
    """Writes a number of the conf: the exact fraction when there is one."""
    if exact is not None:
        return str(Fraction(exact))
    if not math.isfinite(value):
        raise ValueError(f"{value!r} cannot be written in a conf.")
    return repr(float(value))


def _exponent(power: float) -> str:
    """Writes the power of a symbol: ``2``, ``-3`` or ``(1/2)``."""
    fraction = Fraction(power).limit_denominator(1000)
    if fraction.denominator == 1:
        return str(fraction.numerator)
    return f"({fraction.numerator}/{fraction.denominator})"


def _product(exponents: Mapping[str, float]) -> str:
    """Writes symbols to powers as a conf does: ``L^2*M*T^-3``, or ``1``."""
    if not exponents:
        return "1"
    return "*".join(
        _name(symbol) if power == 1 else f"{_name(symbol)}^{_exponent(power)}"
        for symbol, power in sorted(exponents.items())
    )


def unit_definition_text(
    symbol: str,
    dimension: Dimension,
    converter: UnitConverter,
    aliases: Iterable[str] = (),
    *,
    recipe: CompoundUnit | None = None,
    allow_prefixes: bool = True,
    kind: str = "delta",
) -> str:
    """Writes the `[Units]` text that defines a unit in the core's registry.

    The names of the unit are its symbol and its aliases: the descriptive name
    that `register_unit` takes is not a name that resolves, so it is not
    written.

    Args:
        symbol: The symbol of the unit, the first of its names.
        dimension: Its dimension.
        converter: A `LinearConverter`, an `OffsetConverter` or a
            `LogarithmicConverter`, exactly: a subclass is another converter.
        aliases: The other names of the unit.
        recipe: The units it is another name for, when it is one.
        allow_prefixes: Whether prefixes apply to it.
        kind: ``"delta"``, or ``"absolute"`` for a unit with a zero point.

    Returns:
        The text: a `[Units]` section of one line.

    Raises:
        TypeError: The converter is not one a conf line can express.
        ValueError: A name cannot be written in a conf, a number is not
            finite, or `kind` or `recipe` contradicts the converter.
    """
    if type(converter) not in _SUPPORTED:
        raise TypeError(
            f"A {type(converter).__name__} is not a converter that a unit of "
            "a system can have: use a LinearConverter, an OffsetConverter or "
            "a LogarithmicConverter."
        )
    names = list(dict.fromkeys([symbol, *aliases]))
    for name in names:
        _name(name)
    if isinstance(converter, OffsetConverter) != (kind == "absolute"):
        raise ValueError(
            f"A {type(converter).__name__} cannot be a unit of kind {kind!r}."
        )
    dimension_text = _product(dimension.exponents)
    recipe_text = None
    if isinstance(converter, LogarithmicConverter):
        if recipe is not None:
            raise ValueError("A level has no recipe.")
        fields = [
            "Log",
            _number(converter.factor, None),
            _number(converter.reference, None),
            dimension_text,
        ]
    elif isinstance(converter, OffsetConverter):
        if converter.offset == 0 or recipe is not None:
            raise ValueError(
                "An OffsetConverter needs a zero point, and has no recipe."
            )
        fields = [
            _number(converter.scale, converter.exact_scale),
            _number(converter.offset, converter.exact_offset),
            dimension_text,
        ]
    else:
        assert isinstance(converter, LinearConverter)
        fields = [_number(converter.scale, converter.exact), dimension_text]
        # next to a dimensionless "1" a third field reads as a zero point
        if recipe is not None and dimension_text != "1":
            recipe_text = _product(recipe.exponents)
    if not allow_prefixes:
        fields.append("noprefix")
    if recipe_text is not None:
        fields.append(recipe_text)
    return f"[Units]\n{symbol} = {', '.join(fields)}, [{', '.join(names)}]\n"


def prefix_definition_text(
    symbol: str,
    factor: float,
    name: str | None = None,
    exact: Fraction | None = None,
) -> str:
    """Writes the `[Prefixes]` text that defines a prefix in the registry.

    Args:
        symbol: The symbol of the prefix (``"k"``).
        factor: Its factor.
        name: Its spelled-out name (``"kilo"``); the symbol when omitted.
        exact: The exact value of the factor, when it has one.

    Returns:
        The text: a `[Prefixes]` section of one line.

    Raises:
        ValueError: A name cannot be written in a conf, or the factor is not
            finite.
    """
    label = _name(name or symbol)
    return f"[Prefixes]\n{label} = {_name(symbol)}, {_number(factor, exact)}\n"
