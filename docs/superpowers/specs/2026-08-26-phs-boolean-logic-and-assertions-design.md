# PHS boolean logic and condition assertions

**Status:** Proposed follow-up to the implemented Quantity-only assertion contract
**Audience:** PHS maintainers, binding maintainers, and authors of engineering validation scripts

## Context

PHS comparisons now return a real `PhsValue::Bool`. This corrected the interpreter's former
dimensionless `Quantity(1.0 | 0.0)` stand-in and aligned comparison results with Python, Rust,
Java, JavaScript, and TypeScript.

The existing assertion API predates that change. It accepts only two quantities:

```phs
assert(actual, expected)
exact_assert(actual, expected)
```

Consequently, the most familiar validation form fails:

```phs
assert(actual == expected)
```

PHS also lacks boolean literals and logical operators. `PhsValue::Bool` exists at runtime, but
there is no source syntax for `True`, `False`, `not`, `and`, or `or`. This leaves comparisons
without a complete language surface and prevents validation scripts from expressing ordinary
engineering conditions cleanly.

This design completes that surface while preserving the existing Quantity assertion behavior.
It is a local extension of builtin dispatch by arity and argument types; it does not introduce
general user-defined function overloading.

## Goals

1. Make `assert(condition)` the primary assertion form for boolean conditions.
2. Add strict boolean literals and `not`, `and`, and `or` operators.
3. Guarantee short-circuit evaluation for `and` and `or`.
4. Preserve `assert(Quantity, Quantity)` and `exact_assert(Quantity, Quantity)` unchanged.
5. Allow an optional diagnostic message for boolean assertions.
6. Preserve behavior across the interpreter and every supported transpile target.
7. Support named boolean intermediate values in generated code for the common, syntactically
   evident cases used by validation scripts.

## Non-goals

- General overload resolution for user-defined functions.
- Implicit truthiness for quantities, numbers, strings, vectors, or other values.
- `and` and `or` returning one of their operands as Python does.
- Boolean arithmetic or ordering comparisons such as `True > False`.
- A configurable numerical tolerance for `assert(actual, expected)`.
- A test discovery runner, scenario coverage report, or code coverage collector.
- Correcting sigma-bound code generation or the interpreter's current `0.05` fallback; those
  are tracked as a separate uncertainty-validation requirement below.
- Inferring boolean return types for arbitrary user-defined functions.

## Public language contract

### Boolean literals

PHS adds two case-sensitive literals:

```phs
True
False
```

They evaluate to `PhsValue::Bool(true)` and `PhsValue::Bool(false)`. Their spelling matches the
interpreter's existing `Bool` display and the training-friendly Python convention. Lowercase
aliases are not added in this version; one canonical spelling avoids two styles in shared
engineering models.

`True` and `False` become reserved keywords. Scripts that currently use either spelling as an
identifier receive a parse error and must rename that identifier.

### Logical operators

PHS adds:

```phs
not condition
left and right
left or right
```

All operands must be `Bool`, and every logical expression returns `Bool`.

```phs
not False                 # True
True and False            # False
True or False             # True
5 m and 3 m               # type error
not 5                     # type error
```

There is no implicit conversion from `Quantity`, `Number`, `String`, or a collection to Bool.
An engineering condition must state its comparison explicitly:

```phs
pressure > 0 Pa
count != 0
```

### Precedence and associativity

The logical layer is inserted after the existing non-logical expression and comparison layer,
without changing the current arithmetic, conversion, range, formatting, ternary, or `where`
precedence.

From tighter to looser binding, the new portion is:

1. existing arithmetic, conversion, and comparison expressions;
2. `not`;
3. `and`;
4. `or`;
5. existing ternary and `where` tails.

`and` and `or` are left-associative. Repeated `not` is valid. Therefore:

```phs
not pressure > limit and enabled or override
```

means:

```phs
((not (pressure > limit)) and enabled) or override
```

Documentation should still recommend parentheses when mixing `not`, `and`, and `or` in a
condition that an occasional programmer must review.

### Short-circuit evaluation

`and` and `or` must short-circuit:

```phs
denominator != 0 and numerator / denominator > limit
sensor_disconnected or reading < limit
```

- `False and rhs` returns `False` without evaluating `rhs`.
- `True or rhs` returns `True` without evaluating `rhs`.
- `True and rhs` and `False or rhs` evaluate `rhs` and require it to be Bool.
- A non-Bool left operand is an immediate type error; the right operand is not evaluated.

Short-circuiting is language semantics, not an optimization. The interpreter must implement it
before ordinary eager builtin argument evaluation.

### Boolean equality

`==` and `!=` accept two booleans and return Bool:

```phs
True == True
flag != False
```

The preferred false assertion remains `assert(not flag)`, but boolean equality is part of a
complete Bool type and is needed outside assertions. Mixed `Bool`/non-Bool equality is a type
error. `<`, `<=`, `>`, `>=`, and `≈` remain invalid for booleans.

## Assertion overloads

The builtin dispatcher recognizes these signatures exactly:

```text
assert(Bool) -> None
assert(Bool, String) -> None
assert(Quantity, Quantity) -> None
exact_assert(Quantity, Quantity) -> None
```

No other overload is accepted.

### Boolean assertion

```phs
assert(power > 0 kW)
assert(not outside_operating_range)
```

- `True` returns `None` and execution continues.
- `False` raises `PhysureError::AssertionFailed { kind: "assert", ... }`.
- A one-argument call with anything other than Bool raises a type error rather than applying
  truthiness.

### Boolean assertion with a message

```phs
assert(
    pressure >= minimum_pressure,
    "V-PUMP-004: pressure is below the operating range"
)
```

The message is included in the assertion failure. It does not replace the CLI's file, source
line, or failing statement context. A successful assertion does not print the message.

The second argument must evaluate to `String`. `assert(Bool, Quantity)` and
`assert(Quantity, String)` are type errors.

### Existing Quantity assertions

These forms retain their current semantics:

```phs
assert(actual, expected)
exact_assert(actual, expected)
```

- `assert(Quantity, Quantity)` uses the existing fixed relative and absolute tolerances after
  checking dimensional compatibility and converting scale.
- `exact_assert(Quantity, Quantity)` requires the existing exact unit and magnitude contract.
- `exact_assert` does not gain a Bool overload. Boolean equality is expressed as
  `assert(left == right)`.

This type-based dispatch is intentionally local to the builtin. It provides an implementation
pattern that future overload work may study, but it does not establish a public generic
overload-resolution algorithm.

### Statement-only rule

All assertion forms remain standalone statements:

```phs
assert(condition)
```

The following remain invalid:

```phs
x = assert(condition)
f(assert(condition))
```

An assertion performs control flow and produces no usable value.

## Parser and AST design

### Grammar

`True`, `False`, `not`, `and`, and `or` are added to the keyword set so they cannot be consumed
as units or identifiers.

The grammar adds boolean literals and three logical precedence tiers above the current
non-logical expression parser. The parser must accept line breaks around `and` and `or` wherever
the existing expression newline rules allow them, so long validation conditions can remain
readable.

### AST representation

Add one literal variant:

```rust
Expr::Bool(bool)
```

Reuse the existing operator-desugaring pattern for logical expressions:

```text
not x   -> FunctionCall("op_not", [x])
a and b -> FunctionCall("op_and", [a, b])
a or b  -> FunctionCall("op_or", [a, b])
```

This matches the current representation of comparison operators and avoids adding three more
AST node families. `op_and` and `op_or` are nevertheless special forms in the interpreter
because ordinary function calls evaluate all arguments eagerly.

The shared codegen module adds helpers that recognize Bool literals, logical forms, boolean
comparisons, and assertion call shapes. Target generators should not rediscover these shapes
independently.

## Interpreter design

### Literal evaluation

`Expr::Bool(value)` evaluates directly to `PhsValue::Bool(value)`.

### Logical evaluation

`eval_expr` intercepts `op_not`, `op_and`, and `op_or` before generic function-call argument
evaluation:

- `op_not` evaluates one operand and requires Bool.
- `op_and` evaluates the left operand first and returns early when it is `False`.
- `op_or` evaluates the left operand first and returns early when it is `True`.
- When the right operand is required, it is evaluated once and must be Bool.

The existing shared `is_truthy` helper remains available for historical control-flow paths,
but the new logical operators and `assert(Bool)` must not call it. Strict Bool checking prevents
quantities such as `1 m` from silently becoming conditions.

### Equality dispatch

The `op_==` and `op_!=` builtin paths add a Bool/Bool branch before Quantity comparison. The
existing Quantity and sigma-bound branches remain in place. Every other mixed or unsupported
pair produces a diagnostic that names both encountered types.

### Assertion dispatch

The assertion builtin dispatches first by name, then arity, then argument types. Error messages
list the accepted signatures rather than reporting only “expects two quantities.”

Illustrative diagnostic:

```text
assert received (Quantity); expected assert(Bool), assert(Bool, String),
or assert(Quantity, Quantity)
```

The existing `PhysureError::AssertionFailed` variant is reused for a false condition. Parser,
arity, and type errors remain ordinary evaluation errors because the assertion itself was not
validly formed.

## Transpiler design

All targets emit native short-circuit boolean operators:

| PHS | Python | Rust | Java | JavaScript / TypeScript |
|---|---|---|---|---|
| `True` | `True` | `true` | `true` | `true` |
| `False` | `False` | `false` | `false` | `false` |
| `not x` | `not x` | `!x` | `!x` | `!x` |
| `a and b` | `a and b` | `a && b` | `a && b` | `a && b` |
| `a or b` | `a or b` | `a || b` | `a || b` | `a || b` |

Parentheses are emitted around logical expressions so target precedence cannot change the PHS
meaning.

### Assertion emission

Generated assertions must remain active in production configurations:

| Target | Boolean assertion shape |
|---|---|
| Python | `if not condition: raise AssertionError(message)` |
| Rust | `assert!(condition, "message")` |
| Java | `if (!condition) throw new AssertionError(message);` |
| JavaScript / TypeScript | `if (!condition) throw new Error(message);` |

Python must not emit the `assert` statement because `python -O` removes it. Java must not emit
the language `assert` statement because JVM assertions may be disabled. Validation behavior may
not depend on runtime flags.

The existing two-Quantity assertion emission remains unchanged except where Python currently
uses a removable `assert` statement; Python must also switch that path to an explicit
conditional raise.

### Named boolean assignments

Python, Rust, and untyped JavaScript infer local boolean types naturally. Java and typed
TypeScript currently assume most non-string assignments are `Quantity`, so they need a small
shared “definitely boolean expression” classifier.

The classifier recognizes:

- `Expr::Bool`;
- comparison expressions;
- `op_not`, `op_and`, and `op_or`;
- identifiers previously assigned one of those recognized boolean expressions.

Java emits `boolean` and typed TypeScript emits `boolean` for those assignments. A reassignment
that changes a known boolean variable to another value category is a code-generation error.

This is deliberately not a general type checker. Boolean-returning user functions and complex
cross-scope inference remain deferred until PHS has an explicit function return-type design.

## Uncertainty-boundary requirement

This design makes the interpreter form syntactically valid:

```phs
assert(result == reference +/- 2 sigma)
```

It does not make the current sigma-bound implementation suitable for cross-language validation.
Two known issues remain explicit:

1. when `result` has no uncertainty, the interpreter substitutes `0.05`; a rigorous validator
   should report that a sigma comparison requires uncertainty instead of inventing one; and
2. current code generation treats the sigma-marked quantity as an ordinary quantity and does
   not preserve the interpreter's bound semantics.

Until a separate sigma-validation design fixes both issues, training material must label this
assertion form as interpreter-only and must use `assert(actual, expected)` for deterministic
cross-target validation. Documentation must not claim transpiler parity for sigma-bound
conditions.

## Testing strategy

### Parser tests

- Parse `True` and `False` as `Expr::Bool`.
- Reserve both literal spellings against identifiers and units.
- Verify `not`, `and`, and `or` precedence and left associativity.
- Verify multiline logical conditions.
- Reject missing operands and invalid operator placement.

### Interpreter tests

- Truth table for `not`, `and`, and `or`.
- Bool equality and inequality.
- Type errors for every non-Bool logical operand.
- Short-circuit guards using a right-hand expression that would divide by zero if evaluated.
- `assert(True)` passes and returns `None`.
- `assert(False)` raises `AssertionFailed`.
- `assert(False, "scenario id")` includes the message.
- Invalid assertion signatures produce the accepted-signature diagnostic.
- Existing approximate and exact Quantity assertion tests continue to pass unchanged.
- `assert(actual == expected)` works for compatible converted units.
- A false sigma-bound comparison reaches `assert(Bool)` and fails in the interpreter.

### Codegen tests

For Python, Rust, Java, JavaScript, and TypeScript:

- literal and logical operator emission;
- parentheses preserve PHS precedence;
- one-argument boolean assertion emission;
- boolean assertion with a message;
- named Bool assignment emission;
- false generated assertion terminates the program;
- short-circuit generated expression does not evaluate its invalid branch;
- existing Quantity assertion output and execution still pass;
- nested or assigned assertions remain rejected.

Generated Python must be tested under both ordinary execution and `python -O` to prove that
assertion failures cannot be optimized away. Java tests run without `-ea` to prove the same
property.

### Regression tests

The comparison regression suite keeps its existing requirement that comparison results are
real `PhsValue::Bool`. New assertion tests must use those values directly rather than rebuilding
a dimensionless Quantity stand-in.

## Documentation updates

After implementation:

1. update the PHS language reference with literals, logical operators, precedence, strict Bool
   typing, and short-circuit behavior;
2. update the assertion reference with all four accepted signatures;
3. add deterministic examples using separate assertions for better diagnostics;
4. document compound conditions as appropriate for named domain predicates, not as a reason to
   compress every validation into one assertion; and
5. document the sigma-bound parity limitation next to every uncertainty-validation example.

## Compatibility and migration

- Existing `assert(Quantity, Quantity)` and `exact_assert(Quantity, Quantity)` scripts continue
  to work without source changes.
- Comparisons continue to return `PhsValue::Bool`.
- Existing variables named exactly `True` or `False` become invalid and must be renamed.
- Existing generated Python assertions become reliable under `python -O`; this is an intentional
  strengthening, not a source-language change.
- No numeric or Quantity truthiness is introduced.

## Acceptance criteria

The design is implemented when:

1. the interpreter accepts and correctly evaluates all specified Bool syntax;
2. `assert(Bool)` and `assert(Bool, String)` pass and fail with actionable diagnostics;
3. existing Quantity assertion behavior remains green;
4. short-circuit behavior is proven by tests;
5. every target emits equivalent boolean and assertion behavior;
6. named boolean assignments used by validation scripts compile in every target;
7. Python assertions fail under `python -O` and Java assertions fail without `-ea`; and
8. sigma-bound limitations remain explicitly documented and are not presented as cross-target
   compatible.
