"""Currency units (USD, EUR, MXN), read from the shipped ``currency.conf``.

The units and their rates are catalog text, the file ``currency.conf`` of
``physure.infrastructure.config.systems``, not Python literals:
`enable_currency` reads it and extends a system with it through the core's
registry. The rates are placeholders, not market data; override them with
your own conf text (``system.extend_core(...)``), where a later definition
of a unit replaces the shipped one.

The file is read from the installed package and from nowhere else: a
missing file raises, it never falls back to a ``currency.conf`` in the
working directory.

Python does not honour a frozen catalog yet (until Phase 5 of the conf
freeze): these units come from the shipped file even while a frozen file is
in force, so they are not covered by it.
"""

from importlib import resources

_SYSTEMS_PACKAGE = "physure.infrastructure.config.systems"
_CURRENCY_CONF = "currency.conf"


def currency_conf_text() -> str:
    """The text of the shipped ``currency.conf``.

    Raises:
        FileNotFoundError: The installed package has no ``currency.conf``.
    """
    return (
        resources.files(_SYSTEMS_PACKAGE)
        .joinpath(_CURRENCY_CONF)
        .read_text(encoding="utf-8")
    )


def enable_currency(system=None):
    """Adds the money dimension and the currency units to a system.

    The current system when `system` is None. Calling it again defines the
    same units again, without a warning, and puts back the shipped rates.

    Raises:
        FileNotFoundError: The installed package has no ``currency.conf``;
            the system is left as it was.
    """
    text = currency_conf_text()
    if system is None:
        from physure.application.context import get_current_system

        system = get_current_system()
    system.extend_core(text)
