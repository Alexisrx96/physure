---
name: add-unit
description: Add a unit, constant, prefix, or dimension to physure's .conf catalog. Use whenever asked to add/rename/alias a unit or physical constant — the alias-collision check and doc regeneration are mandatory steps that are easy to forget.
---

# Adding a unit or constant

All catalog entries live in `physure-core/src/units/physure.conf`
(sections `[Dimensions]`, `[Prefixes]`, `[Units]`, `[Constants]`) — this is
the **only** file to hand-edit. System-specific base-unit choices live in
`physure-core/src/units/systems/international.conf` and `imperial.conf` if
present, otherwise check `physure-core/src/units/` for the current layout.

**Known issue:** `physure-python/physure/infrastructure/config/physure.conf`
is a second, hand-maintained copy of this catalog that has already
drifted out of sync with the Rust one (missing temperature-scale units and
`%` as of 2026-08). Do not edit it to "keep it in sync" — that's a
standing architecture problem tracked separately, not something to
band-aid per-unit. If your change needs to show up in the pure-Python
fallback path today, say so explicitly rather than silently patching both
files.

## Formats (copy a neighboring line and adapt)

```ini
# [Units] — name = factor, DIMENSION, [aliases...]
meter    = 1.0, L, [m, meter, metro, metros]
# Some units have extra fields (offset, noprefix, etc.) — match the nearest existing example.

# [Constants] — name = value unit_expression
avogadro_constant = 6.022141e+23 mol^-1
```

## Mandatory steps, in order

1. **Collision check FIRST.** Every symbol and alias shares one namespace,
   prefixes generate more (e.g. `p` + `H` = pico-Henry vs `pH`). The
   registry only *logs a warning* on redefinition and the later
   definition silently wins — this caused the `gal` gallon/galileo bug
   (PR #17). Check every new symbol/alias:

   ```bash
   grep -rn "SYMBOL" physure-core/src/units/*.conf physure-core/src/units/systems/*.conf 2>/dev/null
   ```

   Also consider prefix + existing-symbol clashes for short symbols.

2. **Add the entry** in `physure-core/src/units/physure.conf`, in the
   appropriate section, keeping the file's grouping/comments.

3. **Rebuild the Rust core** (config is compiled in, not read at runtime
   for the Rust-backed path): `cd physure-core && maturin develop && cd ..`

4. **Verify no redefinition warnings at bootstrap** and that the unit
   resolves:

   ```bash
   uv run python -c "
   import logging; logging.basicConfig(level=logging.WARNING)
   from physure import Q_
   print(Q_(1, 'NEWSYMBOL'))"
   ```

   Any `is being redefined` warning means step 1 was missed — fix before
   continuing.

5. **Regenerate the units reference** (docs/UNITS.md is generated, never
   hand-edit) — confirm the actual generator script still exists at
   `physure-python/scripts/generate_units_readme.py` (path may have moved along with the
   crate split) before running it:

   ```bash
   uv run python physure-python/scripts/generate_units_readme.py
   ```

6. **Run the tests**: `uv run pytest tests/ -x -q`. Add a conversion test
   if the unit has a nontrivial factor or offset.
