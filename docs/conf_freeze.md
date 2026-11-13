# Freezing the configuration (`phs conf`)

This guide is for sites where a calculation, once validated, must not change because the software was
upgraded: factories, laboratories, anything under an audit. It says what a frozen configuration is, what
the `phs conf` commands do (as built in Phase 2 of the
[design](https://github.com/Alexisrx96/physure/blob/main/docs/superpowers/specs/2026-11-07-conf-freeze-design.md)),
and, just as plainly, what a frozen configuration does **not** pin.

## 1. Why

An audited site validates its calculations against **one** state of the software and its data, and
changing that state takes paperwork and months of testing that do not follow this project's releases.
Without a freeze, an upgrade of physure can change a result without anyone choosing it: a constant is
revised, a unit definition corrected, an algorithm improved. For such a site that is not an improvement,
it is an unannounced alteration of a validated calculation.

A frozen configuration fixes the data that determines a result, pins the engine that computes it, records
the environment, and makes every later change deliberate and traced: who, when, why, and what changed.

## 2. The files

| File | What it is |
| --- | --- |
| `physure.conf` | The ordinary overlay of the working directory: units, constants, settings... read over the embedded catalog when nothing is frozen. |
| `physure.frozen.conf` | The frozen configuration. Its name is deliberately different, so that a tool that reads `physure.conf` leniently never mistakes it for an overlay. |

**Which configuration is in force** is decided the same way by every command, in this order:

1. `--conf PATH` (on `phs conf` commands);
2. the environment variable `PHYSURE_CONF=PATH`;
3. the nearest `physure.frozen.conf` found walking up from the working directory (as git finds `.git`);
4. none: the embedded catalog plus the working directory's `physure.conf`, **unless**
   `PHYSURE_REQUIRE_FROZEN=1`, which makes the absence of a frozen file an error (exit code 2): an audited
   site fails closed.

An explicit path or `PHYSURE_CONF` that does not name a file is an error and never falls back to
anything. `PHYSURE_REQUIRE_FROZEN` must be unset, `0` or `1`; any other value is an error, so a typo
cannot switch the audit mode off. While a frozen file is in force, the working directory's
`physure.conf` is **ignored** (`phs conf status` warns about it).

Because discovery walks up, a stray `physure.frozen.conf` in a parent directory (your home directory, a
parent of the repository) governs everything below it. `phs conf status` always says which file is in
force and why (`--conf`, `PHYSURE_CONF` or `discovery`).

## 3. What a frozen file contains

The first line is `#physure-frozen 1`, then a header in INI syntax, then framed blocks whose text is kept
verbatim. A file made by `phs conf freeze` looks like this (the embedded catalog is abridged):

```ini
#physure-frozen 1
[Freeze]
format = 1
created = 2026-11-13T04:59:07Z
content_sha256 = a1b2c48e...
seal_sha256 = b1844aad...

[Engine]
version = 0.3.0
commit = unknown
profile = debug
target = x86_64-pc-windows-msvc
features = html-viewer
rustc = rustc 1.97.1 (8bab26f4f 2026-07-14)
flags = opt-level=0;debug-assertions=on;panic=unwind;rustflags=e3b0c442...;target-features=50e03b9d...
policy = release

[Environment]
os = windows
arch = x86_64
family = windows
endian = little
pointer_width = 64
cpus = 12

[Witness]

[History.1]
time = 2026-11-13T04:59:07Z
action = freeze
engine = 0.3.0+unknown
engine_sha256 = e0492524...
policy = release
pinned = 0.3.0+unknown
actor = alice@lab-pc
reason = initial validation
old_content_sha256 =
new_content_sha256 = a1b2c48e...
diff = embedded 847 lines; overrides 2 lines
prev_entry_sha256 =
entry_sha256 = 76202900...

@@ source 1 origin=embedded sha256=260ff6fc...
...the catalog embedded in the library, as it was read...
@@ end source 1
@@ source 2 origin=overrides sha256=619a3b4b...
[Constants]
molar_gas_constant = 8.0 J*mol^-1*K^-1
@@ end source 2
```

* **The sources are kept verbatim, separately and in order**, not merged: source 1 is the catalog the
  library embedded when the file was made; source 2 is **the overrides**, your `physure.conf` at the time of
  the freeze (or an empty block with a comment when there was none). Loading the file replays these
  sources, and nothing else: the catalog of the installed library and the working directory's
  `physure.conf` are not read. That is why an upgrade cannot change a value.
* **Source 2 is the one block meant to be edited by hand.** After editing it, run `phs conf seal`.
* **`[Engine]`** pins the engine that computes (section 5).
* **`[Environment]`** records the machine. It is informational, outside the seal; differences are warnings.
* **`[Witness]`** is empty: witnesses (reference calculations recomputed bit for bit) are Phase 3.
* **The history** (`[History.n]`) has one entry per `freeze`, `update` and `seal`, each chained to the
  previous one by its hash: deleting, reordering or editing an entry breaks the chain. Each entry records
  the policy, the pinned identity and the hash of the `[Engine]` block as it was at that moment. `actor`
  is `$USER`/`$USERNAME` and the host name: informational, **not authenticated**.
* **The seal** (`seal_sha256`) covers the format, `created`, every source with its origin and position,
  `[Engine]`, `[Witness]`, the witness scripts and the whole history (through its last entry). Outside the
  seal are only `[Environment]` and formatting that carries no data: comment and blank lines, whitespace
  around header keys and values, the order of header keys and sections, line endings, and in a source
  whatever its canonical form drops (comments, blank lines, indentation, trailing whitespace). One digit
  changed anywhere else breaks the seal.

A frozen file's `[Settings]` and `[Compatibility]` are applied when it comes into force, over the
library's compiled defaults: a setting the file does not mention takes its compiled default.

## 4. The commands

Every command accepts `--json` (one JSON document on stdout, section 4.10) and every command but
`freeze` accepts `--conf PATH`. `phs conf --help` prints the full usage. The commands work on a broken file
or on a file pinned to another engine: that is what they are for, so `phs conf` is exempt from the check
that every other `phs` command makes before it runs (`phs: refusing to run: ...`). `show` is the one
exception, since it shows the configuration *in force*.

The exit codes are the same for all of them:

| Code | Meaning |
| --- | --- |
| `0` | ok (identical, for `diff`; nothing to do, for `update` and `seal`) |
| `1` | a discrepancy: a modified or unreadable file, a broken seal or chain, an `--expect` mismatch, differences (`diff`), sources that do not replay, a file changed while the command was asking |
| `2` | misuse (an unknown command or flag, a missing `--reason`, a malformed `--expect`), a missing required frozen file or a `--conf`/`PHYSURE_CONF` that names no file, a refused operation, a declined confirmation, no terminal and no `--yes`, nothing to compare |
| `3` | the engine differs from the one the file pins, and that is the only discrepancy |

Misuse is decided before any file is read.

### 4.1 `phs conf` / `phs conf status`

Which configuration is in force and why; whether it verifies; its seal, creation date, engine state,
number of history entries and atomic-weight edition; and the warnings: an ignored `physure.conf`, a
missing required frozen file, environment differences.

```console
$ phs conf
frozen configuration in force (discovery)
file: /srv/lab/physure.frozen.conf
seal: b1844aad0461a8b15bae3505155eb2b2c3333c3421bb3fd72625aab8c3326658
created: 2026-11-13T04:59:07Z
history: 1 entries
verification: the seal, the hashes and the chain hold
engine: matches (policy release, pinned 0.3.0+unknown)
atomic-weight edition: standard-2024
warning: /srv/lab/physure.conf is ignored: the frozen file is in force; to adopt an edit, change the overrides of the frozen file and run `phs conf seal`
```

With nothing frozen it says `no frozen configuration in force: the embedded catalog`, and whether a
`physure.conf` is read as an overlay of it. Exit: `0` sound or unfrozen; `1` the file does not read, does
not verify, its `[Engine]` block does not read, or its sources do not replay; `3` only the engine
differs; `2` a resolution error (`PHYSURE_REQUIRE_FROZEN=1` with no file, a `--conf` that names no file).
`--allow-engine-mismatch` is accepted and adds a warning; it never changes the exit code (3 stays 3).

### 4.2 `phs conf show [SELECTOR] [--origin]`

The configuration in force as `section.key = value`, sorted: all of it, one section (`constants`), one key
(`constants.molar_gas_constant`) or a key prefix. `--origin` adds the source each value comes from
(`embedded`, `overrides`, or `embedded (restated by overrides)` when the overrides set a value to what it
already was; unfrozen, `embedded` or `physure.conf`).

```console
$ phs conf show constants.molar_gas_constant --origin
constants.molar_gas_constant = 8.0 J*mol^-1*K^-1  [overrides]
```

Sections that pool several kinds of entry (`units`, `atomic_weights`, `settings`) show the kind after the
key, for example `units.m (base_unit) = ...`.

**Unit values include an internal `id`** (a hash computed by the build that runs the command). It is
**not stable across builds**: compare `show` or `diff` output produced by the same `phs`, never text saved
from one version against text from another. To compare configurations across versions, use `phs conf diff`
on the frozen files themselves: both sides are then read by the same build.

Exit: `show` first requires the configuration in force to be usable, so a broken file gives that error's
code (`1` integrity, `2` resolution, `3` engine); `2` a selector that matches nothing (the message lists the
sections); `1` an unfrozen `physure.conf` that cannot be read as text.

### 4.3 `phs conf freeze --reason TEXT [--engine-policy P] [--out PATH]`

Writes `physure.frozen.conf` (in the working directory, or at `--out`) from what is in force now
unfrozen: source 1 the embedded catalog, source 2 the working directory's `physure.conf` (or an empty
overrides block). It refuses to overwrite an existing frozen file: that is `update` or `seal`.

```console
$ phs conf freeze --reason "initial validation" --engine-policy release
wrote /srv/lab/physure.frozen.conf
seal: b1844aad0461a8b15bae3505155eb2b2c3333c3421bb3fd72625aab8c3326658
  record this seal outside this directory: `phs conf verify --expect SEAL` checks the file against it
engine: 0.3.0+unknown (policy release)
```

The default policy is `build`. **A build that does not name its commit cannot record `build` or
`binary`**: the commit comes from the variable `PHYSURE_BUILD_COMMIT` when `phs` is compiled (the release
workflows set it), and without it the commit is `unknown`. Such a `phs` refuses with exit 2 and says that
`--engine-policy release` or `record` is available:

```console
$ phs conf freeze --reason "initial validation"
phs conf: the engine cannot be recorded: this build does not name its commit (build it with PHYSURE_BUILD_COMMIT set to the full lowercase hash), so the policy build cannot be recorded; use release or record (with the CLI: --engine-policy release or record)
```

Exit: `0` written; `2` a frozen file already in force or at the target, a missing `--reason`, `--conf`
given, a policy that cannot be recorded; `1` a `physure.conf` that does not read strictly, an I/O error.

### 4.4 `phs conf verify [FILE] [--expect SEAL] [--allow-engine-mismatch]`

Recomputes the hashes, the history chain and the seal of FILE (default: the file in force), checks the
engine, replays the sources, and reports environment differences. With `--expect`, it also compares the
file's seal with the one you recorded elsewhere: the comparison holds only when the file verifies
**and** its seal equals SEAL (case does not matter).

```console
$ phs conf verify --expect b1844aad0461a8b15bae3505155eb2b2c3333c3421bb3fd72625aab8c3326658
file: /srv/lab/physure.frozen.conf
seal: b1844aad0461a8b15bae3505155eb2b2c3333c3421bb3fd72625aab8c3326658
...
verification: the seal, the hashes and the chain hold
engine: matches (policy release, pinned 0.3.0+unknown)
atomic-weight edition: standard-2024
expected seal: matches
```

After a hand edit of the overrides it names what changed:

```console
$ phs conf verify
...
verification: the file does not verify (modified since the last seal):
  source 2 (overrides) was modified: expected 619a3b4b..., found fd308bca...
  the content hash does not match the sources: expected a1b2c48e..., found ab330ed6...
  the seal does not match the format, creation time, content, engine, witnesses, scripts and history: expected b1844aad..., found 02d800c2...
engine: matches (policy release, pinned 0.3.0+unknown)
catalog: not replayed (the file does not verify)
```

Exit: as `status` (`0`, `1`, `3`); `1` an `--expect` mismatch, or a FILE that does not exist; `2` an
`--expect` that is not 64 hexadecimal characters, or no FILE and no file in force.
`--allow-engine-mismatch` adds a warning and never changes the exit code.

### 4.5 `phs conf diff [A] [B] [--allow-engine-mismatch]`

The semantic difference of two configurations: data entries by kind (`+` added, `-` removed, `~` changed
`old -> new`), then the `[Engine]` and `[Environment]` differences, with counts.

* No argument: A = the frozen file in force, B = what **this** build gives unfrozen in this directory.
* One argument: that file against what this build gives unfrozen here.
* Two arguments: both files. The word `unfrozen` stands for what this build gives unfrozen here.

```console
$ phs conf diff ../sealed-v1.conf physure.frozen.conf
a: ../sealed-v1.conf
b: physure.frozen.conf
data differences (2): constant +1 -0 ~0; constant_meta +1 -0 ~0
  constant:
    + lab_speed_of_sound = 343 m/s
  constant_meta:
    + lab_speed_of_sound = 343 m/s None None
engine differences (0):
environment differences (0, informational):
different
```

**What the default compares.** B is built from this build's embedded catalog **and the working
directory's `physure.conf`**, so the data of the default `diff` covers both what a library upgrade
changes and any drift of this directory's `physure.conf` from the frozen overrides. In particular, after
`freeze`, an edit of source 2 and `seal`, the directory's `physure.conf` is stale: the default `diff`
reports the overrides you sealed as differences until you copy them into `physure.conf` too (or compare two
files explicitly).

No engine is checked (the blocks are compared as written), so `--allow-engine-mismatch` changes nothing.
A file side that does not verify is reported and its catalog is not compared; that is never "identical".
`diff` never writes anything.

Exit: `0` identical; `1` different, a file side that does not verify or cannot be read, a `physure.conf`
that cannot be read; `2` no argument and no frozen file in force ("nothing to compare"), or a resolution
error.

### 4.6 `phs conf update --reason TEXT [--engine-policy P] [--yes]`

Adopts what the installed build brings (its embedded catalog as the new source 1, its engine in
`[Engine]`) into the file in force, **keeping the overrides**. It shows the difference the update makes
(the catalog of the file as it is against the catalog of the file it will write, and the `[Engine]` pairs
that change), asks for confirmation, writes atomically (a temporary file and a rename) and appends an
`update` entry to the history. If there is nothing new it says so and writes nothing:

```console
$ phs conf update --reason "adopt 0.3.1"
/srv/lab/physure.frozen.conf: nothing to update: the embedded catalog and the engine are this build's
```

`--engine-policy` changes the policy at the same time; the change is recorded in the history. This is
also the only way to run a file on another engine: there is no override for evaluating.

### 4.7 `phs conf seal --reason TEXT [--yes]`

Accepts a hand edit of the overrides block (source 2). An edit of any other part is refused (exit 2) with
what `verify` found. `seal` shows:

1. the full canonical text of the overrides as they will be sealed, and
2. **what they contribute to the catalog**: the entry difference between the catalog the sources build
   without the overrides and with them.

```console
$ phs conf seal --reason "add the lab's speed of sound" --yes
seal: /srv/lab/physure.frozen.conf
summary: overrides edited by hand: 619a3b4b5d3f -> eab825c66f60
the overrides (source 2) as they will be sealed, canonical:
  | [Constants]
  | molar_gas_constant = 8.0 J*mol^-1*K^-1
  | lab_speed_of_sound = 343 m/s
what the overrides contribute to the catalog (4): constant +1 -0 ~1; constant_meta +1 -0 ~1
  constant:
    + lab_speed_of_sound = 343 m/s
    ~ molar_gas_constant: 8.31446261815324 J*mol^-1*K^-1 -> 8.0 J*mol^-1*K^-1
  constant_meta:
    + lab_speed_of_sound = 343 m/s None None
    ~ molar_gas_constant: 8.31446261815324 J*mol^-1*K^-1 None None -> 8.0 J*mol^-1*K^-1 None None
wrote /srv/lab/physure.frozen.conf
seal: 13a40b21d253887e12bea347e4efc910433c240665b5b058907e9f5a039b8c82
  record this seal outside this directory: `phs conf verify --expect SEAL` checks the file against it
```

This is **not** a before/after of your edit: the file keeps only the hash of the overrides it last sealed,
not their text, so the earlier overrides cannot be shown from the file alone. To see exactly what an edit
changes, keep a copy of the file as it was last sealed and give it to `phs conf diff COPY physure.frozen.conf`
(as in the `diff` example above). `seal` is the only way to change the overrides: there is no `set` or
`unset`.

### 4.8 Confirmation and the compare-and-swap (`update`, `seal`)

* Without `--yes`, `update` and `seal` print what will change and ask `Proceed? [y/N]` on stderr; only `y`
  or `yes` proceeds. Declining writes nothing (exit 2).
* Without `--yes` and without a terminal on stdin (a pipe, a CI job), they refuse with exit 2 instead of
  reading piped input. Scripts pass `--yes`.
* With `--json` the preview is part of the JSON document, which is printed only at the end, after the
  question: use `--json` together with `--yes`.
* **A file that changes while the prompt waits is not overwritten.** The command remembers the length and
  modification time of the file as it read it; just before writing it checks them again and, if either
  moved, refuses with exit 1 (`changed since it was read ... nothing written: run the command again`).
  Nothing is written, and the edit made by someone else is kept.

Exit for both: `0` written, or nothing to do; `2` no file in force, a missing `--reason`, a refused plan
(for example an edit outside the overrides), a declined confirmation, no terminal and no `--yes`; `1` the
file changed while asking, a file that does not read, an I/O error (nothing written).

### 4.9 `phs conf history [FILE]`

The log of the file in force (or FILE), oldest first. Problems of the file come first; rows after the first
broken link of the chain are marked `[unverified]`. A row says when the `[Engine]` block changed from the
previous entry (`the [Engine] block changed`) and how the policy or the pinned identity changed (`pin:
policy: release -> record`). It says *that* the block changed, not which field: a moved `binary_sha256`
or a changed profile, target, features, compiler or flags shows only as `the [Engine] block changed`.
To see the fields, keep a copy of the file before each `update` and compare it with
`phs conf diff COPY physure.frozen.conf`, whose engine section lists every `[Engine]` pair that differs.

```console
$ phs conf history
file: /srv/lab/physure.frozen.conf
  1  2026-11-13T04:59:07Z  freeze  alice@lab-pc  engine 0.3.0+unknown  policy release  pinned 0.3.0+unknown
     reason: initial validation
     diff: embedded 847 lines; overrides 2 lines
  2  2026-11-13T04:59:23Z  seal    alice@lab-pc  engine 0.3.0+unknown  policy release  pinned 0.3.0+unknown
     reason: add the lab's speed of sound
     diff: overrides edited by hand: 619a3b4b5d3f -> eab825c66f60
```

Exit: the file's soundness, as `status` (`0`, `1`, `3`); `2` no FILE and no file in force.

### 4.10 The JSON documents

`--json` prints exactly one JSON document on stdout. Its schema is stable: every document has
`"schema": 1`, `"command"` (the command's name, or `"conf"` for an argument error) and `"exit_code"`;
fields are only ever added. Text output is for people and may change. An error is
`{"schema": 1, "command": ..., "error": "<message>", "exit_code": <code>}` (the message also goes to
stderr). The fields of each command:

| Command | Fields |
| --- | --- |
| `status` | `frozen`; unfrozen: `physure_conf` (the overlay's path or `null`), `warnings`; frozen: `why` (`--conf`, `PHYSURE_CONF`, `discovery`), `warnings` and the inspection fields below |
| `verify` | the inspection fields, and `seal_matches` (`true`/`false` with `--expect`, else `null`) |
| inspection fields | `path`, `seal`, `created`, `history_entries`, `verified`, `problems` (strings), `engine`, `engine_matches`, `catalog_problems`, `atomic_weight_edition`, `environment` (`[{key, old, new}]`, old = recorded, new = here), `sources` (`[{origin, sha256}]`); a file that cannot be read has only `path`, `unreadable` (the message), `verified: false`, `engine_matches: false` |
| `engine` | `null`, `{"state": "matches", "policy", "pinned"}`, `{"state": "differs", "policy", "differences": [strings]}` or `{"state": "unreadable", "message"}` |
| `show` | `frozen`, `path` (the frozen file or `null`), `selector`, `entries` (`[{section, key, kind, value}]`, plus `origin` with `--origin`), `warnings` |
| `freeze` | `path`, `seal`, `engine` (the short identity), `policy`, `sources` |
| `diff` | `a`, `b` (labels), `identical`, `data` (`[{change, kind, name, old, new}]`, `change` one of `added`, `removed`, `changed`; `old` or `new` is `null` on the side that lacks the entry), `engine` and `environment` (`[{key, old, new}]`), `summary`, `warnings`, `a_problems`, `b_problems` |
| `update` | `path`, `written`, `nothing_to_do`, and when there is a plan: `summary`, `data` (as in `diff`, or `null` with `data_problem`), `engine`, and `seal` once written |
| `seal` | `path`, `written`, `nothing_to_do`, and when there is a plan: `summary`, `overrides` (the canonical text), `data` (what the overrides contribute, or `null` with `data_problem`), and `seal` once written |
| `history` | `path`, `problems`, `rows` (`[{n, time, action, actor, reason, engine, policy, pinned, diff, engine_block_changed, pin_change, unverified}]`); a file that cannot be read has `unreadable` and empty `rows` |

For example:

```console
$ phs conf diff ../sealed-v1.conf physure.frozen.conf --json
{
  "a": "../sealed-v1.conf",
  "a_problems": [],
  "b": "physure.frozen.conf",
  "b_problems": [],
  "command": "diff",
  "data": [
    {
      "change": "added",
      "kind": "constant",
      "name": "lab_speed_of_sound",
      "new": "343 m/s",
      "old": null
    },
    ...
  ],
  "engine": [],
  "environment": [],
  "exit_code": 1,
  "identical": false,
  "schema": 1,
  "summary": "constant +1 -0 ~0; constant_meta +1 -0 ~0",
  "warnings": []
}
```

## 5. Engine pinning

A data file cannot hold an algorithm, so the engine is identified and pinned instead. `[Engine] policy`
is set by `freeze --engine-policy` and changed only by `update` (so the change is in the history):

| Policy | The running engine must have |
| --- | --- |
| `binary` | the same file that contains the engine (its SHA-256); for `phs`, its own executable |
| `build` (default) | the same version, commit, profile, target, features, compiler (`rustc`) and build flags |
| `release` | the same version |
| `record` | nothing (the identity is recorded, never enforced) |

A mismatch is an error (exit 3) **when evaluating**: `phs eval` and every other command refuse to run.
`status`, `verify` and `diff` still inspect the file. To run on another engine, `update`, which records
the change and its reason.

* `unknown` never equals anything, not even another `unknown`: under `build`, a commit that is `unknown`
  on either side is a mismatch.
* `build` and `binary` cannot be **recorded** by a build whose commit is `unknown` (section 4.3). The
  commit is the builder's claim: the variable `PHYSURE_BUILD_COMMIT` at build time, a full lowercase hash
  of 40 or 64 hexadecimal characters; anything else gives `unknown`. Git is not consulted.
* `binary` attests **the file at its path when it is checked**, not the image in memory. Under `build`, a
  recorded `binary_sha256` is informational.
* The engine identity's facts (profile, target, features, compiler, flags) are those of `physure-core`'s
  build. Two builds that enable different features of `physure-core` have different `features` strings: a
  file frozen under `build` by one tool (for example `phs`) is refused by a build with other features (for
  example a language server built without `html-viewer`). Use `release` for a file several tools share,
  or freeze with the tool that evaluates.
* `release` accepts development builds that share a release's version number (between releases the
  version is not bumped). Prefer `build` or `binary` with a release build when this matters.

## 6. Trust boundary and what is not pinned

State these limits in your audit documentation.

* **A hash detects that a file changed; it does not show who wrote it.** Whoever can edit the file can
  recompute its seal. For authenticity, **record the seal outside the directory** (in your quality system)
  when `freeze`, `update` or `seal` prints it, and check the file against it with
  `phs conf verify --expect SEAL`. Without the recorded seal, a consistent rewrite of the whole file is not
  detectable. Cryptographic signing is out of scope.
* **Whoever controls the build environment** (`PHYSURE_BUILD_COMMIT`, `PATH`, `RUSTFLAGS`, the toolchain)
  controls every field of the identity. `build` protects against accidents, not against a malicious
  builder; `binary` is the only policy whose evidence is the artefact itself.
* **`commit` is the builder's claim about the revision it built**, and the build facts are those of
  `physure-core`. The code that evaluates also includes `physure-script` (the PHS interpreter), `physure-cli`
  and the bindings: `build` pins the claim and the build facts, **not the whole evaluating engine**. Only
  `binary` pins the artefact.
* **Not pinned by `build`**, because a build script cannot see them: `overflow-checks`, `lto`,
  `codegen-units`; the versions of the dependencies when physure is built inside someone else's workspace
  (their `Cargo.lock` decides); dynamically linked libraries (libm, the C runtime, an interpreter, numpy,
  torch); extra arguments given to the compiler after `--` in `cargo rustc`; `RUSTC_WRAPPER`; dependency
  overrides (`paths` and `[patch]` through `--config` or `~/.cargo`, source replacement, vendoring); custom
  profiles, which report themselves as `debug` or `release`.
* **What is frozen is data**; algorithms are identified, pinned and (from Phase 3) witnessed, not preserved.
  To reproduce a result years later, keep the binary that `[Engine]` names.
* **Environment differences are warnings**: bit-for-bit reproducibility across machines is not promised.
* **Not frozen yet** (Phase 4 moves each into the catalog): the exchange rates of `physure.ext.currency`
  (literals in Python), the default number of Monte Carlo samples (a literal in the core), the unit systems
  `imperial.conf` and `international.conf` (read by Python only), and the Python chemistry tables
  (`STANDARD_ENTHALPY_FORMATION`, `STANDARD_ENTROPY`).

### Long-running processes

A single `phs` command reads the file in force afresh every time it runs. A process that keeps running
(the language server, a long computation in a host program) caches what it verified:

* **Staleness, never a bypass.** A frozen file edited after a process loaded it, with its length and
  modification time restored by hand, is not re-verified until either changes; what is in force is still
  the verified content that was read. A frozen file created after the process resolved its directory to
  unfrozen is not seen until the inputs of resolution change.
* **The file in force can change within one process.** The path is fixed after the first resolution, but a
  valid sealed file that replaces the content at that path is verified and activated when its stamp
  changes, so a long computation can see two catalogs in sequence (each a verified one, never a mixture).
  A process that must not change catalogs mid-run must be restarted after the file changes.

## 7. The workflow

1. **Freeze** the configuration you validated:
   `phs conf freeze --reason "validation VR-2026-14" --engine-policy build` (or `release`/`record` on a build
   without a commit). Record the printed seal in your quality system.
2. **Change the overrides** only by editing source 2 of `physure.frozen.conf`; `phs conf verify` now says
   what changed (exit 1), and every other `phs` command refuses to run until the edit is sealed.
3. **Seal** the edit: `phs conf seal --reason "..."`. Review the overrides and what they contribute,
   confirm, record the new seal. If you also keep a `physure.conf` in the directory, update it to match,
   or the default `diff` will report it as drift.
4. **Upgrade** physure (`phs upgrade`). The frozen file is not touched and still governs every calculation;
   `phs upgrade` ends with `A frozen configuration is in force; review what the new version changes with:
   phs conf diff`. (Under every policy but `record`, a new version of `phs` refuses to evaluate with exit 3 until step 6: the pin no longer matches.)
5. **Diff**: `phs conf diff` shows what the new version would change (data, engine, environment).
6. **Update**, if and when your process approves it: `phs conf update --reason "adopt 0.3.1, change CR-77"`.
   The overrides are kept, the history records the update, and the new seal is printed: record it.

`phs conf history` shows the whole trail at any time. There is no `unfreeze`: deleting the file does it,
and with `PHYSURE_REQUIRE_FROZEN=1` that fails closed.

## 8. Other bindings, and what comes next

* **Python and Java refuse while a frozen file is in force** until Phase 5 wires them to the same
  resolution: Python's `UnitSystem` raises `RuntimeError` and Java's catalog calls fail, each naming the
  file in force. They never silently use the unfrozen catalog.
* **WebAssembly ignores frozen files**: it has no file system, environment or working directory, so its
  configuration is always the unfrozen one.
* **Witnesses are Phase 3**: `freeze --witness FILE.phs` (reference calculations stored bit for bit) and
  `verify --run` (recompute them under the frozen configuration) are not accepted yet; today they are an
  unknown flag (exit 2).

The implementation: the commands are in
[`physure-cli/src/conf.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-cli/src/conf.rs#L28), and
everything they decide is in the core, for example
[`inspect`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/frozen/report.rs#L62) and
[`compare`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/frozen/report.rs#L296).
