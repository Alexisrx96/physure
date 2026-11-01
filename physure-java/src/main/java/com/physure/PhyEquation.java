package com.physure;

import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * An equation in PHS source, such as {@code F = m * a}: it can be combined with another, have a variable
 * substituted, be solved for a variable and be evaluated once all but one of its variables are known.
 * Compatible with Java 8+.
 */
public class PhyEquation {
    private static final UnitRegistry DEFAULT_REGISTRY = new UnitRegistry();
    private static final Pattern IDENTIFIER = Pattern.compile("[A-Za-z_]\\w*");

    private final UnitRegistry registry;
    private final String expression;

    public PhyEquation(String expression) {
        this(DEFAULT_REGISTRY, expression);
    }

    public PhyEquation(UnitRegistry registry, String expression) {
        this.registry = registry != null ? registry : DEFAULT_REGISTRY;
        this.expression = withoutOuterQuotes(Objects.requireNonNull(expression, "expression").trim());
    }

    /** The text without a double quote at its start and one at its end. */
    private static String withoutOuterQuotes(String text) {
        int start = text.startsWith("\"") ? 1 : 0;
        int end = text.length() > start && text.endsWith("\"") ? text.length() - 1 : text.length();
        return text.substring(start, end);
    }

    public static PhyEquation of(String expression) {
        return new PhyEquation(expression);
    }

    public static PhyEquation of(UnitRegistry registry, String expression) {
        return new PhyEquation(registry, expression);
    }

    /**
     * The index of the equation's own {@code =}: the first one that is not part of {@code ==}, {@code <=},
     * {@code >=} or {@code !=} and not inside brackets or a string; -1 if there is none.
     */
    private int equationSign() {
        int depth = 0;
        boolean inString = false;
        int i = 0;
        while (i < expression.length()) {
            char c = expression.charAt(i);
            if (inString) {
                inString = c != '"';
            } else if (c == '"') {
                inString = true;
            } else if (depth == 0 && c == '=') {
                if (charAt(i + 1) == '=') {
                    i++; // `==` is a comparison, and so is its second sign
                } else if (!endsComparison(charAt(i - 1))) {
                    return i;
                }
            } else {
                depth += bracketDelta(c);
            }
            i++;
        }
        return -1;
    }

    /** The character of the expression at {@code i}, or a space outside it. */
    private char charAt(int i) {
        return i >= 0 && i < expression.length() ? expression.charAt(i) : ' ';
    }

    /** True for the characters that make a following {@code =} part of {@code <=}, {@code >=}, {@code !=} or {@code ==}. */
    private static boolean endsComparison(char before) {
        return before == '<' || before == '>' || before == '!' || before == '=';
    }

    private static int bracketDelta(char c) {
        if ("([{".indexOf(c) >= 0) {
            return 1;
        }
        return ")]}".indexOf(c) >= 0 ? -1 : 0;
    }

    public String getLhs() {
        int sign = equationSign();
        return sign < 0 ? expression.trim() : expression.substring(0, sign).trim();
    }

    public String getRhs() {
        int sign = equationSign();
        return sign < 0 ? "0" : expression.substring(sign + 1).trim();
    }

    public PhyEquation add(PhyEquation other) {
        return combined("+", other.getLhs(), other.getRhs());
    }

    public PhyEquation add(Object val) {
        String s = source(val);
        return combined("+", s, s);
    }

    public PhyEquation sub(PhyEquation other) {
        return combined("-", other.getLhs(), other.getRhs());
    }

    public PhyEquation sub(Object val) {
        String s = source(val);
        return combined("-", s, s);
    }

    public PhyEquation mul(PhyEquation other) {
        return combined("*", other.getLhs(), other.getRhs());
    }

    public PhyEquation mul(Object val) {
        String s = source(val);
        return combined("*", s, s);
    }

    public PhyEquation div(PhyEquation other) {
        return combined("/", other.getLhs(), other.getRhs());
    }

    public PhyEquation div(Object val) {
        String s = source(val);
        return combined("/", s, s);
    }

    /** Each side of this equation combined with the matching side of {@code otherLhs = otherRhs} by {@code operator}. */
    private PhyEquation combined(String operator, String otherLhs, String otherRhs) {
        return new PhyEquation(registry, combine(getLhs(), operator, otherLhs) + " = " + combine(getRhs(), operator, otherRhs));
    }

    private static String combine(String own, String operator, String other) {
        return "(" + own + ") " + operator + " (" + other + ")";
    }

    /** A value as PHS source: a quantity keeps its precision and its uncertainty, anything else is its text. */
    private static String source(Object val) {
        Objects.requireNonNull(val, "val");
        return val instanceof Quantity ? ((Quantity) val).toPhs() : val.toString();
    }

    public String getExpression() {
        return expression;
    }

    public PhyEquation solve(String variable) {
        String solvedExpr = registry.solve(expression, variable);
        if (!solvedExpr.contains("=")) {
            solvedExpr = variable + " = " + solvedExpr;
        }
        return new PhyEquation(registry, solvedExpr);
    }

    /**
     * Replaces every whole-word occurrence of {@code target} with {@code val}, which is taken as text:
     * {@code $} and {@code \} in it are characters like any other.
     */
    public PhyEquation substitute(String target, Object val) {
        String subVal = val instanceof PhyEquation ? "(" + ((PhyEquation) val).getRhs() + ")" : "(" + source(val) + ")";
        return new PhyEquation(registry, replaceName(getLhs(), target, subVal) + " = " + replaceName(getRhs(), target, subVal));
    }

    private static String replaceName(String text, String name, String replacement) {
        String regex = "\\b" + Pattern.quote(name) + "\\b";
        return text.replaceAll(regex, Matcher.quoteReplacement(replacement));
    }

    /**
     * Calls this equation by providing named arguments: every variable of the equation but one is given, and
     * the answer is the value of the one left, solved for.
     * E.g. eq.call("R", Quantity.of(10.0, "ohm"), "I", Quantity.of(2.0, "A")) for {@code V = R * I} -> Quantity(20.0, "V")
     * A quantity argument keeps its uncertainty.
     *
     * @throws IllegalArgumentException if the arguments are not name and value pairs, or the equation does
     *     not have exactly one variable that is not among them
     */
    public Quantity call(Object... args) {
        if (args.length % 2 != 0) {
            throw new IllegalArgumentException("Arguments must be key-value pairs (String name, Quantity or String value).");
        }
        Map<String, String> argMap = new LinkedHashMap<>();
        for (int i = 0; i < args.length; i += 2) {
            argMap.put(Objects.requireNonNull(args[i], "argument name").toString(), source(args[i + 1]));
        }
        return callMap(argMap);
    }

    /**
     * Calls this equation with a map of argument values (PHS source each): see {@link #call(Object...)}.
     */
    public Quantity callMap(Map<String, String> args) {
        String unknown = unknownVariable(args.keySet());
        String solved = registry.solve(expression, unknown);
        int sign = solved.indexOf('=');
        String result = sign < 0 ? solved.trim() : solved.substring(sign + 1).trim();
        // by substitution, not by assigning the arguments: assigning would leave them behind in the
        // registry's interpreter, where the next evaluation could read them
        for (Map.Entry<String, String> arg : args.entrySet()) {
            result = replaceName(result, arg.getKey(), "(" + arg.getValue() + ")");
        }
        return registry.evaluate(result);
    }

    /** The one variable of the equation that is neither a function nor among the arguments. */
    private String unknownVariable(Set<String> given) {
        Set<String> unknown = new LinkedHashSet<>();
        Set<String> functions = new HashSet<>();
        Matcher functionCalls = Pattern.compile("([A-Za-z_]\\w*)\\s*\\(").matcher(expression);
        while (functionCalls.find()) {
            functions.add(functionCalls.group(1));
        }
        Matcher identifiers = IDENTIFIER.matcher(expression);
        while (identifiers.find()) {
            String name = identifiers.group();
            boolean unitOfANumber = identifiers.start() > 0 && Character.isDigit(expression.charAt(identifiers.start() - 1));
            if (!given.contains(name) && !functions.contains(name) && !unitOfANumber) {
                unknown.add(name);
            }
        }
        if (unknown.size() != 1) {
            throw new IllegalArgumentException(
                "An equation is called with all its variables but one; in '" + expression + "' the ones not given are " + unknown);
        }
        return unknown.iterator().next();
    }

    @Override
    public String toString() {
        return "PhyEquation{\"" + expression + "\"}";
    }
}
