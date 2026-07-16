# Kill the fake Rust-fallback illusion in quantity.py

## Context

`physure/domain/measurement/quantity.py` has a try/except that manually does
`raise ImportError("Use Python CoreQuantity container")` immediately before a real
`from physure._core import Quantity as CoreQuantity` — guaranteeing the except branch always
runs and the Rust import is permanently unreachable. The pure-Python `CoreQuantity` class (the
only path ever taken) is docstring-labeled `"""Pure-Python stand-in when physure._core is
unavailable."""` and the dead flag `IS_CORE_AVAILABLE = False` sits next to it. Grep confirms
`IS_CORE_AVAILABLE` has zero consumers anywhere in the codebase — it's not read by anything.

This reads as an oversight, but it isn't one. `physure/__init__.py:33-48` already hard-imports
the real Rust `Quantity` as `CoreQuantity` unconditionally (fails fast if missing) — matching the
pattern the codebase has been actively applying elsewhere per recent commits (mandating direct
Rust `RationalUnit`, `CovarianceStore`, `PruningConfig` imports without fallback classes).
quantity.py's illusion is the last holdout of the old fallback style, kept in place for a real,
stated reason (line 79's comment): the pure-Python container exists for PyTorch Dynamo tracing
and multiple-inheritance compatibility with `ArithmeticMixin`/`BackendMixin`, not because the
Rust extension might be missing.

`units.py`'s `CompoundUnit(BaseExponentEntity, RationalUnit)` proves multiple inheritance from a
Rust PyO3 base does work for a simpler value type — but `Quantity` carries multi-backend
magnitudes, uncertainty, and Dynamo tracing, so this isn't proof the same holds for `Quantity`.
Whether it could be made to work is a separate, higher-effort question (see Non-goals).

Two other `IS_CORE_AVAILABLE = True` occurrences (`units.py:148`, `native.py:806`) are a
different, milder case: no try/except, sitting right after a genuinely successful unconditional
import. Not deceptive — just dead, unread flags (also zero consumers). Bundled into this cleanup
since they're the same category of leftover and cost one line each to remove.

## Scope

1. Delete the dead try/except in `quantity.py`: the manually-raised `ImportError`, the
   unreachable `from physure._core import Quantity as CoreQuantity`, and `IS_CORE_AVAILABLE`
   (both the `True` and `False` assignments).
2. Keep the pure-Python `CoreQuantity` class's implementation exactly as-is — it is load-bearing,
   not a fallback, and nothing about its behavior changes.
3. Rewrite its docstring/leading comment to state the real reason it exists (Dynamo tracing +
   multiple-inheritance compatibility with `ArithmeticMixin`/`BackendMixin`), replacing the
   "stand-in when physure._core is unavailable" framing.
4. Remove the two vestigial `IS_CORE_AVAILABLE = True` lines in `units.py:148` and
   `native.py:806` (re-confirm zero consumers at implementation time before deleting).
5. Leave `_CORE_QUANTITY_TYPE` and its usages untouched — it's a live runtime string-based type
   check, unrelated to this illusion.

## Non-goals

- Investigating or attempting to make `Quantity` inherit directly from the real Rust `Quantity`
  type (reversing the container pattern) — a separate, higher-uncertainty future spec if ever
  pursued, not required to fix the honesty problem.
- Any change to `_arithmetic_mixin.py`'s per-operator dispatch or the three-parallel-
  arithmetic-engines duplication — identified during this investigation but not the selected
  target; separate future specs.
- Any behavior change. This is a dead-code-and-comment-only cleanup.

## Testing

- No new tests — no behavior changes. `uv run pytest` must stay green (dead code removal only).
- Re-grep for `IS_CORE_AVAILABLE` consumers immediately before deleting each occurrence, in case
  something new referenced it since this spec was written.

## Risks / open questions

None blocking. The Dynamo/multiple-inheritance justification in the existing comment is taken at
face value here — verifying it against a real `torch.compile` trace is out of scope for a
comment-only fix. If it later turns out to be wrong or outdated, that's the reversing-the-
container-pattern item under Non-goals to chase separately.
