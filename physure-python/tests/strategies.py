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
from physure.domain.measurement.converters import LinearConverter
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


@st.composite
def available_units(draw) -> str:
    """Selects a unit name from the default system registry.

    If the registry is empty (unlikely after startup), falls back to a small set.
    """
    system = get_default_system()
    # Ensure some units are loaded/discovered if using lazy loading
    # Ideally we'd trigger discovery here if empty, but accessing keys usually triggers.
    if hasattr(system, "UNIT_REGISTRY"):
        # We manually peek or just try to get known units
        # For simplicity, we use a predefined list of common units to ensure stability
        # and avoid testing "weird" units unless specifically asked.
        # But we can also mix in system.available_units if accessible.
        pass

    # Common units for physics tests
    common = [
        "m",
        "s",
        "kg",
        "A",
        "K",
        "mol",
        "cd",
        "m/s",
        "N",
        "J",
        "Pa",
        "W",
        "V",
    ]

    # Try to add registered ones
    try:
        registered = sorted(list(system.UNIT_SYMBOL_REGISTRY.keys()))
        if registered:
            # Sample from registered to get coverage
            return draw(st.sampled_from(registered))
    except Exception:
        pass

    return draw(st.sampled_from(common))


@st.composite
def linear_units(draw) -> str:
    """Selects a linear unit from the registry (excludes affine/log)."""
    system = get_default_system()

    # Get all units that have a LinearConverter
    linear_candidates = []

    # 1. Check registry
    if hasattr(system, "UNIT_SYMBOL_REGISTRY"):
        for name, definition in system.UNIT_SYMBOL_REGISTRY.items():
            if isinstance(definition.converter, LinearConverter):
                # Optionally filter out "strange" units if needed
                linear_candidates.append(name)

    # 2. Fallback to common linear ones if empty
    if not linear_candidates:
        linear_candidates = ["m", "s", "kg", "A", "N", "J", "Pa", "W", "V"]

    return draw(st.sampled_from(linear_candidates))


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
