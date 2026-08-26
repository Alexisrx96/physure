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
mass                    = 2.0 ± 0.1 kg
kinetic_energy          = 625.0 ± 40.0195264839553 J
```

## 4. `where` for showing intermediate steps

```phs
force = mass * acceleration where acceleration = 9.81 m / s^2
```

`where` binds a name for use in the expression to its left without a separate assignment line
above it — useful for a constant that only this one calculation needs, kept visible right next
to where it's used instead of buried earlier in the script:

```
force                   = 19.62 ± 0.981 N
```

Every value computed so far can be interpolated straight into a string, for a one-line summary
of the whole calculation:

```phs
"The pressure is {pressure_bar}, the kinetic energy is {kinetic_energy}, and the force is {force}"
```

```
The pressure is 1.0 bar, the kinetic energy is 625.0 ± 40.0195264839553 J, and the force is 19.62 ± 0.981 N
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
