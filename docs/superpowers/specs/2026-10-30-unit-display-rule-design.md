# One rule for how a unit is shown (F5) — design

**Date:** 2026-10-30
**Status:** approved by the maintainer on 2026-10-30 (the three decisions below), implemented on `feat/unit-display-rule`.
**Base:** local `main` @ `bf873c2`
**Follows:** the draft `2026-10-30-draft-unit-display-rule.md` (finding F5 of `2026-10-30-post-a1-inventory.md`).

## What was measured first

The draft was written before the strictness pass changed the core's text for a fractional power, so its table was partly stale. What each layer printed on 2026-10-30, before this change:

| | `m^0.5` | `s^(1/3)` | reads back? |
|---|---|---|---|
| `CompoundUnit` / `str(Quantity)` / the core's `simplified` | `m⁰⋅⁵` | `s⁰⋅³³³³³³³³³³³³³³³³` | no: `GHz⁰⋅⁵` is a `ValueError` |
| the extension's `RationalUnit` | `m^0.5`, the text as typed | `s^(1/3)` | yes |
| `Dimension` | `L^(1/2)` | `T^(1/3)` | n/a |
| the core's ASCII form (`__repr__` of a built unit) | `m^(1/2)` | `s^(1/3)` | yes (fixed earlier) |

Two things the draft did not say:

1. **Python formatted powers on its own**, twice: `BaseExponentEntity.__str__` (a `Fraction` as its decimal, through Python's `to_superscript`) and `Dimension._calculate_representation`. The extension did not expose the core's printer, so there were two implementations of one rule, which the project says Python must not have.
2. **A unit and a quantity name things differently**, on purpose or by accident: `str(2 N * 3 m)` is `6.0 J` (the quantity prints the name the catalog gives its unit) while `str(its unit)` is `kg·m²/s²`, and `to_string(use_alias=True)` / `format(unit, "alias")` returned the symbols, because `__format__` never passed the `system` the alias lookup required. `°` printed as `°` for a quantity and as `deg` for the unit.

Found on the way: `µm` (either micro sign, U+00B5 or U+03BC) was an `UnknownUnitError`, while the core's own printer writes `µ` for a micro scale. The catalog has one micro prefix, `u`.

## Decisions (maintainer, 2026-10-30)

| Question | Decision | Rejected |
|---|---|---|
| The form of a fractional power | **`^(n/d)`, exact.** Whole powers stay superscript digits. | `s^0.5` (a third is still a long decimal), keeping `s⁰⋅⁵` (does not read back) |
| Does `str(unit)` show the name a quantity shows? | **No: keep both, and fix `use_alias`.** A unit prints as the symbols it is made of; `to_string(use_alias=True)` and `format(unit, "alias")` give the name, with the active system when none is passed. | `str(unit)` is `J` everywhere (a unit would hide what it is made of) |
| The micro sign | **Yes: `µ` (U+00B5) and `μ` (U+03BC) are the prefix `u`**, in this change. | A separate item |

The rule is stated once: *a unit says what it is made of; a quantity says what it is called; a power is a superscript if it is whole and `^(n/d)` if it is not; and every text a layer writes is a text the parser reads.*

## What was built

| Piece | Change |
|---|---|
| `physure-core/src/units/rational.rs` | `power_text(n, d)`, `unit_text(&[(symbol, (n, d))])` and `formula_text(...)` are the one place a power is formatted; `RationalUnit::symbolic_repr` calls `unit_text`. The decimal `superscript(f64)` is gone. |
| `physure-core/src/units/registry.rs` | `split_prefix_of` reads a leading `µ` or `μ` as `u`. The text a quantity was written with is the text it keeps. |
| `physure-python/src/lib.rs` | `physure._core.unit_text` and `formula_text` expose the two printers. |
| `physure-python/.../base_entity.py`, `dimensions.py` | `BaseExponentEntity.__str__` and `Dimension._calculate_representation` call them; the Python formatting of powers is deleted. |
| `physure-python/.../units.py` | `to_string(use_alias=True)` uses the active system when none is given (and the symbols when there is none); `format(unit, "alias")` therefore works. |
| Tests | `physure-core/tests/unit_text.rs` (8: the rule by example, a dimension, **every one of the catalog's resolvable names is written and read back**, `facts().simplified` is a unit, a third is not a decimal, the micro sign); `physure-python/tests/core/test_unit_text.py` (the rule through Python, read back, the core and Python write the same text, `use_alias`, the micro sign); two existing assertions updated (`m⁰⋅⁵` is `m^(1/2)`). |

## What changes for a user

- **Printed output changes** for a fractional power: `m⁰⋅⁵` is `m^(1/2)`, `s⁰⋅³³³…` is `s^(1/3)`, `1/m⁰⋅⁵` is `1/m^(1/2)`. Whole powers, derived names (`J`, `N`) and the ASCII text the bindings hand over are unchanged. It is listed under Changed in the changelog, not as a fix, because doctests, notebooks and error messages that show a fractional power change with it.
- `µm`, `µs`, `µF`, `µg` parse.
- `format(unit, "alias")` works.

## Not done

- **`°` versus `deg`** is the rule, not a bug: a unit prints the symbol the system stores (`deg`), a quantity prints the catalog's name for it (`°`). The core keeps the name a unit was *typed* with (`display_name`); `CompoundUnit` is built from symbols and does not, so `Q_(1, "°").unit` is `deg`. Keeping the typed name would change the flyweight key and the hot path (option C of the draft) and was not asked for.
- The Python lexer still reads `⁰⋅⁵` (`parse_superscript`), which is harmless: it accepts more than the printer writes.
- Not run in CI; the extension was rebuilt and the Python suite run locally.
