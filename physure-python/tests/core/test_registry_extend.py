"""A1 stage 2, step 2a: a registry takes the definitions of a system, strictly."""

import pytest

from physure._core import UnitRegistry

MONEY = (
    "[Units]\n"
    "USD = 1.0, $, noprefix, [USD, dollar, dollars]\n"
    "EUR = 1.08, $, noprefix, [EUR, euro, euros]\n"
)


def _extended(text):
    registry = UnitRegistry.from_conf()
    registry.extend_conf(text)
    return registry


def test_money_is_a_dimension_the_registry_can_scale():
    registry = _extended(MONEY)
    assert registry.facts("EUR")["dimension"] == {"$": 1}
    factor = registry.factor_between(
        registry.parse_atomic("EUR"), registry.parse_atomic("USD")
    )
    assert factor == 1.08
    assert registry.alias_of(registry.parse_atomic("euros")) == "euros"


def test_a_level_of_a_power_has_the_dimension_of_a_power():
    registry = _extended("[Units]\ndBW = Log, 10, 1.0, M*L^2*T^-3, [dBW]\n")
    facts = registry.facts("dBW")
    assert facts["dimension"] == {"L": 2, "M": 1, "T": -3}
    assert facts["is_linear"] is False


def test_a_line_the_core_cannot_read_is_an_error_and_keeps_nothing():
    registry = UnitRegistry.from_conf()
    text = "[Units]\nfurlong = 201.168, L, [fur]\nleague = lots, L, [lg]\n"
    with pytest.raises(ValueError, match="is not a number"):
        registry.extend_conf(text)
    with pytest.raises(ValueError, match="Unknown unit"):
        registry.facts("fur")


def test_from_conf_text_is_strict_too():
    with pytest.raises(ValueError, match="is not a number"):
        UnitRegistry.from_conf_text("[Units]\nleague = lots, L, [lg]\n")
    registry = UnitRegistry.from_conf_text(MONEY)
    assert registry.facts("euro")["dimension"] == {"$": 1}


def test_a_registry_of_its_own_knows_what_it_is_given_and_the_si_base_units():
    registry = UnitRegistry()
    with pytest.raises(ValueError, match="Unknown unit"):
        registry.facts("ft")
    registry.extend_conf(
        "[Units]\nmetre = 1.0, L, [m, metre]\nfoot = 0.3048, L, [ft]\n"
    )
    assert registry.factor_between(
        registry.parse_atomic("ft"), registry.parse_atomic("m")
    ) == pytest.approx(0.3048)
    # any text parsed also registers the eight SI base units, whether or not it mentions them
    assert registry.facts("kg")["dimension"] == {"M": 1}
    with pytest.raises(ValueError, match="Unknown unit"):
        registry.facts("lb")
