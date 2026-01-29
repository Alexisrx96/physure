This is the specific roadmap for **MeasureKit (v0.2 → v1.0)**, designed to act as the "engine block" for the chassis we just designed in MeasureNote.

While MeasureNote focuses on the _user experience_ (Editor, Graphs, UI), MeasureKit must focus on **Runtime Performance, Memory Layout, and WASM Compatibility**.

The core theme of this roadmap is **"Headless & Serializable"**. MeasureKit must run blindly in a Web Worker and provide data structures that can be teleported to the main thread instantly.

---

# **Technical Roadmap: MeasureKit (The "Engine" Upgrade)**

## **Phase 1: The "WASM-First" Optimization (Crucial for MeasureNote)**

**Objective:** Ensure MeasureKit runs in Pyodide/WASM with minimal startup time and memory footprint.

### **1.1. Lazy Backend Loading**

- **Problem:** Currently, `measurekit` might try to import `torch`, `jax`, or `scipy` at the top level to check availability. In Pyodide, this triggers massive network requests to download 100MB+ libraries even if the user only wants simple arithmetic.
- **Action:** Refactor `measurekit/core/dispatcher.py` and `backend` imports to be strictly lazy.
- **Implementation:**
- Use `importlib.util.find_spec` inside methods, or local imports.
- Ensure `import measurekit` completes in <50ms in a fresh Pyodide environment.

### **1.2. The "Pyodide Compatibility" Build**

- **Problem:** The Rust extension (`measurekit_core`) must be compiled to WASM.
- **Action:** Create a `maturin` build pipeline specifically for `emscripten/wasm32-unknown-emscripten`.
- **Deliverable:** A `measurekit-0.x.x-cp311-cp311-emscripten_3_1_45_wasm32.whl` released on GitHub Releases. This allows MeasureNote to `micropip.install()` a pre-compiled binary instead of compiling Rust on the client (impossible) or falling back to slow Python mode.

---

## **Phase 2: Serialization Protocols (The "Zero-Copy" Bridge)**

**Objective:** Enable the `CovarianceStore` to dump its state directly to a `SharedArrayBuffer` for the MeasureNote UI.

### **2.1. Apache Arrow Export**

- **Context:** MeasureNote needs to render heatmaps of the covariance matrix.
- **Action:** Add a method `CovarianceStore.to_arrow()`.
- **Implementation:**
- Do not use `pyarrow` (too heavy for browser). Use `flatbuffers` or a lightweight struct packer.
- **Format:**

```python
def export_buffer(self):
    # Returns a bytes object representing the CSR matrix
    # [header: 4 bytes][row_ptrs: N*4][col_indices: M*4][values: M*8]
    return self._core.serialize_csr()

```

- **Usage:** MeasureNote's JS side can view this memory directly via `new Float64Array(wasm.memory.buffer, offset)`.

### **2.2. State Snapshots (Time Travel)**

- **Feature:** `measurekit.save_state()` / `measurekit.load_state()`.
- **Action:** Make `SymbolTable` and `CovarianceStore` serializable via `pickle` or `msgpack`.
- **Benefit:** Enables MeasureNote to implement "Undo/Redo" for the entire physics engine state, not just the text.

---

## **Phase 3: Symbolic-Numeric Compilation (The "Solver" Bridge)**

**Objective:** Bridge the gap between "solving an equation" and "running it fast."

### **3.1. The `Lambdify` Pipeline**

- **Current State:** `symbolic.py` exists but is isolated.
- **Action:** Implement `SymbolicExpression.compile(backend='numpy')`.
- **Logic:**

1. Take a SymPy expression (e.g., ).
2. Trace the variable dependencies.
3. Generate a Python function using `def` syntax (AST generation).
4. **Key Step:** Inject `measurekit` unit checks into the generated code.
5. Return a callable that runs at native NumPy speed.

- **Why:** Allows MeasureNote to support user-defined functions like `f(x) = x^2 + 2*x` that run efficiently in loops.

---

## **Phase 4: Robust "Fall-Through" Dispatch**

**Objective:** Ensure the engine never crashes, even if the user feeds it "garbage" data types (like pure Python lists).

### **4.1. The "List-to-Array" Guard**

- **Problem:** As noted in your `roadmap.md`, `sin([1, 2])` might fail in the PythonBackend.
- **Action:** Implement the "Vectorization Wrapper" in `measurekit/core/dispatcher.py`.
- **Logic:**

```python
def ensure_backend_compatible(func):
    def wrapper(x):
        if isinstance(x, list):
            # Auto-upgrade to NumPy if available, else map
            if numpy_available: return func(np.array(x))
            return [func(i) for i in x]
        return func(x)
    return wrapper

```

- **Benefit:** MeasureNote users often type vectors as `[1, 2, 3]`. This ensures they don't see "TypeError".

---

## **Summary of MeasureKit Deliverables for Interoperability**

| Version  | Focus         | Deliverable for MeasureNote                                           |
| -------- | ------------- | --------------------------------------------------------------------- |
| **v0.2** | **WASM**      | `measurekit.whl` (wasm32) with pre-compiled Rust core.                |
| **v0.3** | **Transport** | `CovarianceStore.export_binary()` for fast UI heatmaps.               |
| **v0.4** | **Solver**    | `compile()` method to turn SymPy equations into executable Functions. |
| **v1.0** | **Stability** | Full JAX/Torch support for the "AI Extension".                        |

### **Immediate Next Step for You:**

Focus on **Phase 1.2**. Generating a valid **WASM wheel** for `measurekit` is the single biggest blocker to getting high-performance physics in the browser. Without this, MeasureNote is stuck using the slow, pure-Python fallback.
