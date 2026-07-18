"""Pandas Accessor for Physure.

This module provides a custom Pandas accessor `.mk` for Series containing
Physure Quantity objects, enabling high-performance vectorized operations.
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any

import numpy as np

from physure.domain.measurement.quantity import Quantity

if TYPE_CHECKING:
    import pandas as pd
    from pandas.api.extensions import register_series_accessor

    HAS_PANDAS = True
else:
    try:
        import pandas as pd

        # Verify minimal functionality to avoid broken installations
        _ = pd.Series
        _ = pd.DataFrame
        # ponytail: the real pandas decorator and the dummy fallback below are
        # structurally different callables by design (pandas is optional);
        # callers only ever see one or the other.
        from pandas.api.extensions import (
            register_series_accessor,  # pyright: ignore[reportAssignmentType]
        )

        HAS_PANDAS = True
    except (ImportError, AttributeError):
        # ponytail: HAS_PANDAS toggles between True/False across the
        # try/except branches by design; not a real constant-redefinition bug.
        HAS_PANDAS = False  # pyright: ignore[reportConstantRedefinition]
        pd = None

        # Define a dummy decorator if pandas is not available or broken
        def register_series_accessor(_name: str):
            """Dummy decorator if pandas is not available."""
            return lambda cls: cls


@register_series_accessor("mk")
class PhysureAccessor:
    """Pandas accessor for Physure quantities."""

    def __init__(self, pandas_obj: Any):
        """Initializes the accessor with a Pandas Series."""
        self._validate(pandas_obj)
        self._obj = pandas_obj

    @staticmethod
    def _validate(obj: Any) -> None:
        """Validates that the Series contains Quantity objects."""
        # Check dtype first
        if not pd.api.types.is_object_dtype(obj):
            # Sometimes it might be our custom ExtensionDtype if it exists
            if getattr(obj.values, "dtype", None) == "physure":
                return
            raise TypeError(
                "Series must be of object dtype to contain Quantity objects, "
                f"got {obj.dtype}"
            )

        # Heuristic: check first non-null element
        sample = obj.dropna()
        if sample.empty:
            return

        if not isinstance(sample.iloc[0], Quantity):
            raise TypeError(
                "Series must contain Quantity objects, "
                f"found {type(sample.iloc[0])}"
            )

        # For strictness as requested, we check all elements if reasonably
        # sized.
        if len(sample) < 1000 and not all(
            isinstance(x, Quantity) for x in sample
        ):
            raise TypeError("Series contains non-Quantity objects.")

    @property
    def magnitude(self) -> pd.Series:
        """Returns a Series of raw float values."""
        return self._obj.apply(
            lambda q: q.magnitude if q is not None else np.nan
        )

    @property
    def uncertainty(self) -> pd.Series:
        """Returns a Series of raw standard deviations."""
        return self._obj.apply(
            lambda q: q.uncertainty if q is not None else np.nan
        )

    @property
    def unit(self) -> Any:
        """Returns the shared unit object of the column."""
        valid_q = self._obj.dropna()
        if valid_q.empty:
            return None

        # Extract units
        units = valid_q.apply(lambda q: q.unit).unique()
        if len(units) == 1:
            return units[0]

        # Optimization: if we have many units, returning a list/Series of them
        return units

    @property
    def array(self) -> Quantity:
        """Returns the underlying vectorized Quantity array."""
        valid_data = self._obj.dropna()
        if valid_data.empty:
            raise ValueError("Series is empty or contains only null values.")

        magnitudes = np.array([q.magnitude for q in valid_data])
        uncertainties = np.array([q.uncertainty for q in valid_data])

        first_q = valid_data.iloc[0]
        unit = first_q.unit
        system = first_q.system

        # Ensure unit consistency for the vectorized array
        if not all(q.unit == unit for q in valid_data):
            # Convert everyone to the first unit's unit
            magnitudes = np.array([q.to(unit).magnitude for q in valid_data])
            uncertainties = np.array(
                [q.to(unit).uncertainty for q in valid_data]
            )

        return Quantity.from_input(
            magnitudes, unit, system, uncertainty=uncertainties
        )

    def to(self, unit_name: str) -> pd.Series:
        """Converts the column to a different unit via vectorized engine."""
        vec_q = self.array.to(unit_name)
        return self._wrap_vectorized(vec_q)

    def plus_minus(self, uncertainty: Any) -> pd.Series:
        """Attaches uncertainty to the quantities in bulk."""
        vec_q = self.array
        # Create a new vectorized Quantity with the provided uncertainty
        # If uncertainty is a Series, convert to numpy
        if isinstance(uncertainty, pd.Series):
            uncertainty = uncertainty.values

        new_vec_q = Quantity.from_input(
            vec_q.magnitude,
            vec_q.unit,
            vec_q.system,
            uncertainty=uncertainty,
        )
        return self._wrap_vectorized(new_vec_q)

    def to_json(self) -> str:
        """Produces a serialization-friendly JSON string for the column."""
        vec_q = self.array
        # Handle cases where uncertainty might not be present
        unc_data = vec_q.uncertainty
        if isinstance(unc_data, np.ndarray):
            unc_list = unc_data.tolist()
        else:
            unc_list = [unc_data] * len(vec_q.magnitude)

        data = {
            "magnitudes": vec_q.magnitude.tolist(),
            "uncertainties": unc_list,
            "unit": vec_q.unit.to_string(system=vec_q.system, use_alias=True),
        }
        return json.dumps(data)

    def _wrap_vectorized(self, vec_q: Quantity) -> pd.Series:
        """Wraps a vectorized Quantity back into a Series of Quantities."""
        # To maintain index and NaNs from the original Series:
        res_data = np.empty(len(self._obj), dtype=object)
        valid_mask = self._obj.notna()
        valid_indices = np.nonzero(valid_mask)[0]

        magnitudes = vec_q.magnitude
        stds = vec_q.uncertainty
        unit = vec_q.unit
        system = vec_q.system

        # Reconstruct scalar Quantity objects
        for i, pos in enumerate(valid_indices):
            res_data[pos] = Quantity.from_input(
                magnitudes[i], unit, system, uncertainty=stds[i]
            )

        return pd.Series(res_data, index=self._obj.index)
