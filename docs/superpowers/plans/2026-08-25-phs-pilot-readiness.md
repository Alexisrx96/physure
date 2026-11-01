# PHS training pilot readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Design spec:** `docs/superpowers/specs/2026-08-25-phs-pilot-readiness-design.md` — read it first; this plan implements Sections 1 and 3 of it exactly.

**Goal:** Close two of the three remaining Fase-0 items for the PHS training pilot: prove `physure-lsp` reports accurate diagnostics for this branch's two new error types, and write the PHS-specific tutorial that doesn't exist yet.

**Architecture:** Section 1 is regression-test-only (no production code — the diagnostic-position infrastructure already exists and this proves it handles the two new error shapes). Section 3 is new content: a companion `.phs` script (verified against the real `phs` binary first) plus a markdown guide that narrates it.

**Tech Stack:** Rust (`physure-lsp` test additions), Markdown + PHS (tutorial content).

**Section 2 (cutting release v0.2.4) is intentionally not part of this plan.** Per the design spec's Sequencing note, a release tags a commit on `main` after this branch merges — it cannot happen from a WIP branch. The spec's own Section 2 already documents the process checklist for when that time comes; nothing here duplicates or supersedes it.

---

## Task 1: LSP diagnostic test for the bare-identifier-juxtaposition error

**Files:**
- Modify: `physure-lsp/src/incremental.rs`

- [x] **Step 1: Write the test**

Add to the existing `#[cfg(test)] mod tests` block in `physure-lsp/src/incremental.rs`,
directly after `test_unit_shadowing_lsp_diagnostic_location_and_cleaning` (around line 813):

```rust
    #[test]
    fn test_bare_identifier_juxtaposition_lsp_diagnostic_location_and_cleaning() {
        let script = "x y\n";
        let err = physure_script::parser::parse_phs_with_lines(script).unwrap_err();
        let err_str = err.to_string();
        let (line, col) = extract_line_col_from_err(&err_str);
        assert_eq!(line, 0, "Should point to line 1 (0-indexed 0)");
        assert_eq!(col, 2, "Should point to col 3 (0-indexed 2), right at 'y'");

        let cleaned = clean_error_message(&err_str);
        assert!(!cleaned.contains("Generic("));
        assert!(!cleaned.contains("-->"));
        assert!(cleaned.contains("Missing operator between 'x' and 'y'"));
        assert!(cleaned.contains("Write `x * y`"));
    }
```

The expected line/col and message text above were captured by actually running
`physure_script::parser::parse_phs_with_lines("x y\n")` against this branch's code (not
guessed) — the raw error is `--> 1:3\nMissing operator between 'x' and 'y': PhysureScript does
not read two bare names side by side as a product. Write \`x * y\` if you meant to multiply
them, or add whatever operator belongs between them.`, and `extract_line_col_from_err`
0-indexes both the line and column it extracts (`saturating_sub(1)` on each), so `1:3` becomes
`(0, 2)`.

- [x] **Step 2: Run it**

```bash
cd d:/Projects/physure
cargo test -p physure-lsp test_bare_identifier_juxtaposition_lsp_diagnostic_location_and_cleaning -- --nocapture
```

Expected: **PASS**. Unlike ordinary TDD, this is not expected to start red — the
diagnostic-position infrastructure (`extract_line_col_from_err`) already exists and already
handles the `"--> {line}:{col}"` format generically; this test is proving that existing
behavior for a new error shape, not driving new implementation. If it fails, that is a real,
unexpected finding — stop and report it rather than adjusting the test to match wrong output.

- [x] **Step 3: Run the full physure-lsp suite**

```bash
cargo test -p physure-lsp
```

Expected: `test result: ok. 37 passed` (36 existing + this new one), 0 failed.

- [x] **Step 4: Commit**

```bash
git add physure-lsp/src/incremental.rs
git commit -m "test(lsp): verify diagnostic position for the bare-identifier-juxtaposition error"
```

---

## Task 2: LSP diagnostic test for the head-token shadowing error

**Files:**
- Modify: `physure-lsp/src/incremental.rs`

- [x] **Step 1: Write the test**

Add directly after Task 1's new test:

```rust
    #[test]
    fn test_head_token_shadowing_lsp_diagnostic_location_and_cleaning() {
        let script = "fn kinetic_energy(m, v) = 1/2 m v^2\n";
        let err = physure_script::parser::parse_phs_with_lines(script).unwrap_err();
        let err_str = err.to_string();
        let (line, col) = extract_line_col_from_err(&err_str);
        assert_eq!(line, 0, "Should point to line 1 (0-indexed 0)");
        assert_eq!(col, 0, "Should point to col 1 (0-indexed 0)");

        let cleaned = clean_error_message(&err_str);
        assert!(!cleaned.contains("Generic("));
        assert!(!cleaned.contains("-->"));
        assert!(cleaned.contains("Ambiguous 'm' in the quantity literal `2 m`"));
        assert!(cleaned.contains("Write `2 * m`"));
        assert!(cleaned.contains(r#"2 * "m""#));
    }
```

Same sourcing note as Task 1: captured by actually running
`physure_script::parser::parse_phs_with_lines("fn kinetic_energy(m, v) = 1/2 m v^2\n")` — the
raw error is `--> 1:1\nAmbiguous 'm' in the quantity literal \`2 m\`: 'm' is a registered unit
symbol and also a name this script binds earlier, and PhysureScript will not guess which one
you meant. Write \`2 * m\` to multiply by the variable, or \`2 * "m"\` to keep the unit.` —
this is the motivating kinetic-energy example from this session's earlier work
(`physure-script/tests/unit_shadowing.rs`'s
`the_kinetic_energy_shorthand_that_motivated_this_check_is_rejected`), now proven to also
surface correctly through the LSP diagnostic pipeline, not just the raw parser error.

- [x] **Step 2: Run it**

```bash
cd d:/Projects/physure
cargo test -p physure-lsp test_head_token_shadowing_lsp_diagnostic_location_and_cleaning -- --nocapture
```

Expected: **PASS**, same reasoning as Task 1 Step 2 — this proves existing behavior, it isn't
expected to start red.

- [x] **Step 3: Run the full physure-lsp suite**

```bash
cargo test -p physure-lsp
```

Expected: `test result: ok. 38 passed` (37 from Task 1 + this new one), 0 failed.

- [x] **Step 4: Commit**

```bash
git add physure-lsp/src/incremental.rs
git commit -m "test(lsp): verify diagnostic position for the head-token shadowing error"
```

---

## Task 3: The `phs_primer.phs` companion script

Written and verified against the real `phs` binary **before** the markdown guide (Task 4),
since the guide narrates this script and every quoted output in it must match what the script
actually produces.

**Files:**
- Create: `docs/tutorials/phs_primer.phs`

- [x] **Step 1: Build the `phs` binary from this branch**

```bash
cd d:/Projects/physure
cargo build --release -p physure-cli
```

- [x] **Step 2: Write the script**

Create `docs/tutorials/phs_primer.phs`:

```phs
# @title: PHS primer — quantities, conversion, uncertainty, and a report
# @author: physure
# @abstract: The day-one PHS lesson: a quantity with a unit, converting it, carrying
#   uncertainty through a calculation, and showing the intermediate steps with `where`.

# 1. A quantity is a number with a unit, written the way you'd write it on paper.
presion = 100.0 kPa

# 2. Convert on the spot with `=>`.
presion_bar = presion => bar

# 3. A measurement carries its uncertainty through the whole calculation.
velocidad = 25.0 +/- 0.5 m / s
masa = 2.0 +/- 0.1 kg
energia_cinetica = 0.5 * masa * velocidad^2

# 4. `where` shows an intermediate step without a separate assignment above it.
fuerza = masa * aceleracion where aceleracion = 9.81 m / s^2

"La presion es {presion_bar}, la energia cinetica es {energia_cinetica}, y la fuerza es {fuerza}"
```

- [x] **Step 3: Run it and capture the real output**

```bash
./target/release/phs docs/tutorials/phs_primer.phs
```

Read the actual printed values. If any line's output looks different from what you'd expect
(a unit, a magnitude, an uncertainty), that's the real behavior — Task 4's guide must quote
this actual output, not a guess.

- [x] **Step 4: Generate the HTML report and confirm it opens**

```bash
./target/release/phs docs/tutorials/phs_primer.phs --html
```

Confirm the command succeeds and names the HTML file it wrote. Task 4's guide references this
step as the lesson's closing deliverable.

- [x] **Step 5: Commit**

```bash
git add docs/tutorials/phs_primer.phs
git commit -m "docs(phs): add the phs_primer.phs companion script for the PHS tutorial"
```

---

## Task 4: The two "break it on purpose" examples

Verified separately from Task 3's main script, since both are *expected* to fail — capturing
their real error text now means Task 5's guide can quote it exactly.

**Files:** none created — this task only captures real command output for Task 5 to use.

- [x] **Step 1: Capture the unit-mismatch example**

```bash
cd d:/Projects/physure
printf '5 m + 2 kg' > "$TEMP/break1.phs"
./target/release/phs "$TEMP/break1.phs" 2>&1
```

Record the exact error text printed (expect something naming a unit mismatch between `m` and
`kg`, without needing to guess the precise wording — read it from the real command output).

- [x] **Step 2: Capture the bare-juxtaposition example**

```bash
printf 'masa = 2.0 kg\nvelocidad = 3.0 m/s\ntotal = masa velocidad' > "$TEMP/break2.phs"
./target/release/phs "$TEMP/break2.phs" 2>&1
```

Record the exact error text — this should be the "Missing operator between 'masa' and
'velocidad'" message from this session's fix, proving the exercise actually demonstrates what
it claims to.

- [x] **Step 3: No commit for this task** — the captured text feeds directly into Task 5.

---

## Task 5: The `phs_primer.md` guide

**Files:**
- Create: `docs/tutorials/phs_primer.md`

- [x] **Step 1: Write the guide**

Structure, per the design spec's Section 3 (use Task 3's actual captured output and Task 4's
actual captured error text throughout — no placeholder output):

1. **What PHS is and isn't** — one paragraph. Reuse the framing from
   `physure-cli/README.md`'s "Why PHS exists" section (the person who does the physics writes
   and runs the calculation themselves; the script is the spec).
2. **Quantities with units** — walks `phs_primer.phs`'s section 1, showing the real output
   from Task 3 Step 3.
3. **Conversion with `=>`** — walks section 2, same treatment.
4. **Uncertainty with `+/-`** — walks section 3, showing the propagated uncertainty in the
   real output.
5. **`where` for showing steps** — walks section 4.
6. **`phs script.phs --html`** — the closing deliverable, referencing Task 3 Step 4's
   confirmed HTML generation, with the "photographed whiteboard" framing: the report is the
   audit trail, not just the answer.
7. **Break it on purpose** — both Task 4 examples, quoted verbatim (real error text, not
   paraphrased), each followed by one sentence explaining what a reader should conclude from
   seeing it (a unit mismatch means the formula's dimensions don't work out; the missing-
   operator error means PHS caught a typo that would otherwise have silently multiplied two
   values with no warning).

- [x] **Step 2: Cross-check every quoted output against Task 3/4's captured text**

Read back through the finished guide once and confirm no quoted PHS output, error message, or
magnitude was typed from memory instead of copied from what Tasks 3-4 actually captured.

- [x] **Step 3: Commit**

```bash
cd d:/Projects/physure
git add docs/tutorials/phs_primer.md
git commit -m "docs(phs): add the PHS primer tutorial (docs/tutorials/phs_primer.md)"
```

---

## Task 6: Final verification

**Files:** none — verification only.

- [x] **Step 1: Full physure-lsp and physure-script suites**

```bash
cd d:/Projects/physure
cargo test -p physure-lsp
cargo test -p physure-script
```

Expected: `physure-lsp` at 38 (36 baseline + Tasks 1-2's two new tests), `physure-script`
unchanged at 279.

- [x] **Step 2: Confirm the tutorial's companion script still runs clean**

```bash
./target/release/phs docs/tutorials/phs_primer.phs --html
```

Expected: succeeds, same output as Task 3 Step 3/4 (nothing in Tasks 4-5 should have touched
`phs_primer.phs` itself).

- [x] **Step 3: mkdocs sanity check** (the docs link policy has broken CI twice before)

```bash
cd physure-python
uv run mkdocs build --strict
```

- [x] **Step 4: No commit for this task** — verification-only.
