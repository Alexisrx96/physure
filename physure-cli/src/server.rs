//! Standalone Model Server (`phs serve`) exposing `.phs` formulas via REST/JSON API.
//!
//! Provides Tier 3 execution: containerized/microservice engine hosting formulas with
//! signature catalog discovery, dynamic function invocation with parameter coercion and
//! dimensional validation, multi-step pipeline DAG execution, and optional token authentication.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use physure_core::error::{PhysureError, PhysureResult};
use physure_script::pipeline::{PhsPipeline, PipelineArg, PipelineStep};
use physure_script::{PhsModule, PhsValue};
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

/// Configuration options for [`ModelServer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelServerConfig {
    pub host: String,
    pub port: u16,
    pub auth_token: Option<String>,
}

impl Default for ModelServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            auth_token: None,
        }
    }
}

/// A serialized parameter descriptor returned by `/api/v1/catalog`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogParam {
    pub name: String,
    pub expected_unit: Option<String>,
}

/// A serialized function signature returned by `/api/v1/catalog`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogFunction {
    pub name: String,
    pub docstring: Option<String>,
    pub params: Vec<CatalogParam>,
}

/// A serialized module descriptor returned by `/api/v1/catalog`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogModule {
    pub name: String,
    pub functions: HashMap<String, CatalogFunction>,
}

/// Catalog response payload returned by `GET /api/v1/catalog`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogResponse {
    pub status: String,
    pub modules: HashMap<String, CatalogModule>,
}

/// A step in a pipeline JSON payload for `POST /api/v1/pipeline`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineStepJson {
    pub module: String,
    pub function: String,
    pub inputs: HashMap<String, serde_json::Value>,
    pub output: String,
}

/// Request payload for `POST /api/v1/pipeline`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineRequestJson {
    pub steps: Vec<PipelineStepJson>,
}

/// Model server hosting loaded `.phs` modules.
pub struct ModelServer {
    pub sources: HashMap<String, String>,
    pub modules: HashMap<String, PhsModule>,
    pub config: ModelServerConfig,
}

impl ModelServer {
    /// Constructs a new [`ModelServer`] by parsing and evaluating the provided module sources.
    pub fn new(sources: HashMap<String, String>, config: ModelServerConfig) -> PhysureResult<Self> {
        let mut modules = HashMap::new();
        for (name, src) in &sources {
            let module = PhsModule::from_source(name, src)?;
            modules.insert(name.clone(), module);
        }
        Ok(Self {
            sources,
            modules,
            config,
        })
    }

    /// Loads `.phs` modules from a directory, manifest file, or single `.phs` file.
    pub fn from_path(path: impl AsRef<Path>, config: ModelServerConfig) -> PhysureResult<Self> {
        let p = path.as_ref();
        let mut sources = HashMap::new();

        if p.is_file() {
            let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file_name == "phs.toml" {
                // Load from manifest exports
                let manifest = crate::package::Manifest::from_file(p)?;
                let base_dir = p.parent().unwrap_or_else(|| Path::new("."));
                let report = manifest.validate(base_dir)?;
                for mod_info in report.modules {
                    let full_path = base_dir.join(&mod_info.relative_path);
                    let source = fs::read_to_string(&full_path).map_err(|e| {
                        PhysureError::Generic(format!(
                            "Error reading module '{}': {}",
                            full_path.display(),
                            e
                        ))
                    })?;
                    sources.insert(mod_info.export_name, source);
                }
            } else if p.extension().and_then(|e| e.to_str()) == Some("phs") {
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("main");
                let source = fs::read_to_string(p).map_err(|e| {
                    PhysureError::Generic(format!("Error reading file '{}': {}", p.display(), e))
                })?;
                sources.insert(stem.to_string(), source);
            } else {
                return Err(PhysureError::Generic(format!(
                    "Unrecognized file format '{}'. Expected .phs or phs.toml",
                    p.display()
                )));
            }
        } else if p.is_dir() {
            let manifest_candidate = p.join("phs.toml");
            if manifest_candidate.is_file() {
                return Self::from_path(&manifest_candidate, config);
            }

            // Load all top-level *.phs files
            let entries = fs::read_dir(p).map_err(|e| {
                PhysureError::Generic(format!(
                    "Cannot read directory '{}': {}",
                    p.display(),
                    e
                ))
            })?;

            for entry in entries {
                let entry = entry.map_err(|e| {
                    PhysureError::Generic(format!("Error reading directory entry: {}", e))
                })?;
                let file_path = entry.path();
                if file_path.is_file()
                    && file_path.extension().and_then(|e| e.to_str()) == Some("phs")
                {
                    if let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) {
                        let source = fs::read_to_string(&file_path).map_err(|e| {
                            PhysureError::Generic(format!(
                                "Error reading '{}': {}",
                                file_path.display(),
                                e
                            ))
                        })?;
                        sources.insert(stem.to_string(), source);
                    }
                }
            }
        } else {
            return Err(PhysureError::Generic(format!(
                "Path '{}' does not exist",
                p.display()
            )));
        }

        if sources.is_empty() {
            return Err(PhysureError::Generic(format!(
                "No .phs modules found in '{}'",
                p.display()
            )));
        }

        Self::new(sources, config)
    }

    /// Generates the catalog description for all loaded modules.
    pub fn build_catalog(&self) -> CatalogResponse {
        let mut cat_modules = HashMap::new();
        for (mod_name, module) in &self.modules {
            let mut fn_map = HashMap::new();
            for (fn_name, sig) in &module.functions {
                let params = sig
                    .params
                    .iter()
                    .map(|p| CatalogParam {
                        name: p.name.clone(),
                        expected_unit: p.expected_unit.clone(),
                    })
                    .collect();
                fn_map.insert(
                    fn_name.clone(),
                    CatalogFunction {
                        name: sig.name.clone(),
                        docstring: sig.docstring.clone(),
                        params,
                    },
                );
            }
            cat_modules.insert(
                mod_name.clone(),
                CatalogModule {
                    name: mod_name.clone(),
                    functions: fn_map,
                },
            );
        }
        CatalogResponse {
            status: "success".to_string(),
            modules: cat_modules,
        }
    }

    /// Authenticates incoming request against configured auth token.
    fn check_auth(&self, request: &Request) -> bool {
        if let Some(ref required_token) = self.config.auth_token {
            for header in request.headers() {
                let name = header.field.as_str().as_str();
                let val = header.value.as_str();
                if name.eq_ignore_ascii_case("authorization") {
                    let token_part = val
                        .trim_start_matches("Bearer ")
                        .trim_start_matches("bearer ")
                        .trim();
                    if token_part == required_token {
                        return true;
                    }
                } else if name.eq_ignore_ascii_case("x-api-key") && val == required_token {
                    return true;
                }
            }
            false
        } else {
            true
        }
    }

    /// Processes an incoming HTTP request and returns the serialized response.
    pub fn handle_request(&self, mut request: Request) -> PhysureResult<()> {
        let method = request.method().clone();
        let raw_url = request.url().to_string();
        let path = raw_url.split('?').next().unwrap_or("").trim_end_matches('/');

        // CORS preflight
        if method == Method::Options {
            let response = Response::empty(200)
                .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap())
                .with_header(Header::from_bytes(&b"Access-Control-Allow-Methods"[..], &b"GET, POST, OPTIONS"[..]).unwrap())
                .with_header(Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type, Authorization, X-API-Key"[..]).unwrap());
            let _ = request.respond(response);
            return Ok(());
        }

        // Authenticate
        if !self.check_auth(&request) {
            let err_json = serde_json::json!({
                "status": "error",
                "error": "Unauthorized: missing or invalid API token"
            });
            let response = Response::from_string(err_json.to_string())
                .with_status_code(StatusCode(401))
                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
            let _ = request.respond(response);
            return Ok(());
        }

        // Routing
        match (&method, path) {
            (Method::Get, "" | "/health" | "/api/v1/health") => {
                let mut loaded_names: Vec<&String> = self.modules.keys().collect();
                loaded_names.sort();
                let resp_json = serde_json::json!({
                    "status": "ok",
                    "version": env!("CARGO_PKG_VERSION"),
                    "modules_loaded": self.modules.len(),
                    "modules": loaded_names
                });
                let response = Response::from_string(resp_json.to_string())
                    .with_status_code(StatusCode(200))
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                    .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap());
                let _ = request.respond(response);
            }
            (Method::Get, "/api/v1/catalog" | "/catalog") => {
                let catalog = self.build_catalog();
                let resp_json = serde_json::to_value(&catalog).unwrap_or(serde_json::json!({"status": "error"}));
                let response = Response::from_string(resp_json.to_string())
                    .with_status_code(StatusCode(200))
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                    .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap());
                let _ = request.respond(response);
            }
            (Method::Post, "/api/v1/pipeline" | "/pipeline") => {
                let mut body = String::new();
                if let Err(e) = request.as_reader().read_to_string(&mut body) {
                    let err_json = serde_json::json!({ "status": "error", "error": format!("Failed to read body: {}", e) });
                    let _ = request.respond(Response::from_string(err_json.to_string()).with_status_code(StatusCode(400)));
                    return Ok(());
                }

                let pipeline_req: PipelineRequestJson = match serde_json::from_str(&body) {
                    Ok(p) => p,
                    Err(e) => {
                        let err_json = serde_json::json!({ "status": "error", "error": format!("Invalid JSON pipeline request: {}", e) });
                        let _ = request.respond(Response::from_string(err_json.to_string()).with_status_code(StatusCode(400)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
                        return Ok(());
                    }
                };

                let mut pipeline = PhsPipeline::new();
                for (name, src) in &self.sources {
                    match PhsModule::from_source(name, src) {
                        Ok(m) => pipeline.add_module(m),
                        Err(e) => {
                            let err_json = serde_json::json!({ "status": "error", "error": format!("Failed to instantiate module '{}': {}", name, e) });
                            let _ = request.respond(Response::from_string(err_json.to_string()).with_status_code(StatusCode(500)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
                            return Ok(());
                        }
                    }
                }

                let mut build_error = None;
                for step_json in pipeline_req.steps {
                    let mut inputs = HashMap::new();
                    for (param_name, arg_val) in step_json.inputs {
                        if let Some(ref_obj) = arg_val.as_object() {
                            if let Some(ref_name) = ref_obj.get("$ref").and_then(|r| r.as_str()) {
                                inputs.insert(param_name, PipelineArg::Reference(ref_name.to_string()));
                                continue;
                            }
                        }
                        match json_value_to_phs_value(&arg_val) {
                            Ok(val) => {
                                inputs.insert(param_name, PipelineArg::Literal(val));
                            }
                            Err(e) => {
                                build_error = Some(format!("Invalid argument '{}' in step '{}': {}", param_name, step_json.output, e));
                                break;
                            }
                        }
                    }
                    if build_error.is_some() {
                        break;
                    }
                    pipeline.add_step(PipelineStep {
                        module_name: step_json.module,
                        function_name: step_json.function,
                        inputs,
                        output_alias: step_json.output,
                    });
                }

                if let Some(err_msg) = build_error {
                    let err_json = serde_json::json!({ "status": "error", "error": err_msg });
                    let _ = request.respond(Response::from_string(err_json.to_string()).with_status_code(StatusCode(400)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
                    return Ok(());
                }

                match pipeline.execute() {
                    Ok(outputs) => {
                        let mut results_json = serde_json::Map::new();
                        for (alias, val) in outputs {
                            results_json.insert(alias, phs_value_to_json(&val));
                        }
                        let resp = serde_json::json!({
                            "status": "success",
                            "results": serde_json::Value::Object(results_json)
                        });
                        let response = Response::from_string(resp.to_string())
                            .with_status_code(StatusCode(200))
                            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                            .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap());
                        let _ = request.respond(response);
                    }
                    Err(e) => {
                        let err_json = serde_json::json!({ "status": "error", "error": e.to_string() });
                        let response = Response::from_string(err_json.to_string())
                            .with_status_code(StatusCode(400))
                            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                            .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap());
                        let _ = request.respond(response);
                    }
                }
            }
            (Method::Post, p) if p.starts_with("/api/v1/") => {
                let segments: Vec<&str> = p.trim_start_matches("/api/v1/").split('/').collect();
                if segments.len() != 2 {
                    let err_json = serde_json::json!({ "status": "error", "error": format!("Invalid function route '{}'. Expected /api/v1/:module/:function", p) });
                    let response = Response::from_string(err_json.to_string()).with_status_code(StatusCode(404)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                    let _ = request.respond(response);
                    return Ok(());
                }

                let module_name = segments[0];
                let fn_name = segments[1];

                let module = match self.modules.get(module_name) {
                    Some(m) => m,
                    None => {
                        let mut available: Vec<&String> = self.modules.keys().collect();
                        available.sort();
                        let err_json = serde_json::json!({
                            "status": "error",
                            "error": format!("Module '{}' not found. Available modules: {:?}", module_name, available)
                        });
                        let response = Response::from_string(err_json.to_string()).with_status_code(StatusCode(404)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                        let _ = request.respond(response);
                        return Ok(());
                    }
                };

                let sig = match module.functions.get(fn_name) {
                    Some(s) => s,
                    None => {
                        let mut available: Vec<&String> = module.functions.keys().collect();
                        available.sort();
                        let err_json = serde_json::json!({
                            "status": "error",
                            "error": format!("Function '{}' not found in module '{}'. Available functions: {:?}", fn_name, module_name, available)
                        });
                        let response = Response::from_string(err_json.to_string()).with_status_code(StatusCode(404)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                        let _ = request.respond(response);
                        return Ok(());
                    }
                };

                let mut body = String::new();
                if let Err(e) = request.as_reader().read_to_string(&mut body) {
                    let err_json = serde_json::json!({ "status": "error", "error": format!("Failed to read request body: {}", e) });
                    let response = Response::from_string(err_json.to_string()).with_status_code(StatusCode(400)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                    let _ = request.respond(response);
                    return Ok(());
                }

                let json_args: serde_json::Value = if body.trim().is_empty() {
                    serde_json::Value::Object(serde_json::Map::new())
                } else {
                    match serde_json::from_str(&body) {
                        Ok(v) => v,
                        Err(e) => {
                            let err_json = serde_json::json!({ "status": "error", "error": format!("Invalid JSON request body: {}", e) });
                            let response = Response::from_string(err_json.to_string()).with_status_code(StatusCode(400)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                            let _ = request.respond(response);
                            return Ok(());
                        }
                    }
                };

                // Prepare call arguments
                let call_args_result: PhysureResult<Vec<PhsValue>> = if let Some(arr) = json_args.get("args").and_then(|a| a.as_array()) {
                    // Positional args
                    arr.iter().map(json_value_to_phs_value).collect()
                } else if let Some(map) = json_args.as_object() {
                    // Keyword args mapped to parameter order
                    let mut ordered = Vec::with_capacity(sig.params.len());
                    for param in &sig.params {
                        if let Some(val_json) = map.get(&param.name) {
                            ordered.push(json_value_to_phs_value(val_json)?);
                        } else {
                            return Err(PhysureError::Generic(format!(
                                "Missing required parameter '{}' for {}.{}()",
                                param.name, module_name, fn_name
                            )));
                        }
                    }
                    Ok(ordered)
                } else {
                    Err(PhysureError::Generic("Request body must be a JSON object of parameters or {\"args\": [...]}".into()))
                };

                let call_args = match call_args_result {
                    Ok(args) => args,
                    Err(e) => {
                        let err_json = serde_json::json!({ "status": "error", "error": e.to_string() });
                        let response = Response::from_string(err_json.to_string()).with_status_code(StatusCode(400)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                        let _ = request.respond(response);
                        return Ok(());
                    }
                };

                match module.invoke(fn_name, &call_args) {
                    Ok(result) => {
                        let resp = serde_json::json!({
                            "status": "success",
                            "result": phs_value_to_json(&result)
                        });
                        let response = Response::from_string(resp.to_string())
                            .with_status_code(StatusCode(200))
                            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                            .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap());
                        let _ = request.respond(response);
                    }
                    Err(e) => {
                        let err_json = serde_json::json!({ "status": "error", "error": e.to_string() });
                        let response = Response::from_string(err_json.to_string())
                            .with_status_code(StatusCode(400))
                            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
                            .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap());
                        let _ = request.respond(response);
                    }
                }
            }
            _ => {
                let err_json = serde_json::json!({
                    "status": "error",
                    "error": format!("Not Found: {} {}", method, path)
                });
                let response = Response::from_string(err_json.to_string())
                    .with_status_code(StatusCode(404))
                    .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
                let _ = request.respond(response);
            }
        }
        Ok(())
    }

    /// Starts the blocking HTTP server loop on `server`.
    pub fn run(self, server: Server) -> PhysureResult<()> {
        for request in server.incoming_requests() {
            if let Err(e) = self.handle_request(request) {
                eprintln!("Error handling request: {}", e);
            }
        }
        Ok(())
    }
}

/// Coerces a [`serde_json::Value`] into a [`PhsValue`].
pub fn json_value_to_phs_value(val: &serde_json::Value) -> PhysureResult<PhsValue> {
    match val {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                Ok(PhsValue::Number(f))
            } else {
                Err(PhysureError::Generic(format!("Invalid number: {}", n)))
            }
        }
        serde_json::Value::Bool(b) => Ok(PhsValue::Bool(*b)),
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.contains(' ') {
                let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
                if let Ok(mag) = parts[0].parse::<f64>() {
                    let unit_str = parts[1].trim();
                    match physure_core::Quantity::new(mag, unit_str) {
                        Ok(q) => return Ok(PhsValue::Quantity(q)),
                        Err(e) => {
                            return Err(PhysureError::Generic(format!(
                                "Invalid unit '{}' in quantity '{}': {}",
                                unit_str, s, e
                            )))
                        }
                    }
                }
            } else if let Ok(n) = trimmed.parse::<f64>() {
                return Ok(PhsValue::Number(n));
            } else if trimmed == "true" {
                return Ok(PhsValue::Bool(true));
            } else if trimmed == "false" {
                return Ok(PhsValue::Bool(false));
            }
            Ok(PhsValue::String(trimmed.to_string()))
        }
        _ => Err(PhysureError::Generic(format!(
            "Unsupported JSON value type for calculation: {:?}",
            val
        ))),
    }
}

/// Formats a [`PhsValue`] into a structured JSON response object.
pub fn phs_value_to_json(val: &PhsValue) -> serde_json::Value {
    match val {
        PhsValue::Quantity(q) => {
            serde_json::json!({
                "type": "quantity",
                "magnitude": q.value.mean(),
                "unit": q.unit.__repr__(),
                "uncertainty": q.value.std_dev(),
                "repr": format!("{}", q)
            })
        }
        PhsValue::Number(n) => serde_json::json!({
            "type": "number",
            "value": n
        }),
        PhsValue::Bool(b) => serde_json::json!({
            "type": "bool",
            "value": b
        }),
        PhsValue::String(s) => serde_json::json!({
            "type": "string",
            "value": s
        }),
        PhsValue::None => serde_json::json!({
            "type": "none"
        }),
        other => serde_json::json!({
            "type": "other",
            "repr": format!("{}", other)
        }),
    }
}

/// CLI runner for `phs serve [dir_or_file_or_manifest] [--port <port>] [--host <host>] [--token <auth_token>]`.
pub fn run_serve(args: &[String]) {
    let mut target_path = None;
    let mut port = 8080u16;
    let mut host = "127.0.0.1".to_string();
    let mut token = None;

    let mut skip_next = false;
    for (i, arg) in args.iter().enumerate().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "serve" {
            continue;
        }
        if arg == "--port" || arg == "-p" {
            if i + 1 < args.len() {
                if let Ok(p) = args[i + 1].parse::<u16>() {
                    port = p;
                    skip_next = true;
                }
            }
            continue;
        }
        if arg == "--host" {
            if i + 1 < args.len() {
                host = args[i + 1].clone();
                skip_next = true;
            }
            continue;
        }
        if arg == "--token" {
            if i + 1 < args.len() {
                token = Some(args[i + 1].clone());
                skip_next = true;
            }
            continue;
        }
        if target_path.is_none() && !arg.starts_with('-') {
            target_path = Some(arg.clone());
        }
    }

    let input_path = target_path.unwrap_or_else(|| ".".to_string());
    let config = ModelServerConfig {
        host: host.clone(),
        port,
        auth_token: token.clone(),
    };

    println!("🚀 Initializing Physure Model Server from '{}'...", input_path);
    let server_inst = match ModelServer::from_path(&input_path, config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Initialization Error: {}", e);
            std::process::exit(1);
        }
    };

    let bind_addr = format!("{}:{}", host, port);
    let http_server = match Server::http(&bind_addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to bind server to '{}': {}", bind_addr, e);
            std::process::exit(1);
        }
    };

    println!("✓ Loaded {} module(s):", server_inst.modules.len());
    for (name, module) in &server_inst.modules {
        let fn_names: Vec<&String> = module.functions.keys().collect();
        println!("  - module '{}' ({} function(s))", name, fn_names.len());
    }

    if token.is_some() {
        println!("🔒 Authentication: Enabled (Bearer token or X-API-Key header required)");
    } else {
        println!("⚠️  Authentication: Disabled (local development mode)");
    }

    println!("\n🌐 Physure Model Server running at http://{}", bind_addr);
    println!("   - Health Check: GET  http://{}/health", bind_addr);
    println!("   - API Catalog:  GET  http://{}/api/v1/catalog", bind_addr);
    println!("   - Call Formula: POST http://{}/api/v1/:module/:function", bind_addr);
    println!("   - Run Pipeline: POST http://{}/api/v1/pipeline", bind_addr);
    println!("\nPress Ctrl+C to stop.\n");

    if let Err(e) = server_inst.run(http_server) {
        eprintln!("Server error: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn create_test_server(token: Option<&str>) -> (Arc<ModelServer>, String) {
        let mut sources = HashMap::new();
        sources.insert(
            "geom".to_string(),
            "fn area_tubo(d: m) = 3.1415926535 * (d / 2)^2\n".to_string(),
        );
        sources.insert(
            "hydr".to_string(),
            "/// Hydraulic force\n/// @param P Pressure in kg/(m*s^2)\n/// @param A Area in m^2\nfn fuerza_empuje(P: kg/(m*s^2), A: m^2) = P * A\n".to_string(),
        );

        let server = Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let base_url = format!("http://{}", addr);

        let config = ModelServerConfig {
            host: addr.ip().to_string(),
            port: addr.port(),
            auth_token: token.map(|s| s.to_string()),
        };

        let model_server = Arc::new(ModelServer::new(sources, config).unwrap());
        let server_clone = Arc::clone(&model_server);

        std::thread::spawn(move || {
            for req in server.incoming_requests() {
                let _ = server_clone.handle_request(req);
            }
        });

        (model_server, base_url)
    }

    #[test]
    fn test_server_catalog_and_health() {
        let (_server, base_url) = create_test_server(None);

        // Health
        let health: serde_json::Value = ureq::get(&format!("{}/health", base_url))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(health["status"], "ok");
        assert_eq!(health["modules_loaded"], 2);

        // Catalog
        let catalog: CatalogResponse = ureq::get(&format!("{}/api/v1/catalog", base_url))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(catalog.status, "success");
        assert!(catalog.modules.contains_key("geom"));
        assert!(catalog.modules.contains_key("hydr"));

        let hydr = &catalog.modules["hydr"];
        assert!(hydr.functions.contains_key("fuerza_empuje"));
        let f = &hydr.functions["fuerza_empuje"];
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].name, "P");
        assert_eq!(f.params[0].expected_unit.as_deref(), Some("kg/(m*s^2)"));
    }

    #[test]
    fn test_server_invoke_function_success_and_kwargs() {
        let (_server, base_url) = create_test_server(None);

        let resp: serde_json::Value = ureq::post(&format!("{}/api/v1/geom/area_tubo", base_url))
            .send_json(serde_json::json!({
                "d": "0.05 m"
            }))
            .unwrap()
            .into_json()
            .unwrap();

        assert_eq!(resp["status"], "success");
        let result = &resp["result"];
        assert_eq!(result["type"], "quantity");
        assert_eq!(result["unit"], "m^2");
        let mag = result["magnitude"].as_f64().unwrap();
        assert!((mag - 0.001963495).abs() < 1e-6);
    }

    #[test]
    fn test_server_invoke_wrong_dimensions_returns_400() {
        let (_server, base_url) = create_test_server(None);

        let resp = ureq::post(&format!("{}/api/v1/geom/area_tubo", base_url))
            .send_json(serde_json::json!({
                "d": "10 s" // seconds instead of meters
            }));

        assert!(resp.is_err());
        let err = resp.unwrap_err();
        if let ureq::Error::Status(code, response) = err {
            assert_eq!(code, 400);
            let body: serde_json::Value = response.into_json().unwrap();
            assert_eq!(body["status"], "error");
            assert!(body["error"].as_str().unwrap().contains("incompatible"));
        } else {
            panic!("Expected Status(400)");
        }
    }

    #[test]
    fn test_server_invoke_missing_function_returns_404() {
        let (_server, base_url) = create_test_server(None);

        let resp = ureq::post(&format!("{}/api/v1/geom/non_existent_fn", base_url))
            .send_json(serde_json::json!({}));

        assert!(resp.is_err());
        if let ureq::Error::Status(code, _) = resp.unwrap_err() {
            assert_eq!(code, 404);
        } else {
            panic!("Expected 404");
        }
    }

    #[test]
    fn test_server_pipeline_execution() {
        let (_server, base_url) = create_test_server(None);

        let pipeline_body = serde_json::json!({
            "steps": [
                {
                    "module": "geom",
                    "function": "area_tubo",
                    "inputs": {
                        "d": "0.05 m"
                    },
                    "output": "area"
                },
                {
                    "module": "hydr",
                    "function": "fuerza_empuje",
                    "inputs": {
                        "P": "500000 kg/(m*s^2)",
                        "A": { "$ref": "area" }
                    },
                    "output": "fuerza"
                }
            ]
        });

        let resp: serde_json::Value = ureq::post(&format!("{}/api/v1/pipeline", base_url))
            .send_json(pipeline_body)
            .unwrap()
            .into_json()
            .unwrap();

        assert_eq!(resp["status"], "success");
        let results = &resp["results"];
        assert!(results["area"].is_object());
        assert!(results["fuerza"].is_object());

        let fuerza_mag = results["fuerza"]["magnitude"].as_f64().unwrap();
        assert!((fuerza_mag - 981.7477).abs() < 0.1);
    }

    #[test]
    fn test_server_auth_token_enforcement() {
        let (_server, base_url) = create_test_server(Some("secret-key-123"));

        // 1. Without auth -> 401
        let resp_no_auth = ureq::get(&format!("{}/api/v1/catalog", base_url)).call();
        assert!(resp_no_auth.is_err());
        if let ureq::Error::Status(code, _) = resp_no_auth.unwrap_err() {
            assert_eq!(code, 401);
        } else {
            panic!("Expected 401");
        }

        // 2. With Bearer token -> 200
        let resp_bearer = ureq::get(&format!("{}/api/v1/catalog", base_url))
            .set("Authorization", "Bearer secret-key-123")
            .call();
        assert!(resp_bearer.is_ok());

        // 3. With X-API-Key -> 200
        let resp_api_key = ureq::get(&format!("{}/api/v1/catalog", base_url))
            .set("X-API-Key", "secret-key-123")
            .call();
        assert!(resp_api_key.is_ok());
    }
}
