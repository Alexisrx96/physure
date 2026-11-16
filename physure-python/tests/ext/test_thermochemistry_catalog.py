"""The thermochemistry data are catalog data (docs/thermochemistry.md).

Until chemistry plan 3 makes the Python package delegate to the core's reader,
the Python table must equal the catalog's edition ``legacy-python-1``; and the
new sections must not break the start-up reading of the whole catalog
(configparser, strict: a duplicated section or key would raise).
"""

from physure._core import default_conf
from physure.application.startup import _get_config_parser, create_system
from physure.ext.chemistry import STANDARD_ENTHALPY_FORMATION, STANDARD_ENTROPY

EDITION = "Thermochemistry:legacy-python-1"


def test_start_up_reads_the_catalog_with_its_thermochemistry_sections():
    parser = _get_config_parser()
    assert parser.has_section(EDITION)
    assert parser.has_section("Thermochemistry")
    assert parser.get("Chemistry", "thermochemistry") == "legacy-python-1"
    assert parser.get("Chemistry", "atomic_weights") == "standard"
    headers = [line.strip() for line in default_conf().splitlines()]
    assert headers.count("[Chemistry]") == 1
    assert headers.count(f"[{EDITION}]") == 1
    assert create_system() is not None


def test_the_python_table_is_the_catalog_edition():
    table = _get_config_parser()[EDITION]
    by_formula = {}
    for species, value in table.items():
        formula = species[: species.rindex("(")]
        enthalpy, entropy = (float(x) for x in value.split(","))
        by_formula[formula] = (enthalpy, entropy)
    assert set(STANDARD_ENTROPY) == set(STANDARD_ENTHALPY_FORMATION)
    python = {
        name: (STANDARD_ENTHALPY_FORMATION[name], STANDARD_ENTROPY[name])
        for name in STANDARD_ENTHALPY_FORMATION
    }
    assert by_formula == python
