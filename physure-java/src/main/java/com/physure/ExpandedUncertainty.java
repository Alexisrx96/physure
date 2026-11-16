package com.physure;

import java.math.BigDecimal;
import java.math.MathContext;
import java.util.Locale;

/**
 * An expanded uncertainty with everything needed to say how it was obtained (JCGM 100:2008 6.2, G.4).
 * <p>
 * It is a result of its own, not a {@link Quantity} with an uncertainty: the uncertainty of a quantity is a
 * standard uncertainty, and this one is {@code U = k * u_c} for a coverage probability {@code p}, with the
 * coverage factor {@code k} taken from the Student-t distribution at the effective degrees of freedom
 * (Welch-Satterthwaite, truncated to an integer as the Guide recommends). Both the value and the
 * uncertainties are in {@link #getUnit()}.
 * Compatible with Java 8+.
 */
public final class ExpandedUncertainty {
    private final double value;
    private final double standardUncertainty;
    private final double expandedUncertainty;
    private final double coverageFactor;
    private final double coverageProbability;
    private final double effectiveDof;
    private final String unit;

    ExpandedUncertainty(double value, double standardUncertainty, double expandedUncertainty, double coverageFactor,
                        double coverageProbability, double effectiveDof, String unit) {
        this.value = value;
        this.standardUncertainty = standardUncertainty;
        this.expandedUncertainty = expandedUncertainty;
        this.coverageFactor = coverageFactor;
        this.coverageProbability = coverageProbability;
        this.effectiveDof = effectiveDof;
        this.unit = unit;
    }

    /** The best estimate of the measurand. */
    public double getValue() {
        return value;
    }

    /** The combined standard uncertainty {@code u_c}. */
    public double getStandardUncertainty() {
        return standardUncertainty;
    }

    /** The expanded uncertainty {@code U = k * u_c}. */
    public double getExpandedUncertainty() {
        return expandedUncertainty;
    }

    /** The coverage factor {@code k}. */
    public double getCoverageFactor() {
        return coverageFactor;
    }

    /** The coverage probability {@code p}, strictly between 0 and 1. */
    public double getCoverageProbability() {
        return coverageProbability;
    }

    /** The effective degrees of freedom of {@code u_c}; infinite when no source declared a finite number. */
    public double getEffectiveDof() {
        return effectiveDof;
    }

    public String getUnit() {
        return unit;
    }

    /** {@code (value ± U) unit (k=..., p=... %)}, rounded as the Guide presents a result. */
    @Override
    public String toString() {
        String[] shown = NativeEngine.gumRound(value, expandedUncertainty);
        String probability = BigDecimal.valueOf(coverageProbability * 100).round(new MathContext(6)).stripTrailingZeros().toPlainString();
        return "(" + shown[0] + " ± " + shown[1] + ")" + (unit.isEmpty() ? "" : " " + unit)
            + " (k=" + String.format(Locale.ROOT, "%.2f", coverageFactor) + ", p=" + probability + " %)";
    }
}
