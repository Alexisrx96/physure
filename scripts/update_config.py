with open(
    "measurekit/infrastructure/config/measurekit.conf", encoding="utf-8"
) as f:
    lines = f.readlines()

new_lines = []
in_constants = False

cgs_atomic_natural_units = """
# --- CGS UNITS ---
gal = 0.01, L*T^-2, m/s^2, [Gal, gal]
barye = 0.1, M*L^-1*T^-2, Pa, [Ba, barye]
poise = 0.1, M*L^-1*T^-1, Pa*s, [P, poise]
stokes = 1e-4, L^2*T^-1, m^2/s, [St, stokes]
kayser = 100.0, L^-1, 1/m, [kayser, Kayser]
franklin = 3.3356409519815044e-10, I*T, C, [Fr, franklin, statC, statcoulomb]
statvolt = 299.792458, M*L^2*T^-3*I^-1, V, [statV, statvolt]
gauss = 1e-4, M*T^-2*I^-1, T, [G, gauss, Gauss]
oersted = 79.57747154594767, I*L^-1, A/m, [Oe, oersted, Oersted]
maxwell = 1e-8, M*L^2*T^-2*I^-1, Wb, [Mx, maxwell]
biot = 10.0, I, A, [Bi, biot, abampere]
gilbert = 0.7957747154594767, I, A, [Gb, gilbert]
debye = 3.3356409519815044e-30, I*T*L, C*m, [D, debye]

# --- ATOMIC UNITS ---
bohr = 5.29177210903e-11, L, m, [a0, bohr, bohr_radius]
hartree = 4.3597447222071e-18, M*L^2*T^-2, J, [Eh, hartree]
electron_mass = 9.1093837015e-31, M, kg, [me, electron_mass]
atomic_time = 2.4188843265857e-17, T, s, [tau0, atomic_time]
atomic_charge = 1.602176634e-19, I*T, C, [atomic_charge]

# --- NATURAL UNITS ---
planck_length = 1.616255e-35, L, m, [lp, planck_length]
planck_mass = 2.176434e-8, M, kg, [mp, planck_mass]
planck_time = 5.391247e-44, T, s, [tp, planck_time]
planck_temperature = 1.416784e32, O, K, [Tp, planck_temperature]
planck_charge = 1.875545956e-18, I*T, C, [qp, planck_charge]
"""

for line in lines:
    if line.strip() == "[Constants]":
        in_constants = True
        continue
    if in_constants:
        # Skip everything in the old constants section
        continue

    # Skip currency and money dimension lines
    if "money = $," in line:
        continue
    if "USD = 1.0, $," in line:
        continue
    if "EUR = 1.08, $," in line:
        continue
    if "MXN = 0.059, $," in line:
        continue

    # If we are just before [Constants], insert new units first
    new_lines.append(line)

# Append new units at the end of the Units section (before we add Constants)
# Let's find the logarithmic unit decibel and insert new units right after it
final_lines = []
for line in new_lines:
    final_lines.append(line)
    if "decibel = Log," in line:
        final_lines.append(cgs_atomic_natural_units + "\n")

# Now append the new constants section
with open("scratch/generated_constants.conf", encoding="utf-8") as f:
    constants_content = f.read()

final_content = "".join(final_lines) + "\n" + constants_content

with open(
    "measurekit/infrastructure/config/measurekit.conf", "w", encoding="utf-8"
) as f:
    f.write(final_content)

print("Successfully updated measurekit.conf")
