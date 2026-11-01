package com.physure;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Collections;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/**
 * A {@link UnitRegistry} answers from the catalog it was built from: the embedded one plus the conf text
 * or file it was given. The native side used to rebuild the embedded catalog on every call and ignore the
 * handle, so a unit defined in a custom conf was "unknown" and every call cost about 0.8 ms.
 */
class CustomRegistryTest {
    private static final String ONE_KM_IN_METRES = "1 km => m";
    private static final String FURLONG = "[Units]\nfurlong = 201.168, L, [fur, furlong]\n";

    @Test
    void aRegistryBuiltFromContentKnowsTheUnitsItWasGiven() {
        try (UnitRegistry registry = UnitRegistry.fromContent(FURLONG)) {
            assertEquals(201.168, registry.getUnitScale("fur"), 0.0);
            assertEquals(Collections.singletonMap("m", 1.0), registry.getUnitPowers("fur"));
            assertEquals(Collections.singletonMap("m", 2.0), registry.getUnitPowers("fur*fur"));
        }
    }

    @Test
    void itStillKnowsTheEmbeddedCatalog() {
        try (UnitRegistry registry = UnitRegistry.fromContent(FURLONG)) {
            assertEquals(1000.0, registry.getUnitScale("km"), 0.0);
        }
    }

    @Test
    void theDefaultRegistryDoesNotKnowAUnitOnlyAnotherRegistryWasGiven() {
        try (UnitRegistry custom = UnitRegistry.fromContent(FURLONG);
                UnitRegistry plain = new UnitRegistry()) {
            assertEquals(201.168, custom.getUnitScale("fur"), 0.0);
            assertThrows(PhysureException.class, () -> plain.getUnitScale("fur"));
        }
    }

    @Test
    void registriesDoNotShareDefinitions() {
        try (UnitRegistry short1 = UnitRegistry.fromContent("[Units]\nstep = 0.5, L, [step]\n");
                UnitRegistry long1 = UnitRegistry.fromContent("[Units]\nstep = 2.0, L, [step]\n")) {
            assertEquals(0.5, short1.getUnitScale("step"), 0.0);
            assertEquals(2.0, long1.getUnitScale("step"), 0.0);
        }
    }

    @Test
    void aRegistryBuiltFromAFileKnowsTheUnitsInIt(@TempDir Path dir) throws IOException {
        Path conf = dir.resolve("physure.conf");
        Files.write(conf, FURLONG.getBytes(StandardCharsets.UTF_8));
        try (UnitRegistry registry = new UnitRegistry(conf.toString())) {
            assertEquals(201.168, registry.getUnitScale("fur"), 0.0);
        }
    }

    @Test
    void aMissingConfFileIsAnError(@TempDir Path dir) {
        String missing = dir.resolve("nope.conf").toString();
        PhysureException e = assertThrows(PhysureException.class, () -> new UnitRegistry(missing));
        assertTrue(e.getMessage().contains("nope.conf"), e.getMessage());
    }

    /**
     * PHS evaluation reads unit text against the registry's own catalog: the parser deciding that a word after a
     * number is a unit, and the quantities built from unit text while the program runs. Before, a registry that
     * redefined a unit answered getUnitScale with its definition and evaluate with the embedded one.
     */
    @Test
    void evaluateReadsUnitsAgainstTheRegistrysOwnCatalog() {
        try (UnitRegistry registry = UnitRegistry.fromContent("[Units]\nkm = 7.0, L, [km]\nfurlong = 201.168, L, [fur, furlong]\n")) {
            assertEquals(7.0, registry.getUnitScale("km"), 0.0);
            assertEquals(7.0, registry.evaluate(ONE_KM_IN_METRES).getValue(), 1e-12);
            assertEquals(5 * 201.168, registry.evaluate("5 fur => m").getValue(), 1e-9);
            assertEquals(2 * 201.168, registry.evaluate("2 fur + 0 m => m").getValue(), 1e-9);
        }
    }

    @Test
    void aFunctionAndTheCalculusCallsAreEvaluatedForTheirRegistryToo() {
        try (UnitRegistry registry = UnitRegistry.fromContent(FURLONG)) {
            PhyFunction distance = new PhyFunction(registry, "distance", "distance(n) = n * 1 fur");
            assertEquals(3 * 201.168, registry.evaluate("distance(3) => m").getValue(), 1e-9);
            assertEquals(3.0, distance.call("3").getValue(), 1e-12);
            assertTrue(registry.deriv("x * 2", "x").contains("2"));
        }
    }

    @Test
    void theDefaultRegistryAndACustomOneEachAnswerWithTheirOwnMeaningInTurn() {
        try (UnitRegistry custom = UnitRegistry.fromContent("[Units]\nkm = 7.0, L, [km]\n"); UnitRegistry plain = new UnitRegistry()) {
            for (int i = 0; i < 3; i++) {
                assertEquals(7.0, custom.evaluate(ONE_KM_IN_METRES).getValue(), 1e-12);
                assertEquals(1000.0, plain.evaluate(ONE_KM_IN_METRES).getValue(), 1e-9);
            }
        }
    }

    @Test
    void threadsEvaluatingForDifferentRegistriesEachSeeTheirOwn() throws Exception {
        java.util.concurrent.ExecutorService pool = java.util.concurrent.Executors.newFixedThreadPool(4);
        try {
            java.util.List<java.util.concurrent.Future<Boolean>> results = new java.util.ArrayList<>();
            for (int t = 0; t < 4; t++) {
                final double km = 3.0 + t;
                results.add(pool.submit(() -> {
                    try (UnitRegistry own = UnitRegistry.fromContent("[Units]\nkm = " + km + ", L, [km]\n")) {
                        for (int i = 0; i < 100; i++) {
                            if (Math.abs(own.evaluate(ONE_KM_IN_METRES).getValue() - km) > 1e-9) return false;
                        }
                        return true;
                    }
                }));
            }
            for (java.util.concurrent.Future<Boolean> result : results) assertTrue(result.get(60, java.util.concurrent.TimeUnit.SECONDS));
        } finally {
            pool.shutdownNow();
        }
    }

    @Test
    void anUnknownUnitIsStillAnErrorOnARegistryThatDoesNotHaveIt() {
        try (UnitRegistry registry = UnitRegistry.fromContent(FURLONG)) {
            PhysureException unknown = assertThrows(PhysureException.class, () -> registry.evaluate("5 parsecs_of_nothing"));
            assertFalse(unknown.getMessage().isEmpty());
        }
    }

    @Test
    void aDefinitionWithTheSameValueRedefinesNothing() {
        try (UnitRegistry registry = UnitRegistry.fromContent("[Units]\nfoot = 0.3048, L, [ft, foot, feet]\n")) {
            assertEquals(1.0, registry.evaluate("1 ft").to("ft").getValue(), 1e-12);
        }
    }
}
