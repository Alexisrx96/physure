# Migration Guide: `measurekit` → `physure`

> **TL;DR** — The package is being renamed. The API is 100% identical.
> Replace `import measurekit` with `import physure` and you're done.

---

## Why the rename?

`measurekit` is evolving into a more focused, physics-first library.
The name **`physure`** (physics + measure) better reflects what this tool actually does:
it is a unit-aware, dimension-correct engine for physical quantities — not a general measurement toolkit.

The rename happens in two steps:

| Version | What changes |
|---------|-------------|
| **0.1.9** *(current)* | `measurekit` emits a `DeprecationWarning` on import |
| **0.2.0** | Package published as `physure`; `measurekit` receives no further updates |

---

## Migration steps

### 1. Update your installation

```bash
pip uninstall measurekit
pip install physure
```

Or with `uv`:

```bash
uv remove measurekit
uv add physure
```

With extras (same extras are supported):

```bash
pip install "physure[native]"    # Rust-accelerated core
pip install "physure[numpy]"     # NumPy + SciPy + Numba
pip install "physure[torch]"     # PyTorch integration
pip install "physure[jax]"       # JAX integration
pip install "physure[all]"       # Everything
```

### 2. Update your imports

This is a **global find-and-replace**. No logic changes needed.

```python
# Before
import measurekit
from measurekit import Q_, Quantity, units
from measurekit.domain.measurement.quantity import Quantity

# After
import physure
from physure import Q_, Quantity, units
from physure.domain.measurement.quantity import Quantity
```

### 3. Update the CLI (if used)

```bash
# Before
measurekit "500 N / 2 m^2 => kPa"
python -m measurekit

# After
physure "500 N / 2 m^2 => kPa"
python -m physure
```

### 4. Update pyproject.toml / requirements.txt

```toml
# pyproject.toml — before
dependencies = ["measurekit>=0.1.8"]

# After
dependencies = ["physure>=0.2.0"]
```

```text
# requirements.txt — before
measurekit>=0.1.8

# After
physure>=0.2.0
```

---

## What does NOT change

- **The entire public API** — `Q_`, `Quantity`, `Uncertainty`, `units`, `jit`, all backends, all extras.
- **All unit definitions** — every unit, prefix, and CODATA constant is preserved.
- **The Rust core** — `physure_core` is the renamed `measurekit_core`; same functionality, same performance.
- **Python version support** — Python 3.10–3.14.
- **Zero runtime dependencies** policy.

---

## Suppressing the warning during transition

If you are not ready to migrate and want to silence the `DeprecationWarning` temporarily:

```python
import warnings
with warnings.catch_warnings():
    warnings.simplefilter("ignore", DeprecationWarning)
    import measurekit
```

Or from the command line:

```bash
python -W ignore::DeprecationWarning your_script.py
```

> [!CAUTION]
> This is a **temporary workaround only**. `measurekit` will receive no
> updates after v0.2.0 is released. Plan your migration accordingly.

---

## Timeline

- **v0.1.9** — `DeprecationWarning` added. API unchanged. `physure` package published on PyPI.
- **v0.2.0** — Full rename. `measurekit` archived. All future development on `physure`.

---

## Questions or issues?

Open an issue on [GitHub](https://github.com/Alexisrx96/measurekit/issues).
