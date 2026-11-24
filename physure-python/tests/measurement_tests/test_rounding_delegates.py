"""`round`, `math.floor`, `math.ceil` and `math.trunc` of a scalar `Quantity` are the core's.

The core (`Quantity::round`, `floor`, `ceil`, `trunc` in `physure-core/src/quantity.rs`) moves the
mean and keeps everything else: the unit, the uncertainty and its sources, so the degrees of
freedom and the correlation with the unrounded value survive. Ties go away from zero, as PHS
rounds. Python used to rebuild the quantity from its magnitude with its own `round` (ties to
even) and a new, independent uncertainty source.
"""

import math

import pytest

from physure import Q_
from physure.application.context import python_lineage, uncertainty_mode
from physure.domain.measurement.dimensions import Dimension

ROUNDINGS = [
    ("round", lambda q: round(q, 1)),
    ("floor", math.floor),
    ("ceil", math.ceil),
    ("trunc", math.trunc),
]


@pytest.mark.parametrize(
    ("value", "ndigits", "expected"),
    [
        (2.5, None, 3.0),
        (-2.5, None, -3.0),
        (0.5, None, 1.0),
        (0.125, 2, 0.13),
        (-0.125, 2, -0.13),
        (1234.5, -2, 1200.0),
        (1250.0, -2, 1300.0),
        (2.5, 0, 3.0),
    ],
)
def test_ties_go_away_from_zero_as_the_core_rounds(value, ndigits, expected):
    rounded = round(Q_(value, "m"), ndigits)
    assert rounded.magnitude == expected
    assert str(rounded.unit) == "m"


def test_rounding_to_many_places_or_a_huge_value_never_overflows():
    assert round(Q_(1.5, "m"), 400).magnitude == 1.5
    assert round(Q_(1.5, "m"), -400).magnitude == 0.0
    # Python's own `round` raised `OverflowError: rounded value too large to represent`
    assert round(Q_(1.7e308, "m"), -308).magnitude == 1.7e308
    assert round(Q_(1e300, "m"), 2).magnitude == 1e300
    assert round(Q_(2.5, "m"), 10**12).magnitude == 2.5


@pytest.mark.parametrize("ndigits", [2.0, 1.5, "1"])
def test_ndigits_must_be_an_integer(ndigits):
    with pytest.raises(TypeError):
        round(Q_(2.5, "m"), ndigits)


def test_ndigits_carrying_a_unit_is_refused():
    with pytest.raises(TypeError):
        round(Q_(2.5, "m"), Q_(1, "s"))


@pytest.mark.parametrize(("name", "op"), ROUNDINGS)
def test_the_rounded_value_keeps_the_source_of_the_unrounded_one(name, op):
    x = Q_(1.234, "m", 0.1)
    rounded = op(x)
    assert rounded.uncertainty == pytest.approx(0.1)
    assert (x - rounded).uncertainty == 0.0, name


@pytest.mark.parametrize("mode", ["monte_carlo", "unscented", "gaussian"])
@pytest.mark.parametrize(("name", "op"), ROUNDINGS)
def test_the_source_is_kept_in_every_uncertainty_mode(mode, name, op):
    with uncertainty_mode(mode):
        x = Q_(1.234, "m", 0.1)
        rounded = op(x)
        assert rounded.uncertainty == pytest.approx(0.1, rel=0.1)
        assert (x - rounded).uncertainty == pytest.approx(0.0, abs=1e-12), name


def test_the_source_is_kept_under_python_lineage():
    with python_lineage():
        x = Q_(1.234, "m", 0.1)
        assert (x - round(x, 1)).uncertainty == pytest.approx(0.0, abs=1e-12)


@pytest.mark.parametrize(("name", "op"), ROUNDINGS)
def test_degrees_of_freedom_are_kept(name, op):
    measured = Q_.from_samples(Q_([1.0, 2.0, 4.0], "V"))
    assert measured.dof == 2.0
    assert op(measured).dof == 2.0, name


def test_the_values_rounded_are_the_cores():
    x = Q_(-2.7, "m", 0.1)
    assert math.floor(x).magnitude == -3.0
    assert math.ceil(x).magnitude == -2.0
    assert math.trunc(x).magnitude == -2.0
    assert math.trunc(Q_(2.7, "m")).magnitude == 2.0


def test_a_temperature_point_stays_a_point():
    rounded = round(Q_(20.5, "degC"))
    assert rounded.magnitude == 21.0
    assert rounded.unit == Q_(1, "degC").unit
    assert rounded.to("K").magnitude == pytest.approx(294.15)


def test_a_prefixed_unit_rounds_in_its_own_unit():
    rounded = round(Q_(12.5, "mm"))
    assert rounded.magnitude == 13.0
    assert str(rounded.unit) == "mm"


def test_a_quantity_of_another_system_keeps_its_system(system):
    system.register_unit("ell", Dimension({"L": 1}), 1.143, "ell")
    q = system.Q_(2.5, "ell")
    rounded = round(q)
    assert rounded.magnitude == 3.0
    assert rounded.system is system
    assert rounded.unit == q.unit
    assert math.floor(q).system is system


def test_an_array_backed_magnitude_keeps_its_backend_rule():
    jnp = pytest.importorskip("jax.numpy")
    rounded = round(Q_(jnp.array([0.5, 1.5, 2.5]), "m"))
    # JAX rounds half to even, elementwise; the core is scalar
    assert [float(v) for v in rounded.magnitude] == [0.0, 2.0, 2.0]
    assert str(rounded.unit) == "m"
