package com.physure;

import java.util.HashMap;
import java.util.Map;

/**
 * Represents a physical equation (e.g. "V = R * I").
 * Callable by default: when called with known variables, dynamically solves and evaluates the unknown.
 * Compatible with Java 8+.
 */
public class PhyEquation {
    private static final UnitRegistry DEFAULT_REGISTRY = new UnitRegistry();

    private final UnitRegistry registry;
    private final String expression;

    public PhyEquation(String expression) {
        this(DEFAULT_REGISTRY, expression);
    }

    public PhyEquation(UnitRegistry registry, String expression) {
        this.registry = registry != null ? registry : DEFAULT_REGISTRY;
        this.expression = expression.trim().replaceAll("^\"|\"$", "");
    }

    public static PhyEquation of(String expression) {
        return new PhyEquation(expression);
    }

    public static PhyEquation of(UnitRegistry registry, String expression) {
        return new PhyEquation(registry, expression);
    }

    public String getLhs() {
        if (expression.contains("=")) {
            return expression.split("=", 2)[0].trim();
        }
        return expression.trim();
    }

    public String getRhs() {
        if (expression.contains("=")) {
            return expression.split("=", 2)[1].trim();
        }
        return "0";
    }

    public PhyEquation add(PhyEquation other) {
        return new PhyEquation(registry, "(" + getLhs() + ") + (" + other.getLhs() + ") = (" + getRhs() + ") + (" + other.getRhs() + ")");
    }

    public PhyEquation add(Object val) {
        String s = val instanceof Quantity ? ((Quantity) val).getValue() + " " + ((Quantity) val).getUnit() : val.toString();
        return new PhyEquation(registry, "(" + getLhs() + ") + (" + s + ") = (" + getRhs() + ") + (" + s + ")");
    }

    public PhyEquation sub(PhyEquation other) {
        return new PhyEquation(registry, "(" + getLhs() + ") - (" + other.getLhs() + ") = (" + getRhs() + ") - (" + other.getRhs() + ")");
    }

    public PhyEquation sub(Object val) {
        String s = val instanceof Quantity ? ((Quantity) val).getValue() + " " + ((Quantity) val).getUnit() : val.toString();
        return new PhyEquation(registry, "(" + getLhs() + ") - (" + s + ") = (" + getRhs() + ") - (" + s + ")");
    }

    public PhyEquation mul(PhyEquation other) {
        return new PhyEquation(registry, "(" + getLhs() + ") * (" + other.getLhs() + ") = (" + getRhs() + ") * (" + other.getRhs() + ")");
    }

    public PhyEquation mul(Object val) {
        String s = val instanceof Quantity ? ((Quantity) val).getValue() + " " + ((Quantity) val).getUnit() : val.toString();
        return new PhyEquation(registry, "(" + getLhs() + ") * (" + s + ") = (" + getRhs() + ") * (" + s + ")");
    }

    public PhyEquation div(PhyEquation other) {
        return new PhyEquation(registry, "(" + getLhs() + ") / (" + other.getLhs() + ") = (" + getRhs() + ") / (" + other.getRhs() + ")");
    }

    public PhyEquation div(Object val) {
        String s = val instanceof Quantity ? ((Quantity) val).getValue() + " " + ((Quantity) val).getUnit() : val.toString();
        return new PhyEquation(registry, "(" + getLhs() + ") / (" + s + ") = (" + getRhs() + ") / (" + s + ")");
    }

    public String getExpression() {
        return expression;
    }

    public PhyEquation solve(String var) {
        String solvedExpr = registry.solve(expression, var);
        if (!solvedExpr.contains("=")) {
            solvedExpr = var + " = " + solvedExpr;
        }
        return new PhyEquation(registry, solvedExpr);
    }

    public PhyEquation substitute(String target, Object val) {
        String subVal = val instanceof PhyEquation ? "(" + ((PhyEquation) val).getRhs() + ")" : "(" + val.toString() + ")";
        String regex = "\\b" + java.util.regex.Pattern.quote(target) + "\\b";
        String newLhs = getLhs().replaceAll(regex, subVal);
        String newRhs = getRhs().replaceAll(regex, subVal);
        return new PhyEquation(registry, newLhs + " = " + newRhs);
    }

    /**
     * Calls this equation by providing named arguments.
     * E.g. eq.call("R", Quantity.of(10.0, "ohm"), "I", Quantity.of(2.0, "A")) -> Quantity(20.0, "V")
     */
    public Quantity call(Object... args) {
        if (args.length % 2 != 0) {
            throw new IllegalArgumentException("Arguments must be key-value pairs (String name, Quantity or String value).");
        }
        Map<String, String> argMap = new HashMap<>();
        for (int i = 0; i < args.length; i += 2) {
            String name = args[i].toString();
            Object val = args[i + 1];
            if (val instanceof Quantity) {
                Quantity q = (Quantity) val;
                argMap.put(name, q.getValue() + " " + q.getUnit());
            } else {
                argMap.put(name, val.toString());
            }
        }
        return callMap(argMap);
    }

    /**
     * Calls this equation with a map of argument values.
     */
    public Quantity callMap(Map<String, String> args) {
        StringBuilder sb = new StringBuilder();
        sb.append("use solve from calc\n");
        sb.append("eq_temp = \"").append(expression).append("\"\n");
        sb.append("solve_fn = solve(eq_temp)\n");
        sb.append("solve_fn(");
        int i = 0;
        for (Map.Entry<String, String> entry : args.entrySet()) {
            if (i > 0) sb.append(", ");
            sb.append(entry.getKey()).append(" = ").append(entry.getValue());
            i++;
        }
        sb.append(")\n");
        String evalResult = registry.evaluateRaw(sb.toString()).trim();
        return Quantity.parse(evalResult);
    }

    @Override
    public String toString() {
        return "PhyEquation{\"" + expression + "\"}";
    }
}
