package com.physure;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import org.junit.jupiter.api.Test;

/**
 * The effective degrees of freedom, coverage factor and expanded uncertainty of a sum of independent
 * measurements (JCGM 100:2008 G.4), as {@code tests/golden/uncertainty_dof.tsv} gives them: computed
 * independently with scipy by {@code scripts/gen_uncertainty_dof_golden.py}, and the file that
 * {@code physure-core} and the WebAssembly binding are held to as well.
 */
class UncertaintyDofGoldenTest {
    private static final Path GOLDEN = Paths.get("..", "tests", "golden", "uncertainty_dof.tsv");

    private static double number(String text) {
        return text.equals("inf") ? Double.POSITIVE_INFINITY : Double.parseDouble(text);
    }

    private static boolean close(double actual, double expected, double relative) {
        return actual == expected || Math.abs((actual - expected) / expected) < relative;
    }

    /** The sum of the measurements of a budget ({@code mean:u:nu;...}), each one declared with its degrees of freedom. */
    private static Quantity sumOf(String budget) {
        Quantity sum = null;
        for (String term : budget.split(";")) {
            String[] parts = term.split(":");
            Quantity measurement = new Quantity(number(parts[0]), number(parts[1]), "m");
            if (!Double.isInfinite(number(parts[2]))) {
                measurement = measurement.withDof(number(parts[2]));
            }
            sum = sum == null ? measurement : sum.add(measurement);
        }
        return sum;
    }

    @Test
    void everyRowOfTheGoldenFileIsReproduced() throws IOException {
        int rows = 0;
        for (String line : Files.readAllLines(GOLDEN, StandardCharsets.UTF_8)) {
            if (line.startsWith("#") || line.trim().isEmpty()) {
                continue;
            }
            String[] f = line.split("\t");
            String name = f[0];
            double p = number(f[2]);
            Quantity sum = sumOf(f[1]);
            rows++;
            assertTrue(close(sum.getUncertainty(), number(f[5]), 1e-12), name + ": u_c " + sum.getUncertainty() + " vs " + f[5]);
            assertTrue(close(sum.effectiveDof(), number(f[3]), 1e-12), name + ": nu_eff " + sum.effectiveDof() + " vs " + f[3]);
            assertTrue(close(sum.coverageFactor(p), number(f[4]), 1e-9), name + ": k " + sum.coverageFactor(p) + " vs " + f[4]);
            ExpandedUncertainty expanded = sum.expanded(p);
            assertTrue(close(expanded.getExpandedUncertainty(), number(f[6]), 1e-9),
                name + ": U " + expanded.getExpandedUncertainty() + " vs " + f[6]);
            assertEquals(sum.getValue(), expanded.getValue(), 0.0);
            assertEquals(p, expanded.getCoverageProbability(), 0.0);
        }
        assertEquals(11, rows, "the golden file should hold its rows");
    }
}
