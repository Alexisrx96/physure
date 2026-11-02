package com.physure;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;

/**
 * Degrees of freedom of a Java measurement: declared on it, kept through every operation and conversion
 * (they belong to the source, which the quantity now carries), and read back as the effective degrees of
 * freedom, the coverage factor and the expanded uncertainty. The numbers are the core's; the golden file test
 * holds them to scipy.
 */
class DegreesOfFreedomTest {
    private static final double EPS = 1e-12;

    private static Quantity measured(double value, double uncertainty, String unit) {
        return new Quantity(value, uncertainty, unit);
    }

    @Test
    void aMeasurementWithNoDeclaredDegreesOfFreedomHasInfinitelyMany() {
        assertEquals(Double.POSITIVE_INFINITY, measured(1.0, 0.1, "m").effectiveDof());
        assertEquals(Double.POSITIVE_INFINITY, Quantity.of(3.0, "m").effectiveDof());
    }

    @Test
    void declaringDegreesOfFreedomReturnsTheSameMeasurementAndTheyAreRead() {
        Quantity x = measured(10.0, 0.5, "m");
        assertSame(x, x.withDof(16.0));
        assertEquals(16.0, x.effectiveDof(), EPS);
        assertEquals(16.0, x.multiply(2.0).effectiveDof(), EPS, "a result of one source has the degrees of freedom of that source");
    }

    @Test
    void aConversionKeepsTheDegreesOfFreedomAndTheExpandedUncertaintyIsInTheUnitShown() {
        Quantity metres = measured(1.0, 0.3, "m").withDof(4.0);
        Quantity millimetres = metres.to("mm");
        assertEquals(4.0, millimetres.effectiveDof(), EPS);
        ExpandedUncertainty expanded = millimetres.expanded(0.95);
        assertEquals(millimetres.coverageFactor(0.95) * 300.0, expanded.getExpandedUncertainty(), 1e-9);
        assertEquals("mm", expanded.getUnit());
        // the same measurement in another unit cancels against itself, and takes its degrees of freedom with it
        assertEquals(Double.POSITIVE_INFINITY, metres.subtract(metres).effectiveDof());
    }

    @Test
    void whatCannotBeDeclaredIsAnError() {
        Quantity x = measured(1.0, 0.1, "m");
        assertThrows(PhysureException.class, () -> Quantity.of(3.0, "m").withDof(4.0), "an exact quantity has none to count");
        assertThrows(PhysureException.class, () -> x.withDof(0.0));
        assertThrows(PhysureException.class, () -> x.withDof(-1.0));
        assertThrows(PhysureException.class, () -> x.withDof(Double.NaN));
        assertThrows(PhysureException.class, () -> measured(1.0, 0.1, "m").add(measured(2.0, 0.1, "m")).withDof(4.0),
            "a sum of two measurements is not one");
        Quantity declared = measured(1.0, 0.1, "m").withDof(5.0);
        assertThrows(PhysureException.class, () -> declared.withDof(6.0), "the source already has 5");
        assertSame(declared, declared.withDof(5.0));
    }

    @Test
    void aProbabilityOutsideZeroAndOneIsAnError() {
        Quantity declared = measured(1.0, 0.1, "m").withDof(5.0);
        for (double p : new double[] {0.0, 1.0, -0.5, 1.5, Double.NaN}) {
            assertThrows(PhysureException.class, () -> declared.coverageFactor(p), "p = " + p);
            assertThrows(PhysureException.class, () -> declared.expanded(p), "p = " + p);
        }
    }

    // ---- Type A

    @Test
    void aTypeAEvaluationIsTheMeanTheStandardErrorAndNMinusOneDegreesOfFreedom() {
        Quantity v = Quantity.fromSamples(new double[] {10.0, 10.2, 9.9, 10.1, 10.3}, "V");
        assertEquals(10.1, v.getValue(), EPS);
        assertEquals(Math.sqrt(0.025 / 5.0), v.getUncertainty(), EPS);
        assertEquals(4.0, v.effectiveDof(), EPS);
        assertEquals("V", v.getUnit());
    }

    @Test
    void observationsThatAreAllEqualHaveNoSpreadToEstimate() {
        Quantity same = Quantity.fromSamples(new double[] {2.0, 2.0, 2.0}, "V");
        assertEquals(0.0, same.getUncertainty(), 0.0);
        assertEquals(Double.POSITIVE_INFINITY, same.effectiveDof());
    }

    @Test
    void aTypeAEvaluationNeedsTwoFiniteObservations() {
        assertThrows(PhysureException.class, () -> Quantity.fromSamples(new double[] {1.0}, "V"));
        assertThrows(PhysureException.class, () -> Quantity.fromSamples(new double[0], "V"));
        assertThrows(PhysureException.class, () -> Quantity.fromSamples(new double[] {1.0, Double.NaN}, "V"));
        assertThrows(NullPointerException.class, () -> Quantity.fromSamples(null, "V"));
    }

    // ---- the presentation of an expanded uncertainty

    @Test
    void anExpandedUncertaintyIsRoundedAsTheGuidePresentsAResult() {
        ExpandedUncertainty r = measured(10.0, 0.5, "V").withDof(16.0).expanded(0.95);
        assertEquals(2.1199, r.getCoverageFactor(), 1e-4);
        assertEquals(16.0, r.getEffectiveDof(), EPS);
        assertEquals(0.5, r.getStandardUncertainty(), 0.0);
        assertEquals("(10.0 ± 1.1) V (k=2.12, p=95 %)", r.toString());
    }
}
