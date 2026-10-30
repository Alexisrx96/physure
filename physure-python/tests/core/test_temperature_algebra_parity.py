"""A1, stage 1c: the temperature algebra of Python and of the Rust core.

Points (degC, degF) and intervals (K, delta_celsius...) add as an affine space in both, and Python follows
the core where it once differed: a plain number does not scale an absolute temperature, and a product takes
the temperature as the absolute one it is.
"""

import pytest

from physure import Q_
from physure._core import evaluate_phs_native


def _rust(source: str):
    return evaluate_phs_native(source)[-1]


def _python_kelvin(quantity) -> float:
    return quantity.to("K").magnitude


def _rust_kelvin(source: str) -> float:
    return _rust(f"({source}) => K").magnitude


AGREE = [
    ("20 degC + 5 K", lambda: Q_(20.0, "degC") + Q_(5.0, "K")),
    ("5 K + 20 degC", lambda: Q_(5.0, "K") + Q_(20.0, "degC")),
    (
        "20 degC + 9 delta_fahrenheit",
        lambda: Q_(20.0, "degC") + Q_(9.0, "delta_fahrenheit"),
    ),
    ("20 degC - 5 K", lambda: Q_(20.0, "degC") - Q_(5.0, "K")),
    ("20 degC - 5 degC", lambda: Q_(20.0, "degC") - Q_(5.0, "degC")),
    ("20 degC - 5 degF", lambda: Q_(20.0, "degC") - Q_(5.0, "degF")),
    ("-1 * 25 degC", lambda: -Q_(25.0, "degC")),
]


@pytest.mark.parametrize(("source", "python"), AGREE)
def test_points_and_intervals_agree(source, python):
    assert _python_kelvin(python()) == pytest.approx(
        _rust_kelvin(source), abs=1e-9
    )


@pytest.mark.parametrize(
    ("source", "python"),
    [
        ("20 degC + 5 degC", lambda: Q_(20.0, "degC") + Q_(5.0, "degC")),
        ("5 K - 20 degC", lambda: Q_(5.0, "K") - Q_(20.0, "degC")),
    ],
)
def test_both_refuse_what_has_no_meaning(source, python):
    with pytest.raises(Exception, match=r"(?i)absolute"):
        python()
    with pytest.raises(Exception, match=r"(?i)absolute"):
        _rust(source)


@pytest.mark.parametrize(
    ("source", "python"),
    [
        ("20 degC == 293.15 K", lambda: Q_(20.0, "degC") == Q_(293.15, "K")),
        ("20 degC > 280 K", lambda: Q_(20.0, "degC") > Q_(280.0, "K")),
        ("20 degC < 70 degF", lambda: Q_(20.0, "degC") < Q_(70.0, "degF")),
    ],
)
def test_comparisons_agree(source, python):
    assert python() is True
    assert _rust(source) is True


def test_python_refuses_to_scale_a_temperature_by_a_number():
    with pytest.raises(Exception, match=r"(?i)absolute temperature"):
        _ = Q_(20.0, "degC") * 2


def test_python_multiplies_by_the_absolute_temperature():
    energy = Q_(8.314, "J/(mol*K)") * Q_(25.0, "degC")
    assert energy.to("J/mol").magnitude == pytest.approx(8.314 * 298.15)
