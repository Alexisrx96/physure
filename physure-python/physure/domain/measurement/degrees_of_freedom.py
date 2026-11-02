"""Degrees of freedom, coverage factor and expanded uncertainty of a `Quantity`.

A standard uncertainty that comes from `n` observations is itself uncertain, by `n - 1` degrees of
freedom (JCGM 100:2008 4.2, 4.3). The degrees of freedom of a result follow from those of the
measurements it was computed from by the Welch-Satterthwaite formula (G.4.1), and with them the
coverage factor `k_p` that turns a standard uncertainty into an expanded one, `U = k_p u_c`.

All of the mathematics is the Rust core's: the lineage a scalar quantity already carries
says which independent measurements it came from and how much each contributes,
`physure._core.coverage_factor` is the Student-t factor, and `physure._core.gum_round`
rounds for display. This module only reads and reports them. They are defined for scalar
quantities with the default (Gaussian, lineage-tracked) uncertainty; arrays,
`python_lineage()` and the sampling modes use other models, and raise instead of
answering with a number that was not computed.
"""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from physure.domain.measurement.uncertainty import LineageModel

if TYPE_CHECKING:
    from physure.domain.measurement.units import CompoundUnit


@dataclass(frozen=True)
class ExpandedUncertainty:
    """An expanded uncertainty with everything needed to say how it was obtained.

    It is a result of its own, not a quantity with a `±`: the number after the `±` of a `Quantity`
    is a standard uncertainty, and this one is `U = k u_c` for a coverage probability.

    Examples:
        >>> from physure import Q_
        >>> r = Q_(10.0, "V", uncertainty=0.5).with_dof(16).expanded(0.95)
        >>> print(r)
        (10.0 ± 1.1) V (k=2.12, p=95 %)
        >>> round(r.coverage_factor, 4), r.dof
        (2.1199, 16.0)
    """

    value: float
    unit: CompoundUnit
    standard_uncertainty: float
    expanded_uncertainty: float
    coverage_factor: float
    coverage_probability: float
    dof: float

    def __str__(self) -> str:
        """`(value ± U) unit (k=..., p=... %)`, rounded as the GUM presents a result."""
        from physure._core import gum_round

        value, uncertainty = gum_round(self.value, self.expanded_uncertainty)
        coverage = f"k={self.coverage_factor:.2f}, p={self.coverage_probability * 100:g} %"
        unit = self.unit.to_string(use_alias=True)
        return f"({value} ± {uncertainty}) {unit} ({coverage})"


class DegreesOfFreedomMixin:
    """The degrees of freedom of a quantity's uncertainty, and what follows from them."""

    def _lineage(self) -> Any:
        """The Rust lineage of this quantity; `None` for an exact one.

        Raises:
            NotImplementedError: The uncertainty is not a scalar lineage (an array,
                `python_lineage()`, a sampling mode): those models do not keep the independent
                sources a value came from.
        """
        model = getattr(self, "_uncertainty_obj", None)
        if model is None:
            return None
        if isinstance(model, LineageModel):
            return model.native
        raise NotImplementedError(
            "Degrees of freedom are tracked for scalar quantities with the default uncertainty "
            f"model; this quantity uses {type(model).__name__}, which does not keep the "
            "independent sources a value came from (arrays, python_lineage() and the sampling "
            "modes)."
        )

    def with_dof(self, nu: float) -> Any:
        """Declares that this measurement's standard uncertainty has `nu` degrees of freedom.

        The degrees of freedom belong to a **measurement**: a quantity of one source of
        uncertainty. A result that combines several has those of its sources, so declare them on
        each before combining. Returns this quantity, whose measurement now carries them; every
        copy of it shares them.

        Args:
            nu: Positive, or `inf` for an uncertainty known exactly.

        Raises:
            ValueError: The quantity is exact or combines several measurements, `nu` is not
                positive, or the measurement already has a different number.
            NotImplementedError: The quantity is not a scalar with the default uncertainty model.

        Examples:
            >>> from physure import Q_
            >>> Q_(10.003, "V", uncertainty=0.002).with_dof(9).dof
            9.0
        """
        lineage = self._lineage()
        if lineage is None:
            raise ValueError(
                "an exact quantity has no uncertainty to have degrees of freedom"
            )
        lineage.declare_dof(float(nu))
        return self

    @property
    def dof(self) -> float:
        """The effective degrees of freedom (Welch-Satterthwaite, JCGM 100:2008 G.4.1).

        `inf` when none of the measurements the quantity comes from has a finite number, and for
        an exact quantity.

        Examples:
            >>> from physure import Q_
            >>> a = Q_(1.0, "m", uncertainty=3.0, dof=4)
            >>> b = Q_(2.0, "m", uncertainty=4.0, dof=9)
            >>> round((a + b).dof, 3)
            12.835
            >>> (a + a).dof
            4.0
        """
        lineage = self._lineage()
        return math.inf if lineage is None else float(lineage.effective_dof())

    def coverage_factor(self, p: float = 0.95) -> float:
        """The coverage factor `k_p` for the coverage probability `p`.

        The two-sided Student-t factor at the effective degrees of freedom truncated to an
        integer, as the Guide recommends (G.4.1), and the normal factor when they are infinite.

        Raises:
            ValueError: `p` is not in (0, 1).

        Examples:
            >>> from physure import Q_
            >>> round(Q_(1.0, "m", uncertainty=0.1, dof=16).coverage_factor(0.95), 4)
            2.1199
            >>> round(Q_(1.0, "m", uncertainty=0.1).coverage_factor(0.95), 4)
            1.96
        """
        from physure._core import coverage_factor

        return float(coverage_factor(float(p), self.dof))

    def expanded(self, p: float = 0.95) -> ExpandedUncertainty:
        """The expanded uncertainty `U = k_p u_c` for the coverage probability `p`.

        Raises:
            ValueError: `p` is not in (0, 1).
            NotImplementedError: The quantity is not a scalar with the default uncertainty model.
        """
        dof = self.dof
        k = self.coverage_factor(p)
        standard = float(self.uncertainty)  # type: ignore[attr-defined]
        return ExpandedUncertainty(
            value=float(self.magnitude),  # type: ignore[attr-defined]
            unit=self.unit,  # type: ignore[attr-defined]
            standard_uncertainty=standard,
            expanded_uncertainty=k * standard,
            coverage_factor=k,
            coverage_probability=float(p),
            dof=dof,
        )
