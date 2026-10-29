"""A1 stage 2, step 2b: a system's core registry is built from what it registers."""

import pickle
from fractions import Fraction

import pytest

from physure.application.startup import create_system
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.system import UnitSystem

LENGTH = Dimension({"L": 1})


def _bare_system():
    system = UnitSystem("bare")
    system.register_prefix("k", 1000.0, "kilo", Fraction(1000))
    system.register_unit("m", LENGTH, 1.0, "meter", "meter")
    system.register_unit(
        "ft",
        LENGTH,
        Fraction(381, 1250),
        "foot",
    )
    return system


def test_what_a_system_registers_is_in_its_core_registry():
    registry = _bare_system().core_registry
    assert registry.facts("ft")["dimension"] == {"L": 1}
    factor = registry.factor_between(
        registry.parse_atomic("ft"), registry.parse_atomic("m")
    )
    assert factor == 0.3048
    # the prefix it registered applies, with its factor
    factor = registry.factor_between(
        registry.parse_atomic("km"), registry.parse_atomic("m")
    )
    assert factor == 1000.0


def test_membership_is_what_the_registry_resolves():
    system = _bare_system()
    assert "ft" in system
    assert "km" in system  # a prefix and a unit it was given
    assert "m/s" in system  # an expression of units it knows
    assert "nonsense" not in system
    # any text parsed also registers the eight SI base units
    assert "kg" in system
    assert system.core_registry.facts("kg")["dimension"] == {"M": 1}


@pytest.mark.parametrize(
    ("arguments", "message"),
    [
        ({"scale": "twice"}, "is not a number"),
        ({"scale": float("inf")}, "cannot be written"),
        ({"scale": 2.0, "level": (10.0, 1.0)}, "no scale"),
        ({"offset": 5.0, "level": (10.0, 1.0)}, "no zero point"),
    ],
)
def test_a_unit_a_conf_cannot_say_is_refused_and_changes_nothing(
    arguments, message
):
    system = _bare_system()
    with pytest.raises(ValueError, match=message):
        system.register_unit(
            "sq", Dimension({"L": 2}), name="square", **arguments
        )
    assert "sq" not in system
    with pytest.raises(ValueError, match="Unknown unit"):
        system.core_registry.facts("sq")


def test_a_name_a_conf_cannot_hold_is_refused_and_changes_nothing():
    system = _bare_system()
    with pytest.raises(ValueError, match="cannot be a name"):
        system.register_unit("a,b", LENGTH, 1.0, None)
    assert "a,b" not in system
    with pytest.raises(ValueError, match="cannot be a name"):
        system.register_prefix("x=", 2.0)


def test_a_system_pickles_and_its_registry_is_rebuilt():
    system = _bare_system()
    clone = pickle.loads(pickle.dumps(system))
    assert clone.core_registry.facts("km") == system.core_registry.facts("km")
    clone.register_unit("yd", LENGTH, 0.9144, "yard")
    assert clone.core_registry.facts("yd")["dimension"] == {"L": 1}
    with pytest.raises(ValueError, match="Unknown unit"):
        system.core_registry.facts("yd")


def test_a_unit_of_a_conf_is_in_the_registry(tmp_path, monkeypatch):
    conf = (
        "[Prefixes]\nzorro = zz, 1/3\n[Units]\nfurlong = 201.168, L, [fur]\n"
    )
    (tmp_path / "physure.conf").write_text(conf, encoding="utf-8")
    monkeypatch.chdir(tmp_path)
    registry = create_system().core_registry
    assert registry.factor_between(
        registry.parse_atomic("fur"), registry.parse_atomic("m")
    ) == pytest.approx(201.168)
    assert registry.factor_between(
        registry.parse_atomic("zzm"), registry.parse_atomic("m")
    ) == pytest.approx(1 / 3)


def test_a_conf_line_the_core_cannot_read_stops_the_system_being_built(
    tmp_path, monkeypatch
):
    conf = "[Units]\nleague = lots, L, [lg]\n"
    (tmp_path / "physure.conf").write_text(conf, encoding="utf-8")
    monkeypatch.chdir(tmp_path)
    with pytest.raises(ValueError, match="is not a number"):
        create_system()
