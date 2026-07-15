import pickle

from physure import Q_


def test_unit_identity_preservation():
    """Verify CompoundUnits maintain identity (Flyweight)."""
    # get_default_system() is implicitly active

    # Create via factory
    q_m = Q_(1, "m")
    q_s = Q_(1, "s")

    # Access the unit from the quantity
    unit_compound = (q_m * q_s).unit

    # Verify identity before pickle
    unit_compound_2 = (q_m * q_s).unit
    assert unit_compound is unit_compound_2

    # Pickle and unpickle
    dumped = pickle.dumps(unit_compound)
    loaded = pickle.loads(dumped)

    # Verify identity after unpickle
    assert loaded is unit_compound

    # Test with a fresh unit created after pickle load
    unit_compound_3 = (q_m * q_s).unit
    assert loaded is unit_compound_3


def test_quantity_serialization_roundtrip():
    """Verify Quantity round-trip serialization."""
    q = Q_(10.5, "m")

    dumped = pickle.dumps(q)
    loaded = pickle.loads(dumped)

    import pytest

    assert loaded.magnitude == pytest.approx(q.magnitude)
    # Unit identity should be preserved
    assert loaded.unit is q.unit

    # Check Backend restoration
    assert hasattr(loaded, "_backend")
    assert loaded._backend is not None
    res = loaded * 2
    assert res.magnitude == pytest.approx(21.0)


def test_quantity_with_uncertainty_pickle():
    """Verify pickling quantity with uncertainty."""
    q = Q_(10.0, "m", uncertainty=0.5)

    dumped = pickle.dumps(q)
    loaded = pickle.loads(dumped)

    import pytest

    assert loaded.magnitude == pytest.approx(10.0)
    assert loaded.uncertainty == pytest.approx(0.5)
    assert loaded.unit is q.unit


def test_rich_protocol_existence():
    """Verify __rich__ method exists and runs without error."""
    q = Q_(10, "m", uncertainty=1)

    assert hasattr(q, "__rich__")

    # Call it
    res = q.__rich__()

    try:
        import rich.text

        if isinstance(res, rich.text.Text):
            assert "10" in res.plain
            assert "m" in res.plain
    except ImportError:
        assert isinstance(res, str)
        assert "10" in res


def test_compound_unit_shadowing_resilience():
    """Verify deserialization succeeds even if CompoundUnit is shadowed."""
    import physure.domain.measurement.units as units_module

    # Create a valid unit first (ensures factory is working/cached)
    q = Q_(1, "m")
    payload = pickle.dumps(q.unit)

    # Save original class
    real_compound_unit_class = units_module.CompoundUnit

    try:
        # Shadow the class with a non-callable object
        # This simulates a user accidentally naming a variable 'CompoundUnit'
        # or a bad import shadowing it.
        units_module.CompoundUnit = "Imposter"  # type: ignore

        # Attempt unpickle
        # Without fix: fails because it tries to call "Imposter"()
        # With fix: uses reconstruct_compound_unit factory which falls back
        # to stable ref
        loaded_unit = pickle.loads(payload)

        assert loaded_unit is q.unit

    finally:
        # Restore original class
        units_module.CompoundUnit = real_compound_unit_class
