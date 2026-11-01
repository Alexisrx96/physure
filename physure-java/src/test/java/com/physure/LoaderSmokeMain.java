package com.physure;

/** Run in a child JVM by {@link JarLoaderSmokeTest}: loads the native library however it can and uses it once. */
public final class LoaderSmokeMain {
    private LoaderSmokeMain() {
    }

    public static void main(String[] args) {
        try (UnitRegistry registry = new UnitRegistry()) {
            System.out.println("km=" + registry.getUnitScale("km")); // NOSONAR java:S106: the parent test reads this process's output
        }
    }
}
