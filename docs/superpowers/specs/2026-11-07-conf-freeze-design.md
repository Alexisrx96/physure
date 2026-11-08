# `phs conf`: freezing the whole configuration — design

**Status:** design approved section by section by the user on 2026-11-07; this document is the written spec awaiting their review.
**Scope of this spec:** the mechanism and its first four phases of work (§11). Each phase gets its own implementation plan.

## 1. Why

Some users run physure in strictly audited settings (factories, laboratories). They validate their calculations against **one** state of the software and its data, and changing that state needs a long process (paperwork, months of testing) that does not follow the release rhythm of this project. Today a library upgrade can change a result without anyone choosing it: a constant is revised, a unit definition is corrected, an algorithm is improved. For these users that is not an improvement, it is an unannounced alteration of validated calculations.

The atomic weights already have the answer in miniature: named, frozen editions, a fingerprint test, an edition report (`docs/atomic_weights.md`, §6). This spec generalises it to **everything that determines a result**, and adds the tooling to freeze, inspect, compare and update it deliberately:

```
phs conf            phs conf show      phs conf freeze     phs conf verify
phs conf diff       phs conf update    phs conf seal       phs conf history
```

The user's requirements, in their words (translated): it must not be limited to chemistry, it must cover **everything**; freezing with nothing configured must build the frozen file that holds the overrides; and every change must be deliberate and leave a trace.

## 2. What determines a result (three layers)

| Layer | What it is | How it is frozen |
|---|---|---|
| **Data** | `physure.conf` (settings, dimensions, prefixes, units, constants, categories, compatibility, atomic weights), unit-system files, and any number that today lives only in code | **Copied** into the frozen file and sealed. |
| **Engine** | The algorithms of the core: uncertainty propagation, Student-t quantiles, balancing, parsing… A data file cannot hold an algorithm. | **Identified and pinned** (version, commit, build, target, binary hash) with an enforcement policy, and **witnessed** by recorded reference calculations (§8). |
| **Environment** | OS, CPU architecture, thread count, and from Python the versions of Python, numpy and torch | **Recorded**; `verify` reports differences as warnings. |

Terminology used throughout: *layer* means one of these three; *source* means one stacked piece of configuration text (§3).

### What is not covered today (stated plainly, and kept current in `docs/conf_coverage.md`, Phase 4)

Verified in the code at the time of writing: the exchange rates of `physure.ext.currency` are literals in Python (`EUR = 1.08`, `MXN = 0.059`); the default number of Monte Carlo samples is a literal in the core (`physure-core/src/quantity.rs`, `samples.unwrap_or(1000)`); the unit systems `imperial.conf` and `international.conf` are read by Python only; the Python chemistry tables (`STANDARD_ENTHALPY_FORMATION`, `STANDARD_ENTROPY`) are literals. Phase 4 starts with a complete audit of code-resident numbers and migrates each into the catalog; until a number is migrated it is listed as **not frozen**. Nobody may be led to believe that something is covered when it is not.

## 3. Sources: how the configuration is built today

The effective configuration is not one file; it is a stack of sources applied in order, a later one overriding an earlier one:

1. the catalog embedded in the library (`physure-core/src/units/physure.conf`);
2. a unit system file, if used (Python: `systems/imperial.conf`…);
3. the `physure.conf` of the working directory (the core reads exactly this one; the CLI's `config.rs` looks in five places, but only for the language).

Different surfaces resolve this differently (the core reads only the working directory; Python adds system files). The mechanism needs **one definition of "the configuration in force"**, implemented once in the core and reused by every surface (§6).

A frozen file keeps the **sources as they were read, separately and in order**, instead of a merged result. Reasons: nothing is transcribed (a registry-to-text serialiser would have to be lossless for every unit kind, scale, offset, level and compatibility row, and any slip would alter a calculation); an auditor sees where a value comes from; loading is a replay.

## 4. The frozen file, `physure.frozen.conf`

The name is deliberately different from `physure.conf`: an older tool or another binding that reads the working directory's `physure.conf` leniently must never take a frozen file for a plain overlay and silently ignore its seal and engine pin.

It is **not** a plain conf. The first line is `#physure-frozen 1`. A header in INI syntax is followed by framed blocks whose text is kept verbatim:

```
#physure-frozen 1
[Freeze]
format = 1
created = 2026-11-07T14:03:11Z
content_sha256 = …
seal_sha256 = …

[Engine]
version = 0.3.1
commit = 9f2c1ab3d4e5f60718293a4b5c6d7e8f9a0b1c2d
profile = release
target = x86_64-pc-windows-msvc
features = html-viewer
rustc = rustc 1.97.1 (8bab26f4f 2026-07-14)
flags = opt-level=3;debug-assertions=off;panic=unwind;rustflags=<sha256>;target-features=<sha256>
binary_sha256 = …
policy = build

[Environment]
os = windows
arch = x86_64
family = windows
endian = little
pointer_width = 64
cpus = 16

[Witness]
builtin.conversion.m_to_ft = <bits>
user.validation.1 = <sha256>

[History.1]
time = 2026-11-07T14:03:11Z
action = freeze
engine = 0.3.1+9f2c1ab3d4e5
engine_sha256 = …
actor = irvin@host
reason = initial freeze
old_content_sha256 =
new_content_sha256 = …
diff = …
prev_entry_sha256 =
entry_sha256 = …

@@ source 1 origin=embedded sha256=…
…the embedded catalog text…
@@ end source 1
@@ source 2 origin=overrides sha256=…
# Your overrides: same grammar as physure.conf. After editing, run `phs conf seal`.
@@ end source 2
@@ witness validation.1 sha256=…
…script text…
@@ end witness
```

* `source 2` is **always** the overrides block and is the one meant to be edited by hand. When `freeze` finds an existing `physure.conf`, its text becomes the content of `source 2`; when there is none, the block is created empty with a comment stating the grammar. Further sources (system files) follow it and are added by the bindings that use them (Phase 5).
* **Structure rules** (refused by the constructor, reported by the verifier, refused by the parser where they are syntactic): source 1 has origin `embedded` and source 2 has origin `overrides`, both always present; origins are unique; witness script names are unique; `[Witness]` has a key `user.<name>` for every script `<name>` and no `user.*` key without a script; `created` equals the time of the first history entry; the first entry is a `freeze` with empty `old_content_sha256` and `prev_entry_sha256`, every later entry is an `update` or a `seal` whose `old_content_sha256` is the previous entry's `new_content_sha256`; every reason is 1 to 500 characters. A section opened twice, a key repeated in a section, a number that is not written canonically (digits only, no sign, no leading zero) and a line starting with `@@ ` inside a block that is not that block's end marker are parse errors that name the line.
* Line endings are normalised to LF on write; reading accepts CRLF. A bare carriage return (one not followed by a line feed) is refused, in the file and in a source or script given to the library. Empty lines at the end of a block are not part of its text.
* The header sections follow the strict grammar of the conf (no inline comments; a bad line is an error). `policy` is one of `binary`, `build`, `release`, `record` (§7); from Python `[Environment]` also has `python`, `numpy` and `torch`.
* **`[Engine]` is read strictly** (`frozen::engine::read_engine`): the keys `version`, `commit`, `profile`, `target`, `features`, `rustc`, `flags` and `policy` are required, `binary_sha256` is optional, and **any other key is an error** (an unknown key in the block that sets the policy would be a knob nobody can see). Every value is validated: `version` non-empty `[0-9A-Za-z.+-]`; `commit` exactly 40 or 64 lowercase hexadecimal characters or the literal `unknown`; `profile` `[a-z0-9_-]+`; `target` `[A-Za-z0-9_.-]+`; `features` empty or sorted, unique, comma-separated `[a-z0-9-]+` names; `rustc` and `flags` one non-empty line without padding; `binary_sha256` a lowercase SHA-256. `verify` reads the block: a sealed file whose block cannot be read is a discrepancy, and such a file can be neither made nor resealed.
* `[Environment]` records `os`, `arch`, `family`, `endian`, `pointer_width` and `cpus` (`frozen::engine::environment_pairs`). It is informational, outside the seal, and no behaviour may ever depend on it.

### 4.1 Canonical form and hashes

* The **canonical text** of a source: lines trimmed at both ends, blank lines and lines starting with `#` or `;` dropped, joined with `\n`. Reformatting and comments do not change it; one digit does. This form is sound only because it is **defined by the core's reader** (`physure-core/src/units/conf.rs`, which trims every line and skips `#`/`;` lines in every section): two texts with the same canonical text are read identically by it. Sources are therefore replayed through the core reader only. Python's `configparser` reads an indented line as the continuation of the previous value, so two texts the seal treats as equal could mean different things to it: Phase 5 must not use it to replay a frozen source.
* `source_sha256` = SHA-256 of the canonical text.
* `content_sha256` = SHA-256 of, for each source in order, `origin\n` + `source_sha256\n`.
* **Witness scripts are hashed byte-exact**, never with the canonical form above (in PHS `;` ends a statement, indentation is significant and a string may span lines): `exact_sha256(text)` = SHA-256 of the script text after CRLF becomes LF and trailing newlines are removed, with no other change.
* `engine_sha256` and `witnesses_sha256` = SHA-256 of the canonical text of the block: its `key=value` lines sorted and joined with `\n` (keys are unique names without `=`, values are one line). The same `engine_sha256` is recorded in every history entry, for the block as it was at that entry (§5.3).
* `scripts_sha256` = SHA-256 of, for each witness script in file order, `<name>\n` + `exact_sha256(script)\n` (the empty string when there is none).
* `seal_sha256` = SHA-256 of exactly this text, each line ending with `\n`:

  ```
  physure-frozen-seal-1
  format:<format>
  created:<created>
  content:<content_sha256 recomputed from the source texts>
  engine:<engine_sha256>
  witnesses:<witnesses_sha256>
  scripts:<scripts_sha256>
  history:<entry_sha256 of the last history entry>
  ```

  Every part is a labelled fixed-length hash or a one-line scalar, so no pair, script or boundary can move from one part to another. The history is covered through its last entry: each entry's hash covers the previous entry's hash (§5.3). What is outside the seal is listed in §9.
* SHA-256 is implemented in the core (about 80 lines) and checked against the NIST test vectors: the core keeps its policy of no new dependencies.

## 5. Behaviour

### 5.1 Resolution: "the configuration in force"

In this order, implemented once in the core (`units::frozen`):

1. an explicit file: `--conf PATH` (CLI) or the argument of the API call;
2. the environment variable `PHYSURE_CONF=PATH`;
3. discovery: the nearest `physure.frozen.conf` found walking up from the working directory to the root of the file system (as git finds `.git`);
4. none: today's behaviour (embedded catalog plus the working directory's `physure.conf`) — **unless** `PHYSURE_REQUIRE_FROZEN=1`, which makes the absence of a frozen file an error (exit code 2): the audited setting fails closed.

An explicit path that does not exist, or is not a frozen file, is an error; it never falls back. The result of the verification is cached per process and per (path, size, modification time); a change on disk is re-verified.

### 5.2 Loading a frozen file (sealed)

1. Parse the header and the blocks. Any malformed framing is an error.
2. Recompute every `source_sha256`, `content_sha256`, the history chain and `seal_sha256`. **Any mismatch is an error, never a warning**, and the message names the file, what was expected and what was found.
3. Check the engine against `[Engine]` according to `policy` (§7).
4. Build the registry **only** from the sources, replayed in order and read strictly (a bad line is an error). The `embedded` source is read with the trusted reader (the one entitled to define the shipped atomic-weight editions); every other source is read as untrusted user text. The catalog of the installed library and the working directory's `physure.conf` are **not read**; `status` warns if the latter exists. `[Settings]` of the sources are applied, later ones winning.

An upgrade of the library therefore cannot change a value: the sources do not come from the library.

### 5.3 The history

Every `freeze`, `update` and `seal` appends one entry; entries are chained:

```
n | utc time | action (freeze|update|seal) | engine version+commit | engine_sha256 | actor | reason
  | old content_sha256 | new content_sha256 | diff summary (counts per section) | previous entry sha256
```

and the entry's own SHA-256 is recorded with it: `entry_sha256` = SHA-256 of `n=<n>\ntime=<time>\naction=<action>\nengine=<engine>\nengine_sha256=<engine_sha256>\nactor=<actor>\nreason=<reason>\nold_content_sha256=<…>\nnew_content_sha256=<…>\ndiff=<diff>\nprev_entry_sha256=<…>` (every value one line, no final newline). Deleting, reordering or editing an entry breaks the chain.

`engine` is the short form (`version+` twelve characters of the commit) of the engine that performed the action, free text; `engine_sha256` is the SHA-256 of the canonical `[Engine]` block (§4.1) at the moment of the entry, computed by the library, never given by the caller. A change of the engine block, weakening the policy included, therefore shows in the log mechanically as a different `engine_sha256` between consecutive entries, whatever the free-text `diff` says. `verify` requires the last entry's `engine_sha256` to equal the hash of the current block: a block changed without a reseal is reported, and so is a file from which the entry that changed the block was removed even when every other hash and the seal were recomputed. `actor` is `$USER`/`$USERNAME` and the host name; it is **informational, not authenticated**.

### 5.4 Commands

All accept `--conf PATH` and `--json`. Exit codes: **0** ok (or identical, for `diff`); **1** discrepancy (broken seal, modified file, differences, witness mismatch); **2** misuse, or a required frozen file is missing; **3** engine differs from the validated one. Every error names the file, what was expected and what was found; nothing falls back silently to "not frozen".

| Command | Behaviour |
|---|---|
| `phs conf` / `status` | Which configuration is in force and why (flag, variable, discovery, none); frozen or not; seal and verification; creation date and engine; number of history entries; atomic-weight edition (`atomic_weight_edition()`); warnings: an ignored `physure.conf`, a missing required frozen file, environment differences. |
| `show [KEY]` | The effective configuration as `section.key = value`, whole, one section, or one key (`constants.molar_gas_constant`). `--origin` adds the source each value comes from. |
| `freeze --reason "…" [--out PATH] [--witness FILE.phs]…` | Writes `physure.frozen.conf` from what is in force now; refuses to overwrite an existing frozen file (that is `update`). With **no** `physure.conf` it still writes the file, with `source 1` = the embedded catalog and an empty `source 2`. Records the engine (policy `build`), the environment and the witnesses (§8), and prints the seal, reminding the user to record it outside. |
| `verify [FILE] [--expect SHA] [--run]` | Recomputes seal, hashes and chain; compares with `--expect` (a seal kept in the user's quality system, §9); reports environment differences; with `--run` re-executes the witnesses under the frozen configuration. |
| `diff [A] [B]` | Default: A = the frozen file in force, B = what the installed build would use unfrozen. Data differences by section (added, removed, changed with old → new), then engine and environment differences, with counts. `--json` has a stable schema. Exit 0 identical, 1 different. |
| `update --reason "…" [--yes]` | Adopt what the installed build brings (new catalog, new engine). Shows the `diff`, does nothing if it is empty, asks for confirmation (`--yes` in scripts), writes atomically (temporary file and rename) keeping the overrides, appends a history entry. |
| `seal --reason "…" [--yes]` | Accept the user's hand edit of the `overrides` block (or any hand edit): shows the diff against the last sealed state, asks for confirmation, re-seals, appends an entry. Until then `verify` says "modified since the last seal" and what changed. This is the **only** way to change overrides: no `set`/`unset`. |
| `history` | Prints the log. |

There is no `unfreeze`: deleting the file does it, and with `PHYSURE_REQUIRE_FROZEN=1` that fails closed. `phs upgrade` never touches a frozen file; when one is in force it ends by suggesting `phs conf diff`.

`show` and `diff` need a deterministic **entry listing** of a registry (`section`, `key`, canonical value text) for every kind of definition; the origin of an entry is found by listing after each source and attributing the change to it.

## 6. Where it lives

* Core (`physure-core/src/frozen/`: `sha256`, `canonical`, `model`, `seal`, `time` in Phase 1a; `engine`, the engine identity and its policies, in 1b; resolution and sealed loading in 1c): file model, canonical form, hashes, verification, resolution, sealed loading, the entry listing, the engine identity. Every surface that builds a registry (`build_registry_from_conf`, `with_conf_registry`, PHS, the CLI, the language server, the Rust API) goes through the resolution, so they cannot disagree about what is in force.
* Build metadata: `physure-core/build.rs` **always** emits the commit, profile, target, compiler (`rustc -V`) and build flags under internal names (`PHYSURE_ENGINE_*`) that the library reads with `env!`; the library never reads a variable a user can set, so a value the build script rejected cannot reach the binary. `features` is computed in the code: the sorted named features of `physure-core` that are enabled, without `default`. `flags` is `opt-level=<n>;debug-assertions=<on|off>;panic=<unwind|abort>;rustflags=<sha256 of CARGO_ENCODED_RUSTFLAGS>;target-features=<sha256 of CARGO_CFG_TARGET_FEATURE>`.
* **The commit rule** (a pure function, `build_support/commit.rs`, unit-tested; a commit is exactly 40 or 64 lowercase hexadecimal characters, or the literal `unknown`; a false commit is worse than none, so every doubt gives `unknown`). The variable `PHYSURE_BUILD_COMMIT` is an input of the build only. (1) The variable is set but is not a full lowercase hash: `unknown`, with a cargo warning. (2) Git can answer (the crate sits directly at the root of the repository and `git rev-parse HEAD` succeeds) and tracked files this build uses (the crate and the workspace `Cargo.toml` and `Cargo.lock`) have uncommitted changes: `unknown`, with a warning — **build from a clean checkout**. (3) Git can answer and the tree is clean: the commit is `HEAD`; if the variable is also set and differs, `unknown` with a warning. (4) Git cannot answer (a crates.io tarball, a copy vendored inside another repository, no git): a valid variable is used, otherwise `unknown`. The build script watches only paths that exist (found with `git rev-parse --git-path`), so a git worktree does not rebuild the crate on every invocation.
* A build whose commit is `unknown` cannot record the policy `build` or `binary` (§7).
* CLI (`physure-cli/src/conf.rs`): the eight commands over the core API.
* Python, WebAssembly and Java call the same resolution (Phase 5). Python reads its configuration with `configparser`, adds system files and its own working-directory file, so wiring it is its own task.

## 7. Engine pinning

`[Engine] policy` — set by `freeze --engine-policy`, default `build`, changed only through `update`/`seal` (so it is in the history: each entry's `engine_sha256` covers the policy, §5.3):

| Policy | The running engine must have |
|---|---|
| `binary` | the same file that contains the engine (SHA-256 of `binary_sha256`) |
| `build` (default) | the same version, commit, profile, target, features, compiler (`rustc`) and build flags (`flags`) |
| `release` | the same version |
| `record` | nothing (the identity is recorded, never enforced) |

A mismatch is an error (exit 3) **when evaluating**. `status`, `verify` and `diff` accept `--allow-engine-mismatch` so that a different build can still inspect the file. There is no override for evaluating: to run on another engine, `update` (which records the change, with its reason, in the history).

The comparison (`frozen::engine::check`) reports every field that differs, and every doubt is a difference:

* **`unknown` never equals anything**, not even another `unknown`: under `build`, a recorded or running commit that is `unknown` is a mismatch.
* `binary` and `build` cannot be **recorded** for an identity whose commit is `unknown` (`freezable`, enforced when the block is built by `engine_pairs`). This is a design choice: a binary hash alone does not tell an auditor which source it came from. `release` and `record` can always be recorded.
* `binary` needs a recorded `binary_sha256`; without one it is a mismatch, and the running file is hashed only under `binary` and only when a hash is recorded. `binary` attests **the file at its path at check time**, not the image in memory (on Windows a running executable can be renamed and replaced). The caller must hash the file that contains this engine: for the CLI the executable, for a Python extension module the module (not the interpreter), for the Java library the library. The core deliberately has no "hash my own executable" convenience, which in a binding would pin the interpreter.
* Under `build`, a recorded `binary_sha256` is informational and ignored.

**Trust boundary.** Whoever controls the build environment (the variable `PHYSURE_BUILD_COMMIT`, `PATH`, `RUSTFLAGS`, the toolchain) controls every field of the identity. `build` protects against accidents, not against a malicious builder; `binary` is the only policy whose evidence is the artefact itself.

**What the policies do not prove.** Not visible to a build script and therefore not pinned by `build`: `overflow-checks`, `lto`, `codegen-units`; the versions of the dependencies when `physure` is built inside someone else's workspace (their `Cargo.lock` decides); dynamically linked libraries (libm, the C runtime, the interpreter, numpy, torch). A dirty tree is detected only for **tracked** files of this crate and the workspace manifests. `release` accepts development builds that share a release's version number (between releases the version is not bumped). A copy of the crate placed directly under the root of another repository reports that repository's commit. The witnesses (§8) are how a difference in what is not pinned is detected.

## 8. Witnesses

A witness is a reference calculation whose result is stored bit for bit and recomputed by `verify --run` under the frozen configuration. It catches what an engine pin cannot: the same binary on a different CPU or C library giving different last digits, and a change that the version string does not reflect.

* **Built-in battery** (about 40 cases; the list is part of the plan of Phase 3): unit conversions including affine and logarithmic units, uncertainty propagation in each mode with a fixed seed, the Welch–Satterthwaite example of the GUM (H.1), Student-t quantiles at several ν, molar masses, reaction balancing, Arrhenius, the gas constant read from the catalog. Each stores the mean and the standard uncertainty as IEEE-754 bit patterns and the unit text.
* **User witnesses:** `freeze --witness validation.phs`. The script is embedded in the frozen file (it is self-contained), evaluated with the same machinery as `phs eval` (JSON), and the canonical result is hashed. `verify --run` re-evaluates the embedded text.

## 9. Security model and limits (to be stated in the user documentation)

* A hash detects that a file changed; it does **not** show who wrote it: whoever edits the file can recompute the seal. For authenticity, record the seal outside the file (`freeze` prints it; `verify --expect` compares). Cryptographic signing with a key is a possible later improvement and is out of scope.
* **What is outside the seal**, exactly: the `[Environment]` block, and formatting that carries no data: comment and blank lines and the whitespace around keys and values in the header, the order of the keys in a header section and of the sections, empty lines between and at the end of blocks, line endings, and in a source whatever its canonical form drops (comment lines, blank lines, indentation and trailing whitespace, §4.1). Nothing else: the format, `created`, the content of every source and its origin and position, `[Engine]`, `[Witness]`, every witness script byte for byte with its name and position, and every history entry are sealed.
* `actor` in the history is informational.
* Environment differences are warnings, not errors: bit-for-bit reproducibility across machines is not promised, and the witnesses are how a difference is detected.
* What is frozen is data; algorithms are identified, pinned and witnessed, not preserved. To reproduce a result years later, keep the binary named by `[Engine]`.
* The engine pin has the limits of §7, to be stated in the audit documentation: the identity is only as trustworthy as the build environment that produced it (`build` protects against accidents, not a malicious builder; `binary` is the only policy whose evidence is the artefact); `overflow-checks`, `lto`, `codegen-units`, a consumer's lockfile and dynamically linked libraries are not pinned; a dirty tree is detected only for tracked files of the crate; `release` accepts development builds sharing a version number; `binary` attests the file at its path when checked.
* The coverage table (`docs/conf_coverage.md`) is part of the guarantee: it says what is and is not frozen.

## 10. Testing

* SHA-256 against the NIST vectors; canonical form: comments and whitespace do not change it, one digit does.
* Tampering: editing a value, deleting a source, editing, deleting and reordering history entries, changing the recorded engine — each must break the seal and name what.
* Resolution and precedence: walking up, `PHYSURE_CONF`, `--conf`, `PHYSURE_REQUIRE_FROZEN`, a bad explicit path.
* A working-directory `physure.conf` is ignored while a frozen file is in force, and `status` says so.
* Engine policy matrix (four policies × matching/mismatching) and exit codes.
* Witnesses: altering a stored result is detected by `verify --run`.
* The sealed registry equals the registry built from the same sources unfrozen (differential test on the whole entry listing).
* CLI integration tests run the real `phs` and assert output and exit codes; documentation examples are executable (as for `docs/atomic_weights.md`).

## 11. Phases

Each is its own plan and is useful alone.

1. **Core:** SHA-256, canonical form, file model, history chain, resolution, sealed loading, engine identity and policies, entry listing.
2. **CLI:** the eight commands, JSON output, exit codes, `phs upgrade` hint.
3. **Witnesses:** the built-in battery, user witnesses, `verify --run`.
4. **Data migration:** audit of code-resident numbers (currency, Monte Carlo defaults, Python chemistry tables, others the audit finds); each moves into the catalog; `docs/conf_coverage.md`.
5. **Bindings:** Python, WebAssembly and Java honour the frozen file and report their environment (Python, numpy, torch versions); system files become sources.

## 12. Out of scope

Cryptographic signing; `diff --impact "H2O,CO2"` (numeric effect of a change on named molar masses; it can be added without changing anything above); anything about the user's own scripts beyond witnesses; freezing the versions of Python packages (recorded, not enforced).

## 13. Risks and open points

* The registry replay must reproduce the unfrozen registry exactly (the differential test of §10 guards it).
* Verification cost: a frozen file embeds the whole catalog (~900 lines); the SHA-256 of that is microseconds-to-milliseconds, cached per process.
* `[Settings]` are process-wide in the core: a frozen file applies them at first use, like the working-directory conf does today.
* A binary hash for a Python wheel means the extension module file; its definition is settled in Phase 5.
* The size of Phase 4 is unknown until its audit finds how many code-resident numbers exist; it must not block Phases 1–3.
* **Residual risks left by the independent review of Phase 1a** (each one has an owner phase; none is open silently):
  1. The soundness of the source hash depends on `physure-core/src/units/conf.rs` staying line-oriented (trim every line, skip `#`/`;` lines, no continuation lines). Nothing ties the two together today. **Phase 1c adds a test** that source texts with the same canonical form build the same registry, and any change to the conf grammar must update the canonical form in the same commit.
  2. History times are not checked to be non-decreasing, nor at or after `created`. **Phase 1c** adds the check to `verify`.
  3. The value of a `user.<name>` witness key is not tied to anything until witnesses are evaluated (**Phase 3**).
  4. `[Environment]` and header comments are outside the seal and must never drive behaviour in any later phase (a rule for reviewers of Phases 1b–5).
  5. `FrozenFile::verify` takes no expected seal: whoever does not hold the externally recorded seal can rewrite the whole file consistently. The comparison with the recorded seal is `phs conf verify --expect` (**Phase 2**), and it is what makes the guarantee meaningful.
* **Residual risks left by Phase 1b** (the engine identity; each one has an owner):
  1. Hashing a 30–100 MB executable or module with the in-house SHA-256 on every load would break the ~0.5 s first-use budget: the hash must be cached per process. **Phase 1c.**
  2. Which file to hash per binding (CLI `current_exe`, the Python extension module, the Java library), given that the core has no convenience for it. **Phase 5.**
  3. A language-server build without `html-viewer` has a different `features` string and will refuse a file frozen with `build` by another binding: document it, or choose `release`. **Phases 2 and 5.**
  4. The build inputs §7 lists as not pinned (`overflow-checks`, `lto`, `codegen-units`, a consumer's lockfile, dynamic libraries). **Documentation, Phase 2.**
  5. `release` accepts development builds that share a release's version number. **Phase 2 documentation, or a development-version rule.**
  6. `physure-cli/build.rs` names `../.git/...` paths that do not exist in a worktree, so the CLI and everything depending on it recompile on every invocation. **A separate small fix** (use `git rev-parse --git-path`, as `physure-core/build.rs` does).
  7. A copy of the crate placed at `<their repo>/physure-core` reports their commit (it does identify that tree, but an auditor will not find it in physure's history). **Documentation.**
  8. **Fixed in Phase 1b.** The dirty check used `git status`, which does not see changes to tracked files marked `assume-unchanged` or `skip-worktree` (measured): such a build claimed `HEAD` while compiling other code. `build.rs` now also reads `git ls-files -v` for the crate and the workspace manifests and treats a lowercase tag (assume-unchanged) or `S` (skip-worktree) as dirty (`commit_rules::hides_changes`, unit-tested); demonstrated: clean tree gives the full `HEAD`, an edited assume-unchanged file gives `unknown` with a warning, restoring it gives `HEAD` again. What this still cannot see: untracked files the build might `include!`, and changes outside the crate and the two workspace manifests.
