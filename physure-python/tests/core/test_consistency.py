"""Cross-Backend consistency tests (The 'Three-Body' Tests).

Ensures that NumPy, PyTorch, and JAX backends produce numerically identical results.
"""

import numpy as np
import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from physure import Quantity, get_unit
from tests.strategies import (
    linear_units,
)

# Check availability
try:
    import torch

    TORCH_AVAILABLE = True
except ImportError:
    TORCH_AVAILABLE = False

try:
    import jax  # noqa: F401
    import jax.numpy as jnp

    JAX_AVAILABLE = True
except ImportError:
    JAX_AVAILABLE = False

# We skip consistency tests if we don't have all backends
if not (TORCH_AVAILABLE and JAX_AVAILABLE):
    pytest.skip(
        "Skipping consistency tests: Torch and JAX required",
        allow_module_level=True,
    )


def to_numpy(val):
    """Helper to convert any backend result to numpy."""
    if isinstance(val, Quantity):
        val = val.magnitude

    if hasattr(val, "detach"):  # Torch
        return val.detach().cpu().numpy()
    if hasattr(val, "__array__"):  # Jax/Numpy
        return np.array(val)
    return np.array(val)


def _convert_args_to_backend(args_data, unit, backend_name):
    """Converts raw data to Quantities on the specified backend."""
    qs = []
    for data in args_data:
        # data is numpy array or scalar
        if backend_name == "numpy":
            val = data
        elif backend_name == "torch":
            val = torch.from_numpy(np.array(data))  # Ensure copy/convert
        elif backend_name == "jax":
            val = jnp.array(data)
        else:
            raise ValueError(f"Unknown backend {backend_name}")

        qs.append(Quantity.from_input(val, unit, None))
    return qs


def assert_all_backends_agree(
    op_func, args_data, unit_name, rtol=1e-4, atol=1e-5
):
    """
    Executes op_func(*args) on Numpy, Torch, and JAX and asserts consistency.

    args_data: List of raw numpy data (magnitudes).
    unit_name: Unit to assign to inputs.
    """
    unit = get_unit(unit_name)

    # 1. NumPy Execution (Ground Truth)
    try:
        args_np = _convert_args_to_backend(args_data, unit, "numpy")
        res_np = op_func(*args_np)
        val_np = to_numpy(res_np)
    except Exception as e:
        # If ground truth fails, we assume invalid input for the op (e.g. log(-1))
        # We assume the caller handles validity, or we fail.
        # But for fuzzing, we might want to let hypothesis catch it?
        # Let's re-raise to see what happens.
        raise e

    # 2. Torch Execution
    try:
        args_torch = _convert_args_to_backend(args_data, unit, "torch")
        res_torch = op_func(*args_torch)
        val_torch = to_numpy(res_torch)
    except Exception as e:
        pytest.fail(f"Torch execution failed while Numpy succeeded: {e}")

    # 3. JAX Execution
    try:
        args_jax = _convert_args_to_backend(args_data, unit, "jax")
        res_jax = op_func(*args_jax)
        val_jax = to_numpy(res_jax)
    except Exception as e:
        pytest.fail(f"JAX execution failed while Numpy succeeded: {e}")

    # Assertions
    np.testing.assert_allclose(
        val_torch,
        val_np,
        rtol=rtol,
        atol=atol,
        err_msg="Torch vs Numpy mismatch",
    )
    np.testing.assert_allclose(
        val_jax, val_np, rtol=rtol, atol=atol, err_msg="JAX vs Numpy mismatch"
    )

    # Unit Consistency
    assert res_torch.unit == res_np.unit
    assert res_jax.unit == res_np.unit


# -----------------------------------------------------------------------------
# Operations Tests
# -----------------------------------------------------------------------------


@settings(max_examples=50, deadline=None)
@given(
    st.lists(st.floats(min_value=-100, max_value=100), min_size=2, max_size=2),
    linear_units(),
)
def test_consistency_addition(data, unit_name):
    """Uniary addition consistency."""
    # Note: data coming in as float scalars for simplicity.
    # Convert to scalar or 1-element array?
    # Let's use scalars.
    args = [np.array(x) for x in data]

    assert_all_backends_agree(lambda a, b: a + b, args, unit_name)


@settings(max_examples=50, deadline=None)
@given(
    st.lists(st.floats(min_value=-100, max_value=100), min_size=2, max_size=2),
    linear_units(),
)
def test_consistency_multiplication(data, unit_name):
    args = [np.array(x) for x in data]
    assert_all_backends_agree(lambda a, b: a * b, args, unit_name)


@settings(max_examples=50, deadline=None)
@given(st.floats(min_value=0.1, max_value=10.0), st.just("1"))
def test_consistency_log(val, unit_name):
    """Logarithm consistency (requires dimensionless usually, or handles unit)."""
    # Physure log returns log of magnitude only if dimensionless?
    # Or computes log(q/q0)?
    # Quantity.log() implementation in Physure is dimensionless-only usually.
    # But let's check `_apply_transcendental`. It propagates on magnitude.
    # If valid, we check consistency.

    # We strip unit for log test to ensure validity,
    # OR we use a dimensionless unit.
    # Let's assume we pass in a dimensionless Quantity for log.

    args = [np.array(val)]
    # Use "radian" or empty unit for dimensionless
    # Or just ignore unit in the oracle helper if we want strict dimensionless?
    # The oracle uses unit_name.
    # Let's try to call .log().
    # If the unit is not dimensionless, log might warn or behave specifically.
    # We'll stick to a dimensionless test case.

    # Override unit to dimensionless
    # args_np logic removed as it was unused and misleading

    # We just run logic manually here or adapt oracle?
    # Let's adapt test to just pass "1" (dimensionless unit) string.
    assert_all_backends_agree(lambda a: a.log(), args, "1")


@settings(max_examples=50, deadline=None)
@given(st.floats(min_value=-np.pi, max_value=np.pi), st.just("rad"))
def test_consistency_sin(val, unit_name):
    # Sin expects dimensionless or radians
    args = [np.array(val)]
    assert_all_backends_agree(lambda a: a.sin(), args, "rad")
