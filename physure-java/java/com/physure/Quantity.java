package com.physure;

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

    public Quantity add(Quantity other) {
        if (!this.unit.equals(other.getUnit()) && !this.unit.isEmpty() && !other.getUnit().isEmpty()) {
            try {
                return NativeEngine.addQuantities(this, other.to(this.unit));
            } catch (Exception e) {
                return new Quantity(this.value + other.getValue(), this.uncertainty + other.getUncertainty(), this.unit);
            }
        }
        try {
            return NativeEngine.addQuantities(this, other);
        } catch (Exception e) {
            return new Quantity(this.value + other.getValue(), this.uncertainty + other.getUncertainty(), this.unit);
        }
    }

    public Quantity subtract(Quantity other) {
        if (!this.unit.equals(other.getUnit()) && !this.unit.isEmpty() && !other.getUnit().isEmpty()) {
            try {
                return NativeEngine.subQuantities(this, other.to(this.unit));
            } catch (Exception e) {
                return new Quantity(this.value - other.getValue(), this.uncertainty + other.getUncertainty(), this.unit);
            }
        }
        try {
            return NativeEngine.subQuantities(this, other);
        } catch (Exception e) {
            return new Quantity(this.value - other.getValue(), this.uncertainty + other.getUncertainty(), this.unit);
        }
    }

    public Quantity multiply(Quantity other) {
        return NativeEngine.mulQuantities(this, other);
    }

    public Quantity multiply(double scalar) {
        return new Quantity(this.value * scalar, this.uncertainty * scalar, this.unit);
    }

    public Quantity divide(Quantity other) {
        return NativeEngine.divQuantities(this, other);
    }

    public Quantity divide(double scalar) {
        return new Quantity(this.value / scalar, this.uncertainty / scalar, this.unit);
    }

    public QuantityVector multiply(QuantityVector vec) {
        double[] newVals = new double[vec.length()];
        double[] vVals = vec.getValues();
        for (int i = 0; i < vec.length(); i++) {
            newVals[i] = this.value * vVals[i];
        }
        return new QuantityVector(newVals, this.unit.isEmpty() ? vec.getUnit() : (vec.getUnit().isEmpty() ? this.unit : this.unit + " * " + vec.getUnit()));
    }

    public QuantityVector divide(QuantityVector vec) {
        double[] newVals = new double[vec.length()];
        double[] vVals = vec.getValues();
        for (int i = 0; i < vec.length(); i++) {
            newVals[i] = this.value / vVals[i];
        }
        return new QuantityVector(newVals, this.unit.isEmpty() ? vec.getUnit() : (vec.getUnit().isEmpty() ? this.unit : this.unit + " / " + vec.getUnit()));
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

    public Quantity to(String targetUnit) {
        return NativeEngine.convertQuantity(this, targetUnit);
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
