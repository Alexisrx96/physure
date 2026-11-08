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
commit = 9f2c1ab
profile = release
target = x86_64-pc-windows-msvc
features = html-viewer
binary_sha256 = …
policy = build

[Environment]
os = windows
arch = x86_64
rayon_threads = 16

[Witness]
builtin.conversion.m_to_ft = <bits>
user.validation.1 = <sha256>

[History]
1 = <entry, §5.3>

@@ source 1 origin=embedded sha256=…
…the embedded catalog text…
@@ end source 1
@@ source 2 origin=overrides sha256=…
# Your overrides: same grammar as physure.conf. After editing, run `phs conf seal`.
@@ end source 2
@@ witness user.validation.1 sha256=…
…script text…
@@ end witness
```

* `source 2` is **always** the overrides block and is the one meant to be edited by hand. When `freeze` finds an existing `physure.conf`, its text becomes the content of `source 2`; when there is none, the block is created empty with a comment stating the grammar. Further sources (system files) follow it and are added by the bindings that use them (Phase 5).
* Line endings are normalised to LF on write; reading accepts CRLF.
* The header sections follow the strict grammar of the conf (no inline comments; a bad line is an error). `policy` is one of `binary`, `build`, `release`, `record` (§7); from Python `[Environment]` also has `python`, `numpy` and `torch`.

### 4.1 Canonical form and hashes

* The **canonical text** of a source: lines trimmed at both ends, blank lines and lines starting with `#` or `;` dropped, joined with `\n`. Reformatting and comments do not change it; one digit does.
* `source_sha256` = SHA-256 of the canonical text.
* `content_sha256` = SHA-256 of, for each source in order, `origin\n` + `source_sha256\n`.
* `seal_sha256` = SHA-256 of `"physure-frozen-v1\n"` + `content_sha256` + the canonical `[Engine]` block + the canonical `[Witness]` block + the hash of the last history entry. Any edit of data, engine, witnesses or history breaks the seal. `[Environment]` is informational and outside the seal.
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
n | utc time | action (freeze|update|seal) | engine version+commit | actor | reason
  | old content_sha256 | new content_sha256 | diff summary (counts per section) | previous entry sha256
```

and the entry's own SHA-256 is recorded with it. Deleting, reordering or editing an entry breaks the chain. `actor` is `$USER`/`$USERNAME` and the host name; it is **informational, not authenticated**.

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

* Core (`physure-core/src/frozen/`: `sha256`, `canonical`, `model`, `seal`, `time` in Phase 1a; resolution, sealed loading and the engine identity in 1b/1c): file model, canonical form, hashes, verification, resolution, sealed loading, the entry listing, the engine identity. Every surface that builds a registry (`build_registry_from_conf`, `with_conf_registry`, PHS, the CLI, the language server, the Rust API) goes through the resolution, so they cannot disagree about what is in force.
* Build metadata: a `build.rs` in the core exports the commit, profile, target and features that the engine block needs. A build outside a git checkout records `commit = unknown`, and a policy of `build` or stricter then refuses to freeze with it.
* CLI (`physure-cli/src/conf.rs`): the eight commands over the core API.
* Python, WebAssembly and Java call the same resolution (Phase 5). Python reads its configuration with `configparser`, adds system files and its own working-directory file, so wiring it is its own task.

## 7. Engine pinning

`[Engine] policy` — set by `freeze --engine-policy`, default `build`, changed only through `update`/`seal` (so it is in the history):

| Policy | The running engine must have |
|---|---|
| `binary` | the same executable (SHA-256) |
| `build` (default) | the same version, commit, profile, target and features |
| `release` | the same version |
| `record` | nothing (the identity is recorded, never enforced) |

A mismatch is an error (exit 3) **when evaluating**. `status`, `verify` and `diff` accept `--allow-engine-mismatch` so that a different build can still inspect the file. There is no override for evaluating: to run on another engine, `update` (which records the change, with its reason, in the history).

## 8. Witnesses

A witness is a reference calculation whose result is stored bit for bit and recomputed by `verify --run` under the frozen configuration. It catches what an engine pin cannot: the same binary on a different CPU or C library giving different last digits, and a change that the version string does not reflect.

* **Built-in battery** (about 40 cases; the list is part of the plan of Phase 3): unit conversions including affine and logarithmic units, uncertainty propagation in each mode with a fixed seed, the Welch–Satterthwaite example of the GUM (H.1), Student-t quantiles at several ν, molar masses, reaction balancing, Arrhenius, the gas constant read from the catalog. Each stores the mean and the standard uncertainty as IEEE-754 bit patterns and the unit text.
* **User witnesses:** `freeze --witness validation.phs`. The script is embedded in the frozen file (it is self-contained), evaluated with the same machinery as `phs eval` (JSON), and the canonical result is hashed. `verify --run` re-evaluates the embedded text.

## 9. Security model and limits (to be stated in the user documentation)

* A hash detects that a file changed; it does **not** show who wrote it: whoever edits the file can recompute the seal. For authenticity, record the seal outside the file (`freeze` prints it; `verify --expect` compares). Cryptographic signing with a key is a possible later improvement and is out of scope.
* `actor` in the history is informational.
* Environment differences are warnings, not errors: bit-for-bit reproducibility across machines is not promised, and the witnesses are how a difference is detected.
* What is frozen is data; algorithms are identified, pinned and witnessed, not preserved. To reproduce a result years later, keep the binary named by `[Engine]`.
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
