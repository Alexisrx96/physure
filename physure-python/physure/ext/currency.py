def enable_currency(system=None):
    """Enables the Money/Currency dimension and registers USD, EUR, and MXN units dynamically."""
    if system is None:
        from physure.application.context import get_current_system

        system = get_current_system()

    from physure.domain.measurement.converters import LinearConverter
    from physure.domain.measurement.dimensions import Dimension

    money = Dimension({"$": 1})
    system.register_dimension(money, "Money")

    # Register currency units without prefix support to prevent unit/prefix
    # bloat
    system.register_unit(
        "USD",
        money,
        LinearConverter(scale=1.0),
        "dollar",
        "USD",
        "dollars",
        "dolar",
        allow_prefixes=False,
    )
    system.register_unit(
        "EUR",
        money,
        LinearConverter(scale=1.08),
        "euro",
        "EUR",
        "euros",
        allow_prefixes=False,
    )
    system.register_unit(
        "MXN",
        money,
        LinearConverter(scale=0.059),
        "peso",
        "MXN",
        "pesos",
        allow_prefixes=False,
    )
