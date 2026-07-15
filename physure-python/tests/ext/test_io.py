import numpy as np
import pytest

from physure import Q_
from physure.domain.measurement.quantity import Quantity


def test_hdf5_serialization_scalar(tmp_path):
    # Ensure h5py is available
    try:
        import h5py
    except ImportError:
        pytest.skip("h5py not installed")

    q_original = Q_(10.5, "kg * m / s^2", uncertainty=0.1)
    file_path = tmp_path / "test_scalar.h5"

    # Save
    with h5py.File(file_path, "w") as f:
        q_original.to_hdf5(f, "force")

    # Load
    with h5py.File(file_path, "r") as f:
        q_loaded = Quantity.from_hdf5(f["force"])

    assert float(q_loaded.magnitude) == pytest.approx(
        float(q_original.magnitude)
    )
    # Note: unit serialization might change order, but should be equivalent
    # We compare conversion factor or simplified version if needed,
    # but for simple ones str() is usually fine if deterministic.
    assert q_loaded.unit.to_string() == q_original.unit.to_string()
    assert float(q_loaded.uncertainty) == pytest.approx(
        float(q_original.uncertainty)
    )


def test_hdf5_serialization_array(tmp_path):
    try:
        import h5py
    except ImportError:
        pytest.skip("h5py not installed")

    data = np.array([1.0, 2.0, 3.0])
    unc = np.array([0.1, 0.2, 0.3])
    q_original = Q_(data, "m / s", uncertainty=unc)
    file_path = tmp_path / "test_array.h5"

    # Save
    with h5py.File(file_path, "w") as f:
        q_original.to_hdf5(f, "velocity")

    # Load
    with h5py.File(file_path, "r") as f:
        q_loaded = Quantity.from_hdf5(f["velocity"])

    np.testing.assert_allclose(q_loaded.magnitude, q_original.magnitude)
    assert q_loaded.unit.to_string() == q_original.unit.to_string()
    np.testing.assert_allclose(q_loaded.uncertainty, q_original.uncertainty)


def test_hdf5_version_metadata(tmp_path):
    try:
        import h5py
    except ImportError:
        pytest.skip("h5py not installed")

    from physure import __version__

    q = Q_(1, "m")
    file_path = tmp_path / "test_meta.h5"

    with h5py.File(file_path, "w") as f:
        ds = q.to_hdf5(f, "length")
        assert ds.attrs["physure_version"] == __version__
