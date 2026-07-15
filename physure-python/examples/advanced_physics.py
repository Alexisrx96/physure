"""Example script demonstrating advanced physics capabilities in MeasureKit.

This script demonstrates:
1. Non-linear unit conversions (Celsius <-> Fahrenheit).
2. Advanced arithmetic with temperature units (Offset vs Delta).
3. Symbolic differentiation of physical quantities.
"""

import sympy as sp

from measurekit import Q_


def main():
    print("--- Advanced Physics in MeasureKit ---\n")

    # 1. Temperature Conversions (Offset Units)
    print("1. Temperature Conversions:")
    t_c = Q_(100, "degC")
    t_f = t_c.to("degF")
    print(f"  {t_c} = {t_f}")

    t_k = t_c.to("K")
    print(f"  {t_c} = {t_k}")
    print("")

    # 2. Temperature Arithmetic (Offset vs Delta)
    print("2. Temperature Arithmetic:")
    t1 = Q_(100, "degC")
    t2 = Q_(90, "degC")

    print(f"  T1 = {t1}")
    print(f"  T2 = {t2}")

    # Difference between two temperatures is a Delta (Linear)
    try:
        delta_t = t1 - t2
        print(f"  T1 - T2 = {delta_t} (Should be Kelvin/Delta)")
        # Ideally we want to show it in Kelvin or similar
        print(f"  In Kelvin: {delta_t.to('K')}")
    except Exception as e:
        print(f"  Error calculating T1 - T2: {e}")

    # Adding Delta to Temperature
    # 5 K is treated as Delta
    d_k = Q_(5, "K")
    t_new = t1 + d_k
    print(f"  T1 + 5 K = {t_new} (Should be 105 degC)")

    # Adding two Temperatures (Should fail)
    try:
        bad_sum = t1 + t2
        print(f"  T1 + T2 = {bad_sum}")
    except ValueError as e:
        print(f"  T1 + T2 failed as expected: {e}")
    print("")

    # 3. Logarithmic Units
    print("3. Logarithmic Units (dB):")
    # P = 10^(L/10) * Pref
    # 10 dB -> 10^1 = 10.
    # 20 dB -> 10^2 = 100.
    # Sum = 110.
    # 10 log10(110) approx 20.41 dB

    db1 = Q_(10, "dB")  # Assuming dB is registered as Logarithmic
    db2 = Q_(20, "dB")

    # Note: MeasureKit default system might define dB as Linear (dimensionless) if not updated.
    # But usually startup.py handles Log logic if in config.
    # Let's hope default config or logic handles it.
    # If not, this might do linear addition (30 dB).
    # We will see based on implementation.

    try:
        sum_db = db1 + db2
        print(f"  {db1} + {db2} = {sum_db}")
    except Exception as e:
        print(f"  dB addition issue: {e}")
    print("")

    # 4. Symbolic Differentiation
    print("4. Symbolic Differentiation:")
    t = sp.Symbol("t")
    # Define a time quantity with symbolic magnitude
    q_t = Q_(t, "s")

    # Position equation: x = 0.5 * a * t^2
    # Let a = 9.8 m/s^2
    # We can perform symbolic arithmetic with Quantities usually if backend handles it?
    # Or strict differentiation of Q(expr).

    # Let's try simpler: x(t) = 5 * t^2 meters
    expression = 5 * t**2
    x = Q_(expression, "m")

    print(f"  Position x(t) = {x}")
    print("  Differentiating with respect to t (symbol 't')...")

    # Diff with respect to symbol 't' (dimensionless/unknown unit assumed s if logic?)
    # user spec: diff(variable: Quantity | str)
    # If we pass 't', it assumes dimensionless denominator?
    # To get Velocity m/s, we must divide by Time unit.
    # So we should diff wrt a Time Quantity or handle units manually if passing str?
    # diff(self, variable: Quantity) handles unit division.

    # Use q_t which is Quantity(t, 's')

    v = x.diff(q_t)
    print(f"  Velocity v(t) = dx/dt = {v}")

    # Acceleration
    a = v.diff(q_t)
    print(f"  Acceleration a(t) = dv/dt = {a}")

    print("\nDone.")


if __name__ == "__main__":
    main()
