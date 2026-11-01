package com.physure;

import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

/**
 * A function of quantities: either defined in PHS inside a {@link UnitRegistry} (it can then be
 * differentiated, integrated, solved and combined symbolically) or written in Java (it can be called and
 * combined with other Java functions).
 * Compatible with Java 8+.
 */
public class PhyFunction {
    @FunctionalInterface
    public interface SingleVarFunc {
        Quantity apply(Quantity arg);
    }

    @FunctionalInterface
    public interface MultiVarFunc {
        Quantity apply(Quantity... args);
    }

    private final UnitRegistry registry;
    private final String name;
    private final SingleVarFunc singleLambda;
    private final MultiVarFunc multiLambda;

    public PhyFunction(String name, SingleVarFunc lambda) {
        this.registry = null;
        this.name = name;
        this.singleLambda = lambda;
        this.multiLambda = null;
    }

    public PhyFunction(String name, MultiVarFunc lambda) {
        this.registry = null;
        this.name = name;
        this.singleLambda = null;
        this.multiLambda = lambda;
    }

    /**
     * Defines a function in the registry's interpreter: {@code body} is its PHS definition,
     * e.g. {@code "area(w, h) = w * h"}.
     *
     * @throws PhysureException if the body does not parse or run
     */
    public PhyFunction(UnitRegistry registry, String name, String body) {
        this.registry = Objects.requireNonNull(registry, "registry");
        this.name = name;
        this.singleLambda = null;
        this.multiLambda = null;
        this.registry.evaluateRaw(body);
    }

    /**
     * Internal constructor for already-registered functions.
     */
    private PhyFunction(UnitRegistry registry, String name) {
        this.registry = registry;
        this.name = name;
        this.singleLambda = null;
        this.multiLambda = null;
    }

    private boolean isWrittenInJava() {
        return registry == null;
    }

    private UnitRegistry phsRegistry() {
        if (registry == null) {
            throw new IllegalStateException("'" + name + "' is a function written in Java: it has no PHS definition");
        }
        return registry;
    }

    /**
     * Gets the parameter names of this function.
     *
     * @throws IllegalStateException for a function written in Java
     */
    public String[] getParams() {
        return phsRegistry().getFunctionParams(name);
    }

    /**
     * Calls the function with the given arguments, each one PHS source.
     * E.g. func.call("10 kg", "5 m/s") -> Quantity(125.0, "J")
     */
    public Quantity call(String... args) {
        List<String> sources = new ArrayList<>();
        for (String arg : args) {
            sources.add(Objects.requireNonNull(arg, "argument"));
        }
        return phsRegistry().evaluate(callSource(sources));
    }

    /**
     * Calls the function with strongly-typed Quantity arguments and returns a Quantity result.
     * E.g. func.call(new Quantity(10.0, "kg"), new Quantity(5.0, "m/s")) -> Quantity(125.0, "J")
     * The arguments cross as they are, uncertainty included.
     *
     * @throws IllegalArgumentException if a function written in Java is called with the wrong number of arguments
     */
    public Quantity call(Quantity... args) {
        if (singleLambda != null) {
            if (args.length != 1) {
                throw new IllegalArgumentException("'" + name + "' takes one argument, got " + args.length);
            }
            return singleLambda.apply(args[0]);
        }
        if (multiLambda != null) {
            return multiLambda.apply(args);
        }
        if (registry == null) {
            throw new IllegalStateException("'" + name + "' has no function behind it");
        }
        List<String> sources = new ArrayList<>();
        for (Quantity arg : args) {
            sources.add(Objects.requireNonNull(arg, "argument").toPhs());
        }
        return registry.evaluate(callSource(sources));
    }

    private String callSource(List<String> arguments) {
        return name + "(" + String.join(", ", arguments) + ")";
    }

    /**
     * Returns a new PhyFunction representing the symbolic derivative of this function with respect to var.
     */
    public PhyFunction deriv(String var) {
        String[] params = getParams();
        if (params == null || params.length == 0) {
            throw new IllegalStateException("Cannot differentiate a function with no parameters.");
        }

        String paramsJoined = String.join(", ", params);
        String callExpr = name + "(" + paramsJoined + ")";
        String derivResult = registry.deriv(callExpr, var);

        String newName = "d_" + name + "_d_" + var;
        String newBody = newName + "(" + paramsJoined + ") = " + derivResult;

        return new PhyFunction(registry, newName, newBody);
    }

    /**
     * Returns a new PhyFunction representing the symbolic integral of this function with respect to var.
     */
    public PhyFunction integral(String var) {
        String[] params = getParams();
        if (params == null || params.length == 0) {
            throw new IllegalStateException("Cannot integrate a function with no parameters.");
        }

        String paramsJoined = String.join(", ", params);
        String callExpr = name + "(" + paramsJoined + ")";
        String integralResult = registry.integral(callExpr, var);

        String newName = "int_" + name + "_d_" + var;
        String newBody = newName + "(" + paramsJoined + ") = " + integralResult;

        return new PhyFunction(registry, newName, newBody);
    }

    /**
     * Solves this function equation for a target variable, returning a new PhyFunction representing the solved formula.
     * E.g. solve("m") for kinetic_energy(m, v) generates solved function: solve_kinetic_energy_for_m(target, v) = target / (0.5 * v^2)
     */
    public PhyFunction solve(String var) {
        String[] params = getParams();
        if (params == null || params.length == 0) {
            throw new IllegalStateException("Cannot solve a function with no parameters.");
        }

        String paramsJoined = String.join(", ", params);
        String callExpr = name + "(" + paramsJoined + ")";
        String targetName = "target";

        // Solve the equation: kinetic_energy(m, v) = target
        String solveResult = registry.solve(callExpr + " = " + targetName, var);
        // the answer is the equation `var = ...`: the body of the new function is its right-hand side
        int equalsSign = solveResult.indexOf('=');
        if (equalsSign >= 0) {
            solveResult = solveResult.substring(equalsSign + 1).trim();
        }

        // New parameters are: target, plus all original parameters except the one solved for
        List<String> newParamsList = new ArrayList<>();
        newParamsList.add(targetName);
        for (String p : params) {
            if (!p.equals(var)) {
                newParamsList.add(p);
            }
        }
        String newParamsJoined = String.join(", ", newParamsList);

        String newName = "solve_" + name + "_for_" + var;
        String newBody = newName + "(" + newParamsJoined + ") = " + solveResult;

        return new PhyFunction(registry, newName, newBody);
    }

    /**
     * Returns a new PhyFunction representing the sum of this and other: (this + other)(x) = this(x) + other(x)
     * Both functions are defined in the same registry, or both are single-variable functions written in Java.
     *
     * @throws IllegalArgumentException if the functions belong to different registries
     * @throws UnsupportedOperationException if the two are not of the same kind
     */
    public PhyFunction add(PhyFunction other) {
        return combine(other, "+", "add", Quantity::add);
    }

    /** this(x) - other(x); see {@link #add(PhyFunction)}. */
    public PhyFunction subtract(PhyFunction other) {
        return combine(other, "-", "subtract", Quantity::subtract);
    }

    /** this(x) * other(x); see {@link #add(PhyFunction)}. */
    public PhyFunction multiply(PhyFunction other) {
        return combine(other, "*", "multiply", Quantity::multiply);
    }

    /** this(x) / other(x); see {@link #add(PhyFunction)}. */
    public PhyFunction divide(PhyFunction other) {
        return combine(other, "/", "divide", Quantity::divide);
    }

    /**
     * The composition {@code this(inner(x))}; see {@link #add(PhyFunction)} for which functions combine.
     * For functions defined in PHS the inner function feeds the first parameter of this one.
     */
    public PhyFunction compose(PhyFunction inner) {
        Objects.requireNonNull(inner, "inner");
        if (!sameKind(inner)) {
            throw new UnsupportedOperationException(
                "A function written in Java and one defined in PHS cannot be combined: " + name + ", " + inner.name);
        }
        if (isWrittenInJava()) {
            requireSingleVariable(inner);
            return new PhyFunction(this.name + "_compose_" + inner.name,
                (SingleVarFunc) v -> this.singleLambda.apply(inner.singleLambda.apply(v)));
        }
        requireSameRegistry(inner);
        String[] outerParams = getParams();
        String[] innerParams = inner.getParams();
        if (outerParams.length == 0) {
            throw new IllegalStateException("The outer function must have at least one parameter.");
        }
        List<String> combined = new ArrayList<>(java.util.Arrays.asList(innerParams));
        List<String> outerArgs = new ArrayList<>();
        outerArgs.add(inner.name + "(" + String.join(", ", innerParams) + ")");
        for (int i = 1; i < outerParams.length; i++) {
            outerArgs.add(outerParams[i]);
            if (!combined.contains(outerParams[i])) {
                combined.add(outerParams[i]);
            }
        }
        String newName = "compose_" + this.name + "_" + inner.name;
        String body = newName + "(" + String.join(", ", combined) + ") = " + this.name + "(" + String.join(", ", outerArgs) + ")";
        return new PhyFunction(registry, newName, body);
    }

    private interface JavaOperation {
        Quantity apply(Quantity a, Quantity b);
    }

    private PhyFunction combine(PhyFunction other, String symbol, String opName, JavaOperation javaOperation) {
        Objects.requireNonNull(other, "other");
        if (!sameKind(other)) {
            throw new UnsupportedOperationException(
                "A function written in Java and one defined in PHS cannot be combined: " + name + ", " + other.name);
        }
        if (isWrittenInJava()) {
            requireSingleVariable(other);
            return new PhyFunction(this.name + "_" + opName + "_" + other.name,
                (SingleVarFunc) v -> javaOperation.apply(this.singleLambda.apply(v), other.singleLambda.apply(v)));
        }
        return binaryOp(other, symbol, opName);
    }

    private boolean sameKind(PhyFunction other) {
        return isWrittenInJava() == other.isWrittenInJava();
    }

    private void requireSingleVariable(PhyFunction other) {
        if (this.singleLambda == null || other.singleLambda == null) {
            throw new UnsupportedOperationException(
                "Only functions of one variable written in Java can be combined: " + name + ", " + other.name);
        }
    }

    private void requireSameRegistry(PhyFunction other) {
        if (this.registry != other.registry) {
            throw new IllegalArgumentException("Functions must share the same UnitRegistry context.");
        }
    }

    private PhyFunction binaryOp(PhyFunction other, String opSymbol, String opName) {
        requireSameRegistry(other);
        String[] params1 = getParams();
        String[] params2 = other.getParams();
        List<String> combined = combineParams(params1, params2);
        String combinedParamsJoined = String.join(", ", combined);

        String newName = opName + "_" + this.name + "_" + other.name;
        String body = newName + "(" + combinedParamsJoined + ") = " +
                      this.name + "(" + String.join(", ", params1) + ") " + opSymbol + " " +
                      other.name + "(" + String.join(", ", params2) + ")";

        return new PhyFunction(this.registry, newName, body);
    }

    private List<String> combineParams(String[] p1, String[] p2) {
        List<String> combined = new ArrayList<>();
        if (p1 != null) {
            for (String p : p1) {
                combined.add(p);
            }
        }
        if (p2 != null) {
            for (String p : p2) {
                if (!combined.contains(p)) {
                    combined.add(p);
                }
            }
        }
        return combined;
    }

    public String getName() {
        return name;
    }
}
