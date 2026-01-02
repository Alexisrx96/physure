"""Pandas extension for MeasureKit quantities."""

from __future__ import annotations

from collections.abc import Sequence
from typing import Any, cast

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

    def register_extension_dtype(cls):
        return cls


from measurekit.domain.measurement.quantity import Quantity
from measurekit.domain.measurement.units import CompoundUnit


@register_extension_dtype
class MeasureKitDtype(ExtensionDtype):
    """Pandas ExtensionDtype for MeasureKit Quantity."""

    name = "measurekit"
    type = Quantity
    kind = "O"

    def __init__(self, unit: CompoundUnit | str | None = None):
        """Initializes the dtype with an optional unit."""
        self._unit = unit

    @property
    def unit(self) -> CompoundUnit | str | None:
        """Returns the unit associated with this dtype."""
        return self._unit

    @classmethod
    def construct_array_type(cls) -> type[MeasureKitArray]:
        """Returns the array type for this dtype."""
        return MeasureKitArray

    def __repr__(self) -> str:
        """Returns a string representation."""
        return f"MeasureKitDtype(unit={self.unit})"

    @classmethod
    def construct_from_string(cls, string: str) -> MeasureKitDtype:
        """Construct from string like 'measurekit[m/s]'."""
        if string == "measurekit":
            return cls()
        if string.startswith("measurekit[") and string.endswith("]"):
            unit = string[11:-1]
            return cls(unit=unit)
        raise TypeError(f"Cannot construct a MeasureKitDtype from {string}")


class MeasureKitArray(ExtensionArray):
    """Pandas ExtensionArray for MeasureKit Quantity."""

    def __init__(
        self,
        values: Any,
        dtype: MeasureKitDtype | None = None,
        copy: bool = False,
    ):
        """Initializes the MeasureKitArray."""
        self._data = (
            np.array(values, copy=True) if copy else np.asarray(values)
        )
        self._dtype = dtype or MeasureKitDtype()

    @property
    def dtype(self) -> MeasureKitDtype:
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
    def _from_sequence(cls, scalars, dtype=None, copy=False):
        return cls(scalars, dtype=dtype, copy=copy)

    @classmethod
    def _from_factorized(cls, values, original):
        return cls(values, dtype=original.dtype)

    def copy(self):
        """Returns a copy of the array."""
        return type(self)(self._data.copy(), dtype=self.dtype)

    def isna(self) -> np.ndarray:
        """Returns a boolean mask of missing values."""
        return np.array([v is None for v in self._data], dtype=bool)

    def take(self, indices, allow_fill=False, fill_value=None):
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
        self, to_concat: Sequence[MeasureKitArray]
    ) -> MeasureKitArray:
        return type(self)(
            np.concatenate([cast("np.ndarray", p._data) for p in to_concat]),
            dtype=self.dtype,
        )

    def _reduce(self, name: str, skipna: bool = True, **kwargs):
        if name == "sum":
            # Reduction for Quantity objects
            result = self._data[0]
            for i in range(1, len(self._data)):
                result += self._data[i]
            return result
        raise TypeError(f"Cannot perform reduction {name} on MeasureKitArray")
