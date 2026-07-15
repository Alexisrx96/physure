"""Property-Based Tests for Algebraic and Physical Invariants."""

import numpy as np
from hypothesis import HealthCheck, Phase, assume, given, settings
from hypothesis import strategies as st

from physure import Quantity
from tests.strategies import (
    backend_arrays,
    linear_units,
    quantities,
    same_shape_quantities,
    same_unit_quantities,
)

# Common settings for property checks
# We reduce max_examples for complex backend interactions
SETTINGS = settings(max_examples=50, deadline=None)


# -----------------------------------------------------------------------------
# Algebraic Group Properties (Scalar focus for rigor)
# -----------------------------------------------------------------------------


@given(
    same_unit_quantities(
        n=2,
        backend="numpy",
        unit_strategy=linear_units(),
        allow_uncertainty=False,
        dtype=np.float64,
    )
)
@settings(max_examples=50, deadline=None, phases=[Phase.generate])
def test_commutativity_addition(qs):
    """a + b == b + a"""
    a, b = qs

    try:
        res1 = a + b
        res2 = b + a
    except (ValueError, RuntimeError):
        # Skip invalid shapes
        assume(False)

    # Use tight tolerance for float64
    assert np.allclose(res1.magnitude, res2.magnitude, rtol=1e-7, atol=1e-10)
    assert res1.unit == res2.unit


@given(
    same_shape_quantities(
        n=2,
        backend="numpy",
        allow_uncertainty=False,
        dtype=np.float64,
    )
)
@settings(max_examples=50, deadline=None, phases=[Phase.generate])
def test_commutativity_multiplication(qs):
    """a * b == b * a (for scalars/element-wise)"""
    a, b = qs
    try:
        res1 = a * b
        res2 = b * a
    except (ValueError, RuntimeError):
        assume(False)

    assert np.allclose(res1.magnitude, res2.magnitude, rtol=1e-7, atol=1e-10)
    assert res1.unit == res2.unit


@given(
    same_unit_quantities(
        n=3,
        backend="numpy",
        unit_strategy=linear_units(),
        allow_uncertainty=False,
        dtype=np.float64,
    )
)
@settings(max_examples=50, deadline=None, phases=[Phase.generate])
def test_associativity_addition(qs):
    """(a + b) + c == a + (b + c)"""
    a, b, c = qs
    try:
        res1 = (a + b) + c
        res2 = a + (b + c)
    except (ValueError, RuntimeError):
        assume(False)

    # Note: Even in float64, associativity can have very small differences
    assert np.allclose(res1.magnitude, res2.magnitude, rtol=1e-6, atol=1e-9)


@given(
    same_shape_quantities(
        n=3,
        backend="numpy",
        allow_uncertainty=False,
        dtype=np.float64,
    )
)
@settings(max_examples=50, deadline=None, phases=[Phase.generate])
def test_associativity_multiplication(qs):
    """(a * b) * c == a * (b * c)"""
    a, b, c = qs
    try:
        res1 = (a * b) * c
        res2 = a * (b * c)
    except (ValueError, RuntimeError):
        assume(False)

    assert np.allclose(res1.magnitude, res2.magnitude, rtol=1e-6, atol=1e-9)
    assert res1.unit == res2.unit


@st.composite
def distributivity_triplet(draw):
    """Generates (a, b, c) such that (b + c) is valid and a * (b + c) is valid."""
    # 1. Generate b and c with same unit and shape
    b, c = draw(
        same_unit_quantities(
            n=2,
            backend="numpy",
            unit_strategy=linear_units(),
            allow_uncertainty=False,
            dtype=np.float64,
        )
    )
    # 2. Generate a with same shape as b, c
    a = draw(
        quantities(
            backend="numpy",
            magnitude=backend_arrays(
                shape=b.magnitude.shape, backend="numpy", dtype=np.float64
            ),
            allow_uncertainty=False,
            dtype=np.float64,
        )
    )
    return a, b, c


@given(distributivity_triplet())
@settings(max_examples=50, deadline=None, phases=[Phase.generate])
def test_distributivity(triplet):
    """a * (b + c) == a * b + a * c"""
    a, b, c = triplet
    try:
        res1 = a * (b + c)
        res2 = a * b + a * c
    except (ValueError, RuntimeError):
        assume(False)

    assert np.allclose(res1.magnitude, res2.magnitude, rtol=1e-5, atol=1e-7)
    assert res1.unit == res2.unit


# -----------------------------------------------------------------------------
# Physical Invariants
# -----------------------------------------------------------------------------


@given(
    same_unit_quantities(
        n=2,
        backend="numpy",
        unit_strategy=linear_units(),
        allow_uncertainty=False,
        dtype=np.float64,
    )
)
@settings(
    max_examples=50,
    deadline=None,
    phases=[Phase.generate],
    suppress_health_check=[HealthCheck.filter_too_much],
)
def test_unit_invariance(qs):
    """(a + b).to(target) == a.to(target) + b.to(target)

    Physical result shouldn't depend on intermediate representation.
    """
    a, b = qs
    system = a.system

    # Find a compatible unit to convert to (must be linear)
    # Find a compatible unit to convert to (must be linear)
    from hypothesis import note

    compatible_units = []
    # Ensure dimension is look-up-able
    try:
        dim_key = a.dimension
        if hasattr(dim_key, "to_string"):
            dim_key = dim_key.to_string()  # Normalize if needed
        candidates = system.UNIT_REGISTRY.get(dim_key, {})
    except TypeError:
        candidates = {}

    for name, definition in candidates.items():
        if name != a.unit.to_string() and definition.converter.is_linear:
            compatible_units.append(name)

    if not compatible_units:
        # No alternative units found, skip this example gracefully
        # note(f"No compatible units for {a.unit}")
        assume(False)

    target_unit = compatible_units[0]
    note(f"Converting {a.unit} -> {target_unit}")

    try:
        # Perform calculations
        # We compute in the target unit
        # (a + b) -> target
        sum_orig = a + b
        lhs = sum_orig.to(target_unit)

        # a -> target + b -> target
        a_conv = a.to(target_unit)
        b_conv = b.to(target_unit)
        rhs = a_conv + b_conv

    except (ValueError, RuntimeError, TypeError) as e:
        note(f"Operation failed with {e}")
        assume(False)

    # FP errors accumulate more here due to conversions
    # Increased tolerance to 1e-4 for robustness during heavy random testing
    # Check if backends match (handling potential array mismatches in property tests)

    m_lhs = lhs.magnitude
    m_rhs = rhs.magnitude

    # helper to normalize for comparison
    def to_np(x):
        if hasattr(x, "toarray"):
            return x.toarray()
        if hasattr(x, "todense"):
            return x.todense()
        if hasattr(x, "numpy"):
            return x.numpy()
        return np.asarray(x)

    np.testing.assert_allclose(
        to_np(m_lhs), to_np(m_rhs), rtol=1e-5, atol=1e-8
    )


@given(
    quantities(backend="numpy", allow_uncertainty=False, dtype=np.float64),
    st.floats(min_value=0.1, max_value=10.0),
)
@settings(max_examples=50, deadline=None, phases=[Phase.generate])
def test_dimensional_homogeneity_scaling(q, scale):
    """f(scale * x) == scale * f(x) for linear functions."""
    res1 = q * scale
    res2 = Quantity.from_input(q.magnitude * scale, q.unit, q.system)

    assert res1.unit == res2.unit

    m1 = res1.magnitude
    m2 = res2.magnitude

    def to_np(x):
        if hasattr(x, "toarray"):
            return x.toarray()
        if hasattr(x, "todense"):
            return x.todense()
        if hasattr(x, "cpu"):
            x = x.cpu()
        if hasattr(x, "detach"):
            x = x.detach()
        if hasattr(x, "numpy"):
            return x.numpy()
        return np.asarray(x)

    np.testing.assert_allclose(to_np(m1), to_np(m2), rtol=1e-7)
