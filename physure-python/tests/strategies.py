"""Hypothesis strategies for generating Physure objects.

This module provides data generators for Property-Based Testing (PBT)
of Quantity objects, Units, and algebraic properties.
"""

from __future__ import annotations

from typing import Any

import hypothesis.strategies as st
import numpy as np
from hypothesis.extra import numpy as hnp

from physure import Quantity, get_unit
from physure.core.dispatcher import get_backend
from physure.domain.measurement.units import get_default_system

# --- Backend Availability Checks ---
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

# --- Basic Data Generators ---


def valid_magnitudes(
    min_value=-1e10,
    max_value=1e10,
    allow_nan=False,
    allow_infinity=False,
) -> st.SearchStrategy[float]:
    """Generates valid float magnitudes, filtering NaN/Inf by default."""
    return st.floats(
        min_value=min_value,
        max_value=max_value,
        allow_nan=allow_nan,
        allow_infinity=allow_infinity,
    )


def shapes(
    min_dims=0, max_dims=3, min_side=1, max_side=5
) -> st.SearchStrategy[tuple[int, ...]]:
    """Generates array shapes."""
    return st.lists(
        st.integers(min_value=min_side, max_value=max_side),
        min_size=min_dims,
        max_size=max_dims,
    ).map(tuple)


# --- Unit Generators ---


def unit_names(system) -> list[str]:
    """The names of the units the registry of a system holds."""
    return system.core_registry.names()


def linear_unit_names(system) -> list[str]:
    """The names that are a plain scale: no zero point and no logarithm."""
    names = []
    for name in unit_names(system):
        try:
            if system.get_unit(name).is_linear(system):
                names.append(name)
        except Exception:
            continue  # a name that does not resolve as an expression
    return names


def linear_units_of(system, dimension) -> list[str]:
    """The names of the plain-scale units that measure `dimension`."""
    return [
        name
        for name in linear_unit_names(system)
        if system.get_unit(name).dimension(system) == dimension
    ]


@st.composite
def available_units(draw) -> str:
    """Selects a unit name from the registry of the default system."""
    return draw(st.sampled_from(unit_names(get_default_system())))


@st.composite
def linear_units(draw) -> str:
    """Selects a linear unit from the registry (excludes affine/log)."""
    return draw(st.sampled_from(linear_unit_names(get_default_system())))


# --- Backend-Specific Array Generators ---


@st.composite
def backend_arrays(
    draw,
    shape: tuple[int, ...] | None = None,
    dtype=np.float32,
    backend: str = "numpy",
    elements=None,
) -> Any:
    """Generates an array (or scalar if 0-d) on the specified backend."""
    if shape is None:
        shape = draw(shapes())

    if elements is None:
        elements = valid_magnitudes(
            min_value=-1e5, max_value=1e5
        )  # Tighter range for arrays

    # Generate numpy array first as source of truth
    np_arr = draw(hnp.arrays(dtype=dtype, shape=shape, elements=elements))

    # Convert scalar numpy arrays to python scalars if backend is python
    if backend == "python":
        if np_arr.ndim == 0:
            return float(np_arr)
        return np_arr.tolist()

    if backend == "numpy":
        return np_arr

    if backend == "torch":
        if not TORCH_AVAILABLE:
            raise RuntimeError("Torch requested but not installed.")
        return torch.from_numpy(np_arr)

    if backend == "jax":
        if not JAX_AVAILABLE:
            raise RuntimeError("JAX requested but not installed.")
        return jnp.array(np_arr)

    raise ValueError(f"Unknown backend: {backend}")


# --- Quantity Generator ---


@st.composite
def quantities(
    draw,
    magnitude=None,
    unit=None,
    backend: str | None = None,
    allow_uncertainty: bool = True,
    dtype=np.float32,
) -> Quantity:
    """Generates a Quantity object.

    Args:
        magnitude: Strategy for magnitude (default: valid_magnitudes or backend_arrays).
        unit: Strategy for unit (default: available_units).
        backend: Force specific backend ('numpy', 'torch', 'jax', 'python'). If None, random.
        allow_uncertainty: Whether to include random uncertainty.
    """
    # 1. Select Backend
    if backend is None:
        candidates = ["numpy", "python"]
        if TORCH_AVAILABLE:
            candidates.append("torch")
        if JAX_AVAILABLE:
            candidates.append("jax")
        backend = draw(st.sampled_from(candidates))

    # 2. Generate Magnitude
    if magnitude is None:
        # 50% chance of scalar, 50% chance of array
        if draw(st.booleans()):
            # Scalar (treat as 0-d array for consistency in backend gen)
            mag_val = draw(
                backend_arrays(shape=(), backend=backend, dtype=dtype)
            )
        else:
            mag_val = draw(backend_arrays(backend=backend, dtype=dtype))
    else:
        mag_val = draw(magnitude)

    # 3. Generate Unit
    if unit is None:
        unit_name = draw(available_units())
        u = get_unit(unit_name)
    elif isinstance(unit, str):
        u = get_unit(unit)
    else:
        u = draw(unit)
        if isinstance(u, str):
            u = get_unit(u)

    # 4. Generate Uncertainty (Optional)
    unc = 0.0
    if allow_uncertainty and draw(st.booleans()):
        # Uncertainty must match shape/backend of magnitude usually,
        # or be a scalar. We'll stick to scalar relative uncertainty or same-shape.

        # Simple case: Scalar uncertainty (relative * abs(mag))?
        # Or just a separate array.
        # Let's generate a positive value.

        # Get shape of magnitude
        b_ops = get_backend(mag_val)
        shape = b_ops.shape(mag_val)

        # Generate raw uncertainty data (positive)
        raw_unc = draw(
            backend_arrays(
                shape=shape, backend=backend, elements=st.floats(0, 10)
            )
        )
        unc = raw_unc

    return Quantity.from_input(
        mag_val, u, get_default_system(), uncertainty=unc
    )


@st.composite
def same_unit_quantities(
    draw,
    n: int = 2,
    backend: str | None = None,
    unit_strategy=None,
    allow_uncertainty: bool = False,
    dtype=np.float32,
) -> list[Quantity]:
    """Generates a list of N quantities with the same unit, backend, and shape."""
    # 1. Pick a shared backend
    if backend is None:
        candidates = ["numpy", "python"]
        if TORCH_AVAILABLE:
            candidates.append("torch")
        if JAX_AVAILABLE:
            candidates.append("jax")
        backend = draw(st.sampled_from(candidates))

    # 2. Pick a shared unit
    if unit_strategy is None:
        unit_strategy = available_units()
    unit_name = draw(unit_strategy)

    # 3. Pick a shared shape
    shape = draw(shapes())

    # 4. Generate N quantities
    qs = []
    for _ in range(n):
        # We use a trick to force unit and shape
        mag_strat = backend_arrays(shape=shape, backend=backend, dtype=dtype)
        qs.append(
            draw(
                quantities(
                    unit=st.just(unit_name),
                    backend=backend,
                    magnitude=mag_strat,
                    allow_uncertainty=allow_uncertainty,
                    dtype=dtype,
                )
            )
        )

    return qs


@st.composite
def same_shape_quantities(
    draw,
    n: int = 2,
    backend: str | None = None,
    allow_uncertainty: bool = False,
    dtype=np.float32,
) -> list[Quantity]:
    """Generates a list of N quantities with the same shape and backend, but potentially different units."""
    if backend is None:
        candidates = ["numpy", "python"]
        if TORCH_AVAILABLE:
            candidates.append("torch")
        if JAX_AVAILABLE:
            candidates.append("jax")
        backend = draw(st.sampled_from(candidates))

    shape = draw(shapes())

    qs = []
    for _ in range(n):
        mag_strat = backend_arrays(shape=shape, backend=backend, dtype=dtype)
        qs.append(
            draw(
                quantities(
                    backend=backend,
                    magnitude=mag_strat,
                    allow_uncertainty=allow_uncertainty,
                    dtype=dtype,
                )
            )
        )
    return qs
