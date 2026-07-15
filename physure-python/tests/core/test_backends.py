from physure.domain.measurement.quantity import Quantity
from physure.domain.measurement.units import CompoundUnit


def to_numpy(val, backend):
    """Utility to convert backend-specific array to numpy for comparison."""
    if hasattr(val, "numpy"):  # Torch
        return val.cpu().detach().numpy()
    if hasattr(val, "tolist") and not hasattr(val, "shape"):  # Scalar-like
        return val
    import numpy as np

    return np.array(val)


def test_backend_parity_basic_math(common_system, backend_instance):
    """Assert mathematical parity across backends for basic operations."""
    m = CompoundUnit({"m": 1})

    # Check if backend supports arrays
    if backend_instance.is_array(backend_instance.asarray([1.0])):
        q1_val = [10.0, 20.0, 30.0]
        q2_val = [1.0, 2.0, 3.0]
        exp_add = [11.0, 22.0, 33.0]
        exp_div = [5.0, 10.0, 15.0]
    else:
        q1_val = 10.0
        q2_val = 1.0
        exp_add = 11.0
        exp_div = 5.0

    q1 = Quantity.from_input(
        backend_instance.asarray(q1_val), m, common_system
    )
    q2 = Quantity.from_input(
        backend_instance.asarray(q2_val), m, common_system
    )

    # Addition
    res_add = q1 + q2
    np_res_add = to_numpy(res_add.magnitude, backend_instance)
    import numpy as np

    assert np.allclose(np_res_add, exp_add)

    # Division
    res_div = q1 / 2.0
    np_res_div = to_numpy(res_div.magnitude, backend_instance)
    assert np.allclose(np_res_div, exp_div)


def test_backend_parity_conversions(common_system, backend_instance):
    """Assert mathematical parity across backends for unit conversions."""
    m = CompoundUnit({"m": 1})
    km = CompoundUnit({"km": 1})

    if backend_instance.is_array(backend_instance.asarray([1.0])):
        q_val = [1000.0, 2500.0]
        exp_val = [1.0, 2.5]
    else:
        q_val = 1000.0
        exp_val = 1.0

    data = backend_instance.asarray(q_val)
    q = Quantity.from_input(data, m, common_system)

    q_km = q.to(km)
    np_mag = to_numpy(q_km.magnitude, backend_instance)
    import numpy as np

    assert np.allclose(np_mag, exp_val)
