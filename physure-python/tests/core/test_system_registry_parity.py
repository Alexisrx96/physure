"""A1 stage 2, step 2b: the registry of a system answers what the system answers.

Nothing reads the registry of a system yet. Before something does (step 3), its
answers are compared with Python's, system by system: the dimension, the kind
and the linearity of every name Python resolves, and the factor between the
linear units of a dimension.
"""

import collections
from fractions import Fraction

import pytest

from physure.application.context import use_system
from physure.application.startup import create_default_system, create_system
from physure.domain.exceptions import UnknownUnitError
from physure.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
    OffsetConverter,
)
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.system import UnitSystem
from physure.domain.measurement.units import CompoundUnit, units
from physure.ext.currency import enable_currency

FIELDS = ("dimension", "kind", "is_linear")
CATALOG_NAMES = dir(units)  # the names of the catalog written as units.<name>


def _plain(number):
    value = float(number)
    return int(value) if value.is_integer() else value


def _python_facts(system, name):
    unit = system.Q_(1.0, name).unit
    return {
        "dimension": {
            key: _plain(exp)
            for key, exp in sorted(unit.dimension(system).exponents.items())
        },
        "kind": unit.kind(system),
        "is_linear": unit.is_linear(system),
    }


def _the_registry_agrees(system, names):
    """Checks every name Python resolves, and returns how many there were."""
    registry = system.core_registry
    resolved = {}
    with use_system(system):
        for name in names:
            try:
                resolved[name] = _python_facts(system, name)
            except UnknownUnitError:
                continue
        for name, python in resolved.items():
            facts = registry.facts(name)
            for field in FIELDS:
                assert facts[field] == python[field], (name, field)
        # the factor of each linear unit to the first unit of its dimension
        groups = collections.defaultdict(list)
        for name, python in resolved.items():
            if python["is_linear"]:
                groups[tuple(python["dimension"].items())].append(name)
        for dimension, group in groups.items():
            references = [group[0]]
            for name in group[1:]:
                for reference in references:
                    try:
                        factor = registry.factor_between(
                            registry.parse_atomic(name),
                            registry.parse_atomic(reference),
                        )
                    except ValueError:
                        # Python files an angle and a solid angle under one
                        # dimension and converts between them; the core keeps
                        # them apart. Known, and closed by step 6
                        # (test_python_does_not_mix_angle_and_solid_angle).
                        assert dimension == (("A", 1),), (name, reference)
                        continue
                    python = system.Q_(1.0, name).to(reference).magnitude
                    assert python == pytest.approx(factor, rel=1e-12), (
                        name,
                        reference,
                    )
                    break
                else:
                    references.append(name)  # another kind of the dimension
    return len(resolved)


def test_the_default_system():
    # every fourth name of the catalog: all of them are compared with the
    # catalog's own registry in test_registry_facts.py
    system = create_default_system()
    assert _the_registry_agrees(system, CATALOG_NAMES[::4]) > 500


def test_the_imperial_system():
    system = create_system("imperial.conf")
    assert _the_registry_agrees(system, CATALOG_NAMES[::4]) > 500


def test_a_system_with_the_conf_of_the_user(tmp_path, monkeypatch):
    conf = (
        "[Prefixes]\nzorro = zz, 0.5\n"
        "[Units]\nfurlong = 201.168, L, [fur, furlong]\n"
        "smoot = 1.7018, L, [smoot]\n"
    )
    (tmp_path / "physure.conf").write_text(conf, encoding="utf-8")
    monkeypatch.chdir(tmp_path)
    names = ["fur", "furlong", "smoot", "zzm", "zorrometer", "m"]
    assert _the_registry_agrees(create_system(), names) == len(names)


def test_a_system_with_currency():
    system = create_default_system()
    enable_currency(system)
    names = ["USD", "EUR", "MXN", "dollars", "dolar", "euros", "pesos", "m"]
    assert _the_registry_agrees(system, names) == len(names)
    registry = system.core_registry
    assert registry.facts("EUR")["dimension"] == {"$": 1}
    factor = registry.factor_between(
        registry.parse_atomic("EUR"), registry.parse_atomic("USD")
    )
    assert factor == 1.08


def test_a_bare_system_with_units_registered_by_hand():
    system = UnitSystem("bare")
    length, mass, time = (Dimension({s: 1}) for s in ("L", "M", "T"))
    heat = Dimension({"O": 1})
    energy = Dimension({"M": 1, "L": 2, "T": -2})
    power = Dimension({"M": 1, "L": 2, "T": -3})
    system.register_prefix("k", 1000.0, "kilo", Fraction(1000))
    system.register_unit("m", length, LinearConverter(1.0), "meter", "meter")
    system.register_unit("cm", length, LinearConverter(0.01), "centimeter")
    system.register_unit(
        "ft",
        length,
        LinearConverter(0.3048, exact=Fraction(381, 1250)),
        "foot",
    )
    system.register_unit("kg", mass, LinearConverter(1.0), "kilogram")
    system.register_unit("s", time, LinearConverter(1.0), "second")
    system.register_unit(
        "J",
        energy,
        LinearConverter(1.0),
        "joule",
        recipe=CompoundUnit({"kg": 1, "m": 2, "s": -2}),
    )
    system.register_unit("W", power, LinearConverter(1.0), "watt")
    system.register_unit(
        "dBW",
        power,
        LogarithmicConverter(10.0),
        "decibel-watt",
        allow_prefixes=False,
    )
    system.register_unit("K", heat, LinearConverter(1.0), "kelvin")
    system.register_unit(
        "degX", heat, OffsetConverter(1.0, 10.0), "x", kind="absolute"
    )
    names = [
        "m", "meter", "cm", "ft", "kg", "s", "J", "W", "dBW", "K", "degX",
        "km", "kilometer",
    ]  # fmt: skip
    assert _the_registry_agrees(system, names) == len(names)
