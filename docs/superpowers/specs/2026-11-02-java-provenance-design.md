# Java quantities that keep their sources

Date: 2026-11-02. Status: approved by the maintainer ("Do it", 2026-11-02). Branch `feat/java-provenance`.

## Why

A Java `Quantity` is a bare `(value, uncertainty, unit)` triple. Every native call turns it into a **new**
core quantity with a **new** source id and turns the answer back into a triple, so the core's lineage (which
measurements a value came from, and how strongly each one still moves it) is thrown away at every step:

- `x.subtract(x)` is `sqrt(2) u`, not `0`; `x.multiply(x)` is `sqrt(2) x u`, not `2 x u`. `QuantityVector`
  works around the second one by hand (`a == b ? a.pow(2) : a.mul(b)`); nothing works around the rest.
- An expression that reuses a measurement reports a larger uncertainty than the one the GUM gives.
- Degrees of freedom (Welch–Satterthwaite, JCGM 100:2008 G.4) cannot work at all: they are kept per source
  and combined through the lineage. This is why the Java generator refuses the `dof` clause today.

The other bindings (core, PHS, Python, WebAssembly) already behave as the core does. Java must too.

## What changes for a Java user

1. A `Quantity` has an identity of measurement. `new Quantity(10, 0.5, "m")` is one measurement, and every
   result derived from it remembers that. Two separately constructed quantities with equal numbers are two
   measurements, as everywhere else.
2. Reusing a measurement cancels as the core says: `x.subtract(x)` is `0`, `x.multiply(x)` is `x^2` with
   uncertainty `2 |x| u`, `x.add(y).subtract(y)` is `x`. **This changes results that were wrong before**;
   it goes in the changelog as "Changed".
3. New API, the same as the other bindings: `withDof(nu)`, `Quantity.fromSamples(double[], unit)` (Type A),
   `effectiveDof()`, `coverageFactor(p)`, `expanded(p)` returning an `ExpandedUncertainty`.
4. `PhyFunction.call(Quantity...)` and `PhyEquation.call(...)` bind quantity arguments **by value, with
   their sources**, instead of as PHS text. `f(x, x)` sees one measurement. A `String` argument stays PHS
   source.
5. The Java generator writes the `dof` clause as `.withDof(n)` instead of refusing it.

## Design

### The core owns the lineage; Java only carries it

No lineage mathematics is written in Java. The core gains two small things, next to `display_parts`:

- `Quantity::from_sources(mean, sources, unit)`: a Gaussian quantity rebuilt from `(source id,
  contribution)` pairs (the contribution is `c_i u_i`, in the unit of the quantity).
- `Quantity::sources()` and `Quantity::display_sources()`: the same pairs read back, in the stored unit and
  in the unit `display_parts` reports (an anonymous compound unit folds its scale into the numbers, and the
  sources must be folded the same way, so that they always add up in quadrature to the uncertainty shown).
  `None` for a model that keeps no sources (Moments, Monte Carlo, unscented, custom).
- `uncertainty::new_source_id()`: a fresh id from the core's one counter, so Java, PHS and Python never
  collide.

Only the Gaussian model has sources, as only it has degrees of freedom. The Java binding never changes the
propagation mode (the `[Settings]` of a conf are process-wide and the registry does not apply them), so every
Java quantity is Gaussian.

### Java `Quantity`

Immutable fields as before, plus the sources: `long[] ids` and `double[] coefficients` (ids are `u32` in the
core and a `long` here, so they never go negative).

- **A quantity that came out of the core** carries the sources the core gave. Exact means empty arrays.
- **A quantity constructed in Java** with an uncertainty has *pending* sources. They are resolved the first
  time they are needed: one id is minted from the core's counter (`NativeEngine.newSourceId()`), with the
  uncertainty as its contribution, and the result is remembered on the object (a volatile immutable holder,
  so two threads that race get the same id). The same object used twice is therefore the same measurement.
  Construction stays pure: it does not load the native library, and a quantity that is never used in
  arithmetic never burns an id.
- `equals` and `hashCode` stay value-based and ignore sources, as before. Two equal quantities can therefore
  behave differently in arithmetic (one measurement or two). The Javadoc says so.
- `round(decimals)` keeps the sources, scaled so they add up to the rounded uncertainty (zero rounds to
  exact). It is a presentation operation, not a new measurement, and it is the one place where Java scales a
  source itself.
- `withDof(nu)` resolves the sources and declares `nu` for the single source through the core, which refuses
  an exact quantity, a combination of several sources and a conflicting value; it returns `this`, as the
  core's does. `effectiveDof()`, `coverageFactor(p)` and `expanded(p)` rebuild the core quantity and ask it.

### JNI

- `get_rust_quantity` reads the value, the unit, `sourceIds()` and `sourceCoefficients()` and calls
  `Quantity::from_sources`, so the core merges lineages as it always does. A conf-defined unit keeps working
  because the unit text is parsed as before.
- `make_java_quantity` builds the Java object from `display_parts` and `display_sources`, so `2 m * 3 km` is
  `6000 m^2` with sources folded the same way. `convertQuantity` hands back the converted value with its
  sources (a conversion mints no id: `x - x.to("cm")` is `0`).
- `sin`, `cos`, `tan` and `abs` stop being computed in Java (`|cos x| u` by hand drops the sources). One
  native, `mathFunction(q, name)`, calls the core's `Quantity::sin/cos/tan/abs`, which already handle the
  angle unit and propagate through the lineage.
- New natives: `newSourceId`, `declareDof(q, nu)`, `effectiveDof(q)`, `expanded(q, p)` (returns `k`, `U`,
  `nu_eff`), `fromSamples(double[], unit)`, `gumRound(mean, u)` (for `ExpandedUncertainty.toString`),
  and `evaluateQuantityWith(handle, expr, names, values)`.

### Calls by value

`PhsInterpreter` gets `run_program_with(&self, program, bindings)`: it evaluates the program in a **copy** of
the interpreter's environment with `bindings` added, returns the value of the last statement, and leaves the
interpreter exactly as it was (no leftover variables, which is why `PhyEquation` substitutes text today). The
Java natives and, later, WebAssembly (whose `PhyFunction.call` has the same text channel) use it. This is the
one change outside `physure-java` besides the core's, so it is held to the stricter Sonar gate
("Physure Core Strict").

`PhyFunction.call(Quantity...)` binds the arguments to reserved names (`_arg0`, `_arg1`...) and evaluates
`name(_arg0, _arg1)`. `PhyEquation.call(Object...)` binds a `Quantity` value under the variable's own name in
the solved right-hand side and substitutes a `String` value as text, as it does today; `callMap` keeps its
`Map<String, String>` contract.

### Tests

- Core: `from_sources`/`sources` round trip, display folding, `None` for other models, and that a rebuilt
  quantity cancels against itself (`physure-core/tests/sources.rs`).
- Script: `run_program_with` sees user functions and shadows variables only for the call, leaves nothing
  behind, and `f(a, a)` with one bound quantity is exact (`physure-script/tests/run_program_with.rs`).
- Java (`mvn test`): cancellation and shared-source cases for every operation, the thread race on the first
  use, `round`, conversion, `evaluate` results, function and equation calls by value (including that the
  registry keeps no variables afterwards), and every row of `tests/golden/uncertainty_dof.tsv`
  (`UncertaintyDofGoldenTest`), the file the core and WebAssembly are held to.
- The generator test asserts `.withDof(9.0)` for Java instead of the refusal.

### Out of scope

WebAssembly's text channel in `PhyFunction.call` (it can use `run_program_with` later); arrays and matrices
with sources beyond what `QuantityVector`/`QuantityMatrix` get for free from their elements; non-Gaussian
models in Java.

## Verification

`cargo test -p physure -p physure-script`, `cargo build -p physure-java`, `mvn test` on JDK 21 locally; CI runs
Java 8 to 25, so the sources stay Java 8 compatible (no `var`, no `List.of`, no `String.repeat`). Sonar: the
core and script changes are new code under the strict gate (coverage 90 %, no new violations).
