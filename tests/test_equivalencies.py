import math

import pytest

from measurekit import Q_, equivalencies, spectral, thermodynamic
from measurekit.domain.exceptions import IncompatibleUnitsError


def test_spectral_equivalency():
    # nm to Hz
    wavelength = Q_(500.0, "nm", uncertainty=10.0)

    # Converting without equivalencies should fail
    with pytest.raises(IncompatibleUnitsError):
        wavelength.to("Hz")

    # Converting with equivalencies keyword argument
    frequency = wavelength.to("Hz", equivalencies=spectral())

    expected_freq_val = 299792458.0 / 500e-9
    assert math.isclose(frequency.magnitude, expected_freq_val, rel_tol=1e-5)

    # Propagated uncertainty check
    # E = c / lambda => dE/dlambda = -c / lambda^2
    # uncertainty: (c / lambda^2) * unc_lambda
    # c = 299792458.0, lambda = 500e-9, unc_lambda = 10e-9
    expected_unc = (299792458.0 / (500e-9**2)) * 10e-9
    assert math.isclose(frequency.uncertainty, expected_unc, rel_tol=1e-4)


def test_spectral_equivalency_context():
    with equivalencies(spectral()):
        energy = Q_(2.0, "eV", uncertainty=0.1)
        wavelength = energy.to("nm")

        # E = h * c / lambda => lambda = h * c / E
        # h = 6.62607015e-34 J*s, c = 299792458 m/s, eV = 1.602176634e-19 J
        h = 6.62607015e-34
        c = 299792458.0
        eV_to_J = 1.602176634e-19

        E_J = 2.0 * eV_to_J
        expected_lambda_m = h * c / E_J
        expected_lambda_nm = expected_lambda_m * 1e9
        assert math.isclose(
            wavelength.magnitude, expected_lambda_nm, rel_tol=1e-5
        )

        # dlambda/dE = -h*c/E^2
        # unc_E_J = 0.1 * eV_to_J
        # expected_unc_m = (h * c / (E_J**2)) * unc_E_J
        # expected_unc_nm = expected_unc_m * 1e9
        expected_unc_nm = (expected_lambda_nm / 2.0) * 0.1
        assert math.isclose(
            wavelength.uncertainty, expected_unc_nm, rel_tol=1e-4
        )


def test_thermodynamic_equivalency():
    temp = Q_(300.0, "K", uncertainty=1.0)

    energy = temp.to("eV", equivalencies=thermodynamic())

    k_B = 1.380649e-23
    eV_to_J = 1.602176634e-19
    expected_energy_ev = (300.0 * k_B) / eV_to_J
    expected_unc_ev = (1.0 * k_B) / eV_to_J

    assert math.isclose(energy.magnitude, expected_energy_ev, rel_tol=1e-5)
    assert math.isclose(energy.uncertainty, expected_unc_ev, rel_tol=1e-4)
