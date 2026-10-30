package com.physure;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTimeoutPreemptively;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.time.Duration;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;

/**
 * Strict tests of {@link UnitRegistry}: evaluating, the calculus calls, the catalog, the lifecycle of
 * the native handle and use from several threads.
 */
class RegistryStrictTest {
    private static UnitRegistry registry;

    @BeforeAll
    static void open() {
        registry = new UnitRegistry();
    }

    @AfterAll
    static void close() {
        registry.close();
    }

    // ---- evaluate

    @Test
    void evaluateReturnsTheLastStatementAsTheAnswer() {
        // the README's own example
        Quantity area = registry.evaluate("a = 15 m; b = 5 m; a * b");
        assertEquals(75.0, area.getValue());
        assertEquals("m^2", area.getUnit());
        Quantity same = registry.evaluate("a = 15 m\nb = 5 m\na * b");
        assertEquals(75.0, same.getValue());
        assertEquals("m^2", same.getUnit());
    }

    @Test
    void evaluateKeepsTheUncertaintyOfAResult() {
        Quantity measured = registry.evaluate("1.5 +/- 0.1 m");
        assertEquals(1.5, measured.getValue());
        assertEquals(0.1, measured.getUncertainty(), 1e-12);
        assertEquals("m", measured.getUnit());
        Quantity sum = registry.evaluate("(1 +/- 0.1 m) + (2 +/- 0.2 m)");
        assertEquals(3.0, sum.getValue());
        assertEquals(Math.hypot(0.1, 0.2), sum.getUncertainty(), 0.01);
        assertEquals("m", sum.getUnit());
    }

    @Test
    void evaluateAppliesTheTemperatureAndAngleRulesOfTheCore() {
        Quantity warmer = registry.evaluate("20 degC + 5 K");
        assertEquals(25.0, warmer.getValue());
        assertEquals("degC", warmer.getUnit());
        assertThrows(PhysureException.class, () -> registry.evaluate("2 * 20 degC"));
        assertEquals(1.0, registry.evaluate("sin(90 deg)").getValue(), 1e-12);
        assertThrows(PhysureException.class, () -> registry.evaluate("sin(1 m)"));
    }

    @Test
    void anExpressionThatIsNotAQuantityIsAnErrorAndNotAQuantityOfTheText() {
        assertThrows(PhysureException.class, () -> registry.evaluate("\"hello\""));
    }

    @Test
    void evaluateReportsAnErrorWithItsCause() {
        assertTrue(assertThrows(PhysureException.class, () -> registry.evaluate("1 +")).getMessage().contains("Parse error"));
        assertTrue(assertThrows(PhysureException.class, () -> registry.evaluate("1 m + 2 s")).getMessage().contains("mismatch"));
        assertTrue(assertThrows(PhysureException.class, () -> registry.evaluate("1/0")).getMessage().contains("Division by zero"));
    }

    @Test
    void variablesBelongToTheirRegistry() {
        try (UnitRegistry one = new UnitRegistry(); UnitRegistry other = new UnitRegistry()) {
            one.evaluateRaw("x = 5 m");
            assertEquals(10.0, one.evaluate("x * 2").getValue());
            assertNotEquals("5.0 m", other.evaluateRaw("x").trim());
        }
    }

    @Test
    void textSurvivesTheCrossingIntact() {
        assertEquals("héllo ✓ 😀", registry.evaluateRaw("\"héllo ✓ 😀\"").trim());
        assertEquals("a\u0000b", registry.evaluateRaw("\"a\u0000b\"").trim());
        StringBuilder big = new StringBuilder("\"");
        for (int i = 0; i < 1_000_000; i++) big.append('x');
        big.append('"');
        assertEquals(1_000_000, registry.evaluateRaw(big.toString()).trim().length());
    }

    @Test
    void aRunawayLoopStopsWithAnError() {
        PhysureException e = assertThrows(PhysureException.class, () -> registry.evaluateRaw("while true { x = 1 }"));
        assertTrue(e.getMessage().contains("did not converge"), e.getMessage());
    }

    @Test
    void deepNestingIsParsedQuicklyOrRefusedAndNeverCrashesTheJvm() {
        // 24 nested parentheses took 53 s, and 500 levels ended the process in a stack overflow
        String shallow = repeat("(", 24) + "1" + repeat(")", 24);
        assertTimeoutPreemptively(Duration.ofSeconds(5), () -> assertEquals("1.0", registry.evaluateRaw(shallow).trim()));
        String deep = repeat("(", 5000) + "1" + repeat(")", 5000);
        PhysureException e = assertThrows(PhysureException.class, () -> registry.evaluateRaw(deep));
        assertTrue(e.getMessage().contains("nested"), e.getMessage());
    }

    private static String repeat(String s, int times) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < times; i++) sb.append(s);
        return sb.toString();
    }

    // ---- calculus

    @Test
    void solveDeriveAndIntegrateReturnExpressions() {
        String solved = registry.solve("P * V = n * R * T", "T");
        for (String symbol : new String[] {"P", "V", "n", "R"}) {
            assertTrue(solved.contains(symbol), solved);
        }
        String derivative = registry.deriv("v0 * t + 0.5 * a * t^2", "t");
        assertTrue(derivative.contains("v0") && derivative.contains("a") && derivative.contains("t"), derivative);
        assertTrue(registry.integral("3 * t^2", "t").contains("t^3"));
    }

    @Test
    void argumentsOfTheCalculusCallsAreDataNotCode() {
        // a quote would end the PHS string they are put in, and the rest would be a statement of its own
        assertThrows(IllegalArgumentException.class, () -> registry.solve("x = \"a\"", "x"));
        assertThrows(IllegalArgumentException.class, () -> registry.deriv("t\"); 1/0; (\"t", "t"));
        assertThrows(IllegalArgumentException.class, () -> registry.solve("x = 1", "x\", \"y"));
        assertThrows(IllegalArgumentException.class, () -> registry.integral("x", "\""));
    }

    // ---- the catalog

    @Test
    void everyUnitOfEveryCategoryIsKnownToTheRegistry() {
        Map<String, String[]> categories = registry.getCategories();
        assertTrue(categories.size() >= 10, "categories: " + categories.size());
        List<String> unknown = new ArrayList<>();
        int checked = 0;
        for (Map.Entry<String, String[]> category : categories.entrySet()) {
            for (String unit : category.getValue()) {
                checked++;
                try {
                    double scale = registry.getUnitScale(unit);
                    if (Double.isNaN(scale) || Double.isInfinite(scale) || scale == 0.0) unknown.add(unit + " -> " + scale);
                } catch (PhysureException e) {
                    unknown.add(category.getKey() + ": " + unit);
                }
            }
        }
        assertTrue(checked > 50, "units checked: " + checked);
        assertTrue(unknown.isEmpty(), unknown.size() + " units of the categories are not units: " + unknown);
    }

    @Test
    void anEmptyUnitIsDimensionlessAndATextThatIsNotAUnitIsAnError() {
        assertEquals(1.0, registry.getUnitScale(""));
        assertTrue(registry.getUnitExponents("").isEmpty());
        assertThrows(PhysureException.class, () -> registry.getUnitScale("2 m"));
        assertThrows(PhysureException.class, () -> registry.getUnitScale("m/0"));
        assertThrows(PhysureException.class, () -> registry.getUnitPowers("not_a_unit"));
    }

    @Test
    void aRegistryRefusesDefinitionsItCannotRead() {
        assertThrows(PhysureException.class, () -> UnitRegistry.fromContent("[Units]\nbad = notanumber, L, [bad]\n"));
        assertThrows(PhysureException.class, () -> UnitRegistry.fromContent("[Units]\nthis is not a unit line\n"));
    }

    // ---- null arguments

    @Test
    void aNullArgumentIsANullPointerExceptionNamingNothingNative() {
        assertThrows(NullPointerException.class, () -> registry.getUnitScale(null));
        assertThrows(NullPointerException.class, () -> registry.getUnitExponents(null));
        assertThrows(NullPointerException.class, () -> registry.getUnitPowers(null));
        assertThrows(NullPointerException.class, () -> registry.evaluate(null));
        assertThrows(NullPointerException.class, () -> registry.evaluateRaw(null));
        assertThrows(NullPointerException.class, () -> registry.solve(null, "x"));
        assertThrows(NullPointerException.class, () -> registry.solve("x = 1", null));
        assertThrows(NullPointerException.class, () -> registry.deriv(null, "x"));
        assertThrows(NullPointerException.class, () -> registry.integral("x", null));
        assertThrows(NullPointerException.class, () -> UnitRegistry.fromContent(null));
        assertThrows(NullPointerException.class, () -> new UnitRegistry((String) null));
    }

    // ---- lifecycle

    @Test
    void everyCallOnAClosedRegistryIsAnIllegalStateAndClosingTwiceIsFine() {
        UnitRegistry closed = new UnitRegistry();
        closed.close();
        closed.close();
        List<Callable<Object>> calls = new ArrayList<>();
        calls.add(() -> closed.getUnitScale("m"));
        calls.add(() -> closed.getUnitExponents("m"));
        calls.add(() -> closed.getUnitPowers("m"));
        calls.add(() -> closed.evaluate("1 m"));
        calls.add(() -> closed.evaluateRaw("1 m"));
        calls.add(() -> closed.getCategories());
        calls.add(() -> closed.solve("x = 1", "x"));
        calls.add(() -> closed.deriv("x", "x"));
        calls.add(() -> closed.integral("x", "x"));
        for (Callable<Object> call : calls) {
            assertThrows(IllegalStateException.class, call::call);
        }
    }

    @Test
    void manyRegistriesCanBeOpenedAndClosed() {
        for (int i = 0; i < 2000; i++) {
            try (UnitRegistry each = new UnitRegistry()) {
                assertEquals(1000.0, each.getUnitScale("km"));
            }
        }
    }

    // ---- threads

    /** A registry shared by threads: its interpreter is one mutable native object, so calls must not overlap. */
    @Test
    void oneRegistryUsedFromManyThreadsGivesEveryThreadItsOwnCorrectAnswer() throws Exception {
        try (UnitRegistry shared = new UnitRegistry()) {
            ExecutorService pool = Executors.newFixedThreadPool(8);
            try {
                List<Future<Integer>> results = new ArrayList<>();
                for (int t = 0; t < 8; t++) {
                    final int id = t;
                    results.add(pool.submit(() -> {
                        int wrong = 0;
                        for (int i = 0; i < 500; i++) {
                            Quantity doubled = shared.evaluate("v" + id + " = " + i + " m\nv" + id + " * 2");
                            if (doubled.getValue() != 2.0 * i || !"m".equals(doubled.getUnit())) wrong++;
                            if (shared.getUnitScale("km") != 1000.0) wrong++;
                        }
                        return wrong;
                    }));
                }
                for (Future<Integer> result : results) {
                    assertEquals(0, result.get(120, TimeUnit.SECONDS));
                }
            } finally {
                pool.shutdownNow();
            }
        }
    }

    @Test
    void quantityArithmeticFromManyThreadsIsCorrect() throws Exception {
        ExecutorService pool = Executors.newFixedThreadPool(8);
        try {
            List<Future<Boolean>> results = new ArrayList<>();
            for (int t = 0; t < 8; t++) {
                results.add(pool.submit(() -> {
                    for (int i = 0; i < 1000; i++) {
                        Quantity sum = new Quantity(i, "m").add(new Quantity(1, "km"));
                        if (sum.getValue() != i + 1000.0) return false;
                    }
                    return true;
                }));
            }
            for (Future<Boolean> result : results) assertTrue(result.get(120, TimeUnit.SECONDS));
        } finally {
            pool.shutdownNow();
        }
    }

    @Test
    void closingARegistryWhileAnotherThreadUsesItIsSafe() throws Exception {
        for (int round = 0; round < 50; round++) {
            UnitRegistry shared = new UnitRegistry();
            ExecutorService pool = Executors.newFixedThreadPool(2);
            try {
                Future<?> user = pool.submit(() -> {
                    try {
                        for (int i = 0; i < 200; i++) shared.evaluateRaw("1 m + 2 m");
                    } catch (IllegalStateException closedBeforeUse) {
                        // the registry may be closed under it; what must not happen is a crash
                    }
                });
                Future<?> closer = pool.submit(shared::close);
                closer.get(30, TimeUnit.SECONDS);
                user.get(30, TimeUnit.SECONDS);
            } finally {
                pool.shutdownNow();
            }
        }
    }
}
