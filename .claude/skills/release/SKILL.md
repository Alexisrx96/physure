---
name: release
description: Cut a PyPI release of measurekit and measurekit-core. Use when asked to release, publish, bump the version, or tag a version — the tag/version match is enforced by CI and there are two independent version numbers.
---

# Cutting a release

**Version bumps happen only here, and only via the bump workflow.** Feature/fix
PRs never change `__version__` or the `Cargo.toml` version by hand — the bump
is part of the release itself and is automated.

Releases are fully automated by two chained workflows:

1. `.github/workflows/bump-release.yml` (manual trigger) — bumps
   `__version__` in `measurekit/__init__.py` by the requested part
   (patch/minor/major), bumps `measurekit_core/Cargo.toml` too if
   `measurekit_core/src/` changed since the last tag, commits, tags
   `vX.Y.Z`, and pushes.
2. `.github/workflows/release.yml` (triggered by the `v*` tag push) — builds
   and publishes **both** packages (`measurekit` pure wheel + sdist,
   `measurekit-core` abi3-py310 wheels + sdist) to PyPI via Trusted Publishing
   (GitHub environment `pypi`). No PyPI tokens exist locally — never try `twine`.

## Security

There's no long-lived PyPI token to leak — Trusted Publishing mints a
short-lived, single-use token per run via OIDC. The real risk is anyone who
can push a `v*` tag or edit `release.yml` triggering a real publish. Two
mitigations:

- Third-party actions in both workflows are pinned to commit SHAs (not
  mutable tags like `@v4`), so a repointed upstream tag can't inject code
  into a run that holds `id-token: write`.
- **When creating the `pypi` GitHub environment (one-time setup, see the
  workflow header), add a required reviewer.** Without one, any run that
  reaches the `publish` job auto-publishes with no human gate — a compromised
  push or workflow edit goes straight to PyPI.

## Two independent versions

- `measurekit` version: `__version__` in `measurekit/__init__.py`
  (pyproject uses `dynamic = ["version"]`). **CI rejects the tag if it doesn't
  match `__version__` exactly.**
- `measurekit-core` version: `version` in `measurekit_core/Cargo.toml`.
  It only needs bumping when the Rust core changed since its last publish
  (PyPI rejects re-uploads of an existing core version). The bump workflow
  detects this automatically.

## Steps

1. Confirm main is green: `gh run list --branch main --limit 3`.
2. Trigger the bump (choose the semver part — `measurekit-core`'s bump, if
   any, is automatic and always a patch bump):

   ```bash
   gh workflow run bump-release.yml -f bump=patch   # or minor / major
   ```

3. Watch it commit + tag + push: `gh run watch $(gh run list --workflow bump-release.yml --limit 1 --json databaseId -q '.[0].databaseId')`.
   This chain-triggers `release.yml`.
4. Watch the release run: `gh run watch` (jobs: check-version → core-wheels /
   core-sdist / measurekit-dist → publish). The publish job has two separate
   upload steps because Trusted Publishing mints per-project tokens.
5. Verify: `pip index versions measurekit` or check https://pypi.org/project/measurekit/.

## If it fails

- `bump-release` fails to push → main moved or is protected; rerun after
  rebasing, or push manually with the version it computed.
- `check-version` failure → tag ≠ `__version__`; delete the tag
  (`git push origin :vX.Y.Z`), fix, re-tag.
- `publish` failure mentioning trusted publisher / OIDC → PyPI Trusted Publisher
  config for that project (repo, workflow `release.yml`, environment `pypi`) is
  missing or wrong; that's a PyPI-side setting the user must fix in the browser.
- Core upload "File already exists" → Cargo.toml version wasn't bumped but core
  changed; bump-release.yml should have caught this — check its diff logic
  against `git describe --tags --abbrev=0`.
