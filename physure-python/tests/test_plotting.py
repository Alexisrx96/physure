# Use non-interactive Matplotlib backend for headless tests
import matplotlib
import numpy as np

matplotlib.use("Agg")

import matplotlib.pyplot as plt

import physure as ps
from physure import Q_


def test_plot_imports():
    """Ensure plotting functions are lazily exposed at package and class levels."""
    assert hasattr(ps, "plot")
    assert hasattr(ps, "plot_3d")
    assert hasattr(ps, "export_3d")
    assert hasattr(ps, "plot_slices")
    assert hasattr(ps, "plot_interactive")
    assert hasattr(ps, "plot_parallel_coordinates")
    assert hasattr(ps, "plot_pairplot")
    assert hasattr(ps, "plot_covariance")

    q = Q_(10, "m")
    assert hasattr(q, "plot")
    assert hasattr(q, "plot_3d")
    assert hasattr(q, "export_3d")
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
    ax_parallel = ps.plot_parallel_coordinates(data_dict, target=target)
    assert ax_parallel is not None
    plt.close(ax_parallel.figure)

    # Corner Plot (Pairplot)
    axes_corner = ps.plot_pairplot(data_dict)
    assert axes_corner is not None
    plt.close(axes_corner[0, 0].figure)


def test_plot_covariance_matrix():
    """Verify covariance/correlation plot on Quantities with correlated uncertainties."""
    # Use correlated uncertainty mode
    with ps.uncertainty_mode("correlated"):
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


def test_plot_3d_rendering(tmp_path):
    """Verify true 3D interactive plotting and HTML generation."""
    x = Q_(np.linspace(-5, 5, 10), "m", symbol="X_Position")
    y = Q_(np.linspace(-5, 5, 10), "m", symbol="Y_Position")
    X, Y = np.meshgrid(x.magnitude, y.magnitude)
    Z = Q_(np.sin(np.sqrt(X**2 + Y**2)), "Pa", symbol="Pressure")

    # Test Matplotlib 3D backend
    ax_mpl = Z.plot_3d(x=x, y=y, backend="matplotlib")
    assert ax_mpl is not None
    plt.close(ax_mpl.figure)

    # Test HTML WebGL Three.js backend
    html_file = tmp_path / "interactive_3d.html"
    html_content = Z.plot_3d(x=x, y=y, backend="html", filename=str(html_file))
    assert "OrbitControls" in html_content
    assert "WebGLRenderer" in html_content
    assert html_file.exists()


def test_export_3d_formats(tmp_path):
    """Verify exporting 3D surface meshes to STL, OBJ, glTF, PLY, and HTML formats."""
    x = Q_(np.linspace(0, 1, 5), "m", symbol="Width")
    y = Q_(np.linspace(0, 1, 5), "m", symbol="Height")
    X, Y = np.meshgrid(x.magnitude, y.magnitude)
    Z = Q_(X + Y, "K", symbol="Temperature")

    # 1. STL Binary
    stl_bin_path = tmp_path / "mesh.stl"
    stl_bin = Z.export_3d(filename=str(stl_bin_path), fmt="stl", binary=True)
    assert stl_bin_path.exists()
    assert isinstance(stl_bin, bytes)
    assert stl_bin.startswith(b"Physure 3D")

    # 2. STL ASCII
    stl_ascii_path = tmp_path / "mesh_ascii.stl"
    stl_ascii = Z.export_3d(
        filename=str(stl_ascii_path), fmt="stl", binary=False
    )
    assert stl_ascii_path.exists()
    assert isinstance(stl_ascii, str)
    assert "solid physure_mesh" in stl_ascii

    # 3. Wavefront OBJ
    obj_path = tmp_path / "mesh.obj"
    obj_str = Z.export_3d(filename=str(obj_path), fmt="obj")
    assert obj_path.exists()
    assert "v " in obj_str
    assert "f " in obj_str

    # 4. glTF 2.0
    gltf_path = tmp_path / "mesh.gltf"
    gltf_str = Z.export_3d(filename=str(gltf_path), fmt="gltf")
    assert gltf_path.exists()
    assert '"asset"' in gltf_str
    assert "Physure 3D" in gltf_str

    # 5. PLY
    ply_path = tmp_path / "mesh.ply"
    ply_str = Z.export_3d(filename=str(ply_path), fmt="ply")
    assert ply_path.exists()
    assert "format ascii 1.0" in ply_str

    # 6. HTML Three.js
    html_path = tmp_path / "viewer.html"
    html_str = Z.export_3d(filename=str(html_path), fmt="html")
    assert html_path.exists()
    assert "Physure 3D" in html_str
