"""Every name in physure.conf means one unit; the shadowed readings are listed.

The registry only logs a warning when two units claim a name and the later one
silently wins (the `gal` gallon/galileo incident). This test reads the catalog
and fails instead.
"""

import re

from physure import Q_
from physure._core import default_conf

# A short symbol that is also a prefix followed by another unit's symbol
# (centi + d, femto + t, pico + H, yocto + d). The exact symbol wins; the other
# reading needs a long prefixed name, which the unit parser does not accept yet.
SHADOWED = {"cd", "ft", "kg", "pH", "yd"}


def _catalog():
    text = default_conf()
    prefixes = []
    for line in text.split("[Prefixes]")[1].split("[Units]")[0].splitlines():
        m = re.match(r"^(\w+)\s*=\s*(\w+),", line)
        if m:
            prefixes.append(m.group(2))
    units = []
    for line in text.split("[Units]")[1].split("[Constants]")[0].splitlines():
        m = re.match(r"^(\w+)\s*=\s*(.*)$", line)
        alias_list = m and re.search(r"\[([^\]]*)\]\s*$", m.group(2))
        if alias_list:
            aliases = {a.strip() for a in alias_list.group(1).split(",")}
            units.append(
                (m.group(1), aliases | {m.group(1)}, "noprefix" in m.group(2))
            )
    return prefixes, units


def test_no_name_is_claimed_by_two_units():
    _, units = _catalog()
    owners = {}
    clashes = []
    for name, names, _ in units:
        for symbol in names:
            if symbol in owners and owners[symbol] != name:
                clashes.append((symbol, owners[symbol], name))
            owners[symbol] = name
    assert not clashes, f"names claimed by two units: {clashes}"


def test_the_shadowed_readings_are_exactly_the_known_ones():
    prefixes, units = _catalog()
    exact = {symbol for _, names, _ in units for symbol in names}
    readings = {}
    for name, names, noprefix in units:
        if noprefix:
            continue
        for symbol in names:
            for prefix in prefixes:
                readings.setdefault(prefix + symbol, set()).add(name)
    shadowed = {s for s in exact if s in readings}
    assert shadowed == SHADOWED
    ambiguous = {
        s: u for s, u in readings.items() if s not in exact and len(u) > 1
    }
    assert not ambiguous, f"prefix readings that collide: {ambiguous}"


def test_every_unit_can_be_written_with_its_own_name():
    _, units = _catalog()
    unresolved = []
    for name, _, _ in units:
        try:
            Q_(1, name)
        except Exception:
            unresolved.append(name)
    assert not unresolved, f"names that do not resolve: {unresolved}"
