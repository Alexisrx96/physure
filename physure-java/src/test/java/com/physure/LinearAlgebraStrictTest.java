package com.physure;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.util.Arrays;
import java.util.List;
import org.junit.jupiter.api.Test;

/** Strict tests of {@link QuantityVector} and {@link QuantityMatrix}: units follow every product and sum. */
class LinearAlgebraStrictTest {
    private static Quantity q(double value, String unit) {
        return new Quantity(value, unit);
    }

    private static QuantityVector v(Quantity... components) {
        return QuantityVector.of(components);
    }

    private static QuantityMatrix m(List<Quantity>... rows) {
        return new QuantityMatrix(Arrays.asList(rows));
    }

    private static List<Quantity> row(Quantity... cells) {
        return Arrays.asList(cells);
    }

    private static void near(double expected, Quantity actual) {
        assertEquals(expected, actual.getValue(), 1e-9 * Math.max(1, Math.abs(expected)));
    }

    // ---- vectors

    @Test
    void aDotProductHasTheUnitOfTheProductsAndSumsThemInOneScale() {
        Quantity dot = v(q(1, "m"), q(2, "m")).dot(v(q(3, "m"), q(4, "m")));
        near(11, dot);
        assertEquals("m^2", dot.getUnit());
        // 1 m * 1 m + 2 km * 1 m = 1 m^2 + 2000 m^2
        Quantity mixed = v(q(1, "m"), q(2, "km")).dot(v(q(1, "m"), q(1, "m")));
        near(2001, mixed);
        assertEquals("m^2", mixed.getUnit());
    }

    @Test
    void aDotProductOfDifferentDimensionsIsAnError() {
        assertThrows(PhysureException.class, () -> v(q(1, "m"), q(2, "s")).dot(v(q(3, "m"), q(4, "s"))));
        assertThrows(IllegalArgumentException.class, () -> v(q(1, "m")).dot(v(q(1, "m"), q(2, "m"))));
    }

    @Test
    void aCrossProductHasTheRightComponentsAndUnits() {
        QuantityVector cross = v(q(1, "m"), q(2, "m"), q(3, "m")).cross(v(q(4, "m"), q(5, "m"), q(6, "m")));
        near(-3, cross.get(0));
        near(6, cross.get(1));
        near(-3, cross.get(2));
        for (int i = 0; i < 3; i++) assertEquals("m^2", cross.get(i).getUnit());
        QuantityVector areaRate = v(q(1, "m"), q(0, "m"), q(0, "m")).cross(v(q(0, "m/s"), q(1, "m/s"), q(0, "m/s")));
        near(1, areaRate.get(2));
        near(1, areaRate.get(2).to("m^2/s"));
        assertThrows(IllegalArgumentException.class, () -> v(q(1, "m"), q(2, "m")).cross(v(q(1, "m"), q(2, "m"))));
    }

    @Test
    void aNormIsTheLengthInTheUnitOfTheComponents() {
        Quantity norm = v(q(3, "m"), q(4, "m")).norm();
        near(5, norm);
        assertEquals("m", norm.getUnit());
        near(5000, v(q(3, "km"), q(4, "km")).norm().to("m"));
    }

    @Test
    void aNormAndASelfDotProductKeepTheUncertaintyOfTheirComponents() {
        QuantityVector measured = v(new Quantity(3, 0.1, "m"), new Quantity(4, 0.1, "m"));
        // first order: sqrt((3*0.1)^2 + (4*0.1)^2) / 5 = 0.1, and 2 * sqrt((3*0.1)^2 + (4*0.1)^2) = 1.0 for the square.
        // Two independent factors would give 0.0707 and 0.707: x * x is x^2, not a product of two measurements.
        assertEquals(0.1, measured.norm().getUncertainty(), 1e-9);
        assertEquals(1.0, measured.dot(measured).getUncertainty(), 1e-9);
        // a different vector with the same numbers is a different measurement
        QuantityVector other = v(new Quantity(3, 0.1, "m"), new Quantity(4, 0.1, "m"));
        assertEquals(Math.sqrt(0.5), measured.dot(other).getUncertainty(), 1e-9);
    }

    @Test
    void aUnitVectorIsDimensionlessAndTheZeroVectorHasNone() {
        QuantityVector unit = v(q(3, "m"), q(4, "m")).unitVector();
        near(0.6, unit.get(0));
        near(0.8, unit.get(1));
        assertEquals("", unit.get(0).getUnit());
        assertThrows(PhysureException.class, () -> v(q(0, "m"), q(0, "m")).unitVector());
    }

    @Test
    void aVectorNeedsComponentsAndKeepsThemUnchanged() {
        assertThrows(IllegalArgumentException.class, () -> QuantityVector.of());
        assertThrows(IllegalArgumentException.class, () -> new QuantityVector(null));
        QuantityVector vector = v(q(1, "m"));
        assertThrows(UnsupportedOperationException.class, () -> vector.getComponents().add(q(2, "m")));
        assertEquals(1, vector.size());
    }

    @Test
    void aScalarTimesAVectorScalesEveryComponent() {
        QuantityVector scaled = q(2, "s").multiply(v(q(1, "m"), q(2, "m")));
        near(2, scaled.get(0));
        near(4, scaled.get(1));
        near(2, scaled.get(0).to("m*s"));
    }

    // ---- matrices

    @Test
    void aTwoByTwoDeterminantHasTheUnitOfTheProducts() {
        Quantity det = m(row(q(1, "m"), q(2, "m")), row(q(3, "m"), q(4, "m"))).det();
        near(-2, det);
        assertEquals("m^2", det.getUnit());
    }

    @Test
    void aThreeByThreeDeterminantIsComputed() {
        QuantityMatrix matrix = m(row(q(2, "m"), q(0, "m"), q(1, "m")), row(q(1, "m"), q(3, "m"), q(2, "m")), row(q(1, "m"), q(1, "m"), q(4, "m")));
        Quantity det = matrix.det();
        near(18, det);
        assertEquals("m^3", det.getUnit());
    }

    @Test
    void aDeterminantNeedsASquareMatrix() {
        assertThrows(IllegalArgumentException.class, () -> m(row(q(1, "m"), q(2, "m"))).det());
    }

    @Test
    void aProductOfMatricesCarriesTheUnitsOfBoth() {
        QuantityMatrix product = m(row(q(1, "m"), q(2, "m"))).matmul(m(row(q(1, "s")), row(q(2, "s"))));
        near(5, product.get(0, 0));
        assertEquals(1, product.getRows());
        assertEquals(1, product.getCols());
        assertThrows(IllegalArgumentException.class, () -> m(row(q(1, "m"), q(2, "m"))).matmul(m(row(q(1, "s")))));
    }

    @Test
    void aTransposeSwapsRowsAndColumns() {
        QuantityMatrix t = m(row(q(1, "m"), q(2, "m"), q(3, "m")), row(q(4, "m"), q(5, "m"), q(6, "m"))).transpose();
        assertEquals(3, t.getRows());
        assertEquals(2, t.getCols());
        near(4, t.get(0, 1));
        near(3, t.get(2, 0));
    }

    @Test
    void aMatrixNeedsEqualRowsAndSomeCells() {
        assertThrows(IllegalArgumentException.class, () -> new QuantityMatrix(null));
        assertThrows(IllegalArgumentException.class, () -> new QuantityMatrix(Arrays.<List<Quantity>>asList()));
        assertThrows(IllegalArgumentException.class, () -> m(row(q(1, "m"), q(2, "m")), row(q(1, "m"))));
    }
}
