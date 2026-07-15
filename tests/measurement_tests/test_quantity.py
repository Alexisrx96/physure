"""Test suite for the Quantity class using pytest."""

import numpy as np
import pytest

from measurekit.domain.exceptions import IncompatibleUnitsError
from measurekit.domain.measurement.converters import LinearConverter
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.units import CompoundUnit


@pytest.fixture
def quantity_system(system):
    """Set up a fresh UnitSystem for quantity tests."""
    length = Dimension({"L": 1})
    time = Dimension({"T": 1})
    mass = Dimension({"M": 1})

    system.register_unit("m", length, LinearConverter(1.0), "meter")
    system.register_unit("cm", length, LinearConverter(0.01), "centimeter")
    system.register_unit("km", length, LinearConverter(1000.0), "kilometer")
    system.register_unit("s", time, LinearConverter(1.0), "second")
    system.register_unit("min", time, LinearConverter(60.0), "minute")
    system.register_unit("h", time, LinearConverter(3600.0), "hour")
    system.register_unit("kg", mass, LinearConverter(1.0), "kilogram")
    system.register_unit("g", mass, LinearConverter(0.001), "gram")
    system.register_unit("rad", Dimension({}), LinearConverter(1.0), "radian")

    return system


@pytest.fixture
def units(quantity_system):
    """Provides common unit objects."""
    return {
        "meter": CompoundUnit({"m": 1}),
        "centimeter": CompoundUnit({"cm": 1}),
        "kilometer": CompoundUnit({"km": 1}),
        "second": CompoundUnit({"s": 1}),
        "gram": CompoundUnit({"g": 1}),
    }


def test_initialization(quantity_system, units):
    """Test basic initialization patterns."""
    meter = units["meter"]
    q1 = quantity_system.Q_(5.0, meter)
    assert q1.magnitude == 5.0
    assert q1.unit == meter
    assert q1.dimension == meter.dimension(quantity_system)
    assert q1.system is quantity_system


def test_conversion(quantity_system, units):
    """Test unit conversion with the to method."""
    meter = units["meter"]
    centimeter = units["centimeter"]
    kilometer = units["kilometer"]

    length = quantity_system.Q_(5.0, meter)
    length_cm = length.to(centimeter)
    assert length_cm.magnitude == 500.0
    assert length_cm.unit == centimeter

    length_km_str = length.to("km")
    assert np.isclose(length_km_str.magnitude, 0.005)
    assert length_km_str.unit == kilometer


def test_arithmetic_operations(quantity_system, units):
    """Test arithmetic operations between quantities."""
    meter = units["meter"]
    second = units["second"]

    length1 = quantity_system.Q_(5.0, meter)
    length2 = quantity_system.Q_(10.0, meter)
    time = quantity_system.Q_(2.0, second)

    sum_length = length1 + length2
    assert sum_length.magnitude == 15.0
    assert sum_length.unit == meter

    diff_length = length2 - length1
    assert diff_length.magnitude == 5.0

    double_length = length1 * 2
    assert double_length.magnitude == 10.0

    velocity = length1 / time
    assert velocity.magnitude == 2.5
    assert velocity.unit.exponents == {"m": 1, "s": -1}


def test_comparison_operations(quantity_system):
    """Test comparison operations between quantities."""
    length1 = quantity_system.Q_(5.0, "m")
    length2 = quantity_system.Q_(500.0, "cm")
    length3 = quantity_system.Q_(10.0, "m")

    assert length1 == length2
    assert length1 != length3
    assert length1 < length3
    assert length3 > length1


def test_uncertainty_propagation(quantity_system, units):
    """Test the propagation of uncertainty for basic arithmetic."""
    meter = units["meter"]
    q1 = quantity_system.Q_(10.0, meter, uncertainty=0.1)
    q2 = quantity_system.Q_(5.0, meter, uncertainty=0.2)

    result_add = q1 + q2
    assert np.isclose(result_add.magnitude, 15.0)
    assert np.isclose(result_add.uncertainty, 0.22361, atol=1e-5)


def test_rtruediv_uncertainty(quantity_system, units):
    """Test uncertainty for inverse division (1/q)."""
    meter = units["meter"]
    q = quantity_system.Q_(4.0, meter, uncertainty=0.1)
    result = 1 / q

    assert np.isclose(result.magnitude, 0.25)
    assert result.unit.exponents == {"m": -1}
    assert np.isclose(result.uncertainty, 0.00625)


def test_dunder_methods(quantity_system):
    """Test various dunder methods."""
    q1 = quantity_system.Q_(10, "m", uncertainty=0.1)
    q2 = quantity_system.Q_(5, "m")

    assert quantity_system.Q_(5, "m") - q2 == quantity_system.Q_(0, "m")
    assert (-q1).magnitude == -10
    assert (+q1).magnitude == 10
    assert abs(quantity_system.Q_(-5, "m")).magnitude == 5
    assert float(q2) == 5.0

    assert repr(q1) in [
        "Quantity(10.0, m, uncertainty=0.1)",
        "Quantity(10, m, uncertainty=0.1)",
    ]
    assert str(q1) in ["(10.0 ± 0.1) m", "(10 ± 0.1) m"]
    assert str(q2) in ["5.0 m", "5 m"]

    q_arr_unc = quantity_system.Q_(10, "m", uncertainty=np.array([0.1, 0.2]))
    assert "uncertainty=" in repr(q_arr_unc)


def test_comparison_edge_cases(quantity_system):
    """Test __le__, __ge__ and comparisons with non-quantities."""
    q1 = quantity_system.Q_(5, "m")
    q2 = quantity_system.Q_(5, "m")
    q3 = quantity_system.Q_(10, "m")

    assert q1 <= q2
    assert q1 <= q3
    assert q2 >= q1
    assert q3 >= q1
    assert q1 != 5

    with pytest.raises(TypeError):
        _ = q1 < 5
    with pytest.raises(TypeError):
        _ = q1 <= 5
    with pytest.raises(TypeError):
        _ = q1 > 5
    with pytest.raises(TypeError):
        _ = q1 >= 5


def test_numpy_ufuncs(quantity_system):
    """Test interactions with NumPy universal functions."""
    angle = quantity_system.Q_(np.pi / 2, "rad", 0.01)
    assert np.isclose(np.sin(angle).magnitude, 1.0)
    assert np.isclose(np.cos(angle).magnitude, 0.0)
    assert np.isclose(
        np.tan(quantity_system.Q_(np.pi / 4, "rad")).magnitude, 1.0
    )

    with pytest.raises(IncompatibleUnitsError):
        np.sin(quantity_system.Q_(1, "m"))

    area = quantity_system.Q_(16, "m**2")
    side = np.sqrt(area)
    assert side.magnitude == 4.0
    assert side.unit.exponents == {"m": 1.0}
    assert np.square(side) == area

    arr_q = quantity_system.Q_(np.array([1, 2, 3]), "m")
    assert np.add.reduce(arr_q) == quantity_system.Q_(6, "m")

    result = quantity_system.Q_(np.array([-1, -2]), "m")
    expected = quantity_system.Q_(np.array([1, 2]), "m")
    assert np.all(np.absolute(result) == expected)


def test_vector_and_array_ops(quantity_system):
    """Test dot, cross, len, and getitem."""
    v1 = quantity_system.Q_(np.array([1, 0, 0]), "m")
    v2 = quantity_system.Q_(np.array([0, 2, 0]), "m")

    assert v1.dot(v2).magnitude == 0
    assert v1.dot(v2).unit.exponents == {"m": 2}

    cross_prod = v1.cross(v2)
    np.testing.assert_array_equal(cross_prod.magnitude, [0, 0, 2])
    assert cross_prod.unit.exponents == {"m": 2}

    assert len(v1) == 3
    assert v1[0] == quantity_system.Q_(1, "m")
    np.testing.assert_array_equal(v1[1:].magnitude, np.array([0, 0]))

    with pytest.raises(TypeError):
        len(quantity_system.Q_(1, "m"))
    with pytest.raises(TypeError):
        _ = (quantity_system.Q_(1, "m"))[0]


def test_formatting(quantity_system):
    """Test the __format__ method with composable flags and default alias behavior."""
    q = quantity_system.Q_(1234.567, "m/s**2", 0.02)
    assert format(q, ".2f") == "(1234.57 ± 0.02) m/s²"

    quantity_system.register_alias({"m": 1, "s": -2}, "acceleration")
    assert format(q, "alias") == "(1234.567 ± 0.02) acceleration"
    assert format(q, ".1f|alias") == "(1234.6 ± 0.0) acceleration"

    from measurekit import get_active_system
    sys = get_active_system()
    q_force = sys.Q_(2.0, "N")
    assert str(q_force) == "2.0 N"
    assert format(q_force, ".2f") == "2.00 N"
    assert format(q_force, ".3e") == "2.000e+00 N"
    assert format(q_force, "base") == "2.0 kg·m/s²"
    assert format(q_force, "raw") == "2.0 kg·m/s²"
    assert format(q_force, ".2f|base") == "2.00 kg·m/s²"
    assert format(q_force, ".3e|base") == "2.000e+00 kg·m/s²"
    assert format(q_force, ".4f|alias") == "2.0000 N"

    q_frac = sys.Q_(1.5, "N")
    assert format(q_frac, "frac") == "3/2 N"
    assert format(q_frac, "frac|base") == "3/2 kg·m/s²"


def test_to_base_units():
    """Test converting a derived unit quantity to base SI units."""
    from measurekit import get_active_system
    sys = get_active_system()
    force = sys.Q_(2.0, "N")
    base_force = force.to_base_units()
    assert base_force.unit.to_string(sys) == "kg·m/s²"
    assert base_force.magnitude == 2.0




def test_dimensionless_display_omits_unit(quantity_system):
    """A dimensionless result (e.g. m/m) should not display a trailing '1'."""
    ratio = quantity_system.Q_(1, "m") / quantity_system.Q_(1, "m")
    assert str(ratio) == "1.0"
    assert format(ratio, "alias") == "1.0"
    assert ratio.to_latex() == "1.0"

    ratio_unc = quantity_system.Q_(1, "m", 0.1) / quantity_system.Q_(1, "m")
    assert str(ratio_unc) == "(1.0 ± 0.1)"


def test_latex_representation(quantity_system):
    """Test LaTeX output."""
    q_unc = quantity_system.Q_(10, "m/s", 0.1)
    q_no_unc = quantity_system.Q_(5, "kg")

    assert q_unc.to_latex() in [
        "(10.0 \\pm 0.1) \\; \\frac{m}{s}",
        "(10 \\pm 0.1) \\; \\frac{m}{s}",
    ]
    assert q_no_unc.to_latex() in ["5.0 \\; kg", "5 \\; kg"]
    assert q_unc._repr_latex_() == f"${q_unc.to_latex()}$"


def test_multiplication_with_unit(quantity_system):
    """Test multiplying a Quantity by a CompoundUnit."""
    q = quantity_system.Q_(10, "m")
    unit_s = quantity_system.get_unit("s")
    result = q * unit_s
    assert result.magnitude == 10
    assert result.unit.exponents == {"m": 1, "s": 1}


def test_division_by_unit(quantity_system):
    """Test dividing a Quantity by a CompoundUnit."""
    q = quantity_system.Q_(10, "m")
    unit_s = quantity_system.get_unit("s")
    result = q / unit_s
    assert result.magnitude == 10
    assert result.unit.exponents == {"m": 1, "s": -1}


def test_round(quantity_system):
    """Test rounding a Quantity."""
    q = quantity_system.Q_(3.14159, "rad")
    assert round(q, 2).magnitude == 3.14
    assert round(q).magnitude == 3.0


def test_hash(quantity_system):
    """Test that Quantity instances are hashable."""
    q1 = quantity_system.Q_(5, "m")
    q2 = quantity_system.Q_(5, "m")
    q3 = quantity_system.Q_(10, "m")

    assert hash(q1) == hash(q2)
    assert hash(q1) != hash(q3)


def test_edge_case_operations(quantity_system):
    """Test edge cases in arithmetic operations."""
    q = quantity_system.Q_(10, "m")

    assert (q * 0).magnitude == 0
    assert (0 * q).magnitude == 0
    assert (q / 1).magnitude == 10
    assert (1 / q).magnitude == 0.1
    assert (1 / q).unit.exponents == {"m": -1}

    zero_q = quantity_system.Q_(0, "m")
    assert (q + zero_q).magnitude == 10
    assert (zero_q + q).magnitude == 10
    assert (q - q).magnitude == 0


def test_invalid_operations(quantity_system):
    """Test operations that should raise errors."""
    q_length = quantity_system.Q_(10, "m")
    q_time = quantity_system.Q_(5, "s")

    with pytest.raises(IncompatibleUnitsError):
        _ = q_length + q_time
    with pytest.raises(IncompatibleUnitsError):
        _ = q_length - q_time
    with pytest.raises(IncompatibleUnitsError):
        _ = q_length < q_time
    with pytest.raises(TypeError):
        _ = q_length * "invalid"
    with pytest.raises(TypeError):
        _ = q_length / "invalid"


def test_division_with_multiple_units(quantity_system):
    """Test division resulting in compound units."""
    q1 = quantity_system.Q_(20, "m")
    q2 = quantity_system.Q_(4, "s")
    result = q1 / q2
    assert result.magnitude == 5
    assert result.unit.exponents == {"m": 1, "s": -1}

    q3 = quantity_system.Q_(4, "m")
    q4 = quantity_system.Q_(2, "s")
    result = q3 / q4
    assert result.magnitude == 2
    assert result.unit.exponents == {"m": 1, "s": -1}


def test_simplification_by_multiplication(quantity_system):
    """Test simplification of units through multiplication."""
    q1 = quantity_system.Q_(10, "m/s")
    q2 = quantity_system.Q_(2, "s")
    result = q1 * q2
    assert result.magnitude == 20
    assert result.unit.exponents == {"m": 1}


def test_simplification_by_division(quantity_system):
    """Test simplification of units through division."""
    q1 = quantity_system.Q_(10, "m")
    q2 = quantity_system.Q_(2, "m/s")
    result = q1 / q2
    assert result.magnitude == 5
    assert result.unit.exponents == {"s": 1}


def test_subtraction_with_uncertainty(quantity_system):
    """Test subtraction where one quantity has uncertainty."""
    q1 = quantity_system.Q_(10.0, "m", uncertainty=0.1)
    q2 = quantity_system.Q_(3.0, "m")
    result = q1 - q2
    assert np.isclose(result.magnitude, 7.0)
    assert np.isclose(result.uncertainty, 0.1)


def test_subtraction(quantity_system):
    """Test subtraction where both quantities have uncertainty."""
    q1 = quantity_system.Q_(10.0, "m")
    q2 = quantity_system.Q_(3.0, "m")
    result = q1 - q2
    assert np.isclose(result.magnitude, 7.0)


def test__rsub__(quantity_system):
    """Test right-side subtraction."""
    q1 = quantity_system.Q_(10.0, "m")
    q2 = quantity_system.Q_(3.0, "m")
    result = q2 - q1
    assert np.isclose(result.magnitude, -7.0)


def test_repr_html_returns_string(quantity_system, units):
    """Test that _repr_html_ returns a string."""
    from measurekit.domain.measurement.quantity import Quantity

    q = Quantity(10.0, units["meter"], system=quantity_system)
    html = q._repr_html_()
    assert isinstance(html, str)
    assert "10.0" in html
    assert "<span" in html


def test_repr_html_dimensionless(quantity_system):
    """Test that _repr_html_ handles dimensionless quantities."""
    from measurekit.domain.measurement.quantity import Quantity

    q = Quantity(1.0, CompoundUnit({}), system=quantity_system)
    html = q._repr_html_()
    assert "dimensionless" in html


def test_repr_mimebundle_keys(quantity_system, units):
    """Test that _repr_mimebundle_ returns correct MIME types."""
    from measurekit.domain.measurement.quantity import Quantity

    q = Quantity(10.0, units["meter"], system=quantity_system)
    bundle = q._repr_mimebundle_()
    assert "text/plain" in bundle
    assert "text/latex" in bundle
    assert "text/html" in bundle


def test_repr_html_with_uncertainty(quantity_system, units):
    """Test that _repr_html_ shows &plusmn; when uncertainty is present."""
    from measurekit.domain.measurement.quantity import Quantity

    q = Quantity(10.0, units["meter"], uncertainty=0.5, system=quantity_system)
    html = q._repr_html_()
    assert "&plusmn;" in html
    assert "0.5" in html


def test_m_alias(quantity_system, units):
    """Test .m property alias for magnitude."""
    from measurekit.domain.measurement.quantity import Quantity

    q = Quantity(42.0, units["meter"], system=quantity_system)
    assert q.m == 42.0
    assert q.m is q.magnitude


def test_u_alias(quantity_system, units):
    """Test .u property alias for unit."""
    from measurekit.domain.measurement.quantity import Quantity

    q = Quantity(42.0, units["meter"], system=quantity_system)
    assert q.u is q.unit


def test_scalar_unpack(quantity_system, units):
    """Test scalar unpacking via __iter__."""
    from measurekit.domain.measurement.quantity import Quantity

    q = Quantity(42.0, units["meter"], system=quantity_system)
    mag, unit = q
    assert mag == 42.0
    assert unit is q.unit


def test_scalar_unpack_numpy_scalar(quantity_system, units):
    """numpy 0-d arrays should unpack as (magnitude, unit) not return []."""
    import numpy as np

    from measurekit.domain.measurement.quantity import Quantity

    q = Quantity(np.float64(42.0), units["meter"], system=quantity_system)
    mag, unit = q
    assert float(mag) == pytest.approx(42.0)
    assert unit is q.unit


def test_array_iter_unchanged(quantity_system, units):
    """Array __iter__ must still yield individual Quantity elements."""
    from measurekit.domain.measurement.quantity import Quantity

    arr = Quantity(
        np.array([1.0, 2.0, 3.0]), units["meter"], system=quantity_system
    )
    elements = list(arr)
    assert len(elements) == 3
    assert elements[0].magnitude == pytest.approx(1.0)
