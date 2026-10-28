"""[Constants] is CODATA 2022 at full precision: what scipy.constants says.

The table used to be written with `f"{value:e}"`, which keeps 7 significant
digits, so 189 of its 411 values were wrong in the 7th digit. The generator
now writes `repr`, and this test compares the stored values with what the
generator produces from scipy.constants.
"""

import importlib.util
import pathlib
import re

import pytest

from physure._core import default_conf

scipy_constants = pytest.importorskip("scipy.constants")
if scipy_constants.physical_constants["electron mass"][0] != 9.1093837139e-31:
    pytest.skip("scipy.constants is not CODATA 2022", allow_module_level=True)

GENERATOR = (
    pathlib.Path(__file__).resolve().parents[2]
    / "scripts"
    / "generate_constants_config.py"
)


def _values(section):
    return {
        m.group(1): float(m.group(2))
        for m in re.finditer(r"^(\w+) = (\S+) ", section, re.M)
    }


def test_every_constant_is_the_codata_2022_value_at_full_precision():
    spec = importlib.util.spec_from_file_location("generator", GENERATOR)
    generator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(generator)
    generated = _values(generator.generate_constants_section())
    stored = _values(
        default_conf().split("[Constants]")[1].split("[Categories]")[0]
    )
    assert len(stored) == len(generated) > 400
    assert stored == generated
