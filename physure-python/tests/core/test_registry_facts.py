"""A1 stage 2, step 1: the registry object answers what Python answers.

A ``UnitSystem`` will own one of these (step 2). Before anything depends on it,
its answers are compared with the system's, over the units of the catalog.
"""

import json
from pathlib import Path

import pytest

from physure import Q_
from physure._core import UnitRegistry
from physure.application.context import get_current_system
from physure.domain.exceptions import UnknownUnitError
from physure.units._index import UNIT_INDEX

GOLDEN_FILE = Path(__file__).parent / "golden" / "unit_semantics.json"
FACT_FIELDS = ("dimension", "kind", "is_linear", "alias", "simplified")


def _plain(number):
    value = float(number)
    return int(value) if value.is_integer() else value


def _python_facts(expression):
    system = get_current_system()
    unit = Q_(1.0, expression).unit
    return {
        "dimension": {
            key: _plain(exp)
            for key, exp in sorted(unit.dimension(system).exponents.items())
        },
        "kind": unit.kind(system),
        "is_linear": unit.is_linear(system),
        "alias": unit.to_string(system, use_alias=True),
        "simplified": str(unit.simplify(system)),
    }


@pytest.fixture(scope="module")
def registry():
    return UnitRegistry.from_conf()


@pytest.mark.parametrize(
    "expression",
    sorted(json.loads(GOLDEN_FILE.read_text(encoding="utf-8"))),
)
def test_the_registry_reproduces_the_frozen_unit_semantics(
    registry, expression
):
    frozen = json.loads(GOLDEN_FILE.read_text(encoding="utf-8"))[expression]
    facts = registry.facts(expression)
    for field in FACT_FIELDS:
        assert facts[field] == frozen[field], field


def test_the_registry_agrees_with_python_on_every_unit_it_resolves(registry):
    checked = 0
    for name in sorted(UNIT_INDEX):
        try:
            python = _python_facts(name)
        except UnknownUnitError:
            continue
        rust = registry.facts(name)
        for field in FACT_FIELDS:
            assert rust[field] == python[field], (name, field)
        checked += 1
    assert checked > 2000


def test_the_pieces_of_a_unit_answer_like_the_facts(registry):
    unit = registry.parse_atomic("J/s")
    simplified = registry.simplify(unit)
    assert registry.dimension_of(simplified) == {"L": 2, "M": 1, "T": -3}
    assert registry.alias_of(simplified) == "W"
    assert registry.kind_of(registry.parse_atomic("degC")) == "absolute"
    assert registry.is_linear_of(registry.parse_atomic("dB")) is False
    assert registry.is_linear_of(unit) is True


@pytest.mark.parametrize(
    ("source", "target"),
    [
        ("km/h", "m/s"),
        ("kWh", "J"),
        ("psi", "Pa"),
        ("g/cm^3", "kg/m^3"),
        ("L/min", "m^3/s"),
        ("BTU/h", "W"),
        ("ft*lbf", "J"),
        ("mol/L", "mol/m^3"),
    ],
)
def test_the_factor_between_two_units_is_pythons(registry, source, target):
    system = get_current_system()
    python = Q_(1.0, source).unit.conversion_factor_to(
        Q_(1.0, target).unit, system
    )
    rust = registry.factor_between(
        registry.parse_atomic(source), registry.parse_atomic(target)
    )
    assert rust == python


def test_incompatible_units_have_no_factor(registry):
    with pytest.raises(ValueError, match="mismatch"):
        registry.factor_between(
            registry.parse_atomic("m"), registry.parse_atomic("s")
        )


def test_extra_conf_text_adds_units_to_that_registry_only():
    extra = "[Units]\nfurlong = 201.168, L, [fur, furlong]\n"
    registry = UnitRegistry.from_conf_text(extra)
    assert registry.facts("fur")["dimension"] == {"L": 1}
    assert registry.factor_between(
        registry.parse_atomic("fur"), registry.parse_atomic("m")
    ) == pytest.approx(201.168)
    with pytest.raises(ValueError, match="Unknown unit"):
        UnitRegistry.from_conf().facts("fur")
