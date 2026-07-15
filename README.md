> [!WARNING]
> **`measurekit` is deprecated as of v0.1.9.** This package will be renamed to **`physure`** in v0.2.0.
> The API is identical — migration is a one-liner:
> ```bash
> pip uninstall measurekit && pip install physure
> ```
> Then replace `import measurekit` → `import physure` in your code.
> See the full **[Migration Guide](MIGRATION.md)** for details.

# MeasureKit *(deprecated → physure)*

<div align="center">
<img src="https://cdn.irvintorres.com/MeasureKitLogoBeta.jpg" alt="MeasureKit Logo" width="500">

<h3><b>The Over-Engineered Homework Validator</b></h3>
<p><i>A Multi-Backend Physical Dimension Engine born from a professional dev's obsession with doing simple things the hard way.</i></p>
</div>

---

## 🛑 What is this actually?

MeasureKit didn't start as an enterprise product. It started because I wanted to verify my physics studies. But I'm not a student learning to code; I'm a professional developer who looked at `12 m + 13 m` and thought, _"I could just add them... or I could build a backend-agnostic, JIT-compilable, tensor-compatible engine to do it for me."_

I chose the latter.

This project is the result of applying enterprise architecture patterns and performance obsessions to a problem that didn't strictly need them. It is a one-man (plus AI) show, pushing Python's dynamic nature to its absolute limit to see what happens when you try to force physics into high-performance compute graphs.

---

---

## ⚡ The "Zero-Overhead" Reality Check

The original marketing says "Zero-Overhead." Let's be precise about what that means, because in standard Python, **there is overhead.**

- **Eager Mode (Standard Python):** 🐢 **Slow.** Creating a `Quantity` object involves allocating Python classes, checking units, and handling validations. It is significantly slower than raw `float` or `torch.Tensor` math (sometimes 10x-100x overhead). Do not use this for tight loops in production unless speed is irrelevant.
- **Compiled Mode (The Trick):** 🚀 **Fast.** The "Zero-Overhead" claim is only true if you use **JIT Compilation** (`torch.compile` or `jax.jit`).
- We use `__torch_dispatch__` to strip away the `Quantity` abstraction during the tracing phase.
- The final execution graph (e.g., Triton kernel) sees only raw tensors. The units evaporate.
- **Trade-off:** Unit safety checks happen _at compile time_. If you bypass them or have dynamic units that change at runtime, you might break the illusion.

---

## 🛠 Features (The Good & The Complex)

### 1. Homework Syntax (The Original Goal)

This is what it was built for. Simple, intuitive syntax to check your work.

```python
from measurekit import Q_

# Solving a kinematic problem
d = Q_(10, "km")
t = Q_(2, "hr")
v = d / t

print(f"My answer: {v.to('m/s')}") # 1.3888888888888888 m/s

```

### 2. Multi-Backend Tensors (The Ambition)

We wrap **NumPy**, **PyTorch**, and **JAX**. If you pass a tensor, we try to stay out of the way.

- **Warning:** Broadcasting uncertainty (e.g., adding a scalar error to a tensor value) is mathematically expensive and complex. We handle it, but it's heavy machinery.

### 3. Rust Core (The Optimization)

We integrated a Rust backend (`measurekit_core`) via PyO3 to speed up the heavy lifting.

- **Honesty:** Crossing the Python-Rust boundary isn't free. It helps, but it doesn't magically fix Python's inherent slowness for small scalars.

### 4. Engineering Notes as Code (The Grammar Extension)

`measurekit.ext.grammar` evaluates [MeasureNote](https://github.com/Alexisrx96/MeasureNote)-style meta-language: write physics like you'd write it on paper, and let the unit system keep you honest. Zero extra dependencies.

```python
from measurekit.ext.grammar import GrammarInterpreter

mn = GrammarInterpreter()
mn.run("""
force = 500 N              # assignment (`->` works too)
area = 2 m^2
stress = force / area
""")
mn.eval("stress => kPa")       # Quantity(0.25, kPa)
mn.eval("stress == 250 Pa")    # True — assertions validate your work
mn.eval("g = 9.81 +/- 0.02 m/s^2 = ?")  # uncertainty literals, query with `= ?`
```

- Units are just identifiers: `500 N` is implicit multiplication against the active `UnitSystem`, so every registered unit (and typo suggestion) works out of the box.
- **Honesty:** it covers the core statements (assign, query, convert, assert). Function definitions, derivatives, and Monte Carlo commands live in the `symbolic` module, not here.

### 5. Terminal Calculator (The Freebie)

The grammar extension powers a REPL, so measurekit works without writing any Python — like GNU `units`, but with uncertainty propagation:

```console
$ python -m measurekit "500 N / 2 m^2 => kPa"
0.25 kPa

$ python -m measurekit < notes.mnml   # evaluate a file of statements

$ python -m measurekit                # interactive session
mk> g = 9.81 +/- 0.02 m/s^2
mk> g => ft/s^2
```

Also available as `measurekit repl` from the CLI.

### 📚 Supported Units & Constants Reference
For a complete, dynamically generated list of all supported dimensions, prefixes, physical units, and CODATA 2022 physical constants, see [docs/UNITS.md](docs/UNITS.md).

---

## 📦 Installation

### 1. From PyPI
We recommend using `uv` or `pip` to install. You can customize the installation using extras depending on your use case:

* **Minimal installation** (pure Python fallback, zero runtime dependencies):
  ```bash
  pip install measurekit
  ```

* **With native Rust acceleration** (highly recommended for eager mode scalar speedups):
  ```bash
  pip install "measurekit[native]"
  ```

* **With all backend integrations** (NumPy, PyTorch, JAX, SymPy, Pandas, etc.):
  ```bash
  pip install "measurekit[all]"
  ```

### 2. Building from Source (Local Development)
Because MeasureKit uses a hybrid Python-Rust architecture, building from source requires the Rust toolchain and `maturin`.

1. **Prerequisites**:
   Ensure you have Rust installed (via `rustup`) and the `uv` virtual environment configured.

2. **Compile and develop the Rust Core backend**:
   ```bash
   cd measurekit_core
   # This builds the Rust extension and registers it in the active virtualenv
   maturin develop --release
   cd ..
   ```

3. **Install the Python package in editable mode**:
   ```bash
   # Install MeasureKit along with all optional and development dependencies
   pip install -e ".[all]"
   ```

---

## 🧪 Benchmarks (No Cherry-Picking)

We run benchmarks to keep ourselves honest.

- **Pure PyTorch:** 0.0xxx ms
- **MeasureKit (Eager):** Significantly slower (overhead from Python objects).
- **MeasureKit (Compiled):** **< 1.1x overhead** compared to raw PyTorch.

_If you need raw speed, you MUST compile your code. If you just need to check unit consistency for data processing, eager mode is fine._

---

## Contributing & Vision

This project is what happens when a Senior Engineer treats a "side project" with the same intensity as a production system.

It is complex—sometimes intentionally, sometimes because I was learning how to bend PyTorch's dispatcher to my will. It is a testbed for architectural concepts as much as it is a physics library.

I am trying to build something robust that bridges the gap between "handwritten math" and "GPU-accelerated tensors." If you see code that looks incredibly dense or abstracted, it's not because I didn't know better—it's because I was trying to make it _perfect_ (and probably overcooked it).

Contributions are welcome. Just know that you are stepping into a codebase built by one guy who refused to compromise on features, even when he probably should have.

---

## 📄 License

**MIT License**. Use it, break it, inspect the architecture.

Built with ☕, 🤖, and years of accumulated dev trauma by **Irvin Torres**.
