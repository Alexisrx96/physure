# 📐 Physure Language & Syntax Reference Specification

Physure (PHS) is a high-performance domain-specific programming language and computation engine for physical quantities, dimensional analysis, uncertainty propagation, symbolic calculus, and 3D WebGL scientific visualization.

---

## 1. Syntax & Language Constructs

### 1.1 Physical Quantities & Unit Conversions
Quantities consist of a numerical magnitude, an optional uncertainty, and a physical unit expression.

```phs
# Quantity literals with SI or derived units
presion = 100.0 kPa
velocidad = 25.0 m / s
resistencia = 50.0 Ohm
medicion = 10.0 +/- 0.5 m        # Measurement with 0.5 m standard deviation
porcentaje = 5.0 %               # Relative uncertainty or percentage

# Unit Conversion Operator '=>'
p_bar = 100.0 kPa => bar         # Converts 100.0 kPa to bar
v_kmh = 25.0 m / s => km / h     # Converts 25 m/s to km/h
```

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
let x = 10.0 m in x * 2.0
if P > 50.0 kPa then "Alta Presion" else "Presion Normal"
```

### 1.3.1 Strings & Interpolation
Un string entrecomillado es exactamente el texto escrito: nunca se resuelve como variable,
aunque exista una con ese nombre. Para meter un valor se usa `{...}`, y ahi si se evalua la
expresion en el entorno actual.

```phs
v = 3.0 m/s
"v"                             # -> "v", el nombre, no la cantidad
deriv("0.5 * m * v^2", "v")     # -> m * v   (v y m siguen siendo simbolos)

m = 2.0 kg
"0.5 * {m} * v^2"               # -> "0.5 * 2.0 kg * v^2"
deriv("0.5 * {m} * v^2", "v")   # -> 2 * kg * v   (m ya es constante, con unidad)
```

Asi una constante se congela en la formula sin alterarla en silencio, y la unidad viaja con
ella hasta el resultado simbolico. La interpolacion se transpila a los tres objetivos:
concatenacion en Python y Java, `format!` en Rust.

### 1.4 Imports & Domain Modules
```phs
use solve, deriv, integral from calc
use plot, plot3d, export3d from plot
use linspace, gradient, trapz from array

# Un modulo .phs local se importa por ruta, relativa al script que se ejecuta:
# import "./thermodynamics.phs"
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
| `array` | `linspace(a, b, n)` | Vector generation | `linspace(0.0 m, 10.0 m, 100)` |
| `array` | `gradient(y, x)` | Numerical derivative dy/dx | `gradient(presion_vec, pos_vec)` |
| `array` | `trapz(y, x)` | Numerical integration (area) | `trapz(fuerza_vec, pos_vec)` |

---

## 4. Physical Units & Aliases Registry

| Symbol / Alias | Category | Base SI Dimensions |
| :--- | :--- | :--- |
| `1` | Derived | `unity` |
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
| `arcmin` | Derived | `sr` |
| `arcsec` | Derived | `sr` |
| `atm` | Derived | `kg * m^-1 * s^-2` |
| `atomic_charge` | Derived | `A * s` |
| `bar` | Derived | `kg * m^-1 * s^-2` |
| `cal` | Derived | `kg * m^2 * s^-2` |
| `d` | Derived | `s` |
| `deg` | Derived | `sr` |
| `dyn` | Derived | `kg * m * s^-2` |
| `eV` | Derived | `kg * m^2 * s^-2` |
| `erg` | Derived | `kg * m^2 * s^-2` |
| `fl_oz` | Derived | `m^3` |
| `ft` | Derived | `m` |
| `g` | Derived | `kg` |
| `gal` | Derived | `m^3` |
| `h` | Derived | `s` |
| `ha` | Derived | `m^2` |
| `hp` | Derived | `kg * m^2 * s^-3` |
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
| `pc` | Derived | `m` |
| `psi` | Derived | `kg * m^-1 * s^-2` |
| `qp` | Derived | `A * s` |
| `rad` | Derived | `sr` |
| `slug` | Derived | `kg` |
| `statV` | Derived | `A^-1 * kg * m^2 * s^-3` |
| `t` | Derived | `kg` |
| `tau0` | Derived | `s` |
| `tp` | Derived | `s` |
| `yd` | Derived | `m` |
| `yr` | Derived | `s` |
| `Å` | Derived | `m` |
| `Ωm` | Derived | `A^-2 * kg * m^3 * s^-3` |
| `"] #"` | Alias -> `arcsec` | - |
| `'] #'` | Alias -> `arcmin` | - |
| `Gauss` | Alias -> `G` | - |
| `Kayser` | Alias -> `kayser` | - |
| `Oersted` | Alias -> `Oe` | - |
| `abampere` | Alias -> `Bi` | - |
| `acre` | Alias -> `acre` | - |
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
| `calorie` | Alias -> `cal` | - |
| `calories` | Alias -> `cal` | - |
| `candela` | Alias -> `cd` | - |
| `candelas` | Alias -> `cd` | - |
| `coulomb` | Alias -> `C` | - |
| `coulombs` | Alias -> `C` | - |
| `day` | Alias -> `d` | - |
| `days` | Alias -> `d` | - |
| `debye` | Alias -> `D` | - |
| `degree` | Alias -> `deg` | - |
| `degrees` | Alias -> `deg` | - |
| `dyne` | Alias -> `dyn` | - |
| `electron_mass` | Alias -> `me` | - |
| `electron_volt` | Alias -> `eV` | - |
| `erg` | Alias -> `erg` | - |
| `farad` | Alias -> `F` | - |
| `farads` | Alias -> `F` | - |
| `feet` | Alias -> `ft` | - |
| `fluid_ounce` | Alias -> `fl_oz` | - |
| `foot` | Alias -> `ft` | - |
| `franklin` | Alias -> `Fr` | - |
| `galileo` | Alias -> `Gal` | - |
| `gallon` | Alias -> `gal` | - |
| `gallon_us` | Alias -> `gal` | - |
| `gallons` | Alias -> `gal` | - |
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
| `hour` | Alias -> `h` | - |
| `hours` | Alias -> `h` | - |
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
| `parsec` | Alias -> `pc` | - |
| `pascal` | Alias -> `Pa` | - |
| `pascalio` | Alias -> `Pa` | - |
| `pascalios` | Alias -> `Pa` | - |
| `pascals` | Alias -> `Pa` | - |
| `planck_charge` | Alias -> `qp` | - |
| `planck_length` | Alias -> `lp` | - |
| `planck_mass` | Alias -> `mp` | - |
| `planck_temperature` | Alias -> `Tp` | - |
| `planck_time` | Alias -> `tp` | - |
| `poise` | Alias -> `P` | - |
| `pound` | Alias -> `lb` | - |
| `pound_force` | Alias -> `lbf` | - |
| `pounds` | Alias -> `lb` | - |
| `psi` | Alias -> `psi` | - |
| `radian` | Alias -> `rad` | - |
| `radians` | Alias -> `rad` | - |
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
| `unity` | Alias -> `1` | - |
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
| `years` | Alias -> `yr` | - |
| `°` | Alias -> `deg` | - |
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
```
