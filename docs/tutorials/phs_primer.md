# PHS primer

A first lesson in PhysureScript (PHS): a small language for writing a physics calculation the
way you'd write it on paper, with units, uncertainty, and conversions checked as you go. This
guide walks through [`phs_primer.phs`](phs_primer.phs), a runnable companion script — every
output quoted below is copied from actually running it, not typed from memory.

If you have `phs` installed, run the script alongside this guide:

```bash
phs docs/tutorials/phs_primer.phs
```

## What PHS is and isn't

Engineering and lab teams already have the physics right. What breaks is the handoff: a
formula written on a whiteboard or in a spreadsheet gets re-typed into application code by a
developer who doesn't know the domain, units get dropped or silently mismatched, and nobody
notices until a number is off by a factor of 1000. PHS closes that gap: the person who knows
the physics writes and runs the calculation themselves, with real units enforced by the
interpreter, and the resulting `.phs` script *is* the spec — handed to a dev team as-is, as a
rendered HTML report, or as transpiled starting code. PHS isn't a general-purpose programming
language, and it isn't trying to replace one; it's a calculator that refuses to let a unit
mistake pass silently.

## 1. A quantity is a number with a unit

```phs
pressure = 100.0 kPa
```

Write the magnitude and the unit exactly like you would on paper — no `Quantity(...)`
constructor, no import. Running it prints:

```
pressure                = 100.0 kPa
```

## 2. Conversion with `=>`

```phs
pressure_bar = pressure => bar
```

`=>` converts a quantity to a different unit *of the same dimension* on the spot — no manual
conversion factor to look up or get wrong:

```
pressure_bar             = 1.0 bar
```

Converting to a dimensionally incompatible unit (say, `pressure => kg`) is a hard error, not a
silently wrong number — see [Break it on purpose](#break-it-on-purpose) below.

## 3. Uncertainty with `+/-`

```phs
velocity = 25.0 +/- 0.5 m / s
mass = 2.0 +/- 0.1 kg
kinetic_energy = 0.5 * mass * velocity^2
```

A measurement's uncertainty isn't a comment or a separate column — it's part of the value, and
it propagates automatically through every operation that uses it. Squaring `velocity` and
multiplying by `mass` widens the error bar exactly the way the underlying statistics say it
should, with no separate calculation to keep in sync:

```
velocity                = 25.0 ± 0.5 m / s
mass                    = 2.00 ± 0.10 kg
kinetic_energy          = 630 ± 40 J
```

The displayed digits are also correlated to the uncertainty itself — showing an uncertainty to
fifteen decimal places would be its own false-precision claim, so PHS rounds it to 1-2
significant figures and rounds the value to match (`mass`'s `0.1` keeps 2 figures here, since
a leading `1` would otherwise round to a coarse `±0.2`). The full, unrounded value is never
lost — it stays available for every further calculation, and an explicit format spec like
`kinetic_energy:.6f` still shows exactly the precision you ask for.

## 4. `where` for showing intermediate steps

```phs
force = mass * acceleration where acceleration = 9.81 m / s^2
```

`where` binds a name for use in the expression to its left without a separate assignment line
above it — useful for a constant that only this one calculation needs, kept visible right next
to where it's used instead of buried earlier in the script:

```
force                   = 19.62 ± 0.98 N
```

Every value computed so far can be interpolated straight into a string, for a one-line summary
of the whole calculation:

```phs
"The pressure is {pressure_bar}, the kinetic energy is {kinetic_energy}, and the force is {force}"
```

```
The pressure is 1.0 bar, the kinetic energy is 630 ± 40 J, and the force is 19.62 ± 0.98 N
```

## 5. The closing deliverable: `phs script.phs --html`

```bash
phs docs/tutorials/phs_primer.phs --html
```

This runs the script exactly as before, but also writes a self-contained HTML report next to
it (`phs_primer.html`) — every quantity, every intermediate step, every unit and uncertainty,
laid out as a single offline document. Think of it as a photographed whiteboard: not just the
final answer, but the whole derivation, in a form you can attach to an email or a ticket
without anyone having to re-run anything to trust it. The report *is* the audit trail.
 
## 6. Booleans, `not`/`and`/`or`, and assertions

Comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`, `≈`) return a real boolean, `True` or `False`.
There is no implicit truthiness: a quantity, a number, or a string is never treated as a
condition on its own.

```phs
pressure > 0 Pa
count != 0
not (sensor_disconnected)
denominator != 0 and numerator / denominator > limit
```

`and` and `or` short-circuit: `False and rhs` and `True or rhs` never evaluate `rhs` at all,
so a guard like the denominator check above is safe even when `rhs` would otherwise raise.
`not` binds tighter than `and`, which binds tighter than `or` — parenthesize a mixed condition
so a later reader doesn't have to work it out:

```phs
(not (pressure > limit) and enabled) or override
```

`assert` takes either a boolean condition (with an optional message) or two quantities to
compare directly:

```phs
assert(power > 0 kW)
assert(pressure >= minimum_pressure, "V-PUMP-004: pressure is below the operating range")
assert(actual, expected)          # existing form: dimensional + magnitude tolerance check
exact_assert(actual, expected)    # existing form: exact unit and magnitude match
```

`assert(actual, expected)` and `exact_assert(actual, expected)` still expect two `Quantity`
values — `assert(actual == expected)` is the boolean form, and gives a less specific failure
message (it doesn't know *how much* the two differ, only that they weren't equal), so prefer
naming the comparison you actually mean.

Prefer several small assertions with descriptive messages over one large compound condition —
a boolean built from named domain predicates (`is_within_tolerance and is_powered`) reads
better and fails with a clearer message than one long inline expression.

A sigma-bound condition like `assert(result == reference +/- 2 sigma)` parses and runs in the
interpreter, but its behavior is not yet identical across every transpile target (see
`docs/uncertainty-gum-compliance.md`) — use `assert(actual, expected)` instead when the check
needs to produce the same result in every generated language.

## Break it on purpose

PHS's whole job is refusing to guess when a calculation doesn't make physical sense. Two
examples worth deliberately breaking, so you recognize the error the first time you meet it for
real instead of during a deadline.

**A dimension mismatch:**

```phs
5 m + 2 kg
```

```
Error Details: Unit mismatch: expected 'm', got 'kg'
```

You can't add a length to a mass — that's not a PHS limitation, it's the calculation itself
being wrong. This error means the formula's dimensions don't work out, which is worth stopping
to fix before trusting anything downstream of it.

**A missing operator:**

```phs
mass = 2.0 kg
velocity = 3.0 m/s
total = mass velocity
```

```
Error Details: Missing operator between 'mass' and 'velocity': PhysureScript does not read two
bare names side by side as a product. Write `mass * velocity` if you meant to multiply them,
or add whatever operator belongs between them.
```

`mass velocity` with the `*` left out by accident isn't read as a product — PHS has caught
exactly the typo that, in a language willing to guess, would have silently multiplied two
values with no warning at all.
