# Draft: one rule for how a unit is shown (F5)

**Status:** DRAFT, not approved. Source: finding F5 of `2026-10-30-post-a1-inventory.md`. The A1 cleanup deferred this on purpose ("what a unit shows as is a design of its own").

## Problem

The same unit is written in different ways depending on which layer prints it.

| Where | Same fractional power `s^(1/2)` shows as |
|---|---|
| Core, `physure-core/src/units/rational.rs` (the documented rule: "`0.5` is `⁰⋅⁵`") | `s⁰⋅⁵` |
| The extension's Python class `RationalUnit` (`str()`) | `m^1/2 * s^-1/3` |
| `Dimension` (`str()`) | `T^(1/2)` |
| `CompoundUnit` (`BaseExponentEntity.__str__`) | `s⁰⋅⁵`, and a third becomes `m⁰⋅³³³³³³³³³³³³³³³³` (the decimal of a rational that is now exact) |

And a display name is lost on the way:

- The core keeps the name a unit was written with (`display_name`): `facts("°")["simplified"]` is `°`, and `facts("E°")["simplified"]` is `E°`.
- Python builds a `CompoundUnit` out of symbols, so the same units come back as `deg` and `Edeg`.
- The parity tests compare only names that are valid Python identifiers (that is what `dir(units)` lists), so this difference has never been checked.

## Why it matters

Unit strings appear in doctests, notebooks, plots, error messages and the REPL. A change of format is visible to users; so a decision made late, after users depend on a format, is a breaking change.

## Options

- **A. Leave it.** Cheapest. The three forms stay.
- **B. One display function in the core, called by Python.** `str(unit)` for a core-backed unit and for `CompoundUnit` ask the registry/unit for its string with a few options (superscript or caret, decimal or fraction). Python no longer decides. Cost: a Python-side `str()` of a `CompoundUnit` needs a registry to find a display name, and units are printed in many places without one in hand (the active system is available through the context variable).
- **C. Keep the display name in the unit.** `CompoundUnit` stores symbol plus the name it was written with. Changes the flyweight key (`(symbol, power)` pairs today) and equality (`°` and `deg` must stay equal), so it is the larger change, and it touches the hot path that A1 just made 37% faster.

## Recommendation

B, with the canonical forms decided by the maintainer first. If B is done, C is not needed: the display name is looked up from the registry at print time instead of being stored.

## Open questions

- Canonical form of a fractional power: `s⁰⋅⁵` (the core's rule, lossy for thirds), `s^(1/2)` (what `Dimension` shows, exact), or `s^0.5`? An exact form is the only one that cannot mislead for 1/3.
- Should `°` print as `°` or `deg`? Which one a user typed, or which one the catalog lists first?
- Is a change of printed format allowed in a minor version (0.4.0), given that doctests and the documentation print units?

## Acceptance

- One test per row of the table above asserts the same string in the core, in `RationalUnit`, in `CompoundUnit` and in `Dimension` (for dimensions, the symbols differ but the power format does not).
- The parity suites compare the display of names that are not identifiers as well.
- Doctests and documentation are regenerated, and the changelog lists the format change as breaking.

## Size and dependencies

Size M, risk medium (user-visible output). Independent; if N1 (package renames) goes first, the work is done on the new names.
