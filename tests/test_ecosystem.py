import numpy as np
import pandas as pd
import pytest
import sympy as sp
from measurekit import Q_, default_system
from measurekit.constants import c, G
from measurekit.pandas_ext import MeasureKitArray


def test_numpy_vectorization():
    v = Q_(np.array([1, 2, 3]), "m/s")
    t = Q_(2, "s")
    d = v * t
    assert np.all(d.magnitude == [2, 4, 6])
    assert str(d.unit) == "m"


def test_numpy_ufuncs():
    angles = Q_(np.array([0, np.pi / 2, np.pi]), "")
    sin_angles = np.sin(angles)
    assert np.allclose(sin_angles.magnitude, [0, 1, 0])

    # Test incompatible units for sin
    lengths = Q_(np.array([1, 2]), "m")
    with pytest.raises(Exception):  # IncompatibleUnitsError
        np.sin(lengths)


def test_numpy_functions():
    v1 = Q_(np.array([1, 2]), "m/s")
    v2 = Q_(np.array([3, 4]), "m/s")
    v_combined = np.concatenate([v1, v2])
    assert np.all(v_combined.magnitude == [1, 2, 3, 4])
    assert v_combined.unit == v1.unit

    v_mean = np.mean(v1)
    assert v_mean.magnitude == 1.5
    assert v_mean.unit == v1.unit


def test_pandas_integration():
    data = [Q_(1, "m"), Q_(2, "m"), Q_(3, "m")]
    ser = pd.Series(MeasureKitArray(data))
    assert ser.dtype.name == "measurekit"
    assert ser[0].magnitude == 1
    assert ser.sum().magnitude == 6


def test_sympy_integration():
    x = sp.Symbol("x")
    q = Q_(x, "m")
    t = Q_(2, "s")
    v = q / t
    assert v.magnitude == x / 2
    assert str(v.unit) == "m/s"


def test_constants():
    assert c.magnitude == 299792458.0
    assert str(c.unit) == "m/s"
    assert G.uncertainty > 0


def test_latex_repr():
    q = Q_(5.0, "m/s^2", uncertainty=0.1)
    latex = q._repr_latex_()
    assert "\\pm" in latex
    assert "m" in latex
    assert "s^{2}" in latex


def test_logarithmic_units():
    # 20 dB should be factor 100 in power (if factor is 10)
    # Actually dB = 10 * log10(P/Pref). So 20 dB => 2 = log10(P/Pref) => P/Pref = 100
    db = Q_(20, "dB")
    val = db.to("1")  # convert to dimensionless base
    assert val.magnitude == 100.0

    # Check pH
    ph = Q_(7, "pH")
    val_ph = ph.to("1")
    assert np.isclose(val_ph.magnitude, 1e-7)
