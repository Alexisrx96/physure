# Changelog

All notable changes to the **Physure** ecosystem are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- **Asymmetric uncertainties as moments** (`physure_core::uncertainty::moments`). A measurement quoted as `12.3 +0.5 -0.4 pb` is not two standard deviations, it is the shape of a distribution, and averaging the two halves — the only thing a symmetric model can do — throws away the part that is often interesting. `moments_from_sigmas` maps the pair onto the first three moments of a dimidiated Gaussian in closed form, and `sigmas_from_moments` inverts it by bisection on the skewness, which is monotonic in σ⁺/σ⁻. A skew beyond what a pair of half-widths can describe (just under 1, reached when one side is zero) is reported rather than silently rounded down to the most skewed shape available. `MomentsBackend` and the `UncertaintyValue::Moments` variant give the pair somewhere to live — mean, spread with its lineage, third moment — and stop there. Propagating a third moment is left open on purpose: every arithmetic path on a moments value raises rather than falling through to a symmetric rule, which would report a plausible number with the skew averaged away. This is the groundwork a propagation model can be built on, not the model.
- **PHS parses asymmetric uncertainties**: `12.3 +/- (0.5, 0.4) pb` and `12.3 ± (0.5, 0.4) pb`, upper half first, in the order the operator reads. The parentheses are what keep the notation out of the binary operators' way — a bare `+` never reaches the uncertainty rule, so `12.3 + (0.5)` is still an addition, and the lexer already matched `+/-` and `±` ahead of `+` and `-`. Each half takes its own percentage, so `+/- (1%, 0.5)` means +2.0 and −0.5 on a magnitude of 200. Evaluating one raises: nothing propagates a third moment yet, and every consumer downstream takes a single standard deviation, so the measurement would come back symmetric with no sign that half of what was written had been dropped. The three transpilers refuse for the same reason rather than emitting a symmetric quantity in Rust, Python or Java.
- **`physure.uncertainty_model(...)`**, a per-scope choice of which uncertainty model a quantity is built with — `"gaussian"` today, `"moments"` once asymmetric propagation lands. It is a separate knob from `propagation_mode`, which decides how correlations are handled, not what shape the distribution has. Keeping them apart is what makes an asymmetric measurement the exception rather than the default: three moments cost more to carry through a large dataset than one standard deviation, so they are asked for per scope instead of paid for everywhere. A model that does not exist is rejected on the spot instead of quietly leaving the previous one in place, and selecting `"moments"` raises rather than handing back a gaussian, which would report symmetric answers inside a block entered to say the measurement is not symmetric.
- **`propagation_mode` is read from `physure.conf`.** The `[Settings]` key had been in the shipped conf since the beginning and nothing had ever read it, so a project that set `propagation_mode = uncorrelated` got correlated results anyway, with nothing on screen to say the file had been ignored. `get_propagation_mode()` now falls back to the active unit system's setting, so the default can be set once in a file instead of wrapping every call site; a `physure.propagation_mode(...)` block still outranks it, including when it asks for the value the file already had. An unknown mode is rejected when the conf is loaded rather than on use — a misspelt `uncorelated` would otherwise fall back to correlated, which is the one wrong answer that looks right.
- **The core honours `propagation_mode` too, so PHS and Python cannot disagree about it** (`physure_core::uncertainty::mode`). The core had no uncorrelated arm to switch to — lineage was always tracked, which is what "correlated" means — so a conf that asked for anything else was read by Python and ignored by everything else, and the same script gave two different answers depending on which language ran it. `uncorrelated` is now read in `Lineage::combine`, the single place where shared ancestry is what makes a difference, so both the Gaussian and the unscented arms get it from one branch: the result is a fresh measurement rather than a derivation, since keeping the operand ids would let a later operation cancel against the source the first one was told to ignore. The Monte Carlo arm carries correlation in its sample array instead of its lineage and gets its own answer, so a hand-picked backend cannot keep cancelling inside the scope that exists to stop it. `monte_carlo` and `unscented` in the conf pick what a new measurement is built with, and an exact value stays Gaussian whatever the setting says — a plain number has no distribution to sample, and drawing a thousand identical samples for every literal in a script is a cost with nothing behind it. `mode::scoped` gives Rust callers the per-thread override Python has as a context manager; the file's setting is process-wide, since a thread-local would land on whichever thread happened to read the conf first.
- **PhyEquation & PhyFunction architecture** with equation arithmetic, callable equations, and multi-language transpilation parity tests across PHS, Rust, Python, and Java 8+.
- **Infinity support** (`inf`, `-inf`, `∞`, `-∞`, `infinity`, `oo`) in PHS grammar, limits, and improper integrals.
- **Advanced vector calculus operators**: high-order derivatives `diff(f, var, order)`, gradient `grad`, divergence `div`, curl `curl`, laplacian `laplacian`.
- **`QuantityVector` & `QuantityMatrix`** (Order 1 & Order 2 tensors) with dot product, cross product, norm, transpose, matrix multiplication, and determinant — all with physical unit propagation.
- **N-dimensional native plotting**, written in the crate with no Rust dependency added:
  - `plot3d(expr, title)`: an interactive 3D surface, rendered in the browser with WebGL — rotate, zoom and pan, rather than the fixed isometric projection it started as. three.js and its `OrbitControls` are vendored into the crate instead of pulled from a CDN, so a plot opens with no network and the version cannot change under a file that was saved months ago.
  - `plot_field(u_expr, v_expr, title)`: 2D vector field arrow plots with magnitude colour scaling.
  - `plot_nd(matrix, title)`: N-D parallel coordinates visualization.
- **Strict tensor type separation**: `Quantity` (scalar), `QuantityVector` (vector), `QuantityMatrix` (matrix) as first-class citizens in Rust, PHS, Python, and Java 8+.
- **`PhsValue::Matrix`** variant in `physure-script` with full exporter support (JSON, CSV, Python).
- **Java 8+ classes** `com.physure.QuantityVector` and `com.physure.QuantityMatrix` with idiomatic Java collections API.
- **Python classes** `physure.QuantityVector` and `physure.QuantityMatrix` exposed via PyO3.
- **`abs` accepts a quantity**, returning the same unit and the same uncertainty — folding a magnitude to its absolute value moves where a measurement sits, not how well it is known. It delegates to the core's `UncertaintyValue::propagate_function`, so a Monte Carlo or unscented backend is not silently downgraded to Gaussian.
- **`exp`, `ln` and `log` accept a dimensionless quantity**, propagating its uncertainty, and reject a dimensioned one with a clear error. They used to refuse every quantity ("exp expects a number"), so `exp(0.5 +/- 0.01)` could not be evaluated at all, while `ln(5 m)` — a physics error, since a power series can only be summed over terms that share a unit — was the sort of thing the tool is supposed to catch.
- **`export3d(expr, "surface.stl")`** writes a plotted surface as a mesh file — STL, OBJ, glTF or PLY, chosen from the extension — so a result can go to a slicer, a CAD tool or a renderer instead of only to a screen. Reachable from PHS, the `phs` CLI, the LSP and Python (`Quantity.plot_3d`, `Quantity.export_3d`).
- **Range syntax, `a .. b`.** Sampling a surface needs an interval, and `plot3d(P, x = -2 m .. 2 m)` was a syntax error: `BinaryOp::Range` and `PhsValue::Range` had been in the tree from the start with no grammar rule that could produce one.
- **String interpolation**: `"the beam carries {load}"`, in the interpreter and all three transpile targets. Python and Rust had been emitting the raw literal as an f-string and a `format!`, which only holds for a bare name — `{2 m + 3 m}` is PHS, not Python — and Java, having no interpolated literal at all, raised. All three now share `split_interpolated`, which cuts the literal exactly as the interpreter does and transpiles each expression on its own; Java builds the result with `+`, and a string-valued assignment is declared `String` instead of `Quantity`, which did not compile.
- **`%` and `ppm` are dimensionless units.** `5.0 %` parses, and `200 kPa * 5 %` is `10 kPa` — a percentage is a ratio, so multiplying by one is a unit-preserving operation the system can check rather than arithmetic the reader has to do in their head.
- **Spanish unit aliases**: julio, vatio, pascalio, voltio, ohmio and porcentaje, alongside the SI symbols.
- **The `base` format spec**, quoting a measurement in the units it is built from: `2 kΩ: base` is `2000 A^-2 * kg * m^2 * s^-3`. The scale moves into the magnitude and the uncertainty is rescaled with it, so the physical value is the one `Display` prints — only the terms change. `physure-cli/README.md` had advertised it since its first page while the grammar only accepted the numeric `.3e` form, so it was a parse error. The `frac` spec advertised beside it is dropped from the README instead of guessed at.
- **The `frac` and `ifrac` format specs**, writing a magnitude as a common or a mixed fraction: `1.5 m: frac` is `3/2 m` and `1.5 m: ifrac` is `1 1/2 m`. Only when a fraction applies — a value with no small one behind it keeps its decimal rather than being rounded into a tidier lie, so `3.14159265358979: frac` stays as it is while `0.1 + 0.2: frac` is `3/10`. The f64 debris is cut at the 15 decimal digits an f64 actually carries before the ratio is taken, the same cut `format_float` makes, or `25 m/s => km/h` would ask for the fraction of 89.99999999999999 rather than of 90. Both halves of a measurement are quoted, not just the mean: `9.81 +/- 0.05 m/s^2: frac` is `981/100 ± 1/20 m/s^2`.
- **A range is checked to be an interval when it is built.** `PhsValue::Range` was assembled from whatever stood on either side of `..`, so `0 m .. 100 s` was a range between a length and a time, `100 m .. 0 m` ran backwards, and `"a" .. 100 m` held a string — none of which a plot's sampling or an integration limit has an answer for, and all of which produce a figure that looks fine and is not. Building one now refuses mismatched dimensions (`Incompatible dimensions in range: 'm' vs 's'`), a pair that does not run upwards, including the empty `5 m .. 5 m`, an endpoint that is not a magnitude, and a bound that cannot be ordered at all. An endpoint carrying no dimension of its own takes the other's unit, so `0 .. 100 m` reads as `0 m .. 100 m` — on paper the lower bound of an interval does not repeat the unit either — while a range of plain numbers stays dimensionless. A missing endpoint is caught by the grammar, which requires both.
- **The core's asymmetric moments are reachable from Python** (`physure._core.AsymmetricMoments`, `physure._core.MomentsBackend`, `max_skewness()`). Only `Lineage` had been exported, so the Python side could not reach `moments_from_sigmas` or its bisection inverse, and a second implementation of them would have drifted from this one until the two disagreed about the skew of the same measurement. Propagation is deliberately not exposed, because the core does not have it: a value built here can be measured and reported, and every arithmetic path still refuses rather than answering symmetrically.

### Changed
- `Quantity.java` now uses `List<Quantity>` instead of raw `double[]` for `QuantityVector` interactions; added `mul()`, `div()`, `sub()` shorthand aliases.
- **BREAKING (PHS)**: local bindings moved from `let name = value in body` to a postfix `where` clause — `body where name = value[, name2 = value2]`. A later binding can use an earlier one. `let` stays a reserved word with no rule behind it, so the old form fails to parse instead of quietly meaning "let times inches".

### Removed
- **`physure.ext.grammar`**, the Python reimplementation of the PHS language (1655 lines), and its test module. Only Rust implements PHS now: `physure.repl` (`python -m physure`, `physure repl`) evaluates through `physure._core.Interpreter` and reports an install hint if the native engine is missing. Startup for `python -m physure "500 N / 2 m^2 => kPa"` drops from the Python `UnitSystem` build to ~0.09 s.

### Fixed
- **Transpiled Rust kept the formula's shape.** The Rust codegen emitted binary operations unparenthesised, so any grouped subexpression was re-associated by Rust's own precedence on the way out: `12.0 m / (3.0 s * 2.0)` became `12 / 3 * 2` and answered 8 where PHS, Python and Java all answer 2. Division was the visible case, but the same flattening applied to every arithmetic operator, and the result still compiled and still carried a plausible-looking unit — the failure mode a transpiler is least able to afford, since the wrong number leaves as source code someone else then trusts. Operands are now wrapped the way the Python target already wrapped them.
- **The pandas accessor is `.phs`, not `.mk`.** `mk` was this project's provisional name and the Series accessor was the last public API still carrying it. `.mk` stays registered as a deprecated alias that warns and forwards, so existing code keeps working and is told what to change rather than failing with an `AttributeError` that says nothing about where the name went. The Python REPL prompt moved from `mk> ` to `phs> ` for the same reason — it disagreed with the native `phs` REPL, which had always used `phs> `.
- **A `(sigma-, sigma+)` pair on a scalar says what it is.** `Q_(12.3, "pb", uncertainty=(0.5, 0.4))` reached `float()` and came back as `TypeError: float() argument must be a string or a real number, not 'tuple'` — a message about the plumbing, where PHS answers the same input by naming what it cannot do yet. The two entry points now say the same thing. The guard is narrow on purpose: a scalar has no second element for a second uncertainty to belong to, so a pair on one can only be asymmetric, while an array magnitude keeps taking a tuple as the ordinary per-element uncertainty it has always been.
- **Java adds uncertainties in quadrature, and refuses a sum it cannot convert.** `Quantity.add` and `Quantity.subtract` fell back to `a.uncertainty + b.uncertainty`, so 0.3 and 0.3 gave 0.6 where Rust and Python give 0.42 — an error about 40% too large for every quantity that went through the Java path. Worse, the mismatched-unit fallback added the raw magnitudes with no conversion at all, so `1 km + 1 m` came back as 2 km; that path now raises a `PhysureException` naming what it cannot do instead of returning a number. There is no JUnit in the build, so the check compiles the classes with `javac` and runs a small program against them.
- **A quantity no longer behaves as if it were independent of itself.** A standard deviation alone cannot say whether two operands came from the same measurement, so `x - x` reported `σ·√2` instead of zero, `x / x` reported an uncertain 1, and `x + x` gave ±0.42 where the answer is ±0.6 — a result that looks plausible enough to ship in a paper. The core now records provenance alongside every scalar uncertainty (`uncertainty::lineage`): each value carries the measurements it came from and their sensitivities, merged through the chain rule `c_new(id) = Σ Jₖ·cₖ(id)`, with `σ = √(Σc²)`. Where two lineages share no source this reduces exactly to the quadrature sum it replaces, so results for independent quantities are unchanged. Tracking is always on, needs no opt-in, and covers all three models — Gaussian and unscented by jacobian, Monte Carlo through its shared sample array. Cancellation survives scaling and intermediate steps: `2x - 2x` is zero and `(x + y) - y` returns `x` with its own uncertainty. Two separate measurements that happen to agree still combine in quadrature, and an exact constant (a conversion factor, a plain number) never becomes a source. This reaches PHS, the Rust API and Python alike, where previously only Python could get it right and only inside a `PhysureContext()`.
- **Python's scalar path now tracks provenance by default, matching the Rust core.** `Uncertainty.from_standard` picked the lineage-tracking model only when a *vector* covariance store happened to be active — a store that path never uses — so a plain `Q_(3.0, "m", uncertainty=0.3)` fell back to a variance-only model and `x - x` came out at 0.4243 while the same expression in Rust and PHS gave 0. Scalars are lineage-tracked unconditionally now, so the three environments agree digit for digit. Array uncertainties keep their existing dispatch. `propagation_mode("uncorrelated")` becomes the opt-out and is now actually honoured — it had no effect on this path before, and only looked like it worked because the default was already behaving uncorrelated.
- **Python computes scalar provenance in the Rust core, not next to it.** The lineage type is exported as `physure._core.Lineage` and `LineageModel` is a thin wrapper over it, so a scalar uncertainty in Python follows the same code that PHS and the Rust API follow instead of a parallel implementation that can drift away from it one rounding decision at a time. A lineage pickles through `Lineage.from_terms`, which rebuilds it with its original source ids rather than minting new ones — an unpickled quantity still cancels against itself — and importing ids raises the core's counter past them, so a lineage restored in a fresh process cannot collide with one minted there later. `Lineage` compares by its terms, never by which variant holds them, so a value that made that round trip still equals the one it came from.
- **A live JAX or torch uncertainty now says so instead of answering differently.** Coefficients are `f64` in the core, so a value still attached to an autograd graph or a `jax.jit` trace cannot be handed to it without detaching it. Under `jax.jit`, `x - x` used to come back as 0.1414 while PHS, Rust and eager Python all said 0. Such a value now raises and names the two ways forward: `physure.python_lineage()` for the Python implementation, which keeps the graph and gives the correct 0, or `physure.propagation_mode("uncorrelated")` to drop provenance deliberately. It is per-user and per-scope, never a default, because a silently different answer across two languages is worse than an error. Concrete tensors and arrays with no graph are untouched and still go through the array machinery, where `x - x` remains the quadrature sum.
- **`QuantityVector::norm()` reported an uncertainty exactly √2 too small.** It squared the vector through `dot(self, self)`, and `mul` treats its two operands as statistically independent, so each component contributed σ(xᵢ²) = √2·xᵢσᵢ instead of the correct 2·xᵢσᵢ. Every component is now squared with `pow(2.0)`, whose analytic derivative knows it is the same variable, which gives the GUM result σ = √(Σ (xᵢ/|v|)²σᵢ²) and preserves the component's uncertainty backend instead of collapsing it to Gaussian. For `(1 ± 0.09, 2 ± 0.06, 2 ± 0.18) m` the norm now reads `3 ± 0.13 m`, not `3 ± 0.0919 m`.
- **Monte Carlo propagation no longer redraws an operand that already carries samples.** `ensure_samples` rebuilt the other operand's array from its mean and standard deviation even when that operand was itself a Monte Carlo value, so any shared history between the two was destroyed — the one thing sampling is supposed to give for free. `x - x` returned 0.0028 ± 0.4253 instead of 0 ± 0, `x / x` returned 1.0101 ± 0.1455 instead of 1 ± 0, and `x + x` reported ±0.4227 where the answer is ±0.6. The propagation path now reuses the operand's own sample array when the lengths match; two independently drawn quantities still combine in quadrature, and correlation survives a unit conversion because rescaling multiplies by a zero-sigma constant.
- **A unit symbol that the script also binds as a variable is now rejected instead of guessed.** `g = 9.81 m / s ^ 2` followed by `f = 10.0 kg * g` read `g` as the gram unit and produced a mass-squared quantity where the author meant 98.1 N; with `t = 3.0 s` bound, `5 m / t` meant metre per *tonne*. The check runs once over the parsed program, so the interpreter and all three transpile targets are covered, and the error names the token and both rewrites: `(10 kg) * g` for the variable, `10 kg * "g"` for the unit. Only a token following an explicit `*` or `/` is a candidate — `10 m` is a quantity literal by grammar, so binding `m` does not poison every later literal.
- **`floor` and `ceil` keep the uncertainty**, like `round` now does. Both rebuilt the quantity with a zero standard deviation, so `floor(9.81 +/- 0.05 m/s^2)` printed as an exact `9.0 m / s ^ 2`. The mean is moved by adding an exact offset, which slides the whole distribution and leaves a Monte Carlo or unscented backend intact.
- **`sin`, `cos` and `tan` are dimensionally checked and keep the uncertainty.** They returned a bare number built from the mean — unit and uncertainty both discarded — and applied themselves to anything, so `sin(9.81 m/s^2)` answered -0.379 as though metres per second squared were radians. The argument must now be an angle (converted to radians through its own scale, so `sin(90 deg)` is 1) or a dimensionless value read as radians; the result is dimensionless and the sigma comes from the derivative.
- **A degree is a plane angle again.** `physure.conf` declares `radian` and `steradian` against the same dimension symbol, and the Rust loader let the last one win, so `deg`, `arcmin` and `arcsec` were registered as scaled *steradians*: `90 deg => rad` failed as a unit mismatch while `1 deg + 1 sr` was accepted.
- **Inches are usable again**: `in` was a grammar keyword taken by `let ... in`, so `12 in`, `12 in => cm` and `1.5 in^2` were parse errors and only the `inch` alias worked.
- **Every prefixed unit was read one character short.** The `_is_*` predicates in the grammar were written without `&`, so instead of peeking at the next character they consumed it, and the symbol that followed lost its first letter: `1 kg` evaluated as 1 gram, `100 kPa` as 100 pascal, `1 mol` as `1 * ol`. The answers were off by three orders of magnitude and carried the wrong unit while looking entirely ordinary on the page. A sweep test now evaluates all ~240 unit symbols the shipped systems define, and the handful that still fail are listed by name in `KNOWN_GAPS` rather than left to be rediscovered.
- **A unit symbol was truncated at its first digit.** `_unit_char` had no `ASCII_DIGIT`, so the remainder became an implicit multiplication: `1 a0` evaluated to `0.0 a` — the Bohr radius annihilated to zero — and `1 m2` to `2.0 m`. The lookup now tries the whole symbol before the digit-stripped stem, so `a0` stays the Bohr radius while `m2` is still metre squared, and a digit-bearing name that is not a unit (`x2`) is still an ordinary identifier.
- **Non-ASCII unit symbols never reached the registry.** The lookup scanned with `is_ascii_alphabetic`, so any Unicode symbol the grammar accepted resolved to the empty string and was handed to the expression parser, which rejects it: `1 Å` and `1 °` were parse errors, and `1 Ω` worked only because `Ω` happens to be listed in the `identifier` rule. The arcmin and arcsec prime aliases were also written as `'] #'` in `physure.conf`, where the alias list closes at the first `]`.
- **An unknown unit is reported instead of invented.** A symbol the registry did not know had a dimension fabricated for it, so `5 foobar` evaluated happily and every operation after it carried a unit that meant nothing. It now raises `UnknownUnit` with the nearest symbol as a suggestion, retrying case variants first, since a typo in the case is the common cause.
- **`1 km == 1000 m` was false** while `1 km + 1000 m` converted correctly — comparison read the magnitudes and ignored the scale, so the one operation whose whole job is to answer a question about equality was the one that got it wrong. All seven comparison builtins now share the scale-folding helper the arithmetic path uses, and comparing mismatched dimensions raises rather than answering.
- **`10.0 +/- 0.5 m` printed as `10.0 m`.** The uncertainty was parsed, carried and propagated correctly, and then dropped by `Display` — the one place a reader would look to check it.
- **`Ω` was missing from the derived-symbol table**, so `2 V / 3 A` printed as `kg·m²/(A²·s³)` instead of `0.667 Ω`.
- **`PhyFunction.solve()` produced a malformed definition.** The Python binding stringified the whole `PhsValue::Equation`, so `symbolic.py` embedded `var = expr` into a new function body and wrote out `name(...) = var = expr`. It now matches on the equation and returns the right-hand side alone.
- **`cond ? a : b` was eaten by the format spec.** `format_spec` accepted a bare `ASCII_ALPHA+`, so the `: b` of a ternary looked like a format directive and the branch disappeared into it. The `format` builtin also ignored the spec it was given and printed the default rendering.
- **`deriv("0.5*m*v^2", "v")` differentiated whatever was in scope.** A quoted string parsed as `Expr::Identifier`, so the interpreter looked its whole text up in the environment before falling back to a string: with `v = 3.0 m/s` bound, the formula was silently rewritten by the value. Strings are `Expr::Str` now and stay literal; `{expr}` is the explicit way to fold a value in. Interpolating a quantity used to defeat the purpose anyway — the symbolic parser had no implicit multiplication, so `0.5 * 2.0 kg * v^2` stranded `kg` after the number and the derivative collapsed to 0.
- **Calling an equation checked only one side for satisfied free symbols**, so an equation whose unknown sat on the left refused the arguments that would have solved it.
- **More derived symbols print as symbols**: V, F, S, Wb, T, H, lx and kat. Bq, Gy, Sv and lm are deliberately left unmapped: Bq shares its dimension with hertz, Gy with Sv, and lm with candela, and picking one of a genuinely ambiguous pair on the user's behalf is how `gal` became a galileo.
- **An anonymous compound unit folds its scale factor into the printed magnitude** before the derived-symbol lookup, so an ohm times a milliamp displays `239.68 V` instead of a base-unit magnitude with the scale left off.
- **Unicode letters are accepted in unit annotations**, so a parameter can be annotated `R: Ω` — `unit_term` took `ASCII_ALPHA` only, and every annotation using a non-ASCII symbol was a parse error.
- **The LSP reported diagnostics on the wrong line** and wrapped every message in a generic wrapper that hid what was actually wrong; `validate_unit_shadowing` now carries the statement's own position.
- **Method-call syntax never parsed.** `method_call` was a rule with nothing referring to it, so the interpreter arm that handles it was unreachable; `primary` now goes through it.
- **`100 kPa * sin(x)` parsed `sin` as a unit.** The unit rule matched the function name before the call rule could, turning a product into a nonsense dimension.
- **`call_arg` accepted only `name = value`**, so the guide's `plot3d(P, x: r)` would not parse; `name: value` is accepted too, with the plain-expression branch left last so a format spec (`x:.2f`) still wins. Reading a ternary's branches off `expr` also panicked, because `ternary_op` is a rule of its own and nests them a level deeper.
- **`J`, `kJ` and `kg·m²/s²` shared one entry in Python's unit cache.** The key was `RationalUnit::id`, which is derived from the dimensions alone, so whichever spelling arrived first answered for all of them. Scale and display name are part of the key now.
- **`physure_core::linalg` and `physure_core::plotting` were not declared in `lib.rs`**; without the first the workspace did not compile at all.
- **A scaled dimensionless unit printed a `unity` dimension.** A unit of dimension "1" was resolved through `dim_to_base`, which maps "1" to the `unity` unit, so `2 m * 3 %` printed as `m * unity`. Dimension "1" is tested before that lookup now, and `RationalUnit::mul`/`div` carry the display name through when the other operand is a dimensionless scale-1 unit.
- **`25 m/s => km/h` read `89.99999999999999 km/h`.** `format_float` printed the shortest string that round-trips an f64, which is honest and shows every piece of conversion debris. An f64 carries 15 decimal digits exactly and the debris always lands past them, so the magnitude is rounded to 15 significant figures and that result kept only when it is genuinely shorter — a literal typed to full precision still round-trips.
- **`__repr__` reassembles a prefixed symbol** (kPa, mV) for known derived units instead of falling back to base dimensions.
- **A unit symbol meant one thing in a literal and another after `=>`.** `2 Ω => kΩ` reported `Unknown unit 'k'`: a conversion target is parsed as an ordinary expression, so it goes through `identifier`, which was a hand-kept list of Greek letters for the first character and ASCII for the rest, and `kΩ` was cut after the `k`. The literal `1 kΩ` parsed correctly the whole time, because that path reads `_unit_char`, which is `LETTER`. Both rules read `LETTER` now. The list they replace was already missing symbols the registry ships (Å, and µ written as U+00B5 rather than U+03BC), which is how the defect survives: adding a unit does not tell anyone that a grammar rule elsewhere has to learn about it. The sweep guarding the literal position has a counterpart for the target position now, over the same ~240 symbols; `°` and `%` remain out of reach there, since no identifier can spell a character that is not a letter.
- **A constant on the left of an operation could change the uncertainty model.** Propagation dispatches on the left operand, so with `propagation_mode = monte_carlo` in the conf, `3 m + x` fell through to the generic arm and came back Gaussian with the samples discarded, while `x + 3 m` kept them: the same expression gave a different model depending on which side the constant sat on. An exact value has no distribution to lose, so it is now re-expressed in the other operand's model — n copies of the mean, or a zero-width sigma point — before the operation runs.
- `physure._core.Quantity.__str__` renders the measurement (`0.25 kPa`) by delegating to the core's `Display`, instead of repeating `__repr__` (`Quantity(0.25, kPa)`); the REPL and `print()` now read like the `phs` CLI.
- Local bindings now transpile to real code in the Python, Rust and Java targets. They used to be emitted as a call to an undefined `let(...)` function, so the generated file did not compile.
- **Uncertainties survive formatting, rounding and `repr`.** A format spec (`g:.2f`) printed the mean alone, `round(q, n)` rebuilt the quantity with a zero standard deviation, and `physure._core.Quantity.__repr__` omitted the uncertainty — an uncertain measurement looked exact in all three.
- **A percent uncertainty is relative again**: `9.81 +/- 0.5% m/s^2` was parsed as ±0.5 instead of ±0.049, a spread twenty times too wide. A percentage applied to a magnitude that is only known at run time is now rejected rather than guessed.
- Transpiled files stamp the compiler's real version. All three targets hardcoded `v0.2.4` while the workspace was on `0.2.3`, so a generated file named a compiler that had never produced it; the banner now comes from `env!("CARGO_PKG_VERSION")`.
- The `physure` crate README's usage example imported `use physure::{...}`, which does not compile. The package is `physure` but its library target is `physure_core`, and Cargo derives the import path from the latter. The README is not wired in with `include_str!`, so no doctest ever caught it.
- `Debug for Quantity` (Rust) prints `std_dev`, so a failing assertion about an uncertain quantity no longer reports what looks like a different measurement.
- **A PHS format spec applies to the whole expression, not to its right operand.** `op_format` sat inside `comp_expr`, one precedence level below `+ - => ..`, so the spec bound to whatever stood immediately to its left inside that level. `0.1 + 0.2: .2f` was a parse error, and `25 m/s => km/h: .2f` printed `25.00 m/s` — the spec formatted the conversion *target* and the conversion was dropped without a word, the one failure a unit library may never have quietly. Parenthesising the expression was the workaround and still works; it is no longer needed.
- **`..` binds looser than every other operator, so each endpoint is a whole expression.** It used to sit at the same precedence as `+` and `=>` and associate to the left, which left `0 m .. 100 m => km` reading as `(0 m .. 100 m) => km` — and since nothing converted a range, the `=> km` was dropped in silence. The conversion now belongs to the endpoint it was written on (`0 m .. 0.1 km`, and `0 m => km .. 100 m` gives `0 km .. 100 m`), while a parenthesised range converts as a range (`(0 m .. 100 m) => km` is `0 km .. 0.1 km`). A range is its two endpoints and nothing else, so multiplication, division, conversion and the format spec now reach both instead of neither: `(0 .. 100) m` is `0 m .. 100 m` and `0.5 m .. 1.5 m: ifrac` is `1/2 m .. 1 1/2 m`. Exactly one `..` per expression — `a .. b .. c` names no interval, and letting it parse would only pick one of the readings and print it as though it had been meant.
- **Transpiling a range reports instead of crashing.** `BinaryOp::Range` was `unreachable!()` in all three code generators, so `phs transpile` on a script holding `r = 0 m .. 100 m` panicked rather than saying what it could not do. An interval is something a builtin samples, not a value a Rust, Python or Java variable holds, so the three targets now return a codegen error naming the two ways forward: pass the range to the call that consumes it, or transpile its endpoints separately.

---

## [0.2.3] - 2026-07-26

**Tags:** `v0.2.3`, `core-v0.2.3`, `java-v0.2.3`

### Added
- **PHS CLI i18n/LaTeX fixes** and cross-platform installer improvements (#40).
- **Auto-loading of `ext/*.py` functions** into PhysureScript interpreters.
- **Native `.rs` plugin FFI** with rich value types and hot-reload support.
- **`phs new-plugin` scaffold** command in CLI.
- **Domain-gated builtins** and lazy plugin/ext loading.
- **PHS Language Server Protocol (LSP)**:
  - Context-aware `use`/`from` autocompletion.
  - Surface `use`/`from` domain-gated builtins in completions and hover.
  - Auto-pop the `use`/`from` suggestion widget on space.
- **`physure-lsp` binary** shipped alongside `phs`, with native `linux-aarch64` builds.
- **`physure-java` Maven Central packaging** setup, bump to 0.2.3.

### Fixed
- Forward file path in daemon protocol for `use`/`from` resolution.
- Remove lazy Python ext domain loader (`use ... from <py-stem>`).
- Restore `physure` as crates.io package name for `physure-core`.

---

## [0.2.2] - 2026-07-20

**Tags:** `v0.2.2`, `py-core-v0.2.2`

### Added
- **Native PHS Rust core engine**, standalone `phs` CLI binary, and PyO3 FFI bindings (`v0.2.1-phs`).
- **Combined `py-core-v*` tag** to release to both PyPI and Crates.io simultaneously.

### Fixed
- Restore environment to `crates-io` in `core-release.yml`.
- Track `physure-core/src/bin/phs.rs` by fixing `.gitignore`.

---

## [0.2.1] - 2026-07-19

**Tags:** `v0.2.1`, `core-v0.2.1`, `py-core-v0.2.1`

### Added
- **Physure logo assets** with theme-aware logos in all READMEs.
- Brand-colored badge set with live PyPI, crates.io and CI tags.
- **MKML grammar extensions**: `linspace`, `plot` support, and text interpolation.
- Rust-native `approx_eq`, `sqrt` and DSL helper exports.
- `RationalUnit::base()` method in core engine.

### Fixed
- Reject bare number addition with dimensioned quantities in symbolic engine.

---

## [0.2.0] - 2026-07-18

**Tag:** `v0.2.0`

### Changed
- **Complete rebranding** from MeasureKit to **Physure**.
- **Monorepo Cargo Workspace** restructure: `physure-core`, `physure-script`, `physure-python`.
- Simplified `physure-python` package layout to flat `physure-python/physure`.

### Added
- **Complete decoupling of `physure-core` from PyO3** — pure Rust core with no Python dependencies.
- **Q⁷ mathematical foundation core**: `DimVector` 7-dimensional vector space, Dual AD, 2nd-order Hessian propagation, and Intervals.
- **Expanded symbolic engine**: general power differentiation `u(x)^v(x)`, algebraic factorization, and advanced integration rules.
- **Full Rust-first migration**: unit parser, pre-baked registry, converters, `DimVector`, `UnitDefinition`, and Hessian propagation — all in Rust.
- `CompoundUnit` becomes a thin wrapper over Rust `RationalUnit`.
- Native Rust `Expr` PyO3 class for symbolic expressions.
- Transcendental function delegation (sin, cos, tan, exp, log, tanh) to Rust with uncertainty propagation.
- **Unit-aware `curve_fit`** and friendly constant name resolution.
- Comprehensive performance benchmarks report (`BENCHMARKS.md`).
- Multi-platform release wheels CI pipeline.
- Pinned GitHub Releases on version tags.

### Fixed
- PEP 639 license metadata for PyPI validation.
- Zero-exponent parser guard for digit-suffixed alias names (`a0`, `tau0`).
- Non-integer dimension exponent rejection from native parser.

### Removed
- Python lexer and recursive-descent parser (replaced entirely by Rust core parser engine).
- `physure/domain/notation` module (consolidated into measurement module).
- Rust-fallback illusion stubs.

---

## [0.1.9] - 2026-07-14

**Tag:** `v0.1.9`

### Added
- **MKML notation layer** improvements:
  - Unicode subscript digits in chemical formulas and reaction terms.
  - Equilibrium arrow (`⇌`, `<=>`) with reversible flag.
  - `×` and `÷` as multiplication/division operators.
  - `√` / `sqrt(...)` prefix operator.
- **MKML math functions**: `round`, `floor`, `ceil`, `min`, `max`, `sin`, `cos`, `tan`, `exp`, `log`, `ln` via dispatch table.
- **MKML user-defined functions**: definitions and calls, recursion with configurable depth limit, optional typed parameters, `let...in` local bindings, display-text blocks.
- **Ternary operator** and comparison operators in MKML grammar.

### Fixed
- CompoundUnit recipe lookup keyed by every alias, not just the canonical symbol.
- Hide unit for dimensionless quantities in display.
- Log-domain-error tests made version-agnostic.

---

## [0.1.8] - 2026-07-10

**Tag:** `v0.1.8`

### Added
- **Automated release pipeline**: bump workflow with pinned GitHub Actions (#33).
- **Rust core engine** with PyO3 0.23 migration, `__torch_dispatch__`, custom Triton kernel for covariance updates.
- **Cross-correlation storage**, pruning config, and autograd covariance store.
- **Arrow IPC export** and pickle state support in Rust core.
- **Symbolic module**: `SymbolicExpression.compile` for optimized numeric execution.
- **MKML (MeasureKit Markup Language) grammar interpreter** for MeasureNote-style notes (#10).
- **Unit-aware terminal REPL** (`python -m measurekit` / `measurekit repl`) (#12).
- **Equivalencies**, currency extensions, expanded unit/constant catalog, PINN example (#14).
- **Exact rational conversion factors** — float only at output (#23).
- PEP 561 `py.typed` marker and typing stubs.
- Optional SymEngine acceleration for symbolic operations.
- Python 3.13/3.14 support (PyO3 0.23→0.25).
- **Zero required runtime dependencies** (#9).
- First-use performance improvement: 4.8s → 0.5s (lazy torch/scipy, single system build, REPL pre-warm) (#18).
- WASM/Pyodide build support via GitHub Action.
- `UnknownUnitError` with `difflib` suggestions.
- `save_state` / `load_state` for application persistence.
- Covariance store pruning and non-linear fallback.
- Comprehensive README overhaul with grammar ext, REPL, install extras, units reference (#13).

### Changed
- Core install slimmed from ~130 MB to ~5 MB.
- Crate-type changed to `staticlib` for Pyodide compatibility.

### Fixed
- 29 SonarQube violations resolved, coverage raised 60→84%.
- `gal` redefinition resolved (gallon keeps `gal`, galileo gets `Gal`) (#17).
- Correctness batch: angles, uncertainty Jacobians, roots, recipes, entry points (#6).

---

## [0.0.3] - 2025-10-01

**Tag:** `0.0.3`

### Changed
- Documentation and formatting improvements across multiple modules and tests.
- Enhanced `SymbolicQuantity` class with improved unit handling and operator overloading.
- Context management for active unit system and enhanced `UnitSystem` class.
- Improved `QuantityFactory` initialization and global default system management.

---

## [0.0.2] - 2025-09-24

**Tag:** `0.0.2`

### Changed
- Refactored MeasureKit's unit handling and initialization system.
- Improved documentation across measurement modules.
- Simplified `Dimension` class implementation.
- Enhanced type hinting and simplified arithmetic operations in `Quantity` and `CompoundUnit`.

### Fixed
- Replace `ValueError` with `IncompatibleUnitsError` in integration and measurement tests.

---

## [0.0.1] - 2025-09-16

**Tag:** `0.0.1`

### Added
- Initial release of MeasureKit (pre-Physure):
  - `Quantity` class with magnitude, uncertainty, and unit handling.
  - `CompoundUnit` for composite unit expressions.
  - Notation module with lexer and parser for unit string expressions.
  - Dynamics module with ODE solver and unit awareness.
  - `Function` class with immutability and validation.
  - Configuration management with singleton pattern.
  - `Uncertainty` class for managing measurement uncertainty.
