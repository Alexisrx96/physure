package com.physure;

import java.util.Locale;

/**
 * Where inside the jar the native library of a platform lives: {@code /natives/<os>/<arch>/<file>}.
 * <p>
 * The release workflow stages the libraries under {@code linux}, {@code macos} and {@code windows} and under
 * {@code amd64} and {@code aarch64}, and the loader has to find them from what the JVM reports. The JVM does
 * not report one name per architecture: the same Intel 64-bit processor is {@code amd64} on Linux and Windows
 * and {@code x86_64} on macOS, and the same ARM 64-bit one is {@code aarch64} in OpenJDK and {@code arm64} in
 * some other runtimes. Reading {@code os.arch} straight into the path made the jar unable to find the library
 * it ships for Intel Macs.
 */
final class NativeLibraryLocator {
    private NativeLibraryLocator() {
    }

    static String resourcePath(String osName, String osArch, String libraryFileName) {
        return "/natives/" + osDirectory(osName) + "/" + archDirectory(osArch) + "/" + libraryFileName;
    }

    static String osDirectory(String osName) {
        String os = osName.toLowerCase(Locale.ROOT);
        // macOS first: "darwin" contains "win"
        if (os.contains("mac") || os.contains("darwin")) {
            return "macos";
        }
        if (os.contains("win")) {
            return "windows";
        }
        return "linux";
    }

    static String archDirectory(String osArch) {
        String arch = osArch.toLowerCase(Locale.ROOT);
        switch (arch) {
            case "amd64":
            case "x86_64":
            case "x86-64":
            case "x64":
                return "amd64";
            case "aarch64":
            case "arm64":
                return "aarch64";
            default:
                return arch;
        }
    }
}
