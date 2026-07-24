package com.physure;

import java.util.Map;

/**
 * High-level Java client wrapper for the Rust UnitRegistry.
 * Implements AutoCloseable to ensure native memory is freed.
 * Compatible with Java 8+.
 */
public class UnitRegistry implements AutoCloseable {
    private final long handle;
    private boolean closed = false;

    public UnitRegistry() {
        this.handle = NativeEngine.initRegistry();
    }

    public UnitRegistry(String configFilePath) {
        this.handle = NativeEngine.initRegistryFromPath(configFilePath);
    }

    public static UnitRegistry fromContent(String configContent) {
        long handle = NativeEngine.initRegistryFromContent(configContent);
        return new UnitRegistry(handle);
    }

    private UnitRegistry(long handle) {
        this.handle = handle;
    }

    public long getHandle() {
        return handle;
    }

    /**
     * Parse and get the exponent map of a unit expression.
     */
    public Map<String, Integer> getUnitExponents(String expr) {
        checkClosed();
        return NativeEngine.getUnitExponents(handle, expr);
    }

    /**
     * Get the scale factor relative to base-SI of a unit expression.
     */
    public double getUnitScale(String expr) {
        checkClosed();
        return NativeEngine.getUnitScale(handle, expr);
    }

    public Quantity evaluate(String expr) {
        checkClosed();
        String res = NativeEngine.evaluateExpression(handle, expr);
        return Quantity.parse(res);
    }

    /**
     * Evaluates a math or PHS text statement block and returns the raw string output.
     */
    public String evaluateRaw(String expr) {
        checkClosed();
        return NativeEngine.evaluateExpression(handle, expr);
    }

    /**
     * Gets all unit categories and their configured units.
     */
    public Map<String, String[]> getCategories() {
        checkClosed();
        return NativeEngine.getCategories(handle);
    }

    /**
     * Solves a symbolic equation for a given variable.
     * E.g. solve("P * V = n * R * T", "T")
     */
    public String solve(String equation, String variable) {
        checkClosed();
        return evaluateRaw("solve(\"" + equation + "\", \"" + variable + "\")");
    }

    /**
     * Calculates the symbolic derivative of an expression with respect to a variable.
     * E.g. deriv("v0 * t + 0.5 * a * t^2", "t")
     */
    public String deriv(String expression, String variable) {
        checkClosed();
        return evaluateRaw("deriv(\"" + expression + "\", \"" + variable + "\")");
    }

    /**
     * Calculates the symbolic integral of an expression with respect to a variable.
     * E.g. integral("3 * t^2", "t")
     */
    public String integral(String expression, String variable) {
        checkClosed();
        return evaluateRaw("integral(\"" + expression + "\", \"" + variable + "\")");
    }

    @Override
    public void close() {
        if (!closed) {
            NativeEngine.destroyRegistry(handle);
            closed = true;
        }
    }

    private void checkClosed() {
        if (closed) {
            throw new IllegalStateException("UnitRegistry has been closed.");
        }
    }
}
