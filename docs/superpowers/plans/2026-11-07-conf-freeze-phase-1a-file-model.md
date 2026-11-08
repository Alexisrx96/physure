# `phs conf` Phase 1a: SHA-256, canonical form, frozen-file model, history chain, seal — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A pure, dependency-free layer in `physure-core` that can build, render, parse, verify, tamper-detect and re-seal a frozen configuration file, exactly as `docs/superpowers/specs/2026-11-07-conf-freeze-design.md` §4–§5.3 specify, without touching the unit registry.

**Architecture:** A new module `physure_core::frozen` with five small files: `sha256` (FIPS 180-4, checked against the NIST vectors), `canonical` (canonical text and the three hash definitions), `model` (the file as data, its strict renderer and parser), `seal` (construction, verification, re-sealing, the history chain) and `time` (an RFC 3339 UTC formatter, so that the model never reads a clock). Everything is deterministic and clock-free; callers pass the time, the engine identity and the actor in.

**Tech Stack:** Rust 2021, standard library only. No new dependency (the core's policy).

**Scope.** Plan 1a of Phase 1. Not here (written after 1a lands, they need its final types): 1b the engine identity (`build.rs`, `binary_sha256`, policies) and 1c resolution (`--conf`, `PHYSURE_CONF`, discovery, `PHYSURE_REQUIRE_FROZEN`), sealed loading into a `UnitRegistry`, the registry entry listing, and the `freeze`/`update` library operations that read the real catalog. Phases 2–5 are in the spec.

## Decisions fixed here (do not re-litigate)

- **Module path.** `physure-core/src/frozen/` (a directory, because it has several files). The spec §6 says `units/frozen.rs` and `sha256.rs`; Task 7 corrects that sentence of the spec.
- **No clock, no environment reads, no I/O** in this module. Time, engine, actor are arguments; file reading and writing belong to 1c.
- **Strict everywhere.** A malformed frozen file is an error that names the line. Header comments (`#`, `;`) and blank lines are tolerated in the header only; they are not covered by the seal.
- **One-line values.** Every `key = value` value, and every history field, is a single line with no leading or trailing whitespace; a value containing `\n` or `\r` is refused. A `reason` is 1–500 characters.
- **A source may not contain a line starting with `@@ `** (the framing marker); it is refused when the source is built. Source text is normalised: CRLF becomes LF and trailing newlines are removed, so rendering and parsing round-trip exactly.
- **Names** (keys, origins, witness names) use `[A-Za-z0-9_.:/-]` and are non-empty.
- **Hashes** are lowercase hexadecimal SHA-256, 64 characters.
- Error type `FrozenError(String)`; the verifier returns a list of `Problem`s (never stops at the first).

## File structure

| File | Responsibility |
|---|---|
| Create `physure-core/src/frozen/mod.rs` | module list and re-exports |
| Create `physure-core/src/frozen/sha256.rs` | `Sha256`, `sha256`, `sha256_hex`, `to_hex` |
| Create `physure-core/src/frozen/canonical.rs` | `canonical_text`, `source_sha256`, `content_sha256`, `canonical_pairs`, `is_sha256_hex` |
| Create `physure-core/src/frozen/model.rs` | `FrozenError`, `Source`, `WitnessScript`, `HistoryEntry`, `FrozenFile`, `render`, `parse` |
| Create `physure-core/src/frozen/seal.rs` | `EntryInput`, `Problem`, `FrozenFile::{new, reseal, verify}` |
| Create `physure-core/src/frozen/time.rs` | `rfc3339_from_unix`, `is_rfc3339_utc` |
| Modify `physure-core/src/lib.rs` | `pub mod frozen;` |
| Modify | `CHANGELOG.md`, `structure.md`, `ROADMAP.md`, `docs/ROADMAP.md`, the spec §6 (Task 7) |

Unit tests live in `#[cfg(test)] mod tests` at the bottom of each file. All commands run from the worktree root. Commit trailer on every commit: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. New files must be clippy-clean (`cargo clippy -p physure --lib --tests -- -D warnings` has pre-existing errors elsewhere; only diagnostics in `src/frozen/` count).

---

### Task 1: SHA-256

**Files:** Create `physure-core/src/frozen/mod.rs`, `physure-core/src/frozen/sha256.rs`; modify `physure-core/src/lib.rs`.

- [ ] **Step 1: Wire the module.** In `lib.rs` add `pub mod frozen;` below `pub mod chemistry;`. Create `frozen/mod.rs`:

```rust
//! Freezing the configuration: the data model of a sealed `physure.frozen.conf` and the hashes that seal it.
//!
//! See `docs/superpowers/specs/2026-11-07-conf-freeze-design.md`. This module is pure: it reads no clock, no
//! environment and no file, and it never touches the unit registry.

pub mod sha256;
```

- [ ] **Step 2: Write the tests first.** Create `sha256.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nist_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn one_million_a() {
        let mut h = Sha256::new();
        for _ in 0..10_000 {
            h.update(&[b'a'; 100]);
        }
        assert_eq!(to_hex(&h.finish()), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    }

    #[test]
    fn splitting_the_input_anywhere_gives_the_same_hash() {
        let data: Vec<u8> = (0..300u32).map(|i| (i * 7 % 251) as u8).collect();
        let whole = sha256_hex(&data);
        for cut in [0, 1, 55, 56, 63, 64, 65, 120, 128, 299, 300] {
            let mut h = Sha256::new();
            h.update(&data[..cut]);
            h.update(&data[cut..]);
            assert_eq!(to_hex(&h.finish()), whole, "cut at {cut}");
        }
    }

    #[test]
    fn the_constants_are_the_fractional_parts_of_roots_of_primes() {
        let mut primes = Vec::new();
        let mut n = 2u32;
        while primes.len() < 64 {
            if (2..n).take_while(|d| d * d <= n).all(|d| n % d != 0) {
                primes.push(n);
            }
            n += 1;
        }
        let frac32 = |x: f64| ((x - x.floor()) * 4_294_967_296.0).floor() as u32;
        for (p, k) in primes.iter().zip(K) {
            assert_eq!(frac32(f64::from(*p).cbrt()), k, "K for prime {p}");
        }
        for (p, h) in primes.iter().zip(H0) {
            assert_eq!(frac32(f64::from(*p).sqrt()), h, "H0 for prime {p}");
        }
    }
}
```

Run: `cargo test -p physure --lib frozen::sha256` → FAIL to compile.

- [ ] **Step 3: Implement** (above the tests):

```rust
//! SHA-256 (FIPS 180-4) without a dependency, checked against the NIST test vectors.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// An incremental SHA-256.
pub struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    filled: usize,
    length: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Sha256 { state: H0, block: [0; 64], filled: 0, length: 0 }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.length = self.length.wrapping_add(data.len() as u64);
        self.absorb(data);
    }

    fn absorb(&mut self, mut data: &[u8]) {
        if self.filled > 0 {
            let take = (64 - self.filled).min(data.len());
            self.block[self.filled..self.filled + take].copy_from_slice(&data[..take]);
            self.filled += take;
            data = &data[take..];
            if self.filled == 64 {
                let block = self.block;
                compress(&mut self.state, &block);
                self.filled = 0;
            }
        }
        while data.len() >= 64 {
            let (block, rest) = data.split_at(64);
            compress(&mut self.state, block.try_into().expect("64 bytes"));
            data = rest;
        }
        if !data.is_empty() {
            self.block[..data.len()].copy_from_slice(data);
            self.filled = data.len();
        }
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bit_length = self.length.wrapping_mul(8);
        let mut padding = [0u8; 72];
        padding[0] = 0x80;
        let zeros = if self.filled < 56 { 56 - self.filled } else { 120 - self.filled };
        self.absorb(&padding[..zeros]);
        self.absorb(&bit_length.to_be_bytes());
        debug_assert_eq!(self.filled, 0);
        let mut out = [0u8; 32];
        for (chunk, word) in out.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (word, chunk) in w.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_be_bytes(chunk.try_into().expect("4 bytes"));
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for (k, wi) in K.iter().zip(w) {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(*k).wrapping_add(wi);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

/// The SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finish()
}

/// Lowercase hexadecimal text of `bytes`.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The SHA-256 of `data` as 64 lowercase hexadecimal characters.
pub fn sha256_hex(data: &[u8]) -> String {
    to_hex(&sha256(data))
}
```

- [ ] **Step 4: Run.** `cargo test -p physure --lib frozen::sha256` → 4 pass. If the constants test fails for a prime, the `K`/`H0` table has a typo at that position: fix the table from the failing message, never the test, and re-run the NIST vectors.
- [ ] **Step 5: Clippy, commit.** `cargo clippy -p physure --lib --tests -- -D warnings`: nothing from `src/frozen/`.

```bash
git add physure-core/src/frozen physure-core/src/lib.rs
git commit -m "feat(core): SHA-256 without a dependency, checked against the NIST vectors

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Canonical form and the hash definitions

**Files:** Create `physure-core/src/frozen/canonical.rs`; modify `frozen/mod.rs` (`pub mod canonical;`).

- [ ] **Step 1: Tests first** (`canonical.rs` contains only this):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_blank_lines_and_whitespace_do_not_change_the_canonical_text() {
        let a = "# a comment\n[Units]\n  m = 1  \n\n; another\nkg = 2\n";
        let b = "[Units]\r\nm = 1\r\nkg = 2";
        assert_eq!(canonical_text(a), "[Units]\nm = 1\nkg = 2");
        assert_eq!(canonical_text(a), canonical_text(b));
        assert_eq!(source_sha256(a), source_sha256(b));
    }

    #[test]
    fn one_digit_changes_the_hash() {
        assert_ne!(source_sha256("m = 1.0"), source_sha256("m = 1.1"));
    }

    #[test]
    fn an_inline_comment_is_content_not_a_comment() {
        assert_ne!(source_sha256("m = 1"), source_sha256("m = 1 # note"));
    }

    #[test]
    fn the_content_hash_depends_on_origins_order_and_source_hashes() {
        let a = vec![("embedded".to_string(), "a".repeat(64)), ("overrides".to_string(), "b".repeat(64))];
        let swapped = vec![a[1].clone(), a[0].clone()];
        let renamed = vec![("embedded".to_string(), "a".repeat(64)), ("mine".to_string(), "b".repeat(64))];
        assert_ne!(content_sha256(&a), content_sha256(&swapped));
        assert_ne!(content_sha256(&a), content_sha256(&renamed));
        assert_eq!(content_sha256(&a), content_sha256(&a.clone()));
        assert!(is_sha256_hex(&content_sha256(&a)));
    }

    #[test]
    fn canonical_pairs_ignore_order_and_are_unambiguous() {
        let a = vec![("b".to_string(), "2".to_string()), ("a".to_string(), "1".to_string())];
        let b = vec![("a".to_string(), "1".to_string()), ("b".to_string(), "2".to_string())];
        assert_eq!(canonical_pairs(&a), "a=1\nb=2");
        assert_eq!(canonical_pairs(&a), canonical_pairs(&b));
        assert_eq!(canonical_pairs(&[]), "");
    }

    #[test]
    fn the_hex_check() {
        assert!(is_sha256_hex(&"0".repeat(64)));
        assert!(!is_sha256_hex(&"0".repeat(63)));
        assert!(!is_sha256_hex(&"G".repeat(64)));
        assert!(!is_sha256_hex(&"A".repeat(64)), "uppercase is not canonical");
    }
}
```

Run → FAIL to compile.

- [ ] **Step 2: Implement** (above the tests):

```rust
//! The canonical form of a source and the hashes built on it (spec §4.1).

use super::sha256::{sha256_hex, Sha256, to_hex};

/// The canonical text of a source: lines trimmed at both ends, blank lines and lines starting with `#` or `;`
/// dropped, joined with `\n`. Reformatting, comments and line endings do not change it; one digit does. An
/// inline comment (`m = 1 # note`) is content: the conf grammar has none.
pub fn canonical_text(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with(';'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `source_sha256`: the SHA-256 of the canonical text, as lowercase hexadecimal.
pub fn source_sha256(text: &str) -> String {
    sha256_hex(canonical_text(text).as_bytes())
}

/// `content_sha256`: the SHA-256 over, for each `(origin, source_sha256)` in order, `origin\n` + `source_sha256\n`.
pub fn content_sha256(sources: &[(String, String)]) -> String {
    let mut hasher = Sha256::new();
    for (origin, hash) in sources {
        hasher.update(origin.as_bytes());
        hasher.update(b"\n");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    to_hex(&hasher.finish())
}

/// The canonical text of a list of `key = value` pairs: `key=value` lines sorted, joined with `\n`.
pub fn canonical_pairs(pairs: &[(String, String)]) -> String {
    let mut lines: Vec<String> = pairs.iter().map(|(k, v)| format!("{k}={v}")).collect();
    lines.sort();
    lines.join("\n")
}

/// Whether `text` is 64 lowercase hexadecimal characters.
pub fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
```

- [ ] **Step 3:** add `pub mod canonical;` to `mod.rs`. `cargo test -p physure --lib frozen::canonical` → 6 pass. Clippy as before.
- [ ] **Step 4: Commit** `feat(core): the canonical form of a source and the hashes built on it`.

---

### Task 3: RFC 3339 time

**Files:** Create `physure-core/src/frozen/time.rs`; `mod.rs` gets `pub mod time;`.

- [ ] **Step 1: Tests first** (`time.rs` contains only this):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants() {
        assert_eq!(rfc3339_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_unix(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_from_unix(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(rfc3339_from_unix(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(rfc3339_from_unix(4_102_444_800), "2100-01-01T00:00:00Z");
    }

    #[test]
    fn the_validator() {
        assert!(is_rfc3339_utc("2026-11-07T14:03:11Z"));
        for bad in ["", "2026-11-07", "2026-11-07 14:03:11Z", "2026-11-07T14:03:11", "2026-13-07T14:03:11Z",
                    "2026-11-32T14:03:11Z", "2026-11-07T24:03:11Z", "2026-11-07T14:60:11Z", "2026-11-07T14:03:61Z",
                    "2026-1-07T14:03:11Z", "2026-11-07T14:03:11+00:00"] {
            assert!(!is_rfc3339_utc(bad), "{bad:?}");
        }
    }

    #[test]
    fn what_it_writes_it_accepts() {
        for secs in [0, 86_399, 86_400, 1_234_567_890, 4_102_444_800] {
            assert!(is_rfc3339_utc(&rfc3339_from_unix(secs)));
        }
    }
}
```

- [ ] **Step 2: Implement:**

```rust
//! UTC time as RFC 3339 text, for the creation date and the history of a frozen file. The model never reads a
//! clock: the caller passes the time in.

/// `secs` since the Unix epoch as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn rfc3339_from_unix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// The civil date of a day count since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Whether `text` is exactly `YYYY-MM-DDTHH:MM:SSZ` with a plausible date and time.
pub fn is_rfc3339_utc(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' {
        return false;
    }
    let num = |from: usize, to: usize| -> Option<u32> {
        let part = &text[from..to];
        part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse().ok()).flatten()
    };
    let (Some(_y), Some(mo), Some(d), Some(h), Some(mi), Some(s)) =
        (num(0, 4), num(5, 7), num(8, 10), num(11, 13), num(14, 16), num(17, 19))
    else {
        return false;
    };
    (1..=12).contains(&mo) && (1..=31).contains(&d) && h < 24 && mi < 60 && s < 61
}
```

   (The validator checks plausibility, not that the day exists in the month; the test list above is what it must satisfy. `s < 61` allows a leap second.)

- [ ] **Step 3:** `cargo test -p physure --lib frozen::time` → 3 pass; clippy; commit `feat(core): UTC time as RFC 3339 text for the frozen file`.

---

### Task 4: The file model, its renderer and its strict parser

**Files:** Create `physure-core/src/frozen/model.rs`; `mod.rs` gets `pub mod model;` and re-exports.

The format is the one of the spec §4. Exactly:

```
#physure-frozen 1
[Freeze]
format = 1
created = 2026-11-07T14:03:11Z
content_sha256 = <64 hex>
seal_sha256 = <64 hex>

[Engine]
key = value        (zero or more; order kept; keys unique)

[Environment]
[Witness]
[History.1]
time = …
action = …
engine = …
actor = …
reason = …
old_content_sha256 = <64 hex or empty>
new_content_sha256 = <64 hex>
diff = …
prev_entry_sha256 = <64 hex or empty>
entry_sha256 = <64 hex>

@@ source 1 origin=embedded sha256=<64 hex>
…verbatim lines…
@@ end source 1
@@ witness validation.1 sha256=<64 hex>
…verbatim lines…
@@ end witness
```

`[History.N]` sections are numbered 1, 2, 3… in order; the sections `[Freeze]`, `[History.1]` are required (at least one entry); `[Engine]`, `[Environment]`, `[Witness]` may be empty or absent (absent is read as empty; they are always rendered). Source markers are numbered from 1 in order. Text between markers is the source text, lines joined with `\n` without a final newline.

- [ ] **Step 1: Tests first** (`model.rs` contains only this; `sample()` builds a file by hand with placeholder 64-hex hashes, since this task does not verify seals):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn h(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn entry(n: u32) -> HistoryEntry {
        HistoryEntry {
            n,
            time: "2026-11-07T14:03:11Z".into(),
            action: "freeze".into(),
            engine: "0.3.1+9f2c1ab".into(),
            actor: "irvin@host".into(),
            reason: "initial freeze".into(),
            old_content_sha256: String::new(),
            new_content_sha256: h('c'),
            diff: "added 3, removed 0, changed 0".into(),
            prev_entry_sha256: String::new(),
            entry_sha256: h('e'),
        }
    }

    fn sample() -> FrozenFile {
        FrozenFile {
            format: 1,
            created: "2026-11-07T14:03:11Z".into(),
            content_sha256: h('c'),
            seal_sha256: h('d'),
            engine: pairs(&[("version", "0.3.1"), ("policy", "build")]),
            environment: pairs(&[("os", "windows")]),
            witnesses: pairs(&[("builtin.conversion.m_to_ft", "3ff0000000000000")]),
            history: vec![entry(1)],
            sources: vec![
                Source { origin: "embedded".into(), text: "[Units]\nm = 1".into(), sha256: h('a') },
                Source { origin: "overrides".into(), text: String::new(), sha256: h('b') },
            ],
            witness_scripts: vec![WitnessScript { name: "validation.1".into(), text: "1 m => ft".into(), sha256: h('f') }],
        }
    }

    #[test]
    fn it_round_trips() {
        let file = sample();
        let text = file.render().unwrap();
        assert!(text.starts_with("#physure-frozen 1\n[Freeze]\nformat = 1\n"));
        assert_eq!(FrozenFile::parse(&text).unwrap(), file);
        assert_eq!(FrozenFile::parse(&text).unwrap().render().unwrap(), text);
    }

    #[test]
    fn crlf_is_accepted_and_header_comments_are_ignored() {
        let text = sample().render().unwrap().replace('\n', "\r\n");
        assert_eq!(FrozenFile::parse(&text).unwrap(), sample());
        let commented = sample().render().unwrap().replacen("[Engine]\n", "# note\n\n[Engine]\n", 1);
        assert_eq!(FrozenFile::parse(&commented).unwrap(), sample());
    }

    fn parse_err(text: &str) -> String {
        FrozenFile::parse(text).unwrap_err().to_string()
    }

    #[test]
    fn what_is_not_a_frozen_file_is_refused() {
        assert!(parse_err("").contains("physure-frozen"));
        assert!(parse_err("[Units]\nm = 1\n").contains("physure-frozen"));
        assert!(parse_err("#physure-frozen 2\n").contains("physure-frozen"));
    }

    #[test]
    fn the_header_is_strict() {
        let good = sample().render().unwrap();
        assert!(parse_err(&good.replace("[Engine]", "[Engin]")).contains("unknown section"));
        assert!(parse_err(&good.replace("version = 0.3.1", "version = 0.3.1\nversion = 0.3.2")).contains("version"));
        assert!(parse_err(&good.replace("format = 1", "format")).contains("key = value"));
        assert!(parse_err(&good.replace("format = 1", "formats = 1")).contains("formats"));
        assert!(parse_err(&good.replace("[History.1]", "[History.2]")).contains("numbered"));
        assert!(parse_err(&good.replace(&h('c'), &"C".repeat(64))).contains("sha256"));
        assert!(parse_err(&good.replace("time = 2026-11-07T14:03:11Z", "time = yesterday")).contains("time"));
    }

    #[test]
    fn the_blocks_are_strict() {
        let good = sample().render().unwrap();
        assert!(parse_err(&good.replace("@@ end source 1", "@@ end source 7")).contains("end"));
        assert!(parse_err(&good.replace("@@ source 2 ", "@@ source 3 ")).contains("numbered"));
        assert!(parse_err(&good.replace("origin=embedded", "origin=em bedded")).contains("marker"));
        assert!(parse_err(&format!("{good}stray line\n")).contains("unexpected"));
        assert!(parse_err(&good.replace("@@ end witness\n", "")).contains("end"));
    }

    #[test]
    fn a_source_cannot_contain_the_framing_marker() {
        assert!(Source::new("overrides", "m = 1\n@@ end source 2\n").is_err());
        assert!(Source::new("over rides", "m = 1").is_err());
        assert!(Source::new("", "m = 1").is_err());
        assert!(WitnessScript::new("a b", "1 m").is_err());
    }

    #[test]
    fn source_text_is_normalised() {
        let a = Source::new("overrides", "m = 1\r\nkg = 2\r\n\r\n").unwrap();
        assert_eq!(a.text, "m = 1\nkg = 2");
        assert_eq!(a.sha256, super::super::canonical::source_sha256("m = 1\nkg = 2"));
    }

    #[test]
    fn values_must_be_one_line_and_trimmed() {
        let mut file = sample();
        file.engine.push(("note".into(), "two\nlines".into()));
        assert!(file.render().is_err());
        let mut file = sample();
        file.engine.push(("note".into(), " padded".into()));
        assert!(file.render().is_err());
        let mut file = sample();
        file.engine.push(("bad key".into(), "x".into()));
        assert!(file.render().is_err());
    }
}
```

   Run → FAIL to compile.

- [ ] **Step 2: Implement** (above the tests). Follow the format above exactly. The code:

```rust
//! The frozen file as data, and its strict renderer and parser (spec §4).

use std::fmt;

use super::canonical::{is_sha256_hex, source_sha256};
use super::time::is_rfc3339_utc;

pub const FORMAT_VERSION: u32 = 1;
pub const MAGIC: &str = "#physure-frozen 1";

/// Why a frozen file could not be built, rendered or parsed. The text names the offending line when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenError(pub String);

impl fmt::Display for FrozenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FrozenError {}

pub type FrozenResult<T> = Result<T, FrozenError>;

fn err<T>(message: impl Into<String>) -> FrozenResult<T> {
    Err(FrozenError(message.into()))
}

/// A name (key, origin, witness name): non-empty, `[A-Za-z0-9_.:/-]`.
fn is_name(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '/' | '-'))
}

fn check_value(what: &str, value: &str) -> FrozenResult<()> {
    if value.contains('\n') || value.contains('\r') {
        return err(format!("{what} must be one line"));
    }
    if value != value.trim() {
        return err(format!("{what} must not start or end with whitespace"));
    }
    Ok(())
}

/// Source text as it is stored: CRLF becomes LF, trailing newlines go, and no line may start with `@@ `.
fn normalise_text(what: &str, text: &str) -> FrozenResult<String> {
    let text = text.replace("\r\n", "\n");
    let text = text.trim_end_matches('\n').to_string();
    if let Some(line) = text.lines().find(|l| l.starts_with("@@ ")) {
        return err(format!("{what} contains a line that starts with the framing marker `@@ `: {line}"));
    }
    Ok(text)
}

/// One piece of configuration text, kept as it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub origin: String,
    pub text: String,
    pub sha256: String,
}

impl Source {
    pub fn new(origin: &str, text: &str) -> FrozenResult<Source> {
        if !is_name(origin) {
            return err(format!("invalid source origin {origin:?}: use letters, digits and _ . : / -"));
        }
        let text = normalise_text(&format!("source {origin:?}"), text)?;
        let sha256 = source_sha256(&text);
        Ok(Source { origin: origin.to_string(), text, sha256 })
    }
}

/// A user witness: a script kept inside the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessScript {
    pub name: String,
    pub text: String,
    pub sha256: String,
}

impl WitnessScript {
    pub fn new(name: &str, text: &str) -> FrozenResult<WitnessScript> {
        if !is_name(name) {
            return err(format!("invalid witness name {name:?}: use letters, digits and _ . : / -"));
        }
        let text = normalise_text(&format!("witness {name:?}"), text)?;
        let sha256 = source_sha256(&text);
        Ok(WitnessScript { name: name.to_string(), text, sha256 })
    }
}

/// One entry of the hash-chained history (spec §5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub n: u32,
    pub time: String,
    pub action: String,
    pub engine: String,
    pub actor: String,
    pub reason: String,
    pub old_content_sha256: String,
    pub new_content_sha256: String,
    pub diff: String,
    pub prev_entry_sha256: String,
    pub entry_sha256: String,
}

/// A frozen configuration, as data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenFile {
    pub format: u32,
    pub created: String,
    pub content_sha256: String,
    pub seal_sha256: String,
    pub engine: Vec<(String, String)>,
    pub environment: Vec<(String, String)>,
    pub witnesses: Vec<(String, String)>,
    pub history: Vec<HistoryEntry>,
    pub sources: Vec<Source>,
    pub witness_scripts: Vec<WitnessScript>,
}

const HISTORY_FIELDS: [&str; 10] = [
    "time", "action", "engine", "actor", "reason", "old_content_sha256", "new_content_sha256", "diff",
    "prev_entry_sha256", "entry_sha256",
];

fn push_kv(out: &mut String, key: &str, value: &str) -> FrozenResult<()> {
    if !is_name(key) {
        return err(format!("invalid key {key:?}"));
    }
    check_value(&format!("the value of {key}"), value)?;
    out.push_str(&format!("{key} = {value}\n"));
    Ok(())
}

fn push_pairs(out: &mut String, header: &str, pairs: &[(String, String)]) -> FrozenResult<()> {
    out.push_str(&format!("\n[{header}]\n"));
    for (key, value) in pairs {
        push_kv(out, key, value)?;
    }
    Ok(())
}

fn push_unique(pairs: &mut Vec<(String, String)>, section: &str, key: String, value: String) -> FrozenResult<()> {
    if pairs.iter().any(|(k, _)| *k == key) {
        return err(format!("[{section}] defines {key} twice"));
    }
    pairs.push((key, value));
    Ok(())
}

impl FrozenFile {
    /// The text of the file. Every value is checked (one line, trimmed, valid names).
    pub fn render(&self) -> FrozenResult<String> {
        let mut out = String::new();
        out.push_str(MAGIC);
        out.push('\n');
        out.push_str("[Freeze]\n");
        push_kv(&mut out, "format", &self.format.to_string())?;
        push_kv(&mut out, "created", &self.created)?;
        push_kv(&mut out, "content_sha256", &self.content_sha256)?;
        push_kv(&mut out, "seal_sha256", &self.seal_sha256)?;
        push_pairs(&mut out, "Engine", &self.engine)?;
        push_pairs(&mut out, "Environment", &self.environment)?;
        push_pairs(&mut out, "Witness", &self.witnesses)?;
        for entry in &self.history {
            out.push_str(&format!("\n[History.{}]\n", entry.n));
            let fields = [
                &entry.time, &entry.action, &entry.engine, &entry.actor, &entry.reason, &entry.old_content_sha256,
                &entry.new_content_sha256, &entry.diff, &entry.prev_entry_sha256, &entry.entry_sha256,
            ];
            for (key, value) in HISTORY_FIELDS.iter().zip(fields) {
                push_kv(&mut out, key, value)?;
            }
        }
        out.push('\n');
        for (index, source) in self.sources.iter().enumerate() {
            if !is_name(&source.origin) {
                return err(format!("invalid source origin {:?}", source.origin));
            }
            let n = index + 1;
            out.push_str(&format!("@@ source {n} origin={} sha256={}\n", source.origin, source.sha256));
            if !source.text.is_empty() {
                out.push_str(&source.text);
                out.push('\n');
            }
            out.push_str(&format!("@@ end source {n}\n"));
        }
        for script in &self.witness_scripts {
            if !is_name(&script.name) {
                return err(format!("invalid witness name {:?}", script.name));
            }
            out.push_str(&format!("@@ witness {} sha256={}\n", script.name, script.sha256));
            if !script.text.is_empty() {
                out.push_str(&script.text);
                out.push('\n');
            }
            out.push_str("@@ end witness\n");
        }
        Ok(out)
    }

    /// Reads a frozen file. Strict: any malformed line is an error that names it. The stored hashes are checked
    /// for shape here and for truth by [`FrozenFile::verify`].
    pub fn parse(text: &str) -> FrozenResult<FrozenFile> {
        let lines: Vec<&str> = text.lines().collect();
        if lines.first().map(|l| l.trim_end()) != Some(MAGIC) {
            return err("not a physure frozen configuration: the first line must be `#physure-frozen 1`");
        }
        let mut file = FrozenFile {
            format: 0,
            created: String::new(),
            content_sha256: String::new(),
            seal_sha256: String::new(),
            engine: Vec::new(),
            environment: Vec::new(),
            witnesses: Vec::new(),
            history: Vec::new(),
            sources: Vec::new(),
            witness_scripts: Vec::new(),
        };
        let mut entries: Vec<Vec<(String, String)>> = Vec::new();
        let mut section = String::new();
        let mut at = 1;
        while at < lines.len() && !lines[at].starts_with("@@ ") {
            let number = at + 1;
            let line = lines[at].trim();
            at += 1;
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
                if !matches!(name, "Freeze" | "Engine" | "Environment" | "Witness") {
                    let n = name.strip_prefix("History.").and_then(|d| d.parse::<usize>().ok());
                    match n {
                        None => return err(format!("line {number}: unknown section [{name}]")),
                        Some(n) if n != entries.len() + 1 => {
                            return err(format!("line {number}: history entries must be numbered 1, 2, 3, ...; found [{name}]"))
                        }
                        Some(_) => entries.push(Vec::new()),
                    }
                }
                section = name.to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return err(format!("line {number}: expected `key = value`: {line}"));
            };
            let (key, value) = (key.trim().to_string(), value.trim().to_string());
            if !is_name(&key) {
                return err(format!("line {number}: invalid key {key:?}"));
            }
            match section.as_str() {
                "" => return err(format!("line {number}: `{line}` is before any section")),
                "Freeze" => match key.as_str() {
                    "format" => file.format = value.parse().map_err(|_| FrozenError(format!("line {number}: format is not a number")))?,
                    "created" => file.created = value,
                    "content_sha256" => file.content_sha256 = value,
                    "seal_sha256" => file.seal_sha256 = value,
                    other => return err(format!("line {number}: unknown key {other} in [Freeze]")),
                },
                "Engine" => push_unique(&mut file.engine, "Engine", key, value)?,
                "Environment" => push_unique(&mut file.environment, "Environment", key, value)?,
                "Witness" => push_unique(&mut file.witnesses, "Witness", key, value)?,
                _ => {
                    let fields = entries.last_mut().expect("a History section opened it");
                    if !HISTORY_FIELDS.contains(&key.as_str()) {
                        return err(format!("line {number}: unknown key {key} in [{section}]"));
                    }
                    push_unique(fields, &section, key, value)?;
                }
            }
        }
        if file.format != FORMAT_VERSION {
            return err(format!("unsupported format {} (this build reads format {FORMAT_VERSION})", file.format));
        }
        if !is_rfc3339_utc(&file.created) {
            return err(format!("created is not a UTC time like 2026-11-07T14:03:11Z: {:?}", file.created));
        }
        for (key, value) in [("content_sha256", &file.content_sha256), ("seal_sha256", &file.seal_sha256)] {
            if !is_sha256_hex(value) {
                return err(format!("{key} is not a SHA-256 in lowercase hexadecimal: {value:?}"));
            }
        }
        for (index, fields) in entries.into_iter().enumerate() {
            file.history.push(read_entry(index as u32 + 1, fields)?);
        }
        if file.history.is_empty() {
            return err("the history has no entry: a frozen file always records how it was made");
        }
        read_blocks(&lines, at, &mut file)?;
        Ok(file)
    }
}

fn read_entry(n: u32, mut fields: Vec<(String, String)>) -> FrozenResult<HistoryEntry> {
    let mut take = |key: &str| -> FrozenResult<String> {
        match fields.iter().position(|(k, _)| k == key) {
            Some(i) => Ok(fields.remove(i).1),
            None => err(format!("[History.{n}] has no {key}")),
        }
    };
    let entry = HistoryEntry {
        n,
        time: take("time")?,
        action: take("action")?,
        engine: take("engine")?,
        actor: take("actor")?,
        reason: take("reason")?,
        old_content_sha256: take("old_content_sha256")?,
        new_content_sha256: take("new_content_sha256")?,
        diff: take("diff")?,
        prev_entry_sha256: take("prev_entry_sha256")?,
        entry_sha256: take("entry_sha256")?,
    };
    if !is_rfc3339_utc(&entry.time) {
        return err(format!("[History.{n}] time is not a UTC time like 2026-11-07T14:03:11Z: {:?}", entry.time));
    }
    for (key, value, may_be_empty) in [
        ("old_content_sha256", &entry.old_content_sha256, true),
        ("new_content_sha256", &entry.new_content_sha256, false),
        ("prev_entry_sha256", &entry.prev_entry_sha256, true),
        ("entry_sha256", &entry.entry_sha256, false),
    ] {
        if !(value.is_empty() && may_be_empty) && !is_sha256_hex(value) {
            return err(format!("[History.{n}] {key} is not a SHA-256 in lowercase hexadecimal: {value:?}"));
        }
    }
    Ok(entry)
}

/// The `@@` blocks that follow the header.
fn read_blocks(lines: &[&str], mut at: usize, file: &mut FrozenFile) -> FrozenResult<()> {
    while at < lines.len() {
        let number = at + 1;
        let line = lines[at].trim_end();
        at += 1;
        if line.trim().is_empty() {
            continue;
        }
        let words: Vec<&str> = line.split(' ').collect();
        let (end, kind) = match words.as_slice() {
            ["@@", "source", n, origin, sha] => {
                let index: usize = n.parse().map_err(|_| FrozenError(format!("line {number}: bad source marker")))?;
                if index != file.sources.len() + 1 {
                    return err(format!("line {number}: sources must be numbered 1, 2, 3, ...; found {index}"));
                }
                let origin = origin.strip_prefix("origin=").filter(|o| is_name(o));
                let sha = sha.strip_prefix("sha256=").filter(|s| is_sha256_hex(s));
                match (origin, sha) {
                    (Some(origin), Some(sha)) => (format!("@@ end source {index}"), Block::Source(origin.to_string(), sha.to_string())),
                    _ => return err(format!("line {number}: bad source marker (origin=NAME sha256=HEX): {line}")),
                }
            }
            ["@@", "witness", name, sha] => {
                let sha = sha.strip_prefix("sha256=").filter(|s| is_sha256_hex(s));
                match (is_name(name), sha) {
                    (true, Some(sha)) => ("@@ end witness".to_string(), Block::Witness(name.to_string(), sha.to_string())),
                    _ => return err(format!("line {number}: bad witness marker (NAME sha256=HEX): {line}")),
                }
            }
            _ if line.starts_with("@@ source ") || line.starts_with("@@ witness ") => {
                return err(format!("line {number}: bad block marker: {line}"))
            }
            _ => return err(format!("line {number}: unexpected line outside a block: {line}")),
        };
        let start = at;
        while at < lines.len() && lines[at].trim_end() != end {
            at += 1;
        }
        if at >= lines.len() {
            return err(format!("line {number}: the block has no end marker `{end}`"));
        }
        let text = lines[start..at].join("\n");
        at += 1;
        match kind {
            Block::Source(origin, sha256) => file.sources.push(Source { origin, text, sha256 }),
            Block::Witness(name, sha256) => file.witness_scripts.push(WitnessScript { name, text, sha256 }),
        }
    }
    Ok(())
}

enum Block {
    Source(String, String),
    Witness(String, String),
}
```

   Adapt the test expectations only where the plan's error wording and the code's differ trivially (the tests check for the substrings listed); if a test's substring is wrong for a message that is clearly correct, adjust the substring in the test and say so in the report. Do not weaken what a test proves.

- [ ] **Step 3:** `mod.rs`: `pub mod model;` and `pub use model::{FrozenError, FrozenFile, FrozenResult, HistoryEntry, Source, WitnessScript};`. `cargo test -p physure --lib frozen::model` → 8 pass. Clippy. Commit `feat(core): the frozen file as data with a strict renderer and parser`.

---

### Task 5: Construction, verification and re-sealing

**Files:** Create `physure-core/src/frozen/seal.rs`; `mod.rs` gets `pub mod seal;` and `pub use seal::{EntryInput, Problem};`.

Definitions (spec §4.1, §5.3):

- `entry_sha256` = SHA-256 of `n=…\ntime=…\naction=…\nengine=…\nactor=…\nreason=…\nold_content_sha256=…\nnew_content_sha256=…\ndiff=…\nprev_entry_sha256=…` (the entry without its own hash).
- `seal_sha256` = SHA-256 of `"physure-frozen-v1\n"` + `content_sha256` + `"\n"` + `canonical_pairs(engine)` + `"\n"` + `canonical_pairs(witnesses)` + `"\n"` + the `entry_sha256` of the last history entry.
- `content_sha256` is `canonical::content_sha256` over `(origin, source_sha256)` of the sources **recomputed from their texts**.

- [ ] **Step 1: Tests first** (`seal.rs` contains only this):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::frozen::model::{FrozenFile, Source, WitnessScript};

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn input<'a>(action: &'a str, reason: &'a str, time: &'a str) -> EntryInput<'a> {
        EntryInput { time, action, engine: "0.3.1+9f2c1ab", actor: "irvin@host", reason, diff: "added 2" }
    }

    fn fresh() -> FrozenFile {
        FrozenFile::new(
            "2026-11-07T14:03:11Z",
            pairs(&[("version", "0.3.1"), ("policy", "build")]),
            pairs(&[("os", "windows")]),
            pairs(&[("builtin.conversion", "3ff0000000000000")]),
            vec![Source::new("embedded", "[Units]\nm = 1\nkg = 2").unwrap(), Source::new("overrides", "# mine\n").unwrap()],
            vec![WitnessScript::new("validation.1", "1 m => ft").unwrap()],
            input("freeze", "initial freeze", "2026-11-07T14:03:11Z"),
        )
        .unwrap()
    }

    fn problems(file: &FrozenFile) -> Vec<String> {
        file.verify().iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_new_file_verifies_and_survives_a_trip_through_text() {
        let file = fresh();
        assert!(problems(&file).is_empty(), "{:?}", problems(&file));
        assert_eq!(file.history.len(), 1);
        assert_eq!(file.history[0].prev_entry_sha256, "");
        assert_eq!(file.history[0].old_content_sha256, "");
        assert_eq!(file.history[0].new_content_sha256, file.content_sha256);
        let again = FrozenFile::parse(&file.render().unwrap()).unwrap();
        assert!(problems(&again).is_empty());
        assert_eq!(again, file);
    }

    #[test]
    fn the_same_inputs_give_the_same_file() {
        assert_eq!(fresh().render().unwrap(), fresh().render().unwrap());
    }

    fn reparsed_with(edit: impl Fn(&str) -> String) -> FrozenFile {
        FrozenFile::parse(&edit(&fresh().render().unwrap())).unwrap()
    }

    #[test]
    fn editing_a_value_of_a_source_breaks_the_source_the_content_and_the_seal() {
        let file = reparsed_with(|t| t.replace("kg = 2", "kg = 3"));
        let p = problems(&file);
        assert!(p.iter().any(|m| m.contains("source 1") && m.contains("embedded")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("content")), "{p:?}");
        assert!(p.iter().any(|m| m.contains("seal")), "{p:?}");
    }

    #[test]
    fn editing_a_comment_changes_nothing() {
        let file = reparsed_with(|t| t.replace("# mine", "# somebody else's"));
        assert!(problems(&file).is_empty(), "{:?}", problems(&file));
    }

    #[test]
    fn editing_the_engine_or_a_witness_breaks_the_seal() {
        for edit in [("version = 0.3.1", "version = 0.3.2"), ("3ff0000000000000", "3ff0000000000001")] {
            let p = problems(&reparsed_with(|t| t.replace(edit.0, edit.1)));
            assert!(p.iter().any(|m| m.contains("seal")), "{edit:?}: {p:?}");
        }
    }

    #[test]
    fn editing_the_environment_is_allowed() {
        assert!(problems(&reparsed_with(|t| t.replace("os = windows", "os = linux"))).is_empty());
    }

    #[test]
    fn editing_a_witness_script_is_detected() {
        let p = problems(&reparsed_with(|t| t.replace("1 m => ft", "1 m => yd")));
        assert!(p.iter().any(|m| m.contains("witness") && m.contains("validation.1")), "{p:?}");
    }

    #[test]
    fn a_history_entry_cannot_be_edited() {
        let p = problems(&reparsed_with(|t| t.replace("initial freeze", "nothing to see")));
        assert!(p.iter().any(|m| m.contains("History.1")), "{p:?}");
    }

    fn two_entries() -> FrozenFile {
        let mut file = fresh();
        file.sources[1] = Source::new("overrides", "kg = 5").unwrap();
        file.reseal(input("seal", "raised the kilogram", "2026-11-08T09:00:00Z")).unwrap();
        file
    }

    #[test]
    fn resealing_after_an_edit_appends_a_chained_entry() {
        let before = fresh();
        let file = two_entries();
        assert!(problems(&file).is_empty(), "{:?}", problems(&file));
        assert_eq!(file.history.len(), 2);
        let (first, second) = (&file.history[0], &file.history[1]);
        assert_eq!(second.n, 2);
        assert_eq!(second.prev_entry_sha256, first.entry_sha256);
        assert_eq!(second.old_content_sha256, before.content_sha256);
        assert_eq!(second.new_content_sha256, file.content_sha256);
        assert_ne!(file.content_sha256, before.content_sha256);
        assert_ne!(file.seal_sha256, before.seal_sha256);
        assert_eq!(second.action, "seal");
        assert_eq!(first, &before.history[0], "earlier entries are untouched");
    }

    #[test]
    fn a_file_edited_by_hand_can_be_resealed_and_then_verifies() {
        let mut file = reparsed_with(|t| t.replace("kg = 2", "kg = 3"));
        assert!(!problems(&file).is_empty());
        file.reseal(input("seal", "accepted my edit", "2026-11-08T09:00:00Z")).unwrap();
        assert!(problems(&file).is_empty(), "{:?}", problems(&file));
        assert_eq!(file.history.len(), 2);
        assert_ne!(file.history[1].old_content_sha256, file.history[1].new_content_sha256);
    }

    #[test]
    fn deleting_reordering_or_forging_the_history_is_detected() {
        let text = two_entries().render().unwrap();
        let file = FrozenFile::parse(&text).unwrap();

        let mut dropped = file.clone();
        dropped.history.remove(0);
        for (i, e) in dropped.history.iter_mut().enumerate() {
            e.n = i as u32 + 1;
        }
        assert!(!problems(&dropped).is_empty(), "dropping the first entry");

        let mut swapped = file.clone();
        swapped.history.swap(0, 1);
        for (i, e) in swapped.history.iter_mut().enumerate() {
            e.n = i as u32 + 1;
        }
        assert!(!problems(&swapped).is_empty(), "swapping the entries");

        let mut truncated = file.clone();
        truncated.history.pop();
        assert!(!problems(&truncated).is_empty(), "removing the last entry leaves the content ahead of the log");

        let mut forged = file.clone();
        forged.history[1].reason = "something else".into();
        let p = problems(&forged);
        assert!(p.iter().any(|m| m.contains("History.2")), "{p:?}");
    }

    #[test]
    fn the_entry_input_is_validated() {
        let mut file = fresh();
        for bad in [
            input("frobnicate", "r", "2026-11-08T09:00:00Z"),
            input("update", "", "2026-11-08T09:00:00Z"),
            input("update", "two\nlines", "2026-11-08T09:00:00Z"),
            input("update", "r", "yesterday"),
        ] {
            assert!(file.reseal(bad).is_err());
        }
        assert!(file.reseal(input("update", &"x".repeat(501), "2026-11-08T09:00:00Z")).is_err());
        assert!(file.reseal(input("update", &"x".repeat(500), "2026-11-08T09:00:00Z")).is_ok());
        assert!(FrozenFile::new("2026-11-07T14:03:11Z", vec![], vec![], vec![], vec![], vec![],
                                input("update", "r", "2026-11-07T14:03:11Z")).is_err(), "the first entry is a freeze");
    }

    #[test]
    fn problems_say_what_was_expected_and_what_was_found() {
        let p = problems(&reparsed_with(|t| t.replace("kg = 2", "kg = 3")));
        let source = p.iter().find(|m| m.contains("source 1")).unwrap();
        assert!(source.contains("expected") && source.contains("found"), "{source}");
    }
}
```

   Run → FAIL to compile.

- [ ] **Step 2: Implement** (above the tests):

```rust
//! Building, verifying and re-sealing a frozen file (spec §4.1, §5.3).

use std::fmt;

use super::canonical::{canonical_pairs, content_sha256, source_sha256};
use super::model::{FrozenError, FrozenFile, FrozenResult, HistoryEntry, Source, WitnessScript, FORMAT_VERSION};
use super::sha256::sha256_hex;
use super::time::is_rfc3339_utc;

/// The facts of one history entry that the caller knows. The model never reads a clock or the environment.
pub struct EntryInput<'a> {
    pub time: &'a str,
    /// `freeze`, `update` or `seal`.
    pub action: &'a str,
    /// The running engine, short form (`0.3.1+9f2c1ab`).
    pub engine: &'a str,
    /// `user@host`: informational, not authenticated.
    pub actor: &'a str,
    /// Why: 1 to 500 characters on one line.
    pub reason: &'a str,
    /// A one-line summary of what changed.
    pub diff: &'a str,
}

fn one_line(what: &str, value: &str) -> FrozenResult<()> {
    if value.contains('\n') || value.contains('\r') || value != value.trim() {
        return Err(FrozenError(format!("{what} must be one line without leading or trailing whitespace")));
    }
    Ok(())
}

impl EntryInput<'_> {
    fn validate(&self) -> FrozenResult<()> {
        if !matches!(self.action, "freeze" | "update" | "seal") {
            return Err(FrozenError(format!("action must be freeze, update or seal, not {:?}", self.action)));
        }
        if !is_rfc3339_utc(self.time) {
            return Err(FrozenError(format!("time must be a UTC time like 2026-11-07T14:03:11Z, not {:?}", self.time)));
        }
        let length = self.reason.chars().count();
        if length == 0 || length > 500 {
            return Err(FrozenError("the reason must be 1 to 500 characters".into()));
        }
        for (what, value) in [("the reason", self.reason), ("the engine", self.engine), ("the actor", self.actor), ("the diff", self.diff)] {
            one_line(what, value)?;
        }
        Ok(())
    }
}

/// What `verify` found wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Format { found: u32 },
    EmptyHistory,
    SourceHash { index: usize, origin: String, expected: String, found: String },
    ContentHash { expected: String, found: String },
    WitnessScriptHash { name: String, expected: String, found: String },
    HistoryNumber { position: usize, found: u32 },
    HistoryPrev { n: u32, expected: String, found: String },
    HistoryEntryHash { n: u32, expected: String, found: String },
    HistoryTip { expected: String, found: String },
    Seal { expected: String, found: String },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::Format { found } => write!(f, "unsupported format {found}"),
            Problem::EmptyHistory => write!(f, "the history is empty"),
            Problem::SourceHash { index, origin, expected, found } => {
                write!(f, "source {index} ({origin}) was modified: expected {expected}, found {found}")
            }
            Problem::ContentHash { expected, found } => {
                write!(f, "the content hash does not match the sources: expected {expected}, found {found}")
            }
            Problem::WitnessScriptHash { name, expected, found } => {
                write!(f, "witness {name} was modified: expected {expected}, found {found}")
            }
            Problem::HistoryNumber { position, found } => {
                write!(f, "history entry at position {position} is numbered {found}")
            }
            Problem::HistoryPrev { n, expected, found } => {
                write!(f, "History.{n} does not follow the previous entry: expected {expected}, found {found}")
            }
            Problem::HistoryEntryHash { n, expected, found } => {
                write!(f, "History.{n} was modified: expected {expected}, found {found}")
            }
            Problem::HistoryTip { expected, found } => {
                write!(f, "the last history entry does not record the current content: expected {expected}, found {found}")
            }
            Problem::Seal { expected, found } => {
                write!(f, "the seal does not match the content, engine, witnesses and history: expected {expected}, found {found}")
            }
        }
    }
}

fn entry_sha256(entry: &HistoryEntry) -> String {
    let text = format!(
        "n={}\ntime={}\naction={}\nengine={}\nactor={}\nreason={}\nold_content_sha256={}\nnew_content_sha256={}\ndiff={}\nprev_entry_sha256={}",
        entry.n, entry.time, entry.action, entry.engine, entry.actor, entry.reason, entry.old_content_sha256,
        entry.new_content_sha256, entry.diff, entry.prev_entry_sha256
    );
    sha256_hex(text.as_bytes())
}

fn seal_sha256(file: &FrozenFile, content: &str, last_entry: &str) -> String {
    let text = format!(
        "physure-frozen-v1\n{content}\n{}\n{}\n{last_entry}",
        canonical_pairs(&file.engine),
        canonical_pairs(&file.witnesses)
    );
    sha256_hex(text.as_bytes())
}

impl FrozenFile {
    /// The content hash computed from the texts of the sources as they are now.
    pub fn computed_content_sha256(&self) -> String {
        let hashes: Vec<(String, String)> =
            self.sources.iter().map(|s| (s.origin.clone(), source_sha256(&s.text))).collect();
        content_sha256(&hashes)
    }

    /// A new frozen file with its first history entry (`freeze`). All hashes are computed.
    pub fn new(
        created: &str,
        engine: Vec<(String, String)>,
        environment: Vec<(String, String)>,
        witnesses: Vec<(String, String)>,
        sources: Vec<Source>,
        witness_scripts: Vec<WitnessScript>,
        first: EntryInput<'_>,
    ) -> FrozenResult<FrozenFile> {
        if first.action != "freeze" {
            return Err(FrozenError("the first history entry of a file is a freeze".into()));
        }
        first.validate()?;
        if !is_rfc3339_utc(created) {
            return Err(FrozenError(format!("created must be a UTC time like 2026-11-07T14:03:11Z, not {created:?}")));
        }
        let mut file = FrozenFile {
            format: FORMAT_VERSION,
            created: created.to_string(),
            content_sha256: String::new(),
            seal_sha256: String::new(),
            engine,
            environment,
            witnesses,
            history: Vec::new(),
            sources,
            witness_scripts,
        };
        file.append_entry(first);
        Ok(file)
    }

    /// Accepts the current state of the sources, the engine and the witnesses (after an edit by hand or an
    /// update): recomputes every hash, appends a chained history entry whose old content hash is the one stored
    /// before, and seals again.
    pub fn reseal(&mut self, entry: EntryInput<'_>) -> FrozenResult<()> {
        entry.validate()?;
        self.append_entry(entry);
        Ok(())
    }

    fn append_entry(&mut self, input: EntryInput<'_>) {
        for source in &mut self.sources {
            source.sha256 = source_sha256(&source.text);
        }
        for script in &mut self.witness_scripts {
            script.sha256 = source_sha256(&script.text);
        }
        let old = std::mem::take(&mut self.content_sha256);
        self.content_sha256 = self.computed_content_sha256();
        let prev = self.history.last().map(|e| e.entry_sha256.clone()).unwrap_or_default();
        let mut entry = HistoryEntry {
            n: self.history.len() as u32 + 1,
            time: input.time.to_string(),
            action: input.action.to_string(),
            engine: input.engine.to_string(),
            actor: input.actor.to_string(),
            reason: input.reason.to_string(),
            old_content_sha256: old,
            new_content_sha256: self.content_sha256.clone(),
            diff: input.diff.to_string(),
            prev_entry_sha256: prev,
            entry_sha256: String::new(),
        };
        entry.entry_sha256 = entry_sha256(&entry);
        let tip = entry.entry_sha256.clone();
        self.history.push(entry);
        self.seal_sha256 = seal_sha256(self, &self.content_sha256.clone(), &tip);
    }

    /// Everything that is wrong with the file, not only the first thing. Empty means the file is intact.
    pub fn verify(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        if self.format != FORMAT_VERSION {
            problems.push(Problem::Format { found: self.format });
        }
        for (i, source) in self.sources.iter().enumerate() {
            let computed = source_sha256(&source.text);
            if computed != source.sha256 {
                problems.push(Problem::SourceHash {
                    index: i + 1,
                    origin: source.origin.clone(),
                    expected: source.sha256.clone(),
                    found: computed,
                });
            }
        }
        let content = self.computed_content_sha256();
        if content != self.content_sha256 {
            problems.push(Problem::ContentHash { expected: self.content_sha256.clone(), found: content.clone() });
        }
        for script in &self.witness_scripts {
            let computed = source_sha256(&script.text);
            if computed != script.sha256 {
                problems.push(Problem::WitnessScriptHash {
                    name: script.name.clone(),
                    expected: script.sha256.clone(),
                    found: computed,
                });
            }
        }
        let mut previous = String::new();
        for (position, entry) in self.history.iter().enumerate() {
            if entry.n as usize != position + 1 {
                problems.push(Problem::HistoryNumber { position: position + 1, found: entry.n });
            }
            if entry.prev_entry_sha256 != previous {
                problems.push(Problem::HistoryPrev { n: entry.n, expected: previous.clone(), found: entry.prev_entry_sha256.clone() });
            }
            let computed = entry_sha256(entry);
            if computed != entry.entry_sha256 {
                problems.push(Problem::HistoryEntryHash { n: entry.n, expected: entry.entry_sha256.clone(), found: computed });
            }
            previous = entry.entry_sha256.clone();
        }
        let Some(last) = self.history.last() else {
            problems.push(Problem::EmptyHistory);
            return problems;
        };
        if last.new_content_sha256 != self.content_sha256 {
            problems.push(Problem::HistoryTip { expected: self.content_sha256.clone(), found: last.new_content_sha256.clone() });
        }
        let seal = seal_sha256(self, &content, &last.entry_sha256);
        if seal != self.seal_sha256 {
            problems.push(Problem::Seal { expected: self.seal_sha256.clone(), found: seal });
        }
        problems
    }
}
```

   Notes for the implementer: (a) in `append_entry` the stored content hash is taken as `old` **before** it is recomputed, so after a hand edit `old` is the hash the file claimed and `new` is what the texts hash to now; (b) `verify` computes the seal from the content hash **recomputed from the source texts**, so any edit of data, engine, witnesses or history breaks the seal as the spec says, and a forged `content_sha256` field alone is caught by the content check and by the history tip; (c) the test `deleting_reordering_or_forging_the_history_is_detected` removes the first entry and renumbers, so the chain (`prev`) and the entry hashes (they include `n`) must catch it; if any of its four sub-cases is not detected, the bug is in `verify`, not in the test.

- [ ] **Step 3:** `mod.rs`: `pub mod seal;` and `pub use seal::{EntryInput, Problem};`. `cargo test -p physure --lib frozen::seal` → 11 pass. Clippy. Commit `feat(core): sealing a frozen file, the chained history and a verifier that lists every problem`.

---

### Task 6: A property-style check of the whole layer

**Files:** Create `physure-core/tests/frozen_file.rs`.

An integration test that exercises only the public API, to prove the layer is usable from outside the crate and to freeze the shape of the file.

- [ ] **Step 1: Write the test.**

```rust
//! The frozen-file layer through its public API only.

use physure_core::frozen::model::{FrozenFile, Source, WitnessScript};
use physure_core::frozen::seal::EntryInput;
use physure_core::frozen::sha256::sha256_hex;
use physure_core::frozen::time::rfc3339_from_unix;

fn entry<'a>(action: &'a str, reason: &'a str, time: &'a str) -> EntryInput<'a> {
    EntryInput { time, action, engine: "0.0.0+test", actor: "ci@host", reason, diff: "n/a" }
}

fn file() -> FrozenFile {
    FrozenFile::new(
        &rfc3339_from_unix(1_700_000_000),
        vec![("version".into(), "0.0.0".into())],
        vec![],
        vec![],
        vec![
            Source::new("embedded", "[Constants]\nmolar_gas_constant = 8.31446261815324 J*mol^-1*K^-1").unwrap(),
            Source::new("overrides", "").unwrap(),
        ],
        vec![WitnessScript::new("w", "1 m => ft").unwrap()],
        entry("freeze", "initial", "2023-11-14T22:13:20Z"),
    )
    .unwrap()
}

#[test]
fn the_documented_hash_of_the_empty_string_is_what_sha256_says() {
    assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
}

#[test]
fn a_file_survives_text_and_every_single_byte_change_to_its_data_is_caught() {
    let text = file().render().unwrap();
    assert!(FrozenFile::parse(&text).unwrap().verify().is_empty());
    let marker = text.find("8.314").unwrap();
    // change each digit of the constant in turn: the seal must break every time
    for offset in 0..15 {
        let at = marker + offset;
        let c = text.as_bytes()[at];
        if !c.is_ascii_digit() {
            continue;
        }
        let mut bytes = text.clone().into_bytes();
        bytes[at] = if c == b'9' { b'0' } else { c + 1 };
        let edited = FrozenFile::parse(&String::from_utf8(bytes).unwrap()).unwrap();
        assert!(!edited.verify().is_empty(), "digit at offset {offset} went unnoticed");
    }
}

#[test]
fn the_seal_does_not_depend_on_the_environment_block_nor_on_comments() {
    let text = file().render().unwrap();
    let with_env = text.replace("\n[Environment]\n", "\n[Environment]\nos = linux\n");
    assert!(FrozenFile::parse(&with_env).unwrap().verify().is_empty());
    let with_comment = text.replace("[Constants]\n", "[Constants]\n# a comment\n");
    assert!(FrozenFile::parse(&with_comment).unwrap().verify().is_empty());
}
```

- [ ] **Step 2:** `cargo test -p physure --test frozen_file` → 3 pass. Commit `test(core): the frozen-file layer through its public API`.

---

### Task 7: Gates, changelog, structure, roadmap, spec

- [ ] **Step 1: Everything.** `cargo test -p physure --no-fail-fast` all green (report the totals); `cargo clippy -p physure --lib --tests -- -D warnings` shows nothing from `src/frozen/` or `tests/frozen_file.rs`; if `cargo llvm-cov --version` works, `cargo llvm-cov -p physure --summary-only` and report each `frozen/*.rs` file (target ≥ 90 %, the "Physure Core Strict" gate); if it is not installed, say so.
- [ ] **Step 2: CHANGELOG.** One entry at the top of `[Unreleased]` → `### Added`, in the style of its neighbours, keeping the file's line endings: **the data layer of a frozen configuration** (`physure_core::frozen`): SHA-256 without a dependency (checked against the NIST vectors), the canonical form of a source (comments, blank lines, whitespace and line endings do not change the hash; one digit does), the frozen-file model with a strict renderer and parser, a hash-chained history, a seal over data, engine, witnesses and history, and a verifier that lists every problem; it reads no clock, environment or file and does not touch the registry yet; the design is `docs/superpowers/specs/2026-11-07-conf-freeze-design.md`; the commands (`phs conf …`) and the loading of a frozen file come in the next phases.
- [ ] **Step 3: structure.md.** Invoke the `update-structure` skill; it should add `physure-core/src/frozen/` and its five files.
- [ ] **Step 4: Roadmaps** (both files together, see the maintenance guidelines): in `ROADMAP.md` and `docs/ROADMAP.md`, Subsystem 6, rewrite the Phase 1 line to say Phase 1a (SHA-256, canonical form, file model, history chain, seal) is done and 1b/1c (engine identity, resolution, sealed loading, entry listing) remain, tick nothing else; the dashboard row 9 percentage may rise from 5 % to 10 %. Run the strict docs build (`D:/Projects/physure/physure-python/.venv/Scripts/python.exe -m mkdocs build --strict -f physure-python/mkdocs.yml -d <a directory outside the repo>`), then delete any `.cache/` it leaves in the repo.
- [ ] **Step 5: Spec correction.** In `docs/superpowers/specs/2026-11-07-conf-freeze-design.md` §6 replace the path sentence ``Core (`physure-core/src/units/frozen.rs` and `sha256.rs`)`` by ``Core (`physure-core/src/frozen/`: `sha256`, `canonical`, `model`, `seal`, `time` in Phase 1a; resolution, sealed loading and the engine identity in 1b/1c)``.
- [ ] **Step 6: Commit** `docs: changelog, structure, roadmap and spec for the frozen-file data layer`.

---

## Self-review

- **Spec coverage (§4, §4.1, §5.3, §10):** the file format and framing (Task 4), canonical form and the three hashes (Task 2), the seal over data, engine, witnesses and last entry (Task 5), the history chain with tamper detection by deletion, reordering and editing (Task 5 tests), SHA-256 against the NIST vectors (Task 1), comments and whitespace not changing the hash while one digit does (Tasks 2 and 6), canonical-independent environment block (Task 6). Not here by design: engine identity, resolution, sealed loading, entry listing, the `freeze`/`update` operations over the real catalog (1b/1c).
- **Placeholders:** none; every step has its code or its exact definition.
- **Types:** `Source::new`/`WitnessScript::new` (Task 4) are what Task 5 and Task 6 call; `FrozenFile::{new, reseal, verify, computed_content_sha256}` are defined in Task 5 and used in Task 6; `EntryInput` fields are the same in Tasks 5 and 6; `rfc3339_from_unix` (Task 3) is used in Task 6; the history field order of `HISTORY_FIELDS` (Task 4) equals the order of the fields in `entry_sha256` (Task 5) except that the entry's own hash is last and excluded from its own hash.
