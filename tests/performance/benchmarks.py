import numpy as np
import pytest

from measurekit.domain.measurement.converters import LinearConverter
from measurekit.domain.measurement.dimensions import Dimension
from measurekit.domain.measurement.quantity import Quantity
from measurekit.domain.measurement.system import UnitSystem


@pytest.fixture
def benchmark_system():
    sys = UnitSystem("Benchmark")
    le = Dimension({"L": 1})
    sys.register_unit("m", le, LinearConverter(1.0), "meter")
    return sys


def test_benchmark_quantity_addition(benchmark, benchmark_system):
    m = benchmark_system.get_unit("m")
    q1 = Quantity(10.0, m, system=benchmark_system)
    q2 = Quantity(5.0, m, system=benchmark_system)

    def add():
        return q1 + q2

    benchmark(add)


def test_benchmark_quantity_creation(benchmark, benchmark_system):
    m = benchmark_system.get_unit("m")

    def create():
        return Quantity(10.0, m, system=benchmark_system)

    benchmark(create)


def test_benchmark_vectorized_addition(benchmark, benchmark_system):
    m = benchmark_system.get_unit("m")
    data1 = np.random.randn(1000)
    data2 = np.random.randn(1000)
    q1 = Quantity.from_input(data1, m, benchmark_system)
    q2 = Quantity.from_input(data2, m, benchmark_system)

    def add_vectorized():
        return q1 + q2

    benchmark(add_vectorized)
