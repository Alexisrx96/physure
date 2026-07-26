# Installing physure

physure is several independent packages sharing one Rust core. Pick the one(s) you need —
none of them require the others.

| I want to... | Install |
|---|---|
| Run `.phs` scripts from the terminal, no Python involved | [`phs` CLI](#phs-cli-the-fastest-way-in) |
| Use unit-aware quantities from Python (NumPy/PyTorch/JAX) | [Python package](#python) |
| Use the physics engine from a Rust project | [Rust crate](#rust) |
| Use it from a JVM project | [Java / Maven Central](#java) |
| Get syntax highlighting, live evaluation, hover docs in VS Code | [VS Code extension](#vs-code-extension) |

---

## `phs` CLI — the fastest way in

`phs` is a single native binary. It has no Python or Node dependency — the install scripts
below just download the right prebuilt binary for your platform and put it on your `PATH`.

**macOS / Linux:**

```bash
curl -fsSL https://physure.irvintorres.com/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://physure.irvintorres.com/install.ps1 | iex
```

**Windows (cmd.exe):**

```cmd
curl -fsSL https://physure.irvintorres.com/install.cmd -o install.cmd && install.cmd
```

Each script detects your OS/architecture, downloads the matching archive from the
[latest `core-v*` GitHub Release](https://github.com/Alexisrx96/physure/releases), and installs
both `phs` and `physure-lsp` (the language server) into `~/.local/bin`
(`%USERPROFILE%\.local\bin` on Windows). Add that directory to your `PATH` if it isn't already.

Verify it worked:

```bash
phs --help
```

### Manual download

Prebuilt archives for Linux (x86_64/aarch64), macOS (x86_64/aarch64), and Windows (x86_64) are
attached to every [`core-v*` release](https://github.com/Alexisrx96/physure/releases) as
`phs-<os>-<arch>.tar.gz` / `.zip`. Extract and put `phs` (and optionally `physure-lsp`) on your
`PATH`.

### Via `cargo`

If you already have a Rust toolchain:

```bash
cargo install --git https://github.com/Alexisrx96/physure physure-cli --bin phs --locked
```

Set `PHS_BRANCH=<branch>` before running the install script to have it build from a specific
branch instead of downloading a release binary — useful for testing unreleased changes.

---

## Python

```bash
pip install physure            # Rust-compiled wheel, zero other runtime dependencies
pip install "physure[numpy]"   # + NumPy/SciPy/Numba acceleration
pip install "physure[torch]"   # + PyTorch backend
pip install "physure[jax]"     # + JAX backend
pip install "physure[all]"     # everything
```

```python
from physure import Q_
print((Q_(10, "km") / Q_(2, "hr")).to("m/s"))
```

See the [Python package README](physure-python/README.md) for the full API.

---

## Rust

```toml
[dependencies]
physure = "0.2"
```

The crate is pure Rust with no FFI dependencies — it's the same engine that backs the Python
package and the `phs` CLI. See the [Rust crate README](physure-core/README.md) (note: the
crate's directory is `physure-core/`, but the published package is always named `physure`).

---

## Java

Published to Maven Central as `io.github.alexisrx96:physure-java`:

```xml
<dependency>
    <groupId>io.github.alexisrx96</groupId>
    <artifactId>physure-java</artifactId>
    <version>0.2.3</version>
</dependency>
```

The jar bundles prebuilt natives for Linux/macOS/Windows (x86_64 + aarch64) — no
`-Djava.library.path` setup needed. Requires Java 8+. See the
[Java bindings README](physure-java/README.md) for usage examples.

---

## VS Code extension

Search for **Physure (PHS) Support** in the VS Code / Cursor / VSCodium marketplace, or install
manually — see the [extension README](https://github.com/Alexisrx96/vsc_physure#readme) for
symlink-based install instructions.

> **Note:** the extension's *execution, live-preview, and export* features currently shell out
> to a Python environment with the `physure` package installed (see
> [Python](#python) above) — the extension will prompt you to install it if it's missing, or you
> can point it at an existing virtualenv via the `vsc-physure.pythonPath` setting. Syntax
> highlighting, diagnostics, and hover docs come from the native `physure-lsp` server and work
> without Python.

---

## Building from source

Clone the whole workspace to build every component yourself:

```bash
git clone https://github.com/Alexisrx96/physure
cd physure

# Rust workspace (physure crate, phs CLI, physure-lsp)
cargo build --release --workspace

# Python package
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust, if needed
cd physure-python
maturin develop --release
uv sync --group dev
```
