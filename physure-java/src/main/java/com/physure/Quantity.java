package com.physure;

import java.math.BigDecimal;
import java.math.RoundingMode;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.function.DoubleUnaryOperator;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Representation of a physical Quantity in Java.
 * <p>
 * Arithmetic, powers and conversions are the native engine's: the core decides what a unit is, how two
 * units combine, how an uncertainty propagates and what a temperature point plus an interval is. This
 * class does not repeat any of that, and an operation the core refuses (incompatible dimensions,
 * a division by zero) is a {@link PhysureException}, never a number.
 * Compatible with Java 8+.
 */
public class Quantity {
    private static final String OTHER = "other";
    /**
     * A decimal number. Digits, then an optional fraction: {@code \d+\.?\d*} accepts the same text but splits a
     * run of digits between its two parts in every possible way when the rest of the pattern fails (quadratic).
     */
    private static final String NUMBER = "(?:\\d+(?:\\.\\d*)?|\\.\\d+)(?:[eE][+-]?\\d+)?";
    /** value, an optional uncertainty written {@code +/-} or the core's {@code ±}, and the unit if there is one. */
    private static final Pattern WRITTEN = Pattern.compile(
        "^([+-]?(?:" + NUMBER + "|NaN|Infinity))"
            + "(?:\\s*(?:±|\\+/-)\\s*(" + NUMBER + "))?"
            + "(?:\\s+(.*))?$",
        Pattern.DOTALL);

    private final double value;
    private final double uncertainty;
    private final String unit;

    public Quantity(double value, String unit) {
        this(value, 0.0, unit);
    }

    /**
     * @param uncertainty the standard uncertainty, a spread: its magnitude is kept and it must be finite
     *     ({@code NaN} and infinity are not a spread, and reading them as "none" would turn an unknown
     *     error into an exact value)
     * @throws IllegalArgumentException if the uncertainty is {@code NaN} or infinite
     */
    public Quantity(double value, double uncertainty, String unit) {
        if (Double.isNaN(uncertainty) || Double.isInfinite(uncertainty)) {
            throw new IllegalArgumentException("The uncertainty must be a finite number, got " + uncertainty);
        }
        this.value = value;
        this.uncertainty = Math.abs(uncertainty);
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

    public Quantity add(Quantity other) {
        return NativeEngine.addQuantities(this, Objects.requireNonNull(other, OTHER));
    }

    public Quantity sub(Quantity other) {
        return subtract(other);
    }

    public Quantity subtract(Quantity other) {
        return NativeEngine.subQuantities(this, Objects.requireNonNull(other, OTHER));
    }

    public Quantity mul(Quantity other) {
        return multiply(other);
    }

    public Quantity multiply(Quantity other) {
        return NativeEngine.mulQuantities(this, Objects.requireNonNull(other, OTHER));
    }

    /** Multiplies by a plain number: the core's rules apply (a plain number does not scale a temperature point). */
    public Quantity multiply(double scalar) {
        return multiply(new Quantity(scalar, ""));
    }

    public Quantity div(Quantity other) {
        return divide(other);
    }

    public Quantity divide(Quantity other) {
        return NativeEngine.divQuantities(this, Objects.requireNonNull(other, OTHER));
    }

    /** Divides by a plain number: dividing by zero is a {@link PhysureException}. */
    public Quantity divide(double scalar) {
        return divide(new Quantity(scalar, ""));
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

    /**
     * The sine of this angle. The angle is read in its unit (90 {@code deg} is 1) and a quantity that is not
     * an angle is a {@link PhysureException}; a plain number is taken as radians. The uncertainty of the
     * angle propagates: {@code |cos x| * u}.
     */
    public Quantity sin() {
        return trigonometric(Math::sin, Math::cos);
    }

    /** The cosine of this angle; see {@link #sin()}. */
    public Quantity cos() {
        return trigonometric(Math::cos, Math::sin);
    }

    /** The tangent of this angle; see {@link #sin()}. */
    public Quantity tan() {
        return trigonometric(Math::tan, x -> 1.0 / (Math.cos(x) * Math.cos(x)));
    }

    private Quantity trigonometric(DoubleUnaryOperator function, DoubleUnaryOperator derivative) {
        Quantity angle = unit.isEmpty() || unit.equals("unity") ? this : to("rad");
        double x = angle.value;
        return new Quantity(function.applyAsDouble(x), Math.abs(derivative.applyAsDouble(x)) * angle.uncertainty, "");
    }

    public Quantity abs() {
        return new Quantity(Math.abs(this.value), this.uncertainty, this.unit);
    }

    /**
     * Rounds the value and the uncertainty to {@code decimals} decimal places (half up); a negative count
     * rounds to tens, hundreds... {@code NaN} and infinity are returned as they are.
     */
    public Quantity round(int decimals) {
        return new Quantity(roundTo(this.value, decimals), roundTo(this.uncertainty, decimals), this.unit);
    }

    private static double roundTo(double x, int decimals) {
        if (Double.isNaN(x) || Double.isInfinite(x)) {
            return x;
        }
        // exact decimal arithmetic: scaling by 10^decimals and rounding to a long overflows for large values
        return new BigDecimal(x).setScale(decimals, RoundingMode.HALF_UP).doubleValue(); // NOSONAR java:S2111: the exact binary value is the point; valueOf would round the shortest decimal text instead
    }

    public boolean greaterThan(Quantity other) {
        Quantity converted = Objects.requireNonNull(other, OTHER).to(this.unit);
        return this.value > converted.getValue();
    }

    public boolean lessThan(Quantity other) {
        Quantity converted = Objects.requireNonNull(other, OTHER).to(this.unit);
        return this.value < converted.getValue();
    }

    /**
     * True when the two values differ by less than 1e-6 in this quantity's unit. The tolerance is absolute, so
     * it is blind to quantities of a very small magnitude (1e-9 m and 5e-9 m are "equal") and strict for a
     * very large one; {@link #physAssert(Quantity)} is the core's comparison.
     */
    public boolean approxEquals(Quantity other) {
        Quantity converted = Objects.requireNonNull(other, OTHER).to(this.unit);
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
        NativeEngine.assertQuantities(this, Objects.requireNonNull(expected, "expected"));
    }

    /**
     * PHS's {@code exact_assert(actual, expected)}: passes only when both quantities carry
     * the literal same unit (aliases like {@code m}/{@code meter} still match) and the
     * magnitudes are bit-exact. Throws {@link PhysureException} on failure.
     */
    public void physExactAssert(Quantity expected) {
        NativeEngine.assertExactQuantities(this, Objects.requireNonNull(expected, "expected"));
    }

    public Quantity to(String targetUnit) {
        return NativeEngine.convertQuantity(this, Objects.requireNonNull(targetUnit, "targetUnit"));
    }

    public Quantity convertTo(String targetUnit) {
        return to(targetUnit);
    }

    public Quantity convertTo(Quantity target) {
        return to(Objects.requireNonNull(target, "target").getUnit());
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
     * Parses the text of a physical quantity: {@code "125.0 J"}, {@code "5.0 m/s"}, {@code "1.5 +/- 0.1 m"} or
     * the core's {@code "1.5 ± 0.10 m"}. A bare unit ({@code "m/s"}) is one of it, and {@code true} and
     * {@code false} are 1 and 0. Text that starts like a number and is not one ({@code "1,5 m"}) is an error,
     * and not a quantity of 1 with a strange unit.
     * Compatible with Java 8+.
     *
     * @throws IllegalArgumentException if the text is {@code null} or starts with a number it cannot read
     */
    public static Quantity parse(String str) {
        if (str == null) {
            throw new IllegalArgumentException("Quantity string cannot be null.");
        }
        str = str.trim();
        if (str.isEmpty() || str.equals("None")) {
            return new Quantity(0.0, "");
        }
        Matcher written = WRITTEN.matcher(str);
        if (written.matches()) {
            double spread = written.group(2) == null ? 0.0 : Double.parseDouble(written.group(2));
            return new Quantity(Double.parseDouble(written.group(1)), spread, written.group(3));
        }
        if (str.equalsIgnoreCase("true")) return new Quantity(1.0, "");
        if (str.equalsIgnoreCase("false")) return new Quantity(0.0, "");
        char first = str.charAt(0);
        if (Character.isDigit(first) || first == '.' || first == '+' || first == '-') {
            throw new IllegalArgumentException("Not a quantity: '" + str + "'");
        }
        return new Quantity(1.0, str);
    }

    /**
     * This quantity as PHS source: the shortest decimal that reads back to the same number, then
     * {@code +/- uncertainty} if there is one, then the unit ({@code "2.0 +/- 0.1 m"}). It is what a function
     * or an equation is called with, so neither the precision nor the uncertainty is lost on the way.
     */
    String toPhs() {
        StringBuilder source = new StringBuilder(phsNumber(value));
        if (uncertainty != 0.0) {
            source.append(" +/- ").append(phsNumber(uncertainty));
        }
        if (!unit.isEmpty()) {
            source.append(' ').append(unit);
        }
        return source.toString();
    }

    private static String phsNumber(double x) {
        if (Double.isNaN(x) || Double.isInfinite(x)) {
            return Double.toString(x);
        }
        // no exponent: 1.0E-5 is not a number PHS reads
        return BigDecimal.valueOf(x).toPlainString();
    }
}
