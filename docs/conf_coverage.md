# What a frozen configuration covers

A frozen file (`physure.frozen.conf`, see [Freezing the configuration](conf_freeze.md)) copies, seals and
governs the **catalog**: the text of `physure.conf` and your overrides. It identifies the **engine** (the code
that computes) by its `[Engine]` pin and samples its behaviour with **witnesses**. Everything else that can
change a number you see is listed here as **not frozen**, with who owns it and when it moves. This page is part
of the guarantee: if something that changes a result is missing from it, that is a bug of the page.

Read it as an auditor: a row marked `frozen` is held by the file and its seal; a row marked `pinned+witnessed`
or `pinned` lives in the code, identified by the engine pin (keep the binary that `[Engine]` names), and either
exercised by a built-in witness or not; a row marked `not frozen` is outside the guarantee today.

**The format of the tables is strict**, because a test reads them
(`physure-core/tests/conf_coverage.rs`): every table has the columns `Item | Where | Status | Covered by |
Owner`; Status is one of `frozen`, `pinned+witnessed`, `pinned` and `not frozen`; the Item of a `frozen` row
starts with the catalog section or key it is about, written as in `physure.conf`; witnesses are written
as the `[Witness]` key of a frozen file names them (`builtin.units.foot_to_metre`); a `pinned+witnessed` row names at least one, a `pinned`
row says `none`; a `not frozen` row names its owner. The test checks that every catalog key and section named
exists in the shipped `physure.conf`, that every shipped section and every `[Settings]` key the core reads has
a row, and that every witness named is a case of the built-in battery.

## 1. Frozen by the file

The shipped catalog is source 1 of every frozen file and your overrides are source 2; each line of every
section below is an entry that `phs conf diff` compares and the seal covers. A **built-in witness never reads
a setting of the file**: each case pins the settings it depends on at the compiled defaults, so a witness
measures the engine, not your configuration. A **user witness** runs under the file's settings, except the
three loop and pipeline ceilings (fixed at the compiled defaults) and the seed (section 3.3).

### 1.1 The catalog

| Item | Where | Status | Covered by | Owner |
|---|---|---|---|---|
| `[Dimensions]`: the base dimensions and the base unit each is measured in | `physure-core/src/units/physure.conf` | frozen | entries `base_unit`, `base_dimension`, `dimension_base`; every units witness, e.g. `builtin.units.foot_to_metre` | - |
| `[Prefixes]`: the SI prefixes, their exact factors and spelled-out names (`micro = u`) | `physure-core/src/units/physure.conf` | frozen | entries `prefix`, `prefix_exact`, `prefix_name`; `builtin.units.kmh_to_ms`, `builtin.units.atm_to_kpa` | - |
| `[Units]`: every unit, its exact scale, offset (temperatures) or level (dB, pH), aliases, `noprefix`, metadata | `physure-core/src/units/physure.conf` | frozen | entries `derived_unit`, `alias`, `unit`, `unit_names`, `recipe`, `no_prefix`, `unit_meta`; `builtin.units.degf_to_degc_212`, `builtin.units.db_20_to_ratio`, `builtin.units.ph_7_to_ratio` | - |
| `[Constants]`: the physical constants (CODATA 2022 values). **They carry no uncertainty**: the grammar has none, so a measured constant such as G is exact here (audit T14, a row of section 3) | `physure-core/src/units/physure.conf` | frozen | entries `constant`, `constant_meta`; `builtin.const.molar_gas_constant`, `builtin.const.planck_constant`, `builtin.const.speed_of_light_in_vacuum` | - |
| `[Thermochemistry:legacy-python-1]`: the shipped edition of standard enthalpies and entropies of formation (8 species, 298.15 K, 1 bar; its quality is a row of section 3, see [Standard thermochemical data](thermochemistry.md)) | `physure-core/src/units/physure.conf` | frozen | entries `thermochemistry_table`, `thermochemistry_edition`; `builtin.chem.dhf_h2o_l`, `builtin.chem.entropy_h2o_l`, `builtin.chem.gibbs_formation_h2o_l` | - |
| `[Thermochemistry]`: your definitions, which apply with any edition | `physure-core/src/units/physure.conf` | frozen | entries `thermochemistry`; the three thermochemistry witnesses above | - |
| `[AtomicWeights:standard-2024]`: the CIAAW standard atomic weights (see [Standard atomic weights](atomic_weights.md)) | `physure-core/src/units/physure.conf` | frozen | entries `atomic_weight_table`, `atomic_weight_edition`; `builtin.chem.molar_mass_h2o`, `builtin.chem.molar_mass_glucose`, `builtin.chem.mass_to_moles_nacl` | - |
| `[AtomicWeights:abridged-2024]`: the CIAAW abridged table | `physure-core/src/units/physure.conf` | frozen | entries `atomic_weight_table`, `atomic_weight_edition`; read by the molar-mass witnesses only when selected | - |
| `[AtomicWeights]`: your definitions, which apply with any edition | `physure-core/src/units/physure.conf` | frozen | entries `atomic_weight`; `builtin.chem.molar_mass_calcium_hydroxide`, `builtin.chem.molar_mass_iron_sulfate` | - |
| `[Chemistry] atomic_weights`: which edition the chemistry module reads (`standard` is an alias that moves with a release; pin a name to stay put) | `physure-core/src/units/physure.conf` | frozen | entry `atomic_weight_selection`; the molar-mass witnesses | - |
| `[Chemistry] thermochemistry`: which thermochemistry edition is read | `physure-core/src/units/physure.conf` | frozen | entry `thermochemistry_selection`; the thermochemistry witnesses | - |
| `[Categories]`: the unit lists by quantity kind that a unit picker shows (`get_category_units`); no calculation reads them | `physure-core/src/units/physure.conf` | frozen | entry `category` | - |
| `[Compatibility] radian, plain`: a conversion may read the radian as 1 (`m*rad/s => m/s`) | `physure-core/src/units/physure.conf` | frozen | entry `setting radian_plain` (derived from this section) | - |
| `[Compatibility] steradian, plain`: a conversion may read the steradian as 1 (`cd/m^2 => lx`) | `physure-core/src/units/physure.conf` | frozen | entry `setting steradian_plain` (derived from this section) | - |
| `[Compatibility] periodic`: the units reserved for periodic phenomena (`rad/s => Hz` is refused) | `physure-core/src/units/physure.conf` | frozen | the `periodic` flag in the `derived_unit` entry of each unit named | - |

### 1.2 The settings

Every key of `[Settings]` that the core reads (`CORE_SETTING_KEYS` in `physure-core/src/units/conf.rs`). Each is
an entry `setting <key>` with the value in force, the compiled default included, so a file that omits a key
differs from one that sets it to something else. The compiled default reproduces the behaviour from before the
key existed.

| Item | Where | Status | Covered by | Owner |
|---|---|---|---|---|
| `[Settings] propagation_mode`: how uncertainties combine (correlated, uncorrelated, monte_carlo, unscented) | `physure-core/src/uncertainty/mode.rs` | frozen | entry `setting propagation_mode`; built-in cases pin their own mode | - |
| `[Settings] angle_mode`: whether the radian is 1 (`dimensionless`, the SI) or a dimension (`dimension`) | `physure-core/src/units/angle_mode.rs` | frozen | entry `setting angle_mode` | - |
| `[Settings] parallel_threshold`: the element count from which a PHS `for` or `parallel_map` runs on worker threads (never the value of a result: the collect is ordered; a loop in `monte_carlo` mode stays on its thread) | `physure-core/src/settings.rs` | frozen | entry `setting parallel_threshold` | - |
| `[Settings] max_loop_iterations`: the largest range or vector a PHS loop may materialise (an error, not a value) | `physure-core/src/settings.rs` | frozen | entry `setting max_loop_iterations`; witnesses run with it fixed at 1000000 | - |
| `[Settings] max_pipeline_steps`: the step ceiling of one pipeline run | `physure-core/src/settings.rs` | frozen | entry `setting max_pipeline_steps`; witnesses run with it fixed at 1000 | - |
| `[Settings] max_while_iterations`: the ceiling of one PHS `while` loop | `physure-core/src/settings.rs` | frozen | entry `setting max_while_iterations`; witnesses run with it fixed at 10000 | - |
| `[Settings] approx_rel_tol`: the relative tolerance of PHS `≈` and the default of `approx(a, b)` | `physure-core/src/settings.rs` | frozen | entry `setting approx_rel_tol` | - |
| `[Settings] monte_carlo_samples`: how many samples a Monte Carlo measurement draws (2 to 10000000) | `physure-core/src/settings.rs` | frozen | entry `setting monte_carlo_samples`; `builtin.unc.montecarlo_seed_1` pins 1000 | - |
| `[Settings] monte_carlo_seed`: the seed of the Monte Carlo draws, or `random` (not reproducible by choice; `phs conf status` and `verify` warn); its semantics are section 3.3 | `physure-core/src/uncertainty/monte_carlo.rs` | frozen | entry `setting monte_carlo_seed`; `builtin.unc.montecarlo_seed_1` pins seed 1 | - |
| `[Settings] asymmetric_shape`: what a bare asymmetric pair means (`dimidiated` or `fechner`) | `physure-core/src/uncertainty/shapes.rs` | frozen | entry `setting asymmetric_shape`; `builtin.unc.asymmetric_sum` pins dimidiated | - |
| `[Settings] default_dof`: the degrees of freedom of an uncertainty that declares none (`inf` by default) | `physure-core/src/uncertainty/dof.rs` | frozen | entry `setting default_dof`; the unc witnesses pin inf | - |
| `[Settings] display_significant_digits`: the significant digits of a printed decimal number (not the scientific form) | `physure-core/src/quantity.rs` | frozen | entry `setting display_significant_digits`; the display witnesses pin 15 | - |
| `[Settings] display_scientific_below`: below this magnitude a number is printed in scientific notation | `physure-core/src/quantity.rs` | frozen | entry `setting display_scientific_below`; `builtin.display.scientific_below` pins 1e-4 | - |
| `[Settings] display_scientific_from`: from this magnitude a number is printed in scientific notation | `physure-core/src/quantity.rs` | frozen | entry `setting display_scientific_from`; `builtin.display.scientific_from` pins 1e16 | - |
| `[Settings] uncertainty_digits`: the significant figures of a printed uncertainty (`gum` or a count) | `physure-core/src/quantity.rs` | frozen | entry `setting uncertainty_digits`; `builtin.display.gum_figures`, `builtin.display.gum_places` pin gum | - |

The display keys govern the text of `phs`, `phs eval`'s `"text"`, the Java and WebAssembly text; they do **not**
reach Python's `str` (section 3.4). A user witness records value bits, not printed text, so a display key never
changes its hash.

## 2. Pinned by the engine

What the code itself decides. A frozen file does not hold it; the `[Engine]` pin identifies the build that
computes it (with the limits of [section 5 of the freeze guide](conf_freeze.md#5-engine-pinning)), and a
built-in witness, where one exists, detects a change of its bits under `phs conf verify --run`. `pinned` rows
have **no** witness: a change there is seen only through the engine pin, or through a user witness you write.

| Item | Where | Status | Covered by | Owner |
|---|---|---|---|---|
| Student-t and normal quantiles: continued fractions (3000 iterations, 1e-16, 1e-300), Lanczos ln Γ (g = 7, n = 9), the bracket and 200 bisections of the inverse (audit B1-B3) | `physure-core/src/math/stats.rs` | pinned+witnessed | `builtin.stat.t_p95_nu1`, `builtin.stat.t_p95_nu2`, `builtin.stat.t_p95_nu5`, `builtin.stat.t_p95_nu10`, `builtin.stat.t_p95_nu30`, `builtin.stat.t_p95_nu100`, `builtin.stat.t_p99_nu10`, `builtin.stat.t_p99_nu100` | - |
| The normal approximation above ν = 1e6 with the Cornish-Fisher term (B4); the battery's largest ν is 100 | `physure-core/src/math/stats.rs` | pinned | none | - |
| ν_eff truncated to a whole number of at least 1 (GUM G.4.1) and the Welch-Satterthwaite formula (B5) | `physure-core/src/uncertainty/expanded.rs` | pinned+witnessed | `builtin.unc.expanded_k_p95`, `builtin.unc.welch_satterthwaite_g41`, `builtin.unc.gum_h1_nu_eff`, `builtin.unc.gum_h1_uc` | - |
| First-order (correlated and uncorrelated) propagation | `physure-core/src/uncertainty/` | pinned+witnessed | `builtin.unc.sum_correlated`, `builtin.unc.sum_uncorrelated`, `builtin.unc.product_first_order`, `builtin.unc.quotient_first_order`, `builtin.unc.power_first_order` | - |
| The unscented transform (n = 1, λ = 2, weights 2/3, 1/6, 1/6; B6) | `physure-core/src/uncertainty/unscented.rs` | pinned+witnessed | `builtin.unc.unscented_square` | - |
| Monte Carlo draws: per-draw mixing `seed ^ n * 0x9E3779B97F4A7C15`, `StdRng` (ChaCha12), `rand_distr` normal, the sample deviation with M − 1 (B7, B8) | `physure-core/src/uncertainty/monte_carlo.rs` | pinned+witnessed | `builtin.unc.montecarlo_seed_1` | - |
| Asymmetric pairs and moments: 1/√(2π), the maximum skewness 1.6406, `RATIO_MAX = 1e12` (B9); the witness exercises the dimidiated moments, not the extremes | `physure-core/src/uncertainty/shapes.rs` | pinned+witnessed | `builtin.unc.asymmetric_sum` | - |
| Exact unit scales: the rational arithmetic, where it gives up (`MAX_EXPONENT = 400`, 2^53; B13), the scale-1 test that decides a plain number and a display name (B12) | `physure-core/src/units/exact.rs`, `rational.rs` | pinned+witnessed | `builtin.units.foot_to_metre`, `builtin.units.kwh_to_j`, `builtin.units.cal_th_to_j`, `builtin.display.folded_unit` (indirect: they cross these paths) | - |
| The printing algorithm under the display keys: the shortest round-trip text, its rounding, the GUM 1-or-2-figure rule, `@precision`, `x: base` | `physure-core/src/quantity.rs` | pinned+witnessed | `builtin.display.decimal_debris`, `builtin.display.extremes`, `builtin.display.gum_range`, `builtin.display.precision`, `builtin.display.base_units`, `builtin.display.integers_near_1e15` | - |
| Chemistry algorithms: formula parsing, molar masses, reaction balancing, Arrhenius, Clausius-Clapeyron, ΔG = ΔH − TΔS | `physure-core/src/chemistry/` | pinned+witnessed | `builtin.chem.balance_propane_combustion`, `builtin.chem.balance_iron_oxide_reduction`, `builtin.chem.arrhenius`, `builtin.chem.clausius_clapeyron_water`, `builtin.chem.gibbs_298` | - |
| The covariance store's rule for which entries it keeps: exact zeros and negligible correlations are dropped, never by an absolute threshold (B14, fixed) | `physure-core/src/covariance/store.rs` | pinned | none | - |
| Covariance pruning (`max_age 100`, `corr_threshold 1e-6`, off by default; B15) | `physure-core/src/covariance/pruning.rs` | pinned | none | - |
| `Quantity` equality (exact) and PHS equality and `assert(a, b)` (`phs_eq`, relative 1e-12; B10, B11) | `physure-core/src/quantity.rs` | pinned | none | - |
| PHS comparisons `==`, `!=`, the sigma bound `x == t ± kσ`, the rule of `≈` (its tolerance is the key `approx_rel_tol`), conditions that must be Bool (B16-B18, B22): the core battery cannot reach PHS builtins | `physure-script/src/builtins/core.rs` | pinned | none | - |
| PHS `limit` (Richardson extrapolation from h = 1/8 in the variable's unit, T11) and numeric `integral` (tanh-sinh quadrature with an error estimate; B19, B20) | `physure-script/src/builtins/` | pinned | none | - |
| Builtin API defaults: `series` order 6, `linspace` 50 points, `plot3d` grid 25 (B21) | `physure-script/src/builtins/` | pinned | none | - |
| Symbolic-engine epsilons: `dsolve` 1e-12 (repeated roots), 1e-9 in the AST, the checker's denominators up to 12 (B23) | `physure-script/src/symbolic/` | pinned | none | - |
| Sandbox and parser ceilings: call depth 1000 (200 sandboxed), 48 MiB stack, 1 MiB output, parser nesting 128 and 64, witness deadline 60 s (B24): an error or not, never another value | `physure-script/src/limits.rs` | pinned | none | - |

Cheap witnesses that would close `pinned` rows (audit §9): `stat.t_p95_nu2e6` for B4, and PHS-level cases for the
builtins (the core battery cannot call them).

## 3. Not frozen

Each row says who moves it and when. Until then a frozen file **does not govern it**, whatever the file says.

### 3.1 Bindings and their data

| Item | Where | Status | Covered by | Owner |
|---|---|---|---|---|
| Python, Java and WebAssembly honour a frozen file only from Phase 5: Python's `UnitSystem` and Java's catalog calls refuse while one is in force; WebAssembly has no file system and always uses the unfrozen catalog; the bindings do not run witnesses | `physure-python`, `physure-java`, `physure-wasm` | not frozen | none | Phase 5 (bindings) |
| Python exchange rates: `currency.conf`, an opt-in system file read by `enable_currency()` (placeholder rates, not market data) | `physure-python/physure/infrastructure/config/systems/currency.conf` | not frozen | none | Phase 5 |
| Python thermochemistry tables (`STANDARD_ENTHALPY_FORMATION`, `STANDARD_ENTROPY`): equal to the catalog edition today (a test), but read by Python, not the core | `physure-python/physure/ext/chemistry/thermo_kinetics.py` | not frozen | none | chemistry plan 3 (delete, delegate) |
| Python atomic weights (`ATOMIC_WEIGHTS`, 118 elements): disagree with `standard-2024` for 25 elements and read the half-widths as standard uncertainties (C1) | `physure-python/physure/ext/chemistry/species.py` | not frozen | none | chemistry plan 3 (delete, delegate) |
| `imperial.conf` and `international.conf`: Python-only system files (a name and `[System] base_units`) | `physure-python/physure/infrastructure/config/systems/` | not frozen | none | Phase 5 (sources of the frozen file) |
| `[System] base_units`: read by nothing (inert), Python-only; the core and `phs conf` never read `[System]` | the Python system files | not frozen | none | Phase 5 |
| A system file's `[Settings]` and `[Constants]`, and Python's second reading of the working directory's `physure.conf` (C12) | `physure-python/physure/application/startup.py` | not frozen | none | Phase 5 |
| Python's default coverage probability `p = 0.95` (M9; every other surface requires `p`) | `physure-python/physure/domain/measurement/degrees_of_freedom.py` | not frozen | none | Phase 5 (deferred by the owner) |
| Python constants written in code (`c`, `h`, `k_B` in the equivalencies; C2) and the hard-coded unit tables `default_si` / `default_imperial` (C3) | `physure-python`, `physure-core/src/units/registry.rs` | not frozen | none | Phase 5 (remove) |
| Python finite-difference steps for propagation through functions: `h = 1e-7` absolute (unit-dependent), `dx = 1e-8` relative, `epsilon = 1e-5` (C5); the core uses analytic derivatives | `physure-python/physure/domain/measurement/_arithmetic_mixin.py`, `_backend_mixin.py`, `equivalencies.py`, `core/autograd.py` | not frozen | none | Phase 5 (delegate) |
| Python fractional exponents rationalised with `limit_denominator(1000)` (C7) and the dense/sparse covariance switch at 1000 elements (C8) | `physure-python/physure/domain/measurement/` | not frozen | none | Phase 5 |
| Binding comparisons that disagree with the core: Python `Quantity.__eq__` (exact after conversion), `PyQuantity.__eq__`, `approx_eq` defaults `0.1` / `1e-5`, Java `approxEquals` (absolute 1e-6) and `Quantity.equals` (C6) | `physure-python`, `physure-java` | not frozen | none | Phase 5 (binding contract) |
| JAX arrays are float32: the backend never enables `jax_enable_x64` | `physure-python/physure/backends/jax_backend.py` | not frozen | none | Phase 5 |
| The installed versions of scipy, numpy, torch and jax (`solve_ivp`, `curve_fit` defaults; BLAS and torch reductions) | the Python environment | not frozen | none | Phase 5 (recorded as environment) |

### 3.2 Code and inputs outside the file

| Item | Where | Status | Covered by | Owner |
|---|---|---|---|---|
| `.phs` modules and native plugins (`<base_dir>/ext/<stem>.<dll>`) loaded from disk by PHS `use`: code outside the file entirely (the sandbox of the witnesses blocks them) | `physure-script/src/interpreter/statements.rs` | not frozen | none | open (audit Q10: refuse under a frozen file, or record their hashes) |
| The working directory's `physure.conf`: applied only when no frozen file is in force (under one, `phs conf diff` reports it as drift) | `physure-core/src/units/conf.rs` | not frozen | none | by design |
| `PHYSURE_CONF`, `PHYSURE_REQUIRE_FROZEN`, the working directory and its parents: decide which configuration is in force | `physure-core/src/frozen/active.rs` | not frozen | none | by design |
| The CLI language (`lang`) and its search path, `PHS_NO_OPEN`, `PHYSURE_DEBUG`: labels, UI and runtime checks, never a value | `physure-cli/src/config.rs`, `physure-python/physure/core/dispatcher.py` | not frozen | none | by design (not a result) |
| Build-time `PHYSURE_BUILD_COMMIT`, `PHS_BUILD_SHA`: the engine identity, the builder's claim | `physure-core/src/frozen/engine.rs` | not frozen | none | by design (conf_freeze.md section 6) |
| The platform's math library (`exp`, `ln`, `powf`, `sin`, `log10`: UCRT, glibc, musl, or Rust's `libm` on wasm32) can differ in the last bit between targets; no FMA or fast-math is used and builds are deterministic per target. A witness sees a moved last bit in what it computes, never a function no witness calls | the target | not frozen | the built-in battery on each platform | by design: one frozen file per operating system |

### 3.3 Randomness

The seed is a frozen setting (section 1.2); what it makes reproducible is this.

| Item | Where | Status | Covered by | Owner |
|---|---|---|---|---|
| The default seed is 0; `monte_carlo_seed = random` draws operating-system entropy, so a file that says so is not reproducible by choice (`status` and `verify` warn) | `physure-core/src/uncertainty/monte_carlo.rs` | not frozen | none | by design (owner decision) |
| Units of evaluation: one program run (and the TUI and web views of it), one REPL session, each editor-daemon request, one `phs serve` start, an LSP document state, a WebAssembly or Java handle, a Python `PhsInterpreter` each draw one stream from the seed; two measurements in a unit are independent | `physure-core/src/uncertainty/monte_carlo.rs`, `physure-script` | not frozen | none | by design |
| Draws outside a unit (Python `Q_`) come from a stream per thread, `mix(seed, k)` with k the order in which threads first drew: reproducible for one thread doing the same operations in the same order, **not** across threads | `physure-core/src/uncertainty/monte_carlo.rs` | not frozen | none | by design |
| Repeated `phs serve` requests of the same program are **not** independent replicates: each request starts at the same position of the stream and gives the same samples | `physure-cli/src/server.rs` | not frozen | none | by design |
| The language server keeps a document's stream across edits, so a value it shows depends on the edit history; a fresh open (and `phs file.phs`) reproduces | `physure-lsp` | not frozen | none | by design |
| A user witness, `phs eval` and MCP draw the file's integer seed (0 when it is `random`); `phs eval --seed N` wins and the JSON says which | `physure-cli/src/conf_witness.rs`, `physure-script/src/report.rs` | not frozen | none | by design |
| Two properties of the per-draw mixing, kept because changing them would change every seeded bit: seed s draw n + 1 equals seed `s ^ 0x9E3779B97F4A7C15` draw n (other seeds, shifted streams), and a draw of a zero standard deviation still advances the stream (a host that adds such operations shifts later draws) | `physure-core/src/uncertainty/monte_carlo.rs` | not frozen | `builtin.unc.montecarlo_seed_1` | by design |
| A bump of `rand` or `rand_distr` may move Monte Carlo bits (neither promises stable draws across versions); the version string does not show it | `physure-core/Cargo.toml`, `Cargo.lock` | not frozen | `builtin.unc.montecarlo_seed_1` | by design: review with `verify --run`, accept with `--accept-witness-changes` |
| Python `symbolic_regression` is unseeded by default (`random.Random(None)`), with its search constants in code (C9) | `physure-python/physure/ext/symbolic_regression.py` | not frozen | none | Phase 5 (document, seed) |
| Python `nn.layers` initialise from torch's global random generator (C10) | `physure-python/physure/nn/layers.py` | not frozen | none | Phase 5 |

### 3.4 Printed text the display keys do not govern

| Item | Where | Status | Covered by | Owner |
|---|---|---|---|---|
| `frac` / `ifrac`: the 15-digit cut (`{:.14e}`) before the fraction and `MAX_DENOM = 10000` | `physure-core/src/quantity.rs` | not frozen | `builtin.display.fractions` | engine (no key planned) |
| A number quoted in an error message: an integer below 1e15 without a decimal point | `physure-core/src/quantity.rs` | not frozen | none | engine (no key planned) |
| The 17-decimal cap of the decimal form, and `-0.00` that `gum_round` writes for a negative-zero mean (where `format_float` writes `0.0`) | `physure-core/src/quantity.rs` | not frozen | `builtin.display.extremes` | engine (no key planned) |
| PHS `format()` with a spec (`.3f`, `.2e`, `.4g`): precision 6 by default and Rust's formatting, ignoring the display keys | `physure-script/src/builtins/core.rs` | not frozen | none | engine (no key planned) |
| Plot axis labels `{:.3e}` and the symbolic printer's integer cut at 1e15 | `physure-script/src/builtins/plot.rs`, `physure-script/src/symbolic/display.rs` | not frozen | none | engine (no key planned) |
| Python `str` and `repr` of a quantity: the raw float text, no GUM rounding, the unit not folded (C11) | `physure-python/physure/domain/measurement/quantity.py` | not frozen | none | Phase 5 (call the core's display) |
| Unit names written by code tables, not the catalog: `µ` where the catalog's micro is `u`, 16 derived symbols (M10). When it moves, unit text changes, and with it the hash of a user witness (`unit=` is part of `phs-witness-1`) | `physure-core/src/units/rational.rs` | not frozen | none | Phase 5 (deferred by the owner) |

### 3.5 Accepted, ignored, and known gaps

| Item | Where | Status | Covered by | Owner |
|---|---|---|---|---|
| Settings accepted and ignored: `default_output`, `auto_simplify`, `default_system`, `readable_representation`, `verbose`, `mkml_recursion_limit` (`CORE_UNUSED_SETTINGS`). Nothing reads them; a frozen file keeps them as `setting_ignored` entries and `status` / `verify` warn when a source other than the embedded catalog sets one. **Do not assume they do anything** | `physure-core/src/units/conf.rs` | not frozen | none | by design (kept so that older confs read) |
| The uncertainties of the catalog constants: none is recorded (T14) | `physure-core/src/units/physure.conf` | not frozen | none | a later catalog task (grammar for a `[Constants]` uncertainty) |
| `legacy-python-1` is textbook data rounded to 0.1, with no uncertainty and mixed sources (NH3(g) S° about 7 σ from CODATA); a better-sourced edition with uncertainties is future work | `physure-core/src/units/physure.conf` | not frozen | none | future edition (docs/thermochemistry.md) |
| `phs conf status` names the atomic-weight edition in force but has no thermochemistry line (`phs conf show` lists the thermochemistry entries) | `physure-cli/src/conf.rs` | not frozen | none | Phase 4b docs and follow-up |
| An unbound PHS identifier evaluates to its own name as text (T5) | `physure-script` interpreter | not frozen | none | language semantics, owner decision |

## 4. What a validated site must still do itself

The file cannot do these for you:

* **Keep the binary that `[Engine]` names.** What is frozen is data; algorithms are identified, pinned and
  witnessed, not preserved. To reproduce a result years later, run it with that binary.
* **Record the seal outside the directory** (in your quality system) whenever `freeze`, `update` or `seal`
  prints it, and check with `phs conf verify --expect SEAL`: whoever can edit the file can recompute its seal.
* **One frozen file per operating system** when witnesses must match bit for bit: the platform's math library
  can move a last bit (section 3.2).
* **Run `phs conf verify --run` on every machine that evaluates**, after every upgrade and environment change.
* **Pin the names you depend on**: an edition by name (`atomic_weights = standard-2024`), not the moving alias.
* **Keep `monte_carlo_seed` an integer** if results must reproduce, and evaluate Python Monte Carlo on one thread
  (section 3.3).
* **Do not rely on the rows of section 3**: Python, Java and WebAssembly results, modules and plugins loaded with
  `use`, and the Python-only data are outside the guarantee until their owners move them.
