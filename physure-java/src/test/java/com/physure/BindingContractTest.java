package com.physure;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.DynamicTest;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.TestFactory;

/**
 * What the core says about a unit is what the Java binding returns (F3).
 *
 * <p>Reads {@code tests/golden/unit_dimension_scale.tsv}, the file that {@code physure-core} and the
 * WASM binding are held to as well, and checks the exponents and the scale after they cross JNI.
 * Needs the native library: {@code cargo build -p physure-java}, then {@code mvn test} from this directory.
 */
class BindingContractTest {
    private static final Path GOLDEN = Paths.get("..", "tests", "golden", "unit_dimension_scale.tsv");

    private static UnitRegistry registry;

    @BeforeAll
    static void openRegistry() {
        registry = new UnitRegistry();
    }

    @AfterAll
    static void closeRegistry() {
        registry.close();
    }

    /** {@code symbol:exponent;...} as the golden file writes it; {@code -} is no exponents at all. */
    private static Map<String, Double> expectedExponents(String field) {
        Map<String, Double> exponents = new HashMap<>();
        if (!field.equals("-")) {
            for (String pair : field.split(";")) {
                String[] parts = pair.split(":");
                exponents.put(parts[0], Double.parseDouble(parts[1]));
            }
        }
        return exponents;
    }

    @TestFactory
    List<DynamicTest> everyRowOfTheGoldenFileComesBackUnchanged() throws IOException {
        List<DynamicTest> tests = new ArrayList<>();
        for (String line : Files.readAllLines(GOLDEN, StandardCharsets.UTF_8)) {
            if (line.startsWith("#") || line.trim().isEmpty()) {
                continue;
            }
            String[] fields = line.split("\t");
            String expression = fields[0];
            double scale = Double.parseDouble(fields[1]);
            Map<String, Double> exponents = expectedExponents(fields[2]);

            tests.add(DynamicTest.dynamicTest(expression + " scale", () ->
                    assertEquals(scale, registry.getUnitScale(expression), 0.0, expression)));
            tests.add(DynamicTest.dynamicTest(expression + " exponents", () -> {
                assertEquals(exponents, registry.getUnitPowers(expression), expression);
            }));
        }
        assertTrue(tests.size() >= 80, "the golden file should hold its rows, found " + tests.size() / 2);
        return tests;
    }

    @Test
    void wholeExponentsComeBackAsIntegers() {
        Map<String, Integer> exponents = registry.getUnitExponents("kg*m/s^2");
        assertEquals(1, exponents.get("kg"));
        assertEquals(1, exponents.get("m"));
        assertEquals(-2, exponents.get("s"));
    }

    @Test
    void aFractionalPowerIsRefusedInsteadOfTruncatedToItsNumerator() {
        // s^0.5 is s^(1/2): answering {s=1} would be a wrong dimension
        PhysureException e = assertThrows(PhysureException.class, () -> registry.getUnitExponents("s^0.5"));
        assertTrue(e.getMessage().contains("getUnitPowers"), e.getMessage());
    }
}
