"""A1, Stage 0: Python's unit algebra compared with the Rust core.

Rust is the oracle for conversions: the PHS ``=>`` operator runs inside
physure-core's registry and returns a native Quantity whose magnitude is a float
computed by Rust. Python is compared against it with exact equality, over the
whole catalog and over a set of representative compound units.

Every difference is classified. An unclassified difference fails the suite, and
a known gap that disappears turns its strict ``xfail`` into a failure, so the
list of gaps can only shrink. Stages 1 and 2 of A1 close the gaps; this file is
their acceptance test.

Set ``PHYSURE_UPDATE_GOLDEN=1`` to rewrite the golden file after reviewing a
deliberate change in the frozen behaviour.
"""

from __future__ import annotations

import collections
import configparser
import itertools
import json
import math
import os
from pathlib import Path
from typing import NamedTuple

import pytest

import physure._core as core
from physure import Q_
from physure.application.context import get_current_system
from physure.domain.exceptions import UnknownUnitError
from physure.domain.measurement.units import units

GOLDEN_FILE = Path(__file__).parent / "golden" / "unit_semantics.json"
UPDATE_GOLDEN = os.environ.get("PHYSURE_UPDATE_GOLDEN") == "1"

# 1.0 shows scale factors; 25.0 also shows the offset of temperature scales.
VALUES = (1.0, 25.0)

# Each unit is converted to this many reference units of its dimension. One
# is not enough: a dimension can hold several kinds (angles and solid angle,
# ratios and logarithmic units), and a reference of one kind hides the rest.
REFERENCES_PER_GROUP = 2

# Compound conversions (value, from, to) every implementation must agree on.
COMPOUND_CASES = (
    (1.0, "km/h", "m/s"),
    (1.0, "kg*m/s^2", "N"),
    (1.0, "N*m", "J"),
    (1.0, "J/s", "W"),
    (1.0, "kWh", "J"),
    (1.0, "psi", "Pa"),
    (1.0, "mmHg", "Pa"),
    (1.0, "g/cm^3", "kg/m^3"),
    (1.0, "L/min", "m^3/s"),
    (1.0, "eV", "J"),
    (1.0, "cal", "J"),
    (1.0, "BTU/h", "W"),
    (1.0, "hp", "W"),
    (25.0, "degC", "K"),
    (77.0, "degF", "degC"),
    (1.0, "mol/L", "mol/m^3"),
    (1.0, "ft*lbf", "J"),
)

# Units whose semantics are frozen so Stage 1 has a target to match.
SEMANTIC_EXPRESSIONS = (
    "m", "km", "kg", "s", "K", "Pa", "Hz",
    "kg*m/s^2", "N*m", "J/s", "km/h", "m/s^2", "g/cm^3", "L/min", "mol/L",
    "degC", "degF", "delta_celsius", "delta_fahrenheit",
    "percent", "ppm", "dB", "pH",
    "psi", "mmHg", "gallon", "eV", "cal", "hp", "kWh", "BTU/h", "ft*lbf",
)  # fmt: skip


CLASSIFIED_KINDS = {
    "last-bit-difference",
    "rust-rejects-long-prefix-name",
    "rust-does-not-know-unit",
    "rust-rejects-delta-temperature",
    "angle-and-solid-angle-merged-by-python",
}


class Difference(NamedTuple):
    symbol: str
    dimension: str
    reference: str
    value: float
    python: object
    rust: object
    kind: str


class Comparison(NamedTuple):
    groups: dict
    reference: dict
    unresolvable: list
    differences: list


def _long_prefix_names() -> set[str]:
    parser = configparser.ConfigParser(interpolation=None)
    parser.optionxform = str
    parser.read_string(core.default_conf())
    return set(parser["Prefixes"])


def _attempt(function, *args):
    try:
        return function(*args), None
    except Exception as exc:  # both sides are probed on purpose
        return None, exc


def _python_convert(value, source, target):
    return Q_(value, source).to(target).magnitude


def _rust_convert(value, source, target):
    return core.evaluate_phs_native(f"{value!r} {source} => {target}")[
        0
    ].magnitude


def _rust_knows(symbol) -> bool:
    return _attempt(_rust_convert, 1.0, symbol, symbol)[1] is None


def _long_prefixed(symbol, long_prefixes, names) -> bool:
    return any(
        symbol.startswith(prefix) and symbol[len(prefix) :] in names
        for prefix in long_prefixes
    )


def _classify(symbol, dimension, py, py_exc, rs, rs_exc, long_prefixes, names):
    """Returns None when both sides agree, otherwise the kind of difference."""
    if py_exc is None and rs_exc is None:
        if py == rs:
            return None
        if abs(py - rs) <= 4 * math.ulp(abs(py)):
            return "last-bit-difference"
        return "value-differs"
    if py_exc is None:
        if "Unknown unit" in str(rs_exc) and _long_prefixed(
            symbol, long_prefixes, names
        ):
            return "rust-rejects-long-prefix-name"
        if "Unknown unit" in str(rs_exc):
            return "rust-does-not-know-unit"
        if dimension == "O" and "Unit mismatch" in str(rs_exc):
            return "rust-rejects-delta-temperature"
        if dimension == "A":
            return "angle-and-solid-angle-merged-by-python"
        return "rust-error-other"
    if rs_exc is None:
        return "python-error-other"
    return None  # both raise: they agree the conversion is invalid


def _describe(value, exc):
    return value if exc is None else f"{type(exc).__name__}: {str(exc)[:60]}"


@pytest.fixture(scope="module")
def comparison() -> Comparison:
    names = set(dir(units))  # the names of the catalog written as units.<name>
    long_prefixes = _long_prefix_names()
    groups = collections.defaultdict(list)
    unresolvable = []
    for name in sorted(names):
        try:
            groups[str(Q_(1.0, name).dimension)].append(name)
        except UnknownUnitError:
            unresolvable.append(name)
    reference = {
        dim: [
            s
            for s in sorted(members, key=lambda s: (len(s), s))
            if _rust_knows(s)
        ][:REFERENCES_PER_GROUP]
        for dim, members in groups.items()
    }
    differences = []
    for dim, members in groups.items():
        for target in reference[dim]:
            for symbol in members:
                if symbol == target:
                    continue
                for value in VALUES:
                    py, py_exc = _attempt(
                        _python_convert, value, symbol, target
                    )
                    rs, rs_exc = _attempt(_rust_convert, value, symbol, target)
                    kind = _classify(
                        symbol,
                        dim,
                        py,
                        py_exc,
                        rs,
                        rs_exc,
                        long_prefixes,
                        names,
                    )
                    if kind:
                        differences.append(
                            Difference(
                                symbol,
                                dim,
                                target,
                                value,
                                _describe(py, py_exc),
                                _describe(rs, rs_exc),
                                kind,
                            )
                        )
    return Comparison(dict(groups), reference, unresolvable, differences)


def _of_kind(comparison, kind):
    return [d for d in comparison.differences if d.kind == kind]


def _show(differences, limit=12):
    head = "\n".join(f"  {d}" for d in differences[:limit])
    more = (
        f"\n  ... and {len(differences) - limit} more"
        if len(differences) > limit
        else ""
    )
    return f"{len(differences)} difference(s):\n{head}{more}"


def test_the_comparison_covers_the_catalog(comparison):
    """Guards against a comparison that silently compares nothing."""
    assert len(comparison.groups) >= 30
    assert sum(len(m) for m in comparison.groups.values()) >= 2000


def test_every_difference_is_classified(comparison):
    unclassified = [
        d for d in comparison.differences if d.kind not in CLASSIFIED_KINDS
    ]
    assert not unclassified, _show(unclassified)


def test_python_and_rust_agree_to_the_last_bit(comparison):
    found = _of_kind(comparison, "last-bit-difference")
    assert not found, _show(found)


def test_rust_accepts_the_long_prefix_names_python_accepts(comparison):
    rejected = _of_kind(comparison, "rust-rejects-long-prefix-name")
    assert not rejected, _show(rejected)


def test_rust_knows_every_unit_python_knows(comparison):
    unknown = _of_kind(comparison, "rust-does-not-know-unit")
    assert not unknown, _show(unknown)


def test_rust_converts_delta_temperatures_like_python(comparison):
    rejected = _of_kind(comparison, "rust-rejects-delta-temperature")
    assert not rejected, _show(rejected)


def test_python_does_not_mix_angle_and_solid_angle(comparison):
    merged = _of_kind(comparison, "angle-and-solid-angle-merged-by-python")
    assert not merged, _show(merged)


def test_every_catalog_name_can_be_resolved_by_python(comparison):
    assert not comparison.unresolvable, comparison.unresolvable[:12]


def test_incompatible_dimensions_are_rejected_by_both(comparison):
    references = sorted(
        refs[0] for refs in comparison.reference.values() if refs
    )
    accepted_by_rust = []
    accepted_by_python = []
    for source, target in itertools.pairwise(references):
        _, py_exc = _attempt(_python_convert, 1.0, source, target)
        _, rs_exc = _attempt(_rust_convert, 1.0, source, target)
        if py_exc is None:
            accepted_by_python.append((source, target))
        if rs_exc is None:
            accepted_by_rust.append((source, target))
    assert not accepted_by_python, accepted_by_python
    assert not accepted_by_rust, accepted_by_rust


def test_compound_conversions_agree_exactly():
    differing = {}
    for value, source, target in COMPOUND_CASES:
        py, py_exc = _attempt(_python_convert, value, source, target)
        rs, rs_exc = _attempt(_rust_convert, value, source, target)
        assert py_exc is None, (source, target, py_exc)
        assert rs_exc is None, (source, target, rs_exc)
        if py != rs:
            differing[(source, target)] = (py, rs)
    assert not differing, f"compound conversions that differ: {differing}"


def _plain(number):
    value = float(number)
    return int(value) if value.is_integer() else value


def _semantic_facts(expression):
    system = get_current_system()
    try:
        unit = Q_(1.0, expression).unit
        return {
            "dimension": {
                key: _plain(exp)
                for key, exp in sorted(
                    unit.dimension(system).exponents.items()
                )
            },
            "kind": unit.kind(system),
            "is_linear": unit.is_linear(system),
            "alias": unit.to_string(system, use_alias=True),
            "simplified": str(unit.simplify(system)),
        }
    except Exception as exc:  # an error is a fact worth freezing
        return {"error": type(exc).__name__}


def test_unit_semantics_match_the_frozen_python_behaviour():
    """Freezes what Python says about units, as the target for Stage 1.

    Rust has no counterpart for ``kind``, ``is_linear``, ``simplify`` or the
    alias formatting yet. Until Stage 1 adds them, this golden file is the
    specification the Rust implementation must reproduce.
    """
    facts = {expr: _semantic_facts(expr) for expr in SEMANTIC_EXPRESSIONS}
    if UPDATE_GOLDEN:
        GOLDEN_FILE.parent.mkdir(parents=True, exist_ok=True)
        GOLDEN_FILE.write_text(
            json.dumps(facts, indent=2, ensure_ascii=False, sort_keys=True)
            + "\n",
            encoding="utf-8",
        )
    frozen = json.loads(GOLDEN_FILE.read_text(encoding="utf-8"))
    assert facts == frozen


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


@pytest.mark.parametrize(
    "expression", sorted(json.loads(GOLDEN_FILE.read_text(encoding="utf-8")))
)
def test_the_core_reproduces_the_frozen_unit_semantics(expression):
    frozen = json.loads(GOLDEN_FILE.read_text(encoding="utf-8"))[expression]
    facts = core.unit_facts(expression)
    for field in ("dimension", "kind", "is_linear", "alias", "simplified"):
        assert facts[field] == frozen[field], field


def test_the_core_agrees_with_python_on_every_unit_of_the_catalog(comparison):
    names = sorted(
        name for members in comparison.groups.values() for name in members
    )
    for name in names:
        python, rust = _python_facts(name), core.unit_facts(name)
        for field in ("dimension", "kind", "is_linear", "alias", "simplified"):
            assert rust[field] == python[field], (name, field)
