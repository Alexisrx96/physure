import math

import pytest
from hypothesis import HealthCheck, given, settings
from hypothesis import strategies as st

from measurekit.domain.measurement.quantity import Quantity

# Strategies for generating values
floats = st.floats(min_value=-1e12, max_value=1e12)
special_floats = st.floats(allow_nan=True, allow_infinity=True)


@settings(
    deadline=None, suppress_health_check=[HealthCheck.function_scoped_fixture]
)
@pytest.mark.parametrize("unit_name", ["m", "s", "kg"])
@given(a_val=floats, b_val=floats)
def test_commutativity_addition(common_system, unit_name, a_val, b_val):
    """Verify a + b == b + a"""
    unit = common_system.get_unit(unit_name)
    a = Quantity.from_input(a_val, unit, common_system)
    b = Quantity.from_input(b_val, unit, common_system)

    res1 = a + b
    res2 = b + a

    if math.isnan(a_val) or math.isnan(b_val):
        assert math.isnan(res1.magnitude)
        assert math.isnan(res2.magnitude)
    else:
        assert res1.magnitude == pytest.approx(res2.magnitude, nan_ok=True)


@settings(
    deadline=None, suppress_health_check=[HealthCheck.function_scoped_fixture]
)
@pytest.mark.parametrize("unit_name", ["m", "s", "kg"])
@given(a_val=floats, b_val=floats, c_val=floats)
def test_associativity_addition(common_system, unit_name, a_val, b_val, c_val):
    """Verify (a + b) + c == a + (b + c)"""
    unit = common_system.get_unit(unit_name)
    a = Quantity.from_input(a_val, unit, common_system)
    b = Quantity.from_input(b_val, unit, common_system)
    c = Quantity.from_input(c_val, unit, common_system)

    res1 = (a + b) + c
    res2 = a + (b + c)

    if any(math.isnan(v) for v in [a_val, b_val, c_val]):
        assert math.isnan(res1.magnitude)
        assert math.isnan(res2.magnitude)
    else:
        assert res1.magnitude == pytest.approx(
            res2.magnitude, nan_ok=True, rel=1e-6
        )


@settings(
    deadline=None, suppress_health_check=[HealthCheck.function_scoped_fixture]
)
@given(val=st.floats(min_value=1e-6, max_value=1e6))
def test_round_trip_conversion(common_system, val):
    """Verify Quantity(x).to(unit).to(original_unit) ≈ x"""
    m = common_system.get_unit("m")
    km = common_system.get_unit("km")

    q = Quantity.from_input(val, m, common_system)
    q_converted = q.to(km).to(m)

    assert q_converted.magnitude == pytest.approx(val)


@settings(
    deadline=None, suppress_health_check=[HealthCheck.function_scoped_fixture]
)
@given(val=special_floats)
def test_nan_inf_handling(common_system, val):
    """Ensure basic operations don't crash on NaN/Inf"""
    import warnings

    unit = common_system.get_unit("m")
    q = Quantity.from_input(val, unit, common_system)

    # Simple arithmetic should not raise exception
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        try:
            _ = q * 2
            _ = q + q
            _ = q / 2
        except Exception as e:
            pytest.fail(f"Operation raised exception on value {val}: {e}")
