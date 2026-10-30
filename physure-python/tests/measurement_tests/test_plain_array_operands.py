"""A plain number or array is dimensionless: it leaves a quantity's unit alone, whichever side it is on.

``Q_(array, "s") * np.array(...)`` used to come out dimensionless (``unit * ndarray`` is numpy's reflected product, an
array of units), and with torch ``tensor / Q`` kept the unit instead of inverting it and ``tensor + Q`` was treated as
if the tensor were in the quantity's unit. A bare number must not be added to a dimensioned quantity at all.
"""

import numpy as np
import pytest

from physure import Q_
from physure.domain.exceptions import IncompatibleUnitsError

from ._array_backends import AVAILABLE, wrapper

BACKENDS = AVAILABLE
VALUES = np.array([1.0, 2.0, 3.0])
UNCERTAINTIES = np.array([0.1, 0.2, 0.15])
NUMBERS = np.array([2.0, 4.0, 5.0])


def build(lib, uncertain):
    wrap = wrapper(lib)
    quantity = (
        Q_(wrap(VALUES), "s", wrap(UNCERTAINTIES))
        if uncertain
        else Q_(wrap(VALUES), "s")
    )
    return quantity, wrap(NUMBERS)


def exponents(result):
    return {
        symbol: int(power)
        for symbol, power in dict(result.unit.exponents).items()
    }


def values(x):
    return (
        x.detach().numpy()
        if hasattr(x, "detach")
        else np.asarray(x, dtype=float)
    )


@pytest.mark.parametrize(
    "uncertain", [False, True], ids=["exact", "uncertain"]
)
@pytest.mark.usefixtures("x64")
@pytest.mark.parametrize("lib", BACKENDS)
class TestUnitsWithPlainArrays:
    def test_quantity_times_and_divided_by_an_array_keeps_its_unit(
        self, lib, uncertain
    ):
        quantity, numbers = build(lib, uncertain)
        assert exponents(quantity * numbers) == {"s": 1}
        assert exponents(quantity / numbers) == {"s": 1}
        np.testing.assert_allclose(
            values((quantity * numbers).magnitude), VALUES * NUMBERS
        )
        np.testing.assert_allclose(
            values((quantity / numbers).magnitude), VALUES / NUMBERS
        )

    def test_array_times_a_quantity_keeps_its_unit(self, lib, uncertain):
        quantity, numbers = build(lib, uncertain)
        result = numbers * quantity
        assert exponents(result) == {"s": 1}
        np.testing.assert_allclose(values(result.magnitude), VALUES * NUMBERS)

    def test_array_divided_by_a_quantity_inverts_its_unit(
        self, lib, uncertain
    ):
        quantity, numbers = build(lib, uncertain)
        result = numbers / quantity
        assert exponents(result) == {"s": -1}
        np.testing.assert_allclose(values(result.magnitude), NUMBERS / VALUES)

    def test_a_bare_array_cannot_be_added_to_or_subtracted_from_a_dimensioned_quantity(
        self, lib, uncertain
    ):
        quantity, numbers = build(lib, uncertain)
        for operation in (
            lambda: quantity + numbers,
            lambda: numbers + quantity,
            lambda: quantity - numbers,
            lambda: numbers - quantity,
        ):
            with pytest.raises(IncompatibleUnitsError):
                operation()


@pytest.mark.usefixtures("x64")
@pytest.mark.parametrize("lib", BACKENDS)
def test_the_standard_uncertainty_of_a_quantity_with_a_plain_array_follows_the_gum(
    lib,
):
    """y = n * q: u(y) = |n| u(q).  y = q / n: u(y) = u(q) / |n|.  y = n / q: u(y) = |n / q**2| u(q)."""
    quantity, numbers = build(lib, True)
    np.testing.assert_allclose(
        values((quantity * numbers).uncertainty),
        NUMBERS * UNCERTAINTIES,
        rtol=1e-9,
    )
    np.testing.assert_allclose(
        values((numbers * quantity).uncertainty),
        NUMBERS * UNCERTAINTIES,
        rtol=1e-9,
    )
    np.testing.assert_allclose(
        values((quantity / numbers).uncertainty),
        UNCERTAINTIES / NUMBERS,
        rtol=1e-9,
    )
    np.testing.assert_allclose(
        values((numbers / quantity).uncertainty),
        NUMBERS / VALUES**2 * UNCERTAINTIES,
        rtol=1e-9,
    )


@pytest.mark.usefixtures("x64")
@pytest.mark.parametrize("lib", BACKENDS)
def test_a_dimensionless_quantity_can_be_added_to_a_plain_array(lib):
    wrap = wrapper(lib)
    ratio = Q_(wrap(VALUES), "")
    np.testing.assert_allclose(
        values((ratio + wrap(NUMBERS)).magnitude), VALUES + NUMBERS
    )
    np.testing.assert_allclose(
        values((wrap(NUMBERS) - ratio).magnitude), NUMBERS - VALUES
    )
