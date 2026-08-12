---
name: release
description: Cut a release of physure (Python/PyPI, Rust core/crates.io, Java/Maven Central, or WASM/npm). Use when asked to release, publish, bump the version, or tag a version — there are four independent release flows and version numbers, each enforced by CI tag-match checks.
---

# Cutting a release

**Version bumps happen only via the bump workflows below — never by hand
in a feature/fix PR.** Each package has its own version and its own tag
prefix; CI rejects a release tag that doesn't match the package's
version file.

## The four release flows

| Package | Bump workflow (manual trigger) | Release workflow (tag-triggered) | Tag prefix | Registry |
|---|---|---|---|---|
| `physure` (Python) | `bump-release.yml` — bumps `physure-python/physure/__init__.py` | `release.yml` | `vX.Y.Z` | PyPI (Trusted Publishing) |
| `physure` (Rust crate) | `bump-core-release.yml` — bumps `[workspace.package] version` in root `Cargo.toml` | `core-release.yml` — also builds and attaches prebuilt `phs` + `physure-lsp` binaries (5 platforms) to the GitHub release | `core-vX.Y.Z` | crates.io (Trusted Publishing) |
| `physure-java` | `bump-java-release.yml` — bumps `physure-java/pom.xml` | `java-release.yml` — builds native JNI libs (5 platforms), publishes via Sonatype Central Portal | `java-vX.Y.Z` | Maven Central |
| `physure` (npm/WASM) | **none — manual** | `wasm-release.yml` | `wasm-vX.Y.Z` | npm |

No PyPI/crates.io tokens exist locally for any of these — never try
`twine`, `cargo login`, or `npm login` locally. All four use short-lived,
per-run tokens (OIDC Trusted Publishing for PyPI/crates.io; stored
secrets in a required-reviewer GitHub environment for Maven/npm).

## Two independent versions worth knowing about

- `physure-wasm/Cargo.toml` has `version.workspace = true` — it silently
  moves whenever `bump-core-release.yml` bumps the workspace version. It
  is **not** independently bumpable via a Cargo.toml edit alone.
- `physure-wasm/package.json`'s `"version"` is a **separate, hand-edited**
  field. `wasm-release.yml`'s `check-version` job requires the tag,
  `Cargo.toml`'s (workspace-inherited) version, and `package.json`'s
  version to all match — so before tagging a wasm release, bump
  `physure-wasm/package.json` by hand to match whatever the current
  workspace version is.
- `release.yml` actually triggers on `v*`, `py-v*`, **and** `py-core-v*`
  (not just `vX.Y.Z`) — its `check-version` job strips whichever prefix
  matched, trying `py-core-v`, then `py-v`, then `v`, before comparing
  against `__version__`. `core-release.yml` triggers on `core-v*` **and**
  `py-core-v*` too. That overlap means pushing a `py-core-v*` tag fires
  *both* `release.yml` and `core-release.yml` at once — two independent,
  differently-versioned release pipelines simultaneously. Avoid tagging
  `py-core-v*` unless you specifically intend to trigger both.

## Steps: Python release

1. Confirm main is green: `gh run list --branch main --limit 3`.
2. Trigger the bump: `gh workflow run bump-release.yml -f bump=patch` (or
   `minor`/`major`).
3. Watch it commit + tag + push:
   `gh run watch $(gh run list --workflow bump-release.yml --limit 1 --json databaseId -q '.[0].databaseId')`.
   This chain-triggers `release.yml`.
4. Watch the release run: `gh run watch` (jobs: `check-version` →
   `wheels` / `sdist` → `publish` → `github-release`).
5. Verify: `pip index versions physure` or
   https://pypi.org/project/physure/.

## Steps: Rust core release

1. Confirm main is green.
2. Trigger: `gh workflow run bump-core-release.yml -f bump=patch`.
3. Watch it tag `core-vX.Y.Z` and push, chain-triggering `core-release.yml`.
4. Watch the release run: jobs `check-version` → `publish` (crates.io) +
   `phs-binaries` (5-platform matrix) → `github-release`.
5. Verify: https://crates.io/crates/physure, and check the GitHub release
   has all 5 `phs-*` binary archives attached.

## Steps: Java release

1. Confirm main is green.
2. Trigger: `gh workflow run bump-java-release.yml -f bump=patch`.
3. Watch it tag `java-vX.Y.Z` and push, chain-triggering `java-release.yml`.
4. Watch the release run: jobs `check-version` → `native-libs` (5-platform
   matrix) → `publish` (Maven Central via Sonatype Central Portal, GPG-signed).
5. Verify: https://central.sonatype.com/artifact/io.github.alexisrx96/physure-java
   (propagation to Maven Central search can take a few hours).

## Steps: WASM/npm release

1. Confirm main is green.
2. Check the current workspace version: `grep -m1 '^version' Cargo.toml`.
3. Manually bump `physure-wasm/package.json`'s `"version"` field to match
   (or to the version the workspace will be at — coordinate with a core
   release if you need a version bump first, since there's no dedicated
   wasm bump workflow).
4. Commit that change, then tag by hand:
   ```bash
   git add physure-wasm/package.json
   git commit -m "chore: release wasm-vX.Y.Z"
   git tag wasm-vX.Y.Z
   git push origin main wasm-vX.Y.Z
   ```
5. Watch the release run: `gh run watch` (jobs `check-version` →
   `publish`, builds `bundler` + `nodejs` targets via `wasm-pack`,
   publishes to npm).
6. Verify: `npm view physure versions`.

## If it fails

- Any `bump-*.yml` fails to push → main moved or is protected; rerun
  after rebasing, or push manually with the version it computed.
- Any `check-version` job failure → tag doesn't match the package's
  version file; delete the bad tag (`git push origin :<tag>`), fix the
  version file, re-tag.
- `release.yml`/`core-release.yml` publish failure mentioning trusted
  publisher / OIDC → the PyPI/crates.io Trusted Publisher config (repo,
  workflow filename, environment name) is missing or wrong — that's set
  in the registry's web UI, the user must fix it.
- `java-release.yml` publish failure → check the `maven-central`
  environment's `CENTRAL_USERNAME`/`CENTRAL_TOKEN`/`GPG_PRIVATE_KEY`/
  `GPG_PASSPHRASE` secrets exist and the GPG key hasn't expired.
- `wasm-release.yml` `check-version` failure → `physure-wasm/package.json`
  wasn't bumped to match the (workspace-inherited) Cargo version — fix
  and re-tag.
- crates.io "already exists" → someone tagged `core-vX.Y.Z` for a version
  already published; bump again.
