"""The units of the catalog as module attributes: ``physure.units.m``.

Nothing is listed here. Every name is looked up in the registry of the domain
(the catalog and the plugins), so ``from physure import units`` and ``import
physure.units`` answer the same.
"""

from __future__ import annotations

import typing

if typing.TYPE_CHECKING:
    from physure.domain.measurement.units import CompoundUnit


def __getattr__(name: str) -> CompoundUnit:
    from physure.domain.measurement.units import units

    try:
        return getattr(units, name)
    except AttributeError:
        raise AttributeError(
            f"module {__name__!r} has no attribute {name!r}"
        ) from None


def __dir__() -> list[str]:
    from physure.domain.measurement.units import units

    return dir(units)
