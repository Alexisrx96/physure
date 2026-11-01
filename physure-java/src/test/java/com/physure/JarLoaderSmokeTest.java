package com.physure;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/**
 * The library a jar ships is found and loaded on the platform the test runs on.
 * <p>
 * The other tests load the library from {@code java.library.path}. A user of the jar has no such path: the
 * loader falls back to the copy inside the jar, at {@code /natives/<os>/<arch>/}. A child JVM with an empty
 * library path and the layout of the jar on its class path goes down exactly that road.
 */
class JarLoaderSmokeTest {
    private static final class Child {
        int exit;
        String output;
    }

    private static Path builtLibrary(String fileName) {
        for (String entry : System.getProperty("java.library.path", "").split(File.pathSeparator)) {
            if (entry.isEmpty()) continue;
            Path candidate = Paths.get(entry, fileName);
            if (Files.isRegularFile(candidate)) return candidate;
        }
        return null;
    }

    private static Child runChild(Path nativesRoot) throws Exception {
        String java = Paths.get(System.getProperty("java.home"), "bin", "java").toString();
        Path classes = Paths.get(Quantity.class.getProtectionDomain().getCodeSource().getLocation().toURI());
        Path testClasses = Paths.get(LoaderSmokeMain.class.getProtectionDomain().getCodeSource().getLocation().toURI());
        List<String> command = new ArrayList<>();
        command.add(java);
        command.add("-Djava.library.path=");
        command.add("-cp");
        command.add(nativesRoot + File.pathSeparator + classes + File.pathSeparator + testClasses);
        command.add("com.physure.LoaderSmokeMain");
        Process process = new ProcessBuilder(command).redirectErrorStream(true).start();
        ByteArrayOutputStream output = new ByteArrayOutputStream();
        try (InputStream in = process.getInputStream()) {
            byte[] buffer = new byte[4096];
            for (int n; (n = in.read(buffer)) > 0; ) output.write(buffer, 0, n);
        }
        assertTrue(process.waitFor(120, TimeUnit.SECONDS), "the child JVM did not finish");
        Child child = new Child();
        child.exit = process.exitValue();
        child.output = new String(output.toByteArray(), StandardCharsets.UTF_8);
        return child;
    }

    @Test
    void theLibraryInsideTheJarLayoutIsFoundAndLoadedOnThisPlatform(@TempDir Path dir) throws Exception {
        String fileName = System.mapLibraryName("physure_java");
        Path library = builtLibrary(fileName);
        assumeTrue(library != null, "no built " + fileName + " on java.library.path (cargo build -p physure-java)");
        String resource = NativeLibraryLocator.resourcePath(
            System.getProperty("os.name"), System.getProperty("os.arch"), fileName);

        Path jarLayout = dir.resolve("jar");
        Path staged = jarLayout.resolve(resource.substring(1));
        Files.createDirectories(staged.getParent());
        Files.copy(library, staged);

        Child found = runChild(jarLayout);
        assertEquals(0, found.exit, found.output);
        assertTrue(found.output.contains("km=1000.0"), found.output);

        // the control: with no library on the class path either, nothing can be loaded, so the run above
        // really did take the library from the layout of the jar
        Path emptyLayout = dir.resolve("empty");
        Files.createDirectories(emptyLayout);
        Child missing = runChild(emptyLayout);
        assertNotEquals(0, missing.exit, missing.output);
        assertTrue(missing.output.contains("not found at " + resource), missing.output);
    }
}
