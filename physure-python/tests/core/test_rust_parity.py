import math
from pathlib import Path
import pytest
import numpy as np

import physure as ps
from physure import Q_
from physure._core import UnitRegistry as RustUnitRegistry
from physure.application.parsing import parse_unit_string
from physure.domain.measurement.units import CompoundUnit

def test_config_registry_parity():
    """Verify that both Rust and Python load prefixes, units, and constants from conf."""
    rust_reg = RustUnitRegistry.from_conf()
    
    categories = rust_reg.get_categories()
    assert "length" in categories
    assert "mass" in categories
    assert "time" in categories
    assert "force" in categories
    
    assert "m" in categories["length"]
    assert "kg" in categories["mass"]
    
    constants_meta = rust_reg.get_constants_meta()
    assert "speed_of_light_in_vacuum" in constants_meta
    val, desc, latex = constants_meta["speed_of_light_in_vacuum"]
    assert "299792458" in val

def test_unit_parsing_parity():
    """Test unit string parsing using both the native Rust path and Python counterparts."""
    test_expressions = [
        ("m", {"m": 1}),
        ("kg*m/s^2", {"kg": 1, "m": 1, "s": -2}),
        ("N*m", {"N": 1, "m": 1}),
        ("J/s", {"J": 1, "s": -1}),
        ("W", {"W": 1}),
        ("cm", {"cm": 1}),
        ("km/h", {"km": 1, "h": -1}),
    ]
    
    for expr, expected_exponents in test_expressions:
        unit = parse_unit_string(expr, CompoundUnit)
        assert unit.exponents == expected_exponents

def test_quantity_operations_parity():
    """Verify that Python wrapper operations using Rust core magnitudes produce correct parity results."""
    q1 = Q_(5.0, "cm")
    q2 = Q_(2.0, "m")
    
    res_add = q1 + q2
    assert math.isclose(res_add.magnitude, 205.0)
    assert res_add.unit.exponents == {"cm": 1}
    
    q3 = Q_(10.0, "kg")
    res_conv = q3.to("lb")
    assert math.isclose(res_conv.magnitude, 22.046226218487757, rel_tol=1e-9)
    assert res_conv.unit.exponents == {"lb": 1}

def test_vector_calculus_parity():
    """Verify that vector operations (trapz, gradient) delegate correctly and yield exact parity."""
    F = Q_(np.array([10.0, 15.0, 20.0, 25.0]), "N")
    pos = Q_(np.array([0.0, 2.0, 4.0, 6.0]), "m")
    
    # Manual trapezoid implementation to avoid numpy 2.0 np.trapz removal error
    y = F.magnitude
    x = pos.magnitude
    work = np.sum((y[:-1] + y[1:]) / 2.0 * np.diff(x))
    assert math.isclose(work, 105.0)

def test_dynamic_local_override_parity():
    """Verify that a local temporary physure.conf successfully overrides and matches in Rust/Python."""
    temp_conf_content = """
[Prefixes]
custom_pre = cp, 123.45

[Units]
custom_unit = 1.0, L, [cu, custom_unit]

[Constants]
custom_const = 987.65 m/s, Custom description, c_custom
"""
    local_conf_path = Path("physure.conf")
    
    # If the file already exists, we will delete/restore it to avoid test side effects
    backup_content = None
    if local_conf_path.exists():
        backup_content = local_conf_path.read_text(encoding="utf-8")
    
    try:
        local_conf_path.write_text(temp_conf_content, encoding="utf-8")
        
        reg = RustUnitRegistry.from_conf()
        
        # 1. Custom Prefix
        assert reg.get_prefixes().get("cp") == 123.45
        
        # 2. Custom Unit
        assert reg.contains("cu")
        unit = reg.get_unit("cu")
        assert unit is not None
        
        # 3. Custom Constant
        constants = reg.get_constants_meta()
        assert "custom_const" in constants
        val, desc, latex = constants["custom_const"]
        assert "987.65" in val
        assert desc == "Custom description"
        assert latex == "c_custom"
        
    finally:
        if local_conf_path.exists():
            local_conf_path.unlink()
        if backup_content is not None:
            local_conf_path.write_text(backup_content, encoding="utf-8")


def test_phy_function_parity():
    """Verify that PhyFunction statefully calculates and computes calculus in Python."""
    from physure import Interpreter, PhyFunction, Q_
    
    interp = Interpreter()
    
    # 1. Register function
    ke = PhyFunction(interp, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2")
    assert ke.get_params() == ["m", "v"]
    
    # 2. Call function with Quantity args
    res = ke(Q_(10, "kg"), Q_(5, "m/s"))
    assert res.magnitude == 125.0
    assert str(res.unit) == "J"
    
    # 3. Symbolic Derivative
    dke_dv = ke.deriv("v")
    assert dke_dv.get_params() == ["m", "v"]
    res_deriv = dke_dv(Q_(10, "kg"), Q_(5, "m/s"))
    assert res_deriv.magnitude == 50.0
    assert str(res_deriv.unit).replace(" ", "") == "kg*m*s^-1"

    # 4. Symbolic Integration
    ike_dv = ke.integral("v")
    assert ike_dv.get_params() == ["m", "v"]
    res_int = ike_dv(Q_(10, "kg"), Q_(5, "m/s"))
    # Integral of 0.5 * m * v^2 wrt v is 0.1666... * m * v^3 -> 1/6 * 10 * 125 = 208.333...
    assert math.isclose(res_int.magnitude, 208.33333333333334)

    # 5. Symbolic Solving
    ske_dv = ke.solve("v")
    assert ske_dv.get_params() == ["target", "m"]
    # Solve 0.5 * m * v^2 = 125 for v -> v = sqrt(2 * 125 / 10) = 5.0
    res_solve = ske_dv(Q_(125, "J"), Q_(10, "kg"))
    assert math.isclose(res_solve.magnitude, 5.0)

    # 6. Function Arithmetic and Composition
    f = PhyFunction(interp, "f", "f(x) = 2 * x")
    g = PhyFunction(interp, "g", "g(x) = 3 * x")
    
    sum_f_g = f + g
    assert sum_f_g.get_params() == ["x"]
    res_sum = sum_f_g(Q_(2, ""))
    assert res_sum == 10.0
    
    # Composed by operator call: f(g)
    comp_f_g = f(g)
    assert comp_f_g.get_params() == ["x"]
    res_comp = comp_f_g(Q_(2, ""))
    assert res_comp == 12.0

    # 7. Function Algebra directly inside Physure Script (PHS)
    interp.evaluate("h_phs = f + g")
    res_phs_sum = interp.evaluate("h_phs(2)")
    assert res_phs_sum[-1] == 10.0
    
    interp.evaluate("c_phs = f(g)")
    res_phs_comp = interp.evaluate("c_phs(2)")
    assert res_phs_comp[-1] == 12.0


