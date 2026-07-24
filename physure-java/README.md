# physure-java — JVM FFI Bindings

This package contains JVM JNI bindings for the high-performance `physure` core physics engine. It is compatible with **Java 8** through the most recent Java versions.

## Project Structure

* **`src/lib.rs`**: JNI FFI bindings written in Rust (using the `jni-rs` crate) exporting Java-accessible native functions.
* **`java/com/physure/`**: High-level Java wrapper classes (`NativeEngine.java`, `UnitRegistry.java`, `Quantity.java`) offering a clean object-oriented interface.

## How to Build the Native Library

Compile the dynamic JNI library:

```bash
cargo build --release --package physure-java
```

On compilation, cargo outputs the library into `target/release/`:
* **Windows**: `physure_java.dll`
* **Linux**: `libphysure_java.so`
* **macOS**: `libphysure_java.dylib`

## Usage in Java 8+

To load and use the native engine inside your Java project:

```java
import com.physure.UnitRegistry;
import java.util.Map;

public class Main {
    public static void main(String[] args) {
        // Load library by setting -Djava.library.path=path/to/target/release
        // or package the library inside your jar's /natives/ folder.
        
        try (UnitRegistry registry = new UnitRegistry()) {
            // 1. Get exponents of unit expressions
            Map<String, Integer> exponents = registry.getUnitExponents("kg*m/s^2");
            System.out.println("Exponents: " + exponents); // {kg=1, m=1, s=-2}

            // 2. Get scale factor
            double scale = registry.getUnitScale("cm");
            System.out.println("cm scale: " + scale); // 0.01

            // 3. Evaluate statements dynamically and return a Quantity object
            Quantity result = registry.evaluate("a = 15 m; b = 5 m; a * b");
            System.out.println("Eval result: " + result.getValue() + " " + result.getUnit()); // 75.0 m^2

            // 4. Symbolic calculus and equation solving (despejes)
            String derivative = registry.deriv("v0 * t + 0.5 * a * t^2", "t");
            System.out.println("Derivative: " + derivative); // v0 + a * t

            String integral = registry.integral("3 * t^2", "t");
            System.out.println("Integral: " + integral); // t^3

            String solution = registry.solve("P * V = n * R * T", "T");
            System.out.println("Solution for T: " + solution); // P * V / (n * R)

            // 5. Quantity object-oriented arithmetic
            Quantity q1 = new Quantity(10.0, "m");
            Quantity q2 = new Quantity(2.0, "s");
            Quantity velocity = q1.divide(q2);
            System.out.println("Velocity: " + velocity); // 5.0 m/s

            Quantity velocityKmh = velocity.to("km/h");
            System.out.println("In km/h: " + velocityKmh); // 18.0 km/h

            // 6. Stateful Custom Functions (PhyFunction)
            PhyFunction ke = new PhyFunction(registry, "kinetic_energy", "kinetic_energy(m, v) = 0.5 * m * v^2");
            
            // Call with string args (returns a Quantity object)
            Quantity energyStr = ke.call("10 kg", "5 m/s");
            System.out.println("Energy (from String args): " + energyStr); // 125.0 J
            
            // Call with strongly-typed Quantity arguments
            Quantity mass = new Quantity(10.0, "kg");
            Quantity speed = new Quantity(5.0, "m/s");
            Quantity energyQty = ke.call(mass, speed);
            System.out.println("Energy (from Quantity args): " + energyQty); // 125.0 J

            // 7. Symbolic Manipulation of PhyFunctions (differentiable, integrable, solvable)
            PhyFunction dKe_dv = ke.deriv("v");
            System.out.println("dKe/dv expression: " + dKe_dv.call("10 kg", "5 m/s")); // 50.0 kg * m * s^-1 (returns Quantity)

            PhyFunction intKe_dv = ke.integral("v");
            System.out.println("int(Ke)dv expression: " + intKe_dv.call("10 kg", "5 m/s")); // 208.333333333333 J * m * s^-1

            PhyFunction solveM = ke.solve("m"); // solve for mass: solve_kinetic_energy_for_m(target, v)
            Quantity targetEnergy = new Quantity(125.0, "J");
            Quantity solvedMass = solveM.call(targetEnergy, speed);
            System.out.println("Solved Mass: " + solvedMass); // 10.0 kg
        }
    }
}
```
