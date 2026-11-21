use crate::settings::{DisplayRules, UncertaintyDigits, MAX_DISPLAY_DIGITS, MIN_DISPLAY_SCIENTIFIC_BELOW};
use crate::uncertainty::{MonteCarloSeed, PropagationMode, ShapeKind, set_propagation_mode};
use crate::units::angle_mode::{set_angle_mode, AngleMode};
use crate::units::rational::RationalUnit;
use crate::units::registry::{is_atomic_weight_edition_name, resolve_atomic_weight_edition, UnitRegistry};
use crate::frozen::load::FileStamp;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

pub const DEFAULT_PHYSURE_CONF: &str = include_str!("physure.conf");

/// Words that can stand among the fields of a unit line without being one of its values.
fn is_flag(field: &str) -> bool {
    matches!(field, "noprefix" | "own_kind" | "interval")
}

/// Whether the values of a unit line carry `flag`, among fields separated by commas or by spaces.
fn has_flag(val_part: &str, flag: &str) -> bool {
    val_part.split(|c: char| c == ',' || c.is_whitespace()).any(|field| field == flag)
}

/// The entries of the shipped catalog that are temperature intervals. The trusted reading (the embedded catalog, and
/// the `embedded` source of a frozen file) flags them even when their line does not: a file frozen before the flag
/// existed (`delta_celsius = 1.0, O, [...]`) must not lose the rule its library already applied by name.
const SHIPPED_INTERVALS: [&str; 2] = ["delta_celsius", "delta_fahrenheit"];

/// Why `interval` cannot flag a unit of a dimension with no temperature point.
const INTERVAL_WITHOUT_A_POINT: &str =
    "`interval` marks the difference of two points of a scale with a zero point (degC, degF), and no unit with a zero point of this dimension is defined before this line: a unit of a dimension without points is neither a point nor an interval";

/// Whether the dimension of the unit registered as `symbol` has a point: a unit with a zero point (`degC`).
fn has_a_point(registry: &UnitRegistry, symbol: &str) -> bool {
    let Some(unit) = registry.derived_units.get(symbol) else { return false };
    registry.derived_units.values().any(|other| other.is_affine() && other.same_dimensions(unit))
}

/// Why `interval` cannot flag a level: it is the logarithm of a ratio, not a temperature that moves a point.
const INTERVAL_ON_A_LEVEL: &str =
    "`interval` marks a temperature interval (how far apart two points are); a level (`Log, ...`) is the logarithm of a ratio and is never one";
/// Why `interval` cannot flag a unit with a zero point.
const INTERVAL_ON_A_POINT: &str =
    "`interval` marks a temperature interval (how far apart two points are), and a unit with a zero point is a temperature point (like degC): a unit is one or the other";
/// Why `interval` cannot flag the base unit of a dimension.
const INTERVAL_ON_A_BASE: &str =
    "`interval` cannot flag the base unit of a dimension: a difference of two points is written in it, so it stands for a point or an interval alike (like K); flag a unit of scale 1 declared after it (`dT = 1.0, O, interval, [dT]`)";

/// Parses a `physure.conf` configuration string into a UnitRegistry and constants map.
/// All dimension symbols (L, M, T, I, O, N, J, A, etc.), prefixes, units, aliases,
/// and physical constants are extracted 100% dynamically from the configuration file.
///
/// This is the **lenient** reading, the one the `physure.conf` of the working directory gets: a line the parser
/// cannot read is skipped, as it always was, and the rest of the text is applied. Nothing is dropped in
/// silence, though: every problem is reported on stderr, once per process for each distinct message. It is
/// untrusted text: a section that would edit a shipped edition of the atomic weights is a problem and is
/// ignored. Use [`extend_registry`] for the definitions of a unit system, which refuses a text with a problem.
pub fn parse_physure_conf(
    conf_str: &str,
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
) {
    report_lenient(&read_leniently(conf_str, registry, constants, false));
}

/// Reads the embedded catalog into `registry`. It alone may define the shipped editions of the atomic weights
/// (`[AtomicWeights:standard-2024]`, ...): no text of a user can, see [`extend_registry`]. Lenient like
/// [`parse_physure_conf`], and it applies the `[Settings]` of the catalog.
pub fn load_embedded_catalog(registry: &mut UnitRegistry, constants: &mut HashMap<String, String>) {
    report_lenient(&read_leniently(DEFAULT_PHYSURE_CONF, registry, constants, true));
}

/// The lenient reading, returning the problems it found instead of reporting them.
fn read_leniently(
    conf_str: &str,
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
    trusted: bool,
) -> Vec<String> {
    let mut problems = Vec::new();
    let reading = Reading { settings: SettingsMode::Apply, trusted, strict_sections: false };
    parse_conf(conf_str, registry, constants, reading, &mut ProcessSettings::default(), &mut problems);
    problems
}

/// The lenient reading, as the runtime reads the working directory's `physure.conf` (a line it cannot read is skipped
/// and the rest applies), except that `[Settings]` and `[Compatibility]` are collected into `collected` instead of
/// applied: **no process-wide state changes**. Returns the problems, i.e. the lines the runtime ignores. For the
/// reports of `phs conf` (`frozen::report::unfrozen_entries`).
pub(crate) fn read_leniently_collected(
    conf_str: &str,
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
    collected: &mut ProcessSettings,
    trusted: bool,
) -> Vec<String> {
    let mut problems = Vec::new();
    let reading = Reading { settings: SettingsMode::Collect, trusted, strict_sections: false };
    parse_conf(conf_str, registry, constants, reading, collected, &mut problems);
    problems
}

/// The problems of a lenient reading that have not been reported yet, as the lines to print: each distinct
/// problem once for each `seen`. The working-directory conf is read again on every call that needs the catalog,
/// and the same bad line must not fill the screen every time.
fn new_lenient_reports(problems: &[String], seen: &mut HashSet<String>) -> Vec<String> {
    problems
        .iter()
        .filter(|problem| seen.insert((*problem).clone()))
        .map(|problem| format!("physure.conf: {problem}; the line is ignored"))
        .collect()
}

/// Prints on stderr the problems of a lenient reading, once per process for each distinct message.
fn report_lenient(problems: &[String]) {
    if problems.is_empty() {
        return;
    }
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let mut seen = SEEN.get_or_init(Mutex::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    for line in new_lenient_reports(problems, &mut seen) {
        eprintln!("{line}");
    }
}

/// Adds the units, prefixes, constants and categories of `conf_str` to `registry`, all or nothing:
/// a line the parser cannot read is a problem, and if there is one the registry and the constants
/// are left as they were and every problem is returned. `[Settings]` is not read: the settings of a
/// conf are process-wide and a registry does not own them. This is the **strict** reading, to use for the
/// definitions a unit system is made of. It is untrusted text: it cannot define or edit a shipped edition of
/// the atomic weights, nor use a name reserved for one.
pub fn extend_registry(
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
    conf_str: &str,
) -> Result<(), Vec<String>> {
    let backup = (registry.clone(), constants.clone());
    let mut problems = Vec::new();
    let reading = Reading { settings: SettingsMode::Skip, trusted: false, strict_sections: false };
    parse_conf(conf_str, registry, constants, reading, &mut ProcessSettings::default(), &mut problems);
    if problems.is_empty() {
        return Ok(());
    }
    (*registry, *constants) = backup;
    Err(problems)
}

/// Replays one source of a frozen file (spec §5.2) into `registry`, `constants` and `collected`, all or nothing.
///
/// Stricter than [`extend_registry`]: besides every problem that one finds, a `[Settings]` line that does not read
/// (an unknown key, a bad value, no `=`), a `[Compatibility]` row that does not read, a section name that is not one
/// of a conf, a line before the first section (a byte-order mark included), a line without `=` in any section, and a
/// `[Constants]` value that is not a number followed by a unit this registry reads, are all problems: the lenient
/// reader drops each of them, and a frozen catalog must not lose anything in silence. `trusted` is for the
/// `embedded` source only (it alone may define the shipped atomic-weight editions).
///
/// **Nothing process-wide is changed**: the `[Settings]` and `[Compatibility]` of the text are parsed, validated and
/// added to `collected`, for whoever activates the catalog to apply ([`ProcessSettings::apply`]). On a problem,
/// `registry`, `constants` and `collected` are left exactly as they were and every problem is returned.
pub(crate) fn replay_source(
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
    collected: &mut ProcessSettings,
    text: &str,
    trusted: bool,
) -> Result<(), Vec<String>> {
    // one reading into copies, kept only when it is clean: the reading is deterministic, so a second (real) run
    // would build the same thing
    let (mut next_registry, mut next_constants, mut next_collected) = (registry.clone(), constants.clone(), collected.clone());
    let mut problems = Vec::new();
    let reading = Reading { settings: SettingsMode::Collect, trusted, strict_sections: true };
    parse_conf(text, &mut next_registry, &mut next_constants, reading, &mut next_collected, &mut problems);
    if !problems.is_empty() {
        return Err(problems);
    }
    *registry = next_registry;
    *constants = next_constants;
    *collected = next_collected;
    Ok(())
}

/// What a reading does with `[Settings]` and the rows of `[Compatibility]`, which are process-wide.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsMode {
    /// Not read at all (the definitions of a unit system: a registry does not own process-wide settings).
    Skip,
    /// Applied as they are read; a value that does not read is reported on stderr and ignored (the lenient readers).
    Apply,
    /// Parsed, validated (a problem otherwise) and collected, never applied (a frozen replay).
    Collect,
}

/// What a reading of a text may do.
#[derive(Clone, Copy)]
struct Reading {
    settings: SettingsMode,
    /// The text is the embedded catalog, which alone may define the shipped editions of the atomic weights.
    trusted: bool,
    /// What the lenient reading drops in silence is a problem: unknown sections, lines outside a section or without
    /// `=`, and constants that are not quantities. For a frozen replay.
    strict_sections: bool,
}

/// Every key of `[Settings]` the core reads: one arm of `read_setting` each, in the order the unknown-key message names
/// them. The frozen entries list exactly these plus the `[Compatibility]` grants (a test in `frozen/entries.rs`), and the
/// coverage table `docs/conf_coverage.md` must have a row for each (`tests/conf_coverage.rs`), so a key cannot be added
/// without saying what it governs.
pub const CORE_SETTING_KEYS: &[&str] = &[
    "propagation_mode",
    "angle_mode",
    "parallel_threshold",
    "max_loop_iterations",
    "max_pipeline_steps",
    "approx_rel_tol",
    "monte_carlo_samples",
    "max_while_iterations",
    "asymmetric_shape",
    "default_dof",
    "monte_carlo_seed",
    "display_significant_digits",
    "display_scientific_below",
    "display_scientific_from",
    "uncertainty_digits",
];

/// The keys of `[Settings]` that every reader accepts and nothing reads: not the core, not PHS, the CLI, the LSP, the
/// WASM or Java bindings, not Python (which stores a conf's `[Settings]` in `UnitSystem.settings` and acts on
/// `propagation_mode` alone; verified by search, conf freeze Phase 4b). They are accepted so that an older conf
/// still reads: a frozen replay keeps them as written (`setting_ignored` entries), applying one does nothing, and
/// `phs conf status` / `verify` warn when a source other than the embedded catalog sets one
/// ([`ignored_setting_warnings`]). Releases before Phase 4b shipped all six in the embedded catalog; it sets none of
/// them now. Any other unknown key is a problem.
pub const CORE_UNUSED_SETTINGS: [&str; 6] =
    ["default_output", "auto_simplify", "default_system", "readable_representation", "verbose", "mkml_recursion_limit"];

/// The keys of [`CORE_UNUSED_SETTINGS`] that the `[Settings]` sections of `text` set, as `(key, value)` in the order
/// they appear (lines are read as the conf readers read them: trimmed, `#` and `;` comments skipped).
pub fn ignored_settings_in(text: &str) -> Vec<(String, String)> {
    let mut section = "";
    let mut found = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
        } else if section == "Settings" {
            if let Ok(Setting::Ignored { key, value }) = read_setting(line) {
                found.push((key, value));
            }
        }
    }
    found
}

/// One warning per key of [`CORE_UNUSED_SETTINGS`] that `text` (named `origin` in the warning) sets: an auditor must
/// not assume that such a key does something. The caller passes every source but the embedded catalog.
pub fn ignored_setting_warnings(origin: &str, text: &str) -> Vec<String> {
    ignored_settings_in(text)
        .into_iter()
        .map(|(key, value)| {
            format!("{origin}: [Settings] {key} = {value} is accepted and ignored: no part of physure reads it, so it changes nothing")
        })
        .collect()
}

/// One line of `[Settings]`, read and checked, not applied.
#[derive(Debug, Clone, PartialEq)]
pub enum Setting {
    PropagationMode(PropagationMode),
    AngleMode(AngleMode),
    ParallelThreshold(usize),
    MaxLoopIterations(usize),
    MaxPipelineSteps(usize),
    /// The relative tolerance of PHS `≈`: finite and in `(0, 1)`.
    ApproxRelTol(f64),
    /// The sample count of a Monte Carlo measurement that names none: from 2 to 10 000 000.
    MonteCarloSamples(usize),
    /// The ceiling of a PHS `while` loop: at least 1.
    MaxWhileIterations(usize),
    /// What a bare `(σ⁻, σ⁺)` pair means: `dimidiated` or `fechner`.
    AsymmetricShape(ShapeKind),
    /// The degrees of freedom of an uncertainty that declares none: greater than zero, or `inf`.
    DefaultDof(f64),
    /// The seed of the Monte Carlo streams: a whole number (`u64`) or `random`.
    MonteCarloSeed(MonteCarloSeed),
    /// The significant digits of the decimal form of a printed number: from 1 to 17.
    DisplaySignificantDigits(u32),
    /// The magnitude below which a printed number is written in scientific notation: from 1e-6 to 1.
    DisplayScientificBelow(f64),
    /// The magnitude from which a printed number is written in scientific notation: finite, at least 1.
    DisplayScientificFrom(f64),
    /// The significant figures of a printed uncertainty: `gum` or from 1 to 17.
    UncertaintyDigits(UncertaintyDigits),
    /// A key of [`CORE_UNUSED_SETTINGS`], kept as written; applying it does nothing.
    Ignored { key: String, value: String },
}

impl Setting {
    /// Sets the process-wide value this setting names.
    pub(crate) fn apply(&self) {
        match self {
            Setting::PropagationMode(mode) => {
                set_propagation_mode(*mode);
            }
            Setting::AngleMode(mode) => {
                set_angle_mode(*mode);
            }
            Setting::ParallelThreshold(n) => {
                crate::settings::set_parallel_threshold(*n);
            }
            Setting::MaxLoopIterations(n) => {
                crate::settings::set_max_loop_iterations(*n);
            }
            Setting::MaxPipelineSteps(n) => {
                crate::settings::set_max_pipeline_steps(*n);
            }
            Setting::ApproxRelTol(rel) => {
                crate::settings::set_approx_rel_tol(*rel);
            }
            Setting::MonteCarloSamples(n) => {
                crate::settings::set_monte_carlo_samples(*n);
            }
            Setting::MaxWhileIterations(n) => {
                crate::settings::set_max_while_iterations(*n);
            }
            Setting::AsymmetricShape(shape) => {
                crate::uncertainty::shapes::set_asymmetric_shape(*shape);
            }
            Setting::DefaultDof(nu) => {
                crate::uncertainty::dof::set_default_dof(*nu);
            }
            Setting::MonteCarloSeed(seed) => {
                crate::uncertainty::set_monte_carlo_seed(*seed);
            }
            Setting::DisplaySignificantDigits(n) => {
                crate::settings::set_display_significant_digits(*n);
            }
            Setting::DisplayScientificBelow(x) => {
                crate::settings::set_display_scientific_below(*x);
            }
            Setting::DisplayScientificFrom(x) => {
                crate::settings::set_display_scientific_from(*x);
            }
            Setting::UncertaintyDigits(digits) => {
                crate::settings::set_uncertainty_digits(*digits);
            }
            Setting::Ignored { .. } => {}
        }
    }
}

/// The process-wide settings a conf gives, one value each (see [`ProcessSettings::effective`]).
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveSettings {
    pub propagation_mode: PropagationMode,
    pub angle_mode: AngleMode,
    pub parallel_threshold: usize,
    pub max_loop_iterations: usize,
    pub max_pipeline_steps: usize,
    pub approx_rel_tol: f64,
    pub monte_carlo_samples: usize,
    pub max_while_iterations: usize,
    pub asymmetric_shape: ShapeKind,
    pub default_dof: f64,
    pub monte_carlo_seed: MonteCarloSeed,
    /// The four display settings (`display_significant_digits`, `display_scientific_below`, `display_scientific_from`,
    /// `uncertainty_digits`).
    pub display: DisplayRules,
    /// Whether a plain number may stand for a radian / a steradian (`[Compatibility]`).
    pub radian_grant: bool,
    pub steradian_grant: bool,
    /// The keys the core accepts for other bindings and does not use, as last written.
    pub ignored: BTreeMap<String, String>,
}

/// The process-wide part of the texts a frozen replay read: their `[Settings]` lines in order (a later one wins when
/// applied) and the rows of the last `[Compatibility]` section that replaces the grants (`None`: no section did; a
/// section with only a `periodic` line keeps the rows, as in the lenient reading).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcessSettings {
    pub settings: Vec<Setting>,
    pub compatibility: Option<Vec<String>>,
}

impl ProcessSettings {
    /// The value of every process-wide setting this file gives: each takes its last value in the file, or the compiled
    /// default if the file does not give it.
    pub fn effective(&self) -> EffectiveSettings {
        use crate::settings::{
            DEFAULT_APPROX_REL_TOL, DEFAULT_MAX_LOOP_ITERATIONS, DEFAULT_MAX_PIPELINE_STEPS, DEFAULT_MAX_WHILE_ITERATIONS,
            DEFAULT_MONTE_CARLO_SAMPLES, DEFAULT_PARALLEL_THRESHOLD,
        };
        let mut effective = EffectiveSettings {
            propagation_mode: PropagationMode::default(),
            angle_mode: AngleMode::default(),
            parallel_threshold: DEFAULT_PARALLEL_THRESHOLD,
            max_loop_iterations: DEFAULT_MAX_LOOP_ITERATIONS,
            max_pipeline_steps: DEFAULT_MAX_PIPELINE_STEPS,
            approx_rel_tol: DEFAULT_APPROX_REL_TOL,
            monte_carlo_samples: DEFAULT_MONTE_CARLO_SAMPLES,
            max_while_iterations: DEFAULT_MAX_WHILE_ITERATIONS,
            asymmetric_shape: ShapeKind::DEFAULT,
            default_dof: f64::INFINITY,
            monte_carlo_seed: MonteCarloSeed::DEFAULT,
            display: DisplayRules::DEFAULT,
            radian_grant: true,
            steradian_grant: true,
            ignored: BTreeMap::new(),
        };
        for setting in &self.settings {
            match setting {
                Setting::PropagationMode(mode) => effective.propagation_mode = *mode,
                Setting::AngleMode(mode) => effective.angle_mode = *mode,
                Setting::ParallelThreshold(n) => effective.parallel_threshold = *n,
                Setting::MaxLoopIterations(n) => effective.max_loop_iterations = *n,
                Setting::MaxPipelineSteps(n) => effective.max_pipeline_steps = *n,
                Setting::ApproxRelTol(rel) => effective.approx_rel_tol = *rel,
                Setting::MonteCarloSamples(n) => effective.monte_carlo_samples = *n,
                Setting::MaxWhileIterations(n) => effective.max_while_iterations = *n,
                Setting::AsymmetricShape(shape) => effective.asymmetric_shape = *shape,
                Setting::DefaultDof(nu) => effective.default_dof = *nu,
                Setting::MonteCarloSeed(seed) => effective.monte_carlo_seed = *seed,
                Setting::DisplaySignificantDigits(n) => effective.display.significant_digits = *n,
                Setting::DisplayScientificBelow(x) => effective.display.scientific_below = *x,
                Setting::DisplayScientificFrom(x) => effective.display.scientific_from = *x,
                Setting::UncertaintyDigits(digits) => effective.display.uncertainty_digits = *digits,
                Setting::Ignored { key, value } => {
                    effective.ignored.insert(key.clone(), value.clone());
                }
            }
        }
        if let Some(rows) = &self.compatibility {
            (effective.radian_grant, effective.steradian_grant) = compatibility_grants(rows);
        }
        effective
    }

    /// Makes the process-wide values those of this file alone ([`ProcessSettings::effective`]), each stored once.
    /// Unlike `reset_to_defaults` followed by [`ProcessSettings::apply`], no reader on another thread can see a
    /// default in between.
    pub(crate) fn activate(&self) {
        let e = self.effective();
        set_propagation_mode(e.propagation_mode);
        set_angle_mode(e.angle_mode);
        crate::settings::set_parallel_threshold(e.parallel_threshold);
        crate::settings::set_max_loop_iterations(e.max_loop_iterations);
        crate::settings::set_max_pipeline_steps(e.max_pipeline_steps);
        crate::settings::set_approx_rel_tol(e.approx_rel_tol);
        crate::settings::set_monte_carlo_samples(e.monte_carlo_samples);
        crate::settings::set_max_while_iterations(e.max_while_iterations);
        crate::uncertainty::shapes::set_asymmetric_shape(e.asymmetric_shape);
        crate::uncertainty::dof::set_default_dof(e.default_dof);
        crate::uncertainty::set_monte_carlo_seed(e.monte_carlo_seed);
        crate::settings::set_display_rules(e.display);
        crate::units::angle_mode::set_marker_grants(e.radian_grant, e.steradian_grant);
    }

    /// Applies every setting in order, then the compatibility rows if a section gave them. The rows were validated
    /// when they were collected. Only the activation of a frozen catalog calls it (Phase 1c, Task 5).
    pub(crate) fn apply(&self) {
        for setting in &self.settings {
            setting.apply();
        }
        if let Some(rows) = &self.compatibility {
            apply_compatibility(rows);
        }
    }
}

/// Why a `[Settings]` line does not read.
enum SettingIssue {
    NotKeyValue,
    Unknown(String),
    /// A value that does not read; `keeping` says what the lenient reading keeps instead.
    Bad { why: String, keeping: &'static str },
}

fn read_setting(line: &str) -> Result<Setting, SettingIssue> {
    let Some((key, raw)) = line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) else {
        return Err(SettingIssue::NotKeyValue);
    };
    let count = |raw: &str| {
        raw.parse::<usize>().map_err(|_| SettingIssue::Bad { why: format!("invalid {key} '{raw}'"), keeping: "default" })
    };
    match key {
        "propagation_mode" => raw
            .parse::<PropagationMode>()
            .map(Setting::PropagationMode)
            .map_err(|why| SettingIssue::Bad { why, keeping: "correlated" }),
        "angle_mode" => raw
            .parse::<AngleMode>()
            .map(Setting::AngleMode)
            .map_err(|why| SettingIssue::Bad { why, keeping: "the angle mode in force" }),
        "parallel_threshold" => count(raw).map(Setting::ParallelThreshold),
        "max_loop_iterations" => count(raw).map(Setting::MaxLoopIterations),
        "max_pipeline_steps" => count(raw).map(Setting::MaxPipelineSteps),
        "approx_rel_tol" => match raw.parse::<f64>() {
            Ok(rel) if rel.is_finite() && rel > 0.0 && rel < 1.0 => Ok(Setting::ApproxRelTol(rel)),
            _ => Err(SettingIssue::Bad {
                why: format!("invalid approx_rel_tol '{raw}': a finite number between 0 and 1 (exclusive)"),
                keeping: "the tolerance in force",
            }),
        },
        "monte_carlo_samples" => {
            use crate::settings::{MAX_MONTE_CARLO_SAMPLES, MIN_MONTE_CARLO_SAMPLES};
            match raw.parse::<usize>() {
                Ok(n) if (MIN_MONTE_CARLO_SAMPLES..=MAX_MONTE_CARLO_SAMPLES).contains(&n) => Ok(Setting::MonteCarloSamples(n)),
                _ => Err(SettingIssue::Bad {
                    why: format!(
                        "invalid monte_carlo_samples '{raw}': a whole number from {MIN_MONTE_CARLO_SAMPLES} to {MAX_MONTE_CARLO_SAMPLES}"
                    ),
                    keeping: "the sample count in force",
                }),
            }
        }
        "max_while_iterations" => match raw.parse::<usize>() {
            Ok(n) if n >= 1 => Ok(Setting::MaxWhileIterations(n)),
            _ => Err(SettingIssue::Bad {
                why: format!("invalid max_while_iterations '{raw}': a whole number of at least 1"),
                keeping: "the ceiling in force",
            }),
        },
        "asymmetric_shape" => match raw {
            "dimidiated" => Ok(Setting::AsymmetricShape(ShapeKind::Dimidiated)),
            "fechner" => Ok(Setting::AsymmetricShape(ShapeKind::Fechner)),
            _ => Err(SettingIssue::Bad {
                why: format!("invalid asymmetric_shape '{raw}': dimidiated or fechner"),
                keeping: "the shape in force",
            }),
        },
        // `inf` (also `infinity`, any case) as Rust reads it; NaN, zero, negatives and `-inf` are not degrees of freedom
        "default_dof" => match raw.parse::<f64>() {
            Ok(nu) if nu > 0.0 => Ok(Setting::DefaultDof(nu)),
            _ => Err(SettingIssue::Bad {
                why: format!("invalid default_dof '{raw}': a number greater than zero, or inf"),
                keeping: "the default in force",
            }),
        },
        "monte_carlo_seed" => raw
            .parse::<MonteCarloSeed>()
            .map(Setting::MonteCarloSeed)
            .map_err(|why| SettingIssue::Bad { why, keeping: "the seed in force" }),
        "display_significant_digits" => match raw.parse::<u32>() {
            Ok(n) if (1..=MAX_DISPLAY_DIGITS).contains(&n) => Ok(Setting::DisplaySignificantDigits(n)),
            _ => Err(SettingIssue::Bad {
                why: format!("invalid display_significant_digits '{raw}': a whole number from 1 to {MAX_DISPLAY_DIGITS}"),
                keeping: "the digits in force",
            }),
        },
        "display_scientific_below" => match raw.parse::<f64>() {
            Ok(x) if (MIN_DISPLAY_SCIENTIFIC_BELOW..=1.0).contains(&x) => Ok(Setting::DisplayScientificBelow(x)),
            _ => Err(SettingIssue::Bad {
                why: format!("invalid display_scientific_below '{raw}': a number from {MIN_DISPLAY_SCIENTIFIC_BELOW:e} to 1"),
                keeping: "the threshold in force",
            }),
        },
        "display_scientific_from" => match raw.parse::<f64>() {
            Ok(x) if x.is_finite() && x >= 1.0 => Ok(Setting::DisplayScientificFrom(x)),
            _ => Err(SettingIssue::Bad {
                why: format!("invalid display_scientific_from '{raw}': a finite number of at least 1"),
                keeping: "the threshold in force",
            }),
        },
        "uncertainty_digits" => raw
            .parse::<UncertaintyDigits>()
            .map(Setting::UncertaintyDigits)
            .map_err(|why| SettingIssue::Bad { why, keeping: "the rule in force" }),
        _ if CORE_UNUSED_SETTINGS.contains(&key) => Ok(Setting::Ignored { key: key.to_string(), value: raw.to_string() }),
        _ => Err(SettingIssue::Unknown(key.to_string())),
    }
}

/// One line of `[Settings]` (`key = value`) as a [`Setting`], or why it does not read.
pub fn parse_setting(line: &str) -> Result<Setting, String> {
    read_setting(line).map_err(|issue| match issue {
        SettingIssue::NotKeyValue => "a setting is `key = value`".to_string(),
        SettingIssue::Unknown(key) => {
            let (last, rest) = CORE_SETTING_KEYS.split_last().expect("the core reads settings");
            format!(
                "unknown setting '{key}': the core reads {} and {last}, and accepts {} for other bindings",
                rest.join(", "),
                CORE_UNUSED_SETTINGS.join(", ")
            )
        }
        SettingIssue::Bad { why, .. } => why,
    })
}

/// One line of `[Settings]` as the lenient readers take it: applied if it reads; a bad value is reported on stderr
/// and the value in force stays; an unknown key or a line without `=` is ignored.
fn apply_setting_leniently(line: &str) {
    match read_setting(line) {
        Ok(setting) => setting.apply(),
        // A conf is read before there is anywhere to report to, and a value it names is not worth refusing to start
        // over. Saying so on stderr beats falling back with nothing on screen: that is the wrong answer that looks
        // right.
        Err(SettingIssue::Bad { why, keeping }) => eprintln!("physure.conf: {why}; keeping {keeping}"),
        Err(SettingIssue::NotKeyValue | SettingIssue::Unknown(_)) => {}
    }
}

/// Why the value of a `[Constants]` line is not a quantity this registry can read: a finite number, then (after
/// whitespace) a unit expression, or nothing for a plain number. `None` when it is one.
fn constant_problem(expr: &str, registry: &UnitRegistry) -> Option<String> {
    constant_quantity(expr, registry).err()
}

/// The quantity the value of a `[Constants]` line stands for, read against `registry`: a finite number, then (after
/// whitespace) a unit expression of the registry, or nothing for a plain number. Every reader of a constant (PHS, the
/// strict replay of a frozen file) goes through this one reading.
///
/// A constant carries no uncertainty: the grammar of `[Constants]` has none, so a value written with `+/-` or `±` is
/// refused with that reason, never read without its uncertainty.
pub fn constant_quantity(expr: &str, registry: &UnitRegistry) -> Result<crate::quantity::Quantity, String> {
    let expr = expr.trim();
    if expr.contains("+/-") || expr.contains('±') {
        return Err(format!(
            "the value of a constant is a number and a unit, not '{expr}': a constant of the catalog carries no \
             uncertainty (`name = number unit`), and one written with it is not supported"
        ));
    }
    let (number, unit) = match expr.split_once(char::is_whitespace) {
        Some((number, unit)) => (number, unit.trim()),
        None => (expr, ""),
    };
    let number = match number.parse::<f64>() {
        Ok(number) if number.is_finite() => number,
        _ => return Err(format!("the value of a constant is a finite number and a unit, not '{expr}'")),
    };
    let unit = if unit.is_empty() {
        RationalUnit::dimensionless()
    } else {
        crate::units::parser::Parser::parse_expression_with_registry(unit, registry)
            .map_err(|e| format!("the unit '{unit}' of the constant cannot be read: {e}"))?
    };
    Ok(crate::quantity::Quantity::new_scalar(number, 0.0, unit, None, None))
}

/// Why the constant `name` of `registry` and the unit of the same name do not carry one value, if the registry holds
/// both (a name such as `bohr_radius` or `electron_volt` is a unit and a constant of the catalog). `None` when the
/// name is not both, or they agree (to a relative 1e-12, the rounding of a scale computed from exact factors).
/// `2 bohr_radius` reads the unit and `2 * bohr_radius` the constant: two values would make them diverge.
pub fn unit_constant_disagreement(name: &str, registry: &UnitRegistry) -> Option<String> {
    let meta = registry.constants_meta.get(name)?;
    let unit = registry.get_unit(name)?;
    let disagree = |why: String| {
        Some(format!(
            "'{name}' is a unit and a constant of the catalog with different values ({why}); a name defined as both \
             must carry the same value, or `2 {name}` and `2 * {name}` would differ"
        ))
    };
    let constant = match constant_quantity(&meta.value, registry) {
        Ok(constant) => constant,
        // the constant is unreadable: that is its own problem, reported where it is read
        Err(_) => return None,
    };
    if unit.is_affine() || unit.is_logarithmic() {
        return disagree(format!("the unit is a point or a level, the constant is {}", meta.value));
    }
    // both are read against the registry, so both are expanded to its base units with their exact scales
    if !unit.convertible_to(&constant.unit) {
        return disagree(format!("the unit and the constant {} have different dimensions", meta.value));
    }
    let factor = unit.ratio_to(&constant.unit);
    let value = constant.value.mean();
    if (factor - value).abs() <= 1e-12 * value.abs() {
        None
    } else {
        disagree(format!("one {name} is {factor:e} {}, the constant is {}", constant.unit.__repr__(), meta.value))
    }
}

/// The sections that a conf has, spelt as they must be. A header that is one of these but for case or for
/// whitespace is a problem, not an unknown section: see [`misspelt_section`].
const SECTION_NAMES: [&str; 10] = [
    "Settings",
    "Dimensions",
    "Prefixes",
    "Units",
    "Constants",
    "Categories",
    "Compatibility",
    "Chemistry",
    "AtomicWeights",
    "Thermochemistry",
];

/// The sections that hold a table by edition, `[<kind>:<edition>]`.
const EDITION_KINDS: [&str; 2] = ["AtomicWeights", "Thermochemistry"];

/// `(kind, edition)` of a header `[<kind>:<edition>]` of [`EDITION_KINDS`], spelt exactly.
fn edition_section(header: &str) -> Option<(&'static str, &str)> {
    EDITION_KINDS.iter().find_map(|kind| Some((*kind, header.strip_prefix(kind)?.strip_prefix(':')?)))
}

/// The right spelling of `header` (the text between the brackets) when it is a near miss of a known section:
/// the same name once trimmed and compared without regard to case, but not spelt exactly so. `None` for the right
/// spelling and for a name that is not close to any section (a system conf may have sections of its own).
fn misspelt_section(header: &str) -> Option<String> {
    let trimmed = header.trim();
    let right = match trimmed.split_once(':') {
        Some((name, edition)) => {
            let kind = EDITION_KINDS.iter().find(|kind| kind.eq_ignore_ascii_case(name.trim()))?;
            format!("{kind}:{}", edition.trim())
        }
        None => SECTION_NAMES.iter().find(|name| name.eq_ignore_ascii_case(trimmed))?.to_string(),
    };
    (right != header).then_some(right)
}

/// The editions that this library ships are frozen (`docs/atomic_weights.md`, section 6; `docs/thermochemistry.md`),
/// and the names that start like them are kept for the editions a later release may ship: a table of a user called
/// `standard-2026` would be merged with the shipped one on the day a release adds it. One rule for every kind of
/// edition: `standard-`, `abridged-` (the atomic weights) and `legacy-` (the thermochemical data) are reserved.
fn is_reserved_edition_name(edition: &str) -> bool {
    RESERVED_EDITION_PREFIXES.iter().any(|prefix| edition.starts_with(prefix))
}

const RESERVED_EDITION_PREFIXES: [&str; 3] = ["standard-", "abridged-", "legacy-"];

fn parse_conf(
    conf_str: &str,
    registry: &mut UnitRegistry,
    constants: &mut HashMap<String, String>,
    reading: Reading,
    collected: &mut ProcessSettings,
    problems: &mut Vec<String>,
) {
    let Reading { settings, trusted, strict_sections } = reading;
    let mut current_section = "";
    // a section header has been seen (a line before the first one belongs to no section)
    let mut opened = false;
    // (section, name) defined by this text, for the strict rule that a name is defined once per text
    let mut defined: HashSet<(String, String)> = HashSet::new();
    let mut dim_to_base: HashMap<String, String> = registry.dimension_base.clone();
    let mut unit_lines: Vec<&str> = Vec::new();
    let mut constant_lines: Vec<(&str, String)> = Vec::new();
    // the names this text defines in `[Constants]`
    let mut text_constants: Vec<String> = Vec::new();
    let mut periodic_names: Vec<String> = Vec::new();
    let mut saw_periodic = false;
    let mut compatibility_rows: Vec<String> = Vec::new();
    let mut has_compatibility = false;
    // (section, species) defined by this text in a thermochemistry section, in every reading: a species written twice
    // in one section of one text is a problem, not a silent choice of one value
    let mut thermochemistry_keys: HashSet<(String, String)> = HashSet::new();

    // Pass 1: Extract Prefixes, Constants, Dimensions, and Base Units
    for line in conf_str.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            opened = true;
            current_section = &line[1..line.len() - 1];
            if strict_sections && !is_conf_section(current_section) && misspelt_section(current_section).is_none() {
                problems.push(format!(
                    "[{current_section}]: not a section of a conf; the sections are [{}], [AtomicWeights:<edition>] and [Thermochemistry:<edition>]",
                    SECTION_NAMES.join("], [")
                ));
                current_section = "";
                continue;
            }
            if let Some(right) = misspelt_section(current_section) {
                // an unknown section is ignored without a word: the choice or the weights in this one would
                // silently fall back to the defaults
                problems.push(format!(
                    "[{current_section}]: not a section name; the section is spelt [{right}] (names are case-sensitive and take no spaces)"
                ));
                current_section = "";
                continue;
            }
            has_compatibility |= current_section == "Compatibility";
            if let Some((kind, edition)) = edition_section(current_section) {
                let shipped = match kind {
                    "AtomicWeights" => registry.atomic_weight_tables.contains_key(edition),
                    _ => registry.thermochemistry_tables.contains_key(edition),
                };
                if !trusted && is_reserved_edition_name(edition) {
                    // one problem for the section, whose lines are not read
                    problems.push(if shipped {
                        format!("[{current_section}]: '{edition}' is a shipped edition and is frozen; a text of yours cannot edit it, define a table under a name of your own")
                    } else {
                        format!("[{current_section}]: names that start with `standard-`, `abridged-` or `legacy-` are reserved for the editions of this library, present and future; choose another name for your table")
                    });
                    current_section = "";
                } else if kind == "AtomicWeights" && is_atomic_weight_edition_name(edition) && resolve_atomic_weight_edition(edition) != edition {
                    // a table called like an alias could never be selected: the alias would shadow it
                    problems.push(format!(
                        "[{current_section}]: '{edition}' is an alias of '{}' and cannot name a table; choose another name",
                        resolve_atomic_weight_edition(edition)
                    ));
                    current_section = "";
                } else if is_atomic_weight_edition_name(edition) {
                    // the table exists as soon as its section does, even before its first line
                    if kind == "AtomicWeights" {
                        registry.atomic_weight_tables.entry(edition.to_string()).or_default();
                    } else {
                        registry.thermochemistry_tables.entry(edition.to_string()).or_default();
                    }
                } else {
                    problems.push(format!(
                        "[{current_section}]: {EDITION_NAME_RULE}, not '{edition}'"
                    ));
                    // one problem for the section, not one per line of it
                    current_section = "";
                }
            }
            continue;
        }

        if strict_sections {
            if !opened {
                let what = if line.starts_with('\u{FEFF}') { " (it starts with a byte-order mark)" } else { "" };
                problems.push(format!("{line}: a line before the first section{what}; every line belongs to a [section]"));
                continue;
            }
            if !current_section.is_empty() && !line.contains('=') {
                problems.push(format!("{line}: expected `key = value` in [{current_section}]"));
                continue;
            }
            if let Some(why) = strict_line_problem(current_section, line, &mut defined) {
                problems.push(format!("{line}: {why}"));
                continue;
            }
        }

        match current_section {
            "Settings" => match settings {
                SettingsMode::Skip => {}
                SettingsMode::Apply => apply_setting_leniently(line),
                SettingsMode::Collect => match parse_setting(line) {
                    Ok(setting) => collected.settings.push(setting),
                    Err(why) => problems.push(format!("{line}: {why}")),
                },
            },
            "Prefixes" => read_prefix(line, registry, problems),
            "Units" => {
                unit_lines.push(line);
                declare_base_unit(line, registry, &mut dim_to_base);
            }
            "Constants" => {
                if let Some(name) = read_constant(line, registry, constants) {
                    text_constants.push(name.clone());
                    if strict_sections {
                        if name.is_empty() {
                            problems.push(format!("{line}: a constant has a name"));
                        } else {
                            constant_lines.push((line, constants[&name].clone()));
                        }
                    }
                }
            }
            "AtomicWeights" => read_atomic_weight(line, &mut registry.atomic_weights, problems),
            "Chemistry" => read_chemistry(line, registry, problems),
            "Thermochemistry" => {
                let definitions = &mut registry.thermochemistry;
                read_thermochemistry(line, current_section, definitions, false, &mut thermochemistry_keys, problems);
            }
            "Categories" => read_category(line, registry),
            "Compatibility" => {
                if settings == SettingsMode::Collect && !line.split_once('=').is_some_and(|(key, _)| key.trim() == "periodic") {
                    if let Err(why) = read_compatibility_row(line) {
                        problems.push(why);
                    }
                }
                read_compatibility_line(line, &mut periodic_names, &mut saw_periodic, &mut compatibility_rows)
            }
            table => match edition_section(table) {
                Some(("AtomicWeights", edition)) => {
                    read_atomic_weight(line, registry.atomic_weight_tables.entry(edition.to_string()).or_default(), problems);
                }
                Some((_, edition)) => {
                    let table_of = registry.thermochemistry_tables.entry(edition.to_string()).or_default();
                    read_thermochemistry(line, current_section, table_of, true, &mut thermochemistry_keys, problems);
                }
                None => {}
            },
        }
    }

    register_fallback_bases(registry, &mut dim_to_base);

    // the names to check for a unit and a constant of different values: every constant when this text defines units
    // (one of them may be a constant's name, or an alias of one), else the constants this text defines
    let mut clash_candidates: Vec<String> = if unit_lines.is_empty() {
        text_constants
    } else {
        registry.constants_meta.keys().cloned().collect()
    };

    // Pass 2: Parse and register all units using the dynamic dimension map
    for line in unit_lines {
        if let Err(reason) = parse_unit_line(line, registry, &dim_to_base, trusted) {
            problems.push(format!("{line}: {reason}"));
        } else if strict_sections {
            if let Some(why) = unread_unit_fields(line, &dim_to_base) {
                problems.push(format!("{line}: {why}"));
            }
        }
    }

    mark_periodic(registry, &periodic_names, problems);

    // the constants of this text, once its units are registered
    for (line, expr) in constant_lines {
        if let Some(why) = constant_problem(&expr, registry) {
            problems.push(format!("{line}: {why}"));
        }
    }

    // A name that is a unit and a constant carries one value. A strict reading refuses the text; a lenient one drops
    // the constant, so the name reads as the unit wherever it is written.
    clash_candidates.sort();
    clash_candidates.dedup();
    for name in clash_candidates {
        if let Some(why) = unit_constant_disagreement(&name, registry) {
            problems.push(why);
            registry.constants_meta.remove(&name);
            constants.remove(&name);
        }
    }

    // A section replaces the rows when it has rows, or has no `periodic` line at all (an empty section is an explicit
    // "none"). A section that only has a `periodic` line, even an empty one, leaves them alone: a project that adds a
    // unit of its own must not lose the grants.
    if has_compatibility && (!compatibility_rows.is_empty() || !saw_periodic) {
        match settings {
            SettingsMode::Skip => {}
            SettingsMode::Apply => apply_compatibility(&compatibility_rows),
            SettingsMode::Collect => collected.compatibility = Some(compatibility_rows),
        }
    }
}

/// What the strict reading of a frozen source refuses in one `key = value` line that the reader would otherwise
/// drop in part: a name of `[Units]`, `[Constants]` or `[Prefixes]` defined a second time in the same text (the first
/// definition would vanish), a prefix with more than `symbol, factor`, a constant with more than
/// `value, description, latex`.
fn strict_line_problem(section: &str, line: &str, defined: &mut HashSet<(String, String)>) -> Option<String> {
    if !matches!(section, "Units" | "Constants" | "Prefixes") {
        return None;
    }
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if !defined.insert((section.to_string(), key.to_string())) {
        return Some(format!("'{key}' is defined a second time in [{section}] of this text; the first definition would be lost"));
    }
    let fields = value.split(',').count();
    match section {
        "Prefixes" if fields > 2 => Some("a prefix is `name = symbol, factor`; the fields after the factor are not read".into()),
        "Constants" if fields > 3 => Some(
            "a constant is `name = value, description, latex`; the fields after the third are not read (a description has no comma)".into(),
        ),
        _ => None,
    }
}

/// The values of a unit line the reader does not look at, if there are any (a misspelt flag, a stray word): a level
/// reads `Log, factor, reference, dimension`; another unit its factor, an optional zero point, its dimension, and a
/// recipe only when the dimension is not one with a base unit. The flags are `noprefix`, `own_kind` and `interval`.
fn unread_unit_fields(line: &str, dim_to_base: &HashMap<String, String>) -> Option<String> {
    let (_, val_part, _) = cut_unit_line(line)?;
    let parts = unit_value_fields(val_part);
    let read = if parts.first().is_some_and(|p| p.eq_ignore_ascii_case("log")) {
        4
    } else {
        let (_, _, dim_index) = zero_point(&parts);
        let dim = parts.get(dim_index).copied().unwrap_or("1");
        match dim_to_base.get(dim) {
            _ if dim == "1" || dim.is_empty() => dim_index + 1,
            // the catalog names the base unit after its dimension (`bohr = 5.29e-11, L, m`): it says what the
            // dimension already says, so nothing is lost
            Some(base) if parts.get(dim_index + 1) == Some(&base.as_str()) => dim_index + 2,
            Some(_) => dim_index + 1,
            None => dim_index + 2,
        }
    };
    let unread = parts.get(read..).filter(|rest| !rest.is_empty())?;
    Some(format!(
        "the values {} are not read (the flags are noprefix, own_kind and interval; a recipe is read only for a dimension without a base unit, or when it is that base unit)",
        unread.iter().map(|p| format!("'{p}'")).collect::<Vec<_>>().join(", ")
    ))
}

/// Whether `header` (the text between the brackets) names a section of a conf, spelt exactly.
fn is_conf_section(header: &str) -> bool {
    SECTION_NAMES.contains(&header) || edition_section(header).is_some()
}

/// One line of `[Prefixes]`: `name = symbol, factor`.
fn read_prefix(line: &str, registry: &mut UnitRegistry, problems: &mut Vec<String>) {
    let Some((name, val_part)) = line.split_once('=') else {
        return;
    };
    let parts: Vec<&str> = val_part.split(',').map(|s| s.trim()).collect();
    if parts.len() < 2 {
        problems.push(format!("{line}: a prefix is `name = symbol, factor`"));
        return;
    }
    let symbol = parts[0];
    if let Some((factor, exact)) = parse_number(parts[1]) {
        registry.add_prefix_exact(symbol.to_string(), factor, exact);
        registry.add_prefix_name(name.trim().to_string(), symbol.to_string());
    } else {
        problems.push(format!("{line}: the factor of a prefix is a number, not '{}'", parts[1]));
    }
}

/// A line of `[Units]` that declares the base unit of a dimension (scale 1 with a single dimension symbol).
fn declare_base_unit(line: &str, registry: &mut UnitRegistry, dim_to_base: &mut HashMap<String, String>) {
    let Some((left, aliases_str)) = split_unit_line(line) else {
        return;
    };
    let parts: Vec<&str> = left.split(',').map(|s| s.trim()).collect();
    if parts.len() < 2 {
        return;
    }
    let factor: f64 = parts[0].parse().unwrap_or(0.0);
    let dim_symbol = parts[1];
    let symbol = aliases_str.first().cloned().unwrap_or_default();
    // Dimension "1" is the dimensionless marker `unity = 1.0, 1, [1]`
    // declares itself against -- `parse_unit_line` below already treats it
    // as "no dimension" (so `%`/`ppm` don't inherit a spurious `unity`
    // dimension). Without this guard here too, this pass ran first and
    // called `add_base_unit` on it anyway, registering a real, poisoned
    // `("unity", (1,1))` dimension in `base_units` that `get_unit` finds
    // before it ever reaches `parse_unit_line`'s correct, empty-dimension
    // registration in `derived_units` -- so `unity` could not convert to
    // or add with `%`, `ppm`, or a bare number, despite all four being
    // dimensionless.
    if factor == 1.0 && !dim_symbol.is_empty() && !symbol.is_empty() && dim_symbol != "1" {
        // Only the first declaration at scale 1 for a dimension symbol
        // is that dimension's base unit, and it wins: `physure.conf`
        // declares both `radian` and `steradian` against `A`, and
        // overwriting made steradian the base of angle (`90 deg => rad`
        // failed as a unit mismatch and `1 deg + 1 sr` was accepted).
        // A later one is the same quantity under another name
        // (`delta_celsius` is a kelvin: as a base unit of its own it
        // could not convert to K) unless the catalog says it is a kind
        // of its own: `steradian` is flagged `own_kind`.
        let first = !dim_to_base.contains_key(dim_symbol);
        let own_kind = parts.contains(&"own_kind");
        if is_single_ident(dim_symbol) && (first || own_kind) {
            dim_to_base.entry(dim_symbol.to_string()).or_insert_with(|| symbol.clone());
            registry.add_base_unit(symbol.clone());
            registry.base_dimension.insert(symbol.clone(), dim_symbol.to_string());
        }
    }
}

/// One line of `[Constants]`: `name = expression, description, latex`. The name it defined, if the line has `=`.
fn read_constant(line: &str, registry: &mut UnitRegistry, constants: &mut HashMap<String, String>) -> Option<String> {
    let (name, val_part) = line.split_once('=')?;
    let name = name.trim();
    let parts: Vec<&str> = val_part.split(',').map(|s| s.trim()).collect();
    let expr = parts[0];
    let desc = parts.get(1).map(|s| s.to_string());
    let latex = parts.get(2).map(|s| s.to_string());
    constants.insert(name.to_string(), expr.to_string());
    registry.constants_meta.insert(name.to_string(), crate::units::registry::ConstantMetadata {
        value: expr.to_string(),
        description: desc,
        latex_symbol: latex,
    });
    Some(name.to_string())
}

/// One line of an atomic-weight section, in g/mol, stored in `table`. `[AtomicWeights]` (your definitions,
/// applied on top of whichever table is selected) and every `[AtomicWeights:<edition>]` (a table: the shipped
/// `standard-2024` and `abridged-2024`, or one of your own) share this grammar. Two forms:
///
/// - `symbol = weight, half-width [, standard]`: by default the second number is the half-width of the interval
///   CIAAW publishes (its "+-"), read as a uniform distribution, whose standard uncertainty is
///   `half-width / sqrt(3)` (IUPAC, "Interpretation and use of standard atomic weights"); that is what the registry
///   stores. A third field `standard` says the number already is a standard uncertainty (it is stored as given);
///   `interval` spells out the default.
/// - `symbol = [a, b]`: an interval with no single value (CIAAW prints these for elements whose composition varies in
///   nature). The weight is the midpoint `(a + b) / 2` and the standard uncertainty is `(b - a) / sqrt(12)`, the
///   same rule with the half-width `(b - a) / 2`. It takes no third field.
///
/// A weight is positive and finite and the number is finite and not negative (`0 < a < b` for an interval);
/// anything else is a problem, because a molar mass built from it would be a confident wrong answer.
fn read_atomic_weight(line: &str, table: &mut HashMap<String, (f64, f64)>, problems: &mut Vec<String>) {
    let parsed = (|| {
        let (symbol, rest) = line.split_once('=')?;
        let symbol = symbol.trim();
        let mut chars = symbol.chars();
        let is_symbol = chars.next().is_some_and(|c| c.is_ascii_uppercase())
            && symbol.len() <= 3
            && chars.all(|c| c.is_ascii_lowercase());
        let (weight, std) = match rest.trim().strip_prefix('[') {
            Some(interval) => read_weight_interval(interval)?,
            None => read_weight_and_number(rest)?,
        };
        is_symbol.then(|| (symbol.to_string(), (weight, std)))
    })();
    match parsed {
        Some((symbol, value)) => {
            table.insert(symbol, value);
        }
        None => problems.push(format!(
            "{line}: expected `symbol = weight, half-width` (or `, standard` after a standard uncertainty) or `symbol = [a, b]`, with a positive weight and a non-negative number"
        )),
    }
}

const EDITION_NAME_RULE: &str =
    "an edition name is lower-case ASCII letters, digits and `-`, `_`, `.`, starts with a letter and has at most 64 characters";

/// One line of `[Chemistry]`: `atomic_weights = <edition or alias>`, the table the chemistry module reads
/// (`standard` and `abridged` are aliases of the newest shipped editions, see
/// [`resolve_atomic_weight_edition`](crate::units::resolve_atomic_weight_edition)). The choice belongs to the
/// registry, like the definitions, and is not a process-wide `[Settings]` entry. It is checked against the tables
/// when it is used, not here, so the choice and the table may come from different texts in any order. Any other
/// key, or an empty or ill-formed value, is a problem.
fn read_chemistry(line: &str, registry: &mut UnitRegistry, problems: &mut Vec<String>) {
    let edition = |value: &str| {
        let value = value.trim();
        if is_atomic_weight_edition_name(value) {
            Ok(value.to_string())
        } else {
            Err(format!("'{value}' is not an edition name: {EDITION_NAME_RULE}"))
        }
    };
    let reading = match line.split_once('=') {
        Some((key, value)) if key.trim() == "atomic_weights" => {
            edition(value).map(|selection| registry.atomic_weight_selection = selection)
        }
        Some((key, value)) if key.trim() == "thermochemistry" => {
            edition(value).map(|selection| registry.thermochemistry_selection = selection)
        }
        _ => Err(
            "the keys of [Chemistry] are `atomic_weights = <edition or alias>` and `thermochemistry = <edition>`".to_string(),
        ),
    };
    if let Err(why) = reading {
        problems.push(format!("{line}: {why}"));
    }
}

/// One line of a thermochemistry section, stored in `table`: `species = enthalpy, entropy`, the standard enthalpy of
/// formation in kJ/mol and the standard molar entropy in J/(mol*K), at 298.15 K and 1 bar, two finite numbers. The
/// species is a formula with its phase (`H2O(l)`, `Ca(OH)2(s)`; the phases are `l`, `g`, `s`, `cr`, `aq`,
/// [`split_species`](crate::units::registry::split_species)). In a table `[Thermochemistry:<edition>]` the phase is
/// required; in your definitions `[Thermochemistry]` it may be left out, and the species is then the unique phase of
/// that formula in the selected edition, placed when the data are read
/// ([`UnitRegistry::thermochemistry_table`]). A species written twice in one section of one text is a problem.
fn read_thermochemistry(
    line: &str,
    section: &str,
    table: &mut BTreeMap<String, (f64, f64)>,
    phase_required: bool,
    seen: &mut HashSet<(String, String)>,
    problems: &mut Vec<String>,
) {
    let parsed = (|| -> Result<(String, (f64, f64)), String> {
        let (key, values) = line.split_once('=').ok_or("expected `species = enthalpy, entropy`")?;
        let key = key.trim();
        let (formula, phase) = crate::units::registry::split_species(key)?;
        if phase_required && phase.is_none() {
            return Err(format!(
                "a species of an edition is written with its phase: {formula}(l), {formula}(g), {formula}(s), {formula}(cr) or {formula}(aq)"
            ));
        }
        let fields: Vec<&str> = values.split(',').map(str::trim).collect();
        let [enthalpy, entropy] = fields.as_slice() else {
            return Err("expected `species = enthalpy, entropy` (kJ/mol, J/(mol*K)): two numbers".to_string());
        };
        let number = |text: &str| match text.parse::<f64>() {
            Ok(x) if x.is_finite() => Ok(x),
            _ => Err(format!("'{text}' is not a finite number")),
        };
        let data = (number(enthalpy)?, number(entropy)?);
        if !seen.insert((section.to_string(), key.to_string())) {
            return Err(format!("'{key}' is defined a second time in [{section}] of this text; the first definition would be lost"));
        }
        Ok((key.to_string(), data))
    })();
    match parsed {
        Ok((key, data)) => {
            table.insert(key, data);
        }
        Err(why) => problems.push(format!("{line}: {why}")),
    }
}

/// `weight, number [, interval|standard]` of an atomic-weight line: the weight and its standard uncertainty.
fn read_weight_and_number(text: &str) -> Option<(f64, f64)> {
    let (weight, rest) = text.split_once(',')?;
    let (number, kind) = rest.split_once(',').map_or((rest, "interval"), |(number, kind)| (number, kind.trim()));
    let weight: f64 = weight.trim().parse().ok()?;
    let number: f64 = number.trim().parse().ok()?;
    let std = match kind {
        "interval" => number / 3f64.sqrt(),
        "standard" => number,
        _ => return None,
    };
    (weight.is_finite() && weight > 0.0 && number.is_finite() && number >= 0.0).then_some((weight, std))
}

/// `a, b]` (what follows the `[`) of an atomic-weight line: the midpoint and `(b - a) / sqrt(12)`.
fn read_weight_interval(text: &str) -> Option<(f64, f64)> {
    let (low, high) = text.strip_suffix(']')?.split_once(',')?;
    let (low, high): (f64, f64) = (low.trim().parse().ok()?, high.trim().parse().ok()?);
    // `low / 2 + high / 2`, not `(low + high) / 2`: the sum of two finite numbers can overflow when their mean does not
    let midpoint = low / 2.0 + high / 2.0;
    let std = (high - low) / 12f64.sqrt();
    // a width that is not zero but whose standard uncertainty underflows to it would be an exact weight
    (low.is_finite() && high.is_finite() && 0.0 < low && low < high && midpoint.is_finite() && std.is_finite() && std > 0.0)
        .then_some((midpoint, std))
}

/// One line of `[Categories]`: `name = [unit, unit, ...]`.
fn read_category(line: &str, registry: &mut UnitRegistry) {
    let Some((cat_name, list_str)) = line.split_once('=') else {
        return;
    };
    let cat_name = cat_name.trim();
    let list_str = list_str.trim().trim_start_matches('[').trim_end_matches(']').trim();
    let items: Vec<String> = list_str
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    registry.categories.insert(cat_name.to_string(), items);
}

/// One line of `[Compatibility]`. `periodic = hertz` names units that the SI reserves for periodic phenomena (and
/// sets `saw_periodic`, even when no name follows); any other line is a row, `radian, plain = convert`, kept for
/// `apply_compatibility`.
fn read_compatibility_line(line: &str, periodic: &mut Vec<String>, saw_periodic: &mut bool, rows: &mut Vec<String>) {
    match line.split_once('=') {
        Some((key, names)) if key.trim() == "periodic" => {
            *saw_periodic = true;
            periodic.extend(names.split(',').map(str::trim).filter(|name| !name.is_empty()).map(str::to_string));
        }
        _ => rows.push(line.to_string()),
    }
}

/// The marker a row of `[Compatibility]` lets a conversion read as 1.
enum Marker {
    Radian,
    Steradian,
}

/// A row is `unit, plain = convert`, with the unit spelled `radian`/`rad` or `steradian`/`sr`, in either order.
/// Anything else is not supported yet: a sum or a comparison never reads a marker as 1 (Python decides those from
/// `Dimension`, which stays strict), and two markers do not convert into each other.
fn read_compatibility_row(row: &str) -> Result<Marker, String> {
    let Some((kinds, operations)) = row.split_once('=') else {
        return Err(format!("[Compatibility] '{row}' is not `kind, kind = operations`"));
    };
    let operations: Vec<&str> = operations.split_whitespace().collect();
    if operations != ["convert"] {
        return Err(format!(
            "[Compatibility] '{row}': the operation must be exactly `convert` (`add` and `compare` are not supported yet)"
        ));
    }
    let kinds: Vec<&str> = kinds.split(',').map(str::trim).collect();
    let marker = match kinds.as_slice() {
        [marker, "plain"] | ["plain", marker] => *marker,
        _ => return Err(format!("[Compatibility] '{row}': a row pairs a marker unit with `plain`")),
    };
    match marker {
        "radian" | "rad" => Ok(Marker::Radian),
        "steradian" | "sr" => Ok(Marker::Steradian),
        other => Err(format!("[Compatibility] '{row}': '{other}' is not a marker unit (radian or steradian)")),
    }
}

/// Replaces the process-wide grants by the rows of a `[Compatibility]` section. A row that cannot be read is
/// reported on stderr and grants nothing.
fn apply_compatibility(rows: &[String]) {
    let (radian, steradian) = compatibility_grants(rows);
    crate::units::angle_mode::set_marker_grants(radian, steradian);
}

/// The grants (radian, steradian) the rows of a `[Compatibility]` section give; a row that cannot be read is
/// reported on stderr and grants nothing.
fn compatibility_grants(rows: &[String]) -> (bool, bool) {
    let (mut radian, mut steradian) = (false, false);
    for row in rows {
        match read_compatibility_row(row) {
            Ok(Marker::Radian) => radian = true,
            Ok(Marker::Steradian) => steradian = true,
            Err(why) => eprintln!("physure.conf: {why}; the row is ignored"),
        }
    }
    (radian, steradian)
}

/// Flags the units that `[Compatibility] periodic` names, once they are registered. A name is any spelling of the
/// unit (`hertz`); the flag goes on the unit the registry keeps, so that its prefixes (built from a copy of it) and
/// its aliases carry it. A name that does not name a unit defined in `[Units]` (a base unit, or a prefixed spelling,
/// cannot be flagged) is a problem.
fn mark_periodic(registry: &mut UnitRegistry, names: &[String], problems: &mut Vec<String>) {
    for name in names {
        let symbol = registry.resolve_symbol(name);
        match registry.derived_units.get_mut(&symbol) {
            Some(unit) => unit.periodic = true,
            None => problems.push(format!(
                "[Compatibility] periodic: '{name}' does not name a unit defined in [Units] (a base unit or a prefixed spelling cannot be flagged)"
            )),
        }
    }
}

/// The SI base units and the standard dimension symbols, for a conf that does not map them itself.
fn register_fallback_bases(registry: &mut UnitRegistry, dim_to_base: &mut HashMap<String, String>) {
    // Ensure SI base units are registered as base_units
    for &base in &["m", "s", "kg", "A", "K", "mol", "cd", "rad"] {
        if !registry.base_units.contains_key(base) {
            registry.add_base_unit(base.to_string());
        }
    }

    // Fallback defaults for standard dimension symbols if not explicitly mapped
    dim_to_base.entry("L".into()).or_insert_with(|| "m".into());
    dim_to_base.entry("M".into()).or_insert_with(|| "kg".into());
    dim_to_base.entry("T".into()).or_insert_with(|| "s".into());
    dim_to_base.entry("I".into()).or_insert_with(|| "A".into());
    dim_to_base.entry("O".into()).or_insert_with(|| "K".into());
    dim_to_base.entry("N".into()).or_insert_with(|| "mol".into());
    dim_to_base.entry("J".into()).or_insert_with(|| "cd".into());
    dim_to_base.entry("A".into()).or_insert_with(|| "rad".into());

    for (dimension, base) in dim_to_base.iter() {
        registry.base_dimension.entry(base.clone()).or_insert_with(|| dimension.clone());
    }

    registry.dimension_base = dim_to_base.clone();
}

/// A positive number of the catalog as the double and, when it is a decimal or a fraction, the
/// exact rational. `None` when it is neither (a dimension symbol, `noprefix`, ...).
fn parse_number(text: &str) -> Option<(f64, Option<crate::units::exact::ExactScale>)> {
    let exact = crate::units::exact::ExactScale::parse(text);
    match text.parse::<f64>() {
        Ok(value) => Some((value, exact)),
        Err(_) => exact.map(|e| (e.to_f64(), Some(e))),
    }
}

/// A unit line cut into its name, its values and the names it lists in brackets: `name = values [alias, alias]`.
fn cut_unit_line(line: &str) -> Option<(&str, &str, Vec<String>)> {
    let (left, aliases_str) = match line.split_once('[') {
        Some((l, r)) => {
            let r = r.trim_end_matches(']').trim();
            (l.trim(), r)
        }
        None => (line, ""),
    };

    let (key, val_part) = left.split_once('=').map(|(k, v)| (k.trim(), v.trim()))?;

    let aliases: Vec<String> = aliases_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Some((key, val_part, aliases))
}

fn split_unit_line(line: &str) -> Option<(&str, Vec<String>)> {
    let (key, val_part, mut aliases) = cut_unit_line(line)?;

    if !aliases.contains(&key.to_string()) {
        aliases.insert(0, key.to_string());
    }

    Some((val_part, aliases))
}

fn is_single_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// The fields of a unit line's values, without the flags (`noprefix`, `own_kind`, `interval`); a single field with spaces in it
/// is read as the fields separated by them.
fn unit_value_fields(val_part: &str) -> Vec<&str> {
    let parts: Vec<&str> = val_part
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !is_flag(s))
        .collect();

    if parts.len() == 1 && parts[0].contains(' ') {
        return parts[0]
            .split_whitespace()
            .filter(|s| !s.is_empty() && !is_flag(s))
            .collect();
    }
    parts
}

/// One line of `[Units]`. `trusted` is the reading of the library's own text, which flags [`SHIPPED_INTERVALS`].
fn parse_unit_line(
    line: &str,
    registry: &mut UnitRegistry,
    dim_to_base: &HashMap<String, String>,
    trusted: bool,
) -> Result<(), String> {
    let Some((key, val_part, mut aliases)) = cut_unit_line(line) else {
        return Err("a unit line is `name = factor, dimension, [names]`".to_string());
    };

    // The names in the order the catalog lists them: the first one is the unit's symbol in Python.
    let mut names = aliases.clone();
    if !names.iter().any(|name| name == key) {
        names.push(key.to_string());
    }

    if !aliases.contains(&key.to_string()) {
        aliases.insert(0, key.to_string());
    }

    let symbol = aliases[0].clone();

    let no_prefix = val_part.split(',').any(|field| field.trim() == "noprefix");
    // `delta_celsius = 1.0, O, interval, [...]`: a temperature interval, which a point does not compare with
    let interval = has_flag(val_part, "interval");

    let parts = unit_value_fields(val_part);

    if parts.is_empty() {
        return Err("the line has no value".to_string());
    }

    // `pH = Log, -1, 1.0, 1, noprefix`: a level is `Log, factor, reference, dimension`. Its dimension
    // is "1" when it is the logarithm of a ratio, or the dimensions of a quantity (`dBW`).
    if parts[0].eq_ignore_ascii_case("log") {
        if interval {
            return Err(INTERVAL_ON_A_LEVEL.to_string());
        }
        register_level(&parts, &symbol, registry, dim_to_base)?;
        register_aliases(registry, key, &symbol, &aliases, &names, no_prefix);
        return Ok(());
    }

    // A number of the catalog is a decimal (`0.0254`, `1e-5`) or a fraction (`5/9`); the exact
    // rational is what conversions use, so that a ratio is rounded once.
    let (factor, exact) = match parse_number(parts[0]) {
        Some(parsed) => parsed,
        None => return Err(format!("'{}' is not a number", parts[0])),
    };

    // Absolute affine units carry a zero point between the factor and the dimension:
    // `celsius = 1.0, 273.15, O`. Only a 3+ field line can be one, so `unity = 1.0, 1` and
    // friends — where field 2 is the dimension "1" — keep reading field 2 as the dimension.
    let (offset, exact_offset, dim_index) = zero_point(&parts);
    let scale = UnitScale { factor, exact, offset, exact_offset };

    let dim_str = if parts.len() > dim_index { parts[dim_index] } else { "1" };

    let base_symbol = dim_to_base.get(dim_str).cloned().unwrap_or_default();
    if interval && offset != 0.0 {
        return Err(INTERVAL_ON_A_POINT.to_string());
    }
    // an old shipped line without the flag keeps the rule; one with a zero point would be a point, not an interval
    let interval = interval || (trusted && offset == 0.0 && SHIPPED_INTERVALS.contains(&key));
    if interval && factor == 1.0 && symbol == base_symbol {
        return Err(INTERVAL_ON_A_BASE.to_string());
    }
    // a line that restates an interval entry (`delta_celsius` copied from an older catalog) without the flag would
    // silently make it a kelvin again: refused, so the definition in force stays
    if !interval {
        if let Some(name) = [key, symbol.as_str()].into_iter().find(|name| {
            registry.derived_units.get(&registry.resolve_symbol(name)).is_some_and(|unit| unit.temperature == crate::units::TemperatureKind::Interval)
        }) {
            return Err(format!(
                "`{name}` is a temperature interval: the line needs the `interval` flag (`{key} = ..., interval, [...]`);                  without it a temperature point would compare with it and convert to it. The definition in force stays"
            ));
        }
    }
    let previous = registry.derived_units.get(&symbol).cloned();

    // Dimension "1" is tested before `base_symbol`, which maps it to the `unity` unit:
    // going through that mapping would make every scaled ratio (`%`, `ppm`) a quantity of
    // dimension "unity", so `2 m * 3 %` printed "m * unity" instead of plain metres.
    if dim_str == "1" || dim_str.is_empty() {
        let mut u = match &scale.exact {
            Some(e) => RationalUnit::new_from_dimensions([]).with_exact_scale(e.clone()),
            None => RationalUnit::new_from_dimensions([]).with_scale(scale.factor),
        };
        u.display_name = Some(symbol.clone());
        registry.add_derived_unit(symbol.clone(), u);
    } else if !base_symbol.is_empty() {
        register_on_base(registry, &symbol, &base_symbol, scale);
    } else {
        let recipe_str = recipe_field(&parts, dim_index + 1);
        register_recipe_unit(registry, &symbol, &names, (dim_str, recipe_str), &scale, dim_to_base)?;
    }
    // what its line says, and nothing a recipe of an interval unit would pass on
    if let Some(unit) = registry.derived_units.get_mut(&symbol) {
        unit.temperature = if interval { crate::units::TemperatureKind::Interval } else { crate::units::TemperatureKind::Unknown };
    }
    // an interval is the difference of two points: its dimension needs a point (a unit with a zero point, like degC)
    if interval && !has_a_point(registry, &symbol) {
        match previous {
            Some(unit) => registry.derived_units.insert(symbol.clone(), unit),
            None => registry.derived_units.remove(&symbol),
        };
        return Err(INTERVAL_WITHOUT_A_POINT.to_string());
    }

    register_aliases(registry, key, &symbol, &aliases, &names, no_prefix);
    Ok(())
}

/// A unit's scale as its line writes it: the factor, and the zero point of an affine unit; each with its exact
/// rational when it has one.
struct UnitScale {
    factor: f64,
    exact: Option<crate::units::exact::ExactScale>,
    offset: f64,
    exact_offset: Option<crate::units::exact::ExactScale>,
}

/// The zero point of an affine unit (`celsius = 1.0, 273.15, O`) and the index of the dimension field after it.
fn zero_point(parts: &[&str]) -> (f64, Option<crate::units::exact::ExactScale>, usize) {
    if parts.len() > 2 {
        if let Some((parsed, exact)) = parse_number(parts[1]) {
            return (parsed, exact, 2);
        }
    }
    (0.0, None, 1)
}

/// The recipe field of a unit line (`N = 1.0, F, kg*m/s^2`): empty when the line has none or it is a flag.
fn recipe_field<'a>(parts: &[&'a str], index: usize) -> &'a str {
    match parts.get(index) {
        Some(field) if !field.is_empty() && !is_flag(field) => field,
        _ => "",
    }
}

/// A level `Log, factor, reference, dimension`: `1` as the dimension is a level of a plain ratio, any other
/// dimension is a level of a quantity (`dBW`).
fn register_level(
    parts: &[&str],
    symbol: &str,
    registry: &mut UnitRegistry,
    dim_to_base: &HashMap<String, String>,
) -> Result<(), String> {
    let number = |index: usize| parts.get(index).and_then(|text| text.parse::<f64>().ok());
    let (Some(factor), Some(reference), Some(dimension)) = (number(1), number(2), parts.get(3).copied()) else {
        return Err("a level is `Log, factor, reference, dimension`".to_string());
    };
    let quantity = if dimension == "1" {
        RationalUnit::dimensionless()
    } else {
        let normalized = normalize_dimension_symbols(dimension, dim_to_base);
        crate::units::parser::Parser::parse_expression_with_registry(&normalized, registry)
            .map_err(|_| format!("cannot read the dimension '{dimension}' of the level {symbol}"))?
    };
    registry.add_level(symbol.to_string(), quantity, factor, reference);
    Ok(())
}

/// A unit on the base unit of its dimension: affine with a zero point, the base unit itself at scale 1, or scaled.
fn register_on_base(registry: &mut UnitRegistry, symbol: &str, base_symbol: &str, scale: UnitScale) {
    let UnitScale { factor, exact, offset, exact_offset } = scale;
    if offset != 0.0 {
        registry.add_affine_unit_exact(symbol.to_string(), base_symbol, factor, exact, offset, exact_offset);
    } else if factor == 1.0 && symbol == base_symbol {
        registry.add_base_unit(symbol.to_string());
    } else {
        registry.add_scaled_unit_exact(symbol.to_string(), base_symbol, factor, exact);
    }
}

/// A unit defined by a recipe or by its dimension spelled out (`dimension`, `recipe`): the unit that text reads
/// as, rescaled by the line's factor.
fn register_recipe_unit(
    registry: &mut UnitRegistry,
    symbol: &str,
    names: &[String],
    (dim_str, recipe_str): (&str, &str),
    scale: &UnitScale,
    dim_to_base: &HashMap<String, String>,
) -> Result<(), String> {
    let parsed_unit_opt = if !recipe_str.is_empty() {
        crate::units::parser::Parser::parse_expression_with_registry(recipe_str, registry).ok()
    } else {
        None
    };

    let parsed_unit_opt = parsed_unit_opt.or_else(|| {
        let normalized_dim = normalize_dimension_symbols(dim_str, dim_to_base);
        crate::units::parser::Parser::parse_expression_with_registry(&normalized_dim, registry).ok()
    });

    let Some(parsed_unit) = parsed_unit_opt else {
        return Err(format!("cannot read the dimension '{dim_str}' of {symbol}, or its recipe"));
    };
    let rescaled = match (&parsed_unit.exact, &scale.exact) {
        (Some(p), Some(e)) => {
            let combined = p.mul(e);
            parsed_unit.with_exact_scale(combined)
        }
        _ => {
            let product = parsed_unit.scale * scale.factor;
            parsed_unit.with_scale(product)
        }
    };
    // Carry the zero point through here too: dropping it would silently turn an
    // affine definition into a purely multiplicative one.
    let mut u = rescaled.with_offset(scale.offset);
    if let Some(e) = &scale.exact_offset {
        u = u.with_exact_offset(e.clone());
    }
    u.display_name = Some(symbol.to_string());
    // A recipe at scale 1 is a name for its base symbols (`N` is `kg*m/s^2`).
    if !recipe_str.is_empty() && scale.factor == 1.0 && scale.offset == 0.0 {
        registry.note_recipe(symbol, &u.dimensions, names);
    }
    registry.add_derived_unit(symbol.to_string(), u);
    Ok(())
}

/// Makes the key of a unit line and each of its aliases another name for `symbol`.
fn register_aliases(
    registry: &mut UnitRegistry,
    key: &str,
    symbol: &str,
    aliases: &[String],
    names: &[String],
    no_prefix: bool,
) {
    for name in names.iter().map(String::as_str).chain([key, symbol]) {
        if no_prefix {
            registry.no_prefix.insert(name.to_string());
        } else {
            registry.no_prefix.remove(name);
        }
    }
    registry.note_aliases(symbol, names);
    registry.register_alias(key.to_string(), symbol.to_string());
    for alias in aliases {
        if alias != symbol {
            registry.register_alias(alias.clone(), symbol.to_string());
        }
    }
}

/// The problems the lenient reading finds in the embedded catalog: none, or the catalog has a line it cannot read.
/// `[Settings]` and `[Compatibility]` are collected and checked, not applied: the tests that call it run in parallel
/// with tests that set a process-wide setting and read it back.
#[cfg(test)]
pub(crate) fn embedded_catalog_problems() -> Vec<String> {
    let mut collected = ProcessSettings::default();
    read_leniently_collected(DEFAULT_PHYSURE_CONF, &mut UnitRegistry::new(), &mut HashMap::new(), &mut collected, true)
}

/// The embedded catalog, parsed once per process.
fn embedded_catalog() -> &'static (UnitRegistry, HashMap<String, String>) {
    static EMBEDDED: OnceLock<(UnitRegistry, HashMap<String, String>)> = OnceLock::new();
    EMBEDDED.get_or_init(|| {
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        load_embedded_catalog(&mut reg, &mut constants);
        (reg, constants)
    })
}

/// The `[Settings]` and `[Compatibility]` of the embedded catalog, applied again: what [`frozen::active::reset_cache`]
/// does after a frozen configuration was in force, so its settings do not stay on the unfrozen catalog. The
/// caller resets the settings to their compiled defaults first.
///
/// [`frozen::active::reset_cache`]: crate::frozen::active::reset_cache
pub(crate) fn apply_embedded_settings() {
    static EMBEDDED_SETTINGS: OnceLock<Option<ProcessSettings>> = OnceLock::new();
    let collected = EMBEDDED_SETTINGS.get_or_init(|| {
        let mut collected = ProcessSettings::default();
        replay_source(&mut UnitRegistry::new(), &mut HashMap::new(), &mut collected, DEFAULT_PHYSURE_CONF, true)
            .ok()
            .map(|()| collected)
    });
    match collected {
        Some(collected) => collected.apply(),
        // the embedded catalog does not replay strictly: read it as the unfrozen catalog is read, applying as it goes
        None => {
            read_leniently(DEFAULT_PHYSURE_CONF, &mut UnitRegistry::new(), &mut HashMap::new(), true);
        }
    }
}

/// The message of the panic that the two infallible functions below keep as a backstop: programs call
/// `frozen::ensure_valid()` at start-up, and the fallible `try_` variants return the error instead.
fn refuse_to_run(e: &crate::frozen::active::ActiveError) -> ! {
    panic!("physure: refusing to run: {e} (see `phs conf status`)")
}

/// The embedded catalog plus `extra`, the text of a system file or of a user conf, read strictly
/// (`extend_registry`) after it, so that a later definition wins. The `physure.conf` of the working
/// directory is not read: the caller decides what a system is made of. A line the parser cannot
/// read is an error that lists every such line, and `[Settings]` is not applied.
///
/// When a frozen configuration is in force (`frozen::active`), the catalog is the frozen one, and a non-empty
/// `extra` is refused: extra definitions belong in the overrides block of the frozen file. When the configuration
/// in force cannot be used, the error says why; there is no fallback to the embedded catalog.
pub fn build_registry_with(extra: &str) -> Result<(UnitRegistry, HashMap<String, String>), Vec<String>> {
    let (mut reg, mut constants) = match crate::frozen::active::active() {
        Err(e) => return Err(vec![format!("physure: refusing to run: {e}")]),
        Ok(Some(_)) if !extra.is_empty() => {
            return Err(vec![
                "a frozen configuration is in force: extra definitions belong in the overrides block of the frozen file (phs conf seal)"
                    .to_string(),
            ])
        }
        Ok(Some(active)) => (active.catalog.registry.clone(), active.catalog.constants.clone()),
        Ok(None) => embedded_catalog().clone(),
    };
    extend_registry(&mut reg, &mut constants, extra)?;
    Ok((reg, constants))
}

/// The catalog that unit text is read against when nothing says otherwise, as a registry of its own, or why the
/// configuration in force cannot be used: the frozen catalog when a frozen configuration is in force (the working
/// directory's `physure.conf` is then not read), otherwise the embedded catalog plus the `physure.conf` of the
/// working directory.
pub fn try_build_registry_from_conf() -> Result<(UnitRegistry, HashMap<String, String>), crate::frozen::active::ActiveError> {
    match crate::frozen::active::active()? {
        Some(active) => Ok((active.catalog.registry.clone(), active.catalog.constants.clone())),
        None => Ok(match local_catalog() {
            Some(local) => local.catalog.as_ref().clone(),
            None => embedded_catalog().clone(),
        }),
    }
}

/// [`try_build_registry_from_conf`] for callers that cannot return an error: it **panics** when the configuration
/// in force cannot be used (a backstop; programs call `frozen::ensure_valid()` first).
pub fn build_registry_from_conf() -> (UnitRegistry, HashMap<String, String>) {
    try_build_registry_from_conf().unwrap_or_else(|e| refuse_to_run(&e))
}

/// The embedded catalog merged with one version of the working directory's `physure.conf`, as cached in the state of
/// `frozen::active` (one cache, forgotten with the others by `reset_cache`).
#[derive(Debug)]
pub(crate) struct LocalCatalog {
    /// The absolute path of the file (the working directory joined with `physure.conf`).
    pub(crate) path: PathBuf,
    /// The length and modification time of the file, from the handle the text was read through.
    pub(crate) stamp: FileStamp,
    /// The text the catalog was built from.
    text: String,
    /// When the text was read (taken before the file was opened).
    read_at: SystemTime,
    pub(crate) catalog: Arc<(UnitRegistry, HashMap<String, String>)>,
}

/// How recent a modification time must be for a stamp not to be trusted alone. A file system records modification
/// times with a granularity (two seconds on FAT, a clock tick on others): an edit of the same length made within it of
/// the previous write can keep the same stamp. Two seconds covers the coarsest one.
const RACY_WINDOW: Duration = Duration::from_secs(2);

impl LocalCatalog {
    /// Whether the stamp alone proves the text is still the one read: it was read at least [`RACY_WINDOW`] after
    /// the modification time it has, so any later write gets another one (git's "racy clean" rule). A file whose
    /// platform reports no modification time never qualifies.
    fn stamp_suffices(&self) -> bool {
        self.stamp.modified.is_some_and(|modified| self.read_at.duration_since(modified).is_ok_and(|age| age >= RACY_WINDOW))
    }
}

/// The working directory's `physure.conf` merged with the embedded catalog, or `None` when there is no such file
/// (or it cannot be read as text: then the embedded catalog alone, as always).
///
/// Cached by (absolute path, length, modification time): a call costs one `stat` of the file when its stamp is the
/// one cached and the text was read well after the file was last modified. While the modification time is recent
/// ([`RACY_WINDOW`] before the reading), the text is read again on each call and compared, so a quick edit of the
/// same length is not missed. A changed file is read and merged again, which also applies its `[Settings]` and
/// `[Compatibility]` again (the lenient reading applies them as it goes) and reports each of its problems once per
/// process. A file edited with its length and an old modification time restored by hand (`touch -d`) is not seen
/// until either changes, the current directory changes or `frozen::active::reset_cache()` is called.
fn local_catalog() -> Option<Arc<LocalCatalog>> {
    let path = std::env::current_dir().ok()?.join("physure.conf");
    let metadata = fs::metadata(&path).ok().filter(fs::Metadata::is_file)?;
    let stamp = FileStamp::of(&metadata);
    if let Some(cached) = crate::frozen::active::cached_local_catalog() {
        if cached.path == path && cached.stamp == stamp {
            if cached.stamp_suffices() {
                return Some(cached);
            }
            let read_at = SystemTime::now();
            if let Some((text, stamp)) = read_local_conf(&path) {
                if text == cached.text && stamp == cached.stamp {
                    let confirmed =
                        Arc::new(LocalCatalog { path, stamp, text, read_at, catalog: Arc::clone(&cached.catalog) });
                    crate::frozen::active::store_local_catalog(Arc::clone(&confirmed));
                    return Some(confirmed);
                }
            }
        }
    }
    build_local_catalog(path)
}

/// The text of the file and the stamp of the handle it was read through; `None` if it cannot be read as text.
fn read_local_conf(path: &Path) -> Option<(String, FileStamp)> {
    let mut file = fs::File::open(path).ok()?;
    let metadata = file.metadata().ok().filter(fs::Metadata::is_file)?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    Some((text, FileStamp::of(&metadata)))
}

/// Reads the file, merges it with the embedded catalog and caches the result.
fn build_local_catalog(path: PathBuf) -> Option<Arc<LocalCatalog>> {
    let read_at = SystemTime::now();
    let (text, stamp) = read_local_conf(&path)?;
    let (mut reg, mut constants) = embedded_catalog().clone();
    LOCAL_BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    parse_physure_conf(&text, &mut reg, &mut constants);
    let local = Arc::new(LocalCatalog { path, stamp, text, read_at, catalog: Arc::new((reg, constants)) });
    crate::frozen::active::store_local_catalog(Arc::clone(&local));
    Some(local)
}

static LOCAL_BUILDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many times this process built the catalog merged with the working directory's `physure.conf` (for the tests
/// of its cache).
#[doc(hidden)]
pub fn local_conf_builds() -> u64 {
    LOCAL_BUILDS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Runs `f` against the catalog that unit text is read against when nothing says otherwise, or says why the
/// configuration in force cannot be used. With a frozen configuration in force, `f` gets its catalog by
/// reference; otherwise the embedded catalog plus the `physure.conf` of the working directory, if there is one.
///
/// The working-directory conf is looked for on every call (one `stat`), so a file that appears, changes or goes
/// away is seen. Without one, `f` gets the embedded catalog itself, by reference: no copy is made, which is what
/// turning a unit string into a unit costs most when the catalog is copied first. With one, `f` gets the merged
/// registry by reference, built once per version of the file and cached (see `local_catalog`). `f` runs after every
/// cache lock is released, so it may read the catalog again.
pub fn try_with_conf_registry<R>(f: impl FnOnce(&UnitRegistry) -> R) -> Result<R, crate::frozen::active::ActiveError> {
    match crate::frozen::active::active()? {
        Some(active) => Ok(f(&active.catalog.registry)),
        None => match local_catalog() {
            Some(local) => Ok(f(&local.catalog.0)),
            None => Ok(f(&embedded_catalog().0)),
        },
    }
}

/// [`try_with_conf_registry`] for callers that cannot return an error: it **panics** when the configuration in
/// force cannot be used (a backstop; programs call `frozen::ensure_valid()` first).
pub fn with_conf_registry<R>(f: impl FnOnce(&UnitRegistry) -> R) -> R {
    try_with_conf_registry(f).unwrap_or_else(|e| refuse_to_run(&e))
}

fn normalize_dimension_symbols(expr: &str, dim_to_base: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut chars = expr.chars().peekable();
    while let Some(c) = chars.next() {
        let is_standalone = match chars.peek() {
            Some(&next) => !next.is_alphanumeric() && next != '_',
            None => true,
        };
        if is_standalone {
            let key = c.to_string();
            if let Some(base_unit) = dim_to_base.get(&key) {
                out.push_str(base_unit);
            } else {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::registry::UnitRegistry;
    use std::collections::HashMap;

    /// Builds the cached catalog now: its first build applies the catalog's `[Settings]` to the
    /// process-wide globals, and a test that sets one and reads it back must not meet that.
    fn settle_catalog_settings() {
        let _ = build_registry_from_conf();
    }

    #[test]
    fn every_core_setting_key_is_read_and_named_by_the_unknown_key_message() {
        let message = parse_setting("no_such_key = 1").unwrap_err();
        for key in CORE_SETTING_KEYS {
            // a bad value is a `Bad` issue, never `Unknown`: the key has an arm in `read_setting`
            assert!(
                !matches!(read_setting(&format!("{key} = ?")), Err(SettingIssue::Unknown(_))),
                "{key} is listed in CORE_SETTING_KEYS but read_setting does not know it"
            );
            assert!(message.contains(key), "{key} missing from: {message}");
            assert!(!CORE_UNUSED_SETTINGS.contains(key), "{key} is both read and ignored");
        }
        let mut unique = CORE_SETTING_KEYS.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), CORE_SETTING_KEYS.len(), "a key is listed twice");
    }

    #[test]
    fn parses_parallel_threshold_from_settings_section() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        let original = crate::settings::parallel_threshold();
        parse_physure_conf(
            "[Settings]\nparallel_threshold = 500\n",
            &mut reg,
            &mut constants,
        );
        assert_eq!(crate::settings::parallel_threshold(), 500);
        crate::settings::set_parallel_threshold(original);
    }

    #[test]
    fn parses_max_loop_iterations_from_settings_section() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        let original = crate::settings::max_loop_iterations();
        parse_physure_conf(
            "[Settings]\nmax_loop_iterations = 250\n",
            &mut reg,
            &mut constants,
        );
        assert_eq!(crate::settings::max_loop_iterations(), 250);
        crate::settings::set_max_loop_iterations(original);
    }

    #[test]
    fn the_embedded_conf_sets_the_default_angle_mode() {
        use crate::units::angle_mode::{angle_mode, set_angle_mode, AngleMode};
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        // Start from the mode that is NOT the default, so only the conf line can bring `Dimensionless` back.
        set_angle_mode(AngleMode::Dimension);
        load_embedded_catalog(&mut reg, &mut constants);
        assert_eq!(angle_mode(), AngleMode::Dimensionless);
    }

    #[test]
    fn parses_angle_mode_from_settings_section() {
        use crate::units::angle_mode::{angle_mode, set_angle_mode, AngleMode};
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        set_angle_mode(AngleMode::Dimensionless);
        parse_physure_conf("[Settings]\nangle_mode = dimension\n", &mut reg, &mut constants);
        assert_eq!(angle_mode(), AngleMode::Dimension);
    }

    #[test]
    fn a_bad_angle_mode_keeps_the_one_in_force() {
        use crate::units::angle_mode::{angle_mode, set_angle_mode, AngleMode};
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        // Not the default, so a handler that falls back to the default on a bad value is caught.
        set_angle_mode(AngleMode::Dimension);
        parse_physure_conf("[Settings]\nangle_mode = degrees\n", &mut reg, &mut constants);
        assert_eq!(angle_mode(), AngleMode::Dimension);
    }

    #[test]
    fn parses_max_pipeline_steps_from_settings_section() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        let original = crate::settings::max_pipeline_steps();
        parse_physure_conf(
            "[Settings]\nmax_pipeline_steps = 42\n",
            &mut reg,
            &mut constants,
        );
        assert_eq!(crate::settings::max_pipeline_steps(), 42);
        crate::settings::set_max_pipeline_steps(original);
    }

    #[test]
    fn the_embedded_catalog_has_no_line_the_lenient_reading_would_drop() {
        let problems = embedded_catalog_problems();
        assert!(problems.is_empty(), "{problems:?}");
    }

    /// Listing the embedded catalog's problems changes no process-wide setting. It used to read the catalog the way
    /// the runtime does, applying its `[Settings]` as it went, without `TEST_LOCK`: run in parallel with
    /// `parses_max_pipeline_steps_from_settings_section` it could put `max_pipeline_steps` back to the catalog's
    /// 1000 between that test's write of 42 and its read (an intermittent failure seen on this branch).
    #[test]
    fn listing_the_embedded_catalog_problems_changes_no_setting() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let original = crate::settings::set_max_pipeline_steps(42);
        let _ = embedded_catalog_problems();
        let after = crate::settings::set_max_pipeline_steps(original);
        assert_eq!(after, 42);
    }

    /// The problems of the lenient reading of `text` over the embedded catalog.
    fn lenient_problems(text: &str) -> (UnitRegistry, Vec<String>) {
        let (mut registry, mut constants) = build_registry_with("").unwrap();
        let problems = read_leniently(text, &mut registry, &mut constants, false);
        (registry, problems)
    }

    #[test]
    fn the_lenient_reading_finds_the_problems_of_a_bad_atomic_weight_or_chemistry_line_and_applies_the_rest() {
        let (registry, problems) = lenient_problems(
            "[Chemistry]\natomic_weights = Bad Name\n[AtomicWeights]\nFe = abc, 0.1\nCo = 59.0, 0.2\n[chemistry]\nx = 1\n",
        );
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("Bad Name")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("Fe = abc, 0.1")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("[Chemistry]")), "{problems:?}");
        // the bad lines changed nothing, the good one was applied, as it always was
        assert_eq!(registry.atomic_weight_selection, "standard");
        assert_eq!(registry.atomic_weights.get("Fe"), None);
        assert_eq!(registry.atomic_weights.get("Co"), Some(&(59.0, 0.2 / 3f64.sqrt())));
    }

    #[test]
    fn the_lenient_reading_ignores_a_section_that_edits_a_shipped_edition() {
        let (registry, problems) = lenient_problems("[AtomicWeights:standard-2024]\nH = 1.0, 0.1\n");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(registry.atomic_weight_tables["standard-2024"]["H"].0, (1.00784 + 1.00811) / 2.0);
    }

    #[test]
    fn a_problem_is_reported_once_for_each_set_of_seen_messages() {
        let mut seen = HashSet::new();
        let problems = vec!["Fe = abc, 0.1: bad".to_string(), "[chemistry]: bad".to_string(), "Fe = abc, 0.1: bad".to_string()];
        // the same message twice in one reading is one report
        let first = new_lenient_reports(&problems, &mut seen);
        assert_eq!(
            first,
            vec![
                "physure.conf: Fe = abc, 0.1: bad; the line is ignored".to_string(),
                "physure.conf: [chemistry]: bad; the line is ignored".to_string(),
            ]
        );
        // the file is read again on the next call: nothing new, nothing printed
        assert!(new_lenient_reports(&problems, &mut seen).is_empty());
        // a message that was not seen before is reported, once
        let more = vec!["Fe = abc, 0.1: bad".to_string(), "Co = x: worse".to_string()];
        assert_eq!(new_lenient_reports(&more, &mut seen), vec!["physure.conf: Co = x: worse; the line is ignored".to_string()]);
        assert!(new_lenient_reports(&[], &mut seen).is_empty());
    }

    #[test]
    fn a_conf_can_name_a_unit_of_its_own_as_periodic() {
        let (mut reg, mut constants) = build_registry_from_conf();
        extend_registry(
            &mut reg,
            &mut constants,
            "[Units]\nmyfreq = 1.0, T^-1, 1/s, [mf]\n[Compatibility]\nperiodic = myfreq\n",
        )
        .unwrap();
        assert!(reg.get_unit("mf").unwrap().periodic, "an alias carries the flag");
        assert!(reg.get_unit("kmf").unwrap().periodic, "a prefixed form carries the flag");
        assert!(!reg.get_unit("m").unwrap().periodic, "a unit the section does not name does not");
    }

    #[test]
    fn a_periodic_name_that_is_no_unit_is_a_problem_and_the_registry_stays_as_it_was() {
        let (mut reg, mut constants) = build_registry_from_conf();
        let (before, constants_before) = (reg.clone(), constants.clone());
        let problems = extend_registry(
            &mut reg,
            &mut constants,
            "[Units]\nmyfreq = 1.0, T^-1, 1/s, [mf]\n[Compatibility]\nperiodic = myfreq, nosuchunit\n",
        )
        .unwrap_err();
        assert!(problems.iter().any(|problem| problem.contains("nosuchunit")), "{problems:?}");
        assert_eq!(reg, before, "the unit that was fine is rolled back with the one that was not");
        assert_eq!(constants, constants_before);
        assert!(reg.get_unit("myfreq").is_none());
    }

    #[test]
    fn a_strict_reading_ignores_the_lines_of_compatibility_that_are_not_periodic() {
        let (mut reg, mut constants) = build_registry_from_conf();
        let before = reg.clone();
        // No `=`, a row, and a `periodic` that names nothing: none of them is a problem.
        extend_registry(
            &mut reg,
            &mut constants,
            "[Compatibility]\nthis line has no equals sign\nradian, plain = convert\nperiodic =\n",
        )
        .unwrap();
        assert_eq!(reg, before);
    }

    fn marker_grants() -> (bool, bool) {
        use crate::units::angle_mode::marker_converts_to_plain;
        (marker_converts_to_plain("rad"), marker_converts_to_plain("sr"))
    }

    /// The grants after reading `conf` with the grants first set to `start`. The caller holds `TEST_LOCK` and a
    /// `RestoreProcessState` guard.
    fn grants_after(conf: &str, start: (bool, bool)) -> (bool, bool) {
        crate::units::angle_mode::set_marker_grants(start.0, start.1);
        parse_physure_conf(conf, &mut UnitRegistry::new(), &mut HashMap::new());
        marker_grants()
    }

    #[test]
    fn the_embedded_conf_lets_a_conversion_read_both_markers_as_one() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        // Start from "none", so only the rows of the embedded conf can bring both back.
        crate::units::angle_mode::set_marker_grants(false, false);
        load_embedded_catalog(&mut UnitRegistry::new(), &mut HashMap::new());
        assert_eq!(marker_grants(), (true, true));
    }

    #[test]
    fn a_compatibility_section_replaces_the_rows() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        assert_eq!(grants_after("[Compatibility]\nradian, plain = convert\n", (true, true)), (true, false));
        assert_eq!(grants_after("[Compatibility]\nplain, steradian = convert\n", (true, true)), (false, true));
    }

    #[test]
    fn an_empty_compatibility_section_grants_nothing() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        assert_eq!(grants_after("[Compatibility]\n", (true, true)), (false, false));
    }

    #[test]
    fn a_conf_without_the_section_keeps_the_rows() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        // Not the default, so a conf that resets the rows to the default is caught.
        assert_eq!(grants_after("[Settings]\nparallel_threshold = 10000\n", (true, false)), (true, false));
    }

    #[test]
    fn a_section_that_only_names_a_periodic_unit_keeps_the_rows() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        assert_eq!(grants_after("[Compatibility]\nperiodic = hertz\n", (true, false)), (true, false));
        // A `periodic` line that names nothing is still a `periodic` line, not an empty section.
        assert_eq!(grants_after("[Compatibility]\nperiodic =\n", (true, false)), (true, false));
    }

    #[test]
    fn a_section_with_rows_and_a_periodic_unit_replaces_the_rows() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        // The shape of the embedded conf: rows and `periodic` in one section. The periodic name must not keep the
        // old rows alive.
        let conf = "[Compatibility]\nradian, plain = convert\nperiodic = hertz\n";
        assert_eq!(grants_after(conf, (true, true)), (true, false));
    }

    #[test]
    fn a_row_that_cannot_be_read_grants_nothing() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        for row in [
            // A sum or a comparison never reads a marker as 1, and the operation must be exactly `convert`.
            "radian, plain = convert add",
            "radian, plain = compare",
            "radian, plain = add compare convert",
            "radian, plain =",
            "radian, plain = convert convert",
            // Two markers do not convert into each other, and a plain unit is not a marker.
            "radian, steradian = convert",
            "metre, plain = convert",
            "plain, plain = convert",
            "radian = convert",
            // No `=` at all.
            "radian plain convert",
        ] {
            let conf = format!("[Compatibility]\n{row}\n");
            assert_eq!(grants_after(&conf, (true, true)), (false, false), "{row}");
        }
    }

    #[test]
    fn a_row_that_cannot_be_read_does_not_cancel_the_one_beside_it() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let conf = "[Compatibility]\nradian, plain = convert\nradian, plain = add\n";
        assert_eq!(grants_after(conf, (false, false)), (true, false));
    }

    #[test]
    fn a_system_conf_does_not_change_the_grants_but_can_name_a_periodic_unit() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let (mut reg, mut constants) = build_registry_from_conf();
        // Not the default, and not what the rows below would give: applying them would make this (true, true).
        crate::units::angle_mode::set_marker_grants(true, false);
        extend_registry(
            &mut reg,
            &mut constants,
            "[Units]\nmyfreq = 1.0, T^-1, 1/s, [mf]\n[Compatibility]\nperiodic = myfreq\nradian, plain = convert\nsteradian, plain = convert\n",
        )
        .unwrap();
        assert!(reg.get_unit("mf").unwrap().periodic);
        assert_eq!(marker_grants(), (true, false));
        extend_registry(&mut reg, &mut constants, "[Compatibility]\n").unwrap();
        assert_eq!(marker_grants(), (true, false));
    }

    fn row_error(row: &str) -> String {
        match read_compatibility_row(row) {
            Ok(_) => panic!("'{row}' should not be a readable row"),
            Err(why) => why,
        }
    }

    #[test]
    fn a_row_that_cannot_be_read_says_why() {
        assert!(row_error("radian plain convert").contains("kind, kind = operations"));
        assert!(row_error("radian, plain = add").contains("not supported yet"));
        assert!(row_error("radian, steradian = convert").contains("pairs a marker unit with `plain`"));
        assert!(row_error("metre, plain = convert").contains("not a marker unit"));
    }

    // ---- Phase 1c, Task 2 (amended by Task 9, E15 and E16): strict replay of a frozen source ----

    /// Replays `text` into empty registries; the result and what was collected.
    fn replay(text: &str, trusted: bool) -> (Result<(), Vec<String>>, UnitRegistry, HashMap<String, String>, ProcessSettings) {
        let (mut registry, mut constants, mut collected) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
        let result = replay_source(&mut registry, &mut constants, &mut collected, text, trusted);
        (result, registry, constants, collected)
    }

    /// The readable process-wide settings, to show that a replay changed none of them.
    fn process_settings_now() -> (usize, usize, usize, u64, usize, usize, ShapeKind, u64, (MonteCarloSeed, DisplayRules), PropagationMode, AngleMode, (bool, bool)) {
        (
            crate::settings::parallel_threshold(),
            crate::settings::max_loop_iterations(),
            crate::settings::max_pipeline_steps(),
            crate::settings::approx_rel_tol().to_bits(),
            crate::settings::monte_carlo_samples(),
            crate::settings::max_while_iterations(),
            crate::uncertainty::shapes::asymmetric_shape(),
            crate::uncertainty::dof::default_dof().to_bits(),
            (crate::uncertainty::monte_carlo_seed(), crate::settings::display_rules()),
            crate::uncertainty::mode::propagation_mode(),
            crate::units::angle_mode::angle_mode(),
            marker_grants(),
        )
    }

    #[test]
    fn a_clean_text_is_replayed_and_its_settings_are_collected_not_applied() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let before = process_settings_now();
        let (result, registry, constants, collected) = replay(
            "[Settings]\nparallel_threshold = 12345\nangle_mode = dimension\nverbose = true\n[Units]\nzz_len = 1.0, L, [zzl]\n[Constants]\nzz_const = 2 zzl\n[Compatibility]\nradian, plain = convert\n",
            false,
        );
        result.unwrap();
        assert_eq!(constants["zz_const"], "2 zzl");
        assert_eq!(registry.constants_meta["zz_const"].value, "2 zzl");
        assert!(registry.knows("zzl"));
        assert_eq!(
            collected.settings,
            vec![
                Setting::ParallelThreshold(12345),
                Setting::AngleMode(AngleMode::Dimension),
                Setting::Ignored { key: "verbose".into(), value: "true".into() },
            ]
        );
        assert_eq!(collected.compatibility, Some(vec!["radian, plain = convert".to_string()]));
        assert_eq!(process_settings_now(), before, "a replay never changes process-wide state");
    }

    #[test]
    fn a_text_with_a_problem_changes_nothing_not_even_what_was_collected() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let before = process_settings_now();
        let (mut registry, mut constants, mut collected) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
        replay_source(&mut registry, &mut constants, &mut collected, "[Settings]\nmax_pipeline_steps = 7\n[Constants]\na = 1 m\n", true).unwrap();
        let snapshot = (registry.clone(), constants.clone(), collected.clone());
        let problems = replay_source(
            &mut registry,
            &mut constants,
            &mut collected,
            "[Settings]\nparallel_threshold = 54321\n[Constants]\nb = 2 m\n[AtomicWeights]\nFe = abc, 0.1\n",
            false,
        )
        .unwrap_err();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("Fe = abc, 0.1"), "{problems:?}");
        assert_eq!((registry, constants, collected), snapshot);
        assert_eq!(process_settings_now(), before);
    }

    #[test]
    fn trusted_replay_may_define_a_shipped_edition_and_untrusted_may_not() {
        let text = "[AtomicWeights:standard-2024]\nH = 1.0, 0\n";
        let (result, registry, _, _) = replay(text, true);
        result.unwrap();
        assert_eq!(registry.atomic_weight_tables["standard-2024"]["H"], (1.0, 0.0));
        let (mut registry, mut constants, mut collected) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
        replay_source(&mut registry, &mut constants, &mut collected, text, true).unwrap();
        let problems = replay_source(&mut registry, &mut constants, &mut collected, text, false).unwrap_err();
        assert!(problems.iter().any(|p| p.contains("frozen")), "{problems:?}");
    }

    #[test]
    fn the_embedded_catalog_replays_strictly() {
        let (result, registry, constants, collected) = replay(DEFAULT_PHYSURE_CONF, true);
        result.unwrap();
        assert!(registry.knows("m") && registry.knows("Hz"));
        assert!(constants.contains_key("molar_gas_constant"));
        assert!(collected.settings.contains(&Setting::PropagationMode(PropagationMode::Correlated)));
        assert!(collected.settings.contains(&Setting::ApproxRelTol(crate::settings::DEFAULT_APPROX_REL_TOL)));
        assert!(collected.settings.contains(&Setting::MonteCarloSamples(crate::settings::DEFAULT_MONTE_CARLO_SAMPLES)));
        assert!(collected.settings.contains(&Setting::MaxWhileIterations(crate::settings::DEFAULT_MAX_WHILE_ITERATIONS)));
        assert!(collected.settings.contains(&Setting::AsymmetricShape(ShapeKind::DEFAULT)));
        assert!(collected.settings.contains(&Setting::DefaultDof(f64::INFINITY)));
        assert!(collected.settings.contains(&Setting::MonteCarloSeed(MonteCarloSeed::DEFAULT)));
        assert_eq!(collected.effective().display, DisplayRules::DEFAULT, "the shipped catalog states the compiled display rules");
        for key in ["display_significant_digits", "display_scientific_below", "display_scientific_from", "uncertainty_digits"] {
            assert!(DEFAULT_PHYSURE_CONF.lines().any(|line| line.starts_with(&format!("{key} = "))), "{key} is in the shipped catalog");
        }
        assert!(collected.compatibility.is_some());
        // the strict replay builds what the lenient reading of the library builds
        let mut lenient = UnitRegistry::new();
        let mut lenient_constants = HashMap::new();
        let problems = {
            let mut problems = Vec::new();
            let reading = Reading { settings: SettingsMode::Skip, trusted: true, strict_sections: false };
            parse_conf(DEFAULT_PHYSURE_CONF, &mut lenient, &mut lenient_constants, reading, &mut ProcessSettings::default(), &mut problems);
            problems
        };
        assert!(problems.is_empty(), "{problems:?}");
        assert!(registry == lenient, "the strict replay and the lenient reading build different registries");
        assert_eq!(constants, lenient_constants);
    }

    /// The same text, reformatted every way the canonical form ignores (spec §4.1).
    fn reformatted(text: &str) -> String {
        let mut out = String::from("\u{20}\r\n# a comment that is not in the original\r\n; another\r\n\r\n");
        for (i, line) in text.lines().enumerate() {
            out.push_str(&format!("  \t{line}\t  \r\n"));
            if i % 7 == 0 {
                out.push_str("\r\n   # inserted\r\n\t; inserted\r\n");
            }
        }
        out
    }

    #[test]
    fn two_texts_with_the_same_canonical_form_build_the_same_registry() {
        // closes residual risk 1 of spec §13: the core's reader and the canonical form agree
        let original = DEFAULT_PHYSURE_CONF;
        let other = reformatted(original);
        assert_ne!(original, other);
        assert_eq!(crate::frozen::canonical::canonical_text(original), crate::frozen::canonical::canonical_text(&other));
        let (a, registry_a, constants_a, collected_a) = replay(original, true);
        let (b, registry_b, constants_b, collected_b) = replay(&other, true);
        a.unwrap();
        b.unwrap();
        assert!(registry_a == registry_b, "the same canonical text built two registries");
        assert_eq!(constants_a, constants_b);
        assert_eq!(collected_a, collected_b);
        // and an override text
        let overrides = "[Settings]\nparallel_threshold = 77\n[Units]\nzz = 3.0, L, [zzu]\n[Constants]\nmolar_gas_constant = 8.0 J*mol^-1*K^-1\n[Chemistry]\natomic_weights = abridged\n";
        let (mut ra, mut ca, mut sa) = (registry_a.clone(), constants_a.clone(), collected_a.clone());
        let (mut rb, mut cb, mut sb) = (registry_a, constants_a, collected_a);
        replay_source(&mut ra, &mut ca, &mut sa, overrides, false).unwrap();
        replay_source(&mut rb, &mut cb, &mut sb, &reformatted(overrides), false).unwrap();
        assert!(ra == rb);
        assert_eq!((ca, sa), (cb, sb));
    }

    fn frozen_problems(text: &str) -> Vec<String> {
        // over the embedded catalog, as an overrides block is
        let (mut registry, mut constants, mut collected) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
        replay_source(&mut registry, &mut constants, &mut collected, DEFAULT_PHYSURE_CONF, true).unwrap();
        match replay_source(&mut registry, &mut constants, &mut collected, text, false) {
            Ok(()) => Vec::new(),
            Err(problems) => problems,
        }
    }

    #[test]
    fn a_frozen_replay_refuses_what_the_lenient_reader_would_ignore() {
        for (text, needle) in [
            ("[Settings]\npropagation_mode = corelated\n", "corelated"),
            ("[Settings]\nangle_mod = dimension\n", "angle_mod"),
            ("[Settings]\nangle_mode = degrees\n", "degrees"),
            ("[Settings]\nparallel_threshold = -5\n", "-5"),
            ("[Settings]\nmax_loop_iterations = many\n", "many"),
            ("[Settings]\nmax_pipeline_steps =\n", "max_pipeline_steps"),
            ("[Settings]\napprox_rel_tol = 0\n", "approx_rel_tol"),
            ("[Settings]\napprox_rel_tol = -1e-3\n", "-1e-3"),
            ("[Settings]\napprox_rel_tol = nan\n", "nan"),
            ("[Settings]\napprox_rel_tol = abc\n", "abc"),
            ("[Settings]\napprox_rel_tol = 1\n", "approx_rel_tol"),
            ("[Settings]\nmonte_carlo_samples = 0\n", "monte_carlo_samples"),
            ("[Settings]\nmonte_carlo_samples = 1\n", "monte_carlo_samples"),
            ("[Settings]\nmonte_carlo_samples = abc\n", "abc"),
            ("[Settings]\nmonte_carlo_samples = 20000000\n", "20000000"),
            ("[Settings]\nmax_while_iterations = 0\n", "max_while_iterations"),
            ("[Settings]\nmax_while_iterations = abc\n", "abc"),
            ("[Settings]\nasymmetric_shape = foo\n", "foo"),
            ("[Settings]\ndefault_dof = -1\n", "-1"),
            ("[Settings]\ndefault_dof = 0\n", "default_dof"),
            ("[Settings]\ndefault_dof = abc\n", "abc"),
            ("[Settings]\ndisplay_significant_digits = 0\n", "display_significant_digits"),
            ("[Settings]\ndisplay_significant_digits = 18\n", "18"),
            ("[Settings]\ndisplay_scientific_below = 1e-7\n", "1e-7"),
            ("[Settings]\ndisplay_scientific_below = 2\n", "display_scientific_below"),
            ("[Settings]\ndisplay_scientific_from = inf\n", "inf"),
            ("[Settings]\ndisplay_scientific_from = 0.5\n", "display_scientific_from"),
            ("[Settings]\nuncertainty_digits = GUM\n", "GUM"),
            ("[Settings]\nuncertainty_digits = 0\n", "uncertainty_digits"),
            ("[Settings]\nmonte_carlo_seed = -3\n", "-3"),
            ("[Settings]\nmonte_carlo_seed = Random\n", "Random"),
            ("[Settings]\nno equals sign here\n", "no equals sign here"),
            ("[Compatibility]\nradian, plain = add\n", "radian, plain = add"),
            ("[Compatibility]\nthis line has no equals sign\n", "this line has no equals sign"),
            ("[Constant]\nmolar_gas_constant = 8.0 J/mol/K\n", "[Constant]"),
            ("[Unitz]\nx = 1.0, L\n", "[Unitz]"),
            ("[]\nx = 1\n", "[]"),
            ("[Constants]\nzz = this is not a quantity at all\n", "zz = this is not a quantity at all"),
            ("[Constants]\nzz = 1 nosuchunit\n", "nosuchunit"),
            ("[Constants]\nzz = inf m\n", "zz = inf m"),
            ("[Constants]\nzz\n", "zz"),
            ("[Constants]\n= 1 m\n", "= 1 m"),
            ("[Prefixes]\nnoequals\n", "noequals"),
            ("[Categories]\nwhatever\n", "whatever"),
            ("[Dimensions]\nwhatever\n", "whatever"),
            ("zz_unit = 1.0, L\n[Units]\n", "zz_unit = 1.0, L"),
            ("\u{FEFF}[Settings]\nparallel_threshold = 5\n", "[Settings]"),
            // fields the reader never looks at: a misspelt flag, a stray word, one field too many
            ("[Units]\nzzq = 1.0, L, noprefx, [zzqq]\n", "noprefx"),
            ("[Units]\nzzq = 2.0, L, garbage more\n", "garbage"),
            ("[Units]\nzzdb = Log, 10, 1.0, 1, extra, noprefix, [zzdb]\n", "extra"),
            ("[Prefixes]\nzzp = Q, 1000, extra\n", "extra"),
            ("[Constants]\nzc = 1 m, a description, \\zeta, extra\n", "extra"),
            // a name defined twice in one text: the first definition would be dropped
            ("[Units]\nzzq = 2.0, L\nzzq = 3.0, L\n", "zzq"),
            ("[Constants]\nzc = 1 m\n[Constants]\nzc = 2 m\n", "zc"),
            ("[Prefixes]\nzzp = Q, 1000\nzzp = R, 10\n", "zzp"),
            // a constant carries no uncertainty: one written with it is refused, never read without it
            ("[Constants]\nzc = 1 +/- 0.1 m\n", "uncertainty"),
            ("[Constants]\nzc = 1 ± 0.1 m\n", "uncertainty"),
            // a name that is a unit and a constant must carry one value: overriding either side alone is refused
            ("[Constants]\nbohr_radius = 6e-11 m\n", "bohr_radius"),
            ("[Constants]\nelectron_volt = 1.6e-19 J\n", "electron_volt"),
            ("[Constants]\nplanck_mass = 2.176434e-08 m\n", "different dimensions"),
            ("[Units]\nbohr = 6e-11, L, m, [a0, bohr, bohr_radius]\n", "bohr_radius"),
        ] {
            let problems = frozen_problems(text);
            assert!(!problems.is_empty(), "accepted: {text:?}");
            assert!(problems.iter().any(|p| p.contains(needle)), "{text:?}: {needle:?} not in {problems:?}");
            // the same text read for a unit system (not frozen) is not the place of these rules
            let _ = extend_registry(&mut UnitRegistry::new(), &mut HashMap::new(), text);
        }
    }

    #[test]
    fn the_ignored_settings_of_a_text_are_those_its_settings_sections_set() {
        let text = "verbose = 1\n[Settings]\n# verbose = 2\nverbose = true\nparallel_threshold = 5\n  mkml_recursion_limit = 7  \n[Units]\ndefault_output = x\n[Settings]\nauto_simplify = false\n";
        assert_eq!(
            ignored_settings_in(text),
            [("verbose", "true"), ("mkml_recursion_limit", "7"), ("auto_simplify", "false")].map(|(k, v)| (k.to_string(), v.to_string()))
        );
        let warnings = ignored_setting_warnings("overrides", "[Settings]\ndefault_system = SI\n");
        assert_eq!(warnings, ["overrides: [Settings] default_system = SI is accepted and ignored: no part of physure reads it, so it changes nothing"]);
        assert!(ignored_settings_in(DEFAULT_PHYSURE_CONF).is_empty(), "the shipped catalog sets none of them");
    }

    #[test]
    fn a_frozen_replay_accepts_the_keys_the_core_does_not_use() {
        for key in CORE_UNUSED_SETTINGS {
            let text = format!("[Settings]\n{key} = anything\n");
            assert!(frozen_problems(&text).is_empty(), "{key}: {:?}", frozen_problems(&text));
        }
        assert!(frozen_problems("[Settings]\nparallel_threshold = 0\npropagation_mode = monte_carlo\nangle_mode = dimensionless\n").is_empty());
        assert!(frozen_problems("[Constants]\nzz = -1.5 1\nyy = 2\nxx = 3.0e-3 J*mol^-1*K^-1, a description, x\n").is_empty());
        assert!(frozen_problems("[Compatibility]\nperiodic = hertz\nsr, plain = convert\n").is_empty());
        assert!(frozen_problems("# only a comment\n\n").is_empty());
        // a recipe, the base unit named after its dimension, flags, a zero point and a level are all read
        let units = "[Units]\nzza = 2.0, L, m, [zza]\nzzb = 1.0, M*L*T^-2, kg*m/s^2, noprefix, [zzb]\nzzc = 1.0, 10.0, O, [zzc]\nzzd = Log, 10, 1.0, 1, noprefix, [zzd]\nzze = 1.0, A, own_kind, [zze]\nzzf = 1.0, O, interval, [zzf]\n";
        assert!(frozen_problems(units).is_empty(), "{:?}", frozen_problems(units));
        // a unit and a constant of one name may both be overridden, to one value
        let both = "[Units]\nbohr = 6e-11, L, m, [a0, bohr, bohr_radius]\n[Constants]\nbohr_radius = 6e-11 m\n";
        assert!(frozen_problems(both).is_empty(), "{:?}", frozen_problems(both));
    }

    /// The names of the shipped catalog that are a unit and a constant (measured 2026-11-14: 7 of the 411 constants),
    /// each with one value, so `2 bohr_radius` (the unit) and `2 * bohr_radius` (the constant) agree.
    #[test]
    fn every_name_of_the_shipped_catalog_that_is_a_unit_and_a_constant_carries_one_value() {
        let (registry, _) = embedded_catalog();
        let mut both: Vec<&String> = registry.constants_meta.keys().filter(|name| registry.get_unit(name).is_some()).collect();
        both.sort();
        assert_eq!(
            both,
            ["bohr_radius", "electron_mass", "electron_volt", "planck_length", "planck_mass", "planck_temperature", "planck_time"],
            "a new name that is both: check that its unit and its constant agree"
        );
        for name in registry.constants_meta.keys() {
            assert_eq!(unit_constant_disagreement(name, registry), None, "{name}");
        }
        assert_eq!(registry.constants_meta.len(), 411, "the count the clash list was measured against");
    }

    #[test]
    fn a_lenient_reading_drops_a_constant_that_disagrees_with_its_unit_and_says_so() {
        let (mut registry, mut constants) = embedded_catalog().clone();
        let problems = read_leniently("[Constants]\nbohr_radius = 6e-11 m\nzz_ok = 2 m\n", &mut registry, &mut constants, false);
        assert!(problems.iter().any(|p| p.contains("bohr_radius")), "{problems:?}");
        assert!(!registry.constants_meta.contains_key("bohr_radius") && !constants.contains_key("bohr_radius"));
        assert!(registry.get_unit("bohr_radius").is_some(), "the name reads as the unit");
        assert_eq!(registry.constants_meta["zz_ok"].value, "2 m", "the rest of the text applies");
    }

    #[test]
    fn a_constant_is_read_as_a_number_and_a_unit_of_the_registry() {
        let (registry, _) = embedded_catalog();
        let r = constant_quantity("8.31446261815324 J*mol^-1*K^-1", registry).unwrap();
        assert_eq!(r.value.mean(), 8.31446261815324);
        assert_eq!(r.value.std_dev(), 0.0);
        assert_eq!(constant_quantity("2", registry).unwrap().unit, RationalUnit::dimensionless());
        for bad in ["", "abc m", "inf m", "1 nosuchunit", "1 +/- 0.1 m", "1 ± 0.1 m"] {
            assert!(constant_quantity(bad, registry).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_setting_is_parsed_once_and_applied_from_its_value() {
        assert_eq!(parse_setting("propagation_mode = unscented"), Ok(Setting::PropagationMode(PropagationMode::Unscented)));
        assert_eq!(parse_setting(" angle_mode =  dimension "), Ok(Setting::AngleMode(AngleMode::Dimension)));
        assert_eq!(parse_setting("parallel_threshold = 3"), Ok(Setting::ParallelThreshold(3)));
        assert_eq!(parse_setting("max_loop_iterations = 4"), Ok(Setting::MaxLoopIterations(4)));
        assert_eq!(parse_setting("max_pipeline_steps = 5"), Ok(Setting::MaxPipelineSteps(5)));
        assert_eq!(parse_setting("mkml_recursion_limit = 100"), Ok(Setting::Ignored { key: "mkml_recursion_limit".into(), value: "100".into() }));
        for bad in ["nokey", "frobnicate = 1", "parallel_threshold = 1.5", " = 3"] {
            assert!(parse_setting(bad).is_err(), "{bad:?}");
        }
        for bad in ["propagation_mode = corelated", "angle_mode = degrees", "max_loop_iterations = -1"] {
            assert!(parse_setting(bad).unwrap_err().contains(bad.split('=').nth(1).unwrap().trim()), "{bad:?}");
        }
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let original = crate::settings::max_pipeline_steps();
        Setting::MaxPipelineSteps(9).apply();
        assert_eq!(crate::settings::max_pipeline_steps(), 9);
        crate::settings::set_max_pipeline_steps(original);
        let collected = ProcessSettings { settings: vec![], compatibility: Some(vec!["steradian, plain = convert".into()]) };
        crate::units::angle_mode::set_marker_grants(true, true);
        collected.apply();
        assert_eq!(marker_grants(), (false, true));
    }

    #[test]
    fn activation_gives_the_file_alone_over_the_defaults_in_one_step() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let file = ProcessSettings {
            settings: vec![Setting::ParallelThreshold(1), Setting::MaxPipelineSteps(5), Setting::ParallelThreshold(777)],
            compatibility: Some(vec!["steradian, plain = convert".into()]),
        };
        // leftovers of some other file: all of them must go
        crate::settings::set_max_loop_iterations(3);
        crate::units::angle_mode::set_marker_grants(true, false);
        file.activate();
        assert_eq!(crate::settings::parallel_threshold(), 777);
        assert_eq!(crate::settings::max_pipeline_steps(), 5);
        assert_eq!(crate::settings::max_loop_iterations(), crate::settings::DEFAULT_MAX_LOOP_ITERATIONS);
        assert_eq!(marker_grants(), (false, true));
        ProcessSettings::default().activate();
        assert_eq!(crate::settings::parallel_threshold(), crate::settings::DEFAULT_PARALLEL_THRESHOLD);
        assert_eq!(marker_grants(), (true, true));
    }

    #[test]
    fn approx_rel_tol_is_a_finite_fraction_between_zero_and_one() {
        assert_eq!(parse_setting("approx_rel_tol = 1e-6"), Ok(Setting::ApproxRelTol(1e-6)));
        assert_eq!(parse_setting("approx_rel_tol = 0.5"), Ok(Setting::ApproxRelTol(0.5)));
        for bad in ["0", "-0.0", "-1e-3", "nan", "NaN", "inf", "-inf", "1", "1.0", "2", "abc", ""] {
            let why = parse_setting(&format!("approx_rel_tol = {bad}")).unwrap_err();
            assert!(why.contains("approx_rel_tol") && why.contains(&format!("'{bad}'")), "{bad:?}: {why}");
        }
        assert!(parse_setting("frobnicate = 1").unwrap_err().contains("approx_rel_tol"), "the list of keys names it");
    }

    #[test]
    fn the_lenient_reader_applies_a_good_approx_rel_tol_and_keeps_the_one_in_force_for_a_bad_one() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let original = crate::settings::set_approx_rel_tol(2e-3);
        let (mut reg, mut constants) = (UnitRegistry::new(), HashMap::new());
        for bad in ["0", "-1", "nan", "inf", "abc", "1"] {
            parse_physure_conf(&format!("[Settings]\napprox_rel_tol = {bad}\n"), &mut reg, &mut constants);
            assert_eq!(crate::settings::approx_rel_tol(), 2e-3, "{bad:?} replaced the value in force");
        }
        parse_physure_conf("[Settings]\napprox_rel_tol = 1e-5\n", &mut reg, &mut constants);
        assert_eq!(crate::settings::approx_rel_tol(), 1e-5);
        crate::settings::set_approx_rel_tol(original);
    }

    #[test]
    fn activation_sets_approx_rel_tol_from_the_file_or_its_default() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        let original = crate::settings::approx_rel_tol();
        ProcessSettings { settings: vec![Setting::ApproxRelTol(1e-6)], compatibility: None }.activate();
        assert_eq!(crate::settings::approx_rel_tol(), 1e-6);
        ProcessSettings::default().activate();
        assert_eq!(crate::settings::approx_rel_tol(), crate::settings::DEFAULT_APPROX_REL_TOL);
        crate::settings::set_approx_rel_tol(original);
    }

    #[test]
    fn monte_carlo_samples_is_a_whole_number_from_two_to_ten_million() {
        assert_eq!(parse_setting("monte_carlo_samples = 2"), Ok(Setting::MonteCarloSamples(2)));
        assert_eq!(parse_setting("monte_carlo_samples = 1000"), Ok(Setting::MonteCarloSamples(1000)));
        assert_eq!(parse_setting("monte_carlo_samples = 10000000"), Ok(Setting::MonteCarloSamples(10_000_000)));
        for bad in ["0", "1", "-1", "1.5", "abc", "", "10000001", "20000000", "1e3"] {
            let why = parse_setting(&format!("monte_carlo_samples = {bad}")).unwrap_err();
            assert!(why.contains("monte_carlo_samples") && why.contains(&format!("'{bad}'")), "{bad:?}: {why}");
        }
        assert!(parse_setting("frobnicate = 1").unwrap_err().contains("monte_carlo_samples"), "the list of keys names it");
    }

    #[test]
    fn the_lenient_reader_applies_a_good_monte_carlo_samples_and_keeps_the_one_in_force_for_a_bad_one() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let original = crate::settings::set_monte_carlo_samples(1234);
        let (mut reg, mut constants) = (UnitRegistry::new(), HashMap::new());
        for bad in ["0", "1", "abc", "20000000", "-3"] {
            parse_physure_conf(&format!("[Settings]\nmonte_carlo_samples = {bad}\n"), &mut reg, &mut constants);
            assert_eq!(crate::settings::monte_carlo_samples(), 1234, "{bad:?} replaced the value in force");
        }
        parse_physure_conf("[Settings]\nmonte_carlo_samples = 50\n", &mut reg, &mut constants);
        assert_eq!(crate::settings::monte_carlo_samples(), 50);
        crate::settings::set_monte_carlo_samples(original);
    }

    #[test]
    fn activation_sets_monte_carlo_samples_from_the_file_or_its_default() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        ProcessSettings { settings: vec![Setting::MonteCarloSamples(64)], compatibility: None }.activate();
        assert_eq!(crate::settings::monte_carlo_samples(), 64);
        ProcessSettings::default().activate();
        assert_eq!(crate::settings::monte_carlo_samples(), crate::settings::DEFAULT_MONTE_CARLO_SAMPLES);
        Setting::MonteCarloSamples(65).apply();
        assert_eq!(crate::settings::monte_carlo_samples(), 65);
    }

    #[test]
    fn max_while_iterations_is_a_whole_number_of_at_least_one() {
        assert_eq!(parse_setting("max_while_iterations = 1"), Ok(Setting::MaxWhileIterations(1)));
        assert_eq!(parse_setting("max_while_iterations = 20000"), Ok(Setting::MaxWhileIterations(20_000)));
        for bad in ["0", "-1", "1.5", "abc", ""] {
            let why = parse_setting(&format!("max_while_iterations = {bad}")).unwrap_err();
            assert!(why.contains("max_while_iterations") && why.contains(&format!("'{bad}'")), "{bad:?}: {why}");
        }
        assert!(parse_setting("frobnicate = 1").unwrap_err().contains("max_while_iterations"), "the list of keys names it");
    }

    #[test]
    fn the_lenient_reader_applies_a_good_max_while_iterations_and_keeps_the_one_in_force_for_a_bad_one() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        crate::settings::set_max_while_iterations(321);
        let (mut reg, mut constants) = (UnitRegistry::new(), HashMap::new());
        for bad in ["0", "-1", "abc"] {
            parse_physure_conf(&format!("[Settings]\nmax_while_iterations = {bad}\n"), &mut reg, &mut constants);
            assert_eq!(crate::settings::max_while_iterations(), 321, "{bad:?} replaced the value in force");
        }
        parse_physure_conf("[Settings]\nmax_while_iterations = 50\n", &mut reg, &mut constants);
        assert_eq!(crate::settings::max_while_iterations(), 50);
    }

    #[test]
    fn activation_sets_max_while_iterations_from_the_file_or_its_default() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        ProcessSettings { settings: vec![Setting::MaxWhileIterations(64)], compatibility: None }.activate();
        assert_eq!(crate::settings::max_while_iterations(), 64);
        ProcessSettings::default().activate();
        assert_eq!(crate::settings::max_while_iterations(), crate::settings::DEFAULT_MAX_WHILE_ITERATIONS);
        Setting::MaxWhileIterations(65).apply();
        assert_eq!(crate::settings::max_while_iterations(), 65);
    }

    #[test]
    fn asymmetric_shape_is_dimidiated_or_fechner() {
        assert_eq!(parse_setting("asymmetric_shape = dimidiated"), Ok(Setting::AsymmetricShape(ShapeKind::Dimidiated)));
        assert_eq!(parse_setting("asymmetric_shape = fechner"), Ok(Setting::AsymmetricShape(ShapeKind::Fechner)));
        for bad in ["foo", "", "gaussian", "1"] {
            let why = parse_setting(&format!("asymmetric_shape = {bad}")).unwrap_err();
            assert!(why.contains("asymmetric_shape") && why.contains(&format!("'{bad}'")), "{bad:?}: {why}");
        }
        assert!(parse_setting("frobnicate = 1").unwrap_err().contains("asymmetric_shape"), "the list of keys names it");
        let file = ProcessSettings { settings: vec![Setting::AsymmetricShape(ShapeKind::Fechner)], compatibility: None };
        assert_eq!(file.effective().asymmetric_shape, ShapeKind::Fechner);
        assert_eq!(ProcessSettings::default().effective().asymmetric_shape, ShapeKind::DEFAULT);
    }

    /// A bad value keeps the shape in force (the compiled one). Applying a good one process-wide is tested in
    /// `tests/uncertainty_settings.rs`: here it would change the moments of the other tests of the crate.
    #[test]
    fn the_lenient_reader_keeps_the_shape_in_force_for_a_bad_one() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let (mut reg, mut constants) = (UnitRegistry::new(), HashMap::new());
        for bad in ["foo", "", "fechnerr"] {
            parse_physure_conf(&format!("[Settings]\nasymmetric_shape = {bad}\n"), &mut reg, &mut constants);
            assert_eq!(crate::uncertainty::shapes::asymmetric_shape(), ShapeKind::DEFAULT, "{bad:?}");
        }
    }

    #[test]
    fn the_display_settings_read_their_ranges_and_nothing_else() {
        assert_eq!(parse_setting("display_significant_digits = 1"), Ok(Setting::DisplaySignificantDigits(1)));
        assert_eq!(parse_setting("display_significant_digits = 17"), Ok(Setting::DisplaySignificantDigits(17)));
        assert_eq!(parse_setting("display_scientific_below = 1e-6"), Ok(Setting::DisplayScientificBelow(1e-6)));
        assert_eq!(parse_setting("display_scientific_below = 1"), Ok(Setting::DisplayScientificBelow(1.0)));
        assert_eq!(parse_setting("display_scientific_from = 1"), Ok(Setting::DisplayScientificFrom(1.0)));
        assert_eq!(parse_setting("display_scientific_from = 1e300"), Ok(Setting::DisplayScientificFrom(1e300)));
        assert_eq!(parse_setting("uncertainty_digits = gum"), Ok(Setting::UncertaintyDigits(UncertaintyDigits::Gum)));
        assert_eq!(parse_setting("uncertainty_digits = 2"), Ok(Setting::UncertaintyDigits(UncertaintyDigits::Figures(2))));
        for (key, bad) in [
            ("display_significant_digits", ["0", "18", "-1", "1.5", "abc", "", "15e0", "nan"]),
            ("display_scientific_below", ["0", "9e-7", "1.5", "-1e-4", "abc", "", "nan", "inf"]),
            ("display_scientific_from", ["0", "0.99", "-1e16", "abc", "", "nan", "inf", "-inf"]),
            ("uncertainty_digits", ["0", "18", "Gum", "GUM", "1.5", "", "-2", "two"]),
        ] {
            for bad in bad {
                let why = parse_setting(&format!("{key} = {bad}")).unwrap_err();
                assert!(why.contains(key) && why.contains(&format!("'{bad}'")), "{key} = {bad:?}: {why}");
            }
            assert!(parse_setting("frobnicate = 1").unwrap_err().contains(key), "the list of keys names {key}");
        }
        let file = ProcessSettings {
            settings: vec![Setting::DisplaySignificantDigits(6), Setting::UncertaintyDigits(UncertaintyDigits::Figures(2))],
            compatibility: None,
        };
        let expected = DisplayRules { significant_digits: 6, uncertainty_digits: UncertaintyDigits::Figures(2), ..DisplayRules::DEFAULT };
        assert_eq!(file.effective().display, expected);
        assert_eq!(ProcessSettings::default().effective().display, DisplayRules::DEFAULT);
    }

    /// Activation sets the display rules process-wide; this test only activates the compiled rules (a process-wide
    /// change of them would change the text other tests of this crate assert: the other values are tested in
    /// `tests/display_settings.rs`, a binary of its own).
    #[test]
    fn activation_restores_the_compiled_display_rules() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        ProcessSettings { settings: vec![Setting::DisplaySignificantDigits(15)], compatibility: None }.activate();
        assert_eq!(*crate::settings::process_display_rules(), DisplayRules::DEFAULT);
    }

    #[test]
    fn default_dof_is_a_positive_number_or_infinity() {
        assert_eq!(parse_setting("default_dof = inf"), Ok(Setting::DefaultDof(f64::INFINITY)));
        assert_eq!(parse_setting("default_dof = 50"), Ok(Setting::DefaultDof(50.0)));
        assert_eq!(parse_setting("default_dof = 2.5"), Ok(Setting::DefaultDof(2.5)));
        for bad in ["0", "-1", "-0.0", "abc", "nan", "-inf", ""] {
            let why = parse_setting(&format!("default_dof = {bad}")).unwrap_err();
            assert!(why.contains("default_dof") && why.contains(&format!("'{bad}'")), "{bad:?}: {why}");
        }
        assert!(parse_setting("frobnicate = 1").unwrap_err().contains("default_dof"), "the list of keys names it");
        let file = ProcessSettings { settings: vec![Setting::DefaultDof(50.0)], compatibility: None };
        assert_eq!(file.effective().default_dof, 50.0);
        assert_eq!(ProcessSettings::default().effective().default_dof, f64::INFINITY);
    }

    /// A bad value keeps the default in force (the compiled one). Applying a good one process-wide is tested in
    /// `tests/uncertainty_settings.rs`: here it would change the degrees of freedom the other tests read.
    #[test]
    fn monte_carlo_seed_is_a_whole_number_or_random() {
        assert_eq!(parse_setting("monte_carlo_seed = 0"), Ok(Setting::MonteCarloSeed(MonteCarloSeed::Fixed(0))));
        assert_eq!(parse_setting("monte_carlo_seed = 5"), Ok(Setting::MonteCarloSeed(MonteCarloSeed::Fixed(5))));
        assert_eq!(
            parse_setting("monte_carlo_seed = 18446744073709551615"),
            Ok(Setting::MonteCarloSeed(MonteCarloSeed::Fixed(u64::MAX)))
        );
        assert_eq!(parse_setting("monte_carlo_seed = random"), Ok(Setting::MonteCarloSeed(MonteCarloSeed::Random)));
        for bad in ["-3", "abc", "1.5", "Random", "18446744073709551616", "", "0x10"] {
            let why = parse_setting(&format!("monte_carlo_seed = {bad}")).unwrap_err();
            assert!(why.contains("monte_carlo_seed") && why.contains(&format!("'{bad}'")), "{bad:?}: {why}");
        }
        assert!(parse_setting("frobnicate = 1").unwrap_err().contains("monte_carlo_seed"), "the list of keys names it");
        let file = ProcessSettings { settings: vec![Setting::MonteCarloSeed(MonteCarloSeed::Random)], compatibility: None };
        assert_eq!(file.effective().monte_carlo_seed, MonteCarloSeed::Random);
        assert_eq!(ProcessSettings::default().effective().monte_carlo_seed, MonteCarloSeed::Fixed(0));
        assert_eq!((MonteCarloSeed::Fixed(7).name(), MonteCarloSeed::Random.name()), ("7".to_string(), "random".to_string()));
    }

    /// A bad value keeps the seed in force; a good one is applied process-wide (and put back by the guard).
    #[test]
    fn the_lenient_reader_applies_a_seed_and_keeps_the_one_in_force_for_a_bad_one() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = crate::units::angle_mode::RestoreProcessState::capture();
        settle_catalog_settings();
        let (mut reg, mut constants) = (UnitRegistry::new(), HashMap::new());
        parse_physure_conf("[Settings]\nmonte_carlo_seed = 42\n", &mut reg, &mut constants);
        assert_eq!(crate::uncertainty::monte_carlo_seed(), MonteCarloSeed::Fixed(42));
        for bad in ["-3", "abc", "1.5"] {
            parse_physure_conf(&format!("[Settings]\nmonte_carlo_seed = {bad}\n"), &mut reg, &mut constants);
            assert_eq!(crate::uncertainty::monte_carlo_seed(), MonteCarloSeed::Fixed(42), "{bad:?}");
        }
        parse_physure_conf("[Settings]\nmonte_carlo_seed = random\n", &mut reg, &mut constants);
        assert_eq!(crate::uncertainty::monte_carlo_seed(), MonteCarloSeed::Random);
    }

    #[test]
    fn the_lenient_reader_keeps_the_default_dof_in_force_for_a_bad_one() {
        let _lock = crate::settings::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        settle_catalog_settings();
        let (mut reg, mut constants) = (UnitRegistry::new(), HashMap::new());
        for bad in ["0", "-1", "abc", "nan"] {
            parse_physure_conf(&format!("[Settings]\ndefault_dof = {bad}\n"), &mut reg, &mut constants);
            assert_eq!(crate::uncertainty::dof::default_dof(), f64::INFINITY, "{bad:?}");
        }
    }

    #[test]
    fn a_later_compatibility_section_replaces_an_earlier_one_and_a_periodic_only_one_does_not() {
        let (mut registry, mut constants, mut collected) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
        replay_source(&mut registry, &mut constants, &mut collected, DEFAULT_PHYSURE_CONF, true).unwrap();
        let shipped = collected.compatibility.clone();
        replay_source(&mut registry, &mut constants, &mut collected, "[Compatibility]\nperiodic = hertz\n", false).unwrap();
        assert_eq!(collected.compatibility, shipped);
        replay_source(&mut registry, &mut constants, &mut collected, "[Compatibility]\n", false).unwrap();
        assert_eq!(collected.compatibility, Some(vec![]));
    }

    #[test]
    fn a_row_names_its_marker_in_either_order_and_either_spelling() {
        assert!(matches!(read_compatibility_row("radian, plain = convert"), Ok(Marker::Radian)));
        assert!(matches!(read_compatibility_row("rad, plain = convert"), Ok(Marker::Radian)));
        assert!(matches!(read_compatibility_row("plain, steradian = convert"), Ok(Marker::Steradian)));
        assert!(matches!(read_compatibility_row("sr, plain = convert"), Ok(Marker::Steradian)));
    }

    /// The shipped catalog with the `interval` flag taken off its lines: the text of a file frozen before the flag.
    fn catalog_before_the_interval_flag() -> String {
        let old = DEFAULT_PHYSURE_CONF.replace(", O, interval, [delta_deg", ", O, [delta_deg");
        assert_eq!(old.matches(", O, [delta_deg").count(), 2, "the test's anchors are gone from the catalog");
        old
    }

    fn interval_of(registry: &UnitRegistry, name: &str) -> bool {
        registry.get_unit(name).unwrap_or_else(|| panic!("{name}")).is_interval()
    }

    #[test]
    fn the_shipped_intervals_carry_the_flag_on_their_lines() {
        // the trusted rule below is for older texts only: the catalog says it itself
        for key in SHIPPED_INTERVALS {
            let line = DEFAULT_PHYSURE_CONF.lines().find(|l| l.split('=').next().is_some_and(|k| k.trim() == key)).unwrap();
            assert!(has_flag(line.split_once('=').unwrap().1.split('[').next().unwrap(), "interval"), "{line}");
        }
    }

    #[test]
    fn a_trusted_text_without_the_flag_keeps_the_shipped_intervals_and_an_untrusted_one_does_not() {
        let old = catalog_before_the_interval_flag();
        let (mut trusted, mut constants, mut collected) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
        replay_source(&mut trusted, &mut constants, &mut collected, &old, true).unwrap();
        for name in ["delta_celsius", "delta_degC", "degC_delta", "kdelta_degC", "delta_fahrenheit", "delta_degF"] {
            assert!(interval_of(&trusted, name), "{name}");
        }
        assert!(!interval_of(&trusted, "K") && !interval_of(&trusted, "degR"));
        // a user's text that restates the entry without the flag is refused, and the entry stays an interval
        let mut user = trusted.clone();
        let refused = extend_registry(&mut user, &mut HashMap::new(), "[Units]\ndelta_celsius = 1.0, O, [delta_degC, delta_celsius]\n");
        assert!(refused.unwrap_err().iter().any(|p| p.contains("needs the `interval` flag")));
        assert!(interval_of(&user, "delta_degC"));
        // read leniently (a project's physure.conf), the line is reported and ignored
        let mut lenient = trusted.clone();
        let problems = read_leniently_collected(
            "[Units]\ndelta_celsius = 1.0, O, [delta_degC, delta_celsius]\n",
            &mut lenient,
            &mut HashMap::new(),
            &mut ProcessSettings::default(),
            false,
        );
        assert!(problems.iter().any(|p| p.contains("`delta_celsius` is a temperature interval")), "{problems:?}");
        assert!(interval_of(&lenient, "delta_degC"));
        // the old line read as a user's (untrusted) is a user's line
        let mut untrusted = UnitRegistry::new();
        let old_lines =
            "[Units]\nkelvin = 1.0, O, [K, kelvin]\ncelsius = 1.0, 273.15, O, [degC]\ndelta_celsius = 1.0, O, [delta_degC, degC_delta, delta_celsius]\n";
        extend_registry(&mut untrusted, &mut HashMap::new(), old_lines).unwrap();
        assert!(!interval_of(&untrusted, "delta_degC"));
        let (mut again, mut c, mut s) = (UnitRegistry::new(), HashMap::new(), ProcessSettings::default());
        replay_source(&mut again, &mut c, &mut s, old_lines, true).unwrap();
        assert!(interval_of(&again, "delta_degC"), "the same lines, trusted");
    }
}
