# Unit holes: `max`, `min`, `vector`, `linspace`, `round`

Date: 2026-11-17. Branch: `docs/post-release-0.4.0`. Decision of the owner on 2026-11-17: the elements of a vector share one dimension; the rest follows the rule of the house, **a wrong answer with a confident unit is worse than an exception**.

## 1. The holes (measured on 2026-11-17 with `phs eval`)

| Program | Today | Why it is wrong |
|---|---|---|
| `max(1 m, 1 s)`, `min(1 m, 2)`, `max(2, 1 m)`, `max(1 rad, 1)` | `1.0 m` (and so on); `phs check` warns `mixed-extremum` | `extremum` (`physure-script/src/builtins/core.rs`) compares `canonical_magnitude()` of values of different dimension |
| `max(vector(1 m, 2 s))`, `max(x)` for any single `x` | returns the argument unchecked | with one argument the comparison loop never runs |
| `vector(1 m, 2 s)`, `[1 m, 2 s]`, `vector(1, 2 m)` | accepted | a vector of different dimensions has no meaning for `norm`, `dot`, `trapz`, `det`, `matmul`, which already refuse such elements one by one |
| `vector(1 m, 2 m) * vector(1 s, 2 m)` | a vector of `m s` and `m^2` | the elementwise operation (`interpreter/binary_ops.rs`, `collect_vector`) builds a mixed vector |
| `linspace(0 m, 1 s, 3)`, `linspace(0 m, 10, 3)` | accepted | `linspace` (`builtins/array.rs`) reads the means and takes the unit of the first quantity |
| `round(1.5 m, 1 s)`, `round(x, 2 m)` | accepted | the decimals come from a quantity whose unit is ignored |

Not holes (already an error): `dot`, `norm`, `cross` with mismatched dimensions where the result would be wrong, `trapz`, `gradient`, `det`, `matmul`, `+` of vectors, `(1 m .. 5 s)`.

## 2. The rules

1. **`max` / `min`.** Every argument has the same dimension (a plain number is dimensionless). Otherwise an error that names the two:
   `` max of different dimensions: `1 m` is a length and `1 s` a time; compare values of one dimension ``. One argument: a Vector gives its extremum under the same rule (an empty vector is an error); a number or quantity is returned as is; anything else is the existing "expects numbers or quantities". Quantities with uncertainty are still compared by their means; the answer keeps its uncertainty (unchanged). Affine temperatures (`degC`) keep the comparison they have today (the same dimension).
2. **Vectors.** The numeric elements (numbers and quantities) of `vector(...)` and of `[...]` (the parser builds `vector`, `parser/expressions.rs`) share one dimension. Otherwise an error naming the first two that disagree: `` a vector holds one dimension: `1 m` is a length and `2 s` a time; put each dimension in its own vector ``. The same rule applies to the result of an elementwise operation between vectors or a vector and a scalar (`collect_vector`): a result that would be mixed is refused with the same words, naming the operation. Elements that are not numbers or quantities (strings, nested vectors for matrices) are not compared. A vector of a single dimension keeps working exactly as today, including quantities in different units of that dimension (`vector(1 m, 2 km)`).
3. **`linspace(a, b, n)`.** `a` and `b` have the same dimension; the points are in the unit of `a`, with `b` converted to it (so `linspace(0 m, 1 km, 3)` is `0 m, 500 m, 1000 m`: first check what it does today and record it; if it is wrong, that is a bug this change fixes, not a feature). `n` is a plain number (a dimensionless quantity such as `5` is accepted as today, `5 s` is an error).
4. **`round(x, d)`.** `d` is a plain number; `round(1.5 m, 1 s)` is an error. `round(x)` and `round(x, 2)` are unchanged.
5. **The checker** (`check.rs`, code `mixed-extremum`): becomes an **error** (it was the one warning), because the interpreter now refuses the same program; the invariant stays (the checker may miss, it never flags what the interpreter accepts). It still does not see vectors (they are `Unknown` for it): a mixed vector may be missed. `check::explains` knows the new runtime words for `mixed-extremum`, so the report says one problem once (guard `a_flagged_line_is_not_said_twice`).

## 3. What does not change

Single-dimension vectors and matrices; `dot`, `cross`, `norm`, `trapz`, `gradient`, `det`, `matmul` (their refusals stay as they are); the symbolic engine; the Python, WASM and Java bindings (they run the interpreter, so they get the errors; their tests are run and any program that built a mixed vector is fixed in the test).

## 4. Tests and documents

* One test file `physure-script/tests/unit_holes.rs`: every row of section 1 refused with the words of section 2, each valid neighbour still works (`max(1 m, 3 mm)`, `max(5 degC, 3 degC)`, `max(1 m, 2 +/- 1 m)`, `max(100 percent, 2)`, `vector(1 m, 2 km)`, `vector(1, 2)`, a matrix `vector(vector(1 m, 2 m), vector(3 m, 4 m))`, `linspace(0 m, 10 m, 3)`, `round(1.5 m)`, `round(1.2345 m, 2)`), the elementwise result case, and the single-argument cases.
* `tests/check.rs` / `tests/report.rs`: `mixed-extremum` is an error; the guard and a two-problem line.
* The corpus guards (`check_corpus`, the docs guard) stay green; any documented or tested program that used a mixed vector or a mixed `max` is fixed in that program, never the guard.
* CHANGELOG (Changed, a breaking change stated plainly with what to write instead), ROADMAP (the open line about unit holes closed; the fixed-list sentence), the language guide (`phs doc --save` after the generating text in `physure-cli/src/main.rs` changes; the sync test stays green), the audit if it lists these.
* Full runs: Rust suites, `physure` core, Python, WASM, Java.

## 5. Risks

* A program that builds a mixed vector on purpose (a state vector). The owner chose to refuse it; the CHANGELOG says what to write instead (separate vectors).
* A mixed vector built by an operation inside a user function that is never called: refused when called, as every other refusal.
* `max` of an angle and a plain number: in the angle modes, `rad` is a marker dimension (`A`), a plain number is dimensionless: refused as different dimensions (the checker's `mixed-extremum` already says so).

## 6. Not in this change

The code generators' own `max`/`min`/`vector` (JS, Java, Rust emit their libraries' calls); the symbolic engine; the cascade-noise item; `np.mean/dot/cross` in Python.
