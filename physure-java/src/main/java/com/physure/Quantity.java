package com.physure;

import java.util.ArrayList;
import java.util.List;

/**
 * Representation of a physical Quantity in Java.
 * Compatible with Java 8+.
 */
public class Quantity {
    private final double value;
    private final double uncertainty;
    private final String unit;

    public Quantity(double value, String unit) {
        this(value, 0.0, unit);
    }

    public Quantity(double value, double uncertainty, String unit) {
        this.value = value;
        this.uncertainty = uncertainty;
        this.unit = unit != null ? unit.trim() : "";
    }

    public double getValue() {
        return value;
    }

    public double getUncertainty() {
        return uncertainty;
    }

    public String getUnit() {
        return unit;
    }

    /**
     * The uncertainty of a sum or a difference of two independent measurements.
     * <p>
     * They add in quadrature, not linearly: two half-widths of 0.3 give 0.42, not 0.6.
     * Adding them straight is what a worst-case bound does, and it reported an error
     * about 40% too large for every quantity Java touched, disagreeing with the same
     * calculation in Rust and Python.
     * <p>
     * {@code Math.hypot} rather than {@code Math.sqrt(a*a + b*b)} so a large uncertainty
     * cannot overflow on its way to a representable answer.
     */
    static double combineUncertainty(double a, double b) {
        return Math.hypot(a, b);
    }

    /**
     * Reports that an operation across two units needs the native engine to convert them.
     * Adding the raw magnitudes instead — which is what this used to do — silently drops
     * the conversion factor, and 1 km + 1 m is not 2 of anything.
     */
    private PhysureException conversionUnavailable(String op, Quantity other, Exception cause) {
        return new PhysureException(
            "Cannot " + op + " " + this.unit + " and " + other.getUnit()
                + " without the native engine: the conversion between them lives there, and "
                + "combining the magnitudes as they stand would ignore it.",
            cause);
    }

    public Quantity add(Quantity other) {
        if (!this.unit.equals(other.getUnit()) && !this.unit.isEmpty() && !other.getUnit().isEmpty()) {
            try {
                return NativeEngine.addQuantities(this, other.to(this.unit));
            } catch (Exception e) {
                throw conversionUnavailable("add", other, e);
            }
        }
        try {
            return NativeEngine.addQuantities(this, other);
        } catch (Exception e) {
            return new Quantity(this.value + other.getValue(),
                combineUncertainty(this.uncertainty, other.getUncertainty()), this.unit);
        }
    }

    public Quantity sub(Quantity other) {
        return subtract(other);
    }

    public Quantity subtract(Quantity other) {
        if (!this.unit.equals(other.getUnit()) && !this.unit.isEmpty() && !other.getUnit().isEmpty()) {
            try {
                return NativeEngine.subQuantities(this, other.to(this.unit));
            } catch (Exception e) {
                throw conversionUnavailable("subtract", other, e);
            }
        }
        try {
            return NativeEngine.subQuantities(this, other);
        } catch (Exception e) {
            return new Quantity(this.value - other.getValue(),
                combineUncertainty(this.uncertainty, other.getUncertainty()), this.unit);
        }
    }

    public Quantity mul(Quantity other) {
        return multiply(other);
    }

    public Quantity multiply(Quantity other) {
        return NativeEngine.mulQuantities(this, other);
    }

    public Quantity multiply(double scalar) {
        return new Quantity(this.value * scalar, this.uncertainty * scalar, this.unit);
    }

    public Quantity div(Quantity other) {
        return divide(other);
    }

    public Quantity divide(Quantity other) {
        return NativeEngine.divQuantities(this, other);
    }

    public Quantity divide(double scalar) {
        return new Quantity(this.value / scalar, this.uncertainty / scalar, this.unit);
    }

    public QuantityVector multiply(QuantityVector vec) {
        List<Quantity> newComps = new ArrayList<>();
        for (int i = 0; i < vec.size(); i++) {
            newComps.add(this.mul(vec.get(i)));
        }
        return new QuantityVector(newComps);
    }

    public QuantityVector divide(QuantityVector vec) {
        List<Quantity> newComps = new ArrayList<>();
        for (int i = 0; i < vec.size(); i++) {
            newComps.add(this.div(vec.get(i)));
        }
        return new QuantityVector(newComps);
    }

    public Quantity pow(double power) {
        return NativeEngine.powQuantity(this, power);
    }

    public Quantity sqrt() {
        return pow(0.5);
    }

    public Quantity sin() {
        return new Quantity(Math.sin(this.value), "");
    }

    public Quantity cos() {
        return new Quantity(Math.cos(this.value), "");
    }

    public Quantity tan() {
        return new Quantity(Math.tan(this.value), "");
    }

    public Quantity abs() {
        return new Quantity(Math.abs(this.value), this.uncertainty, this.unit);
    }

    public Quantity round(int decimals) {
        double factor = Math.pow(10, decimals);
        double roundedVal = Math.round(this.value * factor) / factor;
        double roundedUnc = Math.round(this.uncertainty * factor) / factor;
        return new Quantity(roundedVal, roundedUnc, this.unit);
    }

    public boolean greaterThan(Quantity other) {
        Quantity converted = other.to(this.unit);
        return this.value > converted.getValue();
    }

    public boolean lessThan(Quantity other) {
        Quantity converted = other.to(this.unit);
        return this.value < converted.getValue();
    }

    public boolean approxEquals(Quantity other) {
        Quantity converted = other.to(this.unit);
        return Math.abs(this.value - converted.getValue()) < 1e-6;
    }

    /**
     * PHS's {@code assert(actual, expected)}: passes when both quantities have compatible
     * dimensions and their magnitudes agree after unit conversion, within a fixed
     * tolerance. Throws {@link PhysureException} on failure — delegates to the same
     * {@code physure-core} comparison every other language target uses, rather than
     * reimplementing the tolerance logic here.
     */
    public void physAssert(Quantity expected) {
        NativeEngine.assertQuantities(this, expected);
    }

    /**
     * PHS's {@code exact_assert(actual, expected)}: passes only when both quantities carry
     * the literal same unit (aliases like {@code m}/{@code meter} still match) and the
     * magnitudes are bit-exact. Throws {@link PhysureException} on failure.
     */
    public void physExactAssert(Quantity expected) {
        NativeEngine.assertExactQuantities(this, expected);
    }

    public Quantity to(String targetUnit) {
        return NativeEngine.convertQuantity(this, targetUnit);
    }

    public Quantity convertTo(String targetUnit) {
        return to(targetUnit);
    }

    public Quantity convertTo(Quantity target) {
        return to(target.getUnit());
    }

    public static Quantity of(double value) {
        return new Quantity(value, "");
    }

    public static Quantity of(double value, String unit) {
        return new Quantity(value, unit);
    }

    public static Quantity withUncertainty(double value, double uncertainty, String unit) {
        return new Quantity(value, uncertainty, unit);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (o == null || getClass() != o.getClass()) return false;
        Quantity quantity = (Quantity) o;
        return Double.compare(quantity.value, value) == 0 &&
               Double.compare(quantity.uncertainty, uncertainty) == 0 &&
               unit.equals(quantity.unit);
    }

    @Override
    public int hashCode() {
        int result;
        long temp;
        temp = Double.doubleToLongBits(value);
        result = (int) (temp ^ (temp >>> 32));
        temp = Double.doubleToLongBits(uncertainty);
        result = 31 * result + (int) (temp ^ (temp >>> 32));
        result = 31 * result + unit.hashCode();
        return result;
    }

    @Override
    public String toString() {
        if (uncertainty != 0.0) {
            return value + " +/- " + uncertainty + (unit.isEmpty() ? "" : " " + unit);
        }
        return value + (unit.isEmpty() ? "" : " " + unit);
    }

    /**
     * Parses a string representation of a physical quantity (e.g. "125.0 J" or "5.0 m/s") into a Quantity.
     * Compatible with Java 8+.
     */
    public static Quantity parse(String str) {
        if (str == null) {
            throw new IllegalArgumentException("Quantity string cannot be null.");
        }
        str = str.trim();
        if (str.isEmpty() || str.equals("None")) {
            return new Quantity(0.0, "");
        }
        int spaceIdx = str.indexOf(' ');
        if (spaceIdx == -1) {
            try {
                double val = Double.parseDouble(str);
                return new Quantity(val, "");
            } catch (NumberFormatException e) {
                if (str.equalsIgnoreCase("true")) return new Quantity(1.0, "");
                if (str.equalsIgnoreCase("false")) return new Quantity(0.0, "");
                return new Quantity(1.0, str);
            }
        } else {
            try {
                double val = Double.parseDouble(str.substring(0, spaceIdx));
                String unit = str.substring(spaceIdx + 1);
                return new Quantity(val, unit);
            } catch (NumberFormatException e) {
                return new Quantity(1.0, str);
            }
        }
    }
}
