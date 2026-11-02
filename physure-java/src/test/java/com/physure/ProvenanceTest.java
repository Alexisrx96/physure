package com.physure;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import org.junit.jupiter.api.Test;

/**
 * A quantity keeps the sources of its uncertainty across the native calls, so a measurement that is reused
 * cancels or accumulates as the core says (JCGM 100:2008 5.2) and two measurements that merely have equal
 * numbers stay two measurements. Before, every native call made a new measurement of each operand and
 * {@code x - x} came out at {@code sqrt(2) u}.
 */
class ProvenanceTest {
    private static final double EPS = 1e-12;

    private static Quantity measured(double value, double uncertainty, String unit) {
        return new Quantity(value, uncertainty, unit);
    }

    private static void assertExact(double expectedValue, Quantity actual) {
        assertEquals(expectedValue, actual.getValue(), EPS);
        assertEquals(0.0, actual.getUncertainty(), EPS);
    }

    // ---- one measurement used twice

    @Test
    void aMeasurementTakenAwayFromItselfIsExactlyZero() {
        Quantity x = measured(10.0, 0.5, "m");
        assertExact(0.0, x.subtract(x));
    }

    @Test
    void aMeasurementMultipliedByItselfIsItsSquareWithTheDerivativeOfTheSquare() {
        Quantity x = measured(10.0, 0.5, "m");
        Quantity square = x.multiply(x);
        assertEquals(100.0, square.getValue(), EPS);
        assertEquals(2 * 10.0 * 0.5, square.getUncertainty(), 1e-9, "2 x u, not sqrt(2) x u");
        assertEquals(square.getUncertainty(), x.pow(2).getUncertainty(), 1e-9);
    }

    @Test
    void aMeasurementDividedByItselfIsExactlyOne() {
        Quantity x = measured(10.0, 0.5, "m");
        assertExact(1.0, x.divide(x));
        assertExact(1.0, x.pow(2).divide(x.pow(2)));
    }

    @Test
    void addingAnotherMeasurementAndTakingItAwayGivesTheFirstBack() {
        Quantity x = measured(10.0, 0.5, "m");
        Quantity y = measured(4.0, 0.3, "m");
        Quantity back = x.add(y).subtract(y);
        assertEquals(10.0, back.getValue(), EPS);
        assertEquals(0.5, back.getUncertainty(), 1e-12, "y cancelled, and only x is left");
    }

    // ---- two measurements with equal numbers

    @Test
    void twoMeasurementsWithEqualNumbersAreTwoMeasurements() {
        Quantity a = measured(10.0, 0.5, "m");
        Quantity b = measured(10.0, 0.5, "m");
        assertEquals(a, b, "equal by value, as before");
        Quantity difference = a.subtract(b);
        assertEquals(0.0, difference.getValue(), EPS);
        assertEquals(0.5 * Math.sqrt(2), difference.getUncertainty(), 1e-12, "independent: in quadrature");
    }

    // ---- what a unit conversion and a folded scale do to the sources

    @Test
    void aConversionIsNotANewMeasurement() {
        Quantity x = measured(10.0, 0.5, "m");
        Quantity centimetres = x.to("cm");
        assertEquals(1000.0, centimetres.getValue(), EPS);
        assertEquals(50.0, centimetres.getUncertainty(), 1e-9);
        assertExact(0.0, x.subtract(centimetres));
        assertExact(0.0, centimetres.subtract(centimetres.to("m")));
    }

    @Test
    void anAnonymousCompoundUnitFoldsItsScaleIntoTheSourcesToo() {
        // 2 m * 3 km is 6000 m^2: the numbers and the sources are folded together
        Quantity area = measured(2.0, 0.1, "m").multiply(measured(3.0, 0.2, "km"));
        assertEquals(6000.0, area.getValue(), 1e-9);
        assertExact(0.0, area.subtract(area));
        assertExact(1.0, area.divide(area));
    }

    // ---- the first use of a quantity built in Java

    @Test
    void aQuantityUsedForTheFirstTimeByManyThreadsAtOnceIsOneMeasurement() throws Exception {
        Quantity x = measured(10.0, 0.5, "m");
        Quantity nothing = Quantity.of(0.0, "m");
        int threads = 8;
        ExecutorService pool = Executors.newFixedThreadPool(threads);
        try {
            CountDownLatch start = new CountDownLatch(1);
            List<Future<Quantity>> results = new ArrayList<>();
            for (int i = 0; i < threads; i++) {
                results.add(pool.submit(() -> {
                    start.await();
                    return x.add(nothing);
                }));
            }
            start.countDown();
            List<Quantity> sums = new ArrayList<>();
            for (Future<Quantity> result : results) {
                sums.add(result.get());
            }
            for (Quantity sum : sums) {
                // a result of one thread cancels against a result of any other only if both come from one id
                assertExact(0.0, sum.subtract(sums.get(0)));
            }
        } finally {
            pool.shutdownNow();
        }
    }

    // ---- round keeps the sources

    @Test
    void roundingKeepsTheSourcesScaledToTheRoundedUncertainty() {
        Quantity x = measured(10.123, 0.456, "m");
        Quantity rounded = x.round(1);
        assertEquals(10.1, rounded.getValue(), EPS);
        assertEquals(0.5, rounded.getUncertainty(), EPS);
        assertExact(0.0, rounded.subtract(x.round(1)));
        assertEquals(0.5 - 0.456, rounded.subtract(x).getUncertainty(), 1e-12, "the same source, two contributions");
    }

    @Test
    void anUncertaintyRoundedToZeroIsExact() {
        Quantity rounded = measured(10.1, 0.4, "m").round(0);
        assertEquals(0.0, rounded.getUncertainty(), 0.0);
        assertEquals(0.1, rounded.add(measured(1.0, 0.1, "m")).getUncertainty(), 1e-12, "no source left to add");
    }

    // ---- what the evaluator hands back

    @Test
    void aResultOfEvaluateKeepsTheSourcesOfTheVariableItCameFrom() {
        try (UnitRegistry registry = new UnitRegistry()) {
            registry.evaluate("x = 10 +/- 0.5 m");
            Quantity a = registry.evaluate("x");
            Quantity b = registry.evaluate("x * 2");
            assertExact(0.0, a.subtract(a));
            assertExact(0.0, b.subtract(a.multiply(2.0)));
        }
    }

    // ---- functions of a measurement are the core's, so they keep its sources

    @Test
    void aTrigonometricFunctionOfAMeasurementKeepsItsSources() {
        Quantity angle = measured(30.0, 1.0, "deg");
        assertExact(0.0, angle.sin().subtract(angle.sin()));
        assertExact(1.0, angle.tan().divide(angle.tan()));
        assertExact(0.0, angle.cos().subtract(angle.cos()));
    }

    @Test
    void theAbsoluteValueOfANegativeMeasurementIsMinusItAndKeepsItsSources() {
        Quantity x = measured(-3.0, 0.1, "m");
        Quantity magnitude = x.abs();
        assertEquals(3.0, magnitude.getValue(), EPS);
        assertEquals(0.1, magnitude.getUncertainty(), EPS);
        assertExact(0.0, magnitude.add(x));
        assertEquals(0.2, magnitude.subtract(x).getUncertainty(), 1e-12, "|x| - x = -2x: the sources add up");
    }

    // ---- an operation the core refuses is still an error

    @Test
    void incompatibleDimensionsAreStillAnErrorForTheSameMeasurement() {
        Quantity x = measured(10.0, 0.5, "m");
        assertThrows(PhysureException.class, () -> x.add(x.to("cm").divide(Quantity.of(1.0, "s"))));
        assertNotEquals(x, x.to("cm"));
    }
}
