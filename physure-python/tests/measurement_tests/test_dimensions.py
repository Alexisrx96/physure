"""Test suite for the Dimension class using pytest."""

import pytest

from physure.domain.measurement.dimensions import Dimension, get_dimension


@pytest.fixture
def dim_system(system):
    """Set up test fixtures for dimension tests."""
    system.register_dimension(Dimension({"L": 1}), "Length")
    system.register_dimension(Dimension({"M": 1}), "Mass")
    system.register_dimension(Dimension({"T": 1}), "Time")
    return system


def test_init_and_caching():
    """Test initialization and caching behavior."""
    dim1 = Dimension({"L": 1})
    assert dim1.exponents == {"L": 1}

    dim2 = Dimension({"L": 1})
    assert dim1 is dim2


def test_arithmetic_operations():
    """Test arithmetic operations between dimensions."""
    length = Dimension({"L": 1})
    time = Dimension({"T": 1})

    velocity_dim = length / time
    assert velocity_dim.exponents == {"L": 1, "T": -1}


def test_string_representation(dim_system):
    """Test the string representation of dimensions."""
    force_dim = Dimension({"M": 1, "L": 1, "T": -2})
    assert str(force_dim) == "L·M·T⁻²"

    length_dim = Dimension({"L": 1})
    assert dim_system._DIMENSION_NAME_REGISTRY.get(length_dim) == "Length"


def test_get_dimension_parsing():
    """Test parsing dimension expressions."""
    assert get_dimension("L·M/T²").exponents == {"L": 1, "M": 1, "T": -2}
    assert get_dimension("(L/T)²").exponents == {"L": 2, "T": -2}
