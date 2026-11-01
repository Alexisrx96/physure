package com.physure;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;

/** Strict tests of {@link PhyFunction} and {@link PhyEquation}: definitions, calls and the algebra of functions. */
class FunctionStrictTest {
    private static final String A_IS_B = "a = b";
    private static final String A_AT_MOST_C = "a <= c";
    private static final String Y_IS_X_PLUS_ONE = "y = x + 1";
    private static final String FORCE_LAW = "F = m * a";
    private static final String ACCELERATION = "m/s^2";
    private static UnitRegistry registry;

    @BeforeAll
    static void open() {
        registry = new UnitRegistry();
    }

    @AfterAll
    static void close() {
        registry.close();
    }

    private static Quantity q(double value, String unit) {
        return new Quantity(value, unit);
    }

    private static void near(double expected, Quantity actual) {
        assertEquals(expected, actual.getValue(), 1e-9 * Math.max(1, Math.abs(expected)));
    }

    private static PhyFunction area() {
        return new PhyFunction(registry, "area", "area(w, h) = w * h");
    }

    // ---- PhyFunction defined in PHS

    @Test
    void aFunctionReportsItsNameAndParametersAndCallsWithTextArguments() {
        PhyFunction f = area();
        assertEquals("area", f.getName());
        assertArrayEquals(new String[] {"w", "h"}, f.getParams());
        Quantity result = f.call("2 m", "3 m");
        near(6, result);
        assertEquals("m^2", result.getUnit());
    }

    @Test
    void aFunctionCalledWithQuantitiesLosesNeitherPrecisionNorUncertainty() {
        PhyFunction f = area();
        near(2.0 * 1.123456789123, f.call(q(1.123456789123, "m"), q(2, "m")));
        Quantity measured = f.call(new Quantity(2, 0.1, "m"), q(3, "m"));
        near(6, measured);
        assertEquals(6 * (0.1 / 2), measured.getUncertainty(), 1e-9);
        // a value written in exponent form, and in a different scale
        near(1e-5 * 2000, f.call(q(1e-5, "m"), q(2, "km")));
    }

    @Test
    void aCallWithTheWrongNumberOrKindOfArgumentIsAnError() {
        PhyFunction f = area();
        assertThrows(PhysureException.class, () -> f.call("2 m"));
        assertThrows(PhysureException.class, () -> f.call("2 m", "3 m", "4 m"));
        assertThrows(PhysureException.class, () -> f.call(q(Double.NaN, "m"), q(1, "m")));
        PhyFunction sum = new PhyFunction(registry, "plus", "plus(a, b) = a + b");
        assertThrows(PhysureException.class, () -> sum.call("2 m", "3 s"));
    }

    @Test
    void anArgumentCannotInjectAnotherCall() {
        assertThrows(PhysureException.class, () -> area().call("1 m); 1/0; area(1 m", "1 m"));
    }

    @Test
    void derivativesIntegralsAndSolutionsAreFunctionsThatCompute() {
        PhyFunction kinetic = new PhyFunction(registry, "kinetic", "kinetic(m, v) = 0.5 * m * v^2");
        near(6, kinetic.deriv("v").call("2 kg", "3 m/s"));
        near(2, kinetic.solve("m").call("9 J", "3 m/s"));
        PhyFunction line = new PhyFunction(registry, "line", "line(t) = 3 * t^2");
        near(8, line.integral("t").call("2"));
    }

    @Test
    void combiningFunctionsGivesFunctionsWithTheCombinedValues() {
        PhyFunction f = new PhyFunction(registry, "fa", "fa(x) = x * 2");
        PhyFunction g = new PhyFunction(registry, "ga", "ga(x) = x + 1 m");
        near(10, f.add(g).call("3 m"));
        near(2, f.subtract(g).call("3 m"));
        near(24, f.multiply(g).call("3 m"));
        near(1.5, f.divide(g).call("3 m"));
        near(8, f.compose(g).call("3 m"));
        near(7, g.compose(f).call("3 m"));
    }

    @Test
    void functionsOfTwoRegistriesDoNotCombine() {
        try (UnitRegistry other = new UnitRegistry()) {
            PhyFunction f = new PhyFunction(registry, "fa", "fa(x) = x");
            PhyFunction g = new PhyFunction(other, "ga", "ga(x) = x");
            assertThrows(IllegalArgumentException.class, () -> f.add(g));
            assertThrows(IllegalArgumentException.class, () -> f.compose(g));
        }
    }

    @Test
    void aBodyThatIsNotADefinitionIsAnError() {
        assertThrows(PhysureException.class, () -> new PhyFunction(registry, "bad", "1 +"));
        assertThrows(PhysureException.class, () -> new PhyFunction(registry, "none", "").getParams());
        assertThrows(PhysureException.class, () -> new PhyFunction(registry, "nosuch", "x = 1").getParams());
    }

    // ---- PhyFunction written in Java

    @Test
    void aFunctionWrittenInJavaCallsAndCombinesWithOthersLikeIt() {
        PhyFunction square = new PhyFunction("sq", (PhyFunction.SingleVarFunc) x -> x.mul(x));
        PhyFunction twice = new PhyFunction("tw", (PhyFunction.SingleVarFunc) x -> x.multiply(2.0));
        PhyFunction twoMetres = new PhyFunction("tm", (PhyFunction.SingleVarFunc) x -> x.mul(q(2, "m")));
        near(9, square.call(q(3, "m")));
        near(15, square.add(twoMetres).call(q(3, "m")));
        near(3, square.subtract(twoMetres).call(q(3, "m")));
        near(54, square.multiply(twoMetres).call(q(3, "m")));
        near(1.5, square.divide(twoMetres).call(q(3, "m")));
        near(36, square.compose(twice).call(q(3, "m")));
    }

    @Test
    void combiningAFunctionWrittenInJavaWithOneDefinedInPhsIsRefusedNotIgnored() {
        PhyFunction java = new PhyFunction("sq", (PhyFunction.SingleVarFunc) x -> x);
        PhyFunction phs = new PhyFunction(registry, "fp", "fp(x) = x");
        assertThrows(UnsupportedOperationException.class, () -> java.add(phs));
        assertThrows(UnsupportedOperationException.class, () -> phs.add(java));
    }

    @Test
    void aFunctionWrittenInJavaHasNoPhsParametersAndAFunctionWithNothingBehindItCannotBeCalled() {
        PhyFunction java = new PhyFunction("sq", (PhyFunction.SingleVarFunc) x -> x);
        assertThrows(IllegalStateException.class, java::getParams);
        assertThrows(IllegalStateException.class, () -> new PhyFunction("empty", (PhyFunction.MultiVarFunc) null).call(q(1, "m")));
        assertThrows(IllegalArgumentException.class, () -> java.call(new Quantity[0]));
    }

    // ---- PhyEquation

    @Test
    void anEquationSplitsOnItsOwnEqualsSignOnly() {
        assertEquals("a", PhyEquation.of(registry, A_IS_B).getLhs());
        assertEquals("b", PhyEquation.of(registry, A_IS_B).getRhs());
        assertEquals(A_AT_MOST_C, PhyEquation.of(registry, A_AT_MOST_C).getLhs());
        assertEquals("0", PhyEquation.of(registry, A_AT_MOST_C).getRhs());
        assertEquals("a == b", PhyEquation.of(registry, "a == b").getLhs());
        assertEquals("y", PhyEquation.of(registry, "y = (a >= b)").getLhs());
        assertEquals("(a >= b)", PhyEquation.of(registry, "y = (a >= b)").getRhs());
        assertEquals("x", PhyEquation.of(registry, "\"x = 1\"").getLhs());
    }

    @Test
    void substitutionTakesTheReplacementAsTextAndOnlyForWholeNames() {
        assertEquals("y = (2) + 1", PhyEquation.of(registry, Y_IS_X_PLUS_ONE).substitute("x", 2).getExpression());
        assertEquals("y = ($1) + 1", PhyEquation.of(registry, Y_IS_X_PLUS_ONE).substitute("x", "$1").getExpression());
        assertEquals("y = (a\\b) + 1", PhyEquation.of(registry, Y_IS_X_PLUS_ONE).substitute("x", "a\\b").getExpression());
        assertEquals("y = xx + (1)", PhyEquation.of(registry, "y = xx + x").substitute("x", 1).getExpression());
    }

    @Test
    void anEquationIsSolvedForAVariableAndEvaluatedWithNamedArguments() {
        PhyEquation force = PhyEquation.of(registry, FORCE_LAW);
        String solved = force.solve("a").getExpression();
        assertTrue(solved.contains("F") && solved.contains("m"), solved);
        Quantity result = force.call("m", Quantity.of(2, "kg"), "a", Quantity.of(3, ACCELERATION));
        near(6, result);
        assertEquals(0, result.getUncertainty());
        assertEquals("N", result.to("N").getUnit());
    }

    @Test
    void anEquationKeepsTheUncertaintyOfItsArguments() {
        PhyEquation force = PhyEquation.of(registry, FORCE_LAW);
        Quantity result = force.call("m", Quantity.withUncertainty(2, 0.1, "kg"), "a", Quantity.of(3, ACCELERATION));
        near(6, result);
        assertEquals(6 * 0.05, result.getUncertainty(), 1e-9);
    }

    @Test
    void callingAnEquationNeedsPairsAndExactlyOneUnknown() {
        PhyEquation force = PhyEquation.of(registry, FORCE_LAW);
        assertThrows(IllegalArgumentException.class, () -> force.call("m"));
        assertThrows(IllegalArgumentException.class, () -> force.call("m", Quantity.of(2, "kg")));
        assertThrows(IllegalArgumentException.class, () -> force.call("m", Quantity.of(2, "kg"), "a", Quantity.of(3, ACCELERATION), "F", Quantity.of(1, "N")));
        assertThrows(NullPointerException.class, () -> PhyEquation.of(registry, null));
    }

    @Test
    void anEquationCombinesWithAnotherOnBothSides() {
        PhyEquation sum = PhyEquation.of(registry, A_IS_B).add(PhyEquation.of(registry, "c = d"));
        assertEquals("(a) + (c) = (b) + (d)", sum.getExpression());
        assertEquals("(a) * (2.0 m) = (b) * (2.0 m)", PhyEquation.of(registry, A_IS_B).mul(Quantity.of(2, "m")).getExpression());
    }
}
