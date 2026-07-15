"""Tests for the pandas `.mk` Series accessor."""

import numpy as np
import pytest

from physure import Q_

pd = pytest.importorskip("pandas")
pytest.importorskip("physure.ext.pandas_support")


@pytest.fixture
def lengths():
    return pd.Series([Q_(1.0, "m"), Q_(2.0, "m"), Q_(3.0, "m")])


def test_accessor_rejects_non_object_series():
    with pytest.raises(TypeError, match="object dtype"):
        pd.Series([1.0, 2.0]).mk.magnitude  # noqa: B018


def test_magnitude_and_uncertainty(lengths):
    assert list(lengths.mk.magnitude) == [1.0, 2.0, 3.0]
    s = pd.Series(
        [Q_(1.0, "m", uncertainty=0.1), Q_(2.0, "m", uncertainty=0.2)]
    )
    assert np.allclose(list(s.mk.uncertainty), [0.1, 0.2])


def test_magnitude_handles_none(lengths):
    s = pd.Series([Q_(1.0, "m"), None])
    mags = s.mk.magnitude
    assert mags.iloc[0] == 1.0
    assert np.isnan(mags.iloc[1])


def test_unit_shared(lengths):
    assert str(lengths.mk.unit) == "m"


def test_unit_mixed_returns_collection():
    s = pd.Series([Q_(1.0, "m"), Q_(1.0, "s")])
    units = s.mk.unit
    assert len(units) == 2


def test_unit_empty_series_is_none():
    s = pd.Series([None], dtype=object)
    assert s.mk.unit is None


def test_array_vectorizes(lengths):
    q = lengths.mk.array
    assert np.allclose(q.magnitude, [1.0, 2.0, 3.0])
    assert str(q.unit) == "m"


def test_array_converts_mixed_units():
    s = pd.Series([Q_(1.0, "m"), Q_(1.0, "km")])
    q = s.mk.array
    assert np.allclose(q.magnitude, [1.0, 1000.0])
    assert str(q.unit) == "m"


def test_array_empty_raises():
    s = pd.Series([None], dtype=object)
    with pytest.raises(ValueError, match="empty"):
        s.mk.array  # noqa: B018


def test_to_converts_series(lengths):
    km = lengths.mk.to("km")
    assert np.allclose(list(km.mk.magnitude), [0.001, 0.002, 0.003])
    assert str(km.mk.unit) == "km"


def test_plus_minus_attaches_uncertainty(lengths):
    s = lengths.mk.plus_minus(0.5)
    uncs = np.asarray(list(s.mk.uncertainty), dtype=float)
    assert np.allclose(uncs, 0.5)


def test_to_json_roundtrippable(lengths):
    import json

    payload = json.loads(lengths.mk.to_json())
    assert payload  # structure is implementation-defined; must be valid JSON
