import sys
import time

# Clear sys.modules of any physure related modules to ensure a fresh import
for key in list(sys.modules.keys()):
    if "physure" in key or "numpy" in key or "torch" in key or "jax" in key:
        del sys.modules[key]

start_time = time.perf_counter()
end_time = time.perf_counter()

print(f"Import time: {(end_time - start_time) * 1000:.2f} ms")
