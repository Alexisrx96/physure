# 📐 Physure Language & Syntax Reference Specification

Physure (PHS) is a high-performance domain-specific programming language and computation engine for physical quantities, dimensional analysis, uncertainty propagation, symbolic calculus, and 3D WebGL scientific visualization.

---

## 1. Syntax & Language Constructs

### 1.1 Physical Quantities & Unit Conversions
Quantities consist of a numerical magnitude, an optional uncertainty, and a physical unit expression.

```phs
# Quantity literals with SI or derived units
pressure = 100.0 kPa
speed = 25.0 m / s
resistance = 50.0 Ohm
measurement = 10.0 +/- 0.5 m        # Measurement with 0.5 m standard deviation
percentage = 5.0 %               # Relative uncertainty or percentage

# Unit Conversion Operator '=>'
p_bar = 100.0 kPa => bar         # Converts 100.0 kPa to bar
v_kmh = 25.0 m / s => km / h     # Converts 25 m/s to km/h
```

#### Angles in a conversion

The SI gives the radian and the steradian the expression 1, and `=>` follows it: an angle is read as 1 when a conversion needs it, so the formulas of mechanics, circuits and photometry convert without ceremony.

```phs
energy = 0.5 * (0.5 kg * m^2) * (10 rad/s)^2   # kg * m^2 * rad^2 * s^-2
e_j = energy => J                              # 25 J
v = (10 rad/s) * (0.5 m) => m/s                # 5 m/s
lux = (100 cd) / (2 m)^2 => lx                 # 25 lx
```

A sum or a comparison never reads an angle as 1: `2 rad + 1` and `2 m*rad + 1 m` are errors, and the way is to convert first, `(2 rad => unity) + 1`. A rate of angle is not a frequency (they differ by 2π), so `10 rad/s => Hz` is an error; divide by the angle of a cycle instead, `(10 rad/s) / (6.283185307179586 rad) => Hz`. `angle_mode = dimension` in `physure.conf` makes the angle a dimension like any other.

#### Comparisons

`==` compares what two quantities stand for, whatever units they are written in: `1 km == 1000 m` and `0.01 degC == 273.16 K` are true in either order. Two values are equal when they agree within a relative 1e-12 of the larger (the rounding of a conversion or of ordinary arithmetic): `1 pm == 2 pm` is false and `(0.1 m + 0.2 m) == 0.3 m` is true. A relative bound never accepts a difference against an exact zero, so `cos(90 deg) == 0` is false: write `abs(cos(90 deg)) < 1e-12`, or `abs(x) < 1e-9 m` with the unit of `x`. Near the bound `==` is not transitive. `inf == inf` is true and NaN equals nothing. `!=` is exactly `not (a == b)`.

`a ≈ b` is `|a − b| <= rel · max(|a|, |b|)` with `rel` from `[Settings] approx_rel_tol` in `physure.conf` (default `1e-3`); `approx(a, b, rel)` names the tolerance (`approx(x, y, 1 %)`). `≈` with exactly one operand zero is an error: no relative tolerance says how close to zero is close.

`x == t +/- k sigma` asks whether the measured `x` lies within `k` of its own standard uncertainties of the exact target `t`: `(9.81 +/- 0.01) m/s^2 == 9.80 m/s^2 +/- 2 sigma` is true. An `x` with no uncertainty is an error; use `==`.

Different dimensions, and operand types that have no comparison (two vectors, a string and a quantity), are errors, never `False`. A comparison binds tighter than `+` and `-`: parenthesise a sum, `(0.1 m + 0.2 m) == 0.3 m`. A number or a quantity used as a condition (`if`, `while`) is false only when it is exactly zero; a temperature point (`0 degC`), a level (`20 dB`) or NaN is not a condition.

### 1.2 Function Definitions & Docstrings
Functions can be defined with or without the optional `fn` keyword. Unit constraints on parameters are checked automatically.

```phs
/// Computes kinetic energy in Joules
/// @param m Mass of the body in kg
/// @param v Velocity of the body in m/s
/// @returns Energy in Joules
fn E_k(m: kg, v: m/s) = 0.5 * m * v^2

# Direct mathematical function syntax (without 'fn')
P(x, y) = 100.0 kPa * sin(x / 1.0 m) * cos(y / 1.0 m)
```

### 1.3 Control Flow & Local Bindings
```phs
doubled = x * 2.0 where x = 10.0 m
status = if pressure > 50.0 kPa then "High Pressure" else "Normal Pressure"
```

### 1.4 Imports & Domain Modules
```phs
use solve, deriv, integral from calc
use plot, plot3d, export3d from plot
use linspace, gradient, trapz from array
```

---

## 2. 3D WebGL Surface Visualization & Mesh Export

Physure includes native 3D physical surface rendering (WebGL 100% offline) and CAD/3D mesh export.

```phs
use plot3d, export3d from plot

fn P(x, y) = 100.0 kPa * sin(x / 1.0 m) * cos(y / 1.0 m)
rango_x = -2.0 m .. 2.0 m
rango_y = -2.0 m .. 2.0 m

# 1. Render interactive 3D WebGL viewer
plot3d(P, x: rango_x, y: rango_y, title: "Pressure Surface Distribution P(x, y)")

# 2. Export standard 3D CAD meshes
export3d(P, x: rango_x, y: rango_y, file: "mesh.stl", format: "stl")
export3d(P, x: rango_x, y: rango_y, file: "mesh.obj", format: "obj")
export3d(P, x: rango_x, y: rango_y, file: "mesh.gltf", format: "gltf")
export3d(P, x: rango_x, y: rango_y, file: "mesh.ply", format: "ply")
```

---

## 3. Built-in Function Modules

| Domain / Module | Function | Description | Example |
| :--- | :--- | :--- | :--- |
| `core` | `sqrt(x)` | Square root | `sqrt(16.0 m^2)` |
| `core` | `sin(x)`, `cos(x)`, `tan(x)` | Trigonometric functions | `sin(3.14159 / 2)` |
| `core` | `exp(x)`, `ln(x)`, `log(x)` | Exponents and logarithms | `ln(10.0)` |
| `core` | `abs(x)` | Absolute value | `abs(-5.0 m)` |
| `core` | `round(x, n)` | Round to n decimal places | `round(3.14159, 2)` |
| `calc` | `solve(eq, target)` | Symbolic equation solver | `solve(P == F / A, F)` |
| `calc` | `deriv(expr, var)` | Symbolic derivative | `deriv(0.5 * m * v^2, v)` |
| `calc` | `integral(expr, var)` | Symbolic integral | `integral(m * g, h)` |
| `calc` | `integral(expr, var, a, b)` | Definite integral (closed form confirmed numerically, or tanh-sinh quadrature with an error estimate) | `integral("v", "t", 0 s, 1 h)` |
| `array` | `linspace(a, b, n)` | Vector generation | `linspace(0.0 m, 10.0 m, 100)` |
| `array` | `gradient(y, x)` | Numerical derivative dy/dx | `gradient(presion_vec, pos_vec)` |
| `array` | `trapz(y, x)` | Numerical integration (area) | `trapz(fuerza_vec, pos_vec)` |

#### Definite integrals

`integral(expr, var, a, b)` (after `use integral from calc`) is the definite integral of `expr` over `var` from `a` to `b`; `integral(expr, var)` is the symbolic antiderivative.

- **Bounds**: a number, a quantity, `"inf"` or `"-inf"`, or a name or an expression written as text (`pi`, `"pi/2"`, `"bohr_radius"`, a variable of the program). A name that resolves to nothing is an error, never 0. A quantity bound makes `var` a quantity of that unit; the other bound must have the same dimension, or be a plain `0` or an infinity. A temperature point or a level is not a bound.
- **Names**: the integrand sees the program's variables (and a function's parameters). A free name resolves to a variable, a unit, `pi` or a constant of the catalog; any other name is an error.
- **Result**: a quantity in (unit of `expr`) times (unit of `var`) whenever a unit or an uncertainty is involved, otherwise a number. With `v = 3 m/s`, `integral("v", "t", 0 s, 1 h)` is `10800 m`.
- **Uncertainty** propagates, from the program's uncertain variables and from uncertain bounds, and the result stays correlated with them.
- **Method**: the closed form `F(b) - F(a)` when the symbolic engine has an antiderivative *and* the quadrature confirms it, so a closed form that jumps across a pole is refused (`integral("x^(-2)", "x", -1, 1)` is an error, not -2). Otherwise tanh-sinh quadrature with an error estimate: accepted when two successive levels agree within a relative 1e-12 of max(|I|, the integral of |f|), from level 4 on, with at most 20000 samples per piece. An infinite end is mapped by x = c ± (1 - u)/u.
- **Refused, not guessed**: a singularity at a non-zero end (`1/sqrt(1-x)` on [0, 1]: substitute so that it sits at 0), a jump or a kink inside the interval (split the interval there), `sin(1/x)` next to 0, an integrand that decays too slowly towards an infinite end. An integrand that cannot be evaluated, or is not finite, at a sample is an error.
- **Known failure mode**: a peak much narrower than the spacing of the first levels' samples is never sampled, and every level agrees on a wrong value: a Gaussian of width 1e-4 at x = 0.3 on [0, 1] integrates to a confident 0. Split the interval around a known peak.

---

## 4. Physical Units & Aliases Registry

| Symbol / Alias | Category | Base SI Dimensions |
| :--- | :--- | :--- |
| `%` | Derived | `dimensionless` |
| `BTU` | Derived | `kg * m^2 * s^-2` |
| `Ba` | Derived | `kg * m^-1 * s^-2` |
| `Bi` | Derived | `A` |
| `C` | Derived | `A * s` |
| `D` | Derived | `A * m * s` |
| `Eh` | Derived | `kg * m^2 * s^-2` |
| `F` | Derived | `A^2 * kg^-1 * m^-2 * s^4` |
| `Fr` | Derived | `A * s` |
| `G` | Derived | `A^-1 * kg * s^-2` |
| `Gal` | Derived | `m * s^-2` |
| `Gb` | Derived | `A` |
| `H` | Derived | `A^-2 * kg * m^2 * s^-2` |
| `Hz` | Derived | `s^-1` |
| `J` | Derived | `kg * m^2 * s^-2` |
| `L` | Derived | `m^3` |
| `Mx` | Derived | `A^-1 * kg * m^2 * s^-2` |
| `N` | Derived | `kg * m * s^-2` |
| `Oe` | Derived | `A * m^-1` |
| `Ohm` | Derived | `A^-2 * kg * m^2 * s^-3` |
| `P` | Derived | `kg * m^-1 * s^-1` |
| `Pa` | Derived | `kg * m^-1 * s^-2` |
| `S` | Derived | `A^2 * kg^-1 * m^-2 * s^3` |
| `St` | Derived | `m^2 * s^-1` |
| `T` | Derived | `A^-1 * kg * s^-2` |
| `Torr` | Derived | `kg * m^-1 * s^-2` |
| `Tp` | Derived | `K` |
| `V` | Derived | `A^-1 * kg * m^2 * s^-3` |
| `W` | Derived | `kg * m^2 * s^-3` |
| `Wb` | Derived | `A^-1 * kg * m^2 * s^-2` |
| `Wh` | Derived | `kg * m^2 * s^-2` |
| `a0` | Derived | `m` |
| `acre` | Derived | `m^2` |
| `acre_us_survey` | Derived | `m^2` |
| `arcmin` | Derived | `rad` |
| `arcsec` | Derived | `rad` |
| `atm` | Derived | `kg * m^-1 * s^-2` |
| `atomic_charge` | Derived | `A * s` |
| `bar` | Derived | `kg * m^-1 * s^-2` |
| `btu_iso` | Derived | `kg * m^2 * s^-2` |
| `btu_th` | Derived | `kg * m^2 * s^-2` |
| `cal` | Derived | `kg * m^2 * s^-2` |
| `cal_it` | Derived | `kg * m^2 * s^-2` |
| `d` | Derived | `s` |
| `dB` | Derived | `dimensionless` |
| `deg` | Derived | `rad` |
| `degR` | Derived | `K` |
| `delta_degC` | Derived | `K` |
| `delta_degF` | Derived | `K` |
| `dyn` | Derived | `kg * m * s^-2` |
| `eV` | Derived | `kg * m^2 * s^-2` |
| `erg` | Derived | `kg * m^2 * s^-2` |
| `fl_oz` | Derived | `m^3` |
| `fluid_ounce_imperial` | Derived | `m^3` |
| `ft` | Derived | `m` |
| `g` | Derived | `kg` |
| `gallon_imperial` | Derived | `m^3` |
| `gallon_us` | Derived | `m^3` |
| `h` | Derived | `s` |
| `ha` | Derived | `m^2` |
| `hp` | Derived | `kg * m^2 * s^-3` |
| `hp_electric` | Derived | `kg * m^2 * s^-3` |
| `hp_metric` | Derived | `kg * m^2 * s^-3` |
| `in` | Derived | `m` |
| `kat` | Derived | `mol * s^-1` |
| `kayser` | Derived | `m^-1` |
| `kp` | Derived | `kg * m * s^-2` |
| `lb` | Derived | `kg` |
| `lbf` | Derived | `kg * m * s^-2` |
| `lm` | Derived | `cd * sr` |
| `lp` | Derived | `m` |
| `lx` | Derived | `cd * m^-2 * sr` |
| `ly` | Derived | `m` |
| `me` | Derived | `kg` |
| `mi` | Derived | `m` |
| `min` | Derived | `s` |
| `mmHg` | Derived | `kg * m^-1 * s^-2` |
| `mp` | Derived | `kg` |
| `nmi` | Derived | `m` |
| `oz` | Derived | `kg` |
| `pH` | Derived | `dimensionless` |
| `pc` | Derived | `m` |
| `ppm` | Derived | `dimensionless` |
| `psi` | Derived | `kg * m^-1 * s^-2` |
| `qp` | Derived | `A * s` |
| `slug` | Derived | `kg` |
| `sr` | Derived | `rad` |
| `statV` | Derived | `A^-1 * kg * m^2 * s^-3` |
| `t` | Derived | `kg` |
| `tau0` | Derived | `s` |
| `tp` | Derived | `s` |
| `unity` | Derived | `dimensionless` |
| `yd` | Derived | `m` |
| `year_gregorian` | Derived | `s` |
| `yr` | Derived | `s` |
| `°C` | Derived | `K` |
| `°F` | Derived | `K` |
| `Å` | Derived | `m` |
| `Ωm` | Derived | `A^-2 * kg * m^3 * s^-3` |
| `1` | Alias -> `unity` | - |
| `Gauss` | Alias -> `G` | - |
| `Kayser` | Alias -> `kayser` | - |
| `Oersted` | Alias -> `Oe` | - |
| `abampere` | Alias -> `Bi` | - |
| `acre` | Alias -> `acre` | - |
| `acre_international` | Alias -> `acre` | - |
| `acre_us_survey` | Alias -> `acre_us_survey` | - |
| `acres` | Alias -> `acre` | - |
| `ampere` | Alias -> `A` | - |
| `amperio` | Alias -> `A` | - |
| `amperios` | Alias -> `A` | - |
| `angstrom` | Alias -> `Å` | - |
| `arcminute` | Alias -> `arcmin` | - |
| `arcsecond` | Alias -> `arcsec` | - |
| `atmosphere` | Alias -> `atm` | - |
| `atmospheres` | Alias -> `atm` | - |
| `atomic_charge` | Alias -> `atomic_charge` | - |
| `atomic_time` | Alias -> `tau0` | - |
| `bar` | Alias -> `bar` | - |
| `bars` | Alias -> `bar` | - |
| `barye` | Alias -> `Ba` | - |
| `biot` | Alias -> `Bi` | - |
| `bohr` | Alias -> `a0` | - |
| `bohr_radius` | Alias -> `a0` | - |
| `btu` | Alias -> `BTU` | - |
| `btu_iso` | Alias -> `btu_iso` | - |
| `btu_it` | Alias -> `BTU` | - |
| `btu_th` | Alias -> `btu_th` | - |
| `cal_th` | Alias -> `cal` | - |
| `calorie` | Alias -> `cal` | - |
| `calorie_it` | Alias -> `cal_it` | - |
| `calorie_th` | Alias -> `cal` | - |
| `calories` | Alias -> `cal` | - |
| `candela` | Alias -> `cd` | - |
| `candelas` | Alias -> `cd` | - |
| `celsius` | Alias -> `°C` | - |
| `centigrade` | Alias -> `°C` | - |
| `coulomb` | Alias -> `C` | - |
| `coulombs` | Alias -> `C` | - |
| `day` | Alias -> `d` | - |
| `days` | Alias -> `d` | - |
| `debye` | Alias -> `D` | - |
| `decibel` | Alias -> `dB` | - |
| `degC` | Alias -> `°C` | - |
| `degC_delta` | Alias -> `delta_degC` | - |
| `degF` | Alias -> `°F` | - |
| `degF_delta` | Alias -> `delta_degF` | - |
| `degree` | Alias -> `deg` | - |
| `degrees` | Alias -> `deg` | - |
| `delta_celsius` | Alias -> `delta_degC` | - |
| `delta_fahrenheit` | Alias -> `delta_degF` | - |
| `dyne` | Alias -> `dyn` | - |
| `electron_mass` | Alias -> `me` | - |
| `electron_volt` | Alias -> `eV` | - |
| `erg` | Alias -> `erg` | - |
| `fahrenheit` | Alias -> `°F` | - |
| `farad` | Alias -> `F` | - |
| `farads` | Alias -> `F` | - |
| `feet` | Alias -> `ft` | - |
| `fl_oz_imp` | Alias -> `fluid_ounce_imperial` | - |
| `fl_oz_uk` | Alias -> `fluid_ounce_imperial` | - |
| `fl_oz_us` | Alias -> `fl_oz` | - |
| `fluid_ounce` | Alias -> `fl_oz` | - |
| `fluid_ounce_imperial` | Alias -> `fluid_ounce_imperial` | - |
| `foot` | Alias -> `ft` | - |
| `franklin` | Alias -> `Fr` | - |
| `gal` | Alias -> `gallon_us` | - |
| `gal_imp` | Alias -> `gallon_imperial` | - |
| `gal_uk` | Alias -> `gallon_imperial` | - |
| `gal_us` | Alias -> `gallon_us` | - |
| `galileo` | Alias -> `Gal` | - |
| `gallon` | Alias -> `gallon_us` | - |
| `gallon_imperial` | Alias -> `gallon_imperial` | - |
| `gallon_us` | Alias -> `gallon_us` | - |
| `gallons` | Alias -> `gallon_us` | - |
| `gauss` | Alias -> `G` | - |
| `gilbert` | Alias -> `Gb` | - |
| `gram` | Alias -> `g` | - |
| `grams` | Alias -> `g` | - |
| `hartree` | Alias -> `Eh` | - |
| `hectare` | Alias -> `ha` | - |
| `hectares` | Alias -> `ha` | - |
| `henries` | Alias -> `H` | - |
| `henry` | Alias -> `H` | - |
| `hertz` | Alias -> `Hz` | - |
| `horsepower` | Alias -> `hp` | - |
| `horsepower_electric` | Alias -> `hp_electric` | - |
| `horsepower_mechanical` | Alias -> `hp` | - |
| `horsepower_metric` | Alias -> `hp_metric` | - |
| `hour` | Alias -> `h` | - |
| `hours` | Alias -> `h` | - |
| `hp_mechanical` | Alias -> `hp` | - |
| `hr` | Alias -> `h` | - |
| `inch` | Alias -> `in` | - |
| `joule` | Alias -> `J` | - |
| `joules` | Alias -> `J` | - |
| `julio` | Alias -> `J` | - |
| `julios` | Alias -> `J` | - |
| `katal` | Alias -> `kat` | - |
| `kayser` | Alias -> `kayser` | - |
| `kelvin` | Alias -> `K` | - |
| `kelvins` | Alias -> `K` | - |
| `kgs` | Alias -> `kg` | - |
| `kilogram` | Alias -> `kg` | - |
| `kilogramo` | Alias -> `kg` | - |
| `kilopond` | Alias -> `kp` | - |
| `l` | Alias -> `L` | - |
| `lbs` | Alias -> `lb` | - |
| `light_year` | Alias -> `ly` | - |
| `liter` | Alias -> `L` | - |
| `liters` | Alias -> `L` | - |
| `lumen` | Alias -> `lm` | - |
| `lumens` | Alias -> `lm` | - |
| `lux` | Alias -> `lx` | - |
| `maxwell` | Alias -> `Mx` | - |
| `meter` | Alias -> `m` | - |
| `metric_ton` | Alias -> `t` | - |
| `metro` | Alias -> `m` | - |
| `metros` | Alias -> `m` | - |
| `mho` | Alias -> `S` | - |
| `mile` | Alias -> `mi` | - |
| `miles` | Alias -> `mi` | - |
| `minute` | Alias -> `min` | - |
| `minutes` | Alias -> `min` | - |
| `mmHg` | Alias -> `mmHg` | - |
| `mole` | Alias -> `mol` | - |
| `moles` | Alias -> `mol` | - |
| `nautical_mile` | Alias -> `nmi` | - |
| `newton` | Alias -> `N` | - |
| `newtons` | Alias -> `N` | - |
| `oersted` | Alias -> `Oe` | - |
| `ohm` | Alias -> `Ohm` | - |
| `ohm_meter` | Alias -> `Ωm` | - |
| `ohmio` | Alias -> `Ohm` | - |
| `ohmios` | Alias -> `Ohm` | - |
| `ohms` | Alias -> `Ohm` | - |
| `ounce` | Alias -> `oz` | - |
| `ounces` | Alias -> `oz` | - |
| `pH` | Alias -> `pH` | - |
| `parsec` | Alias -> `pc` | - |
| `pascal` | Alias -> `Pa` | - |
| `pascalio` | Alias -> `Pa` | - |
| `pascalios` | Alias -> `Pa` | - |
| `pascals` | Alias -> `Pa` | - |
| `pct` | Alias -> `%` | - |
| `percent` | Alias -> `%` | - |
| `planck_charge` | Alias -> `qp` | - |
| `planck_length` | Alias -> `lp` | - |
| `planck_mass` | Alias -> `mp` | - |
| `planck_temperature` | Alias -> `Tp` | - |
| `planck_time` | Alias -> `tp` | - |
| `poise` | Alias -> `P` | - |
| `porcentaje` | Alias -> `%` | - |
| `pound` | Alias -> `lb` | - |
| `pound_force` | Alias -> `lbf` | - |
| `pounds` | Alias -> `lb` | - |
| `ppm` | Alias -> `ppm` | - |
| `psi` | Alias -> `psi` | - |
| `radian` | Alias -> `rad` | - |
| `radians` | Alias -> `rad` | - |
| `rankine` | Alias -> `degR` | - |
| `sec` | Alias -> `s` | - |
| `second` | Alias -> `s` | - |
| `segundo` | Alias -> `s` | - |
| `segundos` | Alias -> `s` | - |
| `siemens` | Alias -> `S` | - |
| `slug` | Alias -> `slug` | - |
| `statC` | Alias -> `Fr` | - |
| `statcoulomb` | Alias -> `Fr` | - |
| `statvolt` | Alias -> `statV` | - |
| `steradian` | Alias -> `sr` | - |
| `steradians` | Alias -> `sr` | - |
| `stokes` | Alias -> `St` | - |
| `tesla` | Alias -> `T` | - |
| `teslas` | Alias -> `T` | - |
| `tonne` | Alias -> `t` | - |
| `torr` | Alias -> `Torr` | - |
| `unity` | Alias -> `unity` | - |
| `vatio` | Alias -> `W` | - |
| `vatios` | Alias -> `W` | - |
| `volt` | Alias -> `V` | - |
| `voltio` | Alias -> `V` | - |
| `voltios` | Alias -> `V` | - |
| `volts` | Alias -> `V` | - |
| `watt` | Alias -> `W` | - |
| `watt_hour` | Alias -> `Wh` | - |
| `watts` | Alias -> `W` | - |
| `weber` | Alias -> `Wb` | - |
| `webers` | Alias -> `Wb` | - |
| `yard` | Alias -> `yd` | - |
| `year` | Alias -> `yr` | - |
| `year_gregorian` | Alias -> `year_gregorian` | - |
| `year_julian` | Alias -> `yr` | - |
| `years` | Alias -> `yr` | - |
| `yr_gregorian` | Alias -> `year_gregorian` | - |
| `yr_julian` | Alias -> `yr` | - |
| `°` | Alias -> `deg` | - |
| `°R` | Alias -> `degR` | - |
| `Ω` | Alias -> `Ohm` | - |

---

## 5. Greek Letters & Mathematical Symbols

| Symbol | LaTeX / Name Aliases | Description |
| :--- | :--- | :--- |
| `Δ` | `delta`, `Delta`, `\delta` | Difference / Variation / Change |
| `σ` | `sigma`, `\sigma` | Standard deviation / Uncertainty / Stress |
| `Ω` | `omega`, `Omega`, `\Omega` | Electric resistance (Ohm) |
| `π` | `pi`, `\pi` | Circle constant (3.14159...) |
| `θ` | `theta`, `\theta` | Angle / Temperature |
| `λ` | `lambda`, `\lambda` | Wavelength |
| `μ` | `mu`, `micro`, `\mu` | Micro prefix / Friction / Permeability |
| `α` | `alpha`, `\alpha` | Thermal expansion / Alpha coefficient |
| `β` | `beta`, `\beta` | Beta coefficient / Ratio |
| `γ` | `gamma`, `\gamma` | Heat capacity ratio |
| `ε` | `epsilon`, `\epsilon` | Permittivity / Strain |
| `η` | `eta`, `\eta` | Efficiency |
| `ρ` | `rho`, `\rho` | Density / Electrical resistivity |
| `τ` | `tau`, `\tau` | Torque / Time constant |
| `ϕ` | `phi`, `\phi` | Magnetic flux / Phase |
| `ψ` | `psi`, `\psi` | Wavefunction |
| `ω` | `omega`, `\omega` | Lowercase Angular frequency |
| `∞` | `infinity`, `\infty` | Infinity symbol |
| `±` | `+/-`, `\pm` | Plus-minus uncertainty |

---

## 6. Transpilation & Integration Targets

Physure scripts (`.phs`) can be transpiled natively into high-performance target code:

```bash
# Transpile to Python with NumPy & SciPy
phs transpile script.phs --target python --output script.py

# Transpile to Rust
phs transpile script.phs --target rust --output main.rs

# Transpile to Java
phs transpile script.phs --target java --output Main.java

# Transpile to TypeScript
phs transpile script.phs --target ts --output script.ts

# Transpile to JavaScript
phs transpile script.phs --target js --output script.js
```
