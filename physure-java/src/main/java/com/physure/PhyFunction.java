package com.physure;

import java.util.ArrayList;
import java.util.List;

/**
 * Represents a physical or mathematical function registered in the Physure context.
 * Delegates function definition and execution statefully to the Rust engine.
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

    public PhyFunction(UnitRegistry registry, String name, String body) {
        this.registry = registry;
        this.name = name;
        this.singleLambda = null;
        this.multiLambda = null;
        this.registry.evaluate(body);
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

    /**
     * Gets the parameter names of this function.
     */
    public String[] getParams() {
        return NativeEngine.getFunctionParams(registry.getHandle(), name);
    }

    /**
     * Calls the function with the given arguments.
     * E.g. func.call("10 kg", "5 m/s") -> Quantity(125.0, "J")
     */
    public Quantity call(String... args) {
        StringBuilder sb = new StringBuilder();
        sb.append(name).append("(");
        for (int i = 0; i < args.length; i++) {
            if (i > 0) {
                sb.append(", ");
            }
            sb.append(args[i]);
        }
        sb.append(")");
        String resultStr = registry.evaluateRaw(sb.toString()).trim();
        return Quantity.parse(resultStr);
    }

    /**
     * Calls the function with strongly-typed Quantity arguments and returns a Quantity result.
     * E.g. func.call(new Quantity(10.0, "kg"), new Quantity(5.0, "m/s")) -> Quantity(125.0, "J")
     */
    public Quantity call(Quantity... args) {
        if (singleLambda != null && args.length > 0) {
            return singleLambda.apply(args[0]);
        }
        if (multiLambda != null) {
            return multiLambda.apply(args);
        }
        if (registry != null) {
            StringBuilder sb = new StringBuilder();
            sb.append(name).append("(");
            for (int i = 0; i < args.length; i++) {
                if (i > 0) {
                    sb.append(", ");
                }
                sb.append(args[i].getValue()).append(" ").append(args[i].getUnit());
            }
            sb.append(")");
            String resultStr = registry.evaluateRaw(sb.toString()).trim();
            return Quantity.parse(resultStr);
        }
        return new Quantity(0.0, "");
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
     */
    public PhyFunction add(PhyFunction other) {
        if (this.singleLambda != null && other.singleLambda != null) {
            return new PhyFunction(this.name + "_add_" + other.name, (SingleVarFunc) (v) -> this.singleLambda.apply(v).add(other.singleLambda.apply(v)));
        }
        return this;
    }

    public PhyFunction subtract(PhyFunction other) {
        if (this.singleLambda != null && other.singleLambda != null) {
            return new PhyFunction(this.name + "_sub_" + other.name, (SingleVarFunc) (v) -> this.singleLambda.apply(v).subtract(other.singleLambda.apply(v)));
        }
        return this;
    }

    public PhyFunction multiply(PhyFunction other) {
        if (this.singleLambda != null && other.singleLambda != null) {
            return new PhyFunction(this.name + "_mul_" + other.name, (SingleVarFunc) (v) -> this.singleLambda.apply(v).multiply(other.singleLambda.apply(v)));
        }
        return this;
    }

    public PhyFunction divide(PhyFunction other) {
        if (this.singleLambda != null && other.singleLambda != null) {
            return new PhyFunction(this.name + "_div_" + other.name, (SingleVarFunc) (v) -> this.singleLambda.apply(v).divide(other.singleLambda.apply(v)));
        }
        return this;
    }

    public PhyFunction compose(PhyFunction inner) {
        if (this.singleLambda != null && inner.singleLambda != null) {
            return new PhyFunction(this.name + "_compose_" + inner.name, (SingleVarFunc) (v) -> this.singleLambda.apply(inner.singleLambda.apply(v)));
        }
        return this;
    }

    private PhyFunction binaryOp(PhyFunction other, String opSymbol, String opName) {
        if (this.registry != other.registry) {
            throw new IllegalArgumentException("Functions must share the same UnitRegistry context.");
        }
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
