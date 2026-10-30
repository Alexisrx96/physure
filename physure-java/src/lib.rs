use jni::JNIEnv;
use jni::objects::{JClass, JString, JObject, JValue};
use jni::sys::{jdouble, jlong, jobject, jstring};
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

fn get_rust_quantity(
    env: &mut JNIEnv,
    java_q: &JObject,
) -> Result<Quantity, jni::errors::Error> {
    let value: f64 = env.get_field(java_q, "value", "D")?.d()?;
    let uncertainty: f64 = env.get_field(java_q, "uncertainty", "D")?.d()?;
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
    
    // the uncertainty goes in with the value: dropping it here made every operation answer "exactly"
    let built = if uncertainty == 0.0 {
        Quantity::new(value, &unit_str)
    } else {
        Quantity::with_uncertainty(value, uncertainty, &unit_str)
    };
    built.map_err(|e| {
        let _ = env.throw_new("com/physure/PhysureException", &format!("{}", e));
        jni::errors::Error::JavaException
    })
}

fn make_java_quantity<'local>(
    env: &mut JNIEnv<'local>,
    q: Quantity,
) -> Result<JObject<'local>, jni::errors::Error> {
    let q_class = env.find_class("com/physure/Quantity")?;
    // the magnitude, uncertainty and unit as one consistent triple (`2 m * 3 km` is 6000 m^2, not 6 m^2)
    let (value, uncertainty, unit) = q.display_parts();
    let unit_jstr = env.new_string(unit)?;
    env.new_object(
        q_class,
        "(DDLjava/lang/String;)V",
        &[JValue::Double(value), JValue::Double(uncertainty), JValue::Object(&unit_jstr)],
    )
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
    let expr_str: String = match env.get_string(&expr) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Invalid JNI string: {}", e));
            return std::ptr::null_mut();
        }
    };

    let unit = match physure_core::units::parser::Parser::parse_expression_with_registry(&expr_str, &reg) {
        Ok(u) => u,
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Unit parse error for '{}': {}", expr_str, e));
            return std::ptr::null_mut();
        }
    };

    let map_class = match env.find_class("java/util/HashMap") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    let map_obj = match env.new_object(map_class, "()V", &[]) {
        Ok(o) => o,
        Err(_) => return std::ptr::null_mut(),
    };

    if !exact {
        if let Some((symbol, (num, den))) = unit.dimensions.iter().find(|(_, (_, den))| *den != 1) {
            throw_physure_exception(
                &mut env,
                &format!(
                    "Unit '{}' has the fractional exponent {}/{} on '{}': getUnitExponents returns whole numbers, use getUnitPowers",
                    expr_str, num, den, symbol
                ),
            );
            return std::ptr::null_mut();
        }
    }

    let value_class = match env.find_class(if exact { "java/lang/Double" } else { "java/lang/Integer" }) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    for (symbol, (num, den)) in &unit.dimensions {
        let key_jstr = match env.new_string(symbol) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let value = if exact {
            env.new_object(&value_class, "(D)V", &[JValue::Double(*num as f64 / *den as f64)])
        } else {
            env.new_object(&value_class, "(I)V", &[JValue::Int(*num as i32)])
        };
        let val_obj = match value {
            Ok(o) => o,
            Err(_) => continue,
        };

        let _ = env.call_method(
            &map_obj,
            "put",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;",
            &[JValue::Object(&key_jstr), JValue::Object(&val_obj)],
        );
    }

    map_obj.into_raw()
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
    mut env: JNIEnv<'local>,
    _class: JClass,
    interpreter_handle: jlong,
    expr: JString<'local>,
) -> jstring {
    let interpreter = get_interpreter(interpreter_handle);
    let expr_str: String = match env.get_string(&expr) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Invalid JNI string: {}", e));
            return std::ptr::null_mut();
        }
    };

    let statements = match physure_script::parse_phs(&expr_str) {
        Ok(s) => s,
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Syntax error: {}", e));
            return std::ptr::null_mut();
        }
    };

    let mut result_str = String::new();
    for (idx, stmt) in statements.statements.iter().enumerate() {
        if idx > 0 {
            result_str.push_str("\n");
        }
        match interpreter.run_statement(stmt) {
            Ok(val) => {
                match val {
                    physure_script::PhsValue::None => {
                        if let physure_script::Statement::Assignment(node) = stmt {
                            if let Some(v) = interpreter.get_var(&node.name) {
                                result_str.push_str(&v.to_string());
                            } else {
                                result_str.push_str("None");
                            }
                        } else {
                            result_str.push_str("None");
                        }
                    }
                    other => result_str.push_str(&other.to_string()),
                }
            }
            Err(e) => {
                throw_physure_exception(&mut env, &format!("{}", e));
                return std::ptr::null_mut();
            }
        }
    }

    match env.new_string(result_str) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Evaluates PHS and returns the value of its last statement as a `Quantity`, exactly: the text that
/// `evaluateExpression` returns is rounded for display (an uncertainty of 0.2236 reads "0.2"), and a
/// quantity rebuilt from it would carry the rounding.
#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_evaluateQuantity<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    interpreter_handle: jlong,
    expr: JString<'local>,
) -> jobject {
    let interpreter = get_interpreter(interpreter_handle);
    let expr_str: String = match env.get_string(&expr) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Invalid JNI string: {}", e));
            return std::ptr::null_mut();
        }
    };

    let program = match physure_script::parse_phs(&expr_str) {
        Ok(p) => p,
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Syntax error: {}", e));
            return std::ptr::null_mut();
        }
    };

    let mut last = physure_script::PhsValue::None;
    for stmt in &program.statements {
        match interpreter.run_statement(stmt) {
            Ok(physure_script::PhsValue::None) => {
                // an assignment's value is the variable it set
                last = match stmt {
                    physure_script::Statement::Assignment(node) => interpreter.get_var(&node.name).map(|v| v.clone()).unwrap_or(physure_script::PhsValue::None),
                    _ => physure_script::PhsValue::None,
                };
            }
            Ok(value) => last = value,
            Err(e) => {
                throw_physure_exception(&mut env, &format!("{}", e));
                return std::ptr::null_mut();
            }
        }
    }

    let quantity = match last {
        physure_script::PhsValue::Quantity(q) => q,
        physure_script::PhsValue::Number(n) => match Quantity::new(n, "") {
            Ok(q) => q,
            Err(e) => {
                throw_physure_exception(&mut env, &format!("{}", e));
                return std::ptr::null_mut();
            }
        },
        physure_script::PhsValue::Bool(b) => match Quantity::new(if b { 1.0 } else { 0.0 }, "") {
            Ok(q) => q,
            Err(e) => {
                throw_physure_exception(&mut env, &format!("{}", e));
                return std::ptr::null_mut();
            }
        },
        physure_script::PhsValue::None => {
            throw_physure_exception(&mut env, "The expression produced no value");
            return std::ptr::null_mut();
        }
        other => {
            throw_physure_exception(&mut env, &format!("The expression did not produce a quantity: {}", other));
            return std::ptr::null_mut();
        }
    };

    match make_java_quantity(&mut env, quantity) {
        Ok(j_q) => j_q.into_raw(),
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Failed to build Java Quantity: {}", e));
            std::ptr::null_mut()
        }
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

    match q1.add(&q2) {
        Ok(res) => {
            match make_java_quantity(&mut env, res) {
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

    match q1.sub(&q2) {
        Ok(res) => {
            match make_java_quantity(&mut env, res) {
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

    match q1.mul(&q2) {
        Ok(res) => {
            match make_java_quantity(&mut env, res) {
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

    match q1.div(&q2) {
        Ok(res) => {
            match make_java_quantity(&mut env, res) {
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

    match q.pow(power) {
        Ok(res) => {
            match make_java_quantity(&mut env, res) {
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
            let q_class = match env.find_class("com/physure/Quantity") {
                Ok(c) => c,
                Err(_) => return std::ptr::null_mut(),
            };
            let unit_jstr = match env.new_string(&unit_str) {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };
            // `res` is already in the target unit: its value and its spread are what to hand back
            // (the canonical magnitude carries the scale and the zero point of the unit).
            let converted_val = res.value.mean();
            let j_q = match env.new_object(
                q_class,
                "(DDLjava/lang/String;)V",
                &[
                    JValue::Double(converted_val),
                    JValue::Double(res.value.std_dev()),
                    JValue::Object(&unit_jstr),
                ],
            ) {
                Ok(o) => o,
                Err(e) => {
                    throw_physure_exception(&mut env, &format!("Failed to build Java Quantity: {}", e));
                    return std::ptr::null_mut();
                }
            };
            j_q.into_raw()
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
