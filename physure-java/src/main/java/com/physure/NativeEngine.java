package com.physure;

import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Map;

/**
 * Native JNI wrapper for the physure core physics engine.
 * Compatible with Java 8+.
 */
public class NativeEngine {
    private NativeEngine() {
    }

    static {
        try {
            // Attempt to load standard system library
            System.loadLibrary("physure_java");
        } catch (UnsatisfiedLinkError e) {
            // Fallback: extract and load dynamic library from classpath resource (for jar packaging)
            try {
                loadFromJar();
            } catch (Exception ex) {
                throw new IllegalStateException("Failed to load native library physure_java", ex);
            }
        }
    }

    private static void loadFromJar() throws Exception {
        String libName = System.mapLibraryName("physure_java");
        String resourcePath = NativeLibraryLocator.resourcePath(
            System.getProperty("os.name"), System.getProperty("os.arch"), libName);

        try (InputStream in = NativeEngine.class.getResourceAsStream(resourcePath)) {
            if (in == null) {
                throw new UnsatisfiedLinkError("Native library " + libName + " not found at " + resourcePath);
            }
            // NIO creates the file readable and writable by its owner only, which the shared temp directory needs
            Path temp = Files.createTempFile("libphysure_java-", "-" + libName);
            temp.toFile().deleteOnExit();
            Files.copy(in, temp, StandardCopyOption.REPLACE_EXISTING);
            System.load(temp.toAbsolutePath().toString());
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
    public static native Map<String, Double> getUnitPowers(long registryHandle, String expr);

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
    public static native Quantity evaluateQuantity(long registryHandle, String expr);
    /**
     * Like {@link #evaluateQuantity}, with {@code values} visible under {@code names} for this call only: bound
     * as they are, with the sources of their uncertainty, and not written into the source as text.
     */
    static native Quantity evaluateQuantityWith(long registryHandle, String expr, String[] names, Quantity[] values);

    /**
     * Returns categories mapping (e.g. "length" -> ["m", "cm", "km", ...]).
     * 
     * @param registryHandle The registry handle.
     * @return A Map of category name to array of unit symbols.
     */
    public static native Map<String, String[]> getCategories(long registryHandle);

    /**
     * A fresh source id from the core's one counter (the same one PHS and Python mint theirs from), for the first
     * use of a quantity constructed in Java: two measurements never share an id by accident.
     */
    static native long newSourceId();

    // --- Quantity JNI math operations ---
    public static native Quantity addQuantities(Quantity a, Quantity b);
    public static native Quantity subQuantities(Quantity a, Quantity b);
    public static native Quantity mulQuantities(Quantity a, Quantity b);
    public static native Quantity divQuantities(Quantity a, Quantity b);
    public static native Quantity powQuantity(Quantity q, double power);
    /** The core's {@code sin}, {@code cos}, {@code tan} or {@code abs} of a quantity, by name. */
    static native Quantity mathFunction(Quantity q, String name);
    public static native Quantity convertQuantity(Quantity q, String targetUnit);
    /** Declares {@code nu} degrees of freedom for the one source of {@code q}; the core refuses what cannot carry them. */
    static native void declareDof(Quantity q, double nu);
    static native double effectiveDof(Quantity q);
    /** {@code {k, U, nu_eff}} of {@code q} for the coverage probability {@code p}. */
    static native double[] expanded(Quantity q, double p);
    /** A Type A evaluation: the mean of the observations, {@code s / sqrt(n)} and {@code n - 1} degrees of freedom. */
    static native Quantity fromSamples(double[] samples, String unit);
    /** {@code {mean, uncertainty}} as text, rounded by the Guide's rule. */
    static native String[] gumRound(double mean, double uncertainty);
    public static native void assertQuantities(Quantity a, Quantity b);
    public static native void assertExactQuantities(Quantity a, Quantity b);
    public static native String[] getFunctionParams(long registryHandle, String funcName);
}
