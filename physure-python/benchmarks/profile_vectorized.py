import cProfile
import pstats

import numpy as np

from physure.domain.measurement.quantity import Quantity
from physure.domain.measurement.units import get_default_system


def run_operation():
    system = get_default_system()
    meter = system.get_unit("meter")

    N = 1000
    val = np.random.rand(N)
    unc = np.random.rand(N) * 0.1

    q1 = Quantity.from_input(val, meter, system, uncertainty=unc)
    q2 = Quantity.from_input(val, meter, system, uncertainty=unc)
    q3 = q1 + q2


def profile():
    pr = cProfile.Profile()
    pr.enable()
    run_operation()
    pr.disable()
    ps = pstats.Stats(pr).sort_stats("tottime")
    ps.print_stats(20)


if __name__ == "__main__":
    profile()
