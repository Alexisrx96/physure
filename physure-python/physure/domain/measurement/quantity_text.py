"""Splits the text of a quantity such as "10 m/s" into a value and a unit."""

from __future__ import annotations

import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from physure.domain.measurement.system import UnitSystem
    from physure.domain.measurement.units import CompoundUnit

# Matches only the leading numeric literal (optional sign, int/float, optional
# exponent); the unit is whatever text follows and is sliced off in Python.
# No trailing `.*$` wildcard and an unambiguous digit pattern
# (`\d+\.?\d*|\.\d+`, not `\d*\.?\d+`), so there is nothing to force the engine
# to re-partition a long digit run — immune to ReDoS backtracking.
_NUMBER_PREFIX_REGEX = re.compile(
    r"\s*([-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?)"
)


def parse_quantity_text(
    text: str, system: UnitSystem
) -> tuple[float | str, CompoundUnit]:
    """Parses a string like '10 m/s' into a value and a unit."""
    match = _NUMBER_PREFIX_REGEX.match(text)
    if not match:
        return text, system.get_unit("dimensionless")

    num_str = match.group(1)
    unit_str = text[match.end() :].strip()
    try:
        parsed_value = float(num_str)
        # If the number was an integer (e.g., "10"), convert back to int
        # for cleanness
        if (
            parsed_value.is_integer()
            and "." not in num_str
            and "e" not in num_str.lower()
        ):
            parsed_value = int(parsed_value)
    except ValueError:
        return text, system.get_unit("dimensionless")

    unit = (
        system.get_unit(unit_str)
        if unit_str
        else system.get_unit("dimensionless")
    )
    return parsed_value, unit
