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


# ---------------------------------------------------------------------------------------------------------------
# The default count is the core's `[Settings] monte_carlo_samples` (Phase 4b, Task 1a).
#
# What these tests prove: a Monte Carlo quantity that names no count (``samples=None``, and ``Q_`` in Monte Carlo
# mode, which passes ``None``) draws the count the core's unfrozen catalog sets: the embedded catalog's 1000, or a
# working-directory ``physure.conf`` read through ``UnitRegistry.from_conf()``. Python cannot honour a *frozen* file
# before Phase 5 (it refuses to start under one), so a frozen file's count is tested in Rust only. The count is not
# exposed to Python; it is observed through the spread of the estimates: with 1000 samples the estimated standard
# deviation of N(5, 1) is within 10 % of 1 with probability above 0.998, with 2 samples about 0.08.


def _within_ten_percent(make, n=40):
    return sum(abs(make().std_dev - 1.0) < 0.1 for _ in range(n))


def _conf_samples(directory, count):
    from physure._core import UnitRegistry

    (directory / "physure.conf").write_text(
        f"[Settings]\nmonte_carlo_samples = {count}\n"
    )
    UnitRegistry.from_conf()


def test_the_default_count_is_the_catalogs_and_a_working_directory_conf_changes_it(
    tmp_path, monkeypatch
):
    def make():
        return Quantity(5.0, METRE, 1.0, mode="monte_carlo", samples=None)

    assert _within_ten_percent(make) >= 36, "the embedded catalog's 1000"
    monkeypatch.chdir(tmp_path)
    try:
        _conf_samples(tmp_path, 2)
        assert _within_ten_percent(make) <= 20, (
            "two samples: the conf's count reached the core"
        )
        explicit = Quantity(5.0, METRE, 1.0, mode="monte_carlo", samples=1000)
        assert 0.9 < explicit.std_dev < 1.1, "an explicit count wins"
    finally:
        _conf_samples(tmp_path, 1000)
    assert _within_ten_percent(make) >= 36


def test_q_in_monte_carlo_mode_lets_the_core_choose_the_count(
    tmp_path, monkeypatch
):
    import physure as ps
    from physure import Q_

    def make():
        with ps.uncertainty_mode("monte_carlo"):
            return Q_(5.0, "m", uncertainty=1.0)

    class _Std:
        def __init__(self, q):
            self.std_dev = float(q.uncertainty)

    assert _within_ten_percent(lambda: _Std(make())) >= 36
    monkeypatch.chdir(tmp_path)
    try:
        _conf_samples(tmp_path, 2)
        assert _within_ten_percent(lambda: _Std(make())) <= 20, (
            "Q_ passed no count of its own"
        )
    finally:
        _conf_samples(tmp_path, 1000)
