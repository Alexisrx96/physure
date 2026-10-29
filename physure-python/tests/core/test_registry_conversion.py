"""A1 stage 2, step 3: the registry plans a conversion and its errors say what they were."""

import pytest

from physure._core import UnitMismatchError, UnitRegistry, UnknownUnitError


@pytest.fixture(scope="module")
def registry():
    return UnitRegistry.from_conf()


def _plan(registry, source, target):
    return registry.conversion(
        registry.parse_atomic(source), registry.parse_atomic(target)
    )


def test_a_factor_is_an_affine_plan_without_a_shift(registry):
    assert _plan(registry, "km", "m") == {
        "kind": "affine",
        "scale": 1000.0,
        "shift": 0.0,
    }


def test_two_temperature_scales_keep_their_zero_points(registry):
    assert _plan(registry, "degC", "degF") == {
        "kind": "affine",
        "scale": 1.8,
        "shift": 32.0,
    }
    # even a prefixed one, which Python could not resolve before
    assert _plan(registry, "centicelsius", "K") == {
        "kind": "affine",
        "scale": 0.01,
        "shift": 273.15,
    }


def test_a_level_goes_through_the_ratio_it_stands_for(registry):
    plan = _plan(registry, "dB", "percent")
    assert plan == {
        "kind": "level",
        "from": {"factor": 10.0, "reference": 1.0},
        "to": {"scale": 0.01},
    }
    # the same level under another name needs nothing
    assert _plan(registry, "decibel", "dB")["kind"] == "affine"


def test_an_unknown_unit_says_which_and_what_it_may_be(registry):
    with pytest.raises(UnknownUnitError, match="Unknown unit") as raised:
        _plan(registry, "metrr", "m")
    assert raised.value.symbol == "metrr"
    assert raised.value.suggestion
    assert isinstance(raised.value, ValueError)


def test_a_mismatch_says_both_units(registry):
    with pytest.raises(UnitMismatchError, match="mismatch") as raised:
        _plan(registry, "m", "s")
    assert (raised.value.expected, raised.value.actual) == ("m", "s")
    with pytest.raises(UnitMismatchError):
        _plan(registry, "rad", "sr")


def test_a_unit_that_takes_no_prefix_takes_none(registry):
    for name in ("kh", "kin", "kmi"):
        with pytest.raises(UnknownUnitError):
            registry.facts(name)
    assert registry.facts("km")["dimension"] == {"L": 1}


def test_the_registry_says_the_unit_of_a_dimension(registry):
    unit = registry.unit_of_dimension({"L": 2, "M": 1, "T": (-5, 2)})
    assert unit.dimensions == {"kg": (1, 1), "m": (2, 1), "s": (-5, 2)}
    with pytest.raises(ValueError, match="no base unit"):
        registry.unit_of_dimension({"X": 1})
