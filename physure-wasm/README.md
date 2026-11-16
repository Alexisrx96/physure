<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/Alexisrx96/physure/main/assets/logo-horizontal-dark.svg">
  <img src="https://raw.githubusercontent.com/Alexisrx96/physure/main/assets/logo-horizontal-light.svg" alt="physure" width="380">
</picture>

<h3><b>physure — WebAssembly Bindings for JavaScript & TypeScript</b></h3>

[![npm](https://img.shields.io/npm/v/physure?color=F59E0B&labelColor=18181A)](https://www.npmjs.com/package/physure)
[![CI](https://img.shields.io/github/actions/workflow/status/Alexisrx96/physure/bindings.yml?branch=main&labelColor=18181A)](https://github.com/Alexisrx96/physure/actions/workflows/bindings.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-F59E0B?labelColor=18181A)](https://github.com/Alexisrx96/physure/blob/main/LICENSE)

</div>

---

WebAssembly (`wasm-bindgen`) bindings for the **[physure](https://github.com/Alexisrx96/physure)** Rust physics and measurement engine. It provides exact rational-exponent dimensional analysis, GUM-compliant correlated uncertainty propagation, unit conversions, and the `.phs` calculation DSL directly in **Node.js** and modern **JS/TS bundlers** (Vite, Webpack, Rollup, esbuild).

## Installation

```bash
npm install physure
```

The package ships dual WebAssembly builds with full TypeScript declarations (`physure_wasm.d.ts`):
- **Node.js (`node` export condition):** `./pkg-node/physure_wasm.js`
- **Bundlers (`import` / `default` condition):** `./pkg-bundler/physure_wasm.js`

## Quick Start

```typescript
import { Quantity, UnitRegistry, PhyFunction } from "physure";

// 1. Unit-aware quantities & conversions
const distance = new Quantity(10, "km");
const time = new Quantity(2, "h");
const speed = distance.divide(time).convertTo("m/s");

console.log(speed.toString()); // "1.3888888888888888 m * s^-1"
console.log(speed.value, speed.unit);

// 2. Correlated uncertainty propagation (GUM) & degrees of freedom
const voltage = Quantity.withUncertainty(12.0, 0.1, "V").withDof(9);
const current = Quantity.fromSamples(new Float64Array([1.98, 2.01, 2.00, 2.02, 1.99]), "A");
const power = voltage.multiply(current).convertTo("W");

console.log("Power:", power.toString());
console.log("Effective DoF (Welch-Satterthwaite):", power.effectiveDof());
console.log("Expanded uncertainty (95%):", power.expandedUncertainty(0.95));

// Reusing the same measurement tracks correlation automatically:
console.log(voltage.subtract(voltage).uncertainty); // 0

// 3. UnitRegistry & PHS script evaluation
const registry = new UnitRegistry();
console.log(registry.getUnitScale("cm")); // 0.01
console.log(registry.getUnitExponents("kg*m/s^2")); // Map { 'kg' => 1, 'm' => 1, 's' => -2 }

console.log(registry.evaluate("a = 15 m; b = 5 m; a * b"));
// "15.0 m\n5.0 m\n75.0 m^2"

// 4. Symbolic calculus & equation solving
console.log(registry.deriv("v0 * t + 0.5 * a * t^2", "t")); // "a * t + v0"
console.log(registry.integral("3 * t^2", "t"));             // "t^3"
console.log(registry.solve("P * V = n * R * T", "T"));      // "T = (P * V)/(R * n)"

// 5. Reusable physics functions (PhyFunction)
const kineticEnergy = new PhyFunction(
  registry,
  "ke",
  "ke(m, v) = 0.5 * m * v^2"
);
const energy = kineticEnergy.call(new Quantity(10, "kg"), new Quantity(5, "m/s")).convertTo("J");
console.log(energy.toString()); // "125.0 J"
```

## Uncertainty & Measurement Semantics

A `Quantity` is a *measurement*: it tracks the identity of its uncertainty sources across operations, following JCGM 100:2008 (GUM):

- Reusing the same `Quantity` instance accounts for correlation (`x.subtract(x)` has `0` uncertainty; `x.multiply(x)` has uncertainty `2 |x| u`).
- Two separately constructed `Quantity` instances represent independent measurements even if their numeric values and units are equal.
- Type A uncertainty from repeated observations is supported via `Quantity.fromSamples(samples, unit)`, and Welch-Satterthwaite effective degrees of freedom and Student-*t* coverage factors are available via `effectiveDof()`, `coverageFactor(p)`, and `expandedUncertainty(p)`.

## License

[MIT](https://github.com/Alexisrx96/physure/blob/main/LICENSE) — Irvin Torres
