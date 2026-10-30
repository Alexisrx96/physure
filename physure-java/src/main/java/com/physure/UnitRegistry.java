package com.physure;

import java.util.Map;
import java.util.Objects;

/**
 * High-level Java client wrapper for the Rust UnitRegistry.
 * Implements AutoCloseable to ensure native memory is freed.
 * <p>
 * A registry is safe to share between threads: every call holds the registry's lock, because the PHS
 * interpreter behind {@link #evaluate(String)} is one mutable native object and two threads inside it at
 * once corrupt the native heap. {@link #close()} takes the same lock, so it cannot free the handle under a
 * call in progress. Separate registries do not share state and do not wait for each other.
 * Compatible with Java 8+.
 */
public class UnitRegistry implements AutoCloseable {
    private final long handle;
    private boolean closed = false;

    public UnitRegistry() {
        this.handle = NativeEngine.initRegistry();
    }

    /**
     * A registry over the embedded catalog extended by the definitions in a conf file.
     *
     * @throws PhysureException if the file cannot be read or a line of it is not a definition
     */
    public UnitRegistry(String configFilePath) {
        this.handle = NativeEngine.initRegistryFromPath(Objects.requireNonNull(configFilePath, "configFilePath"));
    }

    /**
     * A registry over the embedded catalog extended by the definitions in {@code configContent}.
     *
     * @throws PhysureException if a line of it is not a definition
     */
    public static UnitRegistry fromContent(String configContent) {
        long handle = NativeEngine.initRegistryFromContent(Objects.requireNonNull(configContent, "configContent"));
        return new UnitRegistry(handle);
    }

    private UnitRegistry(long handle) {
        this.handle = handle;
    }

    /** The native handle. It is only valid until {@link #close()}, and native calls made with it bypass the lock. */
    public long getHandle() {
        return handle;
    }

    /**
     * Parse and get the exponent map of a unit expression, as whole numbers.
     *
     * @throws PhysureException if the unit has a fractional power ({@code s^0.5}); the whole number
     *     would be a wrong answer, so use {@link #getUnitPowers(String)} for those.
     */
    public synchronized Map<String, Integer> getUnitExponents(String expr) {
        Objects.requireNonNull(expr, "expr");
        checkClosed();
        return NativeEngine.getUnitExponents(handle, expr);
    }

    /**
     * Parse and get the exponent map of a unit expression with its exact powers
     * ({@code s^0.5} is {@code {s=0.5}}), as the WASM binding returns them.
     */
    public synchronized Map<String, Double> getUnitPowers(String expr) {
        Objects.requireNonNull(expr, "expr");
        checkClosed();
        return NativeEngine.getUnitPowers(handle, expr);
    }

    /**
     * Get the scale factor relative to base-SI of a unit expression.
     */
    public synchronized double getUnitScale(String expr) {
        Objects.requireNonNull(expr, "expr");
        checkClosed();
        return NativeEngine.getUnitScale(handle, expr);
    }

    /**
     * Evaluates PHS and returns the value of its last statement, with its uncertainty:
     * {@code "a = 15 m; b = 5 m; a * b"} is 75 m^2.
     *
     * @throws PhysureException if the source does not parse or run, or its last statement is not a
     *     quantity (a string, or nothing)
     */
    public synchronized Quantity evaluate(String expr) {
        Objects.requireNonNull(expr, "expr");
        checkClosed();
        return NativeEngine.evaluateQuantity(handle, expr);
    }

    /**
     * Evaluates a math or PHS text statement block and returns the raw string output.
     */
    public synchronized String evaluateRaw(String expr) {
        Objects.requireNonNull(expr, "expr");
        checkClosed();
        return NativeEngine.evaluateExpression(handle, expr);
    }

    /**
     * Gets all unit categories and their configured units.
     */
    public synchronized Map<String, String[]> getCategories() {
        checkClosed();
        return NativeEngine.getCategories(handle);
    }

    /**
     * The parameter names of a function defined in this registry's interpreter.
     *
     * @throws PhysureException if there is no such function
     */
    synchronized String[] getFunctionParams(String functionName) {
        Objects.requireNonNull(functionName, "functionName");
        checkClosed();
        return NativeEngine.getFunctionParams(handle, functionName);
    }

    /**
     * Solves a symbolic equation for a given variable.
     * E.g. solve("P * V = n * R * T", "T")
     *
     * @throws IllegalArgumentException if an argument contains a double quote: the arguments are put into a
     *     PHS string, and a quote would end it and make the rest a statement of its own
     */
    public synchronized String solve(String equation, String variable) {
        return evaluateRaw("solve(\"" + text(equation, "equation") + "\", \"" + text(variable, "variable") + "\")");
    }

    /**
     * Calculates the symbolic derivative of an expression with respect to a variable.
     * E.g. deriv("v0 * t + 0.5 * a * t^2", "t")
     *
     * @throws IllegalArgumentException if an argument contains a double quote
     */
    public synchronized String deriv(String expression, String variable) {
        return evaluateRaw("deriv(\"" + text(expression, "expression") + "\", \"" + text(variable, "variable") + "\")");
    }

    /**
     * Calculates the symbolic integral of an expression with respect to a variable.
     * E.g. integral("3 * t^2", "t")
     *
     * @throws IllegalArgumentException if an argument contains a double quote
     */
    public synchronized String integral(String expression, String variable) {
        return evaluateRaw("integral(\"" + text(expression, "expression") + "\", \"" + text(variable, "variable") + "\")");
    }

    private static String text(String argument, String name) {
        Objects.requireNonNull(argument, name);
        if (argument.indexOf('"') >= 0) {
            throw new IllegalArgumentException(name + " must not contain a double quote: " + argument);
        }
        return argument;
    }

    @Override
    public synchronized void close() {
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
