use jni::JNIEnv;
use jni::objects::{JClass, JString, JObject, JValue};
use jni::sys::{jdouble, jlong, jobject, jstring};
use physure_core::{UnitRegistry, Quantity};
use physure_script::PhsInterpreter;

// Helper to throw a Java PhysureException
fn throw_physure_exception(env: &mut JNIEnv, msg: &str) {
    let _ = env.throw_new("com/physure/PhysureException", msg);
}

// Helper to convert dynamic units from interpreter
fn get_interpreter(handle: jlong) -> &'static mut PhsInterpreter {
    unsafe { &mut *(handle as *mut PhsInterpreter) }
}

fn get_registry(_handle: jlong) -> UnitRegistry {
    UnitRegistry::build_default_si()
}

fn get_rust_quantity(
    env: &mut JNIEnv,
    java_q: &JObject,
) -> Result<Quantity, jni::errors::Error> {
    let value: f64 = env.get_field(java_q, "value", "D")?.d()?;
    let unit_jstr: JString = env.get_field(java_q, "unit", "Ljava/lang/String;")?.l()?.into();
    let unit_str: String = env.get_string(&unit_jstr)?.into();
    
    let reg = UnitRegistry::build_default_si();
    let r_unit = match physure_core::units::parser::Parser::parse_expression_with_registry(&unit_str, &reg) {
        Ok(u) => u,
        Err(e) => {
            let _ = env.throw_new("com/physure/PhysureException", &format!("Quantity unit parse error for '{}': {}", unit_str, e));
            return Err(jni::errors::Error::JavaException);
        }
    };
    
    Quantity::new(value, &unit_str).map_err(|_| jni::errors::Error::JavaException)
}

fn make_java_quantity<'local>(
    env: &mut JNIEnv<'local>,
    q: Quantity,
) -> Result<JObject<'local>, jni::errors::Error> {
    let q_class = env.find_class("com/physure/Quantity")?;
    let unit_jstr = env.new_string(q.unit.__repr__())?;
    env.new_object(
        q_class,
        "(DLjava/lang/String;)V",
        &[JValue::Double(q.value.mean()), JValue::Object(&unit_jstr)],
    )
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_initRegistry(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    let interpreter = PhsInterpreter::default();
    let boxed = Box::new(interpreter);
    Box::into_raw(boxed) as jlong
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

    let mut reg = UnitRegistry::new();
    let mut constants = std::collections::HashMap::new();

    // 1. Load embedded default master
    physure_core::units::conf::parse_physure_conf(
        physure_core::units::conf::DEFAULT_PHYSURE_CONF,
        &mut reg,
        &mut constants,
    );

    // 2. Load custom override path
    match std::fs::read_to_string(&path_str) {
        Ok(content) => {
            physure_core::units::conf::parse_physure_conf(&content, &mut reg, &mut constants);
        }
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Failed to read physure.conf at '{}': {}", path_str, e));
            return 0;
        }
    }

    let interpreter = PhsInterpreter::default();
    let boxed = Box::new(interpreter);
    Box::into_raw(boxed) as jlong
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

    let mut reg = UnitRegistry::new();
    let mut constants = std::collections::HashMap::new();

    // 1. Load embedded default master
    physure_core::units::conf::parse_physure_conf(
        physure_core::units::conf::DEFAULT_PHYSURE_CONF,
        &mut reg,
        &mut constants,
    );

    // 2. Load custom override content
    physure_core::units::conf::parse_physure_conf(&content_str, &mut reg, &mut constants);

    let interpreter = PhsInterpreter::default();
    let boxed = Box::new(interpreter);
    Box::into_raw(boxed) as jlong
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_destroyRegistry(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        unsafe {
            let _ = Box::from_raw(handle as *mut PhsInterpreter);
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_physure_NativeEngine_getUnitExponents<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass,
    registry_handle: jlong,
    expr: JString<'local>,
) -> jobject {
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

    let integer_class = match env.find_class("java/lang/Integer") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    for (symbol, (num, _den)) in &unit.dimensions {
        let key_jstr = match env.new_string(symbol) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let val_obj = match env.new_object(&integer_class, "(I)V", &[JValue::Int(*num as i32)]) {
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

    let reg = UnitRegistry::build_default_si();
    let r_unit = match physure_core::units::parser::Parser::parse_expression_with_registry(&unit_str, &reg) {
        Ok(u) => u,
        Err(e) => {
            throw_physure_exception(&mut env, &format!("Target unit parse error: {}", e));
            return std::ptr::null_mut();
        }
    };

    match q.convert_to(&r_unit) {
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
