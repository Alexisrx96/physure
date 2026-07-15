"""Test suite for the CompoundUnit class and get_unit function using pytest."""

import pytest

from physure import get_unit
from physure.domain.exceptions import UnknownUnitError
from physure.domain.measurement.converters import LinearConverter
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.units import CompoundUnit


@pytest.fixture
def unit_system(system):
    """Set up test fixtures for unit tests."""
    # Aliases are registered on the instance
    system.register_alias({"m": 1, "s": -1}, "velocity", "speed")

    # Register units into our isolated test system
    length = Dimension({"L": 1})
    time = Dimension({"T": 1})
    mass = Dimension({"M": 1})
    system.register_unit("m", length, LinearConverter(1.0), "meter")
    system.register_unit("s", time, LinearConverter(1.0), "second")
    system.register_unit("kg", mass, LinearConverter(1.0), "kilogram")
    system.register_unit("cm", length, LinearConverter(0.01), "centimeter")
    system.register_unit("km", length, LinearConverter(1000.0), "kilometer")
    return system


def test_init_and_new():
    """Test initialization and __new__ caching behavior."""
    unit1 = CompoundUnit({"m": 1})
    assert unit1.exponents == {"m": 1}

    unit2 = CompoundUnit({"m": 1})
    assert unit1 is unit2


def test_arithmetic_operations():
    """Test arithmetic operations between units."""
    meter = CompoundUnit({"m": 1})
    second = CompoundUnit({"s": 1})
    kilogram = CompoundUnit({"kg": 1})

    velocity = meter / second
    assert velocity.exponents == {"m": 1, "s": -1}

    area = meter**2
    assert area.exponents == {"m": 2}

    force = kilogram * meter / (second**2)
    assert force.exponents == {"kg": 1, "m": 1, "s": -2}


def test_dimension(unit_system):
    """Test dimension calculation, which now requires a system."""
    length = Dimension({"L": 1})
    time = Dimension({"T": 1})

    meter = CompoundUnit({"m": 1})
    assert meter.dimension(unit_system) == length

    velocity = CompoundUnit({"m": 1, "s": -1})
    assert velocity.dimension(unit_system) == length / time

    # Test with an unknown unit
    with pytest.raises(UnknownUnitError, match="unknown_unit"):
        CompoundUnit({"unknown_unit": 1}).dimension(unit_system)


def test_dimension_unknown_unit_raises_with_suggestion(unit_system):
    """CompoundUnit.dimension() raises UnknownUnitError with a suggestion."""
    # Register 'meter' so get_close_matches can suggest it
    length = Dimension({"L": 1})
    unit_system.register_unit("meter", length, LinearConverter(1.0), "meter")

    bad_unit = CompoundUnit({"meterr": 1})  # typo — no dimension registered
    with pytest.raises(UnknownUnitError, match="meterr"):
        bad_unit.dimension(unit_system)


def test_dimension_unknown_unit_no_suggestion(unit_system):
    """UnknownUnitError message has no 'Did you mean' when nothing is close."""
    bad_unit = CompoundUnit({"xyzqqqq": 1})
    with pytest.raises(UnknownUnitError) as exc_info:
        bad_unit.dimension(unit_system)
    assert "Did you mean" not in str(exc_info.value)


def test_conversion_methods(unit_system):
    """Test methods for unit conversion, which now require a system."""
    meter = CompoundUnit({"m": 1})
    centimeter = CompoundUnit({"cm": 1})
    kilometer = CompoundUnit({"km": 1})

    # Test conversion factor calculation using the system
    assert meter.conversion_factor_to(
        centimeter, unit_system
    ) == pytest.approx(100.0)
    assert centimeter.conversion_factor_to(
        meter, unit_system
    ) == pytest.approx(0.01)
    assert kilometer.conversion_factor_to(meter, unit_system) == pytest.approx(
        1000.0
    )


def test_get_unit_simple():
    """Test get_unit with simple expressions."""
    # Note: get_unit uses the default_system, which should have basic units.
    assert get_unit("m").exponents == {"m": 1}
    assert get_unit("kg").exponents == {"kg": 1}
    assert get_unit("m/s").exponents == {"m": 1, "s": -1}


def test_get_unit_complex():
    """Test get_unit with complex expressions."""
    assert get_unit("(kg*m)/s^2").exponents == {"kg": 1, "m": 1, "s": -2}


def test_get_unit_recipe_applies_to_every_alias(system):
    """Regression: a recipe registered via register_unit() must resolve
    for every alias of the unit, not just the canonical `symbol` argument.

    _UNIT_RECIPES used to be keyed only by `symbol`, so get_unit() on any
    OTHER alias silently fell back to treating it as an atomic unit
    (e.g. {"N": 1}) instead of substituting the derived recipe
    ({"kg": 1, "m": 1, "s": -2}). Real-world instance: "ohm" (an alias of
    the canonical "Ohm") returned {"ohm": 1} while "Ohm" correctly
    returned the SI-decomposed form, making dimensionally-identical
    quantities compare unequal depending on which alias was used.
    """
    length = Dimension({"L": 1})
    time = Dimension({"T": 1})
    mass = Dimension({"M": 1})
    system.register_unit("m", length, LinearConverter(1.0), "meter")
    system.register_unit("s", time, LinearConverter(1.0), "second")
    system.register_unit("kg", mass, LinearConverter(1.0), "kilogram")
    system.register_unit(
        "Newton",
        Dimension({"M": 1, "L": 1, "T": -2}),
        LinearConverter(1.0),
        "newton",
        "N",
        recipe=CompoundUnit({"kg": 1, "m": 1, "s": -2}),
    )

    canonical = system.get_unit("Newton")
    alias = system.get_unit("N")

    assert canonical.exponents == {"kg": 1, "m": 1, "s": -2}
    assert alias.exponents == {"kg": 1, "m": 1, "s": -2}
    assert alias == canonical


def test_get_unit_dimensionless_unit_has_empty_exponents(system):
    """Regression: a unit registered with the dimensionless Dimension({})
    and no explicit recipe (physure.conf's "unity" = "1") must resolve
    to CompoundUnit({}), not the atomic {symbol: 1}.

    Otherwise the symbol survives arithmetic as a bogus unpruned exponent
    key and breaks equality against quantities that never touched it
    (e.g. a dimensionless coefficient multiplied into a force * distance
    chain made the resulting Joules compare unequal to a clean Joule).
    """
    system.register_unit("one", Dimension({}), LinearConverter(1.0), "unity")

    assert system.get_unit("one") == CompoundUnit({})
    assert system.get_unit("one").exponents == {}


def test_unknown_unit_error_is_value_error():
    """Test that UnknownUnitError is also a ValueError."""
    err = UnknownUnitError("xyz")
    assert isinstance(err, ValueError)
    assert "xyz" in str(err)


def test_unknown_unit_error_with_suggestions():
    """Test UnknownUnitError with suggestions."""
    err = UnknownUnitError("metter", suggestions=["meter", "m"])
    assert "meter" in str(err)
    assert "m" in str(err)


def test_unknown_unit_error_no_suggestions():
    """Test UnknownUnitError without suggestions."""
    err = UnknownUnitError("qqqq")
    assert "qqqq" in str(err)
    assert "Did you mean" not in str(err)


def test_unknown_unit_error_importable_from_top_level():
    """Test that UnknownUnitError can be imported from top-level physure."""
    from physure import UnknownUnitError as TopLevelError

    assert issubclass(TopLevelError, ValueError)
    err = TopLevelError("test")
    assert isinstance(err, ValueError)


def test_unknown_unit_error_stores_suggestions():
    err = UnknownUnitError("meterr", suggestions=["meter"])
    assert err.suggestions == ["meter"]
    assert err.unit_name == "meterr"


def test_unknown_unit_error_stores_none_suggestions():
    err = UnknownUnitError("xyz")
    assert err.suggestions is None


def test_standard_units_do_not_load_sympy():
    """Common unit strings must parse without importing sympy."""
    import sys

    sympy_was_loaded = "sympy" in sys.modules

    from physure.application.parsing import parse_unit_string
    from physure.domain.measurement.units import CompoundUnit

    # Clear cache to force a fresh parse
    parse_unit_string.cache_clear()

    exprs = [
        "m/s",
        "kg*m/s**2",
        "m2",
        "kg",
        "m/s^2",
        "(kg*m)/s^2",
        "s-1",
    ]
    for expr in exprs:
        result = parse_unit_string(expr, CompoundUnit)
        assert result is not None, f"Failed to parse: {expr}"

    if not sympy_was_loaded:
        assert "sympy" not in sys.modules, (
            "sympy was loaded while parsing standard unit strings"
        )
