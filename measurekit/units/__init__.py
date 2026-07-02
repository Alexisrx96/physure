"""Lazy-loading unit package."""

from __future__ import annotations

import typing

if typing.TYPE_CHECKING:
    from measurekit.domain.measurement.units import CompoundUnit

try:
    from ._index import UNIT_INDEX
except ImportError:
    UNIT_INDEX = {}


def __getattr__(name: str) -> CompoundUnit:
    if name in UNIT_INDEX:
        scope = UNIT_INDEX[name]
        module = __import__(f"measurekit.units.{scope}", fromlist=[name])
        return getattr(module, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__() -> list[str]:
    return sorted(list(globals().keys()) + list(UNIT_INDEX.keys()))
