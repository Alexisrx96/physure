package com.physure;

import java.util.Arrays;
import java.util.List;

/**
 * Represents a vector array of physical quantities with a shared physical unit.
 * E.g. new QuantityVector(new double[]{10, 20, 30}, "m/s")
 * Compatible with Java 8+.
 */
public class QuantityVector {
    private final double[] values;
    private final String unit;

    public QuantityVector(double[] values, String unit) {
        if (values == null) {
            throw new IllegalArgumentException("Values array cannot be null.");
        }
        this.values = values.clone();
        this.unit = unit != null ? unit.trim() : "";
    }

    public QuantityVector(List<Double> values, String unit) {
        if (values == null) {
            throw new IllegalArgumentException("Values list cannot be null.");
        }
        this.values = values.stream().mapToDouble(Double::doubleValue).toArray();
        this.unit = unit != null ? unit.trim() : "";
    }

    public double[] getValues() {
        return values.clone();
    }

    public String getUnit() {
        return unit;
    }

    public int length() {
        return values.length;
    }

    public QuantityVector pow(double power) {
        double[] newVals = new double[values.length];
        for (int i = 0; i < values.length; i++) {
            newVals[i] = Math.pow(values[i], power);
        }
        return new QuantityVector(newVals, unit);
    }

    public QuantityVector add(Quantity q) {
        double[] newVals = new double[values.length];
        for (int i = 0; i < values.length; i++) {
            newVals[i] = values[i] + q.getValue();
        }
        return new QuantityVector(newVals, unit);
    }

    public QuantityVector add(QuantityVector other) {
        double[] newVals = new double[values.length];
        double[] oVals = other.getValues();
        for (int i = 0; i < values.length; i++) {
            newVals[i] = values[i] + (i < oVals.length ? oVals[i] : 0.0);
        }
        return new QuantityVector(newVals, unit);
    }

    public QuantityVector multiply(Quantity q) {
        double[] newVals = new double[values.length];
        for (int i = 0; i < values.length; i++) {
            newVals[i] = values[i] * q.getValue();
        }
        return new QuantityVector(newVals, unit.isEmpty() ? q.getUnit() : (q.getUnit().isEmpty() ? unit : unit + " * " + q.getUnit()));
    }

    public QuantityVector gradient(QuantityVector t) {
        double[] g = new double[values.length];
        if (values.length > 1) {
            g[0] = (values[1] - values[0]) / (t.values[1] - t.values[0]);
            for (int i = 1; i < values.length - 1; i++) {
                g[i] = (values[i + 1] - values[i - 1]) / (t.values[i + 1] - t.values[i - 1]);
            }
            g[values.length - 1] = (values[values.length - 1] - values[values.length - 2]) / (t.values[values.length - 1] - t.values[values.length - 2]);
        }
        return new QuantityVector(g, this.unit.isEmpty() ? t.unit : (t.unit.isEmpty() ? this.unit : this.unit + " / " + t.unit));
    }

    public Quantity trapz(QuantityVector x) {
        double area = 0.0;
        for (int i = 0; i < values.length - 1; i++) {
            double dx = x.values[i + 1] - x.values[i];
            area += 0.5 * (values[i] + values[i + 1]) * dx;
        }
        return new Quantity(area, this.unit.isEmpty() ? x.unit : (x.unit.isEmpty() ? this.unit : this.unit + " * " + x.unit));
    }

    public static QuantityVector linspace(Quantity start, Quantity end, int count) {
        double s = start.getValue();
        double e = end.getValue();
        double step = (count > 1) ? (e - s) / (count - 1) : 0;
        double[] vals = new double[count];
        for (int i = 0; i < count; i++) {
            vals[i] = s + i * step;
        }
        return new QuantityVector(vals, start.getUnit());
    }

    public QuantityVector map(java.util.function.Function<Quantity, Quantity> mapper) {
        double[] newVals = new double[values.length];
        String newUnit = this.unit;
        for (int i = 0; i < values.length; i++) {
            Quantity res = mapper.apply(new Quantity(values[i], this.unit));
            newVals[i] = res.getValue();
            newUnit = res.getUnit();
        }
        return new QuantityVector(newVals, newUnit);
    }

    public static String plot(QuantityVector x, QuantityVector y) {
        return "[PLOT_IMAGE: Live Plot generated for " + x.length() + " data points]";
    }

    public QuantityVector to(String targetUnit) {
        double[] newVals = new double[values.length];
        for (int i = 0; i < values.length; i++) {
            newVals[i] = new Quantity(values[i], unit).to(targetUnit).getValue();
        }
        return new QuantityVector(newVals, targetUnit);
    }

    @Override
    public String toString() {
        StringBuilder sb = new StringBuilder();
        sb.append("[");
        for (int i = 0; i < values.length; i++) {
            if (i > 0) sb.append(", ");
            sb.append(values[i]);
        }
        sb.append("] ").append(unit);
        return sb.toString();
    }
}
