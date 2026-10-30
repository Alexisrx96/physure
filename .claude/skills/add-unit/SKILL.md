---
name: add-unit
description: Add a unit, constant, prefix, or dimension to physure's .conf catalog. Use whenever asked to add/rename/alias a unit or physical constant — the alias-collision check and doc regeneration are mandatory steps that are easy to forget.
---

# Adding a unit or constant

All catalog entries live in `physure-core/src/units/physure.conf`
(sections `[Dimensions]`, `[Prefixes]`, `[Units]`, `[Constants]`) — this is
the **only** file to hand-edit. There is no separate `systems/` directory on
the Rust side; SI vs. imperial base-unit selection is hardcoded in
`physure-core/src/units/registry.rs` (`build_default_si` /
`build_default_imperial`), not read from a `.conf` file. The Python package
has only `physure-python/physure/infrastructure/config/systems/` (small
per-system overrides such as `imperial.conf`); its base catalog is this same
file, embedded in the extension and handed to Python by `_core.default_conf()`.

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
   grep -rn "SYMBOL" physure-core/src/units/*.conf 2>/dev/null
   ```

   Also consider prefix + existing-symbol clashes for short symbols.

2. **Add the entry** in `physure-core/src/units/physure.conf`, in the
   appropriate section, keeping the file's grouping/comments.

3. **Rebuild the extension** (the catalog is compiled in with `include_str!`
   and is also what the Python `UnitSystem` is built from):
   `cd physure-python && uv run maturin develop --release && cd ..`

4. **Verify no redefinition warnings at bootstrap** and that the unit
   resolves:

   ```bash
   cd physure-python && uv run python -c "
   import logging; logging.basicConfig(level=logging.WARNING)
   from physure import Q_
   print(Q_(1, 'NEWSYMBOL'))"
   ```

   Any `is being redefined` warning means step 1 was missed — fix before
   continuing.

5. **Regenerate the units reference** (docs/UNITS.md is generated, never
   hand-edit) — confirm the actual generator script still exists at
   `physure-python/scripts/generate_units_readme.py` (path may have moved
   along with the crate split) before running it. The script resolves its
   input `.conf` path relative to its own CWD, so it must be run from
   `physure-python/`, not the repo root:

   ```bash
   cd physure-python
   uv run python scripts/generate_units_readme.py # docs/UNITS.md
   uv run python -m physure.cli sync-types            # _generated_types.py, registry.pyi, units/__init__.pyi
   cd ..
   ```

6. **Run the tests**: `uv run pytest tests/ -x -q`. Add a conversion test
   if the unit has a nontrivial factor or offset.
