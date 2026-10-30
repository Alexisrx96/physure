package com.physure;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.Arrays;
import java.util.List;
import java.util.Set;
import java.util.TreeSet;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import org.junit.jupiter.api.Test;

/**
 * Where the jar looks for its native library, for what each JVM reports. This is pure logic, so it runs on
 * every platform for every platform: the jar ships libraries for five, and a test machine is only one.
 */
class NativeLibraryLocatorTest {
    private static void assertPath(String expected, String osName, String osArch, String file) {
        assertEquals(expected, NativeLibraryLocator.resourcePath(osName, osArch, file), osName + " / " + osArch);
    }

    @Test
    void everyPlatformTheJarShipsIsFoundFromWhatItsJvmReports() {
        assertPath("/natives/linux/amd64/libphysure_java.so", "Linux", "amd64", "libphysure_java.so");
        assertPath("/natives/linux/aarch64/libphysure_java.so", "Linux", "aarch64", "libphysure_java.so");
        assertPath("/natives/windows/amd64/physure_java.dll", "Windows 11", "amd64", "physure_java.dll");
        assertPath("/natives/macos/aarch64/libphysure_java.dylib", "Mac OS X", "aarch64", "libphysure_java.dylib");
        // OpenJDK reports an Intel Mac as x86_64 (and not amd64, as Linux and Windows do): the library is staged as amd64
        assertPath("/natives/macos/amd64/libphysure_java.dylib", "Mac OS X", "x86_64", "libphysure_java.dylib");
    }

    @Test
    void theOtherNamesOfTheSameProcessorsAreRead() {
        assertPath("/natives/linux/amd64/x", "Linux", "x86_64", "x");
        assertPath("/natives/windows/amd64/x", "Windows 10", "x86_64", "x");
        assertPath("/natives/macos/aarch64/x", "Mac OS X", "arm64", "x");
        assertPath("/natives/linux/aarch64/x", "Linux", "arm64", "x");
        assertPath("/natives/linux/amd64/x", "LINUX", "AMD64", "x");
    }

    @Test
    void darwinIsMacAndNotWindowsEvenThoughItsNameContainsWin() {
        assertEquals("macos", NativeLibraryLocator.osDirectory("Darwin"));
        assertEquals("macos", NativeLibraryLocator.osDirectory("Mac OS X"));
        assertEquals("windows", NativeLibraryLocator.osDirectory("Windows Server 2022"));
        assertEquals("linux", NativeLibraryLocator.osDirectory("Linux"));
        assertEquals("linux", NativeLibraryLocator.osDirectory("FreeBSD"));
    }

    @Test
    void anArchitectureWithoutAnotherNameIsLeftAsItIs() {
        assertPath("/natives/linux/ppc64le/x", "Linux", "ppc64le", "x");
        assertPath("/natives/linux/riscv64/x", "Linux", "riscv64", "x");
    }

    /**
     * The libraries the release workflow stages are the places the loader looks: if the workflow gains a platform
     * (or renames a directory) and the loader does not know it, the jar for it would load nothing.
     */
    @Test
    void theLayoutTheReleaseWorkflowStagesIsTheLayoutTheLoaderReads() throws IOException {
        String workflow = new String(
            Files.readAllBytes(Paths.get("..", ".github", "workflows", "java-release.yml")), StandardCharsets.UTF_8);
        Set<String> staged = new TreeSet<>();
        Matcher matrix = Pattern.compile("os: (\\w+),\\s+arch: (\\w+),").matcher(workflow);
        while (matrix.find()) {
            staged.add("/natives/" + matrix.group(1) + "/" + matrix.group(2));
        }
        // what the JVM of each staged platform reports
        List<String[]> reports = Arrays.asList(
            new String[] {"Linux", "amd64"}, new String[] {"Linux", "aarch64"},
            new String[] {"Mac OS X", "x86_64"}, new String[] {"Mac OS X", "aarch64"},
            new String[] {"Windows 11", "amd64"});
        Set<String> read = new TreeSet<>();
        for (String[] report : reports) {
            String path = NativeLibraryLocator.resourcePath(report[0], report[1], "f");
            read.add(path.substring(0, path.length() - 2));
        }
        assertTrue(staged.size() >= 5, "the workflow stages " + staged);
        assertEquals(staged, read);
    }
}
