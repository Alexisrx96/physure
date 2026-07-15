import math

import pytest

from measurekit import Q_
from measurekit.domain.exceptions import UnknownUnitError
from measurekit.ext.currency import enable_currency


def test_currency_extension():
    # Before enabling, USD/EUR/MXN should not exist or fail to parse
    with pytest.raises(UnknownUnitError):
        Q_(1.0, "USD")

    # Enable currency
    enable_currency()

    # Now they should parse and work!
    usd = Q_(100.0, "USD")
    eur = usd.to("EUR")
    mxn = usd.to("MXN")

    # 100 USD = 100 / 1.08 = 92.59259 EUR
    # 100 USD = 100 / 0.059 = 1694.915 MXN
    assert math.isclose(eur.magnitude, 100.0 / 1.08, rel_tol=1e-5)
    assert math.isclose(mxn.magnitude, 100.0 / 0.059, rel_tol=1e-5)

    # Check that prefixes are not allowed (no milliUSD/attoEUR)
    with pytest.raises(UnknownUnitError):
        Q_(1.0, "mUSD")
    with pytest.raises(UnknownUnitError):
        Q_(1.0, "aEUR")
