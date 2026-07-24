package com.physure;

/**
 * Domain exception for all errors occurring within the Physure engine
 * (e.g., unit mismatches, syntax errors, invalid conversions).
 * Compatible with Java 8+.
 */
public class PhysureException extends RuntimeException {
    
    public PhysureException(String message) {
        super(message);
    }

    public PhysureException(String message, Throwable cause) {
        super(message, cause);
    }
}
