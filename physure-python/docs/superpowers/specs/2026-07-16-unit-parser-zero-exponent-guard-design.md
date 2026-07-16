# Unit parser: don't split digit-suffixed alias names as a zero exponent

## Context

Auditing native-Rust-parser/sympy-fallback parity (per user request) surfaced a real correctness
bug: `physure-core/src/units/parser.rs::split_embedded_exponent` lets any unit-string symbol with a
trailing digit run mean an embedded exponent shorthand (`m2` -> m², `s-1` -> s⁻¹). It applies this
blindly, with no registry awareness, so it also mis-splits genuine atomic alias names that happen to
end in digits — confirmed empirically for `a0` (Bohr radius alias, `physure.conf:211`) and `tau0`:

```
get_unit("a0^2")   -> CompoundUnit({})              # wrong: silently dimensionless
get_unit("a0/s")   -> CompoundUnit({"s": -1})        # a0 silently vanishes
get_unit("kg*a0")  -> CompoundUnit({"kg": 1})        # a0 silently vanishes
```

No exception is raised in any of these cases, so `parse_unit_string()`'s sympy fallback (which
handles `a0` correctly, since sympy's tokenizer keeps it atomic) never engages. This violates
CLAUDE.md's non-negotiable invariant: "Never silently drop a dimension... a wrong answer with
confident units is worse than an exception."

Only the *bare* string `"a0"` is caught correctly today, via `UnitSystem.get_unit()`'s earlier
alias-table lookup (step 3 of its 5-step resolution order) — that path never reaches the parser.
Any compound expression containing `a0` as a substring falls through to the parser and hits the bug.

## Scope

Fix the two confirmed cases (`a0`, `tau0`) with a minimal, registry-agnostic guard. A fully general
fix (see Non-goals) is out of scope for this change.

### Change: `split_embedded_exponent` skips a zero exponent

An embedded exponent of literally `^0` is never a legitimate real-world unit annotation — nobody
writes `m0` to mean "dimensionless via m^0". This is true independent of any registry, so guarding on
it is a safe, general rule, not a special case for `a0`/`tau0` specifically.

`physure-core/src/units/parser.rs`:

```rust
fn split_embedded_exponent(sym: &str) -> (String, Option<(i64, i64)>) {
    let bytes = sym.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i].is_ascii_digit() || (bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit()) {
            let name = sym[..i].to_string();
            if let Ok(num) = sym[i..].parse::<i64>() {
                if num != 0 {
                    return (name, Some((num, 1)));
                }
                break;
            }
        }
    }
    (sym.to_string(), None)
}
```

`break` (not falling through to try further positions) once a valid digit boundary with exponent 0
is found — there is no other split further right worth attempting, and this preserves the existing
"first valid split wins" semantics for the nonzero case unchanged.

No PyO3 signature changes, no Python-side changes — the fix is entirely internal to the Rust
tokenizer and transparent to every caller.

## Non-goals

- **General registry-aware disambiguation** ("Approach B" from the design discussion): threading the
  active `UnitRegistry` into `Parser::parse_expression` so any whole token matching a known
  unit/alias name is never split, regardless of exponent value. This would fully close the bug class
  but requires a new `PyUnitRegistry.parse_expression()` PyO3 method, changing
  `parse_unit_string()`/`_native_parse()` to receive the active `UnitSystem`'s `_core_registry`, and
  extending `parse_unit_string`'s `@functools.lru_cache` key to include system identity (today it
  caches on the expression string alone, which would become a correctness risk once the parse result
  can depend on which system is active). Deferred — see Follow-up.
- Renaming `a0`/`tau0` or other digit-suffixed aliases to avoid the ambiguity — a breaking public API
  change, and doesn't fix the general class for future units/constants.
- Any change to `parse_unit_string()`'s fallback-on-exception logic — orthogonal to this bug (no
  exception was ever the problem here).

## Testing

- Rust: one new unit test in `parser.rs`'s existing `#[cfg(test)] mod tests` block,
  `test_no_split_on_zero_exponent`, asserting `Parser::parse_expression("a0")` and `("tau0")`
  produce single-key dimension maps (`{"a0": (1,1)}`, `{"tau0": (1,1)}`) — not split — while existing
  `m2`, `s-1`, `m²`/`s⁻¹` cases (already covered by `test_parse_embedded_and_superscripts`) keep
  splitting correctly.
- Python: a regression test near `UnitSystem.get_unit()`'s existing test coverage asserting
  `get_unit("a0/s")` and `get_unit("kg*a0")` no longer drop `a0` from the resulting dimension map.

## Follow-up (not implemented here)

The general registry-aware fix (Non-goals, above) remains an open gap for any *nonzero*
digit-suffixed name used inside a compound expression — e.g. `conventional_value_of_ampere_90`,
`quantum_of_circulation_times_2`, `lattice_spacing_of_ideal_si_220`, `molar_mass_of_carbon_12`,
`hyperfine_transition_frequency_of_cs_133` (all present in `physure.conf`). None of these are
confirmed-broken today (unverified whether any are ever used as unit-expression substrings in
practice), so this is logged as a tracked risk, not an active bug.

## Risks / open questions

None blocking. The `break` vs. `continue` choice is deliberate (see Scope) and doesn't affect any
existing passing test.
