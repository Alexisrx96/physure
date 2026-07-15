import time

import numpy as np

from physure.domain.measurement.quantity import Quantity
from physure.domain.measurement.units import get_default_system


def benchmark():
    system = get_default_system()
    meter = system.get_unit("meter")

    N = 10000
    val = np.random.rand(N)
    unc = np.random.rand(N) * 0.1

    q1 = Quantity.from_input(val, meter, system, uncertainty=unc)
    q2 = Quantity.from_input(val, meter, system, uncertainty=unc)

    start = time.perf_counter()
    q3 = q1 + q2
    end = time.perf_counter()

    print(f"Time for N={N} addition: {(end - start) * 1000:.2f} ms")


if __name__ == "__main__":
    benchmark()
