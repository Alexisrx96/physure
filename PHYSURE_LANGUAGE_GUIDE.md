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

Different dimensions, and operand types that have no comparison (two vectors, a string and a quantity), are errors, never `False`. Arithmetic and conversion bind tighter than a comparison, so `0.1 m + 0.2 m == 0.3 m` is true and `x => km < 5 km` converts, then compares (see the operator table below).

**A zero needs a unit.** A plain number has no dimension, so it compares only with a dimensionless value: `x > 0` with `x` in metres is an error, as `x > 5` is. Zeros of different scales are different values (0 degC is 273.15 K, 0 degF is 255.37 K, 0 dB is a ratio of 1), and only the unit says which one is meant. `==`, `!=`, `<`, `<=`, `>`, `>=`, `≈`, `approx`, `assert(a, b)`, `phs check` and transpiled code all follow the same rule, and its error texts say what to write:

| Comparison | Answer | Write instead |
| :--- | :--- | :--- |
| `x > 0` with `x = 2 m` | error: "cannot compare 2.0 m with the plain number 0: a zero needs a unit, write `x > 0 m` (for a temperature point or a level, each scale's zero is a different value: 0 degC is 273.15 K, 0 dB is a ratio of 1)" | `x > 0 m` |
| `x > 5` with `x = 2 m` | error: "... a number compared with a quantity needs a unit, write `x > 5 m`" | `x > 5 m` |
| `t > 0` with `t = 20 degC` | error: the zero message | `t > 0 degC` (or `t > 273.15 K`, the same point) |
| `30 deg > 0`, `1 rad > 0.5` | error: an angle is a dimension in a comparison, in either angle mode | `30 deg > 0 deg`, `1 rad > 0.5 rad` |
| `20 dB == 100` | error: "cannot compare 20.0 dB with the plain number 100: write the level with its unit (`x > 20 dB`) or compare the ratio explicitly (`(x => unity) > 100`)": whether 100 is a ratio, a power ratio or 100 dB is written nowhere | `20 dB > 10 dB`; `(20 dB => unity) == 100` is true |
| `5 dB > 0`, `0 dB == 0` | error: the zero message | `5 dB > 0 dB` |
| `5 delta_degC < 20 degC`, `5 delta_degC == 5 degC` | error: "cannot compare a temperature point (`20.0 degC`) with a temperature interval (`5.0 delta_degC`): ...": an interval (`delta_degC`, `delta_degF`, any unit the catalog flags `interval`) is how far apart two points (`degC`, `degF`) are, not where one is; `max`, `min`, a vector and `linspace` refuse the pair too. An interval times a ratio (`2`, `50 %`), an interval plus an interval or a `K` (either order), and a point minus a point (printed in `K`) are intervals: `(20 degC - 10 degC) < 20 degC` and `(1 K + 5 delta_degC) < 20 degC` are refused too; a step through another dimension (`x / 1 m * 1 m`) loses the kind. `K` and `degR` stand for either: `20 degC > 10 K` and `(30 degC - 20 degC) > 5 delta_degC` are true | `T2 - T1 > 5 delta_degC`: compare a difference of two points with the interval. `x => K` does not make an interval a point: it reads its number as kelvin above absolute zero (`5 delta_degC => K` is 5 K, -268.15 degC) |
| `20 degC => delta_degC`, `5 delta_degC => degC` | error: "cannot convert a temperature point (`20.0 degC`) to a temperature interval (`delta_degC`): a point says where, an interval says how far ..." (and the reverse): it read the point's absolute value (293.15 delta_degC) or the interval as kelvin above absolute zero (-268.15 degC). `10 K => delta_degC` and `5 delta_degC => K` convert (`K` stands for either) | a difference of two points, `(20 degC - 0 degC) => delta_degC` (20 delta_degC); a point moved by an interval, `T1 + 5 delta_degC` |
| `50 % == 0.5`, `1 % > 0`, `3 ppm == 3e-6`, `(2 m / 1 m) > 1` | true: a dimensionless ratio in any unit (not a level) compares with plain numbers by its ratio | |

A test against zero with a tolerance writes the tolerance in the unit of `x`: `abs(x) < 1e-9 m`.

#### Conditions

A condition is a Bool: `True`, `False`, a comparison, or `and`, `or`, `not` of those. This holds wherever PHS reads one: `if c then a else b`, `c ? a : b`, `while c { ... }`, `if c then return v`, `@requires(c, "...")`, `@ensures(c, "...")`, a conditional breakpoint of `phs debug`, the operands of `and`, `or` and `not`, and `assert(c)`. Anything else is an error that names what it got, after "the condition of `if` needs a Bool (True or False), got" (the place changes: `while`, `? :`, `@requires`, "the left side of `and`"...):

| The condition is | The error says | Write instead |
| :--- | :--- | :--- |
| a number, `if n` with `n = 3` | a Number (3.0): a number is not a condition; compare it, e.g. `x != 0` | `if n != 0` |
| a quantity, `if x` with `x = 2 m` | a Quantity (2.0 m): a quantity is not a condition; compare it, e.g. `x != 0 m` | `if x != 0 m` |
| the text `"true"` or `"false"` | a String ("true"): text is not a condition; the Bool literals are True and False | `if True` |
| other text, `if s` with `s = "on"` | a String ("on"): text is not a condition; compare it, e.g. `s == "on"` | `if s == "on"` |
| a name with no value, `while true` | `true` has no value; the Bool literals are True and False | `while True` |
| a vector, a range, a function, None, a sigma bound, an equation, a matrix, a plot | a Vector: a Vector is not a condition (and so on) | the comparison that is meant |

A number is not a condition because "zero is false" is a convention the reader cannot see, and the zero of a quantity depends on its unit (above). `phs check` reports the same error without running the program wherever it knows the type of the condition (a literal, a variable assigned one, a parameter with a unit, a name with no value); where it does not, the error comes when the line runs.

#### Operator precedence

Tightest first; the operators of one row bind equally and, where they repeat, from left to right.

| Operators | Reading |
| :--- | :--- |
| `^`, unary `-` | `-2 ^ 2` is `-(2 ^ 2)`, -4; a power of a power needs parentheses: `(2 ^ 3) ^ 2` or `2 ^ (3 ^ 2)` |
| `*`, `/`, implicit multiplication | `6 m / 2 * 3` is 9 m; `2 3 m` is 6 m |
| `+`, `-` | `a - b + c` is `(a - b) + c` |
| `=>` | converts everything to its left: `1 m + 2 m => cm` is 300 cm; the target is a unit |
| `==`, `!=`, `<`, `<=`, `>`, `>=`, `≈` | one per expression: `0.1 + 0.2 == 0.3` is true |
| `..` | one per expression; each endpoint is a whole expression: `0 m .. 10 m => km` is `0 m .. 0.01 km` |
| `: spec` | formats the whole expression: `0.1 + 0.2: .3f` |
| `not` | `not a + b > c` is `not ((a + b) > c)` |
| `and` | |
| `or` | |
| `c ? a : b` | |
| `where` | |

Two forms are errors that say which parentheses to write. A `+` or `-` after `=> unit`: `x => km - 5 m` cannot subtract from the unit, and the standard order does not say `(x => km) - 5 m`, so write that. A chain of comparisons: `a < b < c` would compare the `True` or `False` of `a < b` with `c`, so write `a < b and b < c`. The strings the symbolic builtins read (`deriv`, `integral`, `limit`, ...) follow the same order: `deriv("-x^2", "x")` is `-2 * x`, and `x^2^3` needs parentheses there too; in them an exponent may carry its own sign (`e^-x`) and `**` is `^`.

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
pressure = 75.0 kPa
doubled = x * 2.0 where x = 10.0 m
status = if pressure > 50.0 kPa then "High Pressure" else "Normal Pressure"
```

A `while` loop stops with the error "while loop did not converge after N iterations" after `[Settings] max_while_iterations` iterations (default 10000, at least 1), as `max_loop_iterations` (default 1000000) bounds the range or vector a `for` loop may build (section 1.6).

### 1.4 Imports & Domain Modules
```phs
use solve, deriv, integral from calc
use plot, plot3d, export3d from plot
use linspace, gradient, trapz from array
```

`use a, b from "lib"` (also `use * from "lib"` and `use a as b from "lib"`) imports from the module file `lib.phs`. A module is a program of its own:

- It runs once per interpreter (again only when its file changed), whole and in definition order, in an environment of its own: the importer's names never reach it. With `lib.phs` holding `k = 2`, `r = 3*k` and `f(x) = k*x`, the program `k = 10; use r, f from "lib"` gets `r` = 6 and `f(1)` = 2.
- An imported value is the value the module computed, so a measurement imported twice is one measurement. An imported function sees the names of its module and its parameters; a function you pass to it keeps seeing yours, and a function it returns keeps its module.
- Errors name the module: a name it does not define, a cyclic import (`a -> b -> a`), a file that cannot be parsed, and a module that binds a reserved name (section 1.5): a module with `pi = 3` makes every import from it an error, whichever name is imported.

### 1.5 Names: variables, `pi` and the constants of the catalog
A bare name resolves in one order, which the interpreter, the text of `integral` and `limit`, and `phs check` follow: a parameter, a `where` binding or a `for` variable, then a variable or a function of the program; then `pi` (or `π`), the number π; then a constant of the catalog in force, a quantity in the unit the catalog gives it; then a unit (`J` alone is one joule). Any other name is an error when its line runs: `` `foo` has no value `` (with the likely fix when there is one: a variable two edits away, a unit one edit away, a function of PHS written as a value). `phs check` reports it before the run only where the name meets a quantity (`5 foo`, `x + foo`).

```phs
area = pi * (2 m)^2                                   # 12.566 m^2
rt = molar_gas_constant * 300 K => J/mol              # 2494.3 J/mol
photon = planck_constant * speed_of_light_in_vacuum / (500 nm) => eV   # 2.48 eV
```

The constants are the `[Constants]` of the catalog in force, by their catalog names: the embedded catalog (411 CODATA values, with no uncertainties), a project's `physure.conf`, a frozen file, or the registry a binding evaluates for. An override or a frozen configuration changes them. Only the catalog's own names are constants: `e`, `c`, `k` and `R` are not (write `exp(1)`, `speed_of_light_in_vacuum`, `boltzmann_constant`, `molar_gas_constant`), and `h`, `g`, `G` are units (hour, gram, gauss). Seven names are a unit and a constant at once (`bohr_radius`, `electron_mass`, `electron_volt`, `planck_length`, `planck_mass`, `planck_temperature`, `planck_time`): the catalog gives both one value (a frozen file or a unit system where they differ is refused; a `physure.conf` where they differ loses the constant, with a message), so `2 bohr_radius` (the unit) and `2 * bohr_radius` (the constant) are the same length. A constant carries no uncertainty: a catalog line is `name = number unit`, and one written with `+/-` is an error when it is used.

Binding one of these names is an error, whatever binds it: `pi = 3`, `f(pi) = ...`, `for pi in ...`, `... where pi = 3`, a function called `pi`, an import of a module that defines it, a value a host passes under that name. The catalog in force decides which names those are, so a `physure.conf` or a frozen file that adds a `[Constants]` entry makes a script that binds that name an error under that configuration only. `phs transpile` cannot read a catalog when the generated code runs: it writes each constant's value at generation, listed in a comment at the top of the file. Section 1.7 lists the other names that can never be bound.

### 1.6 Settings that change a run
The `[Settings]` of the frozen file in force (`phs conf`), or else of the `physure.conf` in the working directory, govern these. A key that is not set takes its default, which is the behaviour from before the key existed (but for the seed, below).

| Key | Default | What it governs |
| :--- | :--- | :--- |
| `max_while_iterations` | `10000` | the ceiling of one `while` loop (at least 1) |
| `monte_carlo_samples` | `1000` | the samples a Monte Carlo measurement draws (2 to 10000000) |
| `monte_carlo_seed` | `0` | the seed of the Monte Carlo draws, or `random` |
| `default_dof` | `inf` | the degrees of freedom of an uncertainty that declares none (`dof`, `coverage_factor`, `expanded`) |
| `display_significant_digits` | `15` | the significant digits of a printed decimal number (1 to 17; 17 prints every number exactly); not the scientific form |
| `display_scientific_below` | `1e-4` | below this magnitude a number prints in scientific notation (1e-6 to 1) |
| `display_scientific_from` | `1e16` | from this magnitude a number prints in scientific notation (finite, at least 1) |
| `uncertainty_digits` | `gum` | the significant figures of a printed uncertainty: `gum` (the GUM rule) or a count from 1 to 17; the value is rounded to the same decimal place |

**Monte Carlo draws are reproducible by default.** Under `propagation_mode = monte_carlo` each unit of evaluation draws one stream from the seed: a program run (`phs file.phs` and its viewers), a REPL session, a request of the editor daemon, one start of `phs serve`, a document of the language server. So the same program gives the same samples on every run, and two measurements of one run are independent. `monte_carlo_seed = random` draws operating-system entropy, different on every run (`phs conf status` warns that such a file is not reproducible). `phs eval --seed N` wins over the setting, and the JSON says which seed was used and where it came from. A `for` loop or `parallel_map` in `monte_carlo` mode runs on one thread, so its values never depend on the number of threads.

**Printed text.** `@precision` wins over the display keys, and `x: base` follows the digits and thresholds without GUM rounding. The keys do not govern `format()` with a spec, `frac` and `ifrac`, plot axes or the symbolic printer. With `display_significant_digits = 6` and `uncertainty_digits = 3`, `x = 1/3` prints `0.333333` (by default `0.333333333333333`) and `10.0 +/- 0.15 m` prints `10.000 ± 0.150 m` (by default `10.0 ± 0.2 m`).

Six old keys are accepted and do nothing: `default_output`, `auto_simplify`, `default_system`, `readable_representation`, `verbose`, `mkml_recursion_limit`. `phs conf status` warns where a file of yours sets one.

### 1.7 Reserved names
A program can never bind a reserved name: not as a variable, a parameter, a function name, a `for` variable, a `where` binding or an import alias. The interpreter refuses it just before the statement runs, and `phs check` reports it before the program runs, with the code `reserved-name`; the message says what the name is and what to do:

| Class | Names |
| :--- | :--- |
| Keywords | `for`, `while`, `let`, `if`, `then`, `else`, `where`, `return`, `not`, `and`, `or`, `True`, `False` |
| Statement words | `use`, `from`, `import`, `export`, `as`, `fn`, `in` |
| Literals | `true`, `false`, `None`, `none`, `inf`, `infinity`, `oo` |
| Functions of the core | `abs`, `approx`, `assert`, `ceil`, `cos`, `cot`, `coverage_factor`, `csc`, `dof`, `erf`, `exact_assert`, `exp`, `expanded`, `floor`, `format`, `from_samples`, `if_then_else`, `ln`, `log`, `max`, `min`, `parallel_map`, `round`, `sec`, `sigma`, `sin`, `sqrt`, `tan`, `ternary`, `uncertainty`, `vector` |
| Functions of `calc` | `curl`, `deriv`, `diff`, `div`, `divergence`, `dsolve`, `expand`, `grad`, `gradient`, `integral`, `integrate`, `inv_laplace`, `inverse_laplace`, `laplace`, `laplacian`, `lim`, `limit`, `series`, `simplify`, `solve`, `sub`, `substitute`, `sym_charpoly`, `sym_det`, `sym_eigenvalues`, `sym_trace`, `sym_transpose`, `taylor` |
| Functions of `array` | `cross`, `det`, `dot`, `gradient`, `linspace`, `matmul`, `norm`, `sym_charpoly`, `sym_det`, `sym_eigenvalues`, `sym_trace`, `sym_transpose`, `transpose`, `trapz`, `unit_vector` |
| Functions of `plot` | `export3d`, `export_3d`, `plot`, `plot3d`, `plot_field`, `plot_nd` |
| Constants | `pi`, `π` and every constant of the catalog (section 1.5) |

```phs
max = 3          # error: `max` is a function of PHS and cannot be bound as a variable; choose another name
use = 3          # error: `use` is a word of the language and cannot be bound as a variable; choose another name
```

* The functions of a module are reserved from the `use` that imports them: `use * from calc` reserves all of `calc`, `use solve from calc` reserves `solve`, and `use solve as sv from calc` reserves `sv` and leaves `solve` free. Importing a function over a name the program already holds, or that a host or a plugin registered, is an error too.
* Not reserved: unit names (`m`, `s`, `kg`), `e`, the words of a format (`base`, `frac`).
* A value a host gives a program, and the arguments of an equation call (`eq(sigma=5, A=2)`), are local to the run and never hide a call, so they may be named like a function (`sigma`, `max`, `dof`); a keyword, a literal, `pi` and the constants are still refused there.
* A host function or a plugin function cannot be installed under a reserved name: a call would reach the builtin first.
* In a symbolic text (`deriv`, `integral`, `solve`, ...), the keywords, the literals and the functions the symbolic parser knows (`sin`, `log`, `deriv`, `integral`, ...) are not symbols; `sigma`, `max`, `min`, `dof` and `sec` are.

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
| `core` | `sin(x)`, `cos(x)`, `tan(x)`, `sec(x)`, `csc(x)`, `cot(x)`, `erf(x)` | Trigonometric functions (of an angle or a plain number); `erf`, the error function (of a plain number) | `sin(3.14159 / 2)`, `sec(60 deg)` |
| `core` | `exp(x)`, `ln(x)`, `log(x)` | Exponents and logarithms | `ln(10.0)` |
| `core` | `abs(x)` | Absolute value | `abs(-5.0 m)` |
| `core` | `round(x, n)` | Round to n decimal places, ties away from zero; `n` is a whole, plain number, negative to round to tens (`round(1.5 m, 1 s)` and `round(x, 2.7)` are errors). The uncertainty and its degrees of freedom are kept; a place finer than the value has digits for leaves it as it is (`round(1.5 m, 400)` is `1.5 m`), one above every digit gives 0 | `round(3.14159, 2)` |
| `core` | `min(a, b, ...)`, `max(a, b, ...)` | The smallest or largest of values of one dimension, in the unit it was written in; one vector argument is the extremum of its elements. Values of different dimensions are an error (`max(1 m, 1 s)`, `min(1 m, 2)`, `max(1 rad, 1)`, a level and a plain number `max(20 dB, 2)`): compare values of one dimension | `max(1 m, 30 cm)`, `min(vector(3 s, 2 s))` |
| `core` | `vector(a, b, ...)`, `[a, b, ...]` | A vector. Its numbers and quantities share one dimension, in any of its units; `vector(1 m, 2 s)` is an error, and so is any vector that would mix dimensions, whatever builds it (an elementwise operation such as `2 m ^ vector(1, 2)`, a `for`, `parallel_map`, `transpose`, a host function): put each dimension in its own vector. The rows of a matrix may differ from one another (`[[1 m, 2 m], [1 s, 2 s]]`), not within a row | `[1 m, 2 km]` |
| `calc` | `solve(eq, target)` | Symbolic equation solver | `solve("P = F / A", "F")` |
| `calc` | `deriv(expr, var)` | Symbolic derivative | `deriv("0.5 * m * v^2", "v")` |
| `calc` | `integral(expr, var)` | Symbolic integral | `integral("m * g", "h")` |
| `calc` | `integral(expr, var, a, b)` | Definite integral (closed form confirmed numerically, or tanh-sinh quadrature with an error estimate) | `integral("v", "t", 0 s, 1 h)` |
| `calc` | `limit(expr, var, point, side?)` | Limit (value at a point of continuity, or Richardson extrapolation with a convergence check); two-sided, or from one side with `side` `"+"` (right) or `"-"` (left) | `limit("sin(x)/x", "x", 0)`, `limit("1/x", "x", 0, "+")` |
| `array` | `linspace(a, b, n)` | Vector generation: `a` and `b` numbers or quantities of one dimension, the points in the unit of `a` (`linspace(0 m, 1 km, 3)` is `0 m, 500 m, 1000 m`); `n` is a whole, non-negative plain number (50 when it is left out) | `linspace(0.0 m, 10.0 m, 100)` |
| `array` | `gradient(y, x)` | Numerical derivative dy/dx | `gradient([1, 4, 9] Pa, [1, 2, 3] m)` |
| `array` | `trapz(y, x)` | Numerical integration (area) | `trapz([1, 2, 3] N, [0, 1, 2] m)` |

#### Definite integrals

`integral(expr, var, a, b)` (after `use integral from calc`) is the definite integral of `expr` over `var` from `a` to `b`; `integral(expr, var)` is the symbolic antiderivative.

- **Bounds**: a number, a quantity, `"inf"` or `"-inf"`, or a name or an expression written as text (`pi`, `"pi/2"`, `"bohr_radius"`, a variable of the program). A name that resolves to nothing is an error, never 0. A quantity bound makes `var` a quantity of that unit; the other bound must have the same dimension, or be a plain `0` or an infinity. A temperature point or a level is not a bound.
- **Names**: the integrand sees the program's variables (and a function's parameters). A free name resolves as everywhere in PHS (section 1.5): a variable, `pi`, a constant of the catalog, a unit; any other name is an error.
- **Result**: a quantity in (unit of `expr`) times (unit of `var`) whenever a unit or an uncertainty is involved, otherwise a number. With `v = 3 m/s`, `integral("v", "t", 0 s, 1 h)` is `10800 m`.
- **Uncertainty** propagates, from the program's uncertain variables and from uncertain bounds, and the result stays correlated with them.
- **Method**: the closed form `F(b) - F(a)` when the symbolic engine has an antiderivative *and* the quadrature confirms it, so a closed form that jumps across a pole is refused (`integral("x^(-2)", "x", -1, 1)` is an error, not -2). Otherwise tanh-sinh quadrature with an error estimate: accepted when two successive levels agree within a relative 1e-12 of max(|I|, the integral of |f|), from level 4 on, with at most 20000 samples per piece. An infinite end is mapped by x = c ± (1 - u)/u.
- **Refused, not guessed**: a singularity at a non-zero end (`1/sqrt(1-x)` on [0, 1]: substitute so that it sits at 0), a jump or a kink inside the interval (split the interval there), `sin(1/x)` next to 0, an integrand that decays too slowly towards an infinite end. An integrand that cannot be evaluated, or is not finite, at a sample is an error.
- **Known failure mode**: a peak much narrower than the spacing of the first levels' samples is never sampled, and every level agrees on a wrong value: a Gaussian of width 1e-4 at x = 0.3 on [0, 1] integrates to a confident 0. Split the interval around a known peak.

#### Limits

`limit(expr, var, point)` (after `use limit from calc`) is the limit of `expr` as `var` tends to `point`, from both sides; `limit(expr, var, point, "+")` is the limit from the right (`var -> point+`) and `"-"` from the left, also written `side="+"`. The symbolic engine has no limit: the answer is numeric, and it is given only when it converged; otherwise the call is an error, never a guessed `0` or `inf`.

- **Point**: a number, a quantity, `"inf"` or `"-inf"`, or a name or an expression written as text (`pi`, `"pi/2"`, a variable, a constant of the catalog). A name that resolves to nothing is an error. A quantity point makes `var` a quantity of that unit; a temperature point or a level is refused. Towards an infinite point `var` is a plain number.
- **Side**: without a side the limit is two-sided and needs `expr` defined on both sides of the point: `limit("sqrt(x)", "x", 0)` and `limit("x*ln(x)", "x", 0)` are errors (undefined for x < 0: use side `"+"`), `limit("x*ln(x)", "x", 0, "+")` is 0. With a side only that side is sampled, and `expr` must be defined there (`limit("sqrt(x)", "x", 0, "-")` is an error). A side other than `"+"` or `"-"` is an error, and so is a side at `"inf"` or `"-inf"` (approached from one side already). Every other keyword argument of a calc builtin is an error too, never ignored.
- **Names**: `expr` sees the program's variables (and a function's parameters), as the integrand of `integral` does; any other free name is an error.
- **Continuous point**: when `expr` calls only `sin`, `cos`, `tan`, `exp`, `ln`, `log`, `sqrt`, `abs`, `min`, `max` and arithmetic (no exponent involving `var`) and its value at the point is defined and finite, and `expr` is defined at the first step of each side approached and at its closest sample (1/2048 of that step from the point), the limit is that value, with the units and the uncertainty the core gives it (`limit("2000000+x", "x", 1)` is `2000001`). This holds from one side too: `limit("1/(x-1e-9)", "x", 0, "+")` is -1e9, its value at 0, where samples from 1/8 down would all lie past the pole. A point outside the domain is never used (`limit("ln(x)", "x", 0, "+")` is `-inf`, from the samples), nor one isolated from the rest of the domain (`limit("sqrt(x^2*(x^2-1e-4))", "x", 0)` is an error: undefined for 0 < |x| < 0.01). Known limitation: a gap in the domain narrower than the closest sample is not seen, and the value at such a point is taken.
- **Extrapolation**: otherwise (a removable singularity such as `sin(x)/x` at 0, or `floor`, `ceil`, `round`, a comparison, a ternary in `expr`) the samples at `point ± h`, `h = 1/8, 1/16, ...` (12 steps), are extrapolated to `h = 0` (Richardson, by Neville's algorithm) separately on each side. A side is accepted when its error estimate is below `1e-9` of max(|L|, the largest sample) at two successive steps, so a limit of 0 works; the two sides of a two-sided limit must agree to the same tolerance. Where `expr` is undefined at the first step (a boundary of its domain or a pole closer than 1/8), that side starts closer, halving the step up to 20 times: `limit("abs(ln(x+0.1))", "x", 0)` is ln 10. Measured: `(1-cos(x))/x^2` at 0 is 0.5 within 5e-11 (its samples cancel), `sin(x)/x` within 1e-15; `limit("sin(sqrt(x))/sqrt(x)", "x", 0, "+")` is 1.
- **Infinite point**: `var = ±8, ±16, ±32, ...` (24 steps), extrapolated in `1/var`: `limit("1/x", "x", "inf")` is 0, `limit("(1+1/x)^x", "x", "inf")` is e.
- **Zero and infinity**: samples of one sign falling geometrically give 0 (`x*ln(x)` at 0); samples of one sign growing with non-decreasing increments give `inf` or `-inf`, for a two-sided limit when both sides agree: `limit("1/x^2", "x", 0)` is `inf`, `limit("1/x", "x", 0)` is an error, `limit("1/x", "x", 0, "+")` is `inf` and `"-"` gives `-inf`.
- **Uncertainty** propagates: at a continuous point through the core, and an extrapolated limit is a weighted sum of samples formed with the core's arithmetic. An uncertain point where the limit must be extrapolated is refused.
- **Refused, not guessed**: sides that disagree (`abs(x)/x` at 0, `floor(x)` at 1: ask for one side, `limit("floor(x)", "x", 1, "-")` is 0), a side where `expr` is undefined (`sqrt(x)` at -1, `sqrt(x)` at 0 without `"+"`), oscillation (`sin(1/x)` at 0, `sin(x)` at `"inf"`), a change on a scale smaller than the smallest step (`(1-cos(1000*x))/x^2` at 0), and samples that rounding has made equal (`min(x, 100)` at `"inf"`).

#### Your functions inside calc texts

The calc builtins (`deriv`, `integral`, `limit`, `solve`, `simplify`, `series`, `laplace`, `dsolve`, `grad`, ...) and `plot3d`/`plot_field` read a call of one of your functions inside their text by writing its body there, every argument in place of its parameter and in parentheses: with `f(x) = 2*x`, `deriv("f(t)", "t")` is 2; with `f(x) = x^2`, `deriv("f(2*t)", "t")` is `8 * t` and `deriv("f(t) + f(2*t)", "t")` is `10 * t`; nested calls are read too (`g(y) = f(3*y) + y` gives `deriv("g(t)", "t")` = `18 * t + 1`). A function is looked up where the builtin is called, then in the program. Refused, naming the function: recursion (`f -> g -> f`), a wrong number of arguments, a parameter with a declared unit, a decorator, an uncertain literal, a body of more than one expression, a body that is not algebra (a comparison, a conditional, text, a vector, `=>`, `..`, a loop), and an imported function whose body reads a name of its module.

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
