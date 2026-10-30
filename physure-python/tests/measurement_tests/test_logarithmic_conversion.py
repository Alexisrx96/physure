"""Levels convert through the ratio they stand for, and agree with the Rust core."""

import pytest

from physure import Q_
from physure._core import evaluate_phs_native


def test_the_same_level_under_another_name_is_not_converted():
    assert Q_(1.0, "decibel").to("dB").magnitude == 1.0
    assert Q_(3.0, "dB", uncertainty=0.5).to("decibel").uncertainty == 0.5


@pytest.mark.parametrize(
    ("value", "source", "target"),
    [
        (20.0, "dB", "unity"),
        (20.0, "dB", "percent"),
        (50.0, "percent", "dB"),
        (7.0, "pH", "unity"),
        (3.0, "dB", "pH"),
        (10.0, "ppm", "dB"),
    ],
)
def test_levels_convert_exactly_like_the_core(value, source, target):
    rust = evaluate_phs_native(f"{value!r} {source} => {target}")[-1]
    assert Q_(value, source).to(target).magnitude == rust.magnitude


def test_levels_add_as_the_ratios_they_stand_for():
    total = Q_(10.0, "dB") + Q_(20.0, "dB")
    rust = evaluate_phs_native("10 dB + 20 dB")[-1]
    assert total.magnitude == rust.magnitude
