import pytest

from physure.domain.measurement.units import get_default_system

# We rely on the implicit default system which should be SI (international)
# The factory q = QuantityFactory()(value, unit) might work if unit is string
# Or we can inspect the units object if it's a registry proxy.


@pytest.fixture
def system():
    return get_default_system()


@pytest.fixture
def q(system):
    return system.Q_


def test_affine_subtraction_point_point(q):
    """20°C - 10°C == 10 delta_degC (matches 10K)."""
    t1 = q(20, "degC")
    t2 = q(10, "degC")

    # Check kinds (internal verification)
    assert t1.unit.kind(t1.system) == "absolute"

    result = t1 - t2

    # Result should be Delta (Vector)
    assert result.unit.kind(result.system) == "delta"

    # Value check: 20C - 10C = 10 units of difference (Kelvin/DeltaC)
    # 293.15 K - 283.15 K = 10 K.
    assert result.magnitude == pytest.approx(10.0)

    # Unit check: Should be Kelvin or delta_degC
    # Our implementation returns Base Unit (Kelvin) for P-P.
    # Let's verify it is physically 10K.
    # If we convert 10K to delta_degC, it should be 10.
    res_converted = result.to("delta_celsius")
    assert res_converted.magnitude == pytest.approx(10.0)


def test_affine_addition_point_vector(q):
    """20°C + 5 delta_degC == 25°C."""
    t1 = q(20, "degC")
    delta = q(5, "delta_degC")

    assert delta.unit.kind(delta.system) == "delta"

    result = t1 + delta

    # Result should be Point (Absolute)
    assert result.unit.kind(result.system) == "absolute"
    assert str(result.unit) in ["degC", "°C"]

    # Value: 20 + 5 = 25
    assert result.magnitude == pytest.approx(25.0)


def test_affine_subtraction_point_vector(q):
    """20°C - 5 delta_degC == 15°C."""
    t1 = q(20, "degC")
    delta = q(5, "delta_degC")

    result = t1 - delta
    assert result.unit.kind(result.system) == "absolute"
    assert result.magnitude == pytest.approx(15.0)


def test_affine_addition_point_point_error(q):
    """20°C + 10°C -> Raises Exception."""
    t1 = q(20, "degC")
    t2 = q(10, "degC")

    with pytest.raises(ValueError, match="Cannot add two absolute quantities"):
        _ = t1 + t2


def test_affine_conversion_absolute(q):
    """100°C (Abs) -> to Kelvin == 373.15 K."""
    t = q(100, "degC")
    assert t.unit.kind(t.system) == "absolute"

    k = t.to("kelvin")
    assert k.magnitude == pytest.approx(373.15)
    # Kelvin might be considered delta (linear) or absolute (point) depending
    # on context?
    # In physure, K is standard linear unit, so kind="delta".
    # Converting P -> V? No, Kelvin is the Base Unit for Temperature.
    # If we convert 100C (Point) to K? It usually implies Absolute
    # Temperature (Point).
    # But our taxonomy says Linear = Delta.
    # Does converting Point -> Linear Unit mean Point -> Vector?
    # Physically 373.15 K is an Absolute Temperature (from Absolute Zero).
    # So using a Linear unit for Absolute Temperature is valid if the
    # scale starts at 0.
    # Our logic mostly restricts P+P.
    # It does not prevent representing Points with Linear units.


def test_affine_conversion_delta(q):
    """100 delta_degC -> to Kelvin == 100 K."""
    d = q(100, "delta_degC")
    assert d.unit.kind(d.system) == "delta"

    k = d.to("kelvin")
    assert k.magnitude == pytest.approx(100.0)


def test_vector_vector_addition(q):
    """5 delta_degC + 10 delta_degC = 15 delta_degC."""
    d1 = q(5, "delta_degC")
    d2 = q(10, "delta_degC")

    res = d1 + d2
    assert res.magnitude == pytest.approx(15.0)
    assert res.unit.kind(res.system) == "delta"


def test_vector_point_error(q):
    """Vector - Point -> Error."""
    d = q(10, "delta_degC")
    p = q(20, "degC")

    with pytest.raises(
        ValueError, match="Cannot subtract an absolute quantity"
    ):
        _ = d - p


@pytest.mark.parametrize(
    "compare",
    [
        lambda a, b: a < b,
        lambda a, b: a > b,
        lambda a, b: a <= b,
        lambda a, b: a >= b,
    ],
)
def test_a_point_and_an_interval_have_no_order(q, compare):
    """5 delta_degC < 20 degC has no answer: the Rust core refuses the pair."""
    from physure.domain.exceptions import IncompatibleUnitsError

    interval, point = q(5, "delta_degC"), q(20, "degC")
    words = (
        "cannot compare a temperature point (`20.0 degC`) with a temperature "
        "interval (`5.0 delta_degC`)"
    )
    for a, b in ((interval, point), (point, interval)):
        with pytest.raises(IncompatibleUnitsError) as refused:
            compare(a, b)
        message = str(refused.value)
        assert message.startswith(words), message
        assert "compare a difference of two points" in message, message


def test_a_point_and_an_interval_are_never_equal(q):
    """`==` does not raise in Python: a point is not an interval, so it is False."""
    assert (q(5, "delta_degC") == q(5, "degC")) is False
    assert q(5, "delta_degC") != q(5, "degC")
    assert q(5, "degC") != q(5, "delta_degC")


def test_points_with_points_and_intervals_with_intervals_still_compare(q):
    """20 degC > 10 K, 5 delta_degC < 6 delta_degC, and K against an interval."""
    assert q(20, "degC") > q(10, "K")
    assert q(10, "K") < q(20, "degC")
    assert q(5, "delta_degC") < q(6, "delta_degC")
    assert q(5, "delta_degC") < q(6, "K")
    assert q(20, "degC") == q(68, "degF")
