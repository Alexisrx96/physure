---
name: release
description: Cut a PyPI release of measurekit and measurekit-core. Use when asked to release, publish, bump the version, or tag a version — the tag/version match is enforced by CI and there are two independent version numbers.
---

# Cutting a release

Releases are fully automated by `.github/workflows/release.yml`: pushing a tag
`vX.Y.Z` builds and publishes **both** packages (`measurekit` pure wheel + sdist,
`measurekit-core` abi3-py310 wheels + sdist) to PyPI via Trusted Publishing
(GitHub environment `pypi`). No PyPI tokens exist locally — never try `twine`.

## Two independent versions

- `measurekit` version: `__version__` in `measurekit/__init__.py`
  (pyproject uses `dynamic = ["version"]`). **CI rejects the tag if it doesn't
  match `__version__` exactly.**
- `measurekit-core` version: `version` in `measurekit_core/Cargo.toml`.
  It only needs bumping when the Rust core changed since its last publish
  (PyPI rejects re-uploads of an existing core version).

## Steps

1. Confirm main is green: `gh run list --branch main --limit 3`.
2. Bump `__version__` in `measurekit/__init__.py`; bump `measurekit_core/Cargo.toml`
   too if `measurekit_core/src/` changed since the last release
   (check: `git log --oneline $(git describe --tags --abbrev=0)..HEAD -- measurekit_core/`).
3. Commit the bump on main (PR or direct per user's call), then tag and push:

   ```bash
   git tag vX.Y.Z && git push origin vX.Y.Z
   ```

4. Watch the run: `gh run watch` (jobs: check-version → core-wheels / core-sdist /
   measurekit-dist → publish). The publish job has two separate upload steps because
   Trusted Publishing mints per-project tokens.
5. Verify: `pip index versions measurekit` or check https://pypi.org/project/measurekit/.

## If it fails

- `check-version` failure → tag ≠ `__version__`; delete the tag
  (`git push origin :vX.Y.Z`), fix, re-tag.
- `publish` failure mentioning trusted publisher / OIDC → PyPI Trusted Publisher
  config for that project (repo, workflow `release.yml`, environment `pypi`) is
  missing or wrong; that's a PyPI-side setting the user must fix in the browser.
- Core upload "File already exists" → Cargo.toml version wasn't bumped but core
  changed; bump it and cut a new patch tag.
