# Physure Performance Benchmarks

Este documento recopila las métricas de rendimiento y la aceleración nativa alcanzada por **`physure`** tras la reestructuración al motor físico Rust en arquitectura **Thin Wrapper** (Rust-first engine via FFI zero-copy).

---

## 💻 Entorno de Ejecución

- **SO**: Linux x86_64
- **Runtime Python**: CPython 3.14 (vía `uv`)
- **Compilador Rust**: `rustc` (Profile `release`)
- **Puente FFI**: `PyO3` / `Maturin` abi3 (C Buffer Protocol / Arrow IPC)
- **Dispositivo GPU / Tensor**: CUDA / CPU (PyTorch)

---

## ⚡ 1. Benchmark de Tiempo de Importación

Medición de overhead al importar el paquete y realizar la primera instanciación física.

| Métrica | Tiempo | Descripción |
| :--- | :--- | :--- |
| **Cold Process Import** | **21.13 ms ± 3.18 ms** | Promedio de 30 procesos Python independientes (`python -c "import physure"`). |
| **Min Cold Import** | **16.07 ms** | Tiempo mínimo absoluto de inicio de proceso e importación. |
| **In-Process Import** | **2.55 ms** | Tiempo interno de carga del paquete dentro de un proceso activo. |
| **Primera creación de `Q_`** | **200.76 ms** | Carga a demanda (*lazy*) de los registros de sistemas de unidades (SI / Imperial). |

---

## 📈 2. Crecimiento de Linaje e Historial de Adiciones

Prueba de acumulación de grafos de correlación bajo iteraciones consecutivas:

| Operaciones | Modo Correlacionado | Modo No Correlacionado |
| :---: | :---: | :---: |
| **1,000 adiciones** | **54.1 ms** (`0.0541 s`) | **34.9 ms** (`0.0349 s`) |
| **5,000 adiciones** | **179.4 ms** (`0.1794 s`) | **179.2 ms** (`0.1792 s`) |

---

## 🧮 3. Propagación de Incertidumbre Vectorizada en Arrays

Rendimiento al propagar matrices de covarianza esparsas sobre arrays de gran dimensión:

| Dimensión de Array ($N$) | Modo Correlacionado | Modo No Correlacionado | Incremento Memoria |
| :---: | :---: | :---: | :---: |
| **$N = 100$** | **3.5 ms** (`0.0035 s`) | **0.8 ms** (`0.0008 s`) | ~143.5 MB (Base Scipy Sparse) |
| **$N = 1,000$** | **54.9 ms** (`0.0549 s`) | **44.2 ms** (`0.0442 s`) | +0.24 MB |
| **$N = 5,000$** | **0.99 s** (`0.9977 s`) | **1.27 s** (`1.2783 s`) | +2.04 MB |

---

## 🔥 4. Aceleración JIT Tensor (PyTorch GPU/CPU)

Benchmarking de operaciones algebraicas sobre tensores de **1,000,000 de elementos** ($10^6$ elementos):

| Modo de Ejecución | Tiempo por Iteración | Overhead vs PyTorch Nativo |
| :--- | :---: | :---: |
| **Pure PyTorch (Baseline)** | **0.2251 ms** | $1.00\times$ (Referencia) |
| **Physure Eager Mode** | **0.7078 ms** | $3.14\times$ |
| **Physure `@torch.compile`** | **0.2643 ms** | **$1.17\times$** *(Prácticamente Zero Overhead)* |

---

## ⚡ 5. Micro-Benchmarks Nativos de Rust (`physure-core` Criterion)

Medición directa sin overhead FFI realizada con la suite de Criterion en Rust (`cargo bench`):

| Micro-Benchmark | Tiempo Promedio | Descripción |
| :--- | :---: | :--- |
| **`unit_mul`** | **53.84 ns** | Multiplicación directa de exponentes de `RationalUnit`. |
| **`unit_div`** | **53.83 ns** | División directa de exponentes de `RationalUnit`. |
| **`quantity_add_scalar`** | **40.42 ns** | Suma escalar con validación dimensional. |
| **`quantity_mul_scalar`** | **65.55 ns** | Multiplicación escalar con acumulación dimensional. |
| **`compiled_symbolic_eval`** | **13.59 ns** | Evaluación de pila en expresiones simbólicas optimizadas. |
| **`covariance_propagate`** | **7.16 µs** | Propagación de covarianza esparsa de bloques matriciales. |

---

## 🚀 Reejecutar los Benchmarks en Local

Para verificar estas métricas localmente en tu sistema:

```bash
# Entrar al repositorio
cd /home/irvint/Projects/physure

# Asegurar que el modulo C está compilado
uv run --directory physure-python maturin develop --release

# Benchmark de importación
uv run --directory physure-python python -c "
import time, subprocess, sys
t0 = time.perf_counter()
import physure
t1 = time.perf_counter()
print(f'In-process import: {(t1 - t0)*1000:.3f} ms')
"

# Ejecutar scripts de benchmark
uv run --directory physure-python python benchmarks/benchmark_lineage.py
uv run --directory physure-python python benchmarks/benchmark_propagation.py
uv run --directory physure-python python benchmarks/benchmark_comparison.py
```
