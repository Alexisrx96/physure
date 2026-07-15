# physure/ext/io.py
"""Scientific Serialization module for Physure."""

from __future__ import annotations

from typing import TYPE_CHECKING

try:
    import h5py
except ImportError:
    h5py = None

try:
    import xarray as xr
except ImportError:
    xr = None

from physure import __version__

if TYPE_CHECKING:
    from h5py import Dataset, Group

    from physure.core.protocols import Numeric
    from physure.domain.measurement.quantity import Quantity


def to_hdf5(quantity: Quantity, group: Group, dataset_name: str) -> Dataset:
    """Saves a Quantity to an HDF5 group as a dataset with unit metadata.

    The magnitude array is saved to the dataset and the unit string is written
    to the dataset's `.attrs["units"]`.

    Args:
        quantity: The Quantity object to save.
        group: An h5py.Group or h5py.File object.
        dataset_name: The name for the new dataset.

    Returns:
        The created h5py.Dataset.
    """
    if h5py is None:
        raise ImportError(
            "h5py is required for HDF5 serialization. "
            "Install it with 'pip install h5py' or use "
            "'pip install physure[io]'."
        )

    # Convert magnitude to something h5py can handle
    magnitude = quantity.magnitude

    # If it's a backend-specific array, convert to numpy
    if hasattr(magnitude, "numpy") and callable(magnitude.numpy):  # Torch
        data = magnitude.detach().cpu().numpy()
    elif hasattr(magnitude, "__array__"):
        data = magnitude
    else:
        import numpy as np

        data = np.array(magnitude)

    # Create dataset
    if dataset_name in group:
        del group[dataset_name]

    ds = group.create_dataset(dataset_name, data=data)

    # Save unit metadata following CF conventions where possible
    ds.attrs["units"] = quantity.unit.to_string(quantity.system)
    ds.attrs["physure_version"] = __version__

    # Optional: Save uncertainty if present
    if quantity._has_uncertainty:
        unc_val = quantity.uncertainty
        if hasattr(unc_val, "shape") and unc_val.shape:
            unc_ds_name = f"{dataset_name}_uncertainty"
            if unc_ds_name in group:
                del group[unc_ds_name]
            group.create_dataset(unc_ds_name, data=unc_val)
            ds.attrs["ancillary_variables"] = unc_ds_name
        else:
            ds.attrs["uncertainty"] = float(unc_val)

    return ds


def from_hdf5(dataset: Dataset) -> Quantity:
    """Reconstitutes a Quantity from an HDF5 dataset.

    Args:
        dataset: An h5py.Dataset object.

    Returns:
        A Quantity object with restored units and magnitude.
    """
    if h5py is None:
        raise ImportError("h5py is required for HDF5 serialization.")

    from physure.application.factories import QuantityFactory

    # Get metadata
    unit_str = dataset.attrs.get("units", "dimensionless")
    magnitude = dataset[...]

    # Resolve factory
    factory = QuantityFactory()

    # Extract uncertainty if present
    uncertainty = 0.0
    if "uncertainty" in dataset.attrs:
        uncertainty = dataset.attrs["uncertainty"]
    elif "ancillary_variables" in dataset.attrs:
        unc_ds_name = dataset.attrs["ancillary_variables"]
        # Ancillary variables are usually in the same group or absolute path
        if unc_ds_name in dataset.file:
            uncertainty = dataset.file[unc_ds_name][...]
        elif "/" not in unc_ds_name and unc_ds_name in dataset.parent:
            uncertainty = dataset.parent[unc_ds_name][...]

    return factory(magnitude, unit_str, uncertainty=uncertainty)


if xr is not None:

    @xr.register_dataarray_accessor("physure")
    class PhysureAccessor:
        """Xarray accessor for Physure integration."""

        def __init__(self, xarray_obj: xr.DataArray):
            self._obj = xarray_obj

        def quantify(self, unit: str | None = None) -> Quantity:
            """Converts the DataArray to a Physure Quantity.

            Args:
                unit: Optional unit string. If not provided, it looks
                    for 'units' in the DataArray attributes.

            Returns:
                A Quantity object.
            """
            from physure.application.factories import QuantityFactory

            unit_str = unit or self._obj.attrs.get("units", "dimensionless")
            # Handle potential uncertainty in ancillary_variables
            uncertainty = 0.0
            ancillary = self._obj.attrs.get("ancillary_variables")
            if ancillary and isinstance(ancillary, str):
                # Try to find uncertainty in the dataset if part of one
                # This is simplified for DataArray
                pass

            return QuantityFactory()(self._obj.values, unit_str, uncertainty)

        def attach_units(self, unit: str) -> xr.DataArray:
            """Attaches units to the DataArray attributes (CF convention).

            Args:
                unit: The unit string to attach.

            Returns:
                The DataArray with updated attributes.
            """
            self._obj.attrs["units"] = unit
            return self._obj

        @property
        def magnitude(self) -> Numeric:
            """Returns the raw underlying data."""
            return self._obj.values
