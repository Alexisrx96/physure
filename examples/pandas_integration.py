import pandas as pd

from measurekit import Q_

# 1. Create a DataFrame with MeasureKit Quantity objects
# Note: In a real-world scenario, these might have uncertainties already
df = pd.DataFrame(
    {
        "volts": [Q_(120.0, "V"), Q_(240.0, "V"), Q_(480.0, "V")],
        "time": [0, 1, 2],
    }
)

print("--- Original DataFrame ---")
print(df)
print(f"Dtype of 'volts': {df['volts'].dtype}")

# 2. Bulk unit conversion via the [.mk] accessor
# This triggers the internal vectorized engine (no row-wise looping for conversion)
df["kV"] = df["volts"].mk.to("kV")

print("\n--- After bulk unit conversion to 'kV' ---")
print(df)

# 3. Extracting magnitudes and uncertainties for plotting or analysis
# These return standard Pandas Series of floats
mags = df["kV"].mk.magnitude
uncs = df["kV"].mk.uncertainty

print("\n--- Extracted Data ---")
print(f"Magnitudes: \n{mags}")
print(f"Uncertainties: \n{uncs}")
print(f"Shared Unit: {df['kV'].mk.unit}")

# 4. Attaching uncertainty in bulk
# Here we attach a constant 5% uncertainty to the original volts
# plus_minus(uncertainty) can take a scalar or an array/Series
df["volts_err"] = df["volts"].mk.plus_minus(df["volts"].mk.magnitude * 0.05)

print("\n--- Volts with attached 5% Relative Uncertainty ---")
print(df["volts_err"])

# 5. Accessing the underlying VectorizedQuantity (The Fast-Path)
# For complex math, avoid Pandas' object loops by dropping down to the vectorized array.
# df["A"].mk.array returns a single Quantity object backed by a NumPy array.
v_array = df["volts"].mk.array
result_fast = v_array * 2 + Q_(10, "V")

print("\n--- Fast-Path Vectorized Operation Result ---")
print(result_fast)
print(f"Result type: {type(result_fast)}")

# 6. Serialization (JSON)
# Produces a clean format for frontend APIs or inter-service communication
json_output = df["volts_err"].mk.to_json()
print("\n--- JSON Serialization ---")
print(json_output)
