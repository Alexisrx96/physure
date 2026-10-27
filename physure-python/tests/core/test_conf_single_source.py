"""The default unit catalog is physure-core's embedded physure.conf."""

from pathlib import Path

import pytest

import physure
from physure import Q_
from physure.application.context import get_current_system


def test_percent_converts_to_ppm():
    assert Q_(5, "percent").to("ppm").magnitude == pytest.approx(50_000)


def test_spanish_alias_julio_is_joule():
    assert Q_(1, "julio").to("J").magnitude == pytest.approx(1.0)


def test_core_settings_reach_python():
    settings = get_current_system().settings
    assert settings["parallel_threshold"] == "10000"
    assert settings["max_loop_iterations"] == "1000000"
    assert settings["max_pipeline_steps"] == "1000"


def test_python_package_ships_no_second_catalog():
    config_dir = Path(physure.__file__).parent / "infrastructure" / "config"
    assert not (config_dir / "physure.conf").exists()


def test_native_extension_exposes_the_embedded_catalog():
    from physure._core import default_conf

    text = default_conf()
    assert "[Units]" in text
    assert "percent" in text


def test_temperature_scales_still_convert():
    assert Q_(25, "degC").to("K").magnitude == pytest.approx(298.15)
    assert Q_(77, "degF").to("K").magnitude == pytest.approx(298.15, rel=1e-3)
