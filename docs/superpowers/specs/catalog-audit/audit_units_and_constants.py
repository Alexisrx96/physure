"""Audit of physure-core/src/units/physure.conf: truncated or wrong numeric factors.

Run from physure-python/ with scipy and mpmath available:
    uv run python -X utf8 ../docs/superpowers/specs/catalog-audit/audit_units_and_constants.py
"""

import importlib.util
import pathlib
import re
import sys

import scipy.constants as sc
from mpmath import mp, mpf, pi, sqrt

mp.dps = 50
ROOT = pathlib.Path(__file__).resolve().parents[4]
CONF = ROOT / "physure-core/src/units/physure.conf"
GEN = ROOT / "physure-python/scripts/generate_constants_config.py"
EPS = 4e-16  # one or two units in the last place of an f64


def rel(actual, ref):
    ref = mpf(ref)
    return abs(mpf(actual) - ref) / abs(ref) if ref != 0 else abs(mpf(actual))


def sections(text):
    out, cur = {}, None
    for line in text.splitlines():
        m = re.match(r"^\[(\w+)\]", line)
        if m:
            cur = m.group(1)
            out[cur] = []
        elif cur is not None:
            out[cur].append(line)
    return out


sec = sections(CONF.read_text(encoding="utf-8"))

# ---------------------------------------------------------------- [Units]
g0 = mpf("9.80665")
lb = mpf("0.45359237")
ft = mpf("0.3048")
inch = mpf("0.0254")
lbf = lb * g0
c = mpf(299792458)

REF = {  # name -> (value, how it is defined)
    "inch": (inch, "exact, international yard and pound agreement"),
    "foot": (ft, "exact"),
    "yard": (mpf("0.9144"), "exact"),
    "mile": (mpf("1609.344"), "exact, statute mile = 5280 ft"),
    "nautical_mile": (mpf(1852), "exact"),
    "light_year": (c * mpf("365.25") * 86400, "c x Julian year, exact"),
    "parsec": (648000 / pi * mpf(149597870700), "648000/pi au, au = 149597870700 m exact"),
    "angstrom": (mpf("1e-10"), "exact"),
    "gram": (mpf("0.001"), "exact"),
    "tonne": (mpf(1000), "exact"),
    "pound": (lb, "exact"),
    "ounce": (lb / 16, "exact, avoirdupois"),
    "slug": (lbf / ft, "lbf s^2/ft, exact given g0"),
    "minute": (mpf(60), "exact"),
    "hour": (mpf(3600), "exact"),
    "day": (mpf(86400), "exact"),
    "year": (mpf("365.25") * 86400, "Julian year, exact (IAU)"),
    "hectare": (mpf(10000), "exact"),
    "acre": (43560 * ft**2, "international acre = 43560 ft^2, exact"),
    "liter": (mpf("0.001"), "exact"),
    "gallon_us": (231 * inch**3, "231 in^3, exact"),
    "fluid_ounce": (231 * inch**3 / 128, "gallon/128, exact"),
    "dyne": (mpf("1e-5"), "exact"),
    "pound_force": (lbf, "lb x g0, exact"),
    "kilopond": (g0, "g0, exact"),
    "erg": (mpf("1e-7"), "exact"),
    "calorie": (mpf("4.184"), "thermochemical calorie, exact"),
    "electron_volt": (mpf("1.602176634e-19"), "SI 2019, exact"),
    "watt_hour": (mpf(3600), "exact"),
    "btu": (mpf("1055.05585262"), "BTU_IT, exact (other BTUs differ)"),
    "horsepower": (550 * ft * lbf, "mechanical hp = 550 ft lbf/s, exact"),
    "bar": (mpf(100000), "exact"),
    "atmosphere": (mpf(101325), "exact"),
    "psi": (lbf / inch**2, "lbf/in^2, exact given g0"),
    "torr": (mpf(101325) / 760, "atm/760, exact"),
    "mmHg": (mpf("13595.1") * g0 / 1000, "conventional mmHg"),
    "degree": (pi / 180, "pi/180"),
    "arcminute": (pi / 10800, "pi/10800"),
    "arcsecond": (pi / 648000, "pi/648000"),
    "percent": (mpf("0.01"), "exact"),
    "ppm": (mpf("1e-6"), "exact"),
    "galileo": (mpf("0.01"), "exact"),
    "barye": (mpf("0.1"), "exact"),
    "poise": (mpf("0.1"), "exact"),
    "stokes": (mpf("1e-4"), "exact"),
    "kayser": (mpf(100), "exact"),
    "franklin": (1 / (10 * c), "1/(10 c) C, exact"),
    "statvolt": (c / 10**6, "c/1e6 V, exact"),
    "gauss": (mpf("1e-4"), "exact"),
    "oersted": (1000 / (4 * pi), "1000/(4 pi) A/m"),
    "maxwell": (mpf("1e-8"), "exact"),
    "biot": (mpf(10), "exact"),
    "gilbert": (10 / (4 * pi), "10/(4 pi) A"),
    "debye": (mpf("1e-21") / c, "1e-21/c C m, exact"),
    "celsius": (mpf(1), "exact"),
    "fahrenheit": (mpf(5) / 9, "5/9 K per degF"),
    "rankine": (mpf(5) / 9, "5/9 K per degR"),
    "delta_celsius": (mpf(1), "exact"),
    "delta_fahrenheit": (mpf(5) / 9, "5/9"),
}
CODATA = {  # name -> scipy CODATA name
    "bohr": "Bohr radius",
    "hartree": "Hartree energy",
    "electron_mass": "electron mass",
    "atomic_time": "atomic unit of time",
    "atomic_charge": "elementary charge",
    "planck_length": "Planck length",
    "planck_mass": "Planck mass",
    "planck_time": "Planck time",
    "planck_temperature": "Planck temperature",
}
hbar = mpf(sc.hbar)
planck_charge = sqrt(4 * pi * mpf(sc.epsilon_0) * hbar * c)

units = []
for line in sec["Units"]:
    m = re.match(r"^(\w+)\s*=\s*([-+0-9.eE]+)\s*,", line)
    if m:
        units.append((m.group(1), m.group(2), line))

print("=" * 78)
print("A. [Units]: numeric factors against their definition")
print("=" * 78)
ok, bad, unverified = [], [], []
for name, lit, _ in units:
    val = mpf(lit)
    if name in REF:
        ref, how = REF[name]
        r = rel(val, ref)
        (ok if r <= EPS else bad).append((name, lit, ref, r, how))
    elif name in CODATA:
        ref = mpf(sc.physical_constants[CODATA[name]][0])
        r = rel(val, ref)
        (ok if r <= EPS else bad).append((name, lit, ref, r, f"CODATA 2022 '{CODATA[name]}'"))
    elif name == "planck_charge":
        r = rel(val, planck_charge)
        (ok if r <= EPS else bad).append((name, lit, planck_charge, r, "sqrt(4 pi eps0 hbar c), CODATA 2022"))
    else:
        unverified.append((name, lit))
print(f"checked {len(ok) + len(bad)} of {len(units)} entries; exact within f64: {len(ok)}; off: {len(bad)}")
print(f"{'unit':20} {'in catalog':>14} {'reference (16 digits)':>24} {'rel. error':>11}  definition")
for name, lit, ref, r, how in sorted(bad, key=lambda t: -t[3]):
    print(f"{name:20} {lit:>14} {mp.nstr(ref, 16):>24} {float(r):>11.2e}  {how}")
print("\nunverified (no reference in this audit):", ", ".join(f"{n}={v}" for n, v in unverified))

# fahrenheit offset
for line in sec["Units"]:
    m = re.match(r"^fahrenheit\s*=\s*([-+0-9.eE]+)\s*,\s*([-+0-9.eE]+)", line)
    if m:
        off = mpf("459.67") * 5 / 9
        print(f"\nfahrenheit zero point: catalog {m.group(2)} vs {mp.nstr(off, 17)} -> rel {float(rel(mpf(m.group(2)), off)):.1e}")

# ---------------------------------------------------------------- [Constants]
spec = importlib.util.spec_from_file_location("gen", GEN)
gen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gen)
sci = {}
for k, (v, u, _unc) in sc.physical_constants.items():
    sci.setdefault(gen.clean_name(k), (v, u))

consts = []
for line in sec["Constants"]:
    m = re.match(r"^(\w+)\s*=\s*([-+0-9.eE]+)\s+(\S.*)$", line)
    if m:
        consts.append((m.group(1), m.group(2), m.group(3)))


def mantissa_digits(lit):
    mant = re.split(r"[eE]", lit)[0].lstrip("+-")
    return len(mant.replace(".", "").lstrip("0")) if mant.strip("0.") else 0


print("\n" + "=" * 78)
print("B. [Constants]: against scipy.constants (CODATA 2022)")
print("=" * 78)
same, seven, other, missing = [], [], [], []
for name, lit, unit in consts:
    if name not in sci:
        missing.append(name)
        continue
    s = mpf(sci[name][0])
    r = rel(mpf(lit), s)
    if r <= EPS:
        same.append(name)
    elif r <= 1e-6 and mantissa_digits(lit) <= 7 and "e" in lit.lower():
        seven.append((name, lit, s, r))
    else:
        other.append((name, lit, s, r))
print(f"{len(consts)} constants. Full precision: {len(same)}. Cut to 7 significant digits: {len(seven)}. "
      f"Other differences: {len(other)}. Not in scipy: {len(missing)}")
if seven:
    worst = max(r for *_, r in seven)
    print(f"cut to 7 digits: worst relative error {float(worst):.1e}")
    print("largest 12:")
    for name, lit, s, r in sorted(seven, key=lambda t: -t[3])[:12]:
        print(f"  {name:48} {lit:>14} vs {mp.nstr(s, 13):>16}  {float(r):.1e}")
if other:
    print("other differences:")
    for name, lit, s, r in sorted(other, key=lambda t: -t[3]):
        print(f"  {name:48} {lit:>16} vs {mp.nstr(s, 13):>18}  {float(r):.1e}")
if missing:
    print("not in scipy:", ", ".join(missing))

# friendly names of physure.constants
sys.path.insert(0, str(ROOT / "physure-python"))
txt = (ROOT / "physure-python/physure/constants.py").read_text(encoding="utf-8")
alias = dict(re.findall(r'"(\w+)":\s*"(\w+)"', txt))
state = {n: ("cut to 7 digits" if n in {a for a, *_ in seven} else "other" if n in {a for a, *_ in other} else "full precision" if n in same else "not in scipy") for n, *_ in consts}
print("\nPublic short names of physure.constants and the state of the entry behind them:")
for short, canon in alias.items():
    print(f"  {short:10} -> {canon:40} {state.get(canon, 'entry not in [Constants]')}")

# duplicate names between [Units] and [Constants]
dups = sorted({n for n, *_ in units} & {n for n, *_ in consts})
print("\nnames defined in both [Units] and [Constants]:", dups)
