"""`phs_compare`, the comparison PHS transpiled to Python calls: the Rust core's, so it answers what the interpreter answers."""

import pytest

from physure import Q_
from physure.ext import helpers


def test_phs_compare_is_the_interpreter_comparison():
    # the Rust core decides: canonical magnitudes, `==` within the relative tolerance of PHS
    assert helpers.phs_compare(Q_(1, "km"), ">", Q_(999, "m"))
    assert helpers.phs_compare(Q_(1, "km"), "==", Q_(1000, "m"))
    assert not helpers.phs_compare(Q_(1, "km"), "!=", Q_(1000, "m"))
    assert helpers.phs_compare(Q_(0.01, "degC"), "==", Q_(273.16, "K"))
    assert helpers.phs_compare(Q_(50, "%"), "==", 0.5)
    assert helpers.phs_compare(3, "<=", 3.0)
    for op in ("<", ">", "<=", ">=", "==", "!="):
        assert (
            helpers.phs_compare(Q_(2, "m"), op, Q_(3, "m"))
            == {
                "<": True,
                ">": False,
                "<=": True,
                ">=": False,
                "==": False,
                "!=": True,
            }[op]
        )


def test_phs_compare_refuses_what_the_interpreter_refuses():
    with pytest.raises(ValueError, match="a zero needs a unit"):
        helpers.phs_compare(Q_(1, "m"), ">", 0)
    with pytest.raises(ValueError, match="write the level with its unit"):
        helpers.phs_compare(Q_(20, "dB"), "==", 100)
    with pytest.raises(ValueError, match="Unit mismatch"):
        helpers.phs_compare(Q_(1, "m"), "<", Q_(1, "s"))


def test_phs_compare_of_bools_and_text_is_equality_only():
    assert helpers.phs_compare(True, "==", True)
    assert helpers.phs_compare("a", "!=", "b")
    with pytest.raises(TypeError):
        helpers.phs_compare(True, "<", False)
    with pytest.raises(TypeError):
        helpers.phs_compare(True, "==", Q_(1, "m"))
