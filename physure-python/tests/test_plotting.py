# Use non-interactive Matplotlib backend for headless tests
import matplotlib
import numpy as np

matplotlib.use("Agg")

import matplotlib.pyplot as plt

import physure as mk
from physure import Q_


def test_plot_imports():
    """Ensure plotting functions are lazily exposed at package and class levels."""
    assert hasattr(mk, "plot")
    assert hasattr(mk, "plot_slices")
    assert hasattr(mk, "plot_interactive")
    assert hasattr(mk, "plot_parallel_coordinates")
    assert hasattr(mk, "plot_pairplot")
    assert hasattr(mk, "plot_covariance")

    q = Q_(10, "m")
    assert hasattr(q, "plot")
    assert hasattr(q, "plot_slices")
    assert hasattr(q, "plot_interactive")
    assert hasattr(q, "plot_covariance")


def test_plot_1d_line_and_scatter():
    """Verify plotting 1D Quantities with and without uncertainties."""
    # Without uncertainty
    x = Q_(np.linspace(0, 10, 50), "s", symbol="Time")
    y = Q_(3 * x.magnitude + 2, "m", symbol="Position")

    ax = y.plot(x=x)
    assert ax is not None
    assert ax.get_xlabel() == "Time (s)"
    assert ax.get_ylabel() == "Position (m)"
    plt.close(ax.figure)

    # With uncertainty (shaded band)
    y_err = Q_(
        3 * x.magnitude + 2,
        "m",
        uncertainty=np.full(50, 0.5),
        symbol="Position",
    )
    ax_err = y_err.plot(x=x, kind="line")
    assert ax_err is not None
    plt.close(ax_err.figure)

    # Scatter with uncertainty (error bars)
    ax_scatter = y_err.plot(x=x, kind="scatter")
    assert ax_scatter is not None
    plt.close(ax_scatter.figure)


def test_plot_2d_heatmap_and_contour():
    """Verify 2D plotting kinds."""
    val_2d = np.random.randn(10, 10)
    q_2d = Q_(val_2d, "V", symbol="Voltage")

    # Heatmap
    ax_heat = q_2d.plot(kind="heatmap")
    assert ax_heat is not None
    plt.close(ax_heat.figure)

    # Contour
    ax_contour = q_2d.plot(kind="contour")
    assert ax_contour is not None
    plt.close(ax_contour.figure)


def test_plot_3d_slices_and_interactive():
    """Verify multi-dimensional slice and interactive plot initializations."""
    val_3d = np.random.randn(5, 10, 10)
    q_3d = Q_(val_3d, "K", symbol="Temperature")

    # Static slices grid
    fig = q_3d.plot_slices(slice_dim=0, num_slices=3)
    assert fig is not None
    plt.close(fig)

    # Interactive slider plot (instantiation check)
    fig_interactive = q_3d.plot_interactive(slice_dims=0)
    assert fig_interactive is not None
    plt.close(fig_interactive)


def test_plot_parallel_coordinates_and_pairplot():
    """Verify high-dimensional plotting functions (Parallel Coordinates & Pair/Corner Plots)."""
    # Create sample N-D dataset
    N = 100
    x1 = Q_(np.random.normal(10, 2, N), "m", symbol="Length")
    x2 = Q_(np.random.normal(5, 1, N), "kg", symbol="Mass")
    x3 = Q_(np.random.normal(2, 0.5, N), "s", symbol="Time")
    target = Q_(
        x1.magnitude * x2.magnitude / x3.magnitude, "J", symbol="Energy"
    )

    data_dict = {"Length": x1, "Mass": x2, "Time": x3}

    # Parallel Coordinates
    ax_parallel = mk.plot_parallel_coordinates(data_dict, target=target)
    assert ax_parallel is not None
    plt.close(ax_parallel.figure)

    # Corner Plot (Pairplot)
    axes_corner = mk.plot_pairplot(data_dict)
    assert axes_corner is not None
    plt.close(axes_corner[0, 0].figure)


def test_plot_covariance_matrix():
    """Verify covariance/correlation plot on Quantities with correlated uncertainties."""
    # Use correlated uncertainty mode
    with mk.uncertainty_mode("correlated"):
        # Create an independent array with standard deviations (as NumPy array)
        q = Q_(
            np.array([10.0, 20.0, 30.0]),
            "m",
            uncertainty=np.array([0.5, 1.0, 1.5]),
        )
        # Force a calculation to populate the CovarianceStore and create correlations
        q_derived = q * 2.0

        # Plot covariance (correlation)
        ax = q_derived.plot_covariance()
        assert ax is not None
        plt.close(ax.figure)
