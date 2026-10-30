"""A prefixed unit is not held by the registry: it is read as a prefix and a
unit. These tests confirm that it resolves to the right dimensions and factors,
that whole-unit names win over decomposition, and that compound expressions
resolve per symbol.
"""

from physure import Q_
from physure.application.context import get_current_system


def test_prefixed_symbol_forms_resolve():
    assert Q_(1.0, "km").to("m").magnitude == 1000.0
    assert Q_(1.0, "ms").to("s").magnitude == 0.001
    assert Q_(1.0, "kPa").to("Pa").magnitude == 1000.0
    assert Q_(1.0, "MHz").to("Hz").magnitude == 1_000_000.0
    assert Q_(1.0, "nm").to("m").magnitude == 1e-9
    assert Q_(1.0, "mm").to("m").magnitude == 0.001
    assert Q_(1.0, "cm").to("m").magnitude == 0.01
    assert Q_(1.0, "GHz").to("Hz").magnitude == 1e9


def test_prefixed_name_form_resolves():
    assert Q_(1.0, "kilometer").to("m").magnitude == 1000.0


def test_whole_units_win_over_decomposition():
    # "min" must resolve to minute, not milli-in (m + in); "cd" to candela,
    # not centi-d; "Pa" to pascal, not peta-a.
    system = get_current_system()
    assert Q_(1, "min").to("s").magnitude == 60.0
    assert str(system.get_unit("cd")) == "cd"
    assert str(system.get_unit("Pa")) == "kg/(m·s²)"


def test_compound_expression_resolves_per_symbol():
    v = Q_(1.0, "km/ms")
    assert v.to("m/s").magnitude == 1_000_000.0


def test_a_prefixed_unit_is_read_from_the_registry():
    system = get_current_system()
    unit = system.get_unit("THz")
    assert unit is not None
    assert "THz" in system
    assert "THz" not in system.core_registry.names()
