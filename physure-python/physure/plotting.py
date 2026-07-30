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

    elif kind == "3d":
        plt.close(fig)
        return plot_3d(y, x=x, **kwargs)

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
        # ponytail: duck-typed DataFrame check so pandas stays optional;
        # hasattr narrowing can't give ty a real type for `.columns`.
        for col in data.columns:  # ty: ignore[not-iterable]
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
        # ponytail: duck-typed DataFrame check so pandas stays optional;
        # hasattr narrowing can't give ty a real type for `.columns`.
        for col in data.columns:  # ty: ignore[not-iterable]
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


def _prepare_3d_mesh(
    z: Quantity | Numeric,
    x: Quantity | Numeric | None = None,
    y: Quantity | Numeric | None = None,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, str, str, str]:
    import numpy as np

    from physure.domain.measurement.quantity import Quantity

    z_is_q = isinstance(z, Quantity)
    z_val = to_numpy(z.magnitude) if z_is_q else to_numpy(z)
    z_unit_str = str(z.unit) if (z_is_q and z.unit) else ""
    z_label = (z.symbol or "Z") if z_is_q else "Z"
    zlabel_text = f"{z_label} ({z_unit_str})" if z_unit_str else z_label

    if z_val.ndim == 1:
        z_val = z_val.reshape((1, -1))
    elif z_val.ndim > 2:
        z_val = z_val.reshape((z_val.shape[0], -1))

    rows, cols = z_val.shape

    x_is_q = isinstance(x, Quantity)
    x_val = (
        to_numpy(x.magnitude)
        if x_is_q
        else (to_numpy(x) if x is not None else None)
    )
    x_unit_str = str(x.unit) if (x_is_q and x.unit) else ""
    x_label = (x.symbol or "X") if x_is_q else "X"
    xlabel_text = f"{x_label} ({x_unit_str})" if x_unit_str else x_label

    y_is_q = isinstance(y, Quantity)
    y_val = (
        to_numpy(y.magnitude)
        if y_is_q
        else (to_numpy(y) if y is not None else None)
    )
    y_unit_str = str(y.unit) if (y_is_q and y.unit) else ""
    y_label = (y.symbol or "Y") if y_is_q else "Y"
    ylabel_text = f"{y_label} ({y_unit_str})" if y_unit_str else y_label

    if x_val is None:
        x_grid_1d = np.arange(cols, dtype=float)
    else:
        x_grid_1d = x_val.flatten()

    if y_val is None:
        y_grid_1d = np.arange(rows, dtype=float)
    else:
        y_grid_1d = y_val.flatten()

    if (
        x_val is not None
        and x_val.ndim == 2
        and y_val is not None
        and y_val.ndim == 2
    ):
        X_grid, Y_grid = x_val, y_val
    else:
        X_grid, Y_grid = np.meshgrid(x_grid_1d[:cols], y_grid_1d[:rows])

    return X_grid, Y_grid, z_val, xlabel_text, ylabel_text, zlabel_text


def _mesh_triangles(
    X: np.ndarray, Y: np.ndarray, Z: np.ndarray
) -> tuple[np.ndarray, np.ndarray]:
    import numpy as np

    rows, cols = Z.shape
    verts = np.column_stack([X.ravel(), Y.ravel(), Z.ravel()])

    r_idx, c_idx = np.meshgrid(
        np.arange(rows - 1), np.arange(cols - 1), indexing="ij"
    )
    r_idx = r_idx.ravel()
    c_idx = c_idx.ravel()

    idx00 = r_idx * cols + c_idx
    idx01 = r_idx * cols + (c_idx + 1)
    idx10 = (r_idx + 1) * cols + c_idx
    idx11 = (r_idx + 1) * cols + (c_idx + 1)

    t1 = np.column_stack([idx00, idx10, idx01])
    t2 = np.column_stack([idx10, idx11, idx01])

    faces = np.vstack([t1, t2])
    return verts, faces


def _compute_face_normals(verts: np.ndarray, faces: np.ndarray) -> np.ndarray:
    import numpy as np

    v0 = verts[faces[:, 0]]
    v1 = verts[faces[:, 1]]
    v2 = verts[faces[:, 2]]
    normals = np.cross(v1 - v0, v2 - v0)
    norms = np.linalg.norm(normals, axis=1, keepdims=True)
    norms[norms == 0] = 1.0
    return normals / norms


def _map_z_to_rgb(Z: np.ndarray, cmap_name: str = "plasma") -> np.ndarray:
    import matplotlib.pyplot as plt
    import numpy as np

    z_flat = Z.ravel()
    z_min, z_max = np.min(z_flat), np.max(z_flat)
    if z_max > z_min:
        z_norm = (z_flat - z_min) / (z_max - z_min)
    else:
        z_norm = np.zeros_like(z_flat)

    try:
        cm = plt.get_cmap(cmap_name)
    except Exception:
        cm = plt.get_cmap("plasma")

    colors_rgba = cm(z_norm)
    return colors_rgba[:, :3]


def _export_stl(
    verts: np.ndarray,
    faces: np.ndarray,
    filename: str | None = None,
    binary: bool = True,
) -> str | bytes:
    import struct

    normals = _compute_face_normals(verts, faces)
    v0 = verts[faces[:, 0]]
    v1 = verts[faces[:, 1]]
    v2 = verts[faces[:, 2]]

    if binary:
        header = b"Physure 3D Mesh Export".ljust(80, b"\x00")
        num_triangles = len(faces)
        buffer = bytearray()
        buffer.extend(header)
        buffer.extend(struct.pack("<I", num_triangles))
        for i in range(num_triangles):
            n = normals[i]
            p0, p1, p2 = v0[i], v1[i], v2[i]
            buffer.extend(
                struct.pack(
                    "<12fH",
                    float(n[0]),
                    float(n[1]),
                    float(n[2]),
                    float(p0[0]),
                    float(p0[1]),
                    float(p0[2]),
                    float(p1[0]),
                    float(p1[1]),
                    float(p1[2]),
                    float(p2[0]),
                    float(p2[1]),
                    float(p2[2]),
                    0,
                )
            )
        out_bytes = bytes(buffer)
        if filename:
            with open(filename, "wb") as f:
                f.write(out_bytes)
        return out_bytes
    else:
        lines = ["solid physure_mesh"]
        for i in range(len(faces)):
            n = normals[i]
            p0, p1, p2 = v0[i], v1[i], v2[i]
            lines.append(f"  facet normal {n[0]:.6e} {n[1]:.6e} {n[2]:.6e}")
            lines.append("    outer loop")
            lines.append(f"      vertex {p0[0]:.6e} {p0[1]:.6e} {p0[2]:.6e}")
            lines.append(f"      vertex {p1[0]:.6e} {p1[1]:.6e} {p1[2]:.6e}")
            lines.append(f"      vertex {p2[0]:.6e} {p2[1]:.6e} {p2[2]:.6e}")
            lines.append("    endloop")
            lines.append("  endfacet")
        lines.append("endsolid physure_mesh\n")
        out_str = "\n".join(lines)
        if filename:
            with open(filename, "w", encoding="utf-8") as f:
                f.write(out_str)
        return out_str


def _export_obj(
    verts: np.ndarray,
    faces: np.ndarray,
    colors: np.ndarray | None = None,
    filename: str | None = None,
) -> str:
    lines = ["# Physure 3D Mesh OBJ Export"]
    for i in range(len(verts)):
        v = verts[i]
        if colors is not None:
            c = colors[i]
            lines.append(
                f"v {v[0]:.6f} {v[1]:.6f} {v[2]:.6f} {c[0]:.4f} {c[1]:.4f} {c[2]:.4f}"
            )
        else:
            lines.append(f"v {v[0]:.6f} {v[1]:.6f} {v[2]:.6f}")

    for f in faces:
        lines.append(f"f {f[0] + 1} {f[1] + 1} {f[2] + 1}")
    out_str = "\n".join(lines) + "\n"
    if filename:
        with open(filename, "w", encoding="utf-8") as file:
            file.write(out_str)
    return out_str


def _export_ply(
    verts: np.ndarray,
    faces: np.ndarray,
    colors: np.ndarray | None = None,
    filename: str | None = None,
) -> str:
    lines = [
        "ply",
        "format ascii 1.0",
        "comment Created by Physure 3D Exporter",
        f"element vertex {len(verts)}",
        "property float x",
        "property float y",
        "property float z",
    ]
    if colors is not None:
        lines.extend([
            "property uchar red",
            "property uchar green",
            "property uchar blue",
        ])
    lines.extend([
        f"element face {len(faces)}",
        "property list uchar int vertex_indices",
        "end_header",
    ])
    for i in range(len(verts)):
        v = verts[i]
        if colors is not None:
            rgb = (colors[i] * 255).astype(int)
            lines.append(
                f"{v[0]:.6f} {v[1]:.6f} {v[2]:.6f} {rgb[0]} {rgb[1]} {rgb[2]}"
            )
        else:
            lines.append(f"{v[0]:.6f} {v[1]:.6f} {v[2]:.6f}")

    for f in faces:
        lines.append(f"3 {f[0]} {f[1]} {f[2]}")

    out_str = "\n".join(lines) + "\n"
    if filename:
        with open(filename, "w", encoding="utf-8") as file:
            file.write(out_str)
    return out_str


def _export_gltf(
    verts: np.ndarray,
    faces: np.ndarray,
    colors: np.ndarray | None = None,
    filename: str | None = None,
) -> str:
    import base64
    import json
    import numpy as np

    v_bytes = verts.astype(np.float32).tobytes()
    f_bytes = faces.astype(np.uint32).tobytes()

    if colors is not None:
        c_bytes = colors.astype(np.float32).tobytes()
        buffer_data = v_bytes + c_bytes + f_bytes
    else:
        c_bytes = b""
        buffer_data = v_bytes + f_bytes

    b64_uri = (
        "data:application/octet-stream;base64,"
        + base64.b64encode(buffer_data).decode("ascii")
    )

    v_len = len(v_bytes)
    c_len = len(c_bytes)

    v_min = verts.min(axis=0).tolist()
    v_max = verts.max(axis=0).tolist()

    buffer_views = [
        {"buffer": 0, "byteOffset": 0, "byteLength": v_len, "target": 34962}
    ]
    accessors = [
        {
            "bufferView": 0,
            "byteOffset": 0,
            "componentType": 5126,
            "count": len(verts),
            "type": "VEC3",
            "min": v_min,
            "max": v_max,
        }
    ]

    attributes = {"POSITION": 0}
    current_offset = v_len

    if colors is not None:
        buffer_views.append({
            "buffer": 0,
            "byteOffset": current_offset,
            "byteLength": c_len,
            "target": 34962,
        })
        accessors.append({
            "bufferView": 1,
            "byteOffset": 0,
            "componentType": 5126,
            "count": len(verts),
            "type": "VEC3",
        })
        attributes["COLOR_0"] = 1
        current_offset += c_len

    indices_view_idx = len(buffer_views)
    buffer_views.append({
        "buffer": 0,
        "byteOffset": current_offset,
        "byteLength": len(f_bytes),
        "target": 34963,
    })
    indices_acc_idx = len(accessors)
    accessors.append({
        "bufferView": indices_view_idx,
        "byteOffset": 0,
        "componentType": 5125,
        "count": len(faces) * 3,
        "type": "SCALAR",
    })

    gltf = {
        "asset": {"version": "2.0", "generator": "Physure 3D Exporter"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [
            {
                "primitives": [
                    {
                        "attributes": attributes,
                        "indices": indices_acc_idx,
                        "mode": 4,
                    }
                ]
            }
        ],
        "buffers": [{"uri": b64_uri, "byteLength": len(buffer_data)}],
        "bufferViews": buffer_views,
        "accessors": accessors,
    }

    out_str = json.dumps(gltf, indent=2)
    if filename:
        with open(filename, "w", encoding="utf-8") as file:
            file.write(out_str)
    return out_str


def _export_html_threejs(
    verts: np.ndarray,
    faces: np.ndarray,
    colors: np.ndarray,
    xlabel: str = "X",
    ylabel: str = "Y",
    zlabel: str = "Z",
    title: str = "Physure 3D Interactive Viewer",
    filename: str | None = None,
) -> str:
    import json

    v_flat = verts.flatten().tolist()
    f_flat = faces.flatten().tolist()
    c_flat = colors.flatten().tolist()

    html_content = f"""<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title}</title>
    <style>
        body {{
            margin: 0;
            padding: 0;
            overflow: hidden;
            background-color: #0f172a;
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            color: #f8fafc;
        }}
        #canvas-container {{
            width: 100vw;
            height: 100vh;
            display: block;
        }}
        #hud {{
            position: absolute;
            top: 16px;
            left: 16px;
            background: rgba(15, 23, 42, 0.85);
            backdrop-filter: blur(8px);
            border: 1px solid rgba(255, 255, 255, 0.1);
            padding: 14px 20px;
            border-radius: 10px;
            box-shadow: 0 4px 20px rgba(0,0,0,0.4);
            max-width: 320px;
            z-index: 10;
        }}
        #hud h2 {{
            margin: 0 0 6px 0;
            font-size: 16px;
            font-weight: 600;
            color: #38bdf8;
        }}
        #hud p {{
            margin: 4px 0;
            font-size: 12px;
            color: #94a3b8;
        }}
        .controls-info {{
            margin-top: 10px;
            font-size: 11px;
            color: #64748b;
            border-top: 1px solid rgba(255,255,255,0.08);
            padding-top: 8px;
        }}
    </style>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/three@0.128.0/examples/js/controls/OrbitControls.js"></script>
</head>
<body>
    <div id="hud">
        <h2>{title}</h2>
        <p><strong>X:</strong> {xlabel}</p>
        <p><strong>Y:</strong> {ylabel}</p>
        <p><strong>Z:</strong> {zlabel}</p>
        <div class="controls-info">
            🎮 <strong>Controls:</strong> Left click + drag to rotate | Right click + drag to pan | Scroll to zoom
        </div>
    </div>
    <div id="canvas-container"></div>

    <script>
        const vertices = new Float32Array({json.dumps(v_flat)});
        const indices = new Uint32Array({json.dumps(f_flat)});
        const colors = new Float32Array({json.dumps(c_flat)});

        const container = document.getElementById('canvas-container');
        const scene = new THREE.Scene();
        scene.background = new THREE.Color(0x0f172a);

        const camera = new THREE.PerspectiveCamera(45, window.innerWidth / window.innerHeight, 0.1, 1000);
        const renderer = new THREE.WebGLRenderer({{ antialias: true }});
        renderer.setSize(window.innerWidth, window.innerHeight);
        renderer.setPixelRatio(window.devicePixelRatio);
        container.appendChild(renderer.domElement);

        const controls = new THREE.OrbitControls(camera, renderer.domElement);
        controls.enableDamping = true;
        controls.dampingFactor = 0.05;

        // Build Geometry
        const geometry = new THREE.BufferGeometry();
        geometry.setAttribute('position', new THREE.BufferAttribute(vertices, 3));
        geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));
        geometry.setIndex(new THREE.BufferAttribute(indices, 1));
        geometry.computeVertexNormals();

        // Calculate Bounding Box & Center
        geometry.computeBoundingBox();
        const bbox = geometry.boundingBox;
        const center = new THREE.Vector3();
        bbox.getCenter(center);
        const size = new THREE.Vector3();
        bbox.getSize(size);

        // Double Sided Mesh Material
        const material = new THREE.MeshStandardMaterial({{
            vertexColors: true,
            side: THREE.DoubleSide,
            roughness: 0.3,
            metalness: 0.2,
            wireframe: false
        }});
        const mesh = new THREE.Mesh(geometry, material);
        scene.add(mesh);

        // Wireframe Overlay
        const wireframeMaterial = new THREE.MeshBasicMaterial({{
            color: 0xffffff,
            wireframe: true,
            transparent: true,
            opacity: 0.1
        }});
        const wireframeMesh = new THREE.Mesh(geometry, wireframeMaterial);
        scene.add(wireframeMesh);

        // Lighting
        const ambientLight = new THREE.AmbientLight(0xffffff, 0.7);
        scene.add(ambientLight);
        const dirLight1 = new THREE.DirectionalLight(0xffffff, 0.8);
        dirLight1.position.set(size.x, size.y * 2, size.z * 2);
        scene.add(dirLight1);
        const dirLight2 = new THREE.DirectionalLight(0x38bdf8, 0.4);
        dirLight2.position.set(-size.x, -size.y, -size.z);
        scene.add(dirLight2);

        // Grid & Axis Helpers
        const maxDim = Math.max(size.x, size.y, size.z);
        const gridHelper = new THREE.GridHelper(maxDim * 2, 20, 0x38bdf8, 0x334155);
        gridHelper.position.set(center.x, bbox.min.y, center.z);
        scene.add(gridHelper);

        const axesHelper = new THREE.AxesHelper(maxDim * 0.8);
        axesHelper.position.copy(bbox.min);
        scene.add(axesHelper);

        // Camera positioning
        camera.position.set(center.x + maxDim * 1.5, center.y + maxDim * 1.2, center.z + maxDim * 1.8);
        camera.lookAt(center);
        controls.target.copy(center);

        window.addEventListener('resize', () => {{
            camera.aspect = window.innerWidth / window.innerHeight;
            camera.updateProjectionMatrix();
            renderer.setSize(window.innerWidth, window.innerHeight);
        }});

        function animate() {{
            requestAnimationFrame(animate);
            controls.update();
            renderer.render(scene, camera);
        }}
        animate();
    </script>
</body>
</html>
"""
    if filename:
        with open(filename, "w", encoding="utf-8") as f:
            f.write(html_content)
    return html_content


def plot_3d(
    z: Quantity | Numeric,
    x: Quantity | Numeric | None = None,
    y: Quantity | Numeric | None = None,
    backend: str = "auto",
    title: str | None = None,
    filename: str | None = None,
    cmap: str = "plasma",
    **kwargs: Any,
) -> Any:
    """True interactive 3D surface plot for physical quantities with perspective & WebGL viewer.

    Args:
        z: 2D heightfield or 3D field Quantity or array.
        x: Optional X coordinates or Quantity.
        y: Optional Y coordinates or Quantity.
        backend: Backend engine ('auto', 'plotly', 'html'/'threejs', 'matplotlib').
        title: Optional plot title.
        filename: Optional path to save HTML file or render image.
        cmap: Colormap for 3D surface.
        **kwargs: Extra parameters.

    Returns:
        Plotly Figure object, Three.js HTML string, or Matplotlib Axes3D.
    """
    X_grid, Y_grid, Z_grid, xlabel, ylabel, zlabel = _prepare_3d_mesh(z, x, y)
    title_str = title or f"3D Surface of {zlabel}"

    if backend == "auto":
        try:
            import plotly  # noqa: F401

            backend = "plotly"
        except ImportError:
            backend = "html"

    backend = backend.lower()

    if backend == "plotly":
        try:
            import plotly.graph_objects as go
        except ImportError as e:
            raise ImportError(
                "Plotly is required for the 'plotly' 3D backend. "
                "Install it via `pip install plotly`."
            ) from e

        fig = go.Figure(
            data=[
                go.Surface(
                    z=Z_grid,
                    x=X_grid,
                    y=Y_grid,
                    colorscale=cmap,
                    colorbar=dict(title=zlabel),
                )
            ]
        )
        fig.update_layout(
            title=title_str,
            scene=dict(
                xaxis_title=xlabel,
                yaxis_title=ylabel,
                zaxis_title=zlabel,
                camera=dict(
                    eye=dict(x=1.5, y=1.5, z=1.2),
                    projection=dict(type="perspective"),
                ),
            ),
            margin=dict(l=0, r=0, b=0, t=40),
        )
        if filename:
            if filename.endswith(".html"):
                fig.write_html(filename)
            else:
                fig.write_image(filename)
        return fig

    elif backend in ("html", "threejs", "webgl"):
        verts, faces = _mesh_triangles(X_grid, Y_grid, Z_grid)
        colors = _map_z_to_rgb(Z_grid, cmap_name=cmap)
        return _export_html_threejs(
            verts,
            faces,
            colors,
            xlabel=xlabel,
            ylabel=ylabel,
            zlabel=zlabel,
            title=title_str,
            filename=filename,
        )

    elif backend in ("matplotlib", "mpl"):
        try:
            import matplotlib.pyplot as plt
        except ImportError as e:
            raise ImportError(
                "Matplotlib is required for the 'matplotlib' 3D backend."
            ) from e

        fig = plt.figure(figsize=(9, 7))
        ax = fig.add_subplot(111, projection="3d")
        cm = plt.get_cmap(cmap)
        surf = ax.plot_surface(
            X_grid, Y_grid, Z_grid, cmap=cm, linewidth=0, antialiased=True
        )
        cbar = fig.colorbar(surf, ax=ax, shrink=0.6, aspect=12, pad=0.08)
        cbar.set_label(
            zlabel, rotation=270, labelpad=15, fontsize=10, color="#1F2937"
        )
        ax.set_xlabel(xlabel, fontsize=10, color="#1F2937", labelpad=8)
        ax.set_ylabel(ylabel, fontsize=10, color="#1F2937", labelpad=8)
        ax.set_zlabel(zlabel, fontsize=10, color="#1F2937", labelpad=8)
        ax.set_title(title_str)

        if filename:
            fig.savefig(filename)
        return ax
    else:
        raise ValueError(
            f"Unknown 3D backend: {backend}. Choose from 'auto', 'plotly', 'html'/'threejs', 'matplotlib'."
        )


def export_3d(
    z: Quantity | Numeric,
    x: Quantity | Numeric | None = None,
    y: Quantity | Numeric | None = None,
    filename: str = "plot_3d.stl",
    fmt: str | None = None,
    binary: bool = True,
    cmap: str = "plasma",
    **kwargs: Any,
) -> str | bytes:
    """Exports 3D mesh geometry of a physical quantity into standard 3D file formats.

    Delegates to 100% native Rust engine (physure._core.export_mesh_3d_native).

    Supports export to:
    - STL (.stl ASCII or binary): Standard 3D CAD / printing model format.
    - OBJ (.obj): Wavefront 3D mesh format with vertex coordinates & colors.
    - glTF / GLB (.gltf, .glb): Khronos Group 3D scene standard for web/AR/VR.
    - PLY (.ply): Polygon mesh file format.
    - HTML (.html): Standalone interactive Three.js 3D WebGL viewer.
    """
    X_grid, Y_grid, Z_grid, xlabel, ylabel, zlabel = _prepare_3d_mesh(z, x, y)
    title = kwargs.get("title", f"Physure 3D Mesh: {zlabel}")

    if fmt is None:
        ext = filename.split(".")[-1].lower() if "." in filename else "stl"
        fmt = ext

    fmt_req = fmt.lower()
    rust_fmt = "stl_ascii" if (fmt_req == "stl" and not binary) else fmt_req

    try:
        from physure._core import export_mesh_3d_native

        rows, cols = Z_grid.shape
        x_flat = [float(v) for v in X_grid.flatten()]
        y_flat = [float(v) for v in Y_grid.flatten()]
        z_flat = [float(v) for v in Z_grid.flatten()]

        out_bytes = export_mesh_3d_native(
            title,
            xlabel,
            ylabel,
            zlabel,
            x_flat,
            y_flat,
            z_flat,
            rows,
            cols,
            rust_fmt,
        )

        if rust_fmt in ("obj", "gltf", "glb", "ply", "html", "threejs", "stl_ascii"):
            res_str = out_bytes.decode("utf-8")
            if filename:
                with open(filename, "w", encoding="utf-8") as f:
                    f.write(res_str)
            return res_str
        else:
            if filename:
                with open(filename, "wb") as f:
                    f.write(out_bytes)
            return out_bytes
    except (ImportError, AttributeError):
        verts, faces = _mesh_triangles(X_grid, Y_grid, Z_grid)
        colors = _map_z_to_rgb(Z_grid, cmap_name=cmap)

        if fmt_req == "stl":
            return _export_stl(verts, faces, filename=filename, binary=binary)
        elif fmt_req == "obj":
            return _export_obj(verts, faces, colors=colors, filename=filename)
        elif fmt_req == "ply":
            return _export_ply(verts, faces, colors=colors, filename=filename)
        elif fmt_req in ("gltf", "glb"):
            return _export_gltf(verts, faces, colors=colors, filename=filename)
        elif fmt_req in ("html", "threejs"):
            return _export_html_threejs(
                verts,
                faces,
                colors,
                xlabel=xlabel,
                ylabel=ylabel,
                zlabel=zlabel,
                title=title,
                filename=filename,
            )
        else:
            raise ValueError(
                f"Unsupported 3D export format: {fmt}. Choose from 'stl', 'obj', 'ply', 'gltf', 'glb', 'html'."
            )


