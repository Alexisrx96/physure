import pytest
import timeit
from measurekit import create_default_system
from measurekit.domain.measurement.quantity import Quantity


def test_celsius_conversion():
    system = create_default_system()
    celsius = system.get_unit("celsius")
    kelvin = system.get_unit("kelvin")

    q_c = Quantity(
        0, unit=celsius, system=system
    )  # Celsius usa AffineConverter(1.0, 273.15)
    q_k = q_c.to(kelvin)
    assert q_k.magnitude == 273.15  # Debe sumar el offset


def test_fahrenheit_conversion():
    system = create_default_system()
    fahrenheit = system.get_unit("fahrenheit")
    celsius = system.get_unit("celsius")

    q_f = Quantity(32, unit=fahrenheit, system=system)
    q_c = q_f.to(celsius)
    assert abs(q_c.magnitude - 0.0) < 1e-9


def test_temperature_difference():
    system = create_default_system()
    celsius = system.get_unit("celsius")

    # Si restas dos temperaturas absolutas:
    t1 = Quantity(100, unit=celsius, system=system)
    t2 = Quantity(90, unit=celsius, system=system)
    diff = t1 - t2
    # Con el Fast Path:
    # diff.magnitude = 100 - 90 = 10
    # diff.unit = celsius
    assert diff.magnitude == 10
    assert diff.unit == celsius


def test_performance_benchmark():
    system = create_default_system()
    u = system.get_unit("m")
    q1 = Quantity(1.0, u, system=system)
    q2 = Quantity(2.0, u, system=system)

    number = 100_000
    t = timeit.timeit(lambda: q1 + q2, number=number)
    print(f"\nSuma de {number} cantidades: {t:.4f}s")
    assert t < 10.0  # Increased threshold to account for coverage overhead
