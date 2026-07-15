"""Test suite for the Quantity Factory API using pytest."""

import time

import pytest

from physure import default_system, get_unit
from physure.application.factories import (
    QuantityFactory,
    SpecializedQuantityFactory,
)
from physure.domain.exceptions import UnknownUnitError
from physure.domain.measurement.quantity import Quantity


def test_specialized_quantity_factory_init():
    """Test the initialization of SpecializedQuantityFactory."""
    factory = SpecializedQuantityFactory(get_unit("m"), default_system)
    assert factory._system is default_system
    assert factory._default_unit == get_unit("m")


def test_specialized_quantity_factory_call():
    """Test the __call__ method of SpecializedQuantityFactory."""
    factory = SpecializedQuantityFactory(get_unit("m"), default_system)
    quantity = factory(5)
    assert quantity.magnitude == 5
    assert quantity.unit == get_unit("m")


def test_specialized_quantity_factory_repr():
    """Test the __repr__ method of SpecializedQuantityFactory."""
    factory = SpecializedQuantityFactory(get_unit("m"), default_system)
    assert repr(factory) == "<Quantity Factory for unit='m'>"


def test_quantity_factory_call():
    """Test the __call__ method of QuantityFactory."""
    factory = QuantityFactory(default_system)
    q = factory(10, "m/s")
    assert isinstance(q, Quantity)
    assert q.magnitude == 10
    assert q.unit == default_system.get_unit("m/s")


def test_quantity_factory_getitem():
    """Test the __getitem__ method of QuantityFactory."""
    factory = QuantityFactory(default_system)
    meter_factory = factory["m"]
    assert isinstance(meter_factory, SpecializedQuantityFactory)
    assert meter_factory._default_unit == default_system.get_unit("m")


@pytest.mark.parametrize(
    ("text", "mag", "unit"),
    [
        ("10 m/s", 10, "m/s"),
        ("1.5e3 kg", 1500.0, "kg"),
        (".5 m", 0.5, "m"),
        ("-3.2 s", -3.2, "s"),
        ("  7  m ", 7, "m"),
        ("10", 10, "1"),
    ],
)
def test_string_parse_splits_magnitude_and_unit(text, mag, unit):
    q = QuantityFactory(default_system)(text)
    assert q.magnitude == mag
    assert str(q.unit) == unit


def test_string_parse_is_not_redos_vulnerable():
    """A long digit run with a failing tail must not backtrack (S5852).

    The old `^\\s*([-+]?\\d*\\.?\\d+...)\\s*(.*)$` regex re-partitioned the
    digit run whenever the anchored tail failed, giving polynomial runtime.
    """
    factory = QuantityFactory(default_system)
    payload = "1" * 200_000 + "\nx\ny"  # tail the unit parser will reject
    start = time.perf_counter()
    with pytest.raises(UnknownUnitError):
        factory(payload)
    assert time.perf_counter() - start < 1.0
