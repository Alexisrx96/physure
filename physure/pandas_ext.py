# pyright: reportAny=false, reportExplicitAny=false, reportUnknownMemberType=false, reportUnknownVariableType=false, reportUnknownArgumentType=false, reportUnknownParameterType=false
# ponytail: pandas is an optional dependency; when unavailable,
# ExtensionArray/ExtensionDtype fall back to plain `object` below, so pyright
# can't resolve their real API and every dtype/array value crossing that
# boundary is genuinely unknown/Any upstream, not a gap in our own
# annotations.
"""Pandas extension for Physure quantities."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import numpy as np

try:
    from pandas.api.extensions import (
        ExtensionArray,
        ExtensionDtype,
        register_extension_dtype,
    )
except (ImportError, AttributeError):
    # Fallback if pandas is missing/broken
    ExtensionArray = object
    ExtensionDtype = object

    def register_extension_dtype(cls: type) -> type:
        """No-op decorator when pandas is unavailable."""
        return cls


from physure.domain.measurement.quantity import Quantity

if TYPE_CHECKING:
    from collections.abc import Sequence

    from physure.domain.measurement.units import CompoundUnit


# ponytail: pandas is an optional dependency; ExtensionDtype falls back
# to plain `object` above when pandas is unavailable, so pyright can't
# statically verify this is a valid base class in both cases.
@register_extension_dtype
class PhysureDtype(ExtensionDtype):  # pyright: ignore[reportGeneralTypeIssues]
    """Pandas ExtensionDtype for Physure Quantity."""

    name = "physure"
    type = Quantity
    kind = "O"

    def __init__(self, unit: CompoundUnit | str | None = None) -> None:
        """Initializes the dtype with an optional unit."""
        super().__init__()
        if isinstance(unit, str):
            from physure.application.context import get_current_system

            unit = get_current_system().resolve_unit(unit)
        self._unit = unit

    @property
    def unit(self) -> CompoundUnit | None:
        """Returns the unit associated with this dtype."""
        return self._unit

    @classmethod
    def construct_array_type(cls) -> type[PhysureArray]:
        """Returns the array type for this dtype."""
        return PhysureArray

    def __repr__(self) -> str:
        """Returns a string representation."""
        return f"PhysureDtype(unit={self.unit})"

    @classmethod
    def construct_from_string(cls, string: str) -> PhysureDtype:
        """Construct from string like 'physure[m/s]'."""
        if string == "physure":
            return cls()
        if string.startswith("physure[" and string.endswith("]"):
            unit = string[11:-1]
            return cls(unit=unit)
        raise TypeError(f"Cannot construct a PhysureDtype from {string}")


# ponytail: same optional-pandas fallback-to-object pattern as
# PhysureDtype above.
class PhysureArray(ExtensionArray):  # pyright: ignore[reportGeneralTypeIssues]
    """Pandas ExtensionArray for Physure Quantity."""

    def __init__(
        self,
        values: Any,
        # ponytail: PhysureDtype's own base is ambiguous to pyright (see
        # the fallback-to-object comment above), so referencing it in a
        # union annotation trips reportGeneralTypeIssues here too.
        dtype: PhysureDtype | None = None,  # pyright: ignore[reportGeneralTypeIssues]
        copy: bool = False,
    ) -> None:
        """Initializes the PhysureArray."""
        super().__init__()
        self._data = (
            np.array(values, dtype=object, copy=True)
            if copy
            else np.array(values, dtype=object)
        )
        self._dtype = dtype or PhysureDtype()

    @property
    def dtype(self) -> PhysureDtype:
        """Returns the dtype of the array."""
        return self._dtype

    def __len__(self) -> int:
        """Returns the length of the array."""
        return len(self._data)

    def __getitem__(self, item: int | slice | np.ndarray) -> Any:
        """Returns the item at the given index or a slice of the array."""
        if isinstance(item, int):
            return self._data[item]
        return type(self)(self._data[item], dtype=self.dtype)

    @classmethod
    def _from_sequence(
        cls,
        scalars: Sequence[Any],
        dtype: PhysureDtype | None = None,  # pyright: ignore[reportGeneralTypeIssues]
        copy: bool = False,
    ) -> PhysureArray:
        return cls(scalars, dtype=dtype, copy=copy)

    @classmethod
    def _from_factorized(
        cls, values: np.ndarray, original: PhysureArray
    ) -> PhysureArray:
        return cls(values, dtype=original.dtype)

    def copy(self) -> PhysureArray:
        """Returns a copy of the array."""
        return type(self)(self._data.copy(), dtype=self.dtype)

    def isna(self) -> np.ndarray:
        """Returns a boolean mask of missing values."""
        return np.array([v is None for v in self._data], dtype=bool)

    def take(
        self,
        indices: Sequence[int],
        allow_fill: bool = False,
        fill_value: Any = None,
    ) -> PhysureArray:
        """Takes elements from the array."""
        from pandas.api.extensions import take

        data = self._data
        if allow_fill and fill_value is None:
            fill_value = None
        result = take(
            data, indices, allow_fill=allow_fill, fill_value=fill_value
        )
        return type(self)(result, dtype=self.dtype)

    def _concat_same_type(
        self, to_concat: Sequence[PhysureArray]
    ) -> PhysureArray:
        return type(self)(
            np.concatenate([p._data for p in to_concat]),
            dtype=self.dtype,
        )

    def _reduce(self, name: str, skipna: bool = True, **kwargs: Any) -> Any:
        # ponytail: skipna/kwargs are part of pandas' ExtensionArray._reduce
        # interface (called by pandas internals with these keywords) but
        # unused by this simple sum-only implementation.
        del skipna, kwargs
        if name == "sum":
            # Reduction for Quantity objects
            result = self._data[0]
            for i in range(1, len(self._data)):
                result += self._data[i]
            return result
        raise TypeError(f"Cannot perform reduction {name} on PhysureArray")
