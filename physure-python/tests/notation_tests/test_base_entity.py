"""Test suite for the base_entity module using pytest."""

from physure.domain.notation.base_entity import BaseExponentEntity


def test_initialization():
    """Test initialization and normalization."""
    # Basic initialization
    entity = BaseExponentEntity({"x": 1, "y": 2})
    assert entity.exponents == {"x": 1, "y": 2}

    # Zero exponents should be removed
    entity = BaseExponentEntity({"x": 1, "y": 0, "z": 2})
    assert entity.exponents == {"x": 1, "z": 2}
    assert "y" not in entity.exponents

    # Empty initialization
    entity = BaseExponentEntity({})
    assert entity.exponents == {}


def test_arithmetic_operations():
    """Test arithmetic operations between entities."""
    # Multiplication
    entity1 = BaseExponentEntity({"x": 1, "y": 2})
    entity2 = BaseExponentEntity({"y": 1, "z": 3})

    result = entity1 * entity2
    assert result.exponents == {"x": 1, "y": 3, "z": 3}

    # Division
    result = entity1 / entity2
    assert result.exponents == {"x": 1, "y": 1, "z": -3}

    # Power
    result = entity1**2
    assert result.exponents == {"x": 2, "y": 4}

    result = entity1**0.5
    assert result.exponents == {"x": 0.5, "y": 1}

    # Complex operations
    entity3 = BaseExponentEntity({"a": 2, "b": -1})
    result = (entity1 * entity2) / entity3
    assert result.exponents == {"x": 1, "y": 3, "z": 3, "a": -2, "b": 1}


def test_equality_and_hash():
    """Test equality comparison and hashing."""
    entity1 = BaseExponentEntity({"x": 1, "y": 2})
    entity2 = BaseExponentEntity({"x": 1, "y": 2})
    entity3 = BaseExponentEntity({"x": 2, "y": 1})

    # Equality
    assert entity1 == entity2
    assert entity1 != entity3

    # Hash
    assert hash(entity1) == hash(entity2)
    assert hash(entity1) != hash(entity3)

    # Equality with non-BaseExponentEntity objects
    assert entity1 != "not an entity"
    assert entity1 != 123


def test_string_representation():
    """Test string representation methods."""
    # Simple entity
    entity = BaseExponentEntity({"x": 1})
    assert str(entity) == "x"

    # Entity with multiple exponents
    entity = BaseExponentEntity({"x": 1, "y": 2})
    assert str(entity) in ("x·y²", "y²·x")

    # Entity with negative exponents
    entity = BaseExponentEntity({"x": 1, "y": -1})
    assert str(entity) == "x/y"

    # Mixed positive and negative exponents
    entity = BaseExponentEntity({"x": 2, "y": 1, "z": -3})
    assert "x²·y/z³" in str(entity)

    # Only negative exponents
    entity = BaseExponentEntity({"x": -1, "y": -2})
    assert str(entity) in ("1/(x·y²)", "1/(y²·x)")

    # Empty entity
    entity = BaseExponentEntity({})
    assert str(entity) == "1"

    # Repr should show the internal dictionary
    entity = BaseExponentEntity({"x": 1, "y": 2})
    assert repr(entity) in ("{'x': 1, 'y': 2}", "{'y': 2, 'x': 1}")


def test_rtruediv():
    """Test the __rtruediv__ method."""
    entity = BaseExponentEntity({"x": 1, "y": 2})

    # Divide a scalar by an entity
    result = 1 / entity
    assert result.exponents == {"x": -1, "y": -2}

    # Should work with any numeric type
    result = 2.5 / entity
    assert result.exponents == {"x": -1, "y": -2}

    result = complex(1, 2) / entity
    assert result.exponents == {"x": -1, "y": -2}
