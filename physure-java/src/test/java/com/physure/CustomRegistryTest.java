package com.physure;

import static org.junit.jupiter.api.Assertions.assertEquals;
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
     * PHS evaluation resolves units through the embedded catalog (the interpreter has one catalog per process),
     * so on a registry that gives a unit of it another meaning it would answer with the old one while
     * getUnitScale answers with the new: the registry would disagree with itself without a word.
     */
    @Test
    void aRegistryThatRedefinesAUnitRefusesPhsEvaluationInsteadOfUsingTheOldMeaning() {
        try (UnitRegistry registry = UnitRegistry.fromContent("[Units]\nkm = 7.0, L, [km]\n")) {
            assertEquals(7.0, registry.getUnitScale("km"), 0.0);
            PhysureException refused = assertThrows(PhysureException.class, () -> registry.evaluate("1 km"));
            assertTrue(refused.getMessage().contains("redefines km"), refused.getMessage());
            assertThrows(PhysureException.class, () -> registry.evaluateRaw("1 m"));
            assertThrows(PhysureException.class, () -> registry.solve("x = 2 * y", "x"));
            assertThrows(PhysureException.class, () -> new PhyFunction(registry, "f", "f(x) = x"));
        }
    }

    @Test
    void aRegistryThatOnlyAddsUnitsStillEvaluatesAndFailsLoudlyOnTheNewOnes() {
        try (UnitRegistry registry = UnitRegistry.fromContent(FURLONG)) {
            assertEquals(3.0, registry.evaluate("1 m + 2 m").getValue());
            assertEquals(201.168, registry.getUnitScale("fur"), 0.0);
            PhysureException unknown = assertThrows(PhysureException.class, () -> registry.evaluate("5 fur"));
            assertTrue(unknown.getMessage().contains("Unknown unit"), unknown.getMessage());
        }
    }

    @Test
    void aDefinitionWithTheSameValueRedefinesNothing() {
        try (UnitRegistry registry = UnitRegistry.fromContent("[Units]\nfoot = 0.3048, L, [ft, foot, feet]\n")) {
            assertEquals(1.0, registry.evaluate("1 ft").to("ft").getValue(), 1e-12);
        }
    }
}
