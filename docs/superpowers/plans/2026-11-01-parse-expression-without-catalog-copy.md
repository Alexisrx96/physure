# Parsing a unit without copying the catalog — plan

**Spec:** `docs/superpowers/specs/2026-11-01-parse-expression-without-catalog-copy-design.md`
**Branch:** `perf/parse-expression-no-copy` (worktree `.claude/worktrees/parse-no-copy`), off `main` @ `7786740`.
**Commit rule:** every commit ends with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; nothing is pushed or merged without the maintainer's word.

## Task 1: the baseline

- [x] `cargo bench -p physure --bench core_benchmarks -- parse_` on the unchanged tree; keep the two figures (the page says 218 µs and 185 µs, from another session).

## Task 2: the failing test, then the function

- [x] In `physure-core/src/units/tests.rs`, add `with_conf_registry_gives_the_embedded_catalog_by_reference`: two calls return the same `*const UnitRegistry`. Run it: it fails to compile (no such function).
- [x] In `physure-core/src/units/conf.rs`, add `with_conf_registry` (the spec's two cases). Run the test: it passes.
- [x] In `Parser::parse_expression`, replace the `None` arm with `crate::units::conf::with_conf_registry(|registry| Self::parse_expression_impl(input, Some(registry)))`.
- [x] `cargo test -p physure -p physure-script`: unchanged results.

## Task 3: the override still works

- [x] `physure-core/tests/parse_local_conf.rs` (its own binary): in a temporary directory, with no file, `Parser::parse_expression("zorb")` is an error; write a `physure.conf` that defines `zorb` and change the working directory there: it parses; remove the file: it is an error again.

## Task 4: the figures

- [x] Re-run the benchmark; put the two rows of `docs/benchmarks.md` §5 and their note in line with the new figures; say what the `stat` costs.
- [x] `CHANGELOG.md` `[Unreleased]` → Changed: one entry with the before and after.

## Task 5: check and commit

- [x] `cargo clippy -p physure` shows nothing new in the files touched; `cargo test -p physure -p physure-script -p physure-cli -p physure-lsp` passes.
- [x] Commit the spec and plan first, then the code, then the figures.
