<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/Alexisrx96/physure/main/assets/logo-horizontal-dark.svg">
  <img src="https://raw.githubusercontent.com/Alexisrx96/physure/main/assets/logo-horizontal-light.svg" alt="physure" width="380">
</picture>

# PHS — PhysureScript

[![Latest release](https://img.shields.io/github/v/release/Alexisrx96/physure?filter=core-v*&color=F59E0B&labelColor=18181A&label=phs)](https://github.com/Alexisrx96/physure/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/Alexisrx96/physure/tests.yml?branch=main&labelColor=18181A)](https://github.com/Alexisrx96/physure/actions/workflows/tests.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-F59E0B?labelColor=18181A)](https://github.com/Alexisrx96/physure/blob/main/LICENSE)

**A DSL for the person who does the physics, not the person who codes it.**

PHS is a small language for writing engineering and lab calculations exactly the way they
already exist on paper — quantities with real units, formulas, conversions, and tolerances —
and getting a verified, reproducible answer without opening a Python file or an IDE. No
runtime, no Python, no dependency install: `phs` is a single native binary.

## Why PHS exists

Engineering and lab teams already have the physics right. What breaks is the handoff: a
formula written on a whiteboard or in a spreadsheet gets re-typed into application code by a
developer who doesn't know the domain, units get dropped or silently mismatched, and nobody
notices until a number is off by a factor of 1000. PHS closes that gap in both directions:

- **The engineer validates the formula, not the developer.** Someone who knows the physics
  writes and runs the `.phs` script themselves, with real units enforced by the interpreter —
  `5 m + 2 kg` is a hard error, not a silent bug three sprints later.
- **The script *is* the spec.** Once a calculation runs clean, it's handed to the dev team as
  a `.phs` file, a rendered HTML report, or transpiled starting code (`phs transpile calc.phs
  --target python`) — not a description of what the code should do.
- **PHS documents the process, not just the result.** Every run keeps the formula, the
  intermediate steps, the units carried through each operation, and the final conversions
  together in one artifact — the calculation is its own audit trail.

## Quick look

```phs
# @title: Coulomb repulsion between two alpha particles
k  = 8.98755e9 N * m^2 / C^2   # Coulomb's constant
e  = 1.602e-19 C               # elementary charge
r  = 0.1 m

q1 = 2 * e
q2 = 2 * e

F_e = k * q1 * q2 / r^2
F_e => nN                      # convert on the spot
F_e: .3e                       # ...or format in scientific notation

# Measurements carry uncertainty through the whole calculation
velocidad = 25.0 +/- 0.5 m / s
masa      = 2.0 +/- 0.1 kg
energia_cinetica = 0.5 * masa * velocidad^2

# Assert a measured value against an expected one within N-sigma
g_exp = 9.81 +/- 0.05 m / s^2
g_exp == 9.80 m / s^2 +/- 2 sigma

"El radio es {r} y la fuerza es {F_e => nN}"
```

```bash
phs coulomb.phs
```

That's the whole workflow: write the physics as physics, run it, get units-checked numbers
back. No project scaffolding, no import statements, no interpreter setup.

## Install

```bash
curl -fsSL https://physure.irvintorres.com/install.sh | sh          # macOS / Linux
irm https://physure.irvintorres.com/install.ps1 | iex               # Windows (PowerShell)
```

Both scripts detect your OS/architecture and drop a prebuilt `phs` (plus `physure-lsp` for
editor support) into `~/.local/bin`. See [`INSTALL.md`](../INSTALL.md) for every install path —
Homebrew-style manual download, `cargo install`, building from source, and the VS Code
extension.

## From formula to documentation

`phs` doesn't just evaluate a script — it can turn it into a shareable record of how a number
was produced, with no extra markup required. The `# @title:`, `# @author:`, and `# @abstract:`
header comments already present in a script become the paper's front matter automatically.

```bash
phs calc.phs --html          # standalone HTML report, opened in your browser
phs calc.phs --web           # live local visualizer (formulas, plots, step trace)
phs calc.phs --tui           # terminal dashboard for a running script
```

The HTML report renders each formula as typeset math (KaTeX), lists every intermediate step in
order, and keeps any `plot(...)` calls as inline SVG figures — a lab notebook entry a reviewer
can read without running anything. This is what makes a `.phs` file good enough to hand to a
developer *as* the specification: it shows its work.

## Language tour

- **Units are load-bearing, not decorative.** `5 m + 2 kg` and `5 pound + 2 kg` are compile-time
  errors; every arithmetic operation checks and carries dimensions.
- **Conversions and formatting are part of the expression.** `F => nN` converts on the spot;
  `F: .3e` sets the digits, `F: base` quotes the measurement in the units it is built from
  (`2 kΩ: base` is `2000 A^-2 * kg * m^2 * s^-3`), and `F: frac` / `F: ifrac` write it as a
  common or mixed fraction (`1.5 m` is `3/2 m` and `1 1/2 m`) — none of them touch the value.
- **Uncertainty propagates automatically.** `25.0 +/- 0.5 m/s` carries its error bar through
  every subsequent operation; `a == b +/- N sigma` asserts agreement within tolerance.
- **Vectors and array math.** `linspace`, `gradient`, `trapz`, and elementwise arithmetic on
  unit-bearing arrays (`[10, 20, 30] m/s + [1, 1, 1] m/s^2 * 2 s`).
- **User functions with typed, unit-checked parameters:**

  ```phs
  E_campo(r: m) =
      k_e = 8.98755e9 N * m^2 / C^2
      q = 1.602e-19 C
      k_e * q / r^2

  E_campo(5 cm)
  ```

- **Function algebra.** Functions add, subtract, multiply, divide, and compose like the math
  they represent: `h = f + g`, `c = f(g)` builds `c(x) = f(g(x))` — useful for superposing
  fields, combining transfer functions, or composing calibration curves.
- **Symbolic calculus, when you need it.** `solve("P * V = n * R * T", "T")`, `deriv(...)`,
  `integral(...)` for algebraic derivations alongside the numeric ones.
- **String interpolation** for readable output: `"F = {F_e => nN}"`.
- **Extensible.** `use plugin_double, plugin_sum from plugin` loads a native Rust plugin (ABI
  v2) or a Python `.py` extension module by the same syntax — bring your own domain functions
  without forking the interpreter. Scaffold one with `phs new-plugin myplugin --lang rust`.

## Transpile: verified physics, real starting code

Once a `.phs` script is validated, hand the dev team something they can build on instead of a
description:

```bash
phs transpile calc.phs --target python -o calc.py
phs transpile calc.phs --target rust
phs transpile calc.phs --target java  -o Calc.java
```

The generated code mirrors the script's variables and formulas directly — the developer starts
from logic the domain expert already confirmed is correct, instead of re-deriving it from a PDF
or a Slack message.

## CLI reference

```
USAGE:
    phs <script.phs> [OPTIONS]
    phs --repl
    phs transpile <script.phs> [--target <rust|python|java>] [--output <file>]
    phs register-protocol
    phs new-plugin <name> [--lang <rust|python|both>] [--dir <path>]

FLAGS & OPTIONS:
    -h, --help               Print help information
    -r, --repl               Start interactive PHS REPL environment
    -t, --target <lang>      Transpile target: rust, python, java (default: rust)
    -o, --output <file>      Specify output file path (e.g. out.py, Main.java)
    --tui                    Launch terminal UI dashboard mode
    --web                    Launch local web visualizer server
    --html, --view           Generate and open HTML report
```

`phs register-protocol` registers a `phs://` OS-level protocol handler, so `.phs` files (or
links to them) can be opened directly by the CLI from a browser or file manager.

## Editor support

The [`vsc-physure`](https://marketplace.visualstudio.com/items?itemName=irvintorres.vsc-physure)
VS Code extension (also works in Cursor/VSCodium forks) ships syntax highlighting, live
CodeLens evaluation, hover docs, unit autocomplete, and diagnostics, backed by the same
`physure-lsp` binary distributed alongside `phs`.

## Architecture

```
physure-script/   the PHS engine: lexer, pest grammar, AST, interpreter, transpiler, symbolic module
physure-cli/      this crate — the `phs` binary: REPL, TUI, web visualizer, HTML reports, plugin scaffolding
physure-lsp/      the language server consumed by vsc-physure and any other LSP client
```

Neither `physure-cli` nor `physure-script` links against Python, NumPy, or the `physure`
Python package — a `.phs` script runs identically whether or not Python is installed on the
machine.

## License

[MIT](https://github.com/Alexisrx96/physure/blob/main/LICENSE) — Irvin Torres
