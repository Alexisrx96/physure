"""The Monte Carlo sample count is validated by the core (audit B8).

The standard uncertainty is the sample standard deviation (JCGM 101:2008 7.6, ``1 / (M - 1)``), which needs at
least two samples. ``samples=0`` used to give a mean of ``0.0`` and a NaN uncertainty, ``samples=1`` a zero
uncertainty, and a count that did not read as a non-negative integer silently became the default 1000.
"""

import pytest

from physure._core import Quantity, RationalUnit

METRE = RationalUnit({"m": (1, 1)})


@pytest.mark.parametrize("samples", [0, 1])
def test_fewer_than_two_samples_are_refused(samples):
    with pytest.raises(ValueError, match="at least 2 samples"):
        Quantity(5.0, METRE, 0.1, mode="monte_carlo", samples=samples)


def test_a_negative_sample_count_is_refused_not_replaced_by_the_default():
    with pytest.raises(OverflowError):
        Quantity(5.0, METRE, 0.1, mode="monte_carlo", samples=-1)


def test_two_samples_and_the_default_are_accepted():
    two = Quantity(5.0, METRE, 0.1, mode="monte_carlo", samples=2)
    assert two.std_dev >= 0.0
    default = Quantity(5.0, METRE, 0.1, mode="monte_carlo", samples=None)
    assert 0.08 < default.std_dev < 0.12
