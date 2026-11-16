import configparser
import logging
import math
import warnings
from importlib import resources

import pytest

from physure import Q_
from physure.application.context import get_current_system
from physure.application.startup import create_system
from physure.domain.exceptions import UnknownUnitError
from physure.domain.measurement.system import UnitSystem
from physure.ext import currency
from physure.ext.currency import currency_conf_text, enable_currency

# The values before the rates moved to the shipped currency.conf, recorded
# with the register_unit code: the move is location only, bit for bit.
EUR_100_IN_USD = 108.0
MXN_100_IN_USD = 5.8999999999999995
ALIASES_IN_USD = {
    "USD": 1.0,
    "dollars": 1.0,
    "dolar": 1.0,
    "EUR": 1.08,
    "euros": 1.08,
    "MXN": 0.059,
    "pesos": 0.059,
}


def _results(system):
    q = system.Q_
    return (
        q(100, "EUR").to("USD").magnitude,
        q(100, "MXN").to("USD").magnitude,
        {name: q(1, name).to("USD").magnitude for name in ALIASES_IN_USD},
    )


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


def test_the_rates_are_bit_identical_to_before():
    enable_currency()
    eur = Q_(100, "EUR").to("USD")
    assert eur.magnitude == EUR_100_IN_USD
    assert str(eur.unit) == "USD"
    assert Q_(100, "MXN").to("USD").magnitude == MXN_100_IN_USD


def test_every_alias_resolves_to_its_rate():
    enable_currency()
    _, _, aliases = _results(get_current_system())
    assert aliases == ALIASES_IN_USD


def test_a_second_call_warns_nothing_and_changes_nothing(caplog):
    system = get_current_system()
    enable_currency()
    first = _results(system)
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        with caplog.at_level(logging.DEBUG):
            enable_currency()
    assert caplog.records == []
    assert _results(system) == first


@pytest.mark.parametrize(
    "make_system",
    [lambda: create_system("imperial.conf"), lambda: UnitSystem("bare")],
    ids=["imperial", "bare"],
)
def test_a_system_given_explicitly(make_system):
    system = make_system()
    enable_currency(system)
    assert _results(system) == (
        EUR_100_IN_USD,
        MXN_100_IN_USD,
        ALIASES_IN_USD,
    )


def test_a_users_own_rate_wins():
    system = get_current_system()
    enable_currency()
    system.extend_core("[Units]\nEUR = 1.2, $, noprefix, [euros]\n")
    assert Q_(100, "EUR").to("USD").magnitude == 120.0
    assert Q_(100, "euros").to("USD").magnitude == 120.0
    assert Q_(100, "MXN").to("USD").magnitude == MXN_100_IN_USD


def test_the_shipped_text_reads_as_a_strict_conf():
    text = currency_conf_text()
    parser = configparser.ConfigParser(interpolation=None, strict=True)
    parser.read_string(text)
    assert dict(parser["Dimensions"]) == {"money": "$, noprefix"}
    assert set(parser["Units"]) == {"usd", "eur", "mxn"}
    assert "placeholder" in text.lower()


def test_a_missing_packaged_file_raises_and_never_reads_the_cwd(
    tmp_path, monkeypatch
):
    decoy = tmp_path / "cwd"
    decoy.mkdir()
    (decoy / "currency.conf").write_text(
        "[Dimensions]\nmoney = $, noprefix\n\n"
        "[Units]\nUSD = 1.0, $, noprefix\nEUR = 2.0, $, noprefix\n",
        encoding="utf-8",
    )
    empty_package = tmp_path / "package"
    empty_package.mkdir()
    monkeypatch.chdir(decoy)
    # built before the patch: it reads a shipped system file through the
    # same importlib.resources module, which fails closed too
    system = get_current_system()
    monkeypatch.setattr(
        currency.resources, "files", lambda _package: empty_package
    )

    with pytest.raises(FileNotFoundError):
        enable_currency()
    assert "EUR" not in system
    assert "USD" not in system


def test_the_loader_reads_the_package_resource():
    # The real lookup: the file is a resource of the systems package.
    path = resources.files("physure.infrastructure.config.systems").joinpath(
        "currency.conf"
    )
    assert path.is_file()
    assert currency_conf_text() == path.read_text(encoding="utf-8")
