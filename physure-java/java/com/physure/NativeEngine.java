package com.physure;

import java.io.File;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.util.Map;

/**
 * Native JNI wrapper for the physure core physics engine.
 * Compatible with Java 8+.
 */
public class NativeEngine {
    static {
        try {
            // Attempt to load standard system library
            System.loadLibrary("physure_java");
        } catch (UnsatisfiedLinkError e) {
            // Fallback: extract and load dynamic library from classpath resource (for jar packaging)
            try {
                loadFromJar();
            } catch (Exception ex) {
                throw new RuntimeException("Failed to load native library physure_java", ex);
            }
        }
    }

    private static void loadFromJar() throws Exception {
        String libName = System.mapLibraryName("physure_java");
        String os = System.getProperty("os.name").toLowerCase();
        String arch = System.getProperty("os.arch").toLowerCase();
        
        String resourcePath = "/natives/";
        if (os.contains("win")) {
            resourcePath += "windows/";
        } else if (os.contains("mac")) {
            resourcePath += "macos/";
        } else {
            resourcePath += "linux/";
        }
        resourcePath += arch + "/" + libName;

        try (InputStream in = NativeEngine.class.getResourceAsStream(resourcePath)) {
            if (in == null) {
                throw new UnsatisfiedLinkError("Native library " + libName + " not found at " + resourcePath);
            }
            File temp = File.createTempFile("libphysure_java-", "-" + libName);
            temp.deleteOnExit();
            Files.copy(in, temp.toPath(), StandardCopyOption.REPLACE_EXISTING);
            System.load(temp.getAbsolutePath());
        }
    }

    /**
     * Initializes a new unit registry handle using the master physure.conf file.
     * 
     * @return Opaque pointer handle to the UnitRegistry in Rust.
     */
    public static native long initRegistry();

    /**
     * Initializes a new unit registry handle with a custom override config file path.
     */
    public static native long initRegistryFromPath(String path);

    /**
     * Initializes a new unit registry handle with custom override config string content.
     */
    public static native long initRegistryFromContent(String content);

    /**
     * Frees the Rust UnitRegistry memory.
     * 
     * @param handle Opaque pointer handle of the UnitRegistry.
     */
    public static native void destroyRegistry(long handle);

    /**
     * Parses a unit expression and returns its dimension exponents.
     * E.g. "kg*m/s^2" returns {"kg": 1, "m": 1, "s": -2}
     * 
     * @param registryHandle The registry handle.
     * @param expr           The unit expression to parse.
     * @return A map of unit symbols to their exponents, or null on error.
     */
    public static native Map<String, Integer> getUnitExponents(long registryHandle, String expr);

    /**
     * Returns the base-SI scale factor of a unit expression.
     * E.g. "km" returns 1000.0, "cm" returns 0.01.
     * 
     * @param registryHandle The registry handle.
     * @param expr           The unit expression to scale.
     * @return The double scale factor relative to base-SI.
     */
    public static native double getUnitScale(long registryHandle, String expr);

    /**
     * Evaluates a phs text block or math expression and returns string results.
     * 
     * @param registryHandle The registry handle.
     * @param expr           The expression or statements (e.g. "a = 5 m; a * 2") to evaluate.
     * @return String representation of evaluation outputs.
     */
    public static native String evaluateExpression(long registryHandle, String expr);

    /**
     * Returns categories mapping (e.g. "length" -> ["m", "cm", "km", ...]).
     * 
     * @param registryHandle The registry handle.
     * @return A Map of category name to array of unit symbols.
     */
    public static native Map<String, String[]> getCategories(long registryHandle);

    // --- Quantity JNI math operations ---
    public static native Quantity addQuantities(Quantity a, Quantity b);
    public static native Quantity subQuantities(Quantity a, Quantity b);
    public static native Quantity mulQuantities(Quantity a, Quantity b);
    public static native Quantity divQuantities(Quantity a, Quantity b);
    public static native Quantity powQuantity(Quantity q, double power);
    public static native Quantity convertQuantity(Quantity q, String targetUnit);
    public static native String[] getFunctionParams(long registryHandle, String funcName);
}
