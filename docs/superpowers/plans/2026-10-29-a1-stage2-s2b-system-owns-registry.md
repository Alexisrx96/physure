# A1 stage 2, step 2b — a system owns its registry — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a `UnitSystem` has a registry of the core built from what the system registers, strictly, and the registry answers what the system answers. The best-effort mirror and the two lookup paths that returned a unit without its scale are deleted.

**Architecture:** a new module writes a unit or a prefix as conf text (`definitions_text.py`). `UnitSystem` gives its registry that text first, so a unit the core cannot take leaves the system as it was, and keeps the texts so that pickling rebuilds the registry. The builder of `create_system` gives the registry the merged `[Prefixes]` and `[Units]` text once and registers in Python only. Differential tests compare the registry with Python system by system.

**Tech Stack:** Python 3.11+ (`physure-python`), Rust (`physure-core`, one small change), pytest.

**Design:** `docs/superpowers/specs/2026-10-29-a1-stage2-design.md` ("Step 2b, refined"). **Replay:** this plan has not been replayed mechanically yet; doing it on a clean branch is the first thing to do once the decisions of the design are confirmed.

**Conventions of the repository for every task:** work in a worktree branch, never on `main`; in a Windows shell `unset VIRTUAL_ENV` and use `uv run --no-sync`; rebuild the extension with `uv run --no-sync maturin develop --release` from `physure-python/` before a Python test sees a change in Rust, and check that `physure._core.__file__` is inside the worktree; `physure-core/src/units/conf.rs` and `tests.rs` use CRLF line endings, so edit them with a script that reads bytes, normalises to LF, edits, and writes back with the file's own EOL (a script must read the plan's blocks from the file, never retype them: escapes such as `\n` get mangled); `physure-python/physure/domain/measurement/system.py` and `physure-python/physure/application/startup.py` may use CRLF too, so check before editing; end commit messages with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`; `ruff check .` and `ruff format --check .` from `physure-python/` must pass.

---

## File structure

| File | Change |
|---|---|
| `physure-core/src/units/conf.rs`, `tests.rs` | a prefix factor can be a fraction |
| `physure-python/physure/domain/measurement/definitions_text.py` | new: a unit or a prefix as conf text |
| `physure-python/physure/domain/measurement/system.py` | owns the registry; the mirror and two lookup paths deleted |
| `physure-python/physure/application/startup.py` | the builder gives the registry the merged text |
| `physure-python/tests/measurement_tests/test_definitions_text.py` | new |
| `physure-python/tests/core/test_system_registry.py` | new: strictness, pickling, membership |
| `physure-python/tests/core/test_system_registry_parity.py` | new: the registry answers what Python answers |
| `CHANGELOG.md`, the stage 2 design | the entries |

---

### Task 1: a prefix factor can be a fraction

**Files:** `physure-core/src/units/conf.rs`, `physure-core/src/units/tests.rs`.

Python registers prefixes with an exact `Fraction`, and the conf of a prefix only took a decimal, although a unit factor may be a fraction (`5/9`). Writing a prefix as text must not round it.

- [ ] **Step 1: Write the failing test.** Append to `physure-core/src/units/tests.rs`:

```rust
/// A prefix factor is a decimal or a fraction, as the factor of a unit is: `1/3` stays exact.
#[test]
fn a_prefix_factor_can_be_a_fraction() {
    use crate::units::parser::Parser;
    let (mut reg, mut constants) = conf::build_registry_from_conf();
    conf::extend_registry(&mut reg, &mut constants, "[Prefixes]\nthird = th, 1/3\n").unwrap();
    let atomic = |text: &str| Parser::parse_expression_atomic(text).unwrap();
    assert_eq!(reg.factor_between(&atomic("thm"), &atomic("m")).unwrap(), 1.0 / 3.0);
    assert_eq!(reg.factor_between(&atomic("thirdm"), &atomic("m")).unwrap(), 1.0 / 3.0);
}
```

- [ ] **Step 2: See it fail.** From `physure-core/`: `cargo test -p physure --lib a_prefix_factor_can_be_a_fraction`. Expected: FAIL (the `unwrap` of the strict reading: `the factor of a prefix is a number, not '1/3'`).

- [ ] **Step 3: Implement.** In `physure-core/src/units/conf.rs`:

Find:
```rust
                        if let Ok(factor) = parts[1].parse::<f64>() {
                            registry.add_prefix_exact(symbol.to_string(), factor, crate::units::exact::ExactScale::parse(parts[1]));
```
Replace with:
```rust
                        if let Some((factor, exact)) = parse_number(parts[1]) {
                            registry.add_prefix_exact(symbol.to_string(), factor, exact);
```

- [ ] **Step 4: See it pass.** `cargo test -p physure` (all green).

- [ ] **Step 5: Commit.**

```bash
git add physure-core/src/units/conf.rs physure-core/src/units/tests.rs
git commit -m "feat(core): a prefix factor in a conf can be a fraction

The factor of a unit has always been a decimal or a fraction (5/9); the
factor of a prefix only took a decimal. A system registers its prefixes
with an exact fraction, and writing one as text must not round it.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: what a system registers, written as conf text

**Files:** `physure-python/physure/domain/measurement/definitions_text.py` (new), `physure-python/tests/measurement_tests/test_definitions_text.py` (new).

- [ ] **Step 1: Write the failing tests.** Create `physure-python/tests/measurement_tests/test_definitions_text.py`:

```python
"""A1 stage 2, step 2b: what a unit system registers, written as conf text."""

from fractions import Fraction

import pytest

from physure._core import UnitRegistry
from physure.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
    OffsetConverter,
    UnitConverter,
)
from physure.domain.measurement.definitions_text import (
    prefix_definition_text,
    unit_definition_text,
)
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.units import CompoundUnit

MONEY = Dimension({"$": 1})
POWER = Dimension({"M": 1, "L": 2, "T": -3})


def _the_core_takes(text):
    registry = UnitRegistry()
    registry.extend_conf(text)
    return registry


class _Square(UnitConverter):
    is_linear = False

    def to_base(self, value):
        return value**2

    def from_base(self, value):
        return value**0.5


class _Scaled(LinearConverter):
    pass


def test_a_scaled_unit_is_written_with_its_names():
    text = unit_definition_text(
        "USD",
        MONEY,
        LinearConverter(1.0),
        ["dollar", "USD"],
        allow_prefixes=False,
    )
    assert text == "[Units]\nUSD = 1.0, $, noprefix, [USD, dollar]\n"
    assert _the_core_takes(text).facts("dollar")["dimension"] == {"$": 1}


def test_an_exact_scale_is_written_as_the_fraction():
    converter = LinearConverter(5 / 9, exact=Fraction(5, 9))
    text = unit_definition_text("F", Dimension({"O": 1}), converter)
    assert text == "[Units]\nF = 5/9, O, [F]\n"
    _the_core_takes(text)


def test_a_unit_with_a_zero_point():
    converter = OffsetConverter(1.0, 273.15, exact_offset=Fraction(5463, 20))
    text = unit_definition_text(
        "degX", Dimension({"O": 1}), converter, kind="absolute"
    )
    assert text == "[Units]\ndegX = 1.0, 5463/20, O, [degX]\n"
    assert _the_core_takes(text).facts("degX")["kind"] == "absolute"


def test_a_level_of_a_quantity():
    text = unit_definition_text(
        "dBW", POWER, LogarithmicConverter(10.0), allow_prefixes=False
    )
    assert (
        text == "[Units]\ndBW = Log, 10.0, 1.0, L^2*M*T^-3, noprefix, [dBW]\n"
    )
    facts = _the_core_takes(text).facts("dBW")
    assert facts["dimension"] == {"L": 2, "M": 1, "T": -3}
    assert facts["is_linear"] is False


def test_a_recipe_is_written_for_a_derived_unit_only():
    energy = Dimension({"M": 1, "L": 2, "T": -2})
    recipe = CompoundUnit({"kg": 1, "m": 2, "s": -2})
    text = unit_definition_text(
        "J", energy, LinearConverter(1.0), ["joule"], recipe=recipe
    )
    assert text == "[Units]\nJ = 1.0, L^2*M*T^-2, kg*m^2*s^-2, [J, joule]\n"
    _the_core_takes(text)
    # a recipe next to a dimensionless "1" would read as a zero point of 1
    unity = unit_definition_text(
        "one", Dimension({}), LinearConverter(1.0), recipe=CompoundUnit({})
    )
    assert unity == "[Units]\none = 1.0, 1, [one]\n"
    _the_core_takes(unity)


@pytest.mark.parametrize("converter", [_Square(), _Scaled(2.0)])
def test_a_converter_a_conf_cannot_say_is_refused(converter):
    with pytest.raises(TypeError, match="not a converter"):
        unit_definition_text("sq", Dimension({"L": 2}), converter)


@pytest.mark.parametrize("name", ["", "a,b", "a=b", "a[b", " a", "#a", "a\nb"])
def test_a_name_a_conf_cannot_hold_is_refused(name):
    with pytest.raises(ValueError, match="cannot be a name"):
        unit_definition_text(name, Dimension({"L": 1}), LinearConverter(1.0))


def test_a_number_that_is_not_finite_is_refused():
    converter = LinearConverter(float("inf"))
    with pytest.raises(ValueError, match="cannot be written"):
        unit_definition_text("x", Dimension({"L": 1}), converter)


def test_a_kind_that_contradicts_the_converter_is_refused():
    with pytest.raises(ValueError, match="kind"):
        unit_definition_text(
            "x", Dimension({"O": 1}), OffsetConverter(1.0, 5.0)
        )
    with pytest.raises(ValueError, match="kind"):
        unit_definition_text(
            "x", Dimension({"L": 1}), LinearConverter(1.0), kind="absolute"
        )


def test_a_level_has_no_recipe_and_an_offset_needs_a_zero_point():
    with pytest.raises(ValueError, match="no recipe"):
        unit_definition_text(
            "dBx",
            POWER,
            LogarithmicConverter(10.0),
            recipe=CompoundUnit({"W": 1}),
        )
    with pytest.raises(ValueError, match="zero point"):
        unit_definition_text(
            "x",
            Dimension({"O": 1}),
            OffsetConverter(1.0, 0.0),
            kind="absolute",
        )


def test_a_prefix_keeps_its_exact_fraction():
    text = prefix_definition_text("th", 1 / 3, "third", Fraction(1, 3))
    assert text == "[Prefixes]\nthird = th, 1/3\n"
    registry = _the_core_takes(text + "[Units]\nmetre = 1.0, L, [m, metre]\n")
    factor = registry.factor_between(
        registry.parse_atomic("thm"), registry.parse_atomic("m")
    )
    assert factor == 1 / 3
    assert prefix_definition_text("k", 1000.0) == "[Prefixes]\nk = k, 1000.0\n"
```

- [ ] **Step 2: See them fail.** From `physure-python/`: `uv run --no-sync pytest tests/measurement_tests/test_definitions_text.py -q -p no:cacheprovider`. Expected: collection error (`No module named 'physure.domain.measurement.definitions_text'`). The Rust change of Task 1 is needed by the last test, so rebuild the extension first (`uv run --no-sync maturin develop --release`).

- [ ] **Step 3: Write the module.** Create `physure-python/physure/domain/measurement/definitions_text.py`:

```python
"""Writes what a unit system registers as the conf text the core reads.

A `UnitSystem` keeps its definitions twice: in Python objects and in the
registry of the core. The registry is built from conf text, so a unit that is
registered by hand is written as the line of the catalog that would define it.
Only what a catalog line can say is supported: a scale, a scale with a zero
point, or a level, on a dimension. Anything else is refused, and the caller
leaves the system as it was.
"""

from __future__ import annotations

import math
from fractions import Fraction
from typing import TYPE_CHECKING

from physure.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
    OffsetConverter,
)

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping

    from physure.domain.measurement.converters import UnitConverter
    from physure.domain.measurement.dimensions import Dimension
    from physure.domain.measurement.units import CompoundUnit

_SUPPORTED = (LinearConverter, OffsetConverter, LogarithmicConverter)
_UNWRITABLE = frozenset("=,[]\n\r")


def _name(text: str) -> str:
    """Returns `text` if a conf can hold it as a name, else raises."""
    if (
        not text
        or text != text.strip()
        or text[0] in "#;"
        or any(char in _UNWRITABLE for char in text)
    ):
        raise ValueError(f"{text!r} cannot be a name in a conf.")
    return text


def _number(value: float, exact: Fraction | None) -> str:
    """Writes a number of the conf: the exact fraction when there is one."""
    if exact is not None:
        return str(Fraction(exact))
    if not math.isfinite(value):
        raise ValueError(f"{value!r} cannot be written in a conf.")
    return repr(float(value))


def _exponent(power: float) -> str:
    """Writes the power of a symbol: ``2``, ``-3`` or ``(1/2)``."""
    fraction = Fraction(power).limit_denominator(1000)
    if fraction.denominator == 1:
        return str(fraction.numerator)
    return f"({fraction.numerator}/{fraction.denominator})"


def _product(exponents: Mapping[str, float]) -> str:
    """Writes symbols to powers as a conf does: ``L^2*M*T^-3``, or ``1``."""
    if not exponents:
        return "1"
    return "*".join(
        _name(symbol) if power == 1 else f"{_name(symbol)}^{_exponent(power)}"
        for symbol, power in sorted(exponents.items())
    )


def unit_definition_text(
    symbol: str,
    dimension: Dimension,
    converter: UnitConverter,
    aliases: Iterable[str] = (),
    *,
    recipe: CompoundUnit | None = None,
    allow_prefixes: bool = True,
    kind: str = "delta",
) -> str:
    """Writes the `[Units]` text that defines a unit in the core's registry.

    The names of the unit are its symbol and its aliases: the descriptive name
    that `register_unit` takes is not a name that resolves, so it is not
    written.

    Args:
        symbol: The symbol of the unit, the first of its names.
        dimension: Its dimension.
        converter: A `LinearConverter`, an `OffsetConverter` or a
            `LogarithmicConverter`, exactly: a subclass is another converter.
        aliases: The other names of the unit.
        recipe: The units it is another name for, when it is one.
        allow_prefixes: Whether prefixes apply to it.
        kind: ``"delta"``, or ``"absolute"`` for a unit with a zero point.

    Returns:
        The text: a `[Units]` section of one line.

    Raises:
        TypeError: The converter is not one a conf line can express.
        ValueError: A name cannot be written in a conf, a number is not
            finite, or `kind` or `recipe` contradicts the converter.
    """
    if type(converter) not in _SUPPORTED:
        raise TypeError(
            f"A {type(converter).__name__} is not a converter that a unit of "
            "a system can have: use a LinearConverter, an OffsetConverter or "
            "a LogarithmicConverter."
        )
    names = list(dict.fromkeys([symbol, *aliases]))
    for name in names:
        _name(name)
    if isinstance(converter, OffsetConverter) != (kind == "absolute"):
        raise ValueError(
            f"A {type(converter).__name__} cannot be a unit of kind {kind!r}."
        )
    dimension_text = _product(dimension.exponents)
    recipe_text = None
    if isinstance(converter, LogarithmicConverter):
        if recipe is not None:
            raise ValueError("A level has no recipe.")
        fields = [
            "Log",
            _number(converter.factor, None),
            _number(converter.reference, None),
            dimension_text,
        ]
    elif isinstance(converter, OffsetConverter):
        if converter.offset == 0 or recipe is not None:
            raise ValueError(
                "An OffsetConverter needs a zero point, and has no recipe."
            )
        fields = [
            _number(converter.scale, converter.exact_scale),
            _number(converter.offset, converter.exact_offset),
            dimension_text,
        ]
    else:
        assert isinstance(converter, LinearConverter)
        fields = [_number(converter.scale, converter.exact), dimension_text]
        # next to a dimensionless "1" a third field reads as a zero point
        if recipe is not None and dimension_text != "1":
            recipe_text = _product(recipe.exponents)
    if not allow_prefixes:
        fields.append("noprefix")
    if recipe_text is not None:
        fields.append(recipe_text)
    return f"[Units]\n{symbol} = {', '.join(fields)}, [{', '.join(names)}]\n"


def prefix_definition_text(
    symbol: str,
    factor: float,
    name: str | None = None,
    exact: Fraction | None = None,
) -> str:
    """Writes the `[Prefixes]` text that defines a prefix in the registry.

    Args:
        symbol: The symbol of the prefix (``"k"``).
        factor: Its factor.
        name: Its spelled-out name (``"kilo"``); the symbol when omitted.
        exact: The exact value of the factor, when it has one.

    Returns:
        The text: a `[Prefixes]` section of one line.

    Raises:
        ValueError: A name cannot be written in a conf, or the factor is not
            finite.
    """
    label = _name(name or symbol)
    return f"[Prefixes]\n{label} = {_name(symbol)}, {_number(factor, exact)}\n"
```

- [ ] **Step 4: See them pass.** `uv run --no-sync pytest tests/measurement_tests/test_definitions_text.py -q -p no:cacheprovider` (all pass). Then `uv run --no-sync ruff format physure/domain/measurement/definitions_text.py tests/measurement_tests/test_definitions_text.py`, `uv run --no-sync ruff check .`, `uv run --no-sync ruff format --check .` (clean). The import-layer test must still pass: `uv run --no-sync pytest tests/core/test_import_layers.py -q -p no:cacheprovider` (the new module is under `physure.domain.*`, the model layer, and imports only converters).

- [ ] **Step 5: Commit.**

```bash
git add physure-python/physure/domain/measurement/definitions_text.py physure-python/tests/measurement_tests/test_definitions_text.py
git commit -m "feat: write what a system registers as the conf text the core reads

unit_definition_text and prefix_definition_text write a unit or a prefix
as the line of the catalog that would define it. Only a LinearConverter,
an OffsetConverter or a LogarithmicConverter can be written (exactly: a
subclass is another converter); a name a conf cannot hold, a number that
is not finite, a kind that contradicts the converter and a level with a
recipe are refused. The descriptive name of register_unit is not written,
because it is not a name that resolves in Python either. A recipe is never
written next to a dimensionless 1, where the parser would read it as a
zero point.

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: a system owns its registry

**Files:** `physure-python/physure/domain/measurement/system.py`, `physure-python/physure/application/startup.py`, `physure-python/tests/core/test_system_registry.py` (new).

- [ ] **Step 1: Measure the first use before the change.** From `physure-python/` (`unset VIRTUAL_ENV`), five times:

```bash
for i in 1 2 3 4 5; do uv run --no-sync python -c "import time; t = time.perf_counter(); from physure import Q_; Q_(1, 'm'); print(round(time.perf_counter() - t, 3))"; done
```

Write the five numbers down (the median is the reference; they are about 0.1 to 0.2 s).

- [ ] **Step 2: Write the failing tests.** Create `physure-python/tests/core/test_system_registry.py`:

```python
"""A1 stage 2, step 2b: a system's core registry is built from what it registers."""

import pickle
from fractions import Fraction

import pytest

from physure.application.startup import create_system
from physure.domain.measurement.converters import (
    LinearConverter,
    UnitConverter,
)
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.system import UnitSystem

LENGTH = Dimension({"L": 1})


class _Square(UnitConverter):
    is_linear = False

    def to_base(self, value):
        return value**2

    def from_base(self, value):
        return value**0.5


class _Scaled(LinearConverter):
    pass


def _bare_system():
    system = UnitSystem("bare")
    system.register_prefix("k", 1000.0, "kilo", Fraction(1000))
    system.register_unit("m", LENGTH, LinearConverter(1.0), "meter", "meter")
    system.register_unit(
        "ft",
        LENGTH,
        LinearConverter(0.3048, exact=Fraction(381, 1250)),
        "foot",
    )
    return system


def test_what_a_system_registers_is_in_its_core_registry():
    registry = _bare_system().core_registry
    assert registry.facts("ft")["dimension"] == {"L": 1}
    factor = registry.factor_between(
        registry.parse_atomic("ft"), registry.parse_atomic("m")
    )
    assert factor == 0.3048
    # the prefix it registered applies, with its factor
    factor = registry.factor_between(
        registry.parse_atomic("km"), registry.parse_atomic("m")
    )
    assert factor == 1000.0


def test_membership_is_the_systems_own_not_the_registrys():
    system = _bare_system()
    assert "kg" not in system
    # any text parsed also registers the eight SI base units
    assert system.core_registry.facts("kg")["dimension"] == {"M": 1}


@pytest.mark.parametrize("converter", [_Square(), _Scaled(2.0)])
def test_a_converter_a_conf_cannot_say_is_refused_and_changes_nothing(
    converter,
):
    system = _bare_system()
    with pytest.raises(TypeError, match="not a converter"):
        system.register_unit("sq", Dimension({"L": 2}), converter, "square")
    assert "sq" not in system
    assert system.get_definition("sq") is None
    with pytest.raises(ValueError, match="Unknown unit"):
        system.core_registry.facts("sq")


def test_a_name_a_conf_cannot_hold_is_refused_and_changes_nothing():
    system = _bare_system()
    with pytest.raises(ValueError, match="cannot be a name"):
        system.register_unit("a,b", LENGTH, LinearConverter(1.0), None)
    assert "a,b" not in system
    with pytest.raises(ValueError, match="cannot be a name"):
        system.register_prefix("x=", 2.0)
    assert "x=" not in system.PREFIX_REGISTRY


def test_a_system_pickles_and_its_registry_is_rebuilt():
    system = _bare_system()
    clone = pickle.loads(pickle.dumps(system))
    assert clone.core_registry.facts("km") == system.core_registry.facts("km")
    clone.register_unit("yd", LENGTH, LinearConverter(0.9144), "yard")
    assert clone.core_registry.facts("yd")["dimension"] == {"L": 1}
    with pytest.raises(ValueError, match="Unknown unit"):
        system.core_registry.facts("yd")


def test_a_unit_of_a_conf_is_in_the_registry(tmp_path, monkeypatch):
    conf = (
        "[Prefixes]\nzorro = zz, 1/3\n[Units]\nfurlong = 201.168, L, [fur]\n"
    )
    (tmp_path / "physure.conf").write_text(conf, encoding="utf-8")
    monkeypatch.chdir(tmp_path)
    registry = create_system().core_registry
    assert registry.factor_between(
        registry.parse_atomic("fur"), registry.parse_atomic("m")
    ) == pytest.approx(201.168)
    assert registry.factor_between(
        registry.parse_atomic("zzm"), registry.parse_atomic("m")
    ) == pytest.approx(1 / 3)


def test_a_conf_line_the_core_cannot_read_stops_the_system_being_built(
    tmp_path, monkeypatch
):
    conf = "[Units]\nleague = lots, L, [lg]\n"
    (tmp_path / "physure.conf").write_text(conf, encoding="utf-8")
    monkeypatch.chdir(tmp_path)
    with pytest.raises(ValueError, match="is not a number"):
        create_system()
```

- [ ] **Step 3: See them fail.** `uv run --no-sync pytest tests/core/test_system_registry.py -q -p no:cacheprovider`. Expected: failures (`'UnitSystem' object has no attribute 'core_registry'`, no `TypeError` for the converters).

- [ ] **Step 4: Edit `system.py`.** All edits are finds that occur once in `physure-python/physure/domain/measurement/system.py`.

(a) Find:
```python
import contextlib
import logging
import math
```
Replace with:
```python
import logging
import math
```

(b) Find:
```python
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.ports.unit_repository import IUnitRepository
```
Replace with:
```python
from physure.domain.measurement.definitions_text import (
    prefix_definition_text,
    unit_definition_text,
)
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.ports.unit_repository import IUnitRepository
```

(c) Find:
```python
        # Initialize Rust Core Registry
        self._core_registry = _RustUnitRegistry()

    def __getstate__(self) -> dict[str, Any]:
        """Custom pickling state to exclude unpickleable Rust registry."""
        state = self.__dict__.copy()
        state["_core_registry"] = None
        return state

    def __setstate__(self, state: dict[str, Any]) -> None:
        """Custom unpickling state to restore (empty) Rust registry."""
        self.__dict__.update(state)
        # Re-initialize Rust Core Registry
        self._core_registry = _RustUnitRegistry()
```
Replace with:
```python
        # The registry of the core is built from the definitions of this
        # system, as conf text (see extend_core).
        self._core_registry = _RustUnitRegistry()
        self._core_texts: list[str] = []

    @property
    def core_registry(self) -> _RustUnitRegistry:
        """The registry of the core, built from what this system registers.

        It also knows the eight SI base units whatever the system registered,
        so it is not what answers ``in``.
        """
        return self._core_registry

    def extend_core(self, conf_text: str) -> None:
        """Adds conf text to the core registry of this system, all or nothing.

        Raises:
            ValueError: The core cannot read a line; every such line is named
                and the registry is left as it was.
        """
        self._core_registry.extend_conf(conf_text)
        self._core_texts.append(conf_text)

    def __getstate__(self) -> dict[str, Any]:
        """Pickles the texts of the registry: the registry itself cannot be."""
        state = self.__dict__.copy()
        state["_core_registry"] = None
        return state

    def __setstate__(self, state: dict[str, Any]) -> None:
        """Rebuilds the registry of the core from its texts."""
        self.__dict__.update(state)
        self._core_registry = _RustUnitRegistry()
        for text in self.__dict__.get("_core_texts", ()):
            self._core_registry.extend_conf(text)
```

(d) Find:
```python
        """Registers a prefix with its symbol, factor, and optional name."""
        if symbol in self.PREFIX_REGISTRY:
            log.warning("Prefix '%s' is being redefined.", symbol)
```
Replace with:
```python
        """Registers a prefix with its symbol, factor, and optional name.

        Raises:
            ValueError: The core cannot take the prefix; nothing is registered.
        """
        self.extend_core(prefix_definition_text(symbol, factor, name, exact))
        self._register_prefix(symbol, factor, name, exact)

    def _register_prefix(
        self,
        symbol: str,
        factor: float,
        name: str | None = None,
        exact: Fraction | None = None,
    ) -> None:
        """Registers a prefix in Python only (see extend_core)."""
        if symbol in self.PREFIX_REGISTRY:
            log.warning("Prefix '%s' is being redefined.", symbol)
```

(e) Find:
```python
        """Registers a unit and its aliases with the system."""
        unit_def = UnitDefinition(
```
Replace with:
```python
        """Registers a unit and its aliases with the system.

        The unit is written as conf text and given to the core registry
        first, so a unit the core cannot take leaves the system as it was.

        Raises:
            TypeError: The converter is not one a conf line can express.
            ValueError: The core cannot take the unit.
        """
        self.extend_core(
            unit_definition_text(
                symbol,
                dimension,
                converter,
                aliases,
                recipe=recipe,
                allow_prefixes=allow_prefixes,
                kind=kind,
            )
        )
        self._register_unit(
            symbol,
            dimension,
            converter,
            name,
            *aliases,
            recipe=recipe,
            allow_prefixes=allow_prefixes,
            kind=kind,
        )

    def _register_unit(
        self,
        symbol: str,
        dimension: Dimension,
        converter: UnitConverter,
        name: str | None,
        *aliases: str,
        recipe: CompoundUnit | None = None,
        allow_prefixes: bool = True,
        kind: str = "delta",
    ) -> None:
        """Registers a unit in Python only (see extend_core)."""
        unit_def = UnitDefinition(
```

(f) Find:
```python
        self._register_unit_recipe_or_base(symbol, recipe, *aliases)
        self._register_rust_aliases(symbol, *aliases)
```
Replace with:
```python
        self._register_unit_recipe_or_base(symbol, recipe, *aliases)
```

(g) Find:
```python
        """Registers the recipe (derived) or base-unit alias and Rust entry."""
```
Replace with:
```python
        """Registers the recipe (derived) or base-unit alias."""
```

(h) Find:
```python
            self.register_alias(recipe.exponents, symbol, *aliases)

            if self._core_registry:
                # CompoundUnit IS a RationalUnit when Rust is active;
                # pass it directly without any manual conversion.
                with contextlib.suppress(Exception):
                    self._core_registry.add_derived_unit(symbol, recipe)
        else:
            self.register_alias({symbol: 1}, symbol, *aliases)

            if self._core_registry:
                self._core_registry.add_base_unit(symbol)

    def _register_rust_aliases(self, symbol: str, *aliases: str) -> None:
        """Registers all aliases in the Rust core registry."""
        if not self._core_registry:
            return
        for alias in aliases:
            with contextlib.suppress(Exception):
                self._core_registry.register_alias(alias, symbol)
```
Replace with:
```python
            self.register_alias(recipe.exponents, symbol, *aliases)
        else:
            self.register_alias({symbol: 1}, symbol, *aliases)
```

(i) Find:
```python
                exact=prefixed_exact,
            )
            if self._core_registry:
                self._register_prefixed_rust(
                    base_def.symbol, prefixed_symbol, prefixed_name or ""
                )

        if (
```
Replace with:
```python
                exact=prefixed_exact,
            )

        if (
```

(j) Find:
```python
    def _register_prefixed_rust(
        self, unit_name: str, prefixed_symbol: str, prefixed_name: str
    ) -> None:
        """Registers the prefixed unit in the Rust registry."""
        try:
            base_ru = self._core_registry.get_unit(unit_name)
            self._core_registry.add_derived_unit(prefixed_symbol, base_ru)
            if prefixed_name and prefixed_name != prefixed_symbol:
                self._core_registry.register_alias(
                    prefixed_name, prefixed_symbol
                )
        except Exception:
            pass

    def get_unit(self, unit_expression: str) -> CompoundUnit:
```
Replace with:
```python
    def get_unit(self, unit_expression: str) -> CompoundUnit:
```

(k) Find:
```python
        # Rust Registry Check (New Path)
        if (
            self._core_registry
            and hasattr(self._core_registry, "contains")
            and self._core_registry.contains(unit_expression)
        ):
            try:
                ru = self._core_registry.get_unit(unit_expression)
                return CompoundUnit(ru.dimensions)
            except Exception as e:
                log.warning(
                    f"Failed to retrieve unit '{unit_expression}' from Rust: {e}"
                )
                # Fallback to parsing

        # Lazy prefix decomposition (km, kilometer, ...): materializes the
```
Replace with:
```python
        # Lazy prefix decomposition (km, kilometer, ...): materializes the
```

(l) Find:
```python
        return (
            item in self.UNIT_DIMENSIONS
            or item in self.ALIAS_TO_EXPONENTS
            or (
                self._core_registry is not None
                and hasattr(self._core_registry, "contains")
                and self._core_registry.contains(item)
            )
        )
```
Replace with:
```python
        return item in self.UNIT_DIMENSIONS or item in self.ALIAS_TO_EXPONENTS
```

- [ ] **Step 5: Edit `startup.py`.** In `physure-python/physure/application/startup.py`:

(a) Find:
```python
class UnitSystemBuilder:
    """A builder class for constructing a UnitSystem instance."""
```
Replace with:
```python
def _conf_section(section: str, entries: dict[str, str]) -> str:
    """Writes entries of the merged conf as the text the core reads."""
    lines = "".join(f"{key} = {value}\n" for key, value in entries.items())
    return f"[{section}]\n{lines}"


class UnitSystemBuilder:
    """A builder class for constructing a UnitSystem instance."""
```

(b) Find:
```python
        for name, value_str in prefixes_data.items():
            symbol, factor_str = [p.strip() for p in value_str.split(",")]
            self._system.register_prefix(
                symbol=symbol,
                factor=float(factor_str),
```
Replace with:
```python
        self._system.extend_core(_conf_section("Prefixes", prefixes_data))
        for name, value_str in prefixes_data.items():
            symbol, factor_str = [p.strip() for p in value_str.split(",")]
            self._system._register_prefix(
                symbol=symbol,
                factor=_number(factor_str),
```

(c) Find:
```python
        self._system.register_unit(
            symbol,
            dimension,
            converter,
            key,
```
Replace with:
```python
        self._system._register_unit(
            symbol,
            dimension,
            converter,
            key,
```

(d) Find:
```python
        self._system.unit_definitions.update(units_data)

        # Pass 1: Register all units
```
Replace with:
```python
        self._system.unit_definitions.update(units_data)
        self._system.extend_core(_conf_section("Units", units_data))

        # Pass 1: Register all units
```

- [ ] **Step 6: See them pass, then the whole suite.** From `physure-python/`: `uv run --no-sync pytest tests/core/test_system_registry.py -q -p no:cacheprovider` (all pass; if the pickling test fails because a part of the system cannot be pickled, stop and report it). Then `uv run --no-sync ruff format physure/domain/measurement/system.py physure/application/startup.py tests/core/test_system_registry.py`, `uv run --no-sync ruff check .`, `uv run --no-sync ruff format --check .`, and the whole suite `uv run --no-sync pytest -q -p no:cacheprovider` (expect 1145 plus the new tests, the same 5 xfailed; `test_uncertainty_modes` is a known flake: rerun it alone if it is the only failure).

- [ ] **Step 7: Measure the first use after the change.** Repeat the five runs of Step 1. Expected: the median is not more than 10% above the reference, and under 0.5 s (the change removes the mirror's registrations and adds two parses of about half a millisecond).

- [ ] **Step 8: Commit.**

```bash
git add physure-python/physure/domain/measurement/system.py physure-python/physure/application/startup.py physure-python/tests/core/test_system_registry.py
git commit -m "feat: a unit system owns the registry of the core, strictly

UnitSystem.core_registry is built from what the system registers.
create_system gives it the merged [Prefixes] and [Units] text once, and
register_unit and register_prefix write what they register as conf text
and give it to the registry first, so a unit the core cannot take leaves
the system as it was: a converter that is not exactly a LinearConverter,
OffsetConverter or LogarithmicConverter is a TypeError, a name, number or
kind the core cannot take is a ValueError. A conf line the core cannot
read stops create_system with every such line named. A prefix factor of a
conf can be a fraction for the builder as for the core. The system keeps
the texts, so pickling rebuilds the registry.

The best-effort mirror is gone: its three contextlib.suppress sites,
prefixed copies registered as units without their prefix factor, and the
two lookup paths of get_unit and 'in' that returned a unit without its
scale (no test of the suite reached them).

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: the registry answers what Python answers

**Files:** `physure-python/tests/core/test_system_registry_parity.py` (new).

The comparison is made inside `use_system(system)`, right after the system is built: Python's conversion caches are global, so a comparison made later meets the state of another system.

- [ ] **Step 1: Write the tests.** Create `physure-python/tests/core/test_system_registry_parity.py`:

```python
"""A1 stage 2, step 2b: the registry of a system answers what the system answers.

Nothing reads the registry of a system yet. Before something does (step 3), its
answers are compared with Python's, system by system: the dimension, the kind
and the linearity of every name Python resolves, and the factor between the
linear units of a dimension.
"""

import collections
from fractions import Fraction

import pytest

from physure.application.context import use_system
from physure.application.startup import create_default_system, create_system
from physure.domain.exceptions import UnknownUnitError
from physure.domain.measurement.converters import (
    LinearConverter,
    LogarithmicConverter,
    OffsetConverter,
)
from physure.domain.measurement.dimensions import Dimension
from physure.domain.measurement.system import UnitSystem
from physure.domain.measurement.units import CompoundUnit
from physure.ext.currency import enable_currency
from physure.units._index import UNIT_INDEX

FIELDS = ("dimension", "kind", "is_linear")


def _plain(number):
    value = float(number)
    return int(value) if value.is_integer() else value


def _python_facts(system, name):
    unit = system.Q_(1.0, name).unit
    return {
        "dimension": {
            key: _plain(exp)
            for key, exp in sorted(unit.dimension(system).exponents.items())
        },
        "kind": unit.kind(system),
        "is_linear": unit.is_linear(system),
    }


def _the_registry_agrees(system, names):
    """Checks every name Python resolves, and returns how many there were."""
    registry = system.core_registry
    resolved = {}
    with use_system(system):
        for name in names:
            try:
                resolved[name] = _python_facts(system, name)
            except UnknownUnitError:
                continue
        for name, python in resolved.items():
            facts = registry.facts(name)
            for field in FIELDS:
                assert facts[field] == python[field], (name, field)
        # the factor of each linear unit to the first unit of its dimension
        groups = collections.defaultdict(list)
        for name, python in resolved.items():
            if python["is_linear"]:
                groups[tuple(python["dimension"].items())].append(name)
        for dimension, group in groups.items():
            references = [group[0]]
            for name in group[1:]:
                for reference in references:
                    try:
                        factor = registry.factor_between(
                            registry.parse_atomic(name),
                            registry.parse_atomic(reference),
                        )
                    except ValueError:
                        # Python files an angle and a solid angle under one
                        # dimension and converts between them; the core keeps
                        # them apart. Known, and closed by step 6
                        # (test_python_does_not_mix_angle_and_solid_angle).
                        assert dimension == (("A", 1),), (name, reference)
                        continue
                    python = system.Q_(1.0, name).to(reference).magnitude
                    assert python == pytest.approx(factor, rel=1e-12), (
                        name,
                        reference,
                    )
                    break
                else:
                    references.append(name)  # another kind of the dimension
    return len(resolved)


def test_the_default_system():
    # every fourth name of the catalog: all of them are compared with the
    # catalog's own registry in test_registry_facts.py
    system = create_default_system()
    assert _the_registry_agrees(system, sorted(UNIT_INDEX)[::4]) > 500


def test_the_imperial_system():
    system = create_system("imperial.conf")
    assert _the_registry_agrees(system, sorted(UNIT_INDEX)[::4]) > 500


def test_a_system_with_the_conf_of_the_user(tmp_path, monkeypatch):
    conf = (
        "[Prefixes]\nzorro = zz, 0.5\n"
        "[Units]\nfurlong = 201.168, L, [fur, furlong]\n"
        "smoot = 1.7018, L, [smoot]\n"
    )
    (tmp_path / "physure.conf").write_text(conf, encoding="utf-8")
    monkeypatch.chdir(tmp_path)
    names = ["fur", "furlong", "smoot", "zzm", "zorrometer", "m"]
    assert _the_registry_agrees(create_system(), names) == len(names)


def test_a_system_with_currency():
    system = create_default_system()
    enable_currency(system)
    names = ["USD", "EUR", "MXN", "dollars", "dolar", "euros", "pesos", "m"]
    assert _the_registry_agrees(system, names) == len(names)
    registry = system.core_registry
    assert registry.facts("EUR")["dimension"] == {"$": 1}
    factor = registry.factor_between(
        registry.parse_atomic("EUR"), registry.parse_atomic("USD")
    )
    assert factor == 1.08


def test_a_bare_system_with_units_registered_by_hand():
    system = UnitSystem("bare")
    length, mass, time = (Dimension({s: 1}) for s in ("L", "M", "T"))
    heat = Dimension({"O": 1})
    energy = Dimension({"M": 1, "L": 2, "T": -2})
    power = Dimension({"M": 1, "L": 2, "T": -3})
    system.register_prefix("k", 1000.0, "kilo", Fraction(1000))
    system.register_unit("m", length, LinearConverter(1.0), "meter", "meter")
    system.register_unit("cm", length, LinearConverter(0.01), "centimeter")
    system.register_unit(
        "ft",
        length,
        LinearConverter(0.3048, exact=Fraction(381, 1250)),
        "foot",
    )
    system.register_unit("kg", mass, LinearConverter(1.0), "kilogram")
    system.register_unit("s", time, LinearConverter(1.0), "second")
    system.register_unit(
        "J",
        energy,
        LinearConverter(1.0),
        "joule",
        recipe=CompoundUnit({"kg": 1, "m": 2, "s": -2}),
    )
    system.register_unit("W", power, LinearConverter(1.0), "watt")
    system.register_unit(
        "dBW",
        power,
        LogarithmicConverter(10.0),
        "decibel-watt",
        allow_prefixes=False,
    )
    system.register_unit("K", heat, LinearConverter(1.0), "kelvin")
    system.register_unit(
        "degX", heat, OffsetConverter(1.0, 10.0), "x", kind="absolute"
    )
    names = [
        "m", "meter", "cm", "ft", "kg", "s", "J", "W", "dBW", "K", "degX",
        "km", "kilometer",
    ]  # fmt: skip
    assert _the_registry_agrees(system, names) == len(names)
```

- [ ] **Step 2: Run them.** `uv run --no-sync pytest tests/core/test_system_registry_parity.py -q -p no:cacheprovider`. Expected: all pass (Task 3 already made the registries; these tests only compare). If one fails, the failure is a real disagreement between Python and the core: stop and report which name and which field, do not weaken the test.

- [ ] **Step 3: Format and lint.** `uv run --no-sync ruff format tests/core/test_system_registry_parity.py`, `uv run --no-sync ruff check .`, `uv run --no-sync ruff format --check .` (clean).

- [ ] **Step 4: Commit.**

```bash
git add physure-python/tests/core/test_system_registry_parity.py
git commit -m "test: the registry of a system answers what the system answers

For the default system, the imperial one, one with a user conf, one with
currency and a bare one registered by hand (prefix, recipe, level, zero
point): the dimension, kind and linearity of every name Python resolves
and the factor between the linear units of each dimension agree with the
registry, compared inside the system that registered them (Python's
conversion caches are global).

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: changelog, design pointer and the full verification

**Files:** `CHANGELOG.md`, `docs/superpowers/specs/2026-10-29-a1-stage2-design.md`.

- [ ] **Step 1: Changelog.** Under `## [Unreleased]` / `### Changed`, as the last bullet before `### Fixed`:

```markdown
- **A unit system owns a registry of the core, built from what it registers.** `UnitSystem.core_registry` is built by `create_system` from the merged `[Prefixes]` and `[Units]` text, and `register_unit` and `register_prefix` write what they register as conf text and give it to the registry first, so a unit the core cannot take leaves the system as it was. **Breaking:** a converter that is not exactly a `LinearConverter`, an `OffsetConverter` or a `LogarithmicConverter` (a custom `UnitConverter`, or a subclass of those) is refused with `TypeError`; a name a conf cannot hold, a number that is not finite, or a `kind` or `recipe` that contradicts the converter, with `ValueError`. `create_system` raises `ValueError` naming every line of `[Units]` or `[Prefixes]` that the core cannot read (the shipped system files have none). A prefix factor in a conf can be a fraction (`third = th, 1/3`), as a unit factor can. The best-effort mirror that kept a core registry in step is gone, with its three `contextlib.suppress` sites, the prefixed units it registered without their prefix factor, and the two lookup paths of `get_unit` and `in` that returned a unit without its scale (no test reached them).
```

- [ ] **Step 2: Design pointer.** Append to `docs/superpowers/specs/2026-10-29-a1-stage2-design.md`:

```markdown

## Outcome of step 2b (a system owns its registry)

- The registry of a system is built from what the system registers: the merged `[Prefixes]` and `[Units]` text for `create_system`, and `unit_definition_text` / `prefix_definition_text` for what is registered by hand, which the system gives to the registry before it changes anything of its own. The texts are kept, so pickling rebuilds the registry.
- The five systems of the parity tests (default, imperial, user conf, currency, bare with a prefix, a recipe, a level and a zero point) agree with the registry on dimension, kind, linearity and conversion factor. The differential test is the contract that steps 3 to 5 keep.
- Membership stays the system's own: the registry knows the eight SI base units whatever a system registered.
- Python's conversion caches are global, not per system. The parity tests compare inside the system that registered the units for that reason; the leak itself ends when steps 3 to 5 remove those caches.
- Step 3 (the seam `physure.domain.measurement.unit_ops`) is planned from here.
```

- [ ] **Step 3: Verify everything.** From the repository root of the worktree: `cargo test --workspace --exclude physure-python --no-fail-fast` (all `test result` lines ok) and `cargo check -p physure-python` (builds). From `physure-python/` (`unset VIRTUAL_ENV`): `uv run --no-sync maturin develop --release`, check `physure._core.__file__` is inside the worktree, `uv run --no-sync ruff check .`, `uv run --no-sync ruff format --check .`, `uv run --no-sync pytest -q -p no:cacheprovider` (all green, the same five xfailed), `uv run --no-sync mkdocs build --strict`. `test_uncertainty_modes` is a known flake (unseeded Monte Carlo): if it is the only failure, rerun it alone.

- [ ] **Step 4: Commit.**

```bash
git add CHANGELOG.md docs/superpowers/specs/2026-10-29-a1-stage2-design.md
git commit -m "docs: changelog and outcome of step 2b (a system owns its registry)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

## Self-review against the design

- **Coverage:** the findings of "Step 2b, refined": the mirror is write-only and is deleted (Task 3), every shape of registration translates (Task 2, validated by the instrumented suite), `name` is not a resolvable name (Task 2 writes the symbol as the key), prefix factors as fractions (Task 1), no recipe next to a dimensionless 1 (Task 2), the registry comes from the merged text for `create_system` (Task 3, startup), the global caches (Task 4 compares inside the system). The decisions: `TypeError` for converters, strict `create_system`, `core_registry`, `kind` and `recipe` contradictions.
- **Placeholders:** none; every edit is an exact find and replace.
- **Type consistency:** `unit_definition_text(symbol, dimension, converter, aliases, *, recipe, allow_prefixes, kind)` is what `register_unit` calls; `prefix_definition_text(symbol, factor, name, exact)` is what `register_prefix` calls; `extend_core(text)` is what the builder and both call; `_register_unit` and `_register_prefix` keep the old bodies and are what the builder calls.
- **Out of scope here:** the seam module (step 3), `Dimension` and `CompoundUnit` on the core (steps 4 and 5), the behaviour changes (step 6), deleting the Python conf parser (step 7), constants and categories in a system's registry.
