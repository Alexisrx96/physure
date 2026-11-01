"""What each kind of plot draws, and how the 3-D plots and exports are chosen."""

import sys
import types

import matplotlib
import numpy as np
import pytest

matplotlib.use("Agg")

import matplotlib.pyplot as plt

from physure import Q_, plotting

T = np.linspace(0, 6, 25)
Y = np.sin(T)
MATRIX = np.outer(np.linspace(1, 2, 9), np.linspace(0.5, 1.5, 7))


def _same_text(written, returned):
    """The file holds what was returned, but for the line endings a text file gets on Windows."""
    return written.replace("\r\n", "\n") == returned.replace("\r\n", "\n")


@pytest.fixture(autouse=True)
def _close_figures():
    yield
    plt.close("all")


def test_a_line_is_drawn_in_the_order_of_x_with_a_band_for_its_uncertainty():
    y = Q_(Y[::-1], "m", uncertainty=np.full(25, 0.1), symbol="Position")
    ax = plotting.plot(y, x=Q_(T[::-1], "s", symbol="Time"))

    assert list(ax.lines[0].get_xdata()) == sorted(T)
    assert len(ax.collections) == 1, "the band of the uncertainty"
    assert (ax.get_xlabel(), ax.get_ylabel()) == ("Time (s)", "Position (m)")
    assert ax.get_legend_handles_labels()[1] == [
        "Position",
        "Position uncertainty",
    ]


def test_a_scalar_is_one_point_and_a_plain_array_is_labelled_by_position():
    point = plotting.plot(Q_(3.0, "m", symbol="Pos"))
    assert len(point.lines[0].get_xdata()) == 1
    plain = plotting.plot(Y)
    assert (plain.get_xlabel(), plain.get_ylabel()) == ("Index", "Value")


def test_options_given_to_plot_reach_the_line():
    ax = plotting.plot(
        Y, x=T, color="#112233", label="L", title="Title", linewidth=3.0
    )
    line = ax.lines[0]
    assert (line.get_color(), line.get_linewidth(), line.get_label()) == (
        "#112233",
        3.0,
        "L",
    )
    assert ax.get_title() == "Title"


def test_a_scatter_has_error_bars_only_where_there_is_an_uncertainty():
    plain = plotting.plot(Q_(Y, "m"), x=Q_(T, "s"), kind="scatter")
    assert not plain.containers
    assert len(plain.collections) == 1

    erred = plotting.plot(
        Q_(Y, "m", uncertainty=np.full(25, 0.1)),
        x=Q_(T, "s", uncertainty=np.full(25, 0.05)),
        kind="scatter",
    )
    assert len(erred.containers) == 1
    only_x = plotting.plot(
        Q_(Y, "m"), x=Q_(T, "s", uncertainty=np.full(25, 0.05)), kind="scatter"
    )
    assert len(only_x.containers) == 1


def test_a_matrix_is_a_heatmap_a_contour_or_a_surface():
    heat = plotting.plot(Q_(MATRIX, "K", symbol="Temp"))
    assert (heat.get_xlabel(), heat.get_ylabel()) == (
        "Column Index",
        "Row Index",
    )
    assert len(heat.figure.axes) == 2, "the colorbar"

    placed = plotting.plot(
        Q_(MATRIX, "K", symbol="Temp"), x=np.arange(7.0), kind="heatmap"
    )
    assert placed.get_xlabel() == "Index"

    filled = plotting.plot(Q_(MATRIX, "K"), kind="contour")
    lines = plotting.plot(Q_(MATRIX, "K"), kind="contour", filled=False)
    assert len(filled.figure.axes) == len(lines.figure.axes) == 2
    assert (filled.collections[0].filled, lines.collections[0].filled) == (
        True,
        False,
    )

    surface = plotting.plot(Q_(MATRIX, "K", symbol="Temp"), kind="surface")
    assert surface.name == "3d"
    assert surface.get_zlabel() == "Temp (K)"


def test_a_histogram_counts_and_is_labelled_by_the_quantity():
    ax = plotting.plot(
        Q_(np.random.default_rng(0).normal(size=100), "m", symbol="v"),
        kind="hist",
    )
    assert (ax.get_xlabel(), ax.get_ylabel()) == ("v (m)", "Frequency")


def test_other_themes_are_left_unstyled_and_a_title_is_still_set():
    ax = plotting.plot(Y, x=T, theme="plain", title="Plain")
    assert ax.get_title() == "Plain"
    assert not ax.get_xlabel()


def test_an_unknown_kind_is_refused_before_a_figure_is_made():
    with pytest.raises(ValueError, match="Unknown plot kind: nope"):
        plotting.plot(Y, kind="nope")
    assert plt.get_fignums() == []


def test_a_cube_is_sliced_and_a_3d_kind_goes_to_plot_3d():
    cube = Q_(np.arange(120.0).reshape(4, 5, 6), "m", symbol="C")
    assert plotting.plot(cube) is not None
    html = plotting.plot(Q_(MATRIX, "m"), kind="3d", backend="html")
    assert isinstance(html, str)
    assert "<html" in html.lower()


def test_parallel_coordinates_name_the_axes_and_color_by_the_target():
    columns = {"a": Q_(np.arange(10.0), "m"), "b": np.arange(10.0) ** 2}
    ax = plotting.plot_parallel_coordinates(
        columns, target=Q_(np.linspace(0, 5, 10), "J", symbol="E")
    )
    assert [t.get_text() for t in ax.get_xticklabels()] == ["a\n(m)", "b"]
    assert len(ax.lines) >= 10
    assert ax.figure.axes[1].get_ylabel().startswith("E (")

    named = plotting.plot_parallel_coordinates(
        [np.arange(4.0), np.ones(4)], names=["u", "v"]
    )
    assert [t.get_text() for t in named.get_xticklabels()] == ["u", "v"]
    symbols = plotting.plot_parallel_coordinates(
        [Q_(np.arange(4.0), "m", symbol="x"), np.ones(4)]
    )
    assert [t.get_text() for t in symbols.get_xticklabels()] == [
        "x\n(m)",
        "Dim 1",
    ]


def test_parallel_coordinates_read_a_dataframe_and_refuse_the_rest():
    pandas = pytest.importorskip("pandas")
    frame = pandas.DataFrame({"u": np.arange(5.0), "w": np.arange(5.0) ** 2})
    ax = plotting.plot_parallel_coordinates(frame)
    assert [t.get_text() for t in ax.get_xticklabels()] == ["u", "w"]
    with pytest.raises(TypeError, match="data must be a dict"):
        plotting.plot_parallel_coordinates(5)
    with pytest.raises(ValueError, match="same length"):
        plotting.plot_parallel_coordinates(
            {"a": np.arange(3.0), "b": np.arange(4.0)}
        )


def test_a_pairplot_has_a_histogram_on_the_diagonal_and_nothing_above_it():
    axes = plotting.plot_pairplot(
        {"a": Q_(np.arange(8.0), "m"), "b": np.arange(8.0) ** 2}
    )
    assert axes.shape == (2, 2)
    assert [a.get_visible() for a in axes.flat] == [True, False, True, True]
    assert axes[1, 0].get_ylabel() == "b"
    assert axes[1, 0].get_xlabel() == "a\n(m)"
    assert plotting.plot_pairplot({"only": np.arange(5.0)}).shape == (1, 1)
    with pytest.raises(TypeError):
        plotting.plot_pairplot("text")
    with pytest.raises(ValueError, match="same length"):
        plotting.plot_pairplot({"a": np.arange(3.0), "b": np.arange(4.0)})


def test_3d_backends_are_chosen_by_name_and_an_unknown_one_is_refused(
    tmp_path,
):
    z = Q_(MATRIX, "m", symbol="Height")
    page = tmp_path / "surface.html"
    html = plotting.plot_3d(
        z, backend="threejs", title="Surf", filename=str(page)
    )
    assert "Surf" in html
    assert _same_text(page.read_bytes().decode("utf-8"), html)

    ax = plotting.plot_3d(
        z, x=Q_(np.arange(7.0), "s", symbol="T"), backend="MPL"
    )
    assert (ax.get_xlabel(), ax.get_zlabel()) == ("T (s)", "Height (m)")
    assert ax.get_title() == "3D Surface of Height (m)"

    with pytest.raises(ValueError, match="Unknown 3D backend: vulkan"):
        plotting.plot_3d(z, backend="vulkan")


def test_the_plotly_backend_gets_the_mesh_and_the_labels(
    monkeypatch, tmp_path
):
    seen = {}

    class Figure:
        def __init__(self, data):
            seen["data"] = data

        def update_layout(self, **layout):
            seen["layout"] = layout

        def write_html(self, name):
            seen["html"] = name

        def write_image(self, name):
            seen["image"] = name

    plotly = types.ModuleType("plotly")
    graph_objects = types.ModuleType("plotly.graph_objects")
    graph_objects.Figure = Figure
    graph_objects.Surface = lambda **fields: fields
    plotly.graph_objects = graph_objects
    monkeypatch.setitem(sys.modules, "plotly", plotly)
    monkeypatch.setitem(sys.modules, "plotly.graph_objects", graph_objects)

    z = Q_(MATRIX, "m", symbol="Height")
    figure = plotting.plot_3d(
        z, filename=str(tmp_path / "s.html")
    )  # "auto" finds plotly

    assert isinstance(figure, Figure)
    surface = seen["data"][0]
    assert surface["colorbar"] == {"title": "Height (m)"}
    assert surface["colorscale"] == "plasma"
    assert seen["layout"]["scene"]["zaxis_title"] == "Height (m)"
    assert seen["html"].endswith("s.html")
    plotting.plot_3d(z, backend="plotly", filename=str(tmp_path / "s.png"))
    assert seen["image"].endswith("s.png")


def test_the_plotly_backend_says_when_plotly_is_missing(monkeypatch):
    monkeypatch.setitem(sys.modules, "plotly", None)
    monkeypatch.setitem(sys.modules, "plotly.graph_objects", None)
    with pytest.raises(ImportError, match="Plotly is required"):
        plotting.plot_3d(Q_(MATRIX, "m"), backend="plotly")
    # "auto" falls back to the HTML viewer
    assert isinstance(plotting.plot_3d(Q_(MATRIX, "m")), str)


@pytest.mark.parametrize("native", [True, False])
@pytest.mark.parametrize(
    ("extension", "needle"),
    [
        ("obj", "v "),
        ("ply", "ply"),
        ("gltf", "asset"),
        ("html", "<html"),
        ("stl", "solid"),
    ],
)
def test_a_mesh_is_exported_in_each_text_format(
    monkeypatch, tmp_path, native, extension, needle
):
    if not native:
        monkeypatch.setattr(
            "physure._core.export_mesh_3d_native",
            lambda *a: (_ for _ in ()).throw(AttributeError("no native")),
        )
    z = Q_(MATRIX, "m", symbol="Height")
    path = tmp_path / f"mesh.{extension}"

    text = plotting.export_3d(z, filename=str(path), binary=False)

    assert needle in text.lower()
    assert _same_text(path.read_bytes().decode("utf-8"), text)


@pytest.mark.parametrize("native", [True, False])
def test_a_binary_stl_and_a_format_that_is_unknown(
    monkeypatch, tmp_path, native
):
    if not native:
        monkeypatch.setattr(
            "physure._core.export_mesh_3d_native",
            lambda *a: (_ for _ in ()).throw(ImportError("no native")),
        )
    z = Q_(MATRIX, "m")
    data = plotting.export_3d(z, filename=str(tmp_path / "m.stl"))
    assert isinstance(data, bytes)
    assert (tmp_path / "m.stl").read_bytes() == data

    if not native:
        with pytest.raises(ValueError, match="Unsupported 3D export format"):
            plotting.export_3d(z, filename=str(tmp_path / "m.xyz"))
