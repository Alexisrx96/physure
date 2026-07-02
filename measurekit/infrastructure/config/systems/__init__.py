"""Pre-configured unit systems.

`imperial` and `international` are built lazily on first attribute access:
startup.py imports this package via ``importlib.resources`` just to locate
the .conf files, and building both systems eagerly tripled bootstrap time.
"""

_SYSTEMS = {"imperial": "imperial.conf", "international": "international.conf"}


def __getattr__(name: str):
    conf = _SYSTEMS.get(name)
    if conf is None:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
    from measurekit.application.startup import create_system

    system = create_system(conf)
    globals()[name] = system  # cache: __getattr__ won't be called again
    return system
