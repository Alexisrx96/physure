package com.physure;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Represents a vector of physical quantities in an N-dimensional space (Order 1 Tensor).
 * Compatible with Java 8+.
 */
public class QuantityVector {
    private final List<Quantity> components;

    public QuantityVector(List<Quantity> components) {
        if (components == null || components.isEmpty()) {
            throw new IllegalArgumentException("QuantityVector components cannot be null or empty");
        }
        this.components = Collections.unmodifiableList(new ArrayList<>(components));
    }

    public static QuantityVector of(Quantity... components) {
        List<Quantity> list = new ArrayList<>();
        Collections.addAll(list, components);
        return new QuantityVector(list);
    }

    public int size() {
        return components.size();
    }

    public Quantity get(int index) {
        return components.get(index);
    }

    public List<Quantity> getComponents() {
        return components;
    }

    /**
     * The dot product. The components of two different vectors are independent measurements, so their
     * uncertainties combine as such; a vector dotted with itself is not: {@code x * x} is {@code x^2}, whose
     * uncertainty is {@code 2 x u} and not the {@code sqrt(2) x u} of two independent factors.
     */
    public Quantity dot(QuantityVector other) {
        if (size() != other.size()) {
            throw new IllegalArgumentException("Vector dimension mismatch in dot product");
        }
        Quantity sum = product(components.get(0), other.get(0));
        for (int i = 1; i < size(); i++) {
            sum = sum.add(product(components.get(i), other.get(i)));
        }
        return sum;
    }

    /** One component times another: a quantity times itself is its square, fully correlated with itself. */
    private static Quantity product(Quantity a, Quantity b) {
        return a == b ? a.pow(2) : a.mul(b);
    }

    public QuantityVector cross(QuantityVector other) {
        if (size() != 3 || other.size() != 3) {
            throw new IllegalArgumentException("Cross product requires 3D vectors");
        }
        Quantity a1 = get(0), a2 = get(1), a3 = get(2);
        Quantity b1 = other.get(0), b2 = other.get(1), b3 = other.get(2);

        Quantity c1 = a2.mul(b3).sub(a3.mul(b2));
        Quantity c2 = a3.mul(b1).sub(a1.mul(b3));
        Quantity c3 = a1.mul(b2).sub(a2.mul(b1));

        return QuantityVector.of(c1, c2, c3);
    }

    /** The length: {@code sqrt(x1^2 + x2^2 + ...)}, each square taken as a square of one measurement. */
    public Quantity norm() {
        return dot(this).pow(0.5);
    }

    public QuantityVector unitVector() {
        Quantity n = norm();
        List<Quantity> units = new ArrayList<>();
        for (Quantity c : components) {
            units.add(c.div(n));
        }
        return new QuantityVector(units);
    }

    @Override
    public String toString() {
        StringBuilder sb = new StringBuilder("QuantityVector([");
        for (int i = 0; i < components.size(); i++) {
            if (i > 0) sb.append(", ");
            sb.append(components.get(i));
        }
        sb.append("])");
        return sb.toString();
    }
}
