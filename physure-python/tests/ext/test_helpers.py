"""The helpers that exported Physure scripts import: numbers and quantities alike."""

import math

import matplotlib
import numpy as np
import pytest

matplotlib.use("Agg")

import matplotlib.pyplot as plt

from physure import Q_
from physure.ext import helpers


@pytest.fixture(autouse=True)
def _close_figures():
    yield
    plt.close("all")


def test_numbers_are_close_within_the_tolerance_given():
    assert helpers.approx_eq(1.0, 1.05)
    assert not helpers.approx_eq(1.0, 1.5)
    assert helpers.approx_eq(1.0, 1.001, rel_tol=0.01)
    assert not helpers.approx_eq(1.0, 1.05, rel_tol=0.001, abs_tol=0.0)
    assert helpers.approx_eq(0.0, 1e-6)


def test_quantities_are_compared_in_the_unit_of_the_first():
    assert helpers.approx_eq(Q_(1000.0, "m"), Q_(1.0, "km"))
    assert helpers.approx_eq(Q_(1.0, "km"), Q_(1000.0, "m"))
    assert not helpers.approx_eq(Q_(1.0, "km"), Q_(2000.0, "m"))
    assert helpers.approx_eq(Q_(2.0, "m"), 2.1)


def test_quantities_of_another_dimension_or_words_are_not_close():
    assert not helpers.approx_eq(Q_(1.0, "m"), Q_(1.0, "s"))
    assert not helpers.approx_eq("a", "b")
    assert not helpers.approx_eq(None, 1.0)


def test_linspace_gives_an_array_or_a_quantity_with_the_unit_it_is_given():
    plain = helpers.linspace(0, 1, 5)
    assert isinstance(plain, np.ndarray)
    assert list(plain) == [0.0, 0.25, 0.5, 0.75, 1.0]
    assert len(helpers.linspace(0, 1)) == 50

    metres = helpers.linspace(Q_(0.0, "m"), Q_(2.0, "m"), 3)
    assert list(metres.magnitude) == [0.0, 1.0, 2.0]
    assert str(metres.unit) == "m"
    assert str(helpers.linspace(0, Q_(2.0, "s"), 3).unit) == "s"


def test_plot_draws_one_or_two_series_and_reports_what_it_cannot(
    monkeypatch, capsys
):
    shown = []
    monkeypatch.setattr(plt, "show", lambda: shown.append(plt.gcf()))

    helpers.plot(Q_(np.arange(5.0), "s"), Q_(np.arange(5.0) ** 2, "m"))
    helpers.plot(np.arange(4.0))
    helpers.plot()
    lines = [len(fig.axes[0].lines) for fig in shown]
    assert lines == [1, 1, 0]

    helpers.plot(1.0, "text")
    assert capsys.readouterr().out.startswith("Plot error: ")


def test_sqrt_of_a_quantity_keeps_its_unit_and_of_a_number_is_a_float():
    root = helpers.sqrt(Q_(9.0, "m^2"))
    assert math.isclose(float(root.magnitude), 3.0)
    assert str(root.unit) == "m"
    assert helpers.sqrt(16) == 4.0
    assert helpers.sqrt("25") == 5.0


@pytest.mark.parametrize(
    ("function", "argument"),
    [
        (helpers.sin, 0.5),
        (helpers.cos, 0.5),
        (helpers.tan, 0.5),
        (helpers.asin, 0.5),
        (helpers.acos, 0.5),
        (helpers.atan, 0.5),
        (helpers.sinh, 0.5),
        (helpers.cosh, 0.5),
        (helpers.tanh, 0.5),
        (helpers.exp, 0.5),
        (helpers.log, 0.5),
        (helpers.log10, 0.5),
    ],
)
def test_each_function_is_the_math_function_of_a_number_or_a_magnitude(
    function, argument
):
    expected = getattr(math, function.__name__)(argument)
    assert function(argument) == expected
    assert function(Q_(argument, "")) == expected


def test_atan2_and_the_constants():
    assert helpers.atan2(1.0, 1.0) == math.atan2(1.0, 1.0)
    assert helpers.atan2(Q_(1.0, ""), Q_(-1.0, "")) == math.atan2(1.0, -1.0)
    assert (helpers.pi, helpers.e) == (math.pi, math.e)
