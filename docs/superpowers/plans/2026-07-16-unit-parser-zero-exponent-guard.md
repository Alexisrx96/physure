# Unit Parser Zero-Exponent Guard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the Rust unit-string parser from silently mis-splitting digit-suffixed alias names (`a0`, `tau0`) into a bogus `base^0` when they appear inside a compound expression.

**Architecture:** One-line guard in `physure-core/src/units/parser.rs::split_embedded_exponent`: only treat a trailing digit run as an embedded exponent if the parsed value is nonzero. An exponent of `^0` is never a legitimate real-world unit annotation, so this closes the bug without any registry awareness or cross-layer plumbing. No PyO3 or Python signature changes — the fix is entirely internal to the Rust tokenizer.

**Tech Stack:** Rust (physure-core crate, `cargo test`), PyO3/maturin (physure-python crate), Python/pytest (physure-python/tests).

Spec: `physure-python/docs/superpowers/specs/2026-07-16-unit-parser-zero-exponent-guard-design.md`

---

### Task 1: Rust unit test for the zero-exponent guard

**Files:**
- Modify: `physure-core/src/units/parser.rs:272-308` (the existing `#[cfg(test)] mod tests` block)

- [ ] **Step 1: Write the failing test**

Add this test function inside the existing `mod tests` block in `physure-core/src/units/parser.rs`, right after `test_parse_embedded_and_superscripts` (currently ending at line 299):

```rust
    #[test]
    fn test_no_split_on_zero_exponent() {
        let u = Parser::parse_expression("a0").unwrap();
        let dims = u.dimensions_map();
        assert_eq!(dims.get("a0"), Some(&(1, 1)));
        assert_eq!(dims.len(), 1);

        let u2 = Parser::parse_expression("tau0").unwrap();
        let dims2 = u2.dimensions_map();
        assert_eq!(dims2.get("tau0"), Some(&(1, 1)));
        assert_eq!(dims2.len(), 1);

        // Compound expressions must not silently drop the atomic symbol.
        let u3 = Parser::parse_expression("a0/s").unwrap();
        let dims3 = u3.dimensions_map();
        assert_eq!(dims3.get("a0"), Some(&(1, 1)));
        assert_eq!(dims3.get("s"), Some(&(-1, 1)));

        let u4 = Parser::parse_expression("kg*a0").unwrap();
        let dims4 = u4.dimensions_map();
        assert_eq!(dims4.get("kg"), Some(&(1, 1)));
        assert_eq!(dims4.get("a0"), Some(&(1, 1)));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd physure-core && cargo test units::parser::tests::test_no_split_on_zero_exponent`

Expected: FAIL. `a0` gets split into base `"a"` with exponent `0`, so `dims.get("a0")` is `None` and the assertion panics (similarly for `tau0`, and `a0` vanishing entirely from the `a0/s` and `kg*a0` compound cases).

- [ ] **Step 3: Implement the minimal fix**

Replace `split_embedded_exponent` in `physure-core/src/units/parser.rs:259-270`:

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

(Only the inner body changed: the `if let Ok(num) = ...` block now checks `num != 0` before returning the split, and `break`s out of the loop instead of returning when the exponent is zero — there is no other split point worth trying further right once this one is found.)

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd physure-core && cargo test units::parser::tests::test_no_split_on_zero_exponent`

Expected: PASS.

- [ ] **Step 5: Run the full parser test module to check for regressions**

Run: `cd physure-core && cargo test units::parser::tests`

Expected: PASS — `test_parse_simple_units`, `test_parse_embedded_and_superscripts` (`m2`, `s2`, `m²`, `s⁻¹` still split correctly since their exponents are nonzero), `test_parse_parens`, and the new `test_no_split_on_zero_exponent` all pass.

- [ ] **Step 6: Commit**

```bash
git add physure-core/src/units/parser.rs
git commit -m "$(cat <<'EOF'
fix(core): don't split digit-suffixed alias names as a zero exponent

split_embedded_exponent silently mis-parsed unit-string symbols like
"a0" (Bohr radius alias) and "tau0" into base^0 whenever they appeared
inside a compound expression, silently dropping the symbol. An
embedded exponent of 0 is never a legitimate real-world unit
annotation, so guard on it directly rather than requiring registry
awareness.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Python regression test (pre-rebuild, confirms it exercises the real bug)

**Files:**
- Modify: `physure-python/tests/measurement_tests/test_units.py` (append after `test_get_unit_dimensionless_unit_has_empty_exponents`, currently ending at line 169)

- [ ] **Step 1: Write the failing test**

Add this test function to `physure-python/tests/measurement_tests/test_units.py`, after `test_get_unit_dimensionless_unit_has_empty_exponents` (line 169):

```python
def test_get_unit_digit_suffixed_alias_not_split_as_exponent():
    """Regression: a0 (Bohr radius alias) and tau0 (atomic time alias) --
    both registered with a trailing digit -- must not be mis-split into
    base^0 by the native Rust parser's embedded-exponent heuristic when
    they appear inside a compound expression.

    Bare "a0"/"tau0" already worked (caught by the alias-table lookup
    before the parser is ever reached); the bug only showed up for
    compound expressions like "a0/s" or "kg*a0", which fall through to
    parse_unit_string() and silently dropped the symbol with no
    exception raised.
    """
    assert get_unit("a0").exponents == {"a0": 1}
    assert get_unit("a0/s").exponents == {"a0": 1, "s": -1}
    assert get_unit("kg*a0").exponents == {"kg": 1, "a0": 1}

    assert get_unit("tau0").exponents == {"tau0": 1}
    assert get_unit("tau0*s").exponents == {"tau0": 1, "s": 1}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd physure-python && uv run pytest tests/measurement_tests/test_units.py::test_get_unit_digit_suffixed_alias_not_split_as_exponent -xvs`

Expected: FAIL. The installed `physure._core` extension is still built from the pre-fix Rust source, so `get_unit("a0/s")` returns `CompoundUnit({"s": -1})` (missing `"a0"`) and the assertion fails.

- [ ] **Step 3: Rebuild the native extension**

Run: `cd physure-python && maturin develop`

Expected: build succeeds (compiles the workspace including the Task 1 fix in `physure-core`, then installs the rebuilt `physure._core` module into the active venv).

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd physure-python && uv run pytest tests/measurement_tests/test_units.py::test_get_unit_digit_suffixed_alias_not_split_as_exponent -xvs`

Expected: PASS.

- [ ] **Step 5: Run the full test_units.py file to check for regressions**

Run: `cd physure-python && uv run pytest tests/measurement_tests/test_units.py -v`

Expected: all tests PASS, including `test_standard_units_do_not_load_sympy` (which already exercises `m2`/`s-1` through `parse_unit_string` — confirms the nonzero-exponent split path is untouched).

- [ ] **Step 6: Commit**

```bash
git add physure-python/tests/measurement_tests/test_units.py
git commit -m "$(cat <<'EOF'
test: regression coverage for a0/tau0 digit-suffix parsing fix

Covers the Python-visible behavior of the physure-core fix: get_unit()
on compound expressions containing a0 or tau0 no longer silently
drops the symbol.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Full-suite verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full Rust test suite**

Run: `cd physure-core && cargo test`

Expected: all tests PASS (no regressions in any other crate module).

- [ ] **Step 2: Run the full Python test suite**

Run: `cd physure-python && uv run pytest`

Expected: all tests PASS.

- [ ] **Step 3: Ruff clean check**

Run: `cd physure-python && uv run ruff check . && uv run ruff format --check .`

Expected: no violations (the only Python file touched, `test_units.py`, was written in the existing file's style — no new lint surface expected, but this is the project's required gate per CLAUDE.md).
