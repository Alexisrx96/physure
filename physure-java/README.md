<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/Alexisrx96/physure/main/assets/logo-horizontal-dark.svg">
  <img src="https://raw.githubusercontent.com/Alexisrx96/physure/main/assets/logo-horizontal-light.svg" alt="physure" width="380">
</picture>

<h3><b>physure — JVM JNI Bindings for Java & Kotlin</b></h3>

[![Maven Central](https://img.shields.io/maven-central/v/io.github.alexisrx96/physure?color=F59E0B&labelColor=18181A)](https://central.sonatype.com/artifact/io.github.alexisrx96/physure)
[![CI](https://img.shields.io/github/actions/workflow/status/Alexisrx96/physure/bindings.yml?branch=main&labelColor=18181A)](https://github.com/Alexisrx96/physure/actions/workflows/bindings.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-F59E0B?labelColor=18181A)](https://github.com/Alexisrx96/physure/blob/main/LICENSE)

</div>

---

JVM JNI bindings for the **[physure](https://github.com/Alexisrx96/physure)** Rust physics and measurement engine. Compatible with **Java 8** through modern Java releases, and tested in CI across every LTS version (**Java 8, 11, 17, 21, and 25**).

## Installation

### Maven

```xml
<dependency>
    <groupId>io.github.alexisrx96</groupId>
    <artifactId>physure</artifactId>
    <version>0.4.0</version>
</dependency>
```

### Gradle (Kotlin DSL)

```kotlin
implementation("io.github.alexisrx96:physure:0.4.0")
```

The published JAR bundles prebuilt native JNI libraries for **Linux** (`x86_64`, `aarch64`), **macOS** (`x86_64`, `aarch64`), and **Windows** (`x86_64`) under `/natives/`, so no `-Djava.library.path` setup is required when consuming from Maven Central.

### Java 24 and later

The JAR loads its bundled JNI library via `System.load` / `System.loadLibrary`. Starting in Java 24, the JVM prints a restricted native access warning unless enabled explicitly. Pass `--enable-native-access=ALL-UNNAMED` when running on **Java 24+** (do not pass it on older JVMs, which do not recognize the flag).

## Quick Start

```java
import com.physure.ExpandedUncertainty;
import com.physure.PhyFunction;
import com.physure.Quantity;
import com.physure.UnitRegistry;
import java.util.Map;

public class Main {
    public static void main(String[] args) {
        try (UnitRegistry registry = new UnitRegistry()) {
            // 1. Dimensional analysis & unit scales
            Map<String, Integer> exponents = registry.getUnitExponents("kg*m/s^2");
            System.out.println("Exponents: " + exponents); // {kg=1, m=1, s=-2}
            // Fractional exponents: registry.getUnitPowers("s^0.5") -> {s=0.5}

            double scale = registry.getUnitScale("cm");
            System.out.println("cm scale: " + scale); // 0.01

            // 2. Unit-aware quantities & conversions
            Quantity distance = new Quantity(10.0, "m");
            Quantity time = new Quantity(2.0, "s");
            Quantity velocity = distance.divide(time);
            System.out.println("Velocity: " + velocity); // 5.0 m/s
            System.out.println("In km/h: " + velocity.to("km/h")); // 18.0 km/h

            // 3. Correlated uncertainty propagation (GUM) & degrees of freedom
            Quantity v = new Quantity(12.0, 0.1, "V").withDof(9);
            Quantity i = Quantity.fromSamples(new double[] {1.98, 2.01, 2.00, 2.02, 1.99}, "A");
            Quantity power = v.multiply(i).to("W");
            ExpandedUncertainty u95 = power.expanded(0.95);
            System.out.println("Power (95%): " + u95);

            // 4. Evaluate .phs scripts dynamically
            Quantity result = registry.evaluate("a = 15 m; b = 5 m; a * b");
            System.out.println("Eval result: " + result.getValue() + " " + result.getUnit()); // 75.0 m^2

            // 5. Symbolic calculus and equation solving
            System.out.println("Derivative: " + registry.deriv("v0 * t + 0.5 * a * t^2", "t")); // v0 + a * t
            System.out.println("Integral: " + registry.integral("3 * t^2", "t"));               // t^3
            System.out.println("Solve for T: " + registry.solve("P * V = n * R * T", "T"));     // P * V / (n * R)

            // 6. Reusable & differentiable functions (PhyFunction)
            PhyFunction ke = new PhyFunction(registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2");
            Quantity mass = new Quantity(10.0, "kg");
            Quantity speed = new Quantity(5.0, "m/s");
            System.out.println("Kinetic energy: " + ke.call(mass, speed)); // 125.0 J

            PhyFunction dKeDv = ke.deriv("v");
            System.out.println("d(KE)/dv: " + dKeDv.call("10 kg", "5 m/s")); // 50.0 kg * m * s^-1

            PhyFunction solveM = ke.solve("m");
            System.out.println("Solved mass: " + solveM.call(new Quantity(125.0, "J"), speed)); // 10.0 kg
        }
    }
}
```

## Uncertainty & Measurement Semantics

A `Quantity` is a *measurement*: it tracks the identity of its uncertainty sources across operations, following JCGM 100:2008 (GUM):

- Reusing a measurement cancels or accumulates uncertainty as the GUM specifies (`x.subtract(x)` has `0` uncertainty; `x.multiply(x)` has uncertainty `2 |x| u`).
- Two quantities constructed separately represent distinct measurements even when their numbers and units are equal. `equals` compares only value, uncertainty, and unit.
- Degrees of freedom and Student-*t* coverage factors are supported via `x.withDof(9)`, `Quantity.fromSamples(new double[] {...}, "V")` (Type A), `y.effectiveDof()`, `y.coverageFactor(0.95)`, and `y.expanded(0.95)` (`ExpandedUncertainty`).
- `PhyFunction.call(Quantity...)` and `PhyEquation.call(...)` preserve measurement sources across calls.

## Thread Safety

- A `UnitRegistry` can be shared across threads: every call acquires the registry's internal lock, and `close()` acquires the same lock. Separate `UnitRegistry` instances do not share mutable state and never contend.
- `Quantity`, `QuantityVector`, and `QuantityMatrix` are immutable.

## Building from Source

```bash
cargo build --release --package physure-java
```

Cargo outputs the native JNI library into `target/release/` (`physure_java.dll` on Windows, `libphysure_java.so` on Linux, `libphysure_java.dylib` on macOS).

## License

[MIT](https://github.com/Alexisrx96/physure/blob/main/LICENSE) — Irvin Torres
