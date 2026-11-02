use jni::JNIEnv;
use jni::objects::{JClass, JDoubleArray, JLongArray, JObject, JString, JValue};
use jni::sys::{jdouble, jlong, jobject, jstring};
use physure_core::error::{PhysureError, PhysureResult};
use physure_core::{UnitRegistry, Quantity};
use physure_script::{parse_phs, PhsInterpreter};
use std::collections::HashMap;
use std::sync::OnceLock;

// Helper to throw a Java PhysureException
fn throw_physure_exception(env: &mut JNIEnv, msg: &str) {
    let _ = env.throw_new("com/physure/PhysureException", msg);
}

/// What a Java `UnitRegistry` owns, and what its `long` handle points to: the unit catalog it was built
/// from (the embedded one, plus the custom conf it was given) and the PHS interpreter `evaluate` runs on.
struct RegistryHandle {
    registry: UnitRegistry,
    interpreter: PhsInterpreter,
}

/// The embedded catalog and its constants, parsed once per process: parsing it is the expensive part of
/// a call, and a registry of its own only needs a copy.
fn default_catalog() -> &'static (UnitRegistry, HashMap<String, String>) {
    static DEFAULT: OnceLock<(UnitRegistry, HashMap<String, String>)> = OnceLock::new();
    DEFAULT.get_or_init(|| {
        let mut reg = UnitRegistry::new();
        let mut constants = HashMap::new();
        physure_core::units::conf::parse_physure_conf(
            physure_core::units::conf::DEFAULT_PHYSURE_CONF,
            &mut reg,
            &mut constants,
        );
        (reg, constants)
    })
}

/// A new registry over the embedded catalog extended by `custom` conf text, as the raw handle Java keeps.
/// The text is read strictly: a line that cannot be read is an error naming it, and not a definition quietly
/// left out (and the `[Settings]` of a conf, which are process-wide, are not the registry's to change).
fn new_handle(custom: Option<&str>) -> Result<jlong, String> {
    let (mut registry, mut constants) = default_catalog().clone();
    if let Some(text) = custom {
        physure_core::units::conf::extend_registry(&mut registry, &mut constants, text)
            .map_err(|problems| format!("Invalid unit definitions: {}", problems.join("; ")))?;
    }
    let handle = RegistryHandle { registry, interpreter: new_interpreter() };
    Ok(Box::into_raw(Box::new(handle)) as jlong)
}

/// An interpreter with the calculus builtins (`solve`, `deriv`, `integral`) unlocked, as the WASM binding's
/// is: `UnitRegistry.solve`, `deriv` and `integral` call them, and without this every one failed with
/// "Undefined function".
fn new_interpreter() -> PhsInterpreter {
    let mut interpreter = PhsInterpreter::default();
    if let Ok(program) = parse_phs("use * from calc;") {
        for statement in &program.statements {
            let _ = interpreter.run_statement(statement);
        }
    }
    interpreter
}

fn get_interpreter(handle: jlong) -> &'static mut PhsInterpreter {
    unsafe { &mut (*(handle as *mut RegistryHandle)).interpreter }
}

/// The registry of `handle`; 0 (the static quantity operations, which carry no registry) is the embedded catalog.
fn get_registry(handle: jlong) -> &'static UnitRegistry {
    if handle == 0 {
        &default_catalog().0
    } else {
        unsafe { &(*(handle as *const RegistryHandle)).registry }
    }
}

/// The sources of the uncertainty that a Java `Quantity` holds: `(id, contribution)` pairs, which the core
/// merges as it does for any quantity. Java mints the one id of a quantity it constructed on its first use.
fn read_sources(env: &mut JNIEnv, java_q: &JObject) -> Result<Vec<(u32, f64)>, jni::errors::Error> {
    let ids = JLongArray::from(env.call_method(java_q, "sourceIds", "()[J", &[])?.l()?);
    let coefficients = JDoubleArray::from(env.call_method(java_q, "sourceCoefficients", "()[D", &[])?.l()?);
    let mut raw_ids = vec![0; env.get_array_length(&ids)? as usize];
    let mut raw_coefficients = vec![0.0; env.get_array_length(&coefficients)? as usize];
    env.get_long_array_region(&ids, 0, &mut raw_ids)?;
    env.get_double_array_region(&coefficients, 0, &mut raw_coefficients)?;
    Ok(raw_ids.into_iter().zip(raw_coefficients).map(|(id, c)| (id as u32, c)).collect())
}

fn get_rust_quantity(
    env: &mut JNIEnv,
    java_q: &JObject,
) -> Result<Quantity, jni::errors::Error> {
    let value: f64 = env.get_field(java_q, "value", "D")?.d()?;
    let unit_jstr: JString = env.get_field(java_q, "unit", "Ljava/lang/String;")?.l()?.into();
    let unit_str: String = env.get_string(&unit_jstr)?.into();
    
    let reg = get_registry(0);
    let _r_unit = match physure_core::units::parser::Parser::parse_expression_with_registry(&unit_str, &reg) {
        Ok(u) => u,
        Err(e) => {
            let _ = env.throw_new("com/physure/PhysureException", &format!("Quantity unit parse error for '{}': {}", unit_str, e));
            return Err(jni::errors::Error::JavaException);
        }
    };
    
    // The uncertainty goes in as the sources the Java quantity holds, not as a number: a number would make a
    // new measurement of an old one, and `x - x` would no longer be zero.
    let sources = read_sources(env, java_q)?;
    let unit = Quantity::new(value, &unit_str)
        .map_err(|e| {
            let _ = env.throw_new("com/physure/PhysureException", &format!("{}", e));
            jni::errors::Error::JavaException
        })?
        .unit;
    Ok(Quantity::from_sources(value, sources, unit))
}

/// A Java `Quantity` from its numbers, its unit and its sources (`None` when the core's model keeps none, and
/// Java then mints one on the first use).
fn java_quantity<'local>(
    env: &mut JNIEnv<'local>,
    value: f64,
    uncertainty: f64,
    unit: &str,
    sources: Option<Vec<(u32, f64)>>,
) -> Result<JObject<'local>, jni::errors::Error> {
    let q_class = env.find_class("com/physure/Quantity")?;
    let unit_jstr = env.new_string(unit)?;
    let (ids, coefficients) = match sources {
        Some(terms) => {
            let ids = env.new_long_array(terms.len() as i32)?;
            let coefficients = env.new_double_array(terms.len() as i32)?;
            let raw_ids: Vec<jlong> = terms.iter().map(|&(id, _)| jlong::from(id)).collect();
            let raw_coefficients: Vec<f64> = terms.iter().map(|&(_, c)| c).collect();
            env.set_long_array_region(&ids, 0, &raw_ids)?;
            env.set_double_array_region(&coefficients, 0, &raw_coefficients)?;
            (JObject::from(ids), JObject::from(coefficients))
        }
        None => (JObject::null(), JObject::null()),
    };
    env.new_object(
        q_class,
        "(DDLjava/lang/String;[J[D)V",
        &[
            JValue::Double(value),
            JValue::Double(uncertainty),
            JValue::Object(&unit_jstr),
            JValue::Object(&ids),
            JValue::Object(&coefficients),
        ],
    )
}

fn make_java_quantity<'local>(
    env: &mut JNIEnv<'local>,
    q: Quantity,
) -> Result<JObject<'local>, jni::errors::Error> {
    // the magnitude, uncertainty, unit and sources as one consistent set (`2 m * 3 km` is 6000 m^2, not 6 m^2)
    let (value, uncertainty, unit) = q.display_parts();
    java_quantity(env, value, uncertainty, &unit, q.display_sources())
}

/// What the core says of the quantity that Java holds. Its refusal is a `PhysureException`, and `default` is
/// only returned with that exception pending.
fn ask<T>(env: &mut JNIEnv, q_obj: &JObject, default: T, f: impl FnOnce(Quantity) -> PhysureResult<T>) -> T {
    let Ok(q) = get_rust_quantity(env, q_obj) else { return default };
    f(q).unwrap_or_else(|e| {
        throw_physure_exception(env, &e.to_string());
        default
    })
}

/// Declares `nu` degrees of freedom for the one source of the quantity (JCGM 100:2008 4.3.2). The id is the one
/// the Java object holds, so every copy of the measurement shares them.
#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_declareDof<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q_obj: JObject<'local>,
    nu: jdouble,
) {
    ask(&mut env, &q_obj, (), |q| q.with_dof(nu).map(|_| ()))
}

/// The effective degrees of freedom of the quantity by Welch-Satterthwaite (JCGM 100:2008 G.4.1).
#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_effectiveDof<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q_obj: JObject<'local>,
) -> jdouble {
    ask(&mut env, &q_obj, 0.0, |q| q.effective_dof())
}

/// `{k, U, nu_eff}` of the quantity for the coverage probability `p`: the coverage factor, the expanded
/// uncertainty (in the unit of the quantity) and the effective degrees of freedom they came from.
#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_expanded<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q_obj: JObject<'local>,
    p: jdouble,
) -> jobject {
    let Some(parts) = ask(&mut env, &q_obj, None, |q| {
        q.expanded(p).map(|e| Some([e.coverage_factor, e.expanded_uncertainty, e.dof_eff]))
    }) else {
        return std::ptr::null_mut();
    };
    match env.new_double_array(parts.len() as i32) {
        Ok(array) if env.set_double_array_region(&array, 0, &parts).is_ok() => array.into_raw(),
        _ => std::ptr::null_mut(),
    }
}

/// A Type A evaluation (JCGM 100:2008 4.2): the mean of the observations with its standard error and `n - 1`
/// degrees of freedom.
#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_fromSamples<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    samples: JDoubleArray<'local>,
    unit: JString<'local>,
) -> jobject {
    let Ok(count) = env.get_array_length(&samples) else { return std::ptr::null_mut() };
    let mut values = vec![0.0; count as usize];
    if env.get_double_array_region(&samples, 0, &mut values).is_err() {
        return std::ptr::null_mut();
    }
    let unit: String = match env.get_string(&unit) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    answer(&mut env, Quantity::from_samples(&values, &unit))
}

/// `{mean, uncertainty}` as text, rounded by the Guide's rule (JCGM 100:2008 7.2.6), which is the core's.
#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_gumRound<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    mean: jdouble,
    uncertainty: jdouble,
) -> jobject {
    let (value, spread) = physure_core::quantity::gum_round(mean, uncertainty, None);
    let Ok(string_class) = env.find_class("java/lang/String") else { return std::ptr::null_mut() };
    let Ok(array) = env.new_object_array(2, &string_class, JObject::null()) else { return std::ptr::null_mut() };
    for (index, text) in [value, spread].iter().enumerate() {
        let Ok(jtext) = env.new_string(text) else { return std::ptr::null_mut() };
        if env.set_object_array_element(&array, index as i32, &jtext).is_err() {
            return std::ptr::null_mut();
        }
    }
    array.into_raw()
}

/// The Java object for the core's answer, or the `PhysureException` for its refusal (a null return with the
/// exception pending).
fn answer(env: &mut JNIEnv, result: PhysureResult<Quantity>) -> jobject {
    match result {
        Ok(res) => match make_java_quantity(env, res) {
            Ok(j_q) => j_q.into_raw(),
            Err(e) => {
                throw_physure_exception(env, &format!("Failed to build Java Quantity: {}", e));
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            throw_physure_exception(env, &format!("{}", e));
            std::ptr::null_mut()
        }
    }
}

/// A source id from the core's one counter, for the first use of a quantity that Java constructed.
#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_newSourceId(_env: JNIEnv, _class: JClass) -> jlong {
    jlong::from(physure_core::uncertainty::new_source_id())
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_initRegistry(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    new_handle(None).unwrap_or(0)
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_initRegistryFromPath<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    path: JString<'local>,
) -> jlong {
    let path_str: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Invalid file path string: {}", e));
            return 0;
        }
    };

    match std::fs::read_to_string(&path_str) {
        Ok(content) => match new_handle(Some(&content)) {
            Ok(handle) => handle,
            Err(message) => {
                throw_physure_exception(&mut env, &format!("{} (in '{}')", message, path_str));
                0
            }
        },
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Failed to read physure.conf at '{}': {}", path_str, e));
            0
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_initRegistryFromContent<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    content: JString<'local>,
) -> jlong {
    let content_str: String = match env.get_string(&content) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Invalid config content string: {}", e));
            return 0;
        }
    };

    match new_handle(Some(&content_str)) {
        Ok(handle) => handle,
        Err(message) => {
            throw_physure_exception(&mut env, &message);
            0
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_destroyRegistry(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        unsafe {
            let _ = Box::from_raw(handle as *mut RegistryHandle);
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_getUnitExponents<'local>(
    env: JNIEnv<'local>,
    _class: JClass,
    registry_handle: jlong,
    expr: JString<'local>,
) -> jobject {
    unit_exponent_map(env, registry_handle, expr, false)
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_getUnitPowers<'local>(
    env: JNIEnv<'local>,
    _class: JClass,
    registry_handle: jlong,
    expr: JString<'local>,
) -> jobject {
    unit_exponent_map(env, registry_handle, expr, true)
}

/// The base-unit exponents of `expr` as a `HashMap`: `Integer` values when `exact` is false (a fractional
/// power is an error, because truncating it to its numerator would be a wrong answer), `Double` values
/// (numerator over denominator, as the WASM binding returns them) when it is true.
fn unit_exponent_map<'local>(mut env: JNIEnv<'local>, registry_handle: jlong, expr: JString<'local>, exact: bool) -> jobject {
    let reg = get_registry(registry_handle);
    match exponent_map(&mut env, reg, &expr, exact) {
        Ok(map) => map.into_raw(),
        Err(failure) => {
            // a failure with a message is a PhysureException; one without is a JNI call that already left its own
            if let Some(message) = failure {
                throw_physure_exception(&mut env, &message);
            }
            std::ptr::null_mut()
        }
    }
}

/// The map `unit_exponent_map` returns, or why there is none (`Some(message)` to throw, `None` to just return null).
fn exponent_map<'local>(
    env: &mut JNIEnv<'local>,
    reg: &UnitRegistry,
    expr: &JString<'local>,
    exact: bool,
) -> Result<JObject<'local>, Option<String>> {
    let expr_str: String = env.get_string(expr).map_err(|e| Some(format!("Invalid JNI string: {}", e)))?.into();
    let unit = physure_core::units::parser::Parser::parse_expression_with_registry(&expr_str, reg)
        .map_err(|e| Some(format!("Unit parse error for '{}': {}", expr_str, e)))?;
    let map_class = env.find_class("java/util/HashMap").map_err(|_| None)?;
    let map_obj = env.new_object(map_class, "()V", &[]).map_err(|_| None)?;

    if !exact {
        if let Some((symbol, (num, den))) = unit.dimensions.iter().find(|(_, (_, den))| *den != 1) {
            return Err(Some(format!(
                "Unit '{}' has the fractional exponent {}/{} on '{}': getUnitExponents returns whole numbers, use getUnitPowers",
                expr_str, num, den, symbol
            )));
        }
    }

    let value_class = env
        .find_class(if exact { "java/lang/Double" } else { "java/lang/Integer" })
        .map_err(|_| None)?;
    for (symbol, (num, den)) in &unit.dimensions {
        // an entry the JVM cannot build is left out
        let _ = put_exponent(env, &map_obj, &value_class, symbol, (*num, *den), exact);
    }
    Ok(map_obj)
}

/// Puts `symbol -> exponent` in the Java map: a `Double` (numerator over denominator) when `exact`, an `Integer` if not.
fn put_exponent<'local>(
    env: &mut JNIEnv<'local>,
    map: &JObject<'local>,
    value_class: &JClass<'local>,
    symbol: &str,
    (num, den): (i64, i64),
    exact: bool,
) -> Result<(), jni::errors::Error> {
    let key = env.new_string(symbol)?;
    let value = if exact {
        env.new_object(value_class, "(D)V", &[JValue::Double(num as f64 / den as f64)])?
    } else {
        env.new_object(value_class, "(I)V", &[JValue::Int(num as i32)])?
    };
    env.call_method(
        map,
        "put",
        "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
        &[JValue::Object(&key), JValue::Object(&value)],
    )?;
    Ok(())
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_getUnitScale(
    mut env: JNIEnv,
    _class: JClass,
    registry_handle: jlong,
    expr: JString,
) -> jdouble {
    let reg = get_registry(registry_handle);
    let expr_str: String = match env.get_string(&expr) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Invalid JNI string: {}", e));
            return 0.0;
        }
    };

    match physure_core::units::parser::Parser::parse_expression_with_registry(&expr_str, &reg) {
        Ok(u) => u.scale as jdouble,
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Unit parse error for '{}': {}", expr_str, e));
            0.0
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_getCategories<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    registry_handle: jlong,
) -> jobject {
    let reg = get_registry(registry_handle);

    let map_class = match env.find_class("java/util/HashMap") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    let map_obj = match env.new_object(map_class, "()V", &[]) {
        Ok(o) => o,
        Err(_) => return std::ptr::null_mut(),
    };

    let string_class = match env.find_class("java/lang/String") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    for (cat_name, list) in &reg.categories {
        let key_jstr = match env.new_string(cat_name) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let array_len = list.len() as i32;
        let array_obj = match env.new_object_array(array_len, &string_class, env.new_string("").unwrap()) {
            Ok(a) => a,
            Err(_) => continue,
        };

        for (idx, item) in list.iter().enumerate() {
            let item_jstr = match env.new_string(item) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let _ = env.set_object_array_element(&array_obj, idx as i32, &item_jstr);
        }

        let _ = env.call_method(
            &map_obj,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[JValue::Object(&key_jstr), JValue::Object(&array_obj)],
        );
    }

    map_obj.into_raw()
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_evaluateExpression<'local>(
    env: JNIEnv<'local>,
    class: JClass,
    interpreter_handle: jlong,
    expr: JString<'local>,
) -> jstring {
    // the registry's own catalog is what the parser and the interpreter read unit text against while it runs
    physure_core::units::with_catalog(get_registry(interpreter_handle), || {
        evaluate_expression(env, class, interpreter_handle, expr)
    })
}

fn evaluate_expression<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    interpreter_handle: jlong,
    expr: JString<'local>,
) -> jstring {
    let interpreter = get_interpreter(interpreter_handle);
    match evaluated_text(&mut env, interpreter, &expr) {
        Ok(text) => env.new_string(text).map_or(std::ptr::null_mut(), |s| s.into_raw()),
        Err(message) => {
            throw_physure_exception(&mut env, &message);
            std::ptr::null_mut()
        }
    }
}

/// The PHS source Java passed, or the message of the exception to throw.
fn source_text(env: &mut JNIEnv, expr: &JString) -> Result<String, String> {
    env.get_string(expr).map(String::from).map_err(|e| format!("Invalid JNI string: {}", e))
}

/// Runs the source and returns the value of each statement, one per line.
fn evaluated_text(env: &mut JNIEnv, interpreter: &mut PhsInterpreter, expr: &JString) -> Result<String, String> {
    let source = source_text(env, expr)?;
    let program = physure_script::parse_phs(&source).map_err(|e| format!("Syntax error: {}", e))?;
    let mut lines = Vec::new();
    for stmt in &program.statements {
        let value = interpreter.run_statement(stmt).map_err(|e| e.to_string())?;
        lines.push(match value {
            physure_script::PhsValue::None => assigned_value(interpreter, stmt).map_or("None".to_string(), |v| v.to_string()),
            other => other.to_string(),
        });
    }
    Ok(lines.join("\n"))
}

/// What a statement that produced no value left behind: the variable an assignment set.
fn assigned_value(interpreter: &PhsInterpreter, stmt: &physure_script::Statement) -> Option<physure_script::PhsValue> {
    match stmt {
        physure_script::Statement::Assignment(node) => interpreter.get_var(&node.name).map(|v| v.clone()),
        _ => None,
    }
}

/// Evaluates PHS and returns the value of its last statement as a `Quantity`, exactly: the text that
/// `evaluateExpression` returns is rounded for display (an uncertainty of 0.2236 reads "0.2"), and a
/// quantity rebuilt from it would carry the rounding.
#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_evaluateQuantity<'local>(
    env: JNIEnv<'local>,
    class: JClass,
    interpreter_handle: jlong,
    expr: JString<'local>,
) -> jobject {
    // the registry's own catalog is what the parser and the interpreter read unit text against while it runs
    physure_core::units::with_catalog(get_registry(interpreter_handle), || {
        evaluate_quantity(env, class, interpreter_handle, expr)
    })
}

fn evaluate_quantity<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    interpreter_handle: jlong,
    expr: JString<'local>,
) -> jobject {
    let interpreter = get_interpreter(interpreter_handle);
    let built = last_quantity(&mut env, interpreter, &expr)
        .and_then(|q| make_java_quantity(&mut env, q).map_err(|e| format!("Failed to build Java Quantity: {}", e)));
    match built {
        Ok(j_q) => j_q.into_raw(),
        Err(message) => {
            throw_physure_exception(&mut env, &message);
            std::ptr::null_mut()
        }
    }
}

/// Runs the source and returns its last value as a quantity, or the message of the exception to throw.
fn last_quantity(env: &mut JNIEnv, interpreter: &mut PhsInterpreter, expr: &JString) -> Result<Quantity, String> {
    let source = source_text(env, expr)?;
    let program = physure_script::parse_phs(&source).map_err(|e| format!("Syntax error: {}", e))?;
    let mut last = physure_script::PhsValue::None;
    for stmt in &program.statements {
        last = match interpreter.run_statement(stmt).map_err(|e| e.to_string())? {
            // an assignment's value is the variable it set
            physure_script::PhsValue::None => assigned_value(interpreter, stmt).unwrap_or(physure_script::PhsValue::None),
            value => value,
        };
    }
    as_quantity(last)
}

fn as_quantity(value: physure_script::PhsValue) -> Result<Quantity, String> {
    match value {
        physure_script::PhsValue::Quantity(q) => Ok(q),
        physure_script::PhsValue::Number(n) => Quantity::new(n, "").map_err(|e| e.to_string()),
        physure_script::PhsValue::Bool(b) => Quantity::new(if b { 1.0 } else { 0.0 }, "").map_err(|e| e.to_string()),
        physure_script::PhsValue::None => Err("The expression produced no value".to_string()),
        other => Err(format!("The expression did not produce a quantity: {}", other)),
    }
}

// --- Quantity Math Helpers ---

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_addQuantities<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q1_obj: JObject<'local>,
    q2_obj: JObject<'local>,
) -> jobject {
    let q1 = match get_rust_quantity(&mut env, &q1_obj) {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };
    let q2 = match get_rust_quantity(&mut env, &q2_obj) {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };

    answer(&mut env, q1.add(&q2))
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_subQuantities<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q1_obj: JObject<'local>,
    q2_obj: JObject<'local>,
) -> jobject {
    let q1 = match get_rust_quantity(&mut env, &q1_obj) {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };
    let q2 = match get_rust_quantity(&mut env, &q2_obj) {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };

    answer(&mut env, q1.sub(&q2))
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_mulQuantities<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q1_obj: JObject<'local>,
    q2_obj: JObject<'local>,
) -> jobject {
    let q1 = match get_rust_quantity(&mut env, &q1_obj) {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };
    let q2 = match get_rust_quantity(&mut env, &q2_obj) {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };

    answer(&mut env, q1.mul(&q2))
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_divQuantities<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q1_obj: JObject<'local>,
    q2_obj: JObject<'local>,
) -> jobject {
    let q1 = match get_rust_quantity(&mut env, &q1_obj) {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };
    let q2 = match get_rust_quantity(&mut env, &q2_obj) {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };

    answer(&mut env, q1.div(&q2))
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_powQuantity<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q_obj: JObject<'local>,
    power: jdouble,
) -> jobject {
    let q = match get_rust_quantity(&mut env, &q_obj) {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };

    answer(&mut env, q.pow(power))
}

/// The function of a quantity that Java asked for, by name: the core's own `sin`, `cos`, `tan` and `abs`, which
/// read the angle unit, refuse what is not an angle and carry the sources through the derivative.
#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_mathFunction<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q_obj: JObject<'local>,
    name: JString<'local>,
) -> jobject {
    let q = match get_rust_quantity(&mut env, &q_obj) {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };
    let name: String = match env.get_string(&name) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    answer(
        &mut env,
        match name.as_str() {
            "sin" => q.sin(),
            "cos" => q.cos(),
            "tan" => q.tan(),
            "abs" => q.abs(),
            other => Err(PhysureError::Generic(format!("'{}' is not a function of a quantity", other))),
        },
    )
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_convertQuantity<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q_obj: JObject<'local>,
    target_unit: JString<'local>,
) -> jobject {
    let q = match get_rust_quantity(&mut env, &q_obj) {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };
    let unit_str: String = match env.get_string(&target_unit) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let reg = get_registry(0);
    let r_unit = match physure_core::units::parser::Parser::parse_expression_with_registry(&unit_str, &reg) {
        Ok(u) => u,
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Target unit parse error: {}", e));
            return std::ptr::null_mut();
        }
    };

    match q.convert_to(&r_unit) {
        Ok(res) => {
            // `res` is already in the target unit: its value, its spread and its sources are what to hand back
            // (the canonical magnitude carries the scale and the zero point of the unit). A conversion is the
            // same measurement in another unit, so it keeps the sources and mints none.
            match java_quantity(&mut env, res.value.mean(), res.value.std_dev(), &unit_str, res.sources()) {
                Ok(j_q) => j_q.into_raw(),
                Err(e) => {
                    throw_physure_exception(&mut env, &format!("Failed to build Java Quantity: {}", e));
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            throw_physure_exception(&mut env, &format!("{}", e));
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_assertQuantities<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q1_obj: JObject<'local>,
    q2_obj: JObject<'local>,
) {
    let q1 = match get_rust_quantity(&mut env, &q1_obj) {
        Ok(q) => q,
        Err(_) => return,
    };
    let q2 = match get_rust_quantity(&mut env, &q2_obj) {
        Ok(q) => q,
        Err(_) => return,
    };
    if let Err(e) = q1.phs_assert(&q2) {
        throw_physure_exception(&mut env, &format!("{}", e));
    }
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_assertExactQuantities<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    q1_obj: JObject<'local>,
    q2_obj: JObject<'local>,
) {
    let q1 = match get_rust_quantity(&mut env, &q1_obj) {
        Ok(q) => q,
        Err(_) => return,
    };
    let q2 = match get_rust_quantity(&mut env, &q2_obj) {
        Ok(q) => q,
        Err(_) => return,
    };
    if let Err(e) = q1.phs_exact_assert(&q2) {
        throw_physure_exception(&mut env, &format!("{}", e));
    }
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_getFunctionParams<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    interpreter_handle: jlong,
    func_name: JString<'local>,
) -> jobject {
    let interpreter = get_interpreter(interpreter_handle);
    let name_str: String = match env.get_string(&func_name) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };

    let string_class = match env.find_class("java/lang/String") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    if let Some(params) = interpreter.get_fn_params(&name_str) {
        let array = match env.new_object_array(params.len() as i32, &string_class, env.new_string("").unwrap()) {
            Ok(a) => a,
            Err(_) => return std::ptr::null_mut(),
        };
        for (idx, param) in params.iter().enumerate() {
            let p_jstr = env.new_string(param).unwrap();
            let _ = env.set_object_array_element(&array, idx as i32, &p_jstr);
        }
        array.into_raw()
    } else {
        throw_physure_exception(&mut env, &format!("Function '{}' not found in interpreter context", name_str));
        std::ptr::null_mut()
    }
}
