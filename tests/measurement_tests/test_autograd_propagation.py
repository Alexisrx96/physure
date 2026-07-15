import importlib.util
import math

import pytest

from measurekit.core.dispatcher import BackendManager
from measurekit.domain.measurement.quantity import Quantity
from measurekit.domain.measurement.system import UnitSystem
from measurekit.domain.measurement.uncertainty import (
    Uncertainty,
)
from measurekit.domain.measurement.units import CompoundUnit


def get_available_backends():
    backends = ["numpy"]
    if importlib.util.find_spec("torch"):
        backends.append("torch")
    if importlib.util.find_spec("jax"):
        backends.append("jax")
    return backends


@pytest.mark.parametrize("backend_name", get_available_backends())
def test_sin_propagation_scalar(backend_name):
    """Test sin(x) propagation: sigma_y = |cos(x)| * sigma_x"""
    backend = BackendManager._get_or_load_backend(backend_name)
    if backend_name == "jax":
        import jax.numpy as jnp

        val = jnp.array(0.5)
        unc_val = jnp.array(0.01)
    elif backend_name == "torch":
        import torch

        val = torch.tensor(0.5)
        unc_val = torch.tensor(0.01)
    else:
        val = 0.5
        unc_val = 0.01

    system = UnitSystem()
    unit = CompoundUnit({})  # Dimensionless
    unc = Uncertainty.from_standard(unc_val)

    q = Quantity.from_input(val, unit, system, uncertainty=unc)

    # Apply sin
    # Note: Quantity.sin() uses autograd
    if backend_name in ["torch", "jax"]:
        res_q = q.sin()

        expected_val = math.sin(0.5)
        expected_std = abs(math.cos(0.5)) * 0.01

        if backend_name == "torch":
            import torch

            expected_val_t = torch.tensor(expected_val)
            expected_std_t = torch.tensor(expected_std)
        else:
            import jax.numpy as jnp

            expected_val_t = jnp.array(expected_val)
            expected_std_t = jnp.array(expected_std)

        # Check Value
        # Use backend check
        assert backend.allclose(res_q.magnitude, expected_val_t, atol=1e-5)

        # result uncertainty
        res_std = res_q.uncertainty
        assert backend.allclose(res_std, expected_std_t, atol=1e-5)
    else:
        # Numpy fallback uses finite diff in my autograd implementation
        res_q = q.sin()
        expected_val = math.sin(0.5)
        assert abs(float(res_q.magnitude) - expected_val) < 1e-5

        expected_std = abs(math.cos(0.5)) * 0.01
        assert abs(float(res_q.uncertainty) - expected_std) < 1e-5


@pytest.mark.parametrize("backend_name", get_available_backends())
def test_exp_propagation_scalar(backend_name):
    """Test exp(x) propagation: sigma_y = |exp(x)| * sigma_x"""
    backend = BackendManager._get_or_load_backend(backend_name)
    val_scalar = 1.0
    unc_scalar = 0.1

    if backend_name == "jax":
        import jax.numpy as jnp

        val = jnp.array(val_scalar)
        unc_val = jnp.array(unc_scalar)
    elif backend_name == "torch":
        import torch

        val = torch.tensor(val_scalar)
        unc_val = torch.tensor(unc_scalar)
    else:
        val = val_scalar
        unc_val = unc_scalar

    system = UnitSystem()
    unit = CompoundUnit({})
    unc = Uncertainty.from_standard(unc_val)
    q = Quantity.from_input(val, unit, system, uncertainty=unc)

    if backend_name in ["torch", "jax"]:
        res_q = q.exp()

        expected_val = math.exp(val_scalar)
        expected_std = math.exp(val_scalar) * unc_scalar

        if backend_name == "torch":
            import torch

            expected_val_t = torch.tensor(expected_val)
            expected_std_t = torch.tensor(expected_std)
        else:
            import jax.numpy as jnp

            expected_val_t = jnp.array(expected_val)
            expected_std_t = jnp.array(expected_std)

        assert backend.allclose(res_q.magnitude, expected_val_t, atol=1e-5)
        assert backend.allclose(res_q.uncertainty, expected_std_t, atol=1e-5)
    else:
        res_q = q.exp()
        expected_val = math.exp(val_scalar)
        assert abs(float(res_q.magnitude) - expected_val) < 1e-5
        expected_std = math.exp(val_scalar) * unc_scalar
        assert abs(float(res_q.uncertainty) - expected_std) < 1e-5


if __name__ == "__main__":
    pytest.main([__file__])
