# Draft: the layering exceptions that outlived A1 (F2)

**Status:** DRAFT, not approved. Source: finding F2 of `2026-10-30-post-a1-inventory.md`. Related: section "A2b: honest names" of `2026-10-27-a2-layering-contract-design.md`.

## Problem

`physure-python/tests/core/import_layers.toml` records the imports that break the layer contract today, and a test fails if a record stops matching the code, so the list can only shrink. Its header says `cause = "a1"` means "the module involved is deleted by the A1 work". A1 is finished and none of those modules was deleted (`CompoundUnit` and `Dimension` stayed, as flyweights over the extension's `RationalUnit`), so six of the seven records name a cause that no longer exists. The seventh is `cause = "release"`.

## The seven records

| # | Importer | Imports | Kind | What it needs |
|---|---|---|---|---|
| 1 | `domain.measurement.system` | `application.factories` (`QuantityFactory`, used for `self.Q_`) | top | the system builds its quantity factory; the factory belongs with the model or must be injected |
| 2 | `domain.measurement.system` | `application.parsing` (`parse_unit_string`) | top | since step 6a the function only calls the core's parser |
| 3 | `application.context` | `application.startup` | lazy, twice | the kernel creates the default system on first use (`create_default_system`, `create_system`) |
| 4 | `domain.measurement.dimensions` | `application.parsing` | lazy | `Dimension.from_string`, same function as #2 |
| 5 | `domain.measurement.quantity` | `application.startup` | lazy | builds a system when none is active |
| 6 | `infrastructure.config.systems` | `application.startup` | lazy | its `__init__` documents that `startup` imports the package to locate files; the lazy import goes the other way |
| 7 | `ext.io` | `physure` (`__version__`) | top | the release workflows grep and sed `__version__` in `physure/__init__.py` |

The reasons for #1 to #5 come from the import lines; #6 was read only from the module docstring and the import, and needs a look before anyone relies on it.

## Options

- **A. Do nothing until A2b.** The renames and moves of A2b change which layer each module is in, so some records disappear on their own. The cost is that the labels keep lying until then.
- **B. Relabel now.** Change `cause = "a1"` to a cause that is true (for example `a2b`, "removed when the modules move"), and fix the header of the file. Smallest honest change.
- **C. Remove the imports now.** The small ones are cheap and independent of A2b:
  - #2 and #4: move `parse_unit_string` down into the model, as `quantity_text.py` was moved in A2 PR D. It is now a thin call into the core.
  - #3 and #5: invert the dependency with a hook the way `register_quantity_factory` does (the application registers a default-system factory when it loads, the kernel calls it).
  - #1: construct the factory lazily or register it the same way.
  - #6 and #7 need their own thought: #7 needs the release workflows changed together with the move of `__version__` to a leaf module, so it belongs with F4.

## Recommendation

B immediately (one small commit, no behaviour change), then C for #2 and #4, which are mechanical, and leave #1, #3, #5 and #6 for A2b where the layers are redrawn. #7 goes with the release work.

## Open questions

- Do you want the contract to say "no exceptions without a live cause", so that a record whose cause is finished fails a test? That needs a `finished` marker in the toml.
- Is `Q_` on the system the right place for the factory, or should `Q_` move to `application` only?

## Acceptance

- Every remaining record names a cause that is still open.
- `physure-python/tests/core/test_import_layers.py` passes, and a record removed from the toml while its import still exists fails it (the test's existing check).
- The suite, ruff and the benchmark gate stay as before: the first-use time (0.26 s) must not move, since #3 and #5 sit on the first-use path.

## Size and dependencies

Size M for B plus C on #2 and #4, risk low. #1, #3, #5, #6 depend on A2b (N1).
