# `phs upgrade`: self-updating CLI

## Context

`phs` has no way to update itself today — a user has to know to re-run `scripts/install.ps1`/
`.sh`/`.cmd`, or manually `cargo install`. Those scripts already contain the right logic
(`core-release.yml` publishes prebuilt `phs`+`physure-lsp` binaries attached to `core-vX.Y.Z`
GitHub Releases, one per platform); this spec turns that same logic into a native `phs upgrade`
subcommand so a user never has to leave the tool to update it.

Two modes, matching the existing install scripts' own two paths:
- **`phs upgrade`** (default): finds the latest `core-v*` GitHub Release, downloads the
  prebuilt binary for the current platform, no Rust toolchain required.
- **`phs upgrade --nightly`**: builds from `main`'s current tip via `cargo install --git`,
  same as `PHS_BRANCH=main` in the install scripts today — requires `cargo`. "Nightly" means
  "the branch this repo currently treats as its rolling head," which is `main` today; the flag
  name is chosen so a future dedicated `nightly` branch doesn't require a CLI-facing rename.

## Prerequisite: `phs --version` / `-V`

Does not exist today — `phs --version` is parsed as a PHS script and fails with a syntax error.
Needed both as the thing `upgrade` compares against and as its own real gap. Add a check
alongside the existing `--help`/`-h` handling in `main()`, printing:

```
phs 0.2.3 (a1b2c3d)
```

`0.2.3` is `env!("CARGO_PKG_VERSION")` (tied to `workspace.package.version`, the same version
`core-release.yml` checks a `core-vX.Y.Z` tag against). `(a1b2c3d)` is the short commit SHA the
binary was built from, captured at compile time by a small `build.rs` (`git rev-parse
--short HEAD`, falling back to omitting the parenthetical entirely when not built inside a git
checkout — the crates.io sdist case). This closes a real gap the version number alone can't:
two `phs upgrade --nightly` builds a day apart both report `0.2.3` until the next version bump,
so without the commit suffix a user has no way to tell whether their nightly build is current.
The commit SHA is display-only — `upgrade`'s stable-path version comparison (below) still
compares plain semver against the release tag, never the SHA.

## `phs upgrade` (stable path)

1. `GET https://api.github.com/repos/Alexisrx96/physure/releases` (unauthenticated — the same
   endpoint and pattern `scripts/install.ps1` already uses). Find the first entry whose
   `tag_name` starts with `core-v` (releases are already newest-first).
2. Parse the version out of the tag (`core-v0.2.4` → `0.2.4`). Compare against
   `env!("CARGO_PKG_VERSION")` as real semver (`semver::Version`, not string equality — a
   `nightly`-built binary that hasn't had its version bumped yet still reads as `0.2.3`, same
   as the last release, so an exact-match check happens to work for that specific case, but a
   naive *inequality* check would not: it would treat "not textually equal" as "go ahead and
   install," which downgrades a `--nightly` build that's ahead of `0.2.3` in commits but not in
   version number back to the published `0.2.3` release with no warning). Three outcomes:
   - Release version == running version → `phs is already up to date (0.2.3).`, exit 0.
   - Release version > running version → proceed to install (step 3).
   - Release version < running version → `Running 0.2.4 is newer than the latest published
     release (0.2.3) -- probably a --nightly build. Nothing to do.`, exit 0. Never silently
     downgrades.
3. Newer release available → pick the platform asset name from `core-release.yml`'s own matrix:

   | Platform | Asset |
   |---|---|
   | Windows x86_64 | `phs-windows-x86_64.zip` |
   | Linux x86_64 | `phs-linux-x86_64.tar.gz` |
   | Linux aarch64 | `phs-linux-aarch64.tar.gz` |
   | macOS x86_64 | `phs-macos-x86_64.tar.gz` |
   | macOS aarch64 | `phs-macos-aarch64.tar.gz` |

   No match (unsupported platform) → clear error naming the platform, suggesting `--nightly`
   (which builds locally instead of downloading a prebuilt asset) or manual install.
4. Download the asset's `browser_download_url` to a temp directory.
5. Extract by shelling out to the system `tar` binary (`tar -xf <archive> -C <dir>`) — bsdtar,
   which every target platform ships by default (Windows 10 1803+, macOS, Linux), reads both
   `.zip` and `.tar.gz` through the same `-xf`, so this needs no new archive-handling crate.
6. For each of `phs`/`physure-lsp` found in the extracted directory, replace the installed copy
   (see "Replacing a binary that might be running" below).
7. Print what changed: `phs: 0.2.3 -> 0.2.4`, and `physure-lsp: <old or "not found"> -> 0.2.4`
   if a sibling `physure-lsp` was located and updated.

## `phs upgrade --nightly`

For a nightly build, the running binary's identity *is* its commit SHA (the `(a1b2c3d)` from
`phs --version`'s `build.rs`) — there is no meaningful semver to compare, `main` doesn't bump
`workspace.package.version` on every commit. So "is there anything to do" is a different
question from the stable path's version check: not "is a newer version published" but "did
`main` move, and if so, did it move in a way that changes what this binary actually is."

1. Confirm `cargo` is on `PATH` (`where`/`which`). Missing → the same message
   `install.ps1`/`.sh` give: point at https://rustup.rs, exit 1, no partial state changed.
2. If the running binary has no embedded commit SHA (built outside a git checkout — the
   crates.io sdist case, see the `--version` section above), there is no baseline to diff from
   → skip straight to step 5 and rebuild unconditionally, same as today's plain `cargo install`.
3. `GET https://api.github.com/repos/Alexisrx96/physure/commits/main` → `.sha`, the latest
   commit on `main`. Equal to the running binary's embedded SHA → `phs is already on the
   latest commit (a1b2c3d).`, exit 0, no rebuild.
4. Different → `GET https://api.github.com/repos/Alexisrx96/physure/compare/{running_sha}...
   {latest_sha}` and check `.files[].filename` for any path starting with `physure-core/`,
   `physure-script/`, `physure-cli/`, or `physure-lsp/` — the four crates that actually compile
   into `phs`/`physure-lsp` (`physure-script` implements the language itself and both binaries
   depend on it directly; leaving it out would miss most of what a typical commit here actually
   changes — confirmed against this session's own history, where nearly every fix touched
   `physure-script`, not `physure-core`). None of the changed files fall under those four
   directories → `main` moved (`a1b2c3d` -> `e5f6a7b`) but nothing that affects this binary
   changed -- `phs is already effectively up to date.`, exit 0, no rebuild. The compare call
   failing for any reason (network error, unreachable SHA, rate limit) is not a reason to
   refuse an upgrade the user asked for -- fall through to step 5 exactly as if it had reported
   relevant changes.
5. Relevant changes found (or the check was skipped/inconclusive per steps 2/4) → for each of
   `phs`/`physure-lsp`: make way for cargo to write cleanly (see below), then run `cargo install
   --git https://github.com/Alexisrx96/physure --branch main <crate> --bin <bin> --locked
   --force` as a child process with inherited stdout/stderr, so the user sees normal `cargo
   install` build output — matching how `cargo install` already prints when a user runs it
   directly (this session ran it that way for both `phs` and `physure-lsp`).

## Replacing a binary that might be running

The concrete problem this session hit twice (`cargo install`'s own `Acceso denegado` failures
rebuilding `target/release/phs.exe`): Windows refuses to delete or directly overwrite a file
that's the current process's own executable, or one another running process has open (e.g.
`physure-lsp.exe` while VS Code's LSP client still has it launched). Unix has no such
restriction — unlinking a running executable's file is fine, the process keeps running off the
now-nameless inode.

Handled with a small platform-difference-absorbing helper (no new dependency — this is the one
well-documented OS-level trick, not a general problem worth pulling in a crate for):

```rust
/// Makes `target` writable even if it's currently executing (this process's own exe, or one
/// another process has open) -- Windows allows *renaming* an in-use executable even though it
/// refuses to delete or overwrite it directly; the renamed file keeps running until whichever
/// process has it open exits. Unix needs nothing: removing/overwriting a running executable's
/// file is always fine there, so this is a no-op except on Windows.
fn make_way_for(target: &Path) -> io::Result<()> {
    if cfg!(windows) && target.exists() {
        let stale = target.with_extension("old");
        let _ = fs::remove_file(&stale); // best-effort: a leftover from a previous upgrade
        fs::rename(target, &stale)?;
    }
    Ok(())
}
```

- **Stable path**: `make_way_for(&installed_path)`, then `fs::copy(&extracted_path,
  &installed_path)`.
- **Nightly path**: `make_way_for(&cargo_bin_path)` *before* invoking `cargo install --force`,
  so cargo (which has no idea this trick exists) writes into a path Windows will actually let
  it write to. Without this, `phs upgrade --nightly` run from the exact binary `cargo install`
  would overwrite hits the identical file-lock error `cargo install` hit twice this session.

`installed_path` for `phs` is always `std::env::current_exe()` — whichever copy is actually
running gets upgraded, avoiding any ambiguity between `~/.local/bin`, `~/.cargo/bin`, or a
custom location on `PATH`. For `physure-lsp`: check the same directory as `phs`'s
`current_exe()` first (where the official installer places them side by side), then
`~/.cargo/bin`; not found in either → skip it and say so, nothing to upgrade.

The renamed `.old` file is deleted on success (best-effort; if Windows still has *that* handle
open too, it's a harmless leftover, not worth failing the upgrade over).

## New dependencies

- `ureq` (small, synchronous HTTP client, rustls-based — no system OpenSSL requirement) for the
  GitHub API call and the asset download.
- `semver` for the version comparison above (parsing and ordering `X.Y.Z` correctly rather than
  treating it as an opaque string).

Nothing else: archive extraction shells out to the system `tar` (see above), and the release
JSON is parsed with `serde_json`, already a dependency. `physure-cli` has no "zero dependencies"
policy (that's specific to `physure-python`'s runtime deps and `physure-core`'s FFI-freedom) —
these are normal crate additions to `physure-cli/Cargo.toml`.

## Command surface

Follows the crate's existing manual-dispatch style (`args[1] == "new-plugin"` etc. in
`main.rs`, each subcommand living in its own module) — no new argument-parsing framework:

```rust
if args[1] == "upgrade" {
    let nightly = args.iter().any(|a| a == "--nightly");
    upgrade::run_upgrade(nightly);
    return;
}
```

New file `physure-cli/src/upgrade.rs`.

## Testing strategy

- Unit tests for the pure logic that doesn't need the network or a real binary swap: parsing a
  version out of a `core-vX.Y.Z` tag, picking the platform asset name (one assertion per
  platform), all three stable-path version-comparison outcomes (equal, newer available, running
  ahead of the latest release), and the nightly relevant-path filter — given a list of changed
  filenames, correctly says "relevant" for a path under any of the four crate directories and
  "not relevant" for one that touches only, say, `physure-python/` or `docs/`.
- `make_way_for`: a real filesystem test — create a temp file, rename it out from under an open
  handle to it (simulating "this is my own running exe" on the platform that matters, skipped
  with a clear reason on non-Windows since the behavior it exists to test is Windows-specific),
  confirm the original path is free and the handle is still valid.
- No test hits the real GitHub API or downloads a real release — the HTTP call and the `tar`
  extraction step are integration-level and platform/network-dependent; verified manually
  against the real API and a real release asset once implemented, not asserted in CI.

## Explicitly out of scope

- Prebuilt nightly binaries / a CI workflow building on every `main` push (asked and declined —
  `--nightly` builds from source via `cargo install`, matching the existing install scripts).
- Auto-upgrading `physure` (the Python package) or the VS Code extension itself — this is the
  `phs`/`physure-lsp` binaries only.
- Signature/checksum verification of downloaded release assets beyond what GitHub's own HTTPS
  transport already provides.
- Rewriting `scripts/install.*` to call the new `phs upgrade` internally, or vice versa — they
  solve the "no `phs` yet" bootstrap case and the "`phs` already installed" case respectively;
  changing the bootstrap scripts is out of scope here.
