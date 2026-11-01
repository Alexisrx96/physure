import com.physure.UnitRegistry;

/**
 * Run from the packaged jar with no library path (`java -Djava.library.path= -cp physure.jar PackagedJarSmoke.java`):
 * the jar has to find and load the native library it carries.
 */
public class PackagedJarSmoke {
    public static void main(String[] args) {
        try (UnitRegistry registry = new UnitRegistry()) {
            double km = registry.getUnitScale("km");
            if (km != 1000.0) {
                System.err.println("the packaged jar answered " + km + " for km");
                System.exit(1);
            }
            System.out.println("packaged jar OK: km = " + km);
        }
    }
}
