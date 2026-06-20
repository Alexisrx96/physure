# UX/DX Improvements — Design Spec

**Date:** 2026-06-19
**Scope:** Three targeted fixes to improve developer and user experience in `measurekit`.

---

## A — Better Error Messages

**Problem:** `ValueError: Unknown dimension for unit 'xyz'` gives no guidance on what went wrong or what to use instead.

**Fix:** Replace the bare `ValueError` at `units.py:165` with a new `UnknownUnitError` subclass of `ValueError` that includes `difflib.get_close_matches` suggestions from the active system's unit registry.

```python
# Before
raise ValueError(f"Unknown dimension for unit '{unit_name}'")

# After
suggestions = difflib.get_close_matches(unit_name, known_units, n=3, cutoff=0.6)
hint = f" Did you mean: {', '.join(suggestions)}?" if suggestions else ""
raise UnknownUnitError(f"Unknown unit '{unit_name}'.{hint}")
```

`UnknownUnitError` lives alongside `IncompatibleUnitsError` in `measurekit/domain/measurement/errors.py`. It subclasses `ValueError` so existing `except ValueError` handlers are not broken.

**Files touched:** `measurekit/domain/measurement/units.py`, `measurekit/domain/measurement/errors.py`

---

## B — Jupyter Display

**Problem:** `Quantity` has `_repr_latex_` but no `_repr_html_` or `_repr_mimebundle_`, so Jupyter notebooks can't render a rich output.

**Fix:** Add two methods to `Quantity`, near the existing `_repr_latex_`:

```python
def _repr_html_(self) -> str:
    unit_latex = self.unit.to_latex()
    return (
        f'<span style="font-family:monospace">'
        f'{self.magnitude} '
        f'<span style="color:#888">{unit_latex or "dimensionless"}</span>'
        f'</span>'
    )

def _repr_mimebundle_(self, **kwargs):
    return {
        "text/plain": repr(self),
        "text/latex": self._repr_latex_(),
        "text/html": self._repr_html_(),
    }
```

Jupyter picks the best available MIME type. LaTeX-capable frontends keep using `_repr_latex_`; basic HTML frontends get `_repr_html_`.

**Files touched:** `measurekit/domain/measurement/quantity.py`

---

## C — Ergonomic Aliases

**Problem:** Accessing magnitude and unit requires `.magnitude` / `.unit`. No unpacking support.

**Fix:** Add `.m`, `.u`, and `__iter__` to `Quantity`:

```python
@property
def m(self) -> ValueType:
    return self.magnitude  # pint-compatible alias

@property
def u(self) -> CompoundUnit:
    return self.unit

def __iter__(self):
    yield self.magnitude
    yield self.unit
```

Enables:
```python
q = Q_(9.8, 'm/s^2')
q.m            # 9.8
q.u            # CompoundUnit
mag, unit = q  # unpack
```

Naming follows `pint` convention. `__iter__` is open — `list(q)` yields `[magnitude, unit]`.

**Files touched:** `measurekit/domain/measurement/quantity.py`

---

## What is NOT in scope

- Rich/ANSI terminal display
- Full error hierarchy with doc links
- Interactive `units` explorer
- Complex HTML tables for Jupyter

Add any of these when there's a concrete use case.
