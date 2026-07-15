import pytest

from physure import Q_
from physure.domain.exceptions import IncompatibleUnitsError
from physure.domain.measurement.units import CompoundUnit
from physure.domain.symbolic.native import Quantity as QuantityNode
from physure.ext.symbolic_regression import (
    FittedFormula,
    SymbolicRegressor,
    _combine,
)


def test_symbolic_regressor_recovers_roadmap_example():
    t = Q_([1.0, 2.0, 3.0, 4.0, 5.0], "s", symbol="t")
    s = Q_([4.9, 19.5, 44.0, 78.3, 122.1], "m", symbol="s")

    regressor = SymbolicRegressor(
        inputs={"t": t},
        target=s,
        allowed_operations=["Add", "Mul", "Pow"],
        max_complexity=10,
        seed=0,
    )
    best_fit = regressor.fit()

    assert isinstance(best_fit, FittedFormula)
    assert best_fit.formula_string.startswith("s = ")
    assert best_fit.constants["k"].value == pytest.approx(4.887, abs=0.01)
    assert best_fit.constants["k"].units == CompoundUnit({"m": 1, "s": -2})


def test_fitted_formula_is_callable_and_matches_data():
    t = Q_([1.0, 2.0, 3.0, 4.0, 5.0], "s", symbol="t")
    s = Q_([4.9, 19.5, 44.0, 78.3, 122.1], "m", symbol="s")

    regressor = SymbolicRegressor(
        inputs={"t": t},
        target=s,
        allowed_operations=["Add", "Mul", "Pow"],
        max_complexity=10,
        seed=0,
    )
    best_fit = regressor.fit()

    predicted = [best_fit(t=v) for v in (1.0, 2.0, 3.0, 4.0, 5.0)]
    actual = [4.9, 19.5, 44.0, 78.3, 122.1]
    for p, a in zip(predicted, actual, strict=True):
        assert p == pytest.approx(a, abs=0.5)


def test_combine_add_rejects_mismatched_units():
    length = QuantityNode("a", CompoundUnit({"m": 1}))
    mass = QuantityNode("b", CompoundUnit({"kg": 1}))
    with pytest.raises(IncompatibleUnitsError):
        _combine("Add", length, mass)


def test_symbolic_regressor_raises_when_no_valid_formula():
    t = Q_([0.0, 0.0, 0.0], "s")
    y = Q_([1.0, 2.0, 3.0], "m")

    regressor = SymbolicRegressor(
        inputs={"t": t},
        target=y,
        allowed_operations=["Add", "Mul", "Pow"],
        max_complexity=6,
        population_size=10,
        generations=3,
        seed=0,
    )
    with pytest.raises(RuntimeError, match="no dimensionally valid formula"):
        regressor.fit()


def test_symbolic_regressor_handles_multiple_inputs_without_crossing_units():
    length = Q_([1.0, 2.0, 3.0, 4.0], "m")
    mass = Q_([10.0, 10.0, 10.0, 10.0], "kg")
    target = Q_([2.0, 4.0, 6.0, 8.0], "m")

    regressor = SymbolicRegressor(
        inputs={"length": length, "mass": mass},
        target=target,
        allowed_operations=["Add", "Mul"],
        max_complexity=8,
        population_size=30,
        generations=15,
        seed=1,
    )
    best_fit = regressor.fit()

    # target = 2 * length exactly, so a correct fit must predict it well
    # regardless of which equally-sized tree the search happened to land on.
    for length_value, expected in zip(
        (1.0, 2.0, 3.0, 4.0), (2.0, 4.0, 6.0, 8.0), strict=True
    ):
        predicted = best_fit(length=length_value, mass=10.0)
        assert predicted == pytest.approx(expected, abs=0.5)
