"""What a user sees: identities that must hold exactly, evaluated by the Rust core.

Run from physure-python/ after building the extension:
    uv run python -X utf8 ../docs/superpowers/specs/catalog-audit/audit_identities.py
"""

import pathlib
import re

from mpmath import mp, mpf, pi
from physure._core import evaluate_phs_native

mp.dps = 40
CONF = pathlib.Path(__file__).resolve().parents[4] / "physure-core/src/units/physure.conf"


def conv(src, dst, value="1"):
    return float(evaluate_phs_native(f"{value} {src} => {dst}")[-1].magnitude)


IDENTITIES = [  # (value, from, to, exact result, why)
    ("1", "deg", "arcmin", mpf(60), "1 degree = 60 arcminutes"),
    ("1", "arcmin", "arcsec", mpf(60), "1 arcminute = 60 arcseconds"),
    ("1", "deg", "arcsec", mpf(3600), "1 degree = 3600 arcseconds"),
    ("1", "deg", "rad", pi / 180, "pi/180"),
    ("180", "deg", "rad", pi, "180 degrees = pi radians"),
    ("1", "mile", "ft", mpf(5280), "statute mile"),
    ("1", "ft", "in", mpf(12), ""),
    ("1", "mile", "yd", mpf(1760), ""),
    ("1", "lb", "oz", mpf(16), ""),
    ("1", "gal", "fl_oz", mpf(128), "US gallon"),
    ("1", "gal", "in^3", mpf(231), "US gallon = 231 in^3"),
    ("1", "acre", "ft^2", mpf(43560), "international acre"),
    ("1", "atm", "Pa", mpf(101325), ""),
    ("1", "atm", "Torr", mpf(760), "1 atm = 760 Torr, by definition"),
    ("1", "psi", "lbf/in^2", mpf(1), "psi is lbf per square inch"),
    ("1", "hp", "ft*lbf/s", mpf(550), "mechanical horsepower"),
    ("1", "slug*ft/s^2", "lbf", mpf(1), "slug is lbf s^2/ft"),
    ("1", "lbf", "lb*kp/kg", mpf(1), "lbf = lb x g0 (kilopond = g0 N)"),
    ("1", "ly", "m", mpf(299792458) * mpf("365.25") * 86400, "c x Julian year"),
    ("1", "pc", "ly", 648000 / pi * mpf(149597870700) / (mpf(299792458) * mpf("365.25") * 86400), "pc/ly"),
    ("1", "d", "h", mpf(24), ""),
    ("1", "yr", "d", mpf("365.25"), "Julian year"),
    ("212", "degF", "degC", mpf(100), "boiling point of water"),
    ("32", "degF", "degC", mpf(0), "freezing point of water"),
    ("0", "degC", "K", mpf("273.15"), ""),
    ("1", "cal", "J", mpf("4.184"), ""),
    ("1", "eV", "J", mpf("1.602176634e-19"), ""),
]
print(f"{'conversion':24} {'result':>26} {'exact':>26} {'rel. error':>11}")
bad = 0
for value, src, dst, exact, why in IDENTITIES:
    got = conv(src, dst, value)
    err = abs(mpf(got) - exact) / abs(exact) if exact != 0 else abs(mpf(got))
    flag = "" if err <= 4e-16 else "  <--"
    bad += bool(flag)
    print(f"{value + ' ' + src + ' => ' + dst:24} {got!r:>26} {mp.nstr(exact, 17):>26} {float(err):>11.1e}{flag}")
print(f"\n{bad} of {len(IDENTITIES)} identities off by more than 4e-16")

# constants: identities between the catalog's own values
text = CONF.read_text(encoding="utf-8")
consts = dict(
    (m.group(1), mpf(m.group(2)))
    for m in re.finditer(r"^(\w+) = ([-+0-9.eE]+) \S", text.split("[Constants]")[1].split("[Categories]")[0], re.M)
)
c, h, k = consts["speed_of_light_in_vacuum"], consts["planck_constant"], consts["boltzmann_constant"]
e, eps0, mu0 = consts["elementary_charge"], consts["vacuum_electric_permittivity"], consts["vacuum_mag_permeability"]
sigma, alpha = consts["stefan_boltzmann_constant"], consts["fine_structure_constant"]
hbar = consts["reduced_planck_constant"]
print("\nIdentities among the constants stored in the catalog:")
print(f"  eps0 * mu0 * c^2 - 1                        = {mp.nstr(eps0 * mu0 * c**2 - 1, 3)}   (should be ~1e-10 or less)")
print(f"  sigma / (2 pi^5 k^4 / (15 h^3 c^2)) - 1      = {mp.nstr(sigma / (2 * pi**5 * k**4 / (15 * h**3 * c**2)) - 1, 3)}")
print(f"  alpha / (e^2 / (4 pi eps0 hbar c)) - 1       = {mp.nstr(alpha / (e**2 / (4 * pi * eps0 * hbar * c)) - 1, 3)}")
print(f"  hbar / (h / 2 pi) - 1                        = {mp.nstr(hbar / (h / (2 * pi)) - 1, 3)}")
