---
name: add-unit
description: Add a unit, constant, prefix, or dimension to measurekit's .conf catalog. Use whenever asked to add/rename/alias a unit or physical constant — the alias-collision check and doc regeneration are mandatory steps that are easy to forget.
---

# Adding a unit or constant

All catalog entries live in `measurekit/infrastructure/config/measurekit.conf`
(sections `[Dimensions]`, `[Prefixes]`, `[Units]`, `[Constants]`). System-specific
base-unit choices live in `measurekit/infrastructure/config/systems/international.conf`
and `imperial.conf`.

## Formats (copy a neighboring line and adapt)

```ini
# [Units] — name = factor, DIMENSION, [aliases...]
meter    = 1.0, L, [m, meter, metro, metros]
# Some units have extra fields (offset, noprefix, etc.) — match the nearest existing example.

# [Constants] — name = value unit_expression
avogadro_constant = 6.022141e+23 mol^-1
```

## Mandatory steps, in order

1. **Collision check FIRST.** Every symbol and alias shares one namespace, prefixes
   generate more (e.g. `p` + `H` = pico-Henry vs `pH`). The registry only *logs a
   warning* on redefinition and the later definition silently wins — this caused the
   `gal` gallon/galileo bug (PR #17). Check every new symbol/alias:

   ```bash
   grep -rn "SYMBOL" measurekit/infrastructure/config/*.conf measurekit/infrastructure/config/systems/*.conf
   ```

   Also consider prefix + existing-symbol clashes for short symbols.

2. **Add the entry** in the appropriate section, keeping the file's grouping/comments.

3. **Verify no redefinition warnings at bootstrap** and that the unit resolves:

   ```bash
   uv run python -c "
   import logging; logging.basicConfig(level=logging.WARNING)
   from measurekit import Q_
   print(Q_(1, 'NEWSYMBOL'))"
   ```

   Any `is being redefined` warning means step 1 was missed — fix before continuing.

4. **Regenerate the units reference** (docs/UNITS.md is generated, never hand-edit):

   ```bash
   uv run python scripts/generate_units_readme.py
   ```

5. **Run the tests**: `uv run pytest tests/ -x -q`. Add a conversion test if the unit
   has a nontrivial factor or offset.
