//! `pi`, `π` and the constants of the catalog in force are global identifiers of PHS (Decision 2 of the
//! language-semantics plan, with the review's corrections).
//!
//! One resolution order for a bare name: lexical bindings and variables, user functions, `pi`/`π` and the catalog's
//! constants, units, then unknown (the text of the name). Binding a global identifier is an error, at run time and in
//! `phs check`. The catalog in force decides which names are constants, so the tests that put a `physure.conf` or a
//! frozen file in the current directory change process-wide state: every test of this file takes one lock, and those
//! put everything back when they end.
//!
//! Measured before (the binary of HEAD 923bfc4): `pi` was the text "pi", `2 pi` and `pi * 2` an unknown-name error,
//! `pi = 3` accepted, `molar_gas_constant` the text of its name, `bohr_radius` one `bohr_radius` (the unit), and an
//! equation using a constant reported it as a missing argument.

use physure_core::frozen::active::{reset_cache, ENV_CONF, ENV_REQUIRE, FROZEN_FILE_NAME};
use physure_core::frozen::{engine_pairs, EngineIdentity, EnginePolicy, EntryInput, FrozenFile, FrozenParts, Source};
use physure_core::units::conf::{build_registry_from_conf, constant_quantity, extend_registry, DEFAULT_PHYSURE_CONF};
use physure_core::units::{with_catalog, UnitRegistry};
use physure_core::Quantity;
use physure_script::check::{check_source, Severity};
use physure_script::interpreter::eval_phs;
use physure_script::resolver::{MemoryModuleResolver, ModuleExport};
use physure_script::{parse_phs, PhsInterpreter, PhsValue};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// A temporary current directory; the previous one and the cache of the configuration in force are restored on drop.
struct Dir {
    path: PathBuf,
    previous: PathBuf,
}

impl Dir {
    fn new(name: &str) -> Dir {
        for variable in [ENV_CONF, ENV_REQUIRE] {
            assert!(std::env::var_os(variable).is_none(), "{variable} is set in the environment of the tests: unset it");
        }
        let path = std::env::temp_dir().join(format!("physure_global_constants_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(&path).unwrap();
        reset_cache();
        Dir { path, previous }
    }

    fn write(&self, name: &str, text: &str) {
        fs::write(self.path.join(name), text).unwrap();
        reset_cache();
    }

    fn remove(&self, name: &str) {
        fs::remove_file(self.path.join(name)).unwrap();
        reset_cache();
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous);
        reset_cache();
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// A valid frozen file: the embedded catalog, and `overrides` as source 2.
fn frozen(overrides: &str) -> String {
    FrozenFile::new(
        "2026-11-14T10:00:00Z",
        FrozenParts {
            engine: engine_pairs(&EngineIdentity::current(), EnginePolicy::Release, None).unwrap(),
            sources: vec![Source::new("embedded", DEFAULT_PHYSURE_CONF).unwrap(), Source::new("overrides", overrides).unwrap()],
            ..FrozenParts::default()
        },
        EntryInput { time: "2026-11-14T10:00:00Z", action: "freeze", engine: "test", actor: "t@h", reason: "test", diff: "d" },
    )
    .unwrap()
    .render()
    .unwrap()
}

fn last(source: &str) -> PhsValue {
    eval_phs(source).unwrap_or_else(|e| panic!("{source:?}: {e}")).into_iter().last().expect("a value")
}

fn refused(source: &str) -> String {
    match eval_phs(source) {
        Ok(values) => panic!("{source:?} should be refused, it gave {:?}", values.last().map(|v| v.to_string())),
        Err(e) => e.to_string(),
    }
}

fn number(source: &str) -> f64 {
    match last(source) {
        PhsValue::Number(n) => n,
        PhsValue::Quantity(q) if q.unit.is_plain_number() => q.value.mean(),
        other => panic!("{source:?} should be a number, it is {other:?}"),
    }
}

fn quantity(source: &str) -> Quantity {
    match last(source) {
        PhsValue::Quantity(q) => q,
        other => panic!("{source:?} should be a quantity, it is {other:?}"),
    }
}

/// `source` in `unit`.
fn in_unit(source: &str, unit: &str) -> f64 {
    quantity(source).to(unit).unwrap_or_else(|e| panic!("{source:?} is not in {unit}: {e}")).value.mean()
}

fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() <= 1e-12 * expected.abs()
}

/// The constant `name` of the catalog in force, read as the catalog says.
fn catalog(name: &str) -> Quantity {
    let (registry, _) = build_registry_from_conf();
    constant_quantity(&registry.constants_meta[name].value, &registry).unwrap()
}

// ---- pi

#[test]
fn pi_and_its_greek_letter_are_the_circle_constant() {
    let _lock = lock();
    let pi = std::f64::consts::PI;
    // before: the text "pi" / "π"
    assert_eq!(number("pi"), pi);
    assert_eq!(number("π"), pi);
    // before: an unknown-name error for each
    assert_eq!(number("pi * 2"), 2.0 * pi);
    assert_eq!(number("2 pi"), 2.0 * pi);
    assert_eq!(number("2 π"), 2.0 * pi);
    assert_eq!(last("sin(pi/2) == 1"), PhsValue::Bool(true));
    assert!(number("cos(π)") + 1.0 == 0.0);
    let q = quantity("2 * pi * 1 m");
    assert_eq!(q.to("m").unwrap().value.mean(), 2.0 * pi);
    assert_eq!(in_unit("r = 2 m\npi * r^2", "m^2"), 4.0 * pi);
}

// ---- the catalog's constants

#[test]
fn a_constant_of_the_catalog_is_its_quantity() {
    let _lock = lock();
    // before: `molar_gas_constant * 300 K` was an unknown-name error
    let rt = quantity("molar_gas_constant * 300 K");
    let r = 8.31446261815324;
    assert!(close(rt.to("J/mol").unwrap().value.mean(), r * 300.0), "{rt}");
    assert!(rt.to("J").is_err(), "the unit is J/mol: {rt}");
    for name in [
        "speed_of_light_in_vacuum",
        "planck_constant",
        "boltzmann_constant",
        "avogadro_constant",
        "elementary_charge",
        "molar_gas_constant",
        "newtonian_constant_of_gravitation",
        "vacuum_electric_permittivity",
        "standard_acceleration_of_gravity",
        "electron_mass",
    ] {
        let phs = quantity(name);
        let expected = catalog(name);
        assert_eq!(phs.value.mean(), expected.value.mean(), "{name}");
        assert_eq!(phs.unit, expected.unit, "{name}");
        assert_eq!(phs.value.std_dev(), 0.0, "{name}: no constant of the shipped catalog carries an uncertainty");
    }
    // `e`, `c`, `k`, `R` are not constants (only the catalog's own names are); `h`, `g`, `G` stay units
    for name in ["e", "c", "k", "R"] {
        assert_eq!(last(name), PhsValue::String(name.to_string()), "{name}");
    }
    assert_eq!(in_unit("h", "s"), 3600.0);
    assert_eq!(in_unit("g", "kg"), 1e-3);
}

#[test]
fn a_name_that_is_a_unit_and_a_constant_reads_one_value_both_ways() {
    let _lock = lock();
    for name in ["bohr_radius", "electron_mass", "electron_volt", "planck_length", "planck_mass", "planck_temperature", "planck_time"] {
        let literal = quantity(&format!("2 {name}")).canonical_magnitude();
        let product = quantity(&format!("2 * {name}")).canonical_magnitude();
        let alone = quantity(name).canonical_magnitude();
        assert!(close(product, literal), "{name}: 2 {name} = {literal}, 2 * {name} = {product}");
        assert!(close(alone * 2.0, literal), "{name}");
        // the bare name is the constant (constants come before units), in the catalog's unit
        assert_eq!(quantity(name).unit, catalog(name).unit, "{name}");
    }
}

#[test]
fn a_variable_or_a_unit_still_resolves_as_before() {
    let _lock = lock();
    // a variable named like a unit wins over the unit
    assert_eq!(number("m = 5\nm * 2"), 10.0);
    assert!(close(in_unit("g = 9.81 m/s^2\ng * 2 kg", "N"), 19.62));
    // a free unit symbol is one of it (the symbolic layer relies on it)
    assert_eq!(in_unit("J", "J"), 1.0);
    // a name that is nothing is still its text (T5, open)
    assert_eq!(last("zz_unknown_name"), PhsValue::String("zz_unknown_name".into()));
    // a function and a variable are found before the constants: a call of a user function named nothing global
    assert_eq!(number("f(x) = x * pi\nf(2)"), 2.0 * std::f64::consts::PI);
}

#[test]
fn an_equation_that_uses_a_constant_does_not_miss_it_as_an_argument() {
    let _lock = lock();
    // before: "Missing argument(s) for equation 'eq': speed_of_light_in_vacuum"
    let e = in_unit("eq = \"E = m * speed_of_light_in_vacuum^2\"\neq(m=1 kg)", "J");
    assert!(close(e, 299792458.0f64.powi(2)), "{e}");
    let area = number("circle = \"A = pi * r^2\"\ncircle(r=2)");
    assert!(close(area, 4.0 * std::f64::consts::PI), "{area}");
    let message = refused("eq = \"y = 2 * x\"\neq(pi=3)");
    assert!(message.contains("`pi` is the constant"), "{message}");
}

// ---- shadowing

/// Every way a program binds a name, with `name`.
fn bindings_of(name: &str) -> Vec<String> {
    vec![
        format!("{name} = 3"),
        format!("f({name}) = {name} * 2"),
        format!("fn g({name}) = {name}"),
        format!("for {name} in 0..3 {{ 1 }}"),
        format!("x = 2 * y where y = 1, {name} = 3"),
        format!("{name}(x) = x"),
        // inside a body that never runs
        format!("fn h(x) = x * a where {name} = 1, a = 2"),
        format!("fn k(x) = for {name} in 0..3 {{ x }}"),
    ]
}

#[test]
fn binding_pi_or_a_constant_is_an_error_at_run_time_and_in_the_checker() {
    let _lock = lock();
    for name in ["pi", "π", "molar_gas_constant", "bohr_radius"] {
        for program in bindings_of(name) {
            let message = refused(&program);
            assert!(message.contains(&format!("`{name}` is")) && message.contains("choose another name"), "{program:?}: {message}");
            let diagnostics = check_source(&program).unwrap_or_else(|e| panic!("{program:?}: {e}"));
            assert!(
                diagnostics.iter().any(|d| d.code == "constant-shadowing" && d.severity == Severity::Error && d.line == 1),
                "{program:?}: {diagnostics:?}"
            );
        }
    }
    let message = refused("pi = 3");
    assert!(message.contains("`pi` is the constant π and cannot be bound as a variable"), "{message}");
    let message = refused("molar_gas_constant = 8");
    assert!(message.contains("constant of the catalog in force (8.31446261815324 J*mol^-1*K^-1)"), "{message}");
    // the report `phs eval` prints says it once, at its line
    let report = physure_script::report::evaluate("x = 1\npi = 3", physure_script::Limits::default(), 0);
    assert!(!report.ok);
    assert!(report.diagnostics.iter().any(|d| d.code == "constant-shadowing" && d.line == 2), "{:?}", report.diagnostics);
}

#[test]
fn a_name_close_to_a_constant_is_not_one() {
    let _lock = lock();
    for program in ["pie = 3\npie", "pi2 = 3\npi2", "molar_gas = 8\nmolar_gas", "R = 8.314\nR", "e = 2.718\ne", "c = 3\nc"] {
        eval_phs(program).unwrap_or_else(|e| panic!("{program:?}: {e}"));
        assert!(check_source(program).unwrap().iter().all(|d| d.code != "constant-shadowing"), "{program:?}");
    }
}

fn module_defining(name: &str) -> PhsInterpreter {
    let mut resolver = MemoryModuleResolver::new();
    let value = parse_phs("3").unwrap();
    let physure_script::Statement::Expr(three) = value.statements[0].clone() else { unreachable!() };
    let symbols = vec![(name.to_string(), three.clone()), ("fine".to_string(), three)];
    resolver.add_module("consts".to_string(), ModuleExport { symbols, functions: HashMap::new() });
    PhsInterpreter::new(Arc::new(resolver))
}

#[test]
fn an_imported_module_that_defines_a_constant_is_an_error() {
    let _lock = lock();
    for name in ["pi", "speed_of_light_in_vacuum"] {
        for program in ["use * from consts", &format!("use {name} from consts"), &format!("use fine as {name} from consts")] {
            let mut interpreter = module_defining(name);
            let error = interpreter.eval_str(program).expect_err(program).to_string();
            assert!(error.contains(&format!("`{name}` is")) && error.contains("imported from `consts`"), "{program}: {error}");
        }
        // what the module defines under another name is imported as before
        let mut interpreter = module_defining(name);
        interpreter.eval_str("use fine from consts").unwrap();
        assert!(interpreter.get_var("fine").is_some());
    }
}

#[test]
fn a_value_given_by_the_host_cannot_be_named_like_a_constant() {
    let _lock = lock();
    let interpreter = PhsInterpreter::default();
    let program = parse_phs("x * 2").unwrap();
    for name in ["pi", "π", "planck_constant"] {
        let bindings = vec![(name.to_string(), PhsValue::Number(3.0)), ("x".to_string(), PhsValue::Number(1.0))];
        let error = interpreter.run_program_with(&program, bindings).expect_err(name).to_string();
        assert!(error.contains(&format!("`{name}` is")) && error.contains("given by the host"), "{error}");
    }
    let fine = interpreter.run_program_with(&program, vec![("x".into(), PhsValue::Number(1.5))]).unwrap();
    assert_eq!(fine.to_string(), "3.0", "{fine:?}");
}

// ---- integral and limit resolve through the same order

#[test]
fn integral_and_limit_read_pi_and_the_constants_unquoted() {
    let _lock = lock();
    assert!(close(number("use * from calc\nintegral(\"sin(x)\", \"x\", 0, pi)"), 2.0));
    assert!(close(number("use * from calc\nintegral(\"cos(x)\", \"x\", 0, pi/2)"), 1.0));
    assert!(number("use * from calc\nlimit(\"sin(x)/x\", \"x\", pi)").abs() < 1e-15);
    // a constant in the integrand is the catalog's (before: the unit `bohr_radius` came first there)
    let q = quantity("use * from calc\nintegral(\"bohr_radius\", \"x\", 0, 2)");
    assert_eq!(q.unit, catalog("bohr_radius").unit);
    assert!(close(q.canonical_magnitude(), 2.0 * catalog("bohr_radius").canonical_magnitude()));
    // a binding of the integrand or the variable named like a constant is refused there too
    for program in [
        "use * from calc\nintegral(\"sin(pi)\", \"pi\", 0, 1)",
        "use * from calc\nlimit(\"x * a where a = 2, pi = 1\", \"x\", 1)",
        "use * from calc\nintegral(\"x * a where pi = 3, a = 2\", \"x\", 0, 1)",
    ] {
        let message = refused(program);
        assert!(message.contains("`pi` is the constant"), "{program:?}: {message}");
    }
    // a variable of the program still wins over a unit of the same name
    assert!(close(number("use * from calc\nm = 2\nintegral(\"m * x\", \"x\", 0, 1)"), 1.0));
}

// ---- the catalog in force decides

fn registry_with(conf: &str) -> UnitRegistry {
    let (mut registry, mut constants) = build_registry_from_conf();
    extend_registry(&mut registry, &mut constants, conf).unwrap_or_else(|problems| panic!("{problems:?}"));
    registry
}

#[test]
fn a_constant_read_in_a_loop_long_enough_to_run_in_parallel_is_the_one_of_the_scoped_catalog() {
    let _lock = lock();
    let custom = registry_with("[Constants]\nmolar_gas_constant = 8.0 J*mol^-1*K^-1\nzz_scoped_constant = 3 m\n");
    let threshold = physure_core::settings::parallel_threshold();
    for n in [3, threshold, threshold + 50] {
        let source = format!("for i in 0..{n} {{ molar_gas_constant * (1 mol*K) + zz_scoped_constant * (0 J/m) }}");
        let values = with_catalog(&custom, || match last(&source) {
            PhsValue::Vector(items) => items,
            other => panic!("{other:?}"),
        });
        assert_eq!(values.len(), n);
        for value in values {
            let PhsValue::Quantity(q) = value else { panic!("{value:?}") };
            assert_eq!(q.to("J").unwrap().value.mean(), 8.0, "{n} items");
        }
    }
    // a scope that names the constant makes binding it an error inside the scope only
    assert!(with_catalog(&custom, || eval_phs("zz_scoped_constant = 1")).is_err());
    eval_phs("zz_scoped_constant = 1").unwrap();
}

#[test]
fn an_override_in_a_physure_conf_or_a_frozen_file_changes_the_constant() {
    let _lock = lock();
    let dir = Dir::new("override");
    let r = || in_unit("molar_gas_constant", "J/(mol*K)");
    assert_eq!(r(), 8.31446261815324);
    dir.write("physure.conf", "[Constants]\nmolar_gas_constant = 8.0 J*mol^-1*K^-1\nzz_project_constant = 2 m\n");
    assert_eq!(r(), 8.0, "the working directory's physure.conf");
    assert_eq!(in_unit("3 * zz_project_constant", "m"), 6.0);
    // a name a conf adds is a constant, and binding it an error, under that conf only
    assert!(refused("zz_project_constant = 1").contains("`zz_project_constant` is a constant of the catalog in force"));
    assert!(check_source("zz_project_constant = 1").unwrap().iter().any(|d| d.code == "constant-shadowing"));
    dir.remove("physure.conf");
    assert_eq!(r(), 8.31446261815324);
    assert_eq!(number("zz_project_constant = 1\nzz_project_constant"), 1.0);

    dir.write(FROZEN_FILE_NAME, &frozen("[Constants]\nmolar_gas_constant = 7.5 J*mol^-1*K^-1\n"));
    assert_eq!(r(), 7.5, "the frozen file's");
    assert!(close(in_unit("molar_gas_constant * (2 mol*K)", "J"), 15.0));
    dir.remove(FROZEN_FILE_NAME);
    assert_eq!(r(), 8.31446261815324);
}

#[test]
fn a_constant_written_with_an_uncertainty_is_refused_not_read_without_it() {
    let _lock = lock();
    let dir = Dir::new("uncertain");
    // the lenient reading of a physure.conf keeps the line; reading it as a constant refuses it
    dir.write("physure.conf", "[Constants]\nzz_uncertain = 1.5 +/- 0.1 m\n");
    let message = refused("2 * zz_uncertain");
    assert!(message.contains("zz_uncertain") && message.contains("uncertainty"), "{message}");
    // a frozen file refuses the line itself
    dir.remove("physure.conf");
    dir.write(FROZEN_FILE_NAME, &frozen("[Constants]\nzz_uncertain = 1.5 +/- 0.1 m\n"));
    let message = refused("1 m");
    assert!(message.contains("uncertainty"), "{message}");
    dir.remove(FROZEN_FILE_NAME);
}

#[test]
fn a_conf_that_gives_a_unit_and_its_constant_two_values_keeps_the_unit() {
    let _lock = lock();
    let dir = Dir::new("clash");
    // the lenient reading drops the constant (and says so on stderr): both spellings read the unit
    dir.write("physure.conf", "[Constants]\nbohr_radius = 6e-11 m\n");
    let literal = quantity("2 bohr_radius").canonical_magnitude();
    let product = quantity("2 * bohr_radius").canonical_magnitude();
    assert!(close(product, literal) && close(literal, 2.0 * 5.29177210544e-11), "{literal} {product}");
    dir.remove("physure.conf");
}

// ---- generated code: a constant is fixed at generation

fn generated(source: &str, target: physure_script::Target) -> String {
    physure_script::transpile(&parse_phs(source).unwrap(), target.clone()).unwrap_or_else(|e| panic!("{target:?}: {e}"))
}

#[test]
fn every_target_writes_the_value_of_a_constant_fixed_at_generation_and_says_so() {
    use physure_script::Target;
    let _lock = lock();
    let source = "rt = molar_gas_constant * 300 K\narea = pi * (2 m)^2";
    for (target, comment, constant, pi) in [
        (Target::Rust, "//", "Quantity::new(8.31446261815324, \"J*mol^-1*K^-1\")", "Quantity::new(3.141592653589793, \"\")"),
        (Target::Python, "#", "Q_(8.31446261815324, \"J*mol^-1*K^-1\")", "Q_(3.141592653589793, \"\")"),
        (Target::Java, "//", "Quantity.of(8.31446261815324, \"J*mol^-1*K^-1\")", "Quantity.of(3.141592653589793)"),
        (Target::JavaScript, "//", "Quantity.of(8.31446261815324, \"J*mol^-1*K^-1\")", "Quantity.of(3.141592653589793)"),
        (Target::TypeScript, "//", "Quantity.of(8.31446261815324, \"J*mol^-1*K^-1\")", "Quantity.of(3.141592653589793)"),
    ] {
        let code = generated(source, target.clone());
        assert!(code.contains(constant), "{target:?}:\n{code}");
        assert!(code.contains(pi), "{target:?}:\n{code}");
        assert!(code.starts_with(&format!("{comment} Constants of the catalog, fixed when this file was generated")), "{target:?}:\n{code}");
        assert!(code.contains(&format!("{comment}   molar_gas_constant = 8.31446261815324 J*mol^-1*K^-1\n")), "{target:?}:\n{code}");
        // the name itself appears only in the comment: no free identifier is left for the target to resolve
        let body: Vec<&str> = code.lines().filter(|line| !line.trim_start().starts_with(comment)).collect();
        assert!(body.iter().all(|line| !line.contains("molar_gas_constant") && !line.contains("pi *")), "{target:?}:\n{code}");
        // pi alone is mathematics, not the catalog: no comment
        let only_pi = generated("x = 2 * pi", target.clone());
        assert!(!only_pi.contains("fixed when this file was generated"), "{target:?}:\n{only_pi}");
        // a program that binds a constant is refused, with the interpreter's reason
        let error = physure_script::transpile(&parse_phs("pi = 3").unwrap(), target.clone()).unwrap_err().to_string();
        assert!(error.contains("`pi` is the constant"), "{target:?}: {error}");
    }
}

#[test]
fn generated_code_takes_the_value_of_the_catalog_in_force_when_it_is_generated() {
    use physure_script::Target;
    let _lock = lock();
    let custom = registry_with("[Constants]\nmolar_gas_constant = 8.0 J*mol^-1*K^-1\n");
    let code = with_catalog(&custom, || generated("molar_gas_constant * 1 K", Target::Rust));
    assert!(code.contains("Quantity::new(8.0, \"J*mol^-1*K^-1\")") && code.contains("molar_gas_constant = 8.0 J*mol^-1*K^-1"), "{code}");
}

#[test]
fn an_equation_with_a_constant_compiles_to_a_function_of_its_arguments_only() {
    use physure_script::Target;
    let _lock = lock();
    let code = generated("eq = \"E = m * speed_of_light_in_vacuum^2\"\nx = eq(m=1 kg)", Target::Rust);
    assert!(code.contains("fn eq(m: Quantity)"), "{code}");
    assert!(code.contains("Quantity::new(299792458.0, \"m*s^-1\")") || code.contains("299792458"), "{code}");
    assert!(!code.lines().filter(|l| !l.starts_with("//")).any(|l| l.contains("speed_of_light_in_vacuum")), "{code}");
}
