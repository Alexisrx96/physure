# Freezing the configuration (`phs conf`)

This guide is for sites where a calculation, once validated, must not change because the software was
upgraded: factories, laboratories, anything under an audit. It says what a frozen configuration is, what
the `phs conf` commands do (as built in Phases 2 and 3 of the
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
builtin.chem.arrhenius = 3fe7365a4517c8e5:0000000000000000:s^-1
builtin.chem.balance_propane_combustion = text=1 C3H8 + 5 O2 -> 3 CO2 + 4 H2O
...
builtin.units.foot_to_metre = 3fd381d7dbf487fd:0000000000000000:m
...
user.check = sha256:6109c6c0875ac66bf44c0313088b6a1693b0d5fa4efaa2eab87b131db707d575

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
diff = embedded 847 lines; overrides 2 lines; witnesses: 47 built-in, 1 user
prev_entry_sha256 =
entry_sha256 = 76202900...

@@ source 1 origin=embedded sha256=260ff6fc...
...the catalog embedded in the library, as it was read...
@@ end source 1
@@ source 2 origin=overrides sha256=619a3b4b...
[Constants]
molar_gas_constant = 8.0 J*mol^-1*K^-1
@@ end source 2
@@ witness check sha256=ceac0101...
x = 2 m
x / 4 s
@@ end witness
```

* **The sources are kept verbatim, separately and in order**, not merged: source 1 is the catalog the
  library embedded when the file was made; source 2 is **the overrides**, your `physure.conf` at the time of
  the freeze (or an empty block with a comment when there was none). Loading the file replays these
  sources, and nothing else: the catalog of the installed library and the working directory's
  `physure.conf` are not read. That is why an upgrade cannot change a value.
* **Source 2 is the one block meant to be edited by hand.** After editing it, run `phs conf seal`.
* **`[Engine]`** pins the engine that computes (section 5).
* **`[Environment]`** records the machine. It is informational, outside the seal; differences are warnings.
* **`[Witness]`** records the witnesses (section 9): the built-in battery as `builtin.<id>` and each user
  witness as `user.<name>`, whose script is kept after the sources in an `@@ witness NAME` block. Both are
  inside the seal.
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
library's compiled defaults: a setting the file does not mention takes its compiled default. One of them
changes answers rather than limits: `approx_rel_tol` (default `1e-3`) is the relative tolerance of PHS `≈`
and of `approx(a, b)`, so a witness script that uses `≈` depends on it.

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
| `1` | a discrepancy: a modified or unreadable file, a broken seal or chain, an `--expect` mismatch, differences (`diff`), sources that do not replay, a file changed while the command was asking; a witness that differs, is missing or could not be evaluated, a file that records no built-in witness (`verify --run`); a witness change `update` or `seal` was not told to accept |
| `2` | misuse (an unknown command or flag, a missing `--reason` or one with a control character, a malformed `--expect`, a FILE and `--conf` together), a missing required frozen file or a `--conf`/`PHYSURE_CONF` that names no file, a refused operation, a declined confirmation, no terminal and no `--yes`, `--json` without `--yes` (`update`, `seal`), nothing to compare, a witness script `freeze` cannot read, name or evaluate cleanly, an `--accept-witness-changes` KEY that did not change |
| `3` | the engine differs from the one the file pins, and that is the only discrepancy (and `seal` of a file pinned to another engine; `verify --run` then evaluates nothing) |

Misuse is decided before any file is read. A FILE and `--conf` together (`verify A --conf B`) are misuse:
which file the command acts on would be ambiguous.

Text output escapes every control character but the line feed (an escape character prints as `\x1b`), so
a file received from elsewhere cannot rewrite your terminal through its history, values or problems. The
JSON documents carry the strings as they are (JSON escapes them).

### 4.1 `phs conf` / `phs conf status`

Which configuration is in force and why; whether it verifies; its seal, creation date, engine state,
number of history entries, recorded witnesses (`builtin.*` and `user.*`) and atomic-weight edition; and the warnings: an ignored `physure.conf`, a
missing required frozen file, environment differences.

```console
$ phs conf
frozen configuration in force (discovery)
file: /srv/lab/physure.frozen.conf
seal: b1844aad0461a8b15bae3505155eb2b2c3333c3421bb3fd72625aab8c3326658
created: 2026-11-13T04:59:07Z
history: 1 entries
witnesses: 47 built-in, 1 user recorded
verification: the seal, the hashes and the chain hold
engine: matches (policy release, pinned 0.3.0+unknown)
atomic-weight edition: standard-2024
warning: /srv/lab/physure.conf is ignored: the frozen file is in force; to adopt an edit, change the overrides of the frozen file and run `phs conf seal`
```

With nothing frozen it says `no frozen configuration in force: the embedded catalog`, and whether a
`physure.conf` is read as an overlay of it, with what that reading warns about, as the runtime reads it: a
line it skips, a strict reading (what `freeze` records) that refuses the file or builds another catalog. A
`physure.conf` that is not UTF-8 text is ignored by the runtime, and `status` says it is not used. The
engine of a file that does not verify is not checked (`engine: not checked (the file does not verify)`;
in JSON `engine` is `null` and `engine_matches` `false`): a block the seal no longer vouches for could
otherwise read as matching. Exit: `0` sound or unfrozen; `1` the file does not read, does
not verify, its `[Engine]` block does not read, or its sources do not replay; `3` only the engine
differs; `2` a resolution error (`PHYSURE_REQUIRE_FROZEN=1` with no file, a `--conf` that names no file).
A file pinned to another engine is inspected all the same (inspection never runs the engine check); the exit code
says so (3).

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

### 4.3 `phs conf freeze --reason TEXT [--engine-policy P] [--out PATH] [--witness FILE.phs]...`

Writes `physure.frozen.conf` (in the working directory, or at `--out`) from what is in force now
unfrozen: source 1 the embedded catalog, source 2 the working directory's `physure.conf` (or an empty
overrides block). It refuses to overwrite an existing frozen file: that is `update` or `seal`. It records
the built-in witness battery and each `--witness` script (section 9), evaluated under the file it is about
to write, never under the unfrozen catalog (so `freeze` works under `PHYSURE_REQUIRE_FROZEN=1` with no file
yet). The refusals (a file already in force, an existing target, a policy, overrides that do not read) come
before anything is evaluated.

```console
$ phs conf freeze --reason "initial validation" --engine-policy release --witness check.phs
wrote /srv/lab/physure.frozen.conf
seal: b1844aad0461a8b15bae3505155eb2b2c3333c3421bb3fd72625aab8c3326658
  record this seal outside this directory: `phs conf verify --expect SEAL` checks the file against it
engine: 0.3.0+unknown (policy release)
witnesses: 47 built-in, 1 user recorded
```

Without `--engine-policy` the default is `build` when this `phs` names its commit and `binary` when it does
not (the strongest policy that needs no commit: it pins the hash of the executable). The commit comes from
the variable `PHYSURE_BUILD_COMMIT` when `phs` is compiled (the release workflows set it); without it the
commit is `unknown`. **Only `build` needs a known commit**, because it compares the commit and `unknown`
equals nothing: a file frozen that way would be refused by the very build that made it. Asking for `build` on
a build without a commit refuses with exit 2 and names the other policies:

```console
$ phs conf freeze --reason "initial validation" --engine-policy build
phs conf: the engine cannot be recorded: this build does not name its commit (build it with PHYSURE_BUILD_COMMIT set to the full lowercase hash), so the policy build cannot be recorded; use binary, release or record (with the CLI: --engine-policy binary, release or record)
```

A reason is one line of 1 to 500 characters without control characters (no escape sequences, tabs,
newlines or carriage returns): `freeze`, `update` and `seal` refuse anything else with exit 2 before reading
any file.

Exit: `0` written; `2` a frozen file already in force or at the target, a missing `--reason` or one with a
control character, `--conf` given, a policy that cannot be recorded, a `--witness` script that cannot be read
(missing, not UTF-8, over 1 MiB), whose name does not fit or is taken twice, or that does not evaluate cleanly
(the findings are listed), a built-in case that could not be evaluated (a panic or a limit, naming the
case); `1` a `physure.conf` that does not read strictly, an I/O error. Nothing is written on any refusal.

### 4.4 `phs conf verify [FILE] [--expect SEAL] [--run [--timeout-ms N]]`

Recomputes the hashes, the history chain and the seal of FILE (default: the file in force), checks the
engine, replays the sources, and reports environment differences. With `--expect`, it also compares the
file's seal with the one you recorded elsewhere: the comparison holds only when the file verifies
**and** its seal equals SEAL (case does not matter). With `--run`, it also evaluates the witnesses again
under FILE and compares them bit for bit with what FILE records (section 9.4).

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
engine: not checked (the file does not verify)
catalog: not replayed (the file does not verify)
```

Exit: as `status` (`0`, `1`, `3`); `1` an `--expect` mismatch, or a FILE that does not exist; `2` an
`--expect` that is not 64 hexadecimal characters, no FILE and no file in force, or a FILE and `--conf`,
`--timeout-ms` without `--run`. With `--run`: section 9.4.

### 4.5 `phs conf diff [A] [B]`

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

No engine is checked (the blocks are compared as written), so a file pinned to another engine can be compared under
any build.
A file side that does not verify is reported and its catalog is not compared; that is never "identical".
`diff` never writes anything.

**The environment is informational here too**: its differences are listed (`environment differences (n,
informational)`), but two configurations that differ only there are identical (exit 0); the JSON document
says so separately with `environment_identical`.

Exit: `0` identical (whatever the environment); `1` different in data or engine, a file side that does not verify or cannot be read, a `physure.conf`
that cannot be read; `2` no argument and no frozen file in force ("nothing to compare"), or a resolution
error.

### 4.6 `phs conf update --reason TEXT [--engine-policy P] [--accept-witness-changes all|KEY[,KEY...]] [--yes]`

Adopts what the installed build brings (its embedded catalog as the new source 1, its engine in
`[Engine]`) into the file in force, **keeping the overrides**. It shows the difference the update makes
(the catalog of the file as it is against the catalog of the file it will write, and the `[Engine]` pairs
that change) and the witnesses as recorded against evaluated under the file it will write (section 9.5),
asks for confirmation, writes atomically (a temporary file and a rename) and appends an `update` entry to
the history. If there is nothing new it says so, evaluates nothing and writes nothing:

```console
$ phs conf update --reason "adopt 0.3.1"
/srv/lab/physure.frozen.conf: nothing to update: the embedded catalog and the engine are this build's, and the built-in witnesses are recorded
```

A file that records no built-in witness (every file made before Phase 3) is never "nothing to update":
`phs conf update --reason "record the witness battery"` records the battery, without
`--accept-witness-changes` (every case is new, nothing validated changes), and its summary says
`witnesses: battery not recorded`. This is how such a file gets the witnesses that `verify --run` needs.

`--engine-policy` changes the policy at the same time; the change is recorded in the history. This is
also the only way to run a file on another engine: there is no override for evaluating.

### 4.7 `phs conf seal --reason TEXT [--accept-witness-changes all|KEY[,KEY...]] [--yes]`

Accepts a hand edit of the overrides block (source 2). An edit of any other part is refused (exit 2) with
what `verify` found. A file pinned to another engine is refused too (exit 3, `1` if its `[Engine]` block
does not read), since sealing would record the edit under an engine the file says is not its own: run
`phs conf update` first, the only way onto another engine. `update` takes the file as last sealed, so set
the edit aside, update, then edit again and seal. `seal` shows:

1. the full canonical text of the overrides as they will be sealed, and
2. **what they contribute to the catalog**: the entry difference between the catalog the sources build
   without the overrides and with them, and
3. the witnesses as recorded against evaluated under the file it will write (section 9.5): an edit that
   changes a validated result is refused (exit 1) until you accept it. The engine check comes first: a file
   pinned to another engine is refused (exit 3) before anything is evaluated.

```console
$ phs conf seal --reason "add the lab's speed of sound" --accept-witness-changes all --yes
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
witnesses: as recorded (47 built-in, 1 user) against evaluated under the file to be written:
...
wrote /srv/lab/physure.frozen.conf
seal: 13a40b21d253887e12bea347e4efc910433c240665b5b058907e9f5a039b8c82
  record this seal outside this directory: `phs conf verify --expect SEAL` checks the file against it
history: overrides edited by hand: 619a3b4b5d3f -> eab825c66f60; witnesses: 3 changed, 0 new, 0 missing; changed: builtin.chem.arrhenius, builtin.chem.clausius_clapeyron_water, builtin.const.molar_gas_constant; old [Witness] sha256 b658378c3ad1
```

The `molar_gas_constant` edit changes three witnesses, so this run passed `--accept-witness-changes all`;
section 9.5 shows the refusal without it.

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
* With `--json` the preview is part of the JSON document, which is printed only at the end, so they never
  ask: `--json` needs `--yes` to write. Without `--yes` they refuse with exit 2, the preview inside the
  document (`"written": false`).
* **A file that changes while the prompt waits is not overwritten.** The command remembers the length and
  modification time of the file as it read it, and the SHA-256 of the bytes it read; just before writing it
  takes all three again and, if any moved, refuses with exit 1 (`changed since it was read ... nothing
  written: run the command again`). Nothing is written, and the edit made by someone else is kept. Because
  the bytes are hashed, an edit of the same length that keeps the modification time (a coarse file system
  such as FAT or exFAT, a time restored by hand) is seen too. Its limit: a short window remains between that
  check and the rename, and an edit made in it is overwritten (there is no lock).
* **A `physure.frozen.conf` that is a symbolic link is replaced by a regular file** when `update` or `seal`
  writes it (they write a temporary file and rename it over the path); the link's target keeps the old
  content. Point `--conf` or `PHYSURE_CONF` at the real file instead of a link.

Exit for both: `0` written, or nothing to do; `2` no file in force, a missing `--reason`, a refused plan
(for example an edit outside the overrides), a declined confirmation, no terminal and no `--yes`, `--json`
without `--yes`, an `--accept-witness-changes` KEY that did not change; `3` (`seal`) the file pins another
engine; `1` the file changed while asking, a file that does not read, an I/O error, a witness that differs
or is missing and was not accepted, a witness that could not be evaluated (nothing written).

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
| `verify` | the inspection fields, and `seal_matches` (`true`/`false` with `--expect`, else `null`); with `--run`: `witnesses` (`{checked, match, differs, missing, new, errors}`, section 9.4, or `null` when nothing was evaluated), `witnesses_ok`, `witness_warnings`, and `witnesses_not_run` (why) when nothing was evaluated |
| inspection fields | `path`, `seal`, `created`, `history_entries`, `witnesses_recorded` (`{builtin, user}`), `verified`, `problems` (strings), `engine` (`null` when the file does not verify), `engine_matches`, `catalog_problems`, `atomic_weight_edition`, `environment` (`[{key, old, new}]`, old = recorded, new = here), `sources` (`[{origin, sha256}]`); a file that cannot be read has only `path`, `unreadable` (the message), `verified: false`, `engine_matches: false` |
| `engine` | `null`, `{"state": "matches", "policy", "pinned"}`, `{"state": "differs", "policy", "differences": [strings]}` or `{"state": "unreadable", "message"}` |
| `show` | `frozen`, `path` (the frozen file or `null`), `selector`, `entries` (`[{section, key, kind, value}]`, plus `origin` with `--origin`), `warnings` |
| `freeze` | `path`, `seal`, `engine` (the short identity), `policy`, `sources`, `witnesses` (`{builtin, user}`: how many were recorded), `warnings` |
| `diff` | `a`, `b` (labels), `identical` (data and engine; the environment is not part of it), `environment_identical`, `data` (`[{change, kind, name, old, new}]`, `change` one of `added`, `removed`, `changed`; `old` or `new` is `null` on the side that lacks the entry), `engine` and `environment` (`[{key, old, new}]`), `summary`, `warnings`, `a_problems`, `b_problems` |
| `update` | `path`, `written`, `nothing_to_do`, and when there is a plan: `summary`, `data` (as in `diff`, or `null` with `data_problem`), `engine`, `witnesses` (as for `verify --run`), `witnesses_rerecorded`, `witness_warnings`, `refused: "witnesses"` when the witness rule refused, and `seal` and `history_diff` once written |
| `seal` | `path`, `written`, `nothing_to_do`, and when there is a plan: `summary`, `overrides` (the canonical text), `data` (what the overrides contribute, or `null` with `data_problem`), `witnesses`, `witnesses_rerecorded`, `witness_warnings`, `refused: "witnesses"` when the witness rule refused, and `seal` and `history_diff` once written |
| `history` | `path`, `problems`, `rows` (`[{n, time, action, actor, reason, engine, policy, pinned, diff, engine_block_changed, pin_change, unverified}]`); a file that cannot be read has `unreadable` and empty `rows` |
| `conf` (`phs conf --help --json`) | `help` (the usage text) |

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
  "environment_identical": true,
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
`status`, `verify` and `diff` still inspect the file; `seal` refuses it (exit 3). To run on another engine,
`update`, which records the change and its reason.

* `unknown` never equals anything, not even another `unknown`: under `build`, a commit that is `unknown`
  on either side is a mismatch.
* `build` cannot be **recorded** by a build whose commit is `unknown` (section 4.3); `binary`, `release` and
  `record` can. The
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
* **What is frozen is data**; algorithms are identified, pinned and witnessed (section 9), not preserved.
  To reproduce a result years later, keep the binary that `[Engine]` names.
* **Environment differences are warnings**, never a discrepancy (not for `status`, `verify` or `diff`):
  bit-for-bit reproducibility across machines is not promised. The witnesses are how the *consequence* of a
  difference is seen: `verify --run` reports a result whose bits moved (section 9.6).
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
   `phs conf freeze --reason "validation VR-2026-14" --engine-policy build --witness validation.phs` (or
   `binary`/`release`/`record` on a build without a commit; `--witness` once per script of your own, or not at
   all). Record the printed seal in your quality system, and run `phs conf verify --run`.
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
   The preview ends with the witnesses evaluated under the updated file: a validated result that would
   change refuses until you review it, keep a copy of the file and pass `--accept-witness-changes`. The
   overrides are kept, the history records the update, and the new seal is printed: record it.
7. **Check** on every machine that evaluates: `phs conf verify --run`. It is also the check when there was
   nothing to update (a build of the same version under `release` or `record`), since `update` then
   evaluates nothing.

`phs conf history` shows the whole trail at any time. There is no `unfreeze`: deleting the file does it,
and with `PHYSURE_REQUIRE_FROZEN=1` that fails closed.

## 8. Other bindings, and what comes next

* **Python and Java refuse while a frozen file is in force** until Phase 5 wires them to the same
  resolution: Python's `UnitSystem` raises `RuntimeError` and Java's catalog calls fail, each naming the
  file in force. They never silently use the unfrozen catalog.
* **WebAssembly ignores frozen files**: it has no file system, environment or working directory, so its
  configuration is always the unfrozen one.
* **Witnesses run in `phs` only**: the bindings do not evaluate them (Phase 5).
* **Next, Phase 4**: the table of what is and is not frozen (`docs/conf_coverage.md`) and the move of the
  numbers that still live in code (section 6) into the catalog.

The implementation: the commands are in
[`physure-cli/src/conf.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-cli/src/conf.rs#L28), and
everything they decide is in the core, for example
[`inspect`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/frozen/report.rs#L64) and
[`compare`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/frozen/report.rs#L300).

## 9. Witnesses

A frozen file fixes the data and identifies the engine; **witnesses pin behaviour**. A witness is a
calculation whose result `freeze` stores bit for bit in the sealed `[Witness]` block and that
`phs conf verify --run` evaluates again under the file. They catch what the engine pin cannot see: the
same release on another CPU or C library giving other last digits, a dependency bump the version string
does not reflect, a change of code that the policy accepts (`release`, `record`), an override that moves a
validated result.

### 9.1 What is recorded, and how

Each witness is one `[Witness]` pair. The key is `builtin.<id>` for a case of the built-in battery and
`user.<name>` for a script of your own. The value has one of four forms, each prefix being the start of the
**whole** value:

| Value | What it is |
| --- | --- |
| `<mean>:<u>:<unit>` | A number: the IEEE-754 bit patterns of the mean and of the standard uncertainty, 16 lowercase hexadecimal digits each, then the unit text the interpreter prints (it may be empty, and it may hold `:`: the value is split at its first two colons). No uncertainty is `0000000000000000`. Example: `builtin.units.foot_to_metre = 3fd381d7dbf487fd:0000000000000000:m`. The mean is the magnitude as shown (`Quantity::display_parts`: the scale of an anonymous compound unit is folded in). |
| `text=<one line>` | A result that is not a number: a balancing, `text=1 C3H8 + 5 O2 -> 3 CO2 + 4 H2O` (every coefficient an integer, 1 included, the species in the order of the input, single spaces). |
| `failed=<one line>` | A case that legitimately returned an error under this configuration (an atomic-weight table of your own without Ca, a constant your overrides removed): the first line of the error. It is recorded and compared like any value, and it does not block `freeze`. |
| `sha256:<64 hex>` | A user witness: the SHA-256 of its canonical result (section 9.3). Only `user.*` keys hold it. |

`verify` checks the block's structure without evaluating anything: a key that is neither `builtin.<id>` nor
`user.<name>`, or a value that does not read, is a problem (`the [Witness] entry ... is not a witness`,
exit 1), and every `user.*` key must name a script of the file and every script a key.

Everywhere (`freeze`, `verify --run`, `update`, `seal`) the witnesses are evaluated in one order: the
battery in id order, then the user scripts in file order.

### 9.2 The built-in battery

This release has **47 cases**. **Ids are versioned**: a release may add cases, and it never changes what an
id computes; a case a newer library adds is *new* for an older file, a case an older file records and the
running build lacks is *missing*. Each case is checked in the test suite against a reference that does not
come from the engine
([`physure-core/tests/battery_reference.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-core/tests/battery_reference.rs#L1));
the cases themselves are in
[`physure-core/src/frozen/battery/`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/frozen/battery/mod.rs#L81).

| Area | Ids (each prefixed `builtin.`) | The reference the tests use |
| --- | --- | --- |
| units (12) | `units.foot_to_metre`, `units.kmh_to_ms`, `units.degc_to_k_25`, `units.degf_to_degc_212`, `units.db_20_to_ratio`, `units.psi_to_pa`, `units.kwh_to_j`, `units.ev_to_j`, `units.atm_to_kpa`, `units.cal_th_to_j`, `units.nmi_per_h_to_ms`, `units.ph_7_to_ratio` | NIST SP 811 (2008), Appendix B.8 and B.9; the SI Brochure, 9th edition (2019); pH: IUPAC Gold Book |
| constants (6) | `const.molar_gas_constant`, `const.boltzmann_constant`, `const.planck_constant`, `const.avogadro_constant`, `const.speed_of_light_in_vacuum`, `const.elementary_charge` | the SI Brochure, Table 1, and CODATA 2022 (exact since the 2019 SI) |
| uncertainty (11) | `unc.sum_correlated`, `unc.sum_uncorrelated`, `unc.product_first_order`, `unc.quotient_first_order`, `unc.power_first_order`, `unc.montecarlo_seed_1`, `unc.unscented_square`, `unc.gum_h1_uc`, `unc.gum_h1_nu_eff`, `unc.expanded_k_p95`, `unc.welch_satterthwaite_g41` | first order: closed forms (GUM 5.1.2, 5.2.2); JCGM 100:2008 H.1, the end gauge (u_c 32 nm, nu_eff 16.7, k 2.12 at 95 %) and G.4.1 (nu_eff 19.0); Monte Carlo against the exact moments of the product within five standard errors (JCGM 101:2008, 7.2); unscented against the moments of `x^2` |
| statistics (8) | `stat.t_p95_nu1`, `stat.t_p95_nu2`, `stat.t_p95_nu5`, `stat.t_p95_nu10`, `stat.t_p95_nu30`, `stat.t_p95_nu100`, `stat.t_p99_nu10`, `stat.t_p99_nu100` | the Student-t table of the NIST/SEMATECH e-Handbook of Statistical Methods, 1.3.6.7.2 (JCGM 100:2008, Table G.2), within 5e-5 relative |
| chemistry (13) | `chem.molar_mass_h2o`, `chem.molar_mass_glucose`, `chem.molar_mass_calcium_hydroxide`, `chem.molar_mass_iron_sulfate`, `chem.mass_to_moles_nacl`, `chem.balance_propane_combustion`, `chem.balance_iron_oxide_reduction`, `chem.arrhenius`, `chem.clausius_clapeyron_water`, `chem.gibbs_298`, `chem.dhf_h2o_l`, `chem.entropy_h2o_l`, `chem.gibbs_formation_h2o_l` | CIAAW 2024 standard atomic weights, summed by hand; the textbook coefficients; closed forms with the catalog's gas constant; the thermochemistry edition `legacy-python-1` and dG = dH - T dS of its entries |

**What the cases read.** Every case reads units, constants and atomic weights through the configuration it
is evaluated under, never through a catalog of its own, so an override shows in the witnesses that depend on
it:

* every conversion reads both of its units from the catalog; every `const.*` case is the catalog's
  `[Constants]` entry;
* the gas constant `molar_gas_constant` is read by `const.molar_gas_constant`, `chem.arrhenius` and
  `chem.clausius_clapeyron_water` (an edit of R changes these three, as the example of section 4.7 shows);
* the atomic weights (the selected edition and your `[AtomicWeights]`) are read by the four molar masses and
  `chem.mass_to_moles_nacl`; O by `h2o`, `glucose`, `calcium_hydroxide` and `iron_sulfate`; an element the
  table lacks is a recorded `failed=`;
* the standard thermochemical data (the edition `[Chemistry] thermochemistry` selects and your
  `[Thermochemistry]` definitions) are read by `chem.dhf_h2o_l`, `chem.entropy_h2o_l` and
  `chem.gibbs_formation_h2o_l` (H2O(l), H2(g), O2(g)); a species the data lack is a recorded `failed=`.
  `chem.gibbs_298` reads none: its dH and dS are literals of the case (and its dS, -163.2 J/(mol K), is not the
  -163.4 of the table: an id is forever, so the case that reads the table is a new one);
* **settings:** the only one a case depends on is the propagation mode, and every `unc.*` case and every
  `chem.*` case that combines uncertain quantities pins its own on its thread (`correlated`, or the mode it
  is named after). The file's `[Settings]` (`propagation_mode`, `parallel_threshold`, the loop ceilings)
  therefore do not reach the battery. The `units.*`, `const.*` and `stat.*` cases and the balancings read no
  setting.

**Determinism rules.** No clock, no environment, no threads whose scheduling could reorder a sum, a fixed
seed wherever a generator is used (`unc.montecarlo_seed_1`: seed 1, 1000 samples, on the calling thread).
Each case runs on its own under `catch_unwind`: a panic or a limit is that case's *error* (it refuses
`freeze`, and `verify --run` exits 1), never an abort, and never confused with a difference.

### 9.3 User witnesses

`phs conf freeze --witness check.phs [--witness other.phs ...]` adds scripts of your own:

* **The name is the file stem** (`check`): letters, digits and `_ . : / -` only, the rule of names in a
  frozen file; another stem, or two scripts with the same stem, is refused (exit 2) before anything is
  read. The file must be UTF-8 text of at most 1 MiB.
* **The script is kept in the file**, sealed, in an `@@ witness check` block after the sources, normalised:
  CRLF becomes LF and trailing newlines are trimmed. The normalised text is what is evaluated, at `freeze`
  and ever after.
* **It is evaluated like `phs eval`**, in the sandbox of section 9.4, with Monte Carlo seed 0, under the
  file about to be written. It must evaluate without an error finding; otherwise `freeze` refuses (exit 2)
  and lists the findings. It must also **produce at least one result**: an empty, comment-only or
  definitions-only script has the same canonical result under every configuration and would match forever,
  so `freeze` refuses it too (exit 2, `a witness must produce at least one result`). Nothing is written.
* **What is recorded is the hash of a canonical result**, the versioned text `phs-witness-1`
  ([`canonical_witness`](https://github.com/Alexisrx96/physure/blob/main/physure-script/src/report.rs#L115)):
  `ok true|false`, then one line per result item with its line, name, kind, the bit patterns of its value
  and uncertainty, its unit text and dimension, then the *code* of each error finding (never a message, which
  may be reworded, and never a warning). `user.check = sha256:<the SHA-256 of that text>`.
* **A difference shows only *that* the result changed.** The file keeps a hash, not the old result, so
  `verify --run` prints the running canonical text and you compare it with a copy you kept elsewhere (save
  the canonical text, or the frozen file, when you validate).

Scripts are chosen at `freeze`; there is no command to add one to an existing file. `update` and `seal`
keep them.

### 9.4 `phs conf verify --run`

`phs conf verify [FILE] --run [--timeout-ms N]` does everything `verify` does, then evaluates every witness
again **under FILE** (FILE is made the configuration of this process for the run; a relative FILE is made
absolute, so `verify --run ../line-2/physure.frozen.conf` evaluates under that file, not under what is in
force here) and compares the results bit for bit with what FILE records:

```console
$ phs conf verify --run
file: /srv/lab/physure.frozen.conf
...
verification: the seal, the hashes and the chain hold
engine: matches (policy release, pinned 0.3.0+unknown)
atomic-weight edition: standard-2024
witnesses: checked 47 built-in, 1 user, under /srv/lab/physure.frozen.conf
48 matched
```

The rows come grouped: `DIFFERS` (both bit patterns, both decimals and the distance in ulps, section 9.6;
for a user witness both hashes and the running canonical text), `MISSING` (recorded, but this build has no
such case), `NEW` (this build has a case the file does not record: informational, it comes with a newer
library and is recorded by the next `update` or `seal`), `ERROR` (a panic, a limit, not run, or a user
script that does not evaluate cleanly or produces no result, section 9.3: an honest file never records one,
so it is an error even when its hash matches), then the count of matches.

**A new built-in case is an error under a pinned identical engine.** When the file's policy is `binary` or
`build` and the running engine is the pinned one, the battery of that exact build is known and the file
recorded every case of it at `freeze`: a `NEW` built-in row means a key was removed from the file, and it is
`ERROR builtin.<id>: the recorded battery lacks builtin.<id>: expected every case of this exact build ...`
(exit 1). Under `release` and `record` another build of the version may be running, so `NEW` stays
informational there. A file that records no built-in witness at all is the migration case below.

**Nothing is evaluated** when the file does not verify (exit 1), when its seal does not match `--expect`
(exit 1: a file that is not the one expected is not one whose scripts are executed; `witnesses: not run: the
seal does not match --expect; nothing was evaluated`), when it pins another engine (exit 3: a result under an
engine the file does not pin proves nothing; `update` first) or when it records no `builtin.*` witness:

```console
witnesses: not run: nothing is witnessed: expected the built-in battery; re-freeze, or run `phs conf update --reason "..."`; nothing was evaluated
```

A file without the battery is refused rather than passed because there would be nothing to compare: every
file made before Phase 3 has an empty `[Witness]`. `phs conf update --reason "..."` records the battery
(section 4.6), with no other flag.

**The file evaluated is the file verified.** `--run` puts the file in force by its path after reading it to
verify it, so it hashes the bytes it verified and checks that the file still has them just before evaluating
(`witnesses: not run: the file changed while it was being verified; nothing was evaluated`, exit 1) and
again just after (`witnesses: discarded: the file changed while its witnesses were being evaluated ...`,
exit 1): an edit in between, even one of the same length with the modification time put back, is never
evaluated as if it were the file that verified.

| Exit | `verify --run` |
| --- | --- |
| `0` | every recorded witness matches (new cases are informational, except a built-in case under `binary` or `build` with the engine matching) |
| `1` | a witness differs, is missing or could not be evaluated; a user script that does not evaluate cleanly or produces no result; a built-in case the file lacks under a pinned identical engine; the file records no built-in witness; the file does not verify; an `--expect` mismatch (nothing is evaluated); the file changed while it was verified |
| `2` | misuse (`--timeout-ms` without `--run`, or not a whole number of milliseconds above 0) |
| `3` | the engine differs; nothing is evaluated |

JSON (`--json`) adds `witnesses`: `checked` (how many the file records), `match` (a count), `differs`
(`[{key, recorded, running, ulps, recorded_decimal, running_decimal}]` for a number, with
`running_canonical` instead of the decimals for a user witness), `missing` and `new` (keys), `errors`
(`[{key, message}]`); `witnesses_ok`; `witness_warnings` (a probe file that could not be removed, ...). When
nothing was evaluated, `witnesses` is `null`, `witnesses_ok` is `false` and `witnesses_not_run` says why.

**`--run` executes the scripts embedded in the file**, which may come from someone else. What bounds them:

* the sandbox of the PHS interpreter: no file is read, no module is loaded from disk, no native plugin, one
  thread, a bounded call depth, native stack and output;
* the loop-iteration and pipeline-step ceilings are **fixed** at 1 000 000 and 1 000 (the compiled
  defaults), opened on the evaluating thread: the file's `[Settings]`, which set the process-wide values when
  it is activated, cannot raise them;
* a wall-clock deadline per user script, 60 s by default (`--timeout-ms N`), and a watchdog 2 s after it for
  an evaluation that does not stop by itself; after one that does not, the remaining scripts are reported
  `not run: an earlier script did not stop` and the process ends after reporting (a thread cannot be
  killed);
* a panic is `ERROR user.<name>: panic: ...` and a limit `ERROR user.<name>: limit: ...`: errors (exit 1),
  never differences;
* **the parser bounds how deep a script nests**, so no script can end the process in a stack overflow
  while it is read, checked, evaluated or dropped: an expression tree is at most 128 levels deep (every
  operator of a chain such as `a + b + c` is a level: a sum of 200 000 terms is refused, not a crash), and
  brackets, `if` expressions chained without brackets and function bodies nested by indentation at most 64
  levels together. A script past either bound is a syntax error (`expression too deeply nested: at most
  128 levels`, `... nested more than 64 levels deep`), which `freeze` refuses and `verify --run` reports.

**There is no memory cap**: a script can allocate until the machine refuses (a vector of millions of
elements, a string doubled in a loop), and nothing isolates the evaluation from the process that runs it
(the scripts run on a thread of the `phs` process, not in a process of their own). Run `--run` on a file
from a source you do not trust inside a container or a session with a memory limit. The battery is the
library's own code and is not bounded by `--timeout-ms`.

### 9.5 Witnesses in `update` and `seal`

Both end their preview with the witnesses **as recorded** against **evaluated under the file they would
write** (a sealed throwaway copy of it, `phs-witness-probe-*` in the temporary directory, deleted on every
path; the user scripts with the 60 s default deadline). A probe file more than a day old, left by a process
that was killed before it could delete its own, is deleted the next time `phs conf` makes a probe (nothing
else in the temporary directory is touched). Then the rule:

* **Every recorded witness matches and nothing is new:** the witnesses are kept byte for byte; the history
  line has no witness part.
* **Only new cases:** they are recorded without any flag (nothing validated changes) and counted in the
  history line. A file with no built-in witness is this case: every case is new.
* **A recorded witness differs or is missing:** refused, **exit 1**, nothing written, JSON
  `"refused": "witnesses"`, unless `--accept-witness-changes` covers every such key: `all`, or a
  comma-separated list of keys (`builtin.<id>`, `user.<name>`). A listed key that did not change is misuse
  (exit 2). With the acceptance, every witness is re-recorded from the evaluation.
* **A witness could not be evaluated** (a panic, a limit), or **a user script does not evaluate cleanly or
  produces no result** under the file to be written (an override it used was renamed): refused (exit 1,
  JSON `"refused": "witnesses"`) whatever the flag, naming the script and its first finding. Re-recording it
  would make a failure the reference result; fix the script or the overrides first.
* **The flag with nothing to accept** (nothing to do, every recorded witness matches, only new cases) is
  not silently ignored: a warning on stderr and in JSON `warnings` says `the flag names N key(s) but nothing
  changed; it had no effect` (exit 0).

```console
$ phs conf seal --reason "lab value of R" --yes
...
witnesses: as recorded (47 built-in, 1 user) against evaluated under the file to be written:
DIFFERS builtin.chem.arrhenius
  recorded: mean 3fe7365a4517c8e5 (0.7253848409148104), uncertainty 0000000000000000 (0.0), unit "s^-1"
  running:  mean 3fcc449f4e7e7426 (0.22084418614113693), uncertainty 0000000000000000 (0.0), unit "s^-1"
  distance: 7584134697931967 ulps
DIFFERS builtin.chem.clausius_clapeyron_water
  ...
DIFFERS builtin.const.molar_gas_constant
  recorded: mean 4020a1013e8990be (8.31446261815324), uncertainty 0000000000000000 (0.0), unit "J*mol^-1*K^-1"
  running:  mean 4020000000000000 (8.0), uncertainty 0000000000000000 (0.0), unit "J*mol^-1*K^-1"
  distance: 177026716242110 ulps
45 matched
phs conf: 3 recorded witness(es) differ or are missing under the file to be written and were not accepted: builtin.chem.arrhenius, builtin.chem.clausius_clapeyron_water, builtin.const.molar_gas_constant; nothing written: review the differences above, keep a copy of this file (the only record of the old values) and pass --accept-witness-changes builtin.chem.arrhenius,builtin.chem.clausius_clapeyron_water,builtin.const.molar_gas_constant (or all) to re-record them
```

With the acceptance, the history `diff` line (and `history:`, and JSON `history_diff`) ends with the witness
part:

```text
witnesses: N changed, M new, K missing; changed: KEY, ...; missing: KEY, ...; old [Witness] sha256 <12 hex>
```

The changed and the missing keys are named (the first 10 of each, then `+N more`), and the 12 hexadecimal
digits are the start of the SHA-256 of the old `[Witness]` block, hashed canonically. They identify the old
block; they do not hold its values. **Keep a copy of the file before accepting**: it is the only record of
the old values (and `phs conf diff COPY physure.frozen.conf` compares the data).

**When there is nothing to do, nothing is evaluated.** An `update` whose catalog and engine are this
build's, in a file that records the battery, says `nothing to update` and never reaches this rule. Under
`release` or `record`, a new build of the same version runs the file without any `update`: there,
`verify --run` is the check. `seal` checks the engine before anything is evaluated (exit 3 for a file pinned
to another engine).

### 9.6 Bit for bit: ulps, platforms, dependencies

**The comparison has no tolerance.** A difference prints both bit patterns, both decimal values and the
distance in **ulps** (units in the last place): how many representable doubles lie between the two values.
`1 ulp` is a last-bit difference (0.3048 and 0.30480000000000007), the mark of another C library or CPU; a
distance of millions, as in the example above, is a changed formula or value. `not measurable in ulps` means
the unit, a sign or a non-finite value differs.

* **Platforms.** The C math library differs between targets (msvc on Windows, glibc on Linux: `exp`, `ln`,
  `pow`), and can differ in the last bit. The same release can therefore make `verify --run` exit 1 on
  another operating system. A configuration validated on two operating systems needs **one frozen file per
  operating system**, each frozen (and its witnesses recorded) on its own.
* **Dependencies.** `unc.montecarlo_seed_1` draws with `rand`'s `StdRng` and `rand_distr`'s normal
  distribution, neither of which promises the same draws across versions: a dependency bump can move its
  bits. Review it as any other difference and accept it with `--accept-witness-changes`.
* **Environment differences stay warnings** (section 6); the witnesses are how their consequence, if any,
  becomes visible.

### 9.7 What witnesses do not prove

* **They sample behaviour.** 47 calculations and your scripts pass or fail; a function that none of them
  touches can change unseen. Passing is not a proof that the rest of the library is correct or unchanged.
* **They compare the engine with itself over time**, not with the truth: the reference values are checked
  in the test suite, not by `verify --run`.
* **A user witness shows only that its result changed**, not what changed (section 9.3).
* **They do not authenticate.** Whoever can edit the file can recompute the witnesses and the seal; the seal
  recorded outside the directory (`verify --expect`) is what detects that.

### 9.8 Other changes of Phase 3

* **`phs eval` reports a breach of the loop or pipeline ceilings as code `limit`** (`max_loop_iterations
  ceiling`, `max_pipeline_steps ceiling`), as it already did for the call depth, the stack and the time; it
  was `runtime`, and the run ends at that statement as for every limit. A script that matched on the old
  code must match `limit`.
* **`--allow-engine-mismatch` is removed.** It was accepted by `status`, `verify` and `diff` and changed
  nothing (inspection never needed it); they now refuse it as an unknown flag (exit 2).
* **`update` and `seal` also compare the bytes before writing** (section 4.8).

The implementation: the witness values and the comparison are
[`physure-core/src/frozen/witness.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/frozen/witness.rs#L239),
the draft, probe and single reseal of `freeze`, `update` and `seal`
[`physure-core/src/frozen/ops/draft.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-core/src/frozen/ops/draft.rs#L96),
and the sandboxed evaluation of the commands
[`physure-cli/src/conf_witness.rs`](https://github.com/Alexisrx96/physure/blob/main/physure-cli/src/conf_witness.rs#L206).
