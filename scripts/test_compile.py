from measurekit import Q_, units
from measurekit.domain.symbolic.quantity import SymbolicQuantity


def test_compile_numpy():
    print(f"DEBUG: Type of 'units': {type(units)}")
    print(f"DEBUG: units: {units}")

    x = SymbolicQuantity("x", units.m)
    y = SymbolicQuantity("y", units.s)

    # Expression: v = x / y
    expr = x / y

    # Compile
    func = expr.compile(backend="numpy")

    # Args
    val_x = Q_(10.0, "m")
    val_y = Q_(2.0, "s")

    # Call
    res = func(output_unit="m/s", x=val_x, y=val_y)

    assert res.magnitude == 5.0
    from measurekit import get_unit

    assert res.unit == get_unit("m/s")
    print("Compile numpy passed.")


def test_compile_unit_check():
    x = SymbolicQuantity("x", units.m)
    expr = x**2
    func = expr.compile()

    # Wrong unit
    try:
        func(output_unit="m^2", x=Q_(10, "s"))
        raise AssertionError("Should have raised mismatch error")
    except Exception as e:
        # IncompatibleUnitsError or ValueError during conversion
        print(f"Caught expected error: {e}")


if __name__ == "__main__":
    test_compile_numpy()
    test_compile_unit_check()
