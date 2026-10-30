# After A1: inventory of findings and next steps

**Status:** DRAFT inventory. Nothing here is approved or scheduled.
**Date:** 2026-10-30, the day after 0.3.0 was published.
**What it is for:** one place that lists what is left after A1, with the evidence for each item and a draft spec for the ones that need a decision. Each draft is meant to go through a brainstorming pass with the maintainer before it becomes a plan; the questions it needs answered are at the end of each draft and collected at the bottom of this file.

Sizes (S: hours, M: a day or two, L: a week or more) and risks are estimates, not measurements.

## 1. State at 0.3.0

| Thing | State | Evidence |
|---|---|---|
| A1 (Rust owns the unit algebra), A2 (import layering), repo hygiene | Done, merged by PR #68 and released | `CHANGELOG.md` [0.3.0]; design and per-step outcomes in `2026-10-29-a1-stage2-design.md` |
| Python `physure` 0.3.0 | Published | PyPI lists 5 wheels and the sdist; the wheel was smoke-tested in an isolated environment |
| Rust crate `physure` 0.3.0 | Published | crates.io `max_version` 0.3.0, not yanked; `core-v0.3.0` release has the 5 `phs` binaries |
| `physure-java` 0.3.0 | **Staged, not public** | `java-release.yml` was green, but `https://repo1.maven.org/.../physure-java/0.3.0/` and `.../0.2.4/` answer 404 and `maven-metadata.xml` says latest 0.2.3; `physure-java/pom.xml` has `autoPublish` set to `false` |
| `physure-wasm` / npm | Not released | out of scope for 0.3.0 |
| PR #70 (release skill corrected) | Open | https://github.com/Alexisrx96/physure/pull/70 |
| Tests at the last code commit | Python 1257 passed and 1 xfailed; Rust 767 passed; ruff and `mkdocs --strict` clean | verified before the release |

## 2. Findings

| ID | Finding | Evidence | Size | Risk | Draft |
|---|---|---|---|---|---|
| F1 | Two copies of the benchmarks disagree | `BENCHMARKS.md` and `docs/benchmarks.md` differ in 27 lines (N=100 row: 3.5 ms against 1.9 ms), both last touched by the same commit `214252b`; the README links only the root one | S | low | `2026-10-30-draft-benchmarks-single-source.md` |
| F2 | Six layering exceptions are labelled `a1` ("A1 deletes the module") and A1 is finished | `physure-python/tests/core/import_layers.toml` records 7 exceptions, 6 with `cause = "a1"`, 1 with `release`; the modules were not deleted | M | low | `2026-10-30-draft-layering-exceptions-after-a1.md` |
| F3 | Java and WASM have no conformance contract | `physure-java/src/test` does not exist; `physure-wasm` has 14 inline `wasm_bindgen_test` and no workflow runs `wasm-pack test`; no workflow runs Maven tests; both bindings expose only `getUnitExponents`, `getUnitScale`, `evaluate`, `getCategories`, `solve`, `deriv`, `integral` | M | low | `2026-10-30-draft-binding-conformance-java-wasm.md` |
| F4 | A release needs manual steps the workflows do not say | the three bump workflows push their tag with `GITHUB_TOKEN`, which starts no workflow; Java also needs a Publish click in the Sonatype portal; the `pre-push` hook makes every `git push` cost about 12 minutes | M | medium (publishing is irreversible) | `2026-10-30-draft-release-pipeline-triggers.md` |
| F5 | Three renderings of one fractional power, and a display name that Python drops | the core shows `m⁰⋅⁵` (`rational.rs`), its Python class prints `m^1/2 * s^-1/3`, `Dimension` prints `T^(1/2)`; `facts("°")["simplified"]` is `°` and Python's is `deg` | M | medium (output users see) | `2026-10-30-draft-unit-display-rule.md` |

## 3. Next steps

| ID | Step | Why now | Size | Risk | Draft |
|---|---|---|---|---|---|
| N1 | Honest package names (A2b): rename the lowest package `core`, move the top-level modules that hold implementation (`plotting.py` 1943 lines, `symbolic.py` 450, `functional.py` 250, `pandas_ext.py` 202) | A1 is done, which was its stated precondition | L | medium (blast radius: `domain` 308 references, `application` 63, `core` 53 per the A2 spec) | already specified: section "A2b: honest names" of `2026-10-27-a2-layering-contract-design.md` |
| N2 | Static dimensional checking of PHS (resolver, language server, `phs check`) | the exact registry exists now; the grammar already allows `param: unit`; today the language server only hovers over a fixed table of units | L | medium | `2026-10-30-draft-static-dimension-checking.md` |
| N3 | Extract the symbolic engine from `physure-script` | 4210 lines in 14 files, one file coupled to the PHS lexer; every binding already depends on `physure-script`; the gain is dependency weight, to be measured first | M | medium | `2026-10-30-draft-symbolic-extraction.md` |
| N4 | Units out of the way of `torch.compile` and `physure.jit` | every unit operation is a graph break (6, 4, 2 per expression), measured and written down in `docs/torch_compile_integration.md` | L | medium (depends on torch internals) | `2026-10-30-draft-jit-trace-time-units.md` |
| N5 | A validation surface for agents and language models | `.mcp.json` is the SonarQube development server, not product support; `docs/LLM_SLM_SYNTAX_EXPOSURE_SPEC.md` is still a proposal; the CLI has `phs doc` and `phs serve` but no `check` | M | medium (executing PHS from outside) | `2026-10-30-draft-llm-validation-surface.md` |

## 4. Housekeeping without a spec

| Item | Detail | Suggested action |
|---|---|---|
| Untracked plans and specs | 11 files under `docs/superpowers/` and `docs/uncertainty-gum-compliance.md` exist only in the main checkout (August 2026 work on the PHS to JS/TS codegen, the WASM binding, assert builtins, pilot readiness, the parser split, boolean logic and GUM precision) | maintainer decides: commit them as the record of work that is merged, or discard |
| Maven Central 0.2.4 | looks unpublished (see section 1) | check the Sonatype portal, publish or drop the deployment |
| Stale local tag | the local `v0.2.4` points to the bump commit; the remote tag was moved to `e1a1e0d`, so `git fetch --tags` refuses to update it | `git fetch origin tag v0.2.4 --force` (a local-only ref) |
| `from physure import units` | is the registry itself unless `import physure.units` ran first, then the forwarding package; both answer the same | document, or make it one object (A2b decides) |
| `DimVector` | public type of the core, used by the Rust crates, exported by `physure`, unused by Python's `Dimension` | decided in the A1 cleanup to keep; revisit only if the Rust crates stop using it |
| Lock files after a bump | after the three bumps `Cargo.lock` still says 0.2.4 for the six workspace crates and `physure-python/uv.lock` says 0.2.4 for `physure`; any build or `uv` command regenerates them, so they show up as uncommitted changes in every checkout (the same drift was fixed by hand before, in `50bb0ca` and `5afb14e`) | sync them in one commit after the release, or make the bump workflows do it (belongs with F4) |
| Local leftovers | empty `physure/core/parsing/` and `domain/measurement/ports/` directories (only `__pycache__`) and `physure_core.pdb`, all ignored by git | delete by hand, no commit |

## 5. Proposed order

1. **Independent and cheap, in any order:** F1 (benchmarks), F2 (exceptions), the untracked docs, and merging PR #70.
2. **F4 before the next release,** so that the next one is not a rehearsal of this one.
3. **F3 and F5** next: F3 protects the bindings before they grow; F5 changes what users see, so it wants the next minor version.
4. **N2 first among the large ones:** it uses what A1 built, and N5 gets better with it. N4 is independent and can run in parallel. N3 only after its measurement. N1 whenever a quiet moment allows a rename, because every other item touches the names it moves.

## 6. Not proposed

- A Rust `Quantity` that holds arrays (kept scalar by the A1 design).
- The formula repository (compiled library plus `.proto` plus `.md` per formula): parked by the maintainer, not to be built unprompted.
- Releasing `physure-wasm` to npm: it has no bump workflow and was not part of 0.3.0.

## 7. Decisions the drafts need from the maintainer

Collected from the drafts; each one is spelled out in its own file.

- **F1:** which benchmark numbers are right, and whether the root file stays for GitHub readers.
- **F2:** whether to remove the six imports now or wait for A2b to move the modules.
- **F3:** how much CI time Java and WASM tests may cost, and whether the bindings get a `facts` call.
- **F4:** a token secret, a dispatch from the bump workflow, or keeping the manual re-tag; and whether Java keeps its manual Publish gate.
- **F5:** the canonical form of a fractional power, and whether a display name survives unit algebra.
- **N2:** warn or error when a unit is unknown; how unannotated parameters are treated.
- **N3:** the size gain that would justify an eighth crate.
- **N4:** the target (`torch.compile`, `physure.jit`, or both).
- **N5:** the safe subset of PHS that may run for an outside caller.
