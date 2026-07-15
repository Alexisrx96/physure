"""Visualization and plotting utilities for Physure physical quantities.

This module provides aesthetic, unit-aware plotting functions for 1D, 2D, and N-D
Quantities. It handles automatic unit labeling on axes, uncertainty rendering (shaded
error bands/error bars), multi-dimensional slicing, parallel coordinates, corner/pair plots,
and covariance matrix visualization.

All heavy visualization libraries (matplotlib, plotly) are imported lazily to
preserve startup performance.
"""

from __future__ import annotations

import contextlib
from typing import TYPE_CHECKING, Any, overload

from physure.core.dispatcher import BackendManager

if TYPE_CHECKING:
    from collections.abc import Callable, Sequence

    import numpy as np
    import pandas as pd
    from matplotlib.axes import Axes
    from matplotlib.figure import Figure
    from mpl_toolkits.mplot3d import Axes3D

    from physure.core.protocols import Numeric
    from physure.domain.measurement.quantity import Quantity

# Modern, premium color palette for beautiful visualizations
COLORS = [
    "#4F46E5",  # Indigo
    "#0D9488",  # Teal
    "#E11D48",  # Rose
    "#7C3AED",  # Violet
    "#D97706",  # Amber
    "#2563EB",  # Blue
    "#059669",  # Emerald
    "#DC2626",  # Red
]


@overload
def to_numpy(data: None) -> None: ...
@overload
def to_numpy(data: Numeric) -> Numeric: ...
def to_numpy(data: Numeric | None) -> Numeric | None:
    """Safely converts any tensor or array-like object to a NumPy array."""
    if data is None:
        return None
    # SciPy sparse matrices / arrays
    if hasattr(data, "toarray"):
        return data.toarray()
    if hasattr(data, "todense"):
        import numpy as np

        return np.asarray(data.todense())
    # PyTorch tensor
    if hasattr(data, "detach"):
        data = data.detach()
    if hasattr(data, "cpu"):
        data = data.cpu()
    if hasattr(data, "numpy"):
        try:
            return data.numpy()
        except Exception:
            pass
    # JAX array
    if hasattr(data, "device_buffer") or "jax" in str(type(data)).lower():
        import numpy as np

        return np.asarray(data)
    # Generic
    import numpy as np

    try:
        return np.asarray(data)
    except Exception:
        return data


def _apply_aesthetic_style(
    ax: Axes,
    title: str | None = None,
    xlabel: str | None = None,
    ylabel: str | None = None,
) -> None:
    """Applies a clean, modern, and aesthetic style to a Matplotlib axes object."""
    # Set pure white background
    ax.set_facecolor("white")

    # Thin, very light gray dashed grid lines
    ax.grid(
        True,
        which="both",
        linestyle="--",
        linewidth=0.5,
        color="#E5E7EB",
        zorder=0,
    )

    # Hide top and right spines
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    # Style remaining spines (bottom and left)
    for spine in ["left", "bottom"]:
        ax.spines[spine].set_color("#9CA3AF")
        ax.spines[spine].set_linewidth(1.0)

    # Style ticks and labels
    ax.tick_params(colors="#4B5563", labelsize=10, width=1.0)

    # Set titles and labels with nice margins
    if xlabel:
        ax.set_xlabel(
            xlabel,
            fontsize=11,
            color="#1F2937",
            fontweight="medium",
            labelpad=8,
        )
    if ylabel:
        ax.set_ylabel(
            ylabel,
            fontsize=11,
            color="#1F2937",
            fontweight="medium",
            labelpad=8,
        )
    if title:
        ax.set_title(
            title, fontsize=13, color="#111827", fontweight="bold", pad=12
        )

    # Tight layout helper
    with contextlib.suppress(Exception):
        ax.figure.tight_layout()


def plot(
    y: Quantity | Numeric,
    x: Quantity | Numeric | None = None,
    kind: str | None = None,
    ax: Axes | None = None,
    theme: str = "physure",
    **kwargs: Any,
) -> Axes | Figure:
    """Aesthetic plotting of physical quantities with automatic units and error propagation.

    Args:
        y: The main Quantity to plot.
        x: Optional independent variable/coordinates. Can be a Quantity or array-like.
        kind: Type of plot ('line', 'scatter', 'heatmap', 'surface', 'contour', 'hist').
              If None, auto-detected based on dimensions.
        ax: Optional Matplotlib axes.
        theme: Style theme to apply (defaults to 'physure' clean style).
        **kwargs: Additional parameters passed to Matplotlib plotting functions.

    Returns:
        The Matplotlib axes or figure.
    """
    try:
        import matplotlib.pyplot as plt
    except ImportError as e:
        raise ImportError(
            "Matplotlib is required for plotting. Install it via `pip install matplotlib`."
        ) from e

    import numpy as np

    from physure.domain.measurement.quantity import Quantity

    # Process y quantity
    y_is_q = isinstance(y, Quantity)
    y_val = to_numpy(y.magnitude) if y_is_q else to_numpy(y)
    y_unc = to_numpy(y.uncertainty) if y_is_q else None
    y_unit_str = str(y.unit) if (y_is_q and y.unit) else ""
    y_label = (y.symbol or "Value") if y_is_q else "Value"

    # Process x quantity
    x_is_q = isinstance(x, Quantity)
    x_val = (
        to_numpy(x.magnitude)
        if x_is_q
        else (to_numpy(x) if x is not None else None)
    )
    x_unc = to_numpy(x.uncertainty) if x_is_q else None
    x_unit_str = str(x.unit) if (x_is_q and x.unit) else ""
    x_label = (x.symbol or "Index") if x_is_q else "Index"

    # Handle shape and dimensions
    shape = y_val.shape if hasattr(y_val, "shape") else ()
    ndim = len(shape)

    # Cast 0D scalar to 1D
    if ndim == 0:
        y_val = np.array([y_val])
        if y_unc is not None:
            y_unc = np.array([y_unc])
        if x_val is None:
            x_val = np.array([0])
        ndim = 1

    # Auto-detect plot kind
    if kind is None:
        if ndim == 1:
            kind = "line"
        elif ndim == 2:
            kind = "heatmap"
        else:
            kind = "slices"

    # Setup axes
    if ax is None:
        if kind == "surface":
            fig = plt.figure(figsize=(8, 6))
            ax = fig.add_subplot(111, projection="3d")
        else:
            fig, ax = plt.subplots(figsize=(8, 5))
    else:
        fig = ax.figure

    # Build axis label texts
    ylabel_text = f"{y_label} ({y_unit_str})" if y_unit_str else y_label
    if x_val is not None:
        xlabel_text = f"{x_label} ({x_unit_str})" if x_unit_str else x_label
    else:
        xlabel_text = "Index"

    # Plot customizations
    color = kwargs.pop("color", COLORS[0])
    label = kwargs.pop("label", y_label)
    title = kwargs.pop("title", None)

    if kind == "line":
        if x_val is None:
            x_val = np.arange(len(y_val))

        # Sort coordinates for line continuity
        if len(x_val.shape) == 1 and len(y_val.shape) == 1:
            sort_idx = np.argsort(x_val)
            x_val_sorted = x_val[sort_idx]
            y_val_sorted = y_val[sort_idx]
            if (
                y_unc is not None
                and hasattr(y_unc, "__len__")
                and len(y_unc) == len(y_val)
            ):
                y_unc_sorted = y_unc[sort_idx]
            else:
                y_unc_sorted = y_unc
        else:
            x_val_sorted = x_val
            y_val_sorted = y_val
            y_unc_sorted = y_unc

        # Main line plot
        linewidth = kwargs.pop("linewidth", 2.0)
        zorder = kwargs.pop("zorder", 3)
        ax.plot(
            x_val_sorted,
            y_val_sorted,
            color=color,
            linewidth=linewidth,
            label=label,
            zorder=zorder,
            **kwargs,
        )

        # Shaded uncertainty band
        if y_unc_sorted is not None and np.any(y_unc_sorted > 0):
            if np.isscalar(y_unc_sorted) or y_unc_sorted.ndim == 0:
                y_unc_sorted = np.full_like(y_val_sorted, y_unc_sorted)
            ax.fill_between(
                x_val_sorted,
                y_val_sorted - y_unc_sorted,
                y_val_sorted + y_unc_sorted,
                color=color,
                alpha=0.15,
                zorder=2,
                label=f"{label} uncertainty",
            )

    elif kind == "scatter":
        if x_val is None:
            x_val = np.arange(len(y_val))

        has_x_err = x_unc is not None and np.any(x_unc > 0)
        has_y_err = y_unc is not None and np.any(y_unc > 0)

        if has_x_err or has_y_err:
            elinewidth = kwargs.pop("elinewidth", 1.5)
            capsize = kwargs.pop("capsize", 3)
            capthick = kwargs.pop("capthick", 1.0)
            markersize = kwargs.pop("markersize", 6)
            markeredgecolor = kwargs.pop("markeredgecolor", "white")
            markeredgewidth = kwargs.pop("markeredgewidth", 1.0)
            zorder = kwargs.pop("zorder", 3)
            ax.errorbar(
                x_val,
                y_val,
                xerr=x_unc if has_x_err else None,
                yerr=y_unc if has_y_err else None,
                fmt="o",
                color=color,
                ecolor=color,
                elinewidth=elinewidth,
                capsize=capsize,
                capthick=capthick,
                markersize=markersize,
                markeredgecolor=markeredgecolor,
                markeredgewidth=markeredgewidth,
                label=label,
                zorder=zorder,
                **kwargs,
            )
        else:
            edgecolor = kwargs.pop("edgecolor", "white")
            linewidth = kwargs.pop("linewidth", 0.5)
            s = kwargs.pop("s", 45)
            zorder = kwargs.pop("zorder", 3)
            ax.scatter(
                x_val,
                y_val,
                color=color,
                edgecolor=edgecolor,
                linewidth=linewidth,
                s=s,
                label=label,
                zorder=zorder,
                **kwargs,
            )

    elif kind == "heatmap":
        cmap = kwargs.pop("cmap", "plasma")
        im = ax.imshow(y_val, cmap=cmap, aspect="auto", **kwargs)

        cbar = fig.colorbar(im, ax=ax)
        cbar.set_label(
            ylabel_text,
            rotation=270,
            labelpad=15,
            fontsize=10,
            color="#1F2937",
        )
        cbar.ax.tick_params(labelsize=9, colors="#4B5563")

        if x_val is None:
            xlabel_text = "Column Index"
            ylabel_text = "Row Index"

    elif kind == "contour":
        cmap = kwargs.pop("cmap", "plasma")
        filled = kwargs.pop("filled", True)
        if filled:
            cnt = ax.contourf(y_val, cmap=cmap, **kwargs)
        else:
            cnt = ax.contour(y_val, cmap=cmap, **kwargs)

        cbar = fig.colorbar(cnt, ax=ax)
        cbar.set_label(
            ylabel_text,
            rotation=270,
            labelpad=15,
            fontsize=10,
            color="#1F2937",
        )
        cbar.ax.tick_params(labelsize=9, colors="#4B5563")

        if x_val is None:
            xlabel_text = "Column Index"
            ylabel_text = "Row Index"

    elif kind == "surface":
        # ponytail: ax is Axes3D here (created via projection="3d" above or
        # passed in by the caller); base Axes stubs don't expose 3D methods.
        ax_3d: Axes3D = ax  # pyright: ignore[reportAssignmentType]
        cmap = kwargs.pop("cmap", plt.get_cmap("plasma"))
        rows, cols = y_val.shape
        X, Y = np.meshgrid(np.arange(cols), np.arange(rows))

        surf = ax_3d.plot_surface(
            X,
            Y,
            y_val,
            cmap=cmap,
            linewidth=0,
            antialiased=True,
            rcount=100,
            ccount=100,
            **kwargs,
        )

        cbar = fig.colorbar(surf, ax=ax_3d, shrink=0.6, aspect=12, pad=0.08)
        cbar.set_label(
            ylabel_text,
            rotation=270,
            labelpad=15,
            fontsize=10,
            color="#1F2937",
        )
        cbar.ax.tick_params(labelsize=9, colors="#4B5563")

        ax_3d.set_zlabel(ylabel_text, fontsize=10, color="#1F2937", labelpad=8)
        xlabel_text = "X Index"
        ylabel_text = "Y Index"

    elif kind == "hist":
        ax.hist(
            y_val.flatten(),
            bins=kwargs.pop("bins", "auto"),
            color=color,
            edgecolor="white",
            alpha=0.85,
            rwidth=0.9,
            zorder=3,
            **kwargs,
        )
        xlabel_text = ylabel_text
        ylabel_text = "Frequency"

    elif kind == "slices":
        plt.close(fig)
        return plot_slices(y, **kwargs)

    else:
        raise ValueError(f"Unknown plot kind: {kind}")

    if theme == "physure" and kind != "slices" and kind != "surface":
        _apply_aesthetic_style(
            ax, title=title, xlabel=xlabel_text, ylabel=ylabel_text
        )
    elif title:
        ax.set_title(title)

    return ax


def plot_slices(
    quantity: Quantity,
    slice_dim: int = 0,
    num_slices: int = 4,
    cmap: str = "plasma",
    **kwargs: Any,
) -> Figure:
    """Plots multiple 2D slice grids of a 3D+ Quantity for N-D field visualization.

    Args:
        quantity: The multi-dimensional Quantity.
        slice_dim: Dimension along which to slice.
        num_slices: Number of grid panels to produce.
        cmap: Colormap for slice heatmaps.
        **kwargs: Additional plotting options.

    Returns:
        The Matplotlib figure containing the grid.
    """
    try:
        import matplotlib.pyplot as plt
    except ImportError as e:
        raise ImportError(
            "Matplotlib is required for plotting. Install it via `pip install matplotlib`."
        ) from e

    import numpy as np

    from physure.domain.measurement.quantity import Quantity

    if not isinstance(quantity, Quantity):
        raise TypeError("plot_slices requires a physure Quantity.")

    val = to_numpy(quantity.magnitude)
    unit_str = str(quantity.unit) if quantity.unit else ""
    label = quantity.symbol or "Value"

    if val.ndim < 3:
        raise ValueError(
            f"plot_slices requires a quantity with >= 3 dimensions. Got shape: {val.shape}"
        )

    dim_len = val.shape[slice_dim]
    indices = np.linspace(0, dim_len - 1, num_slices, dtype=int)

    ncols = min(num_slices, 4)
    nrows = int(np.ceil(num_slices / ncols))

    fig, axes = plt.subplots(
        nrows,
        ncols,
        figsize=(4 * ncols, 3.5 * nrows),
        sharex=True,
        sharey=True,
    )
    axes = np.array([axes]) if num_slices == 1 else axes.flatten()

    global_min = np.min(val)
    global_max = np.max(val)

    for i, idx in enumerate(indices):
        ax = axes[i]
        slicer = [slice(None)] * val.ndim
        slicer[slice_dim] = idx
        slice_data = val[tuple(slicer)]

        while slice_data.ndim > 2:
            slice_data = slice_data[0]

        im = ax.imshow(
            slice_data,
            cmap=cmap,
            vmin=global_min,
            vmax=global_max,
            aspect="auto",
        )

        ax.set_title(
            f"Slice {slice_dim} = {idx}",
            fontsize=10,
            fontweight="semibold",
            color="#374151",
        )
        ax.spines["top"].set_visible(False)
        ax.spines["right"].set_visible(False)
        ax.spines["left"].set_color("#D1D5DB")
        ax.spines["bottom"].set_color("#D1D5DB")
        ax.tick_params(colors="#4B5563", labelsize=9)

    for j in range(num_slices, len(axes)):
        fig.delaxes(axes[j])

    fig.subplots_adjust(right=0.85)
    cbar_ax = fig.add_axes((0.88, 0.15, 0.03, 0.7))
    cbar = fig.colorbar(im, cax=cbar_ax)
    ylabel_text = f"{label} ({unit_str})" if unit_str else label
    cbar.set_label(
        ylabel_text,
        rotation=270,
        labelpad=15,
        fontsize=10,
        color="#1F2937",
        fontweight="medium",
    )
    cbar.ax.tick_params(labelsize=9, colors="#4B5563")

    title = kwargs.pop("title", f"Slices of {label} along dim {slice_dim}")
    fig.suptitle(
        title, fontsize=12, fontweight="bold", color="#111827", y=0.98
    )

    return fig


def plot_interactive(
    quantity: Quantity,
    slice_dims: list[int] | int = 0,
    cmap: str = "plasma",
    **kwargs: Any,
) -> Axes | Figure:
    """Creates an interactive GUI plot with sliders to scrub through N-D dimensions.

    Args:
        quantity: Multi-dimensional Quantity.
        slice_dims: Dimension index or indices to attach sliders to.
        cmap: Heatmap colormap.
        **kwargs: Extra plotting options.

    Returns:
        The Matplotlib figure with interactive controls.
    """
    try:
        import matplotlib.pyplot as plt
        from matplotlib.widgets import Slider
    except ImportError as e:
        raise ImportError(
            "Matplotlib is required for interactive plotting. "
            "Install it via `pip install matplotlib`."
        ) from e

    import numpy as np

    from physure.domain.measurement.quantity import Quantity

    if not isinstance(quantity, Quantity):
        raise TypeError("plot_interactive requires a physure Quantity.")

    val = to_numpy(quantity.magnitude)
    unit_str = str(quantity.unit) if quantity.unit else ""
    label = quantity.symbol or "Value"

    if val.ndim < 3:
        return plot(quantity, **kwargs)

    if isinstance(slice_dims, int):
        slice_dims = [slice_dims]

    plot_dims = [val.ndim - 2, val.ndim - 1]
    slider_dims = [d for d in range(val.ndim) if d not in plot_dims]

    num_sliders = len(slider_dims)
    fig, ax = plt.subplots(figsize=(8, 6 + 0.4 * num_sliders))
    fig.subplots_adjust(bottom=0.15 + 0.05 * num_sliders)

    current_indices = {d: val.shape[d] // 2 for d in slider_dims}

    def get_slice() -> Numeric:
        slicer = [slice(None)] * val.ndim
        for d, idx in current_indices.items():
            slicer[d] = idx
        return val[tuple(slicer)]

    slice_data = get_slice()
    im = ax.imshow(
        slice_data,
        cmap=cmap,
        vmin=np.min(val),
        vmax=np.max(val),
        aspect="auto",
    )

    cbar = fig.colorbar(im, ax=ax)
    ylabel_text = f"{label} ({unit_str})" if unit_str else label
    cbar.set_label(
        ylabel_text, rotation=270, labelpad=15, fontsize=10, color="#1F2937"
    )
    cbar.ax.tick_params(labelsize=9, colors="#4B5563")

    _apply_aesthetic_style(
        ax,
        title=f"{label} (Interactive Slice)",
        xlabel="X Index",
        ylabel="Y Index",
    )

    sliders = []
    for i, d in enumerate(slider_dims):
        ax_slider = fig.add_axes((0.15, 0.05 + 0.05 * i, 0.65, 0.03))
        slider = Slider(
            ax=ax_slider,
            label=f"Dim {d}",
            valmin=0,
            valmax=val.shape[d] - 1,
            valinit=current_indices[d],
            valstep=1,
            color="#4F46E5",
        )
        slider.label.set_color("#374151")
        slider.label.set_fontsize(9)
        slider.valtext.set_color("#4B5563")
        slider.valtext.set_fontsize(9)

        def update_factory(dim: int = d) -> Callable[[float], None]:
            def update(val_slider: float) -> None:
                current_indices[dim] = int(val_slider)
                im.set_data(get_slice())
                fig.canvas.draw_idle()

            return update

        slider.on_changed(update_factory(d))
        sliders.append(slider)

    # Anchor to avoid GC cleanup of widgets
    vars(ax)["_sliders"] = sliders

    return fig


def plot_parallel_coordinates(
    data: dict[str, Quantity | Numeric]
    | Sequence[Quantity | Numeric]
    | pd.DataFrame,
    target: Quantity | Numeric | None = None,
    names: list[str] | None = None,
    ax: Axes | None = None,
    cmap: str = "plasma",
    **kwargs: Any,
) -> Axes:
    """Plots parallel coordinates for high-dimensional Quantity datasets.

    Args:
        data: Dict of 1D Quantities/arrays, list of 1D Quantities/arrays, or a pandas DataFrame.
        target: Optional 1D Quantity or array to color code each line.
        names: List of names for the dimensions/columns.
        ax: Optional Matplotlib axes.
        cmap: Colormap for target-scaled lines.
        **kwargs: Extra plotting options.

    Returns:
        The Matplotlib axes.
    """
    try:
        import matplotlib.pyplot as plt
    except ImportError as e:
        raise ImportError(
            "Matplotlib is required for parallel coordinates. "
            "Install it via `pip install matplotlib`."
        ) from e

    import numpy as np

    from physure.domain.measurement.quantity import Quantity

    cols = []
    col_names = []

    if isinstance(data, dict):
        for k, v in data.items():
            cols.append(v)
            col_names.append(k)
    elif isinstance(data, list):
        cols = data
        if names:
            col_names = names
        else:
            for idx, q in enumerate(cols):
                if isinstance(q, Quantity) and q.symbol:
                    col_names.append(q.symbol)
                else:
                    col_names.append(f"Dim {idx}")
    elif hasattr(data, "columns"):
        for col in data.columns:
            cols.append(data[col])
            col_names.append(col)
    else:
        raise TypeError(
            "data must be a dict, list of quantities, or pandas DataFrame."
        )

    arrays = []
    units = []
    for q in cols:
        if isinstance(q, Quantity):
            arrays.append(to_numpy(q.magnitude))
            units.append(str(q.unit) if q.unit else "")
        else:
            arrays.append(to_numpy(q))
            units.append("")

    num_samples = len(arrays[0])
    for arr in arrays:
        if len(arr) != num_samples:
            raise ValueError("All quantities must have the same length.")

    num_cols = len(arrays)
    normalized = np.zeros((num_samples, num_cols))
    mins = []
    maxs = []

    for c in range(num_cols):
        arr = arrays[c]
        col_min = np.min(arr)
        col_max = np.max(arr)
        mins.append(col_min)
        maxs.append(col_max)
        if col_max > col_min:
            normalized[:, c] = (arr - col_min) / (col_max - col_min)
        else:
            normalized[:, c] = 0.5

    colors = None
    t_min = 0.0
    t_max = 1.0
    target_unit = ""
    target_label = "Target"

    if target is not None:
        if isinstance(target, Quantity):
            target_val = to_numpy(target.magnitude)
            target_unit = str(target.unit) if target.unit else ""
            target_label = target.symbol or "Target"
        else:
            target_val = to_numpy(target)
            target_unit = ""
            target_label = "Target"

        t_min = float(np.min(target_val))
        t_max = float(np.max(target_val))
        if t_max > t_min:
            target_norm = (target_val - t_min) / (t_max - t_min)
        else:
            target_norm = np.zeros_like(target_val)

        cm = plt.get_cmap(cmap)
        colors = cm(target_norm)

    if ax is None:
        fig, ax = plt.subplots(figsize=(10, 5))
    else:
        fig = ax.figure

    for i in range(num_samples):
        line_color = colors[i] if colors is not None else COLORS[0]
        alpha = 0.6 if colors is not None else 0.4
        linewidth = 1.5 if colors is not None else 1.0
        ax.plot(
            np.arange(num_cols),
            normalized[i, :],
            color=line_color,
            alpha=alpha,
            linewidth=linewidth,
        )

    ax.set_xticks(np.arange(num_cols))
    x_labels = []
    for name, unit in zip(col_names, units, strict=False):
        if unit:
            x_labels.append(f"{name}\n({unit})")
        else:
            x_labels.append(name)
    ax.set_xticklabels(
        x_labels, fontsize=10, fontweight="medium", color="#1F2937"
    )

    for c in range(num_cols):
        ax.axvline(c, color="#D1D5DB", linestyle="-", linewidth=1.0, zorder=1)
        ax.text(
            c,
            1.01,
            f"{maxs[c]:.2g}",
            ha="center",
            va="bottom",
            fontsize=8,
            color="#4B5563",
            fontweight="semibold",
        )
        ax.text(
            c,
            -0.01,
            f"{mins[c]:.2g}",
            ha="center",
            va="top",
            fontsize=8,
            color="#4B5563",
            fontweight="semibold",
        )

    ax.spines["top"].set_visible(False)
    ax.spines["bottom"].set_visible(False)
    ax.spines["left"].set_visible(False)
    ax.spines["right"].set_visible(False)
    ax.get_yaxis().set_visible(False)
    ax.set_facecolor("white")

    if target is not None:
        sm = plt.cm.ScalarMappable(
            cmap=cmap, norm=plt.Normalize(vmin=t_min, vmax=t_max)
        )
        sm.set_array([])
        cbar = fig.colorbar(sm, ax=ax, pad=0.05)
        cbar_title = (
            f"{target_label} ({target_unit})" if target_unit else target_label
        )
        cbar.set_label(
            cbar_title,
            rotation=270,
            labelpad=15,
            fontsize=10,
            color="#1F2937",
            fontweight="medium",
        )
        cbar.ax.tick_params(labelsize=9, colors="#4B5563")

    title = kwargs.pop("title", "Parallel Coordinates Plot")
    fig.suptitle(
        title, fontsize=12, fontweight="bold", color="#111827", y=0.98
    )
    with contextlib.suppress(Exception):
        fig.tight_layout()

    return ax


def plot_pairplot(
    data: dict[str, Quantity | Numeric]
    | Sequence[Quantity | Numeric]
    | pd.DataFrame,
    cmap: str = "plasma",
    **kwargs: Any,
) -> np.ndarray:
    """Plots pairwise scatter grids (corner/pair plots) for physical quantities.

    Args:
        data: Dict of 1D Quantities/arrays, list of 1D Quantities/arrays, or a pandas DataFrame.
        cmap: Colormap for plots.
        **kwargs: Extra plotting options.

    Returns:
        Matrix of Matplotlib axes.
    """
    try:
        import matplotlib.pyplot as plt
    except ImportError as e:
        raise ImportError(
            "Matplotlib is required for pairplot. Install it via `pip install matplotlib`."
        ) from e

    import numpy as np

    from physure.domain.measurement.quantity import Quantity

    cols = []
    col_names = []

    if isinstance(data, dict):
        for k, v in data.items():
            cols.append(v)
            col_names.append(k)
    elif isinstance(data, list):
        cols = data
        for idx, q in enumerate(cols):
            if isinstance(q, Quantity) and q.symbol:
                col_names.append(q.symbol)
            else:
                col_names.append(f"Dim {idx}")
    elif hasattr(data, "columns"):
        for col in data.columns:
            cols.append(data[col])
            col_names.append(col)
    else:
        raise TypeError(
            "data must be a dict, list of quantities, or pandas DataFrame."
        )

    arrays = []
    units = []
    for q in cols:
        if isinstance(q, Quantity):
            arrays.append(to_numpy(q.magnitude))
            units.append(str(q.unit) if q.unit else "")
        else:
            arrays.append(to_numpy(q))
            units.append("")

    num_cols = len(arrays)
    num_samples = len(arrays[0])
    for arr in arrays:
        if len(arr) != num_samples:
            raise ValueError("All quantities must have the same length.")

    fig, axes = plt.subplots(
        num_cols,
        num_cols,
        figsize=(2.5 * num_cols, 2.5 * num_cols),
        sharex="col",
        sharey="row",
    )
    if num_cols == 1:
        axes = np.array([[axes]])

    axis_labels = []
    for name, unit in zip(col_names, units, strict=False):
        if unit:
            axis_labels.append(f"{name}\n({unit})")
        else:
            axis_labels.append(name)

    for i in range(num_cols):
        for j in range(num_cols):
            ax = axes[i, j]
            ax.set_facecolor("white")
            ax.spines["top"].set_visible(False)
            ax.spines["right"].set_visible(False)
            ax.spines["left"].set_color("#D1D5DB")
            ax.spines["bottom"].set_color("#D1D5DB")
            ax.tick_params(colors="#4B5563", labelsize=8)

            if i == j:
                ax.hist(
                    arrays[i],
                    bins="auto",
                    color=COLORS[0],
                    edgecolor="white",
                    alpha=0.85,
                    rwidth=0.9,
                    zorder=3,
                )
                ax.grid(
                    True,
                    linestyle="--",
                    linewidth=0.5,
                    color="#F3F4F6",
                    zorder=0,
                )
            elif j < i:
                ax.scatter(
                    arrays[j],
                    arrays[i],
                    color=COLORS[1],
                    edgecolor="white",
                    linewidth=0.5,
                    s=20,
                    alpha=0.7,
                    zorder=3,
                )
                ax.grid(
                    True,
                    linestyle="--",
                    linewidth=0.5,
                    color="#F3F4F6",
                    zorder=0,
                )
            else:
                ax.set_visible(False)

            if i == num_cols - 1:
                ax.set_xlabel(
                    axis_labels[j],
                    fontsize=9,
                    fontweight="medium",
                    color="#1F2937",
                )
            if j == 0:
                ax.set_ylabel(
                    axis_labels[i],
                    fontsize=9,
                    fontweight="medium",
                    color="#1F2937",
                )

    title = kwargs.pop("title", "Pairwise Scatter Grid (Corner Plot)")
    fig.suptitle(
        title, fontsize=12, fontweight="bold", color="#111827", y=0.98
    )
    with contextlib.suppress(Exception):
        fig.tight_layout()

    return axes


def get_covariance_matrix(quantity: Quantity) -> Numeric | None:
    """Attempts to retrieve the underlying covariance matrix of a Quantity."""
    from physure.domain.measurement.quantity import Quantity

    if not isinstance(quantity, Quantity):
        return None

    try:
        from physure.domain.measurement.uncertainty import CovarianceModel
        from physure.domain.measurement.vectorized_uncertainty import (
            ensure_store,
        )

        unc_obj = getattr(quantity, "_uncertainty_obj", None)
        if unc_obj is None and hasattr(quantity, "uncertainty_obj"):
            unc_obj = quantity.uncertainty_obj

        if (
            isinstance(unc_obj, CovarianceModel)
            and unc_obj.vector_slice is not None
        ):
            backend = BackendManager.get_backend(quantity.magnitude)
            store = ensure_store(backend)
            slc = unc_obj.vector_slice
            cov = store.get_covariance_block(slc, slc)
            return to_numpy(cov)
    except Exception:
        pass
    return None


def plot_covariance(
    quantity: Quantity, ax: Axes | None = None, **kwargs: Any
) -> Axes:
    """Plots the covariance/correlation matrix of a Quantity if available.

    Args:
        quantity: The Quantity with correlated uncertainties.
        ax: Optional Matplotlib axes.
        **kwargs: Extra plotting options.

    Returns:
        The Matplotlib axes.
    """
    try:
        import matplotlib.pyplot as plt
    except ImportError as e:
        raise ImportError(
            "Matplotlib is required for covariance plotting. "
            "Install it via `pip install matplotlib`."
        ) from e

    import numpy as np

    cov = get_covariance_matrix(quantity)
    if cov is None:
        raise ValueError(
            "The quantity does not have an active CovarianceStore or correlated uncertainty."
        )

    # Compute correlation matrix: corr = cov / (std * std_T)
    std = np.sqrt(np.diag(cov))
    # Replace zeros with 1 to avoid division by zero
    std_safe = np.where(std == 0, 1.0, std)
    corr = cov / np.outer(std_safe, std_safe)
    np.fill_diagonal(corr, 1.0)  # enforce self-correlation = 1.0

    if ax is None:
        fig, ax = plt.subplots(figsize=(6, 5))
    else:
        fig = ax.figure

    cmap = kwargs.pop("cmap", "coolwarm")
    im = ax.imshow(corr, cmap=cmap, vmin=-1.0, vmax=1.0, **kwargs)

    cbar = fig.colorbar(im, ax=ax)
    cbar.set_label(
        "Correlation", rotation=270, labelpad=15, fontsize=10, color="#1F2937"
    )
    cbar.ax.tick_params(labelsize=9, colors="#4B5563")

    # If matrix is small enough, print correlation coefficients inside cells
    if corr.shape[0] <= 12:
        for i in range(corr.shape[0]):
            for j in range(corr.shape[1]):
                val = corr[i, j]
                # High contrast text color selection
                color = "white" if abs(val) > 0.5 else "black"
                ax.text(
                    j,
                    i,
                    f"{val:.2f}",
                    ha="center",
                    va="center",
                    color=color,
                    fontsize=8,
                    fontweight="semibold",
                )

    title = kwargs.pop(
        "title", f"Correlation Matrix (Shape: {corr.shape[0]}x{corr.shape[1]})"
    )
    _apply_aesthetic_style(
        ax, title=title, xlabel="Variable Index", ylabel="Variable Index"
    )

    return ax
