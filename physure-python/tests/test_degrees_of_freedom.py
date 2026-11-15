"""Degrees of freedom, coverage factor and expanded uncertainty of scalar quantities (JCGM 100:2008 4.2, 4.3, G.4),
ending with the Guide's own example, H.1."""

import math
import pathlib
import pickle
import subprocess
import sys

import numpy as np
import pytest

import physure as ps
from physure import Q_
from physure._core import (
    Lineage,
    coverage_factor,
    transpile_phs_native,
    type_a_evaluation,
)
from physure.domain.measurement.degrees_of_freedom import ExpandedUncertainty
from physure.ext import helpers

REPO = pathlib.Path(__file__).resolve().parents[2]


def close(a, b, rel=1e-12):
    return math.isclose(a, b, rel_tol=rel)


def test_a_declared_dof_is_read_back_and_changes_nothing_else():
    q = Q_(10.003, "V", uncertainty=0.002, dof=9)
    assert (q.magnitude, q.uncertainty, q.dof) == (10.003, 0.002, 9.0)
    assert Q_(10.003, "V", uncertainty=0.002).dof == math.inf
    assert Q_(3.0, "V").dof == math.inf, "an exact quantity has none to count"


def test_with_dof_declares_on_the_measurement_and_is_idempotent():
    q = Q_(1.0, "m", uncertainty=0.1)
    assert q.with_dof(5) is q
    assert q.with_dof(5).dof == 5.0
    with pytest.raises(ValueError, match="already has 5"):
        q.with_dof(6)
    assert Q_(1.0, "m", uncertainty=0.1).with_dof(math.inf).dof == math.inf


def test_a_declared_infinity_is_a_declaration_and_cannot_be_changed_after():
    """Phase 4b: the core used to drop a declared infinity, so `with_dof(inf)` then `with_dof(5)` passed."""
    q = Q_(1.0, "m", uncertainty=0.1).with_dof(math.inf)
    assert q.with_dof(math.inf).dof == math.inf
    with pytest.raises(ValueError, match="already has inf"):
        q.with_dof(5)
    assert q.dof == math.inf


def test_with_dof_refuses_what_has_no_single_measurement_or_no_positive_number():
    two = Q_(1.0, "m", uncertainty=0.1) + Q_(2.0, "m", uncertainty=0.1)
    with pytest.raises(ValueError, match="combines 2 sources"):
        two.with_dof(4)
    with pytest.raises(ValueError, match="exact"):
        Q_(3.0, "m").with_dof(4)
    for nu in (0, -1, math.nan):
        with pytest.raises(ValueError, match="positive"):
            Q_(1.0, "m", uncertainty=0.1).with_dof(nu)


def test_the_dof_of_a_result_comes_from_its_sources():
    a = Q_(1.0, "m", uncertainty=3.0, dof=4)
    b = Q_(2.0, "m", uncertainty=4.0, dof=9)
    expected = 625.0 / (81.0 / 4.0 + 256.0 / 9.0)
    assert close((a + b).dof, expected)
    assert close((a - b).dof, expected)
    assert (a + a).dof == pytest.approx(4.0), "a copy is the same measurement"
    assert (a - a).dof == math.inf, "and cancels"
    assert close(a.to("mm").dof, 4.0)
    assert close((a * 2.0).dof, 4.0)
    # an undeclared source adds to the uncertainty and not to the denominator
    c = Q_(2.0, "m", uncertainty=4.0)
    assert close((a + c).dof, 625.0 / (81.0 / 4.0))


def test_the_coverage_factor_is_student_t_at_the_truncated_dof():
    k16 = coverage_factor(0.95, 16.0)
    assert close(
        Q_(1.0, "m", uncertainty=0.1, dof=16).coverage_factor(0.95), k16
    )
    assert close(
        Q_(1.0, "m", uncertainty=0.1, dof=16.9).coverage_factor(0.95), k16
    ), "16.9 is read as 16"
    assert close(
        Q_(1.0, "m", uncertainty=0.1, dof=0.4).coverage_factor(0.95),
        coverage_factor(0.95, 1.0),
    )
    assert close(
        Q_(1.0, "m", uncertainty=0.1).coverage_factor(), 1.959963984540054
    ), "the default is 95 %"
    for p in (0.0, 1.0, -0.2, 1.5):
        with pytest.raises(ValueError, match="coverage probability"):
            Q_(1.0, "m", uncertainty=0.1).coverage_factor(p)


def test_the_expanded_uncertainty_reports_every_part_of_it():
    r = Q_(10.0, "V", uncertainty=0.5, dof=16.75).expanded(0.95)
    k = coverage_factor(0.95, 16.0)
    assert isinstance(r, ExpandedUncertainty)
    assert (r.value, r.standard_uncertainty, r.coverage_probability) == (
        10.0,
        0.5,
        0.95,
    )
    assert close(r.coverage_factor, k)
    assert close(r.expanded_uncertainty, k * 0.5)
    assert close(r.dof, 16.75), "nu_eff is reported as it is, not truncated"
    assert str(r.unit) == "V" or "kg" in str(r.unit)
    assert str(r).endswith("(k=2.12, p=95 %)")
    assert "±" in str(r)
    with pytest.raises(AttributeError):
        r.value = 1.0  # a result is frozen


def test_a_type_a_evaluation_is_the_mean_the_standard_error_and_n_minus_one_dof():
    v = Q_.from_samples([10.0, 10.2, 9.9, 10.1, 10.3], "V")
    assert close(v.magnitude, 10.1)
    assert close(v.uncertainty, math.sqrt(0.025 / 5))
    assert v.dof == 4.0
    assert type_a_evaluation([10.0, 10.2, 9.9, 10.1, 10.3])[2] == 4.0
    # exact quantities in one dimension, in the unit of the first
    mixed = Q_.from_samples([Q_(1000.0, "mm"), Q_(1.2, "m"), Q_(1.1, "m")])
    assert close(mixed.magnitude, 1100.0)
    assert mixed.dof == 2.0
    # all equal: no spread, so exact
    same = Q_.from_samples([2.0, 2.0, 2.0], "V")
    assert (same.magnitude, same.uncertainty, same.dof) == (2.0, 0.0, math.inf)


def test_a_type_a_evaluation_refuses_what_it_cannot_estimate():
    with pytest.raises(ValueError, match="at least two"):
        Q_.from_samples([1.0], "V")
    with pytest.raises(ValueError, match="finite"):
        Q_.from_samples([1.0, math.nan], "V")
    with pytest.raises(ValueError, match="exact"):
        Q_.from_samples([Q_(1.0, "V", uncertainty=0.1), Q_(2.0, "V")])


def test_arrays_and_the_other_models_raise_instead_of_answering():
    array = Q_(np.array([1.0, 2.0]), "V", uncertainty=0.1)
    with pytest.raises(NotImplementedError, match="CovarianceModel"):
        _ = array.dof
    with pytest.raises(NotImplementedError):
        array.coverage_factor()
    with pytest.raises(NotImplementedError):
        array.with_dof(4)
    with ps.python_lineage():
        scalar = Q_(1.0, "V", uncertainty=0.1)
        with pytest.raises(NotImplementedError, match="python_lineage"):
            _ = scalar.dof


def test_the_dof_of_a_source_survives_a_pickle_in_another_process():
    lineage = Lineage(0.5)
    lineage.declare_dof(7.0)
    assert pickle.loads(pickle.dumps(lineage)).effective_dof() == 7.0
    code = "import pickle, sys; print(pickle.loads(bytes.fromhex(sys.argv[1])).effective_dof())"
    out = subprocess.run(
        [sys.executable, "-I", "-c", code, pickle.dumps(lineage).hex()],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    assert float(out) == 7.0


def test_the_helpers_of_exported_scripts_follow_the_phs_builtins():
    q = Q_(10.0, "V", uncertainty=0.5, dof=16)
    assert helpers.dof(q) == 16.0
    assert helpers.dof(3.0) == math.inf
    assert close(helpers.coverage_factor(q, 0.95), coverage_factor(0.95, 16.0))
    u = helpers.expanded(q, 0.95)
    assert close(u.magnitude, coverage_factor(0.95, 16.0) * 0.5)
    assert u.uncertainty == 0.0, (
        "an expanded uncertainty is a bound, not a measurement"
    )
    assert helpers.expanded(7.0, 0.95) == 0.0
    assert helpers.from_samples([1.0, 2.0, 3.0]).dof == 2.0


def test_the_end_gauge_of_the_guide_in_python():
    """JCGM 100:2008 H.1: the six contributions to the length, in nm, with their degrees of freedom."""
    budget = [
        (50_000_623.0, 25.0, 18),
        (215.0, 5.8, 24),
        (0.0, 3.9, 5),
        (0.0, 6.7, 8),
        (0.0, 16.599, 2),
        (0.0, 2.88679, 50),
    ]
    parts = [Q_(mean, "nm", uncertainty=u, dof=nu) for mean, u, nu in budget]
    length = parts[0]
    for part in parts[1:]:
        length = length + part
    assert length.magnitude == 50_000_838.0
    assert close(length.uncertainty, 31.663_865_170_002_54)
    assert close(length.dof, 16.751_895_334_905_974)
    p95 = length.expanded(0.95)
    assert close(p95.coverage_factor, 2.119_905_299_221_254_6)
    assert close(p95.expanded_uncertainty, 67.124_395_567_715_7)
    assert close(
        length.expanded(0.99).expanded_uncertainty, 92.483_235_483_489_64
    )
    assert f"{p95.coverage_factor:.2f}" == "2.12"


def test_the_python_target_of_phs_runs_the_same_example():
    """`docs/tutorials/dof_example.phs` transpiled to Python and executed gives the Guide's numbers."""
    source = (REPO / "docs" / "tutorials" / "dof_example.phs").read_text(
        encoding="utf-8"
    )
    code = transpile_phs_native(source, "python")
    assert "dof=18.0" in code
    assert "from physure.ext.helpers import" in code
    namespace: dict = {}
    exec(compile(code, "dof_example.py", "exec"), namespace)
    assert close(namespace["nu_eff"], 16.751_895_334_905_974)
    assert close(namespace["k95"], 2.119_905_299_221_254_6)
    assert close(namespace["U95"].magnitude, 67.124_395_567_715_7)
    assert close(namespace["U99"].magnitude, 92.483_235_483_489_64)


def test_a_conversion_restates_the_same_measurement_and_does_not_make_a_new_one():
    x = Q_(1.0, "m", uncertainty=0.3, dof=4)
    in_mm = x.to("mm")
    assert close(in_mm.uncertainty, 300.0)
    # the same measurement in another unit cancels against itself; a new source would give 424 mm
    assert (in_mm - x.to("mm")).uncertainty == 0.0
    assert close((in_mm + x).dof, 4.0)
    # and a round trip through another unit is the same measurement again, up to rounding
    assert abs((x.to("km").to("m") - x).uncertainty) < 1e-12
