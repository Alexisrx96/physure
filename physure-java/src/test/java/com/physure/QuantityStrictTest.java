package com.physure;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTimeoutPreemptively;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.time.Duration;
import org.junit.jupiter.api.Test;

/**
 * Strict tests of {@link Quantity}: a result carries its uncertainty and its scale, an incompatible
 * operation is an error and never a number, and the unit text a result carries can be read back.
 * The expected values are the ones the core gives (the WASM binding's tests hold it to the same).
 */
class QuantityStrictTest {
    private static final double EPS = 1e-12;

    private static Quantity q(double value, String unit) {
        return new Quantity(value, unit);
    }

    private static Quantity q(double value, double uncertainty, String unit) {
        return new Quantity(value, uncertainty, unit);
    }

    private static void near(double expected, double actual) {
        assertEquals(expected, actual, EPS * Math.max(1.0, Math.abs(expected)));
    }

    // ---- the text parser takes time linear in its input

    @Test
    void aLongRunOfDigitsThatIsNotAQuantityIsRefusedInLinearTime() {
        // `\d+\.?\d*` split the digits between its two parts in every possible way before giving up:
        // quadratic. Fifty thousand digits took seconds; the unambiguous form takes milliseconds.
        // (`String.repeat` is Java 11; this module compiles for Java 8)
        StringBuilder digits = new StringBuilder();
        for (int i = 0; i < 50_000; i++) {
            digits.append('1');
        }
        String written = digits.append('x').toString();
        assertTimeoutPreemptively(Duration.ofSeconds(2), () ->
            assertThrows(IllegalArgumentException.class, () -> Quantity.parse(written)));
    }

    @Test
    void theNumbersTheParserReadsAreStillTheSame() {
        assertEquals(12.0, Quantity.parse("12 m").getValue(), 0.0);
        assertEquals(12.0, Quantity.parse("12. m").getValue(), 0.0);
        assertEquals(12.5, Quantity.parse("12.5 m").getValue(), 0.0);
        assertEquals(0.5, Quantity.parse(".5 m").getValue(), 0.0);
        assertEquals(1.5e3, Quantity.parse("1.5e3 m").getValue(), 0.0);
        assertEquals(125.0, Quantity.parse("125 +/- 5 m").getValue(), 0.0);
    }

    // ---- uncertainty survives every operation

    @Test
    void aSumAndADifferenceAddTheirUncertaintiesInQuadrature() {
        Quantity sum = q(10, 0.5, "m").add(q(5, 0.3, "m"));
        near(15, sum.getValue());
        near(Math.hypot(0.5, 0.3), sum.getUncertainty());
        Quantity difference = q(10, 0.5, "m").sub(q(5, 0.3, "m"));
        near(5, difference.getValue());
        near(Math.hypot(0.5, 0.3), difference.getUncertainty());
    }

    @Test
    void aProductAndAQuotientAddTheirRelativeUncertaintiesInQuadrature() {
        double relative = Math.hypot(0.5 / 10, 0.3 / 5);
        Quantity product = q(10, 0.5, "m").mul(q(5, 0.3, "s"));
        near(50, product.getValue());
        near(50 * relative, product.getUncertainty());
        Quantity quotient = q(10, 0.5, "m").div(q(5, 0.3, "s"));
        near(2, quotient.getValue());
        near(2 * relative, quotient.getUncertainty());
    }

    @Test
    void aPowerScalesTheRelativeUncertaintyByTheExponent() {
        Quantity squared = q(3, 0.1, "m").pow(2);
        near(9, squared.getValue());
        near(2 * 3 * 0.1, squared.getUncertainty());
    }

    @Test
    void aConversionScalesTheUncertaintyWithTheValue() {
        Quantity converted = q(10, 0.5, "m").to("cm");
        near(1000, converted.getValue());
        near(50, converted.getUncertainty());
        assertEquals("cm", converted.getUnit());
    }

    @Test
    void aSumInTwoUnitsKeepsTheUncertaintyOfBoth() {
        Quantity sum = q(1, 0.01, "km").add(q(500, 5, "m"));
        near(1.5, sum.getValue());
        near(Math.hypot(0.01, 0.005), sum.getUncertainty());
        assertEquals("km", sum.getUnit());
    }

    @Test
    void aScalarKeepsTheUncertaintyScaled() {
        Quantity tripled = q(2, 0.1, "m").multiply(3.0);
        near(6, tripled.getValue());
        near(0.3, tripled.getUncertainty());
        Quantity halved = q(2, 0.1, "m").divide(2.0);
        near(1, halved.getValue());
        near(0.05, halved.getUncertainty());
    }

    // ---- scale: the number and the unit of a result belong together

    @Test
    void aProductOfTwoScalesIsReadInTheScaleItIsShown() {
        Quantity product = q(2, "m").mul(q(3, "km"));
        near(6000, product.getValue());
        assertEquals("m^2", product.getUnit());
        Quantity swapped = q(3, "km").mul(q(2, "m"));
        near(6000, swapped.getValue());
        assertEquals("m^2", swapped.getUnit());
    }

    @Test
    void aQuotientOfTwoScalesIsADimensionlessNumberOfTheRightSize() {
        Quantity ratio = q(3, "km").div(q(2, "m"));
        near(1500, ratio.getValue());
        assertEquals("", ratio.getUnit());
    }

    @Test
    void theUncertaintyOfAProductOfTwoScalesIsInTheScaleOfItsValue() {
        Quantity product = q(2, 0.1, "m").mul(q(3, "km"));
        near(6000, product.getValue());
        near(300, product.getUncertainty());
    }

    // ---- dimensions: an error, never a number

    @Test
    void incompatibleQuantitiesDoNotAddOrSubtract() {
        Quantity metre = q(1, "m");
        for (Quantity other : new Quantity[] {q(1, "s"), q(1, "kg"), q(5, "")}) {
            assertThrows(PhysureException.class, () -> metre.add(other), "m + " + other);
            assertThrows(PhysureException.class, () -> metre.sub(other), "m - " + other);
            assertThrows(PhysureException.class, () -> other.add(metre), other + " + m");
            assertThrows(PhysureException.class, () -> other.sub(metre), other + " - m");
        }
    }

    @Test
    void anIncompatibleConversionAndAnUnknownUnitAreErrors() {
        assertThrows(PhysureException.class, () -> q(1, "m").to("s"));
        assertThrows(PhysureException.class, () -> q(1, "m").to(""));
        assertThrows(PhysureException.class, () -> q(1, "").to("m"));
        PhysureException unknown = assertThrows(PhysureException.class, () -> q(1, "m").to("not_a_unit"));
        assertTrue(unknown.getMessage().contains("Unknown unit"), unknown.getMessage());
        assertThrows(PhysureException.class, () -> q(1, "not_a_unit").add(q(1, "m")));
    }

    @Test
    void comparisonsConvertAndRefuseOtherDimensions() {
        assertTrue(q(1, "km").greaterThan(q(999, "m")));
        assertFalse(q(1, "km").greaterThan(q(1001, "m")));
        assertTrue(q(1, "m").lessThan(q(1, "km")));
        assertThrows(PhysureException.class, () -> q(1, "m").greaterThan(q(1, "s")));
        assertThrows(PhysureException.class, () -> q(1, "m").lessThan(q(1, "s")));
    }

    @Test
    void assertionsCompareAfterConversionAndRefuseOtherDimensions() {
        q(1, "m").physAssert(q(1000, "mm"));
        assertThrows(PhysureException.class, () -> q(1, "m").physAssert(q(2, "m")));
        assertThrows(PhysureException.class, () -> q(1, "m").physAssert(q(1, "s")));
        q(1, "m").physExactAssert(q(1, "meter"));
        assertThrows(PhysureException.class, () -> q(1, "m").physExactAssert(q(1000, "mm")));
    }

    @Test
    void aNegativeBaseHasNoFractionalPowerAndDividingByZeroIsAnError() {
        assertThrows(PhysureException.class, () -> q(-4, "m^2").sqrt());
        assertThrows(PhysureException.class, () -> q(1, "m").div(q(0, "s")));
        assertThrows(PhysureException.class, () -> q(1, "m").divide(0.0));
    }

    // ---- temperatures are points and intervals

    @Test
    void aTemperaturePointPlusOrMinusAnIntervalIsAPoint() {
        Quantity plus = q(20, "degC").add(q(5, "K"));
        near(25, plus.getValue());
        assertEquals("degC", plus.getUnit());
        Quantity minus = q(20, "degC").sub(q(5, "K"));
        near(15, minus.getValue());
        assertEquals("degC", minus.getUnit());
        Quantity interval = q(25, "degC").sub(q(20, "degC"));
        near(5, interval.getValue());
        assertEquals("K", interval.getUnit());
    }

    @Test
    void twoTemperaturePointsDoNotAddAndAPlainNumberDoesNotScaleOne() {
        assertThrows(PhysureException.class, () -> q(20, "degC").add(q(5, "degC")));
        assertThrows(PhysureException.class, () -> q(20, "degC").multiply(2.0));
        assertThrows(PhysureException.class, () -> q(20, "degC").divide(2.0));
    }

    @Test
    void temperaturesConvertBetweenScales() {
        near(293.15, q(20, "degC").to("K").getValue());
        near(68, q(20, "degC").to("degF").getValue());
        near(-40, q(-40, "degC").to("degF").getValue());
    }

    // ---- the unit a result carries can be fed back in

    @Test
    void theUnitOfEveryResultIsAUnitTheBindingReads() {
        Quantity[] results = {
            q(4, "m").sqrt(),
            q(8, "m").pow(1.0 / 3.0),
            q(4, "s").pow(-0.5),
            q(2, "m").mul(q(3, "km")),
            q(6, "m").div(q(2, "s")),
            q(1, "kg").mul(q(1, "m")).pow(0.25),
        };
        for (Quantity result : results) {
            Quantity again = q(result.getValue(), result.getUnit());
            near(result.getValue(), again.to(result.getUnit()).getValue());
        }
    }

    @Test
    void aSquareRootTimesItselfIsTheOriginalUnit() {
        Quantity root = q(4, "m").sqrt();
        Quantity back = root.mul(root);
        near(4, back.getValue());
        assertEquals("m", back.getUnit());
        near(2, root.to("m^0.5").getValue());
    }

    // ---- trigonometry knows about angles

    @Test
    void trigonometricFunctionsReadTheAngleInItsUnit() {
        near(1.0, q(90, "deg").sin().getValue());
        near(-1.0, q(180, "deg").cos().getValue());
        near(1.0, q(45, "deg").tan().getValue());
        near(Math.sin(0.5), q(0.5, "rad").sin().getValue());
        near(Math.sin(0.5), q(0.5, "").sin().getValue());
        assertEquals("", q(90, "deg").sin().getUnit());
    }

    @Test
    void trigonometricFunctionsRefuseAQuantityThatIsNotAnAngle() {
        assertThrows(PhysureException.class, () -> q(1, "m").sin());
        assertThrows(PhysureException.class, () -> q(1, "s").cos());
        assertThrows(PhysureException.class, () -> q(1, "kg").tan());
    }

    @Test
    void trigonometricFunctionsPropagateTheUncertaintyOfTheAngle() {
        double radians = Math.toRadians(1.0);
        Quantity sine = q(30, 1, "deg").sin();
        near(0.5, sine.getValue());
        near(Math.cos(Math.toRadians(30)) * radians, sine.getUncertainty());
    }

    // ---- rounding and absolute value

    @Test
    void roundingDoesNotOverflowOrLoseNaN() {
        assertEquals(1e300, q(1e300, "m").round(20).getValue());
        assertTrue(Double.isNaN(q(Double.NaN, "m").round(2).getValue()));
        assertEquals(Double.POSITIVE_INFINITY, q(Double.POSITIVE_INFINITY, "m").round(2).getValue());
        near(1200, q(1234.5678, "m").round(-2).getValue());
        Quantity rounded = q(1.2345, 0.0456, "m").round(2);
        near(1.23, rounded.getValue());
        near(0.05, rounded.getUncertainty());
        assertEquals("m", rounded.getUnit());
    }

    @Test
    void theAbsoluteValueKeepsUncertaintyAndUnit() {
        Quantity abs = q(-2, 0.1, "m").abs();
        assertEquals(2.0, abs.getValue());
        assertEquals(0.1, abs.getUncertainty());
        assertEquals("m", abs.getUnit());
    }

    // ---- inputs

    @Test
    void aNullTargetUnitIsRefusedWithAnException() {
        assertThrows(NullPointerException.class, () -> q(1, "m").to(null));
        assertThrows(NullPointerException.class, () -> q(1, "m").add(null));
        assertThrows(NullPointerException.class, () -> q(1, "m").mul(null));
    }

    @Test
    void aMissingUnitIsDimensionlessAndWhitespaceIsTrimmed() {
        assertEquals("", new Quantity(1, null).getUnit());
        assertEquals("m", q(1, "  m ").getUnit());
        assertEquals(q(1, "m"), q(1, " m "));
    }

    @Test
    void anUncertaintyIsANonNegativeFiniteSpread() {
        assertEquals(0.5, q(1, -0.5, "m").getUncertainty());
        assertThrows(IllegalArgumentException.class, () -> q(1, Double.NaN, "m"));
        assertThrows(IllegalArgumentException.class, () -> q(1, Double.POSITIVE_INFINITY, "m"));
    }

    // ---- text

    @Test
    void parsingWhatToStringWritesGivesTheSameQuantity() {
        Quantity[] quantities = {
            q(1.5, 0.1, "m"), q(3, "m/s"), q(-2, "kg"), q(1e-9, "m"), q(1e10, "m"), q(0.25, 0.01, "km/h"), q(7, ""),
        };
        for (Quantity original : quantities) {
            Quantity parsed = Quantity.parse(original.toString());
            assertEquals(original, parsed, original.toString());
        }
    }

    @Test
    void parsingReadsTheWaysTheCoreWritesAnUncertainty() {
        for (String text : new String[] {"1.5 ± 0.1 m", "1.5 +/- 0.1 m", "1.5±0.1 m"}) {
            Quantity parsed = Quantity.parse(text);
            near(1.5, parsed.getValue());
            near(0.1, parsed.getUncertainty());
            assertEquals("m", parsed.getUnit(), text);
        }
    }

    @Test
    void parsingTakesNumbersInTheFormsJavaAndTheCoreWrite() {
        near(1000, Quantity.parse("1e3 m").getValue());
        near(-2.5, Quantity.parse("-2.5 m/s").getValue());
        assertTrue(Double.isNaN(Quantity.parse("NaN m").getValue()));
        assertEquals(Double.POSITIVE_INFINITY, Quantity.parse("Infinity").getValue());
        assertEquals("m", Quantity.parse("3 m  ").getUnit());
        assertEquals("m", Quantity.parse("3   m").getUnit());
    }

    @Test
    void parsingRefusesANumberItCannotRead() {
        assertThrows(IllegalArgumentException.class, () -> Quantity.parse(null));
        assertThrows(IllegalArgumentException.class, () -> Quantity.parse("1,5 m"));
        assertThrows(IllegalArgumentException.class, () -> Quantity.parse("1.2.3 m"));
    }

    @Test
    void equalsAndHashCodeAgreeAndTellQuantitiesApart() {
        assertEquals(q(1, 0.1, "m"), q(1, 0.1, "m"));
        assertEquals(q(1, 0.1, "m").hashCode(), q(1, 0.1, "m").hashCode());
        assertNotEquals(q(1, 0.1, "m"), q(1, 0.2, "m"));
        assertNotEquals(q(1, "m"), q(1, "s"));
        assertNotEquals(q(1, "m"), null);
        assertNotEquals(q(1, "m"), "1 m");
        assertEquals(q(Double.NaN, "m"), q(Double.NaN, "m"));
    }
}
