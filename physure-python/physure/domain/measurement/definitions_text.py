"""Writes what a unit system registers as the conf text the core reads.

A `UnitSystem` keeps its definitions in the registry of the core, which is
built from conf text, so a unit that is registered by hand is written as the
line of the catalog that would define it: a scale, a scale with a zero point,
or a level, on a dimension. What cannot be written as such a line is refused,
and the caller leaves the system as it was.
"""

from __future__ import annotations

import math
from fractions import Fraction
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping

    from physure.domain.measurement.dimensions import Dimension
    from physure.domain.measurement.units import CompoundUnit

# A number of a unit: a `str` ("5/9", "0.3048") or a `Fraction` is exact, a
# `float` is the decimal it prints as.
Number = float | int | Fraction | str

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


def _number(value: Number) -> str:
    """Writes a number of the conf: a fraction as it is, a float as it prints."""
    if isinstance(value, str):
        try:
            return str(Fraction(value))
        except (ValueError, ZeroDivisionError):
            raise ValueError(f"{value!r} is not a number.") from None
    if isinstance(value, (int, Fraction)):
        return str(Fraction(value))
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
    scale: Number | None = None,
    aliases: Iterable[str] = (),
    *,
    offset: Number = 0,
    level: tuple[Number, Number] | None = None,
    recipe: CompoundUnit | None = None,
    allow_prefixes: bool = True,
) -> str:
    """Writes the `[Units]` text that defines a unit in the core's registry.

    The names of the unit are its symbol and its aliases: the descriptive name
    that `register_unit` takes is not a name that resolves, so it is not
    written.

    Args:
        symbol: The symbol of the unit, the first of its names.
        dimension: Its dimension.
        scale: How many base units it is: 1 when omitted. Not for a level.
        aliases: The other names of the unit.
        offset: Where its zero is, in base units. A unit with a zero point
            (degC) is an absolute scale; without one it is a difference.
        level: ``(factor, reference)`` of a level: ``factor * log10(x /
            reference)``, 10 for a power ratio (dB).
        recipe: The units it is another name for, when it is one.
        allow_prefixes: Whether prefixes apply to it.

    Returns:
        The text: a `[Units]` section of one line.

    Raises:
        ValueError: A name cannot be written in a conf, a number is not a
            finite number, or the arguments contradict each other: a level
            has no scale, no zero point and no recipe, and neither has a unit
            with a zero point.
    """
    names = list(dict.fromkeys([symbol, *aliases]))
    for name in names:
        _name(name)
    dimension_text = _product(dimension.exponents)
    has_zero_point = Fraction(_number(offset)) != 0
    recipe_text = None
    if level is not None:
        if scale is not None:
            raise ValueError("A level has no scale.")
        if has_zero_point or recipe is not None:
            raise ValueError("A level has no zero point and no recipe.")
        factor, reference = level
        fields = ["Log", _number(factor), _number(reference), dimension_text]
    elif has_zero_point:
        if recipe is not None:
            raise ValueError("A unit with a zero point has no recipe.")
        fields = [
            _number(1.0 if scale is None else scale),
            _number(offset),
            dimension_text,
        ]
    else:
        fields = [_number(1.0 if scale is None else scale), dimension_text]
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
    return (
        f"[Prefixes]\n{label} = {_name(symbol)}, "
        f"{_number(factor if exact is None else exact)}\n"
    )
