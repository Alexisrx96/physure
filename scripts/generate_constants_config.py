import re

import scipy.constants

# Base and derived unit symbols supported by measurekit.conf (after CGS/Atomic/Natural expansion)
VALID_UNITS = {
    "m",
    "kg",
    "g",
    "s",
    "A",
    "K",
    "mol",
    "cd",
    "rad",
    "sr",
    "Hz",
    "N",
    "Pa",
    "J",
    "W",
    "C",
    "V",
    "Ohm",
    "S",
    "F",
    "Wb",
    "T",
    "H",
    "lm",
    "lx",
    "kat",
    "deg",
    "arcmin",
    "arcsec",
    "1",
    "eV",
    "Ba",
    "dyn",
    "erg",
    "P",
    "St",
    "kayser",
    "Fr",
    "statV",
    "G",
    "Oe",
    "Mx",
    "Bi",
    "Gb",
    "D",
    "bohr",
    "hartree",
    "atomic_time",
    "atomic_charge",
    "planck_length",
    "planck_mass",
    "planck_time",
    "planck_temperature",
    "planck_charge",
}

PREFIXES = {
    "Q",
    "R",
    "Y",
    "Z",
    "E",
    "P",
    "T",
    "G",
    "M",
    "k",
    "h",
    "da",
    "d",
    "c",
    "m",
    "u",
    "n",
    "p",
    "f",
    "a",
    "z",
    "y",
    "r",
    "q",
}


def clean_name(name):
    # Convert name like "Planck constant" to planck_constant
    # Remove characters like "(", ")", "/", "-" and replace with underscore
    name = (
        name.replace("'", "")
        .replace("-", "_")
        .replace(" ", "_")
        .replace("(", "")
        .replace(")", "")
        .replace("/", "_")
    )
    # Remove trailing dot or other symbols
    name = re.sub(r"[^a-zA-Z0-9_]", "", name)
    name = re.sub(r"_+", "_", name)
    return name.lower()


def is_valid_token(token):
    if not token or token.isdigit() or token == "1":
        return True
    # Strip power exponent like ^2, ^-1, etc.
    token = re.split(r"[\^]", token)[0]
    token = token.strip()
    if token in VALID_UNITS:
        return True
    for p in PREFIXES:
        if token.startswith(p):
            sub = token[len(p) :]
            if sub in VALID_UNITS:
                return True
    return False


def clean_unit(unit_str):
    if not unit_str:
        return "1"
    # Replace spaces with *
    unit = unit_str.replace(" ", "*")

    # Split unit expression into tokens and validate each token
    # e.g. C^3*m^3*J^-2
    tokens = re.split(r"[\*/]", unit)
    for t in tokens:
        # Remove sign if any like -1, etc.
        t = t.replace("-", "")
        if not is_valid_token(t):
            return None  # Invalid unit string
    return unit


def generate_constants_section():
    lines = []
    lines.append("[Constants]")
    lines.append(
        "# Fundamental Physical Constants (CODATA 2022 generated from scipy)"
    )

    for name, (val, unit, _unc) in sorted(
        scipy.constants.physical_constants.items()
    ):
        c_name = clean_name(name)
        # Check if name is a valid identifier
        if not re.match(r"^[a-zA-Z_][a-zA-Z0-9_]*$", c_name):
            continue
        c_unit = clean_unit(unit)
        if c_unit is None:
            # Skip constants with unsupported/invalid units
            continue
        val_str = f"{val:e}" if abs(val) < 1e-4 or abs(val) > 1e4 else f"{val}"
        lines.append(f"{c_name} = {val_str} {c_unit}")

    return "\n".join(lines)


if __name__ == "__main__":
    content = generate_constants_section()
    import os

    os.makedirs("scratch", exist_ok=True)
    with open("scratch/generated_constants.conf", "w", encoding="utf-8") as f:
        f.write(content)
    print(
        "Successfully generated constants config at scratch/generated_constants.conf"
    )
