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
| **$N = 100$** | **1.9 ms** (`0.0019 s`) | **0.9 ms** (`0.0009 s`) | ~146.4 MB (Base Scipy Sparse) |
| **$N = 1,000$** | **30.2 ms** (`0.0302 s`) | **14.3 ms** (`0.0143 s`) | +0.27 MB |
| **$N = 5,000$** | **1.70 s** (`1.7045 s`) | **1.37 s** (`1.3735 s`) | +2.13 MB |

---

## 🔥 4. Aceleración JIT Tensor (PyTorch GPU/CPU)

Benchmarking de operaciones algebraicas sobre tensores de **1,000,000 de elementos** ($10^6$ elementos):

| Modo de Ejecución | Tiempo por Iteración | Overhead vs PyTorch Nativo |
| :--- | :---: | :---: |
| **Pure PyTorch (Baseline)** | **0.2196 ms** | $1.00\times$ (Referencia) |
| **Physure Eager Mode** | **0.6771 ms** | $3.08\times$ |
| **Physure `@torch.compile`** | **0.2675 ms** | **$1.22\times$** *(Prácticamente Zero Overhead)* |

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
