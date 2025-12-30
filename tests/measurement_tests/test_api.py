"""Test suite for the Quantity Factory API using pytest."""

from measurekit import default_system, get_unit
from measurekit.application.factories import (
    QuantityFactory,
    SpecializedQuantityFactory,
)
from measurekit.domain.measurement.quantity import Quantity


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
