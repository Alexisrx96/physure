package com.physure;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Represents a matrix of physical quantities (Order 2 Tensor).
 * Compatible with Java 8+.
 */
public class QuantityMatrix {
    private final int rows;
    private final int cols;
    private final List<List<Quantity>> data;

    public QuantityMatrix(List<List<Quantity>> data) {
        if (data == null || data.isEmpty() || data.get(0).isEmpty()) {
            throw new IllegalArgumentException("QuantityMatrix cannot be empty");
        }
        this.rows = data.size();
        this.cols = data.get(0).size();
        List<List<Quantity>> copy = new ArrayList<>();
        for (List<Quantity> r : data) {
            if (r.size() != cols) {
                throw new IllegalArgumentException("All rows in QuantityMatrix must have equal length");
            }
            copy.add(Collections.unmodifiableList(new ArrayList<>(r)));
        }
        this.data = Collections.unmodifiableList(copy);
    }

    public int getRows() {
        return rows;
    }

    public int getCols() {
        return cols;
    }

    public Quantity get(int row, int col) {
        return data.get(row).get(col);
    }

    public QuantityMatrix transpose() {
        List<List<Quantity>> transposed = new ArrayList<>();
        for (int c = 0; c < cols; c++) {
            List<Quantity> colList = new ArrayList<>();
            for (int r = 0; r < rows; r++) {
                colList.add(data.get(r).get(c));
            }
            transposed.add(colList);
        }
        return new QuantityMatrix(transposed);
    }

    public QuantityMatrix matmul(QuantityMatrix other) {
        if (cols != other.getRows()) {
            throw new IllegalArgumentException("Matrix multiplication dimension mismatch");
        }
        List<List<Quantity>> res = new ArrayList<>();
        for (int r = 0; r < rows; r++) {
            List<Quantity> row = new ArrayList<>();
            for (int c = 0; c < other.getCols(); c++) {
                Quantity sum = data.get(r).get(0).mul(other.get(0, c));
                for (int k = 1; k < cols; k++) {
                    sum = sum.add(data.get(r).get(k).mul(other.get(k, c)));
                }
                row.add(sum);
            }
            res.add(row);
        }
        return new QuantityMatrix(res);
    }

    public Quantity det() {
        if (rows != cols) {
            throw new IllegalArgumentException("Determinant requires a square matrix");
        }
        if (rows == 1) {
            return get(0, 0);
        }
        if (rows == 2) {
            Quantity a = get(0, 0), b = get(0, 1);
            Quantity c = get(1, 0), d = get(1, 1);
            return a.mul(d).sub(b.mul(c));
        }
        throw new UnsupportedOperationException("det currently supports up to 2x2 matrices in Java 8");
    }

    @Override
    public String toString() {
        StringBuilder sb = new StringBuilder("QuantityMatrix([");
        for (int r = 0; r < rows; r++) {
            if (r > 0) sb.append(", ");
            sb.append(data.get(r));
        }
        sb.append("])");
        return sb.toString();
    }
}
