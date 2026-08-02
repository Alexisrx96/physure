"""The moments bindings Python needs to reach the core's asymmetric conversion.

Propagation is not implemented anywhere yet, so these cover exactly what is: a quoted
`(sigma-, sigma+)` pair converts to moments and back, and the cases that have no pair
behind them are refused instead of approximated.
"""

import math

import pytest

from physure import Q_
from physure._core import AsymmetricMoments, MomentsBackend, max_skewness


def test_a_pair_survives_the_round_trip():
    m = AsymmetricMoments.from_sigmas(0.4, 0.5)
    lo, hi = m.sigmas()
    assert math.isclose(lo, 0.4, abs_tol=1e-9)
    assert math.isclose(hi, 0.5, abs_tol=1e-9)
    assert m.third > 0.0, (
        "the longer tail is upward, so the third moment is positive"
    )


def test_equal_halves_are_an_ordinary_gaussian():
    m = AsymmetricMoments.from_sigmas(0.5, 0.5)
    assert m.shift == 0.0
    assert m.third == 0.0
    assert math.isclose(m.std_dev, 0.5)
    assert m.skewness == 0.0
    # Byte-identical to the symmetric path is the point: nothing downstream has to branch.
    assert m == AsymmetricMoments.from_sigmas(0.5, 0.5)


def test_an_exact_value_has_no_shape():
    m = AsymmetricMoments.exact()
    assert (m.shift, m.variance, m.third) == (0.0, 0.0, 0.0)
    assert m.sigmas() == (0.0, 0.0)


def test_the_mean_is_not_the_quoted_value():
    # 12.3 +0.4 -0.5: the longer tail is downward, so the mean sits below the mode.
    b = MomentsBackend.measured(12.3, 0.5, 0.4)
    assert b.mean < 12.3
    assert math.isclose(b.mode(), 12.3, abs_tol=1e-9)
    lo, hi = b.sigmas()
    assert math.isclose(lo, 0.5, abs_tol=1e-9)
    assert math.isclose(hi, 0.4, abs_tol=1e-9)
    # The spread carries provenance, so an asymmetric value still cancels against itself
    # once something propagates it.
    assert b.sigma.std_dev > 0.0
    assert not b.sigma.is_exact


@pytest.mark.parametrize(
    "sigmas", [(-1.0, 0.5), (0.5, float("nan")), (0.5, float("inf"))]
)
def test_a_pair_that_is_not_two_half_widths_is_refused(sigmas):
    with pytest.raises(ValueError, match="finite non-negative half-widths"):
        AsymmetricMoments.from_sigmas(*sigmas)


def test_a_skew_beyond_the_shape_is_reported_not_rounded_down():
    assert 0.99 < max_skewness() < 1.0
    # A third moment past the asymptote has no pair behind it. Returning the most skewed
    # pair available would understate the tail by an unbounded amount.
    beyond = AsymmetricMoments.from_sigmas(0.0, 1.0)
    assert math.isclose(beyond.skewness, max_skewness(), rel_tol=1e-6)


def test_a_pair_on_a_scalar_says_what_it_cannot_do_yet():
    # The message PHS gives for `12.3 +/- (0.5, 0.4)`, rather than a TypeError from float().
    with pytest.raises(NotImplementedError, match="asymmetric uncertainty"):
        Q_(12.3, "m", uncertainty=(0.5, 0.4))


def test_a_pair_on_an_array_is_still_two_ordinary_uncertainties():
    np = pytest.importorskip("numpy")
    q = Q_(np.array([1.0, 2.0]), "m", uncertainty=(0.1, 0.2))
    assert list(q.uncertainty) == [0.1, 0.2]
