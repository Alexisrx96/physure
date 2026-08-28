//! Standalone Model Server (`phs serve`) exposing `.phs` formulas via REST/JSON API.
//!
//! Provides Tier 3 execution: containerized/microservice engine hosting formulas with
//! signature catalog discovery, dynamic function invocation with parameter coercion and
//! dimensional validation, multi-step pipeline DAG execution, and optional token authentication.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;

use physure_core::error::{PhysureError, PhysureResult};
use physure_script::pipeline::{PipelineArg, PipelineStep};
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

/// Maximum request body size accepted by the server (10 MB).
pub const MAX_REQUEST_BODY_BYTES: u64 = 10 * 1024 * 1024;

/// Fixed size of the request-handling worker pool started by [`ModelServer::run`]. A
/// generous-but-bounded default for a local/small-deployment model server; bounding it at
/// all (rather than the previous unbounded one-thread-per-connection) is the point -- see
/// `run`'s doc comment for the pentest finding (I1) this closes.
const SERVER_WORKER_THREADS: usize = 32;

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
    pub modules: HashMap<String, PhsModule>,
    pub config: ModelServerConfig,
}

impl ModelServer {
    /// Constructs a new [`ModelServer`] by parsing and evaluating the provided module sources.
    ///
    /// Only `modules` (the parsed result) is retained -- `sources` is consumed here and not
    /// kept as a struct field. It used to be kept around so `/api/v1/pipeline` could
    /// re-parse from it on every request; that re-parse was removed (a pentest-flagged
    /// waste, see `execute_pipeline_steps`'s doc comment) in favor of executing directly
    /// against the already-parsed `modules`, so nothing reads the raw source text again
    /// after construction.
    pub fn new(sources: HashMap<String, String>, config: ModelServerConfig) -> PhysureResult<Self> {
        let mut modules = HashMap::new();
        for (name, src) in &sources {
            let module = PhsModule::from_source(name, src)?;
            modules.insert(name.clone(), module);
        }
        Ok(Self { modules, config })
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

    /// Executes `steps` against this server's already-parsed `self.modules`, threading each
    /// step's result forward by `output_alias`, exactly like
    /// [`physure_script::pipeline::PhsPipeline::execute`] does.
    ///
    /// This is a deliberate, minimal reimplementation of that function's loop body against
    /// borrowed (`&PhsModule`) rather than owned modules -- not a divergent second
    /// implementation of pipeline semantics. It exists only because
    /// `PhsPipeline::add_module` takes ownership of a `PhsModule`, and `PhsModule`
    /// deliberately doesn't implement `Clone` (see its struct doc comment in
    /// `physure-script/src/module.rs`): going through `PhsPipeline` from a server request
    /// handler would force re-parsing every loaded module's source from scratch on every
    /// single pipeline request, even though `self.modules` already holds every module
    /// parsed once at server startup (confirmed wasteful -- a pentest finding, since the
    /// re-parse cost is O(module count) on every request regardless of pipeline size).
    /// Keeping this loop here avoids that waste without adding a public "run a pipeline
    /// against borrowed modules" API to `physure-script` that nothing else currently needs.
    ///
    /// The pipeline-size ceiling (`physure_core::max_pipeline_steps`) is intentionally NOT
    /// re-checked here: the `/api/v1/pipeline` handler rejects an oversized request right
    /// after parsing its JSON body, before calling this method at all, so by the time
    /// `steps` reaches here it has already been validated.
    fn execute_pipeline_steps(&self, steps: &[PipelineStep]) -> PhysureResult<HashMap<String, PhsValue>> {
        let mut scope: HashMap<String, PhsValue> = HashMap::new();

        for step in steps {
            let module = self.modules.get(&step.module_name).ok_or_else(|| {
                PhysureError::Generic(format!(
                    "Module '{}' not found in pipeline (step '{}')",
                    step.module_name, step.output_alias
                ))
            })?;

            let sig = module.functions.get(&step.function_name).ok_or_else(|| {
                PhysureError::Generic(format!(
                    "Function '{}' not found in module '{}' (step '{}')",
                    step.function_name, step.module_name, step.output_alias
                ))
            })?;

            for input_key in step.inputs.keys() {
                if !sig.params.iter().any(|p| &p.name == input_key) {
                    return Err(PhysureError::Generic(format!(
                        "Unexpected input '{}' for {}.{}() in step '{}'",
                        input_key, step.module_name, step.function_name, step.output_alias
                    )));
                }
            }

            let mut call_args = Vec::with_capacity(sig.params.len());
            for param in &sig.params {
                let arg_spec = step.inputs.get(&param.name).ok_or_else(|| {
                    PhysureError::Generic(format!(
                        "Missing input '{}' for step '{}'",
                        param.name, step.output_alias
                    ))
                })?;

                let val = match arg_spec {
                    PipelineArg::Literal(v) => v.clone(),
                    PipelineArg::Reference(ref_name) => scope.get(ref_name).cloned().ok_or_else(|| {
                        PhysureError::Generic(format!(
                            "Unresolved reference '{}' in step '{}'",
                            ref_name, step.output_alias
                        ))
                    })?,
                };
                call_args.push(val);
            }

            let result = module.invoke(&step.function_name, &call_args)?;
            scope.insert(step.output_alias.clone(), result);
        }

        Ok(scope)
    }

    /// Authenticates incoming request against configured auth token.
    fn check_auth(&self, request: &Request) -> bool {
        if let Some(ref required_token) = self.config.auth_token {
            // Defense in depth: `run_serve` rejects `--token ""` at startup (a blank
            // token would otherwise be a full auth bypass, since an empty-valued header
            // like `X-API-Key:` would satisfy `"" == ""`), so this should be
            // unreachable in practice. But if an empty token ever gets through some
            // other construction path (e.g. `ModelServerConfig` built directly rather
            // than via the CLI), fail closed rather than silently accepting anything.
            if required_token.is_empty() {
                return false;
            }
            for header in request.headers() {
                let name = header.field.as_str().as_str();
                let val = header.value.as_str();
                if name.eq_ignore_ascii_case("authorization") {
                    let token_part = val
                        .trim_start_matches("Bearer ")
                        .trim_start_matches("bearer ")
                        .trim();
                    if constant_time_eq(token_part, required_token) {
                        return true;
                    }
                } else if name.eq_ignore_ascii_case("x-api-key") && constant_time_eq(val, required_token) {
                    return true;
                }
            }
            false
        } else {
            true
        }
    }

    /// Whether responses to the current request should carry CORS headers. Only true when
    /// an auth token is configured.
    ///
    /// With no token configured, `phs serve` documents and prints itself as "local
    /// development mode" -- but sending `Access-Control-Allow-Origin: *` unconditionally
    /// would mean any webpage a user's browser happens to visit could script a fetch to
    /// `http://127.0.0.1:<port>/api/v1/...` and both invoke arbitrary loaded functions and
    /// read the results (confirmed live in the pentest, finding I5). Omitting CORS headers
    /// in that mode leaves the browser's own same-origin policy as the default protection,
    /// which is the safe behavior for an unauthenticated local server. When a token *is*
    /// configured, an attacker without it can't do anything useful with a cross-origin
    /// request anyway, so enabling CORS there is fine.
    fn cors_enabled(&self) -> bool {
        self.config.auth_token.is_some()
    }

    /// Adds `Access-Control-Allow-Origin: *` to `response`, but only when [`cors_enabled`]
    /// is true. See [`ModelServer::cors_enabled`] for why this is conditional.
    ///
    /// [`cors_enabled`]: ModelServer::cors_enabled
    fn with_cors<R: std::io::Read>(&self, response: Response<R>) -> Response<R> {
        if self.cors_enabled() {
            response.with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap())
        } else {
            response
        }
    }

    /// Processes an incoming HTTP request and returns the serialized response.
    pub fn handle_request(&self, mut request: Request) -> PhysureResult<()> {
        let method = request.method().clone();
        let raw_url = request.url().to_string();
        let path = raw_url.split('?').next().unwrap_or("").trim_end_matches('/');

        // CORS preflight. Only answer with actual CORS grant headers when an auth token is
        // configured -- see `cors_enabled`'s doc comment. With no token, still answer 200
        // (so same-origin callers aren't broken) but without the headers that would let a
        // cross-origin browser request through.
        if method == Method::Options {
            let mut response = Response::empty(200);
            if self.cors_enabled() {
                response = response
                    .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap())
                    .with_header(Header::from_bytes(&b"Access-Control-Allow-Methods"[..], &b"GET, POST, OPTIONS"[..]).unwrap())
                    .with_header(Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type, Authorization, X-API-Key"[..]).unwrap());
            }
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
                let response = self.with_cors(
                    Response::from_string(resp_json.to_string())
                        .with_status_code(StatusCode(200))
                        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()),
                );
                let _ = request.respond(response);
            }
            (Method::Get, "/api/v1/catalog" | "/catalog") => {
                let catalog = self.build_catalog();
                let resp_json = serde_json::to_value(&catalog).unwrap_or(serde_json::json!({"status": "error"}));
                let response = self.with_cors(
                    Response::from_string(resp_json.to_string())
                        .with_status_code(StatusCode(200))
                        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()),
                );
                let _ = request.respond(response);
            }
            (Method::Post, "/api/v1/pipeline" | "/pipeline") => {
                let mut body = String::new();
                if let Err(e) = std::io::Read::take(request.as_reader(), MAX_REQUEST_BODY_BYTES).read_to_string(&mut body) {
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

                // Reject an oversized pipeline immediately, before doing any further work
                // (JSON-to-PipelineStep conversion, module lookups, execution). Defense in
                // depth on top of the interpreter-level `max_pipeline_steps` ceiling: cheap,
                // and avoids the below step-building work entirely for the oversized case.
                let max_steps = physure_core::max_pipeline_steps();
                if pipeline_req.steps.len() > max_steps {
                    let err_json = serde_json::json!({
                        "status": "error",
                        "error": format!(
                            "pipeline has {} steps, exceeding the max_pipeline_steps ceiling of {}; raise `max_pipeline_steps` in physure.conf's [Settings] section if this is a legitimate workload",
                            pipeline_req.steps.len(), max_steps
                        )
                    });
                    let _ = request.respond(Response::from_string(err_json.to_string()).with_status_code(StatusCode(400)).with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
                    return Ok(());
                }

                // Build the step list directly against `self.modules` (already parsed once
                // at server startup) instead of instantiating a `physure_script::pipeline::
                // PhsPipeline`, which would require re-parsing every loaded module's source
                // from scratch on every request -- see `execute_pipeline_steps`'s doc
                // comment for why.
                let mut steps = Vec::with_capacity(pipeline_req.steps.len());
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
                    steps.push(PipelineStep {
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

                match self.execute_pipeline_steps(&steps) {
                    Ok(outputs) => {
                        let mut results_json = serde_json::Map::new();
                        for (alias, val) in outputs {
                            results_json.insert(alias, phs_value_to_json(&val));
                        }
                        let resp = serde_json::json!({
                            "status": "success",
                            "results": serde_json::Value::Object(results_json)
                        });
                        let response = self.with_cors(
                            Response::from_string(resp.to_string())
                                .with_status_code(StatusCode(200))
                                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()),
                        );
                        let _ = request.respond(response);
                    }
                    Err(e) => {
                        let err_json = serde_json::json!({ "status": "error", "error": e.to_string() });
                        let response = self.with_cors(
                            Response::from_string(err_json.to_string())
                                .with_status_code(StatusCode(400))
                                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()),
                        );
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

                let module_name = percent_decode(segments[0]);
                let fn_name = percent_decode(segments[1]);

                let module = match self.modules.get(&module_name) {
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

                let sig = match module.functions.get(&fn_name) {
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
                if let Err(e) = std::io::Read::take(request.as_reader(), MAX_REQUEST_BODY_BYTES).read_to_string(&mut body) {
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
                    // Keyword args mapped to parameter order. Both branches below become
                    // this block's own `Err(...)` value via `.collect()` -- never a `return`
                    // or `?` that would unwind out of `handle_request` itself. An escape
                    // like that would skip the `call_args_result` match just below (which
                    // exists specifically to turn a bad request into a clean 400 JSON
                    // response) and fall through to `tiny_http`'s "request dropped without
                    // `.respond()`" fallback -- an opaque, bodyless HTTP 500 (pentest
                    // finding I4, confirmed live with both a missing parameter and a
                    // wrong-JSON-type parameter).
                    if let Some(unexpected) = map.keys().find(|k| !sig.params.iter().any(|p| &p.name == *k)) {
                        Err(PhysureError::Generic(format!(
                            "Unexpected parameter '{}' for {}.{}()",
                            unexpected, module_name, fn_name
                        )))
                    } else {
                        sig.params
                            .iter()
                            .map(|param| match map.get(&param.name) {
                                Some(val_json) => json_value_to_phs_value(val_json),
                                None => Err(PhysureError::Generic(format!(
                                    "Missing required parameter '{}' for {}.{}()",
                                    param.name, module_name, fn_name
                                ))),
                            })
                            .collect::<PhysureResult<Vec<PhsValue>>>()
                    }
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

                match module.invoke(&fn_name, &call_args) {
                    Ok(result) => {
                        let resp = serde_json::json!({
                            "status": "success",
                            "result": phs_value_to_json(&result)
                        });
                        let response = self.with_cors(
                            Response::from_string(resp.to_string())
                                .with_status_code(StatusCode(200))
                                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()),
                        );
                        let _ = request.respond(response);
                    }
                    Err(e) => {
                        let err_json = serde_json::json!({ "status": "error", "error": e.to_string() });
                        let response = self.with_cors(
                            Response::from_string(err_json.to_string())
                                .with_status_code(StatusCode(400))
                                .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()),
                        );
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

    /// Starts the request-handling worker pool on `server`, bounded to
    /// [`SERVER_WORKER_THREADS`] concurrent handler threads.
    ///
    /// Previously this spawned a fresh `std::thread::spawn` per accepted connection with no
    /// cap. Confirmed exploitable live (pentest finding I1): a client that sends a
    /// `Content-Length` header and then never finishes sending the body blocks its handler
    /// thread inside `request.as_reader().read_to_string(..)` indefinitely -- `tiny_http`
    /// 0.12's `Request` doesn't expose the underlying stream or any way to set a read
    /// timeout on it (checked its public API: `as_reader()` returns an opaque
    /// `&mut dyn Read`, and there is no `set_read_timeout`-equivalent anywhere in `Server`,
    /// `ServerConfig`, or `Request`), so that is a documented limitation rather than
    /// something this fix can close directly. On top of that, `std::thread::spawn` itself
    /// panics if the OS refuses to create a new thread, and that spawn sat directly in the
    /// accept loop, so exhausting the OS thread limit would have taken down the whole
    /// server process.
    ///
    /// A fixed-size pool bounds both problems: exactly [`SERVER_WORKER_THREADS`] threads
    /// are ever created, all up front inside this call rather than per-request. `Server`'s
    /// own `recv()` is documented safe to call from multiple threads concurrently (it pops
    /// from an internal `Mutex`-guarded queue), so each worker just loops on
    /// `server.recv()`. A pile of slow/malicious clients can now only ever starve this
    /// fixed pool -- new requests queue up in `tiny_http`'s own internal message queue
    /// instead of spawning unbounded OS threads -- and legitimate traffic keeps flowing on
    /// whichever workers aren't currently stuck.
    pub fn run(self, server: Server) -> PhysureResult<()> {
        let model = std::sync::Arc::new(self);
        std::thread::scope(|scope| {
            for _ in 0..SERVER_WORKER_THREADS {
                let model = std::sync::Arc::clone(&model);
                let server = &server;
                scope.spawn(move || loop {
                    match server.recv() {
                        Ok(request) => {
                            if let Err(e) = model.handle_request(request) {
                                eprintln!("Error handling request: {}", e);
                            }
                        }
                        Err(e) => {
                            eprintln!("Server connection error, worker thread exiting: {}", e);
                            // Wake exactly one more sibling worker blocked in `recv()` so
                            // shutdown cascades through the whole pool (each exiting
                            // worker wakes the next) instead of leaving the other workers
                            // parked in `recv()` forever, which would otherwise make this
                            // `thread::scope` block never return.
                            server.unblock();
                            break;
                        }
                    }
                });
            }
        });
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

/// Compares two strings for equality without leaking (via observable timing) information
/// about *where* they first differ, to close a timing side-channel on the auth token
/// comparisons in `check_auth` (pentest finding I2, `token_part == required_token` /
/// `val == required_token` were plain, potentially short-circuiting `PartialEq`).
///
/// This is not the stronger guarantee of being independent of *length* -- a mismatched
/// length still returns immediately, which is observable, but there is no length-independent
/// padding scheme worth picking here (both sides of every real comparison are already the
/// same untrusted-request-controlled and configured-token strings every time, so a length
/// oracle reveals nothing an attacker doesn't already know how to probe for by other means).
/// For two candidates of equal length, every byte is visited and the differences are
/// accumulated with a bitwise OR and no early return, so the comparison takes the same
/// number of steps regardless of where (or whether) a mismatch occurs.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Decodes percent-encoded UTF-8 strings (e.g. `%20` -> ` `, `%C3%AD` -> `í`).
fn percent_decode(s: &str) -> String {
    let mut result = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..=i + 2]).unwrap_or(""), 16) {
                result.push(val);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).to_string()
}

/// Validates a `--token` value before it is allowed to become the server's configured auth
/// token. Returns `Err` with a user-facing message when `token` is `Some` but empty or
/// all-whitespace; `None` (no `--token` flag at all, i.e. local development mode) and any
/// non-blank token are both valid.
///
/// This exists to close a full auth bypass (pentest finding I3): starting the server with
/// `--token ""` printed the startup banner's "🔒 Authentication: Enabled" while actually
/// accepting any request carrying an empty-valued auth header (e.g. the real HTTP header
/// `X-API-Key:` with nothing after the colon), because `check_auth`'s comparison against an
/// empty `required_token` degenerates to `"" == ""`. Rejecting the empty token at startup,
/// before it ever becomes `self.config.auth_token`, means the server can never end up in
/// that silently-open "Enabled" state. It's a free function (not inlined into `run_serve`)
/// specifically so it can be unit-tested directly -- `run_serve` itself exits the process on
/// failure via `std::process::exit`, which isn't practical to exercise in-process.
fn validate_auth_token(token: &Option<String>) -> Result<(), String> {
    if let Some(t) = token {
        if t.trim().is_empty() {
            return Err(
                "--token was given an empty (or all-whitespace) value. An empty token would \
                 silently disable authentication while still reporting it as enabled -- a full \
                 auth bypass, since a request with a blank auth header would then match it. \
                 Omit --token entirely for local development mode, or supply a non-empty token."
                    .to_string(),
            );
        }
    }
    Ok(())
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

    if let Err(msg) = validate_auth_token(&token) {
        eprintln!("Configuration Error: {}", msg);
        std::process::exit(1);
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

    #[test]
    fn test_server_percent_encoded_url_routes() {
        let (_server, base_url) = create_test_server(None);

        // POST /api/v1/geom/area_tubo with percent-encoded characters e.g. %61%72%65%61_tubo ("area_tubo")
        let resp: serde_json::Value = ureq::post(&format!("{}/api/v1/geom/%61%72%65%61_tubo", base_url))
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
    }

    // --- Finding I3: empty --token is a full auth bypass -----------------------------------

    #[test]
    fn validate_auth_token_rejects_empty_string() {
        assert!(validate_auth_token(&Some(String::new())).is_err());
    }

    #[test]
    fn validate_auth_token_rejects_whitespace_only() {
        assert!(validate_auth_token(&Some("   ".to_string())).is_err());
    }

    #[test]
    fn validate_auth_token_accepts_none() {
        assert!(validate_auth_token(&None).is_ok());
    }

    #[test]
    fn validate_auth_token_accepts_nonblank_token() {
        assert!(validate_auth_token(&Some("secret-key-123".to_string())).is_ok());
    }

    #[test]
    fn check_auth_fails_closed_if_an_empty_token_somehow_gets_through() {
        // `validate_auth_token` is the mandatory startup guard, but `check_auth` itself must
        // also refuse to match against an empty configured token as defense in depth --
        // constructing `ModelServerConfig` directly (bypassing `run_serve`'s CLI parsing, as
        // some other caller of this library-shaped code always could) is exactly the "somehow
        // gets through" scenario. Without the defensive guard, an empty-valued X-API-Key
        // header would satisfy the old `"" == ""` comparison and authenticate successfully.
        let (_server, base_url) = create_test_server(Some(""));

        let resp = ureq::get(&format!("{}/api/v1/catalog", base_url))
            .set("X-API-Key", "")
            .call();
        assert!(resp.is_err(), "empty token must not authenticate an empty header");
        if let ureq::Error::Status(code, _) = resp.unwrap_err() {
            assert_eq!(code, 401);
        } else {
            panic!("Expected 401");
        }
    }

    // --- Finding I2: constant-time token comparison -----------------------------------------

    #[test]
    fn constant_time_eq_matches_equal_strings() {
        assert!(constant_time_eq("secret-key-123", "secret-key-123"));
    }

    #[test]
    fn constant_time_eq_rejects_same_length_mismatch() {
        assert!(!constant_time_eq("secret-key-123", "secret-key-124"));
    }

    #[test]
    fn constant_time_eq_rejects_different_length() {
        assert!(!constant_time_eq("short", "much-longer-string"));
    }

    #[test]
    fn constant_time_eq_matches_empty_strings() {
        assert!(constant_time_eq("", ""));
    }

    // --- Finding I4: kwargs branch must produce a clean 400, never an empty 500 ------------

    #[test]
    fn test_server_kwargs_missing_parameter_returns_400_not_500() {
        let (_server, base_url) = create_test_server(None);

        // area_tubo requires "d"; sending an empty object used to `return Err(..)` straight
        // out of `handle_request`, producing tiny_http's bodyless fallback 500 instead of a
        // clean 400 JSON error.
        let resp = ureq::post(&format!("{}/api/v1/geom/area_tubo", base_url)).send_json(serde_json::json!({}));

        assert!(resp.is_err());
        match resp.unwrap_err() {
            ureq::Error::Status(code, response) => {
                assert_eq!(code, 400, "expected a clean 400, not an opaque 500");
                let body: serde_json::Value = response.into_json().expect("response must have a JSON body");
                assert_eq!(body["status"], "error");
                assert!(body["error"].as_str().unwrap().contains("Missing required parameter"));
            }
            other => panic!("Expected Status(400), got {other:?}"),
        }
    }

    #[test]
    fn test_server_kwargs_wrong_json_type_returns_400_not_500() {
        let (_server, base_url) = create_test_server(None);

        // `d: null` fails `json_value_to_phs_value` (falls into its catch-all `Err` arm).
        // The `?` on that call used to propagate straight out of `handle_request` via
        // `PhysureResult<()>`, again producing tiny_http's bodyless fallback 500.
        let resp = ureq::post(&format!("{}/api/v1/geom/area_tubo", base_url))
            .send_json(serde_json::json!({ "d": null }));

        assert!(resp.is_err());
        match resp.unwrap_err() {
            ureq::Error::Status(code, response) => {
                assert_eq!(code, 400, "expected a clean 400, not an opaque 500");
                let body: serde_json::Value = response.into_json().expect("response must have a JSON body");
                assert_eq!(body["status"], "error");
            }
            other => panic!("Expected Status(400), got {other:?}"),
        }
    }

    // --- Finding M1: kwargs branch should reject unexpected parameter names ----------------

    #[test]
    fn test_server_kwargs_unexpected_parameter_returns_400() {
        let (_server, base_url) = create_test_server(None);

        let resp = ureq::post(&format!("{}/api/v1/geom/area_tubo", base_url)).send_json(serde_json::json!({
            "d": "0.05 m",
            "not_a_real_param": 1
        }));

        assert!(resp.is_err());
        match resp.unwrap_err() {
            ureq::Error::Status(code, response) => {
                assert_eq!(code, 400);
                let body: serde_json::Value = response.into_json().unwrap();
                assert!(body["error"].as_str().unwrap().contains("Unexpected parameter"));
            }
            other => panic!("Expected Status(400), got {other:?}"),
        }
    }

    // --- Finding I5: CORS only when a token is configured -----------------------------------

    #[test]
    fn test_server_cors_header_present_when_token_configured() {
        let (_server, base_url) = create_test_server(Some("secret-key-123"));

        let resp = ureq::get(&format!("{}/api/v1/catalog", base_url))
            .set("X-API-Key", "secret-key-123")
            .call()
            .unwrap();
        assert_eq!(resp.header("Access-Control-Allow-Origin"), Some("*"));

        let preflight = ureq::request("OPTIONS", &format!("{}/api/v1/catalog", base_url))
            .call()
            .unwrap();
        assert_eq!(preflight.status(), 200);
        assert_eq!(preflight.header("Access-Control-Allow-Origin"), Some("*"));
        assert!(preflight.header("Access-Control-Allow-Methods").is_some());
        assert!(preflight.header("Access-Control-Allow-Headers").is_some());
    }

    #[test]
    fn test_server_cors_header_absent_when_no_token_configured() {
        let (_server, base_url) = create_test_server(None);

        let resp = ureq::get(&format!("{}/api/v1/catalog", base_url)).call().unwrap();
        assert_eq!(
            resp.header("Access-Control-Allow-Origin"),
            None,
            "an unauthenticated local server must not grant cross-origin access"
        );

        // /health goes through the same auth gate + CORS logic; check it too.
        let health = ureq::get(&format!("{}/health", base_url)).call().unwrap();
        assert_eq!(health.header("Access-Control-Allow-Origin"), None);

        let preflight = ureq::request("OPTIONS", &format!("{}/api/v1/catalog", base_url))
            .call()
            .unwrap();
        assert_eq!(preflight.status(), 200, "preflight must still answer 200");
        assert_eq!(preflight.header("Access-Control-Allow-Origin"), None);
        assert_eq!(preflight.header("Access-Control-Allow-Methods"), None);
        assert_eq!(preflight.header("Access-Control-Allow-Headers"), None);
    }

    #[test]
    fn test_server_cors_header_absent_on_error_responses_without_token() {
        let (_server, base_url) = create_test_server(None);

        let resp = ureq::post(&format!("{}/api/v1/geom/area_tubo", base_url)).send_json(serde_json::json!({}));
        match resp.unwrap_err() {
            ureq::Error::Status(code, response) => {
                assert_eq!(code, 400);
                assert_eq!(response.header("Access-Control-Allow-Origin"), None);
            }
            other => panic!("Expected Status(400), got {other:?}"),
        }
    }

    // --- Finding I1/I6: pipeline step-count rejected before doing any work -----------------

    #[test]
    fn test_server_pipeline_exceeding_max_steps_returns_400_before_executing() {
        let (_server, base_url) = create_test_server(None);

        // Default `max_pipeline_steps` ceiling is 1,000 (see
        // `physure_core::settings::max_pipeline_steps`'s doc comment); this repo's own
        // embedded `physure.conf` agrees, and this test doesn't touch the process-wide
        // setting (it runs in a separate thread from the request handler, so a
        // `scoped_max_pipeline_steps` thread-local override wouldn't even reach it). Naming
        // a nonexistent module in every step proves the ceiling check runs before any
        // per-step module lookup: a "Module ... not found" error would mean the check didn't
        // run first.
        let steps: Vec<PipelineStepJson> = (0..1001)
            .map(|i| PipelineStepJson {
                module: "does_not_exist".to_string(),
                function: "f".to_string(),
                inputs: HashMap::new(),
                output: format!("out_{i}"),
            })
            .collect();
        let pipeline_req = PipelineRequestJson { steps };

        let resp = ureq::post(&format!("{}/api/v1/pipeline", base_url)).send_json(serde_json::to_value(&pipeline_req).unwrap());

        assert!(resp.is_err());
        match resp.unwrap_err() {
            ureq::Error::Status(code, response) => {
                assert_eq!(code, 400);
                let body: serde_json::Value = response.into_json().unwrap();
                let msg = body["error"].as_str().unwrap();
                assert!(msg.contains("max_pipeline_steps"), "unexpected message: {msg}");
                assert!(msg.contains("1001") && msg.contains("1000"), "unexpected message: {msg}");
                assert!(
                    !msg.contains("not found"),
                    "ceiling check must reject before any module lookup: {msg}"
                );
            }
            other => panic!("Expected Status(400), got {other:?}"),
        }
    }

    #[test]
    fn test_server_pipeline_still_executes_via_borrowed_modules() {
        // Regression check for the finding-5 rewrite (server now executes pipeline steps
        // directly against `&self.modules` instead of re-parsing every module's source into
        // a fresh `PhsPipeline` on every request): correctness must be unchanged.
        let (_server, base_url) = create_test_server(None);

        let pipeline_body = serde_json::json!({
            "steps": [
                { "module": "geom", "function": "area_tubo", "inputs": { "d": "0.05 m" }, "output": "area" },
                {
                    "module": "hydr",
                    "function": "fuerza_empuje",
                    "inputs": { "P": "500000 kg/(m*s^2)", "A": { "$ref": "area" } },
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
        let fuerza_mag = resp["results"]["fuerza"]["magnitude"].as_f64().unwrap();
        assert!((fuerza_mag - 981.7477).abs() < 0.1);
    }

    // --- Finding I1: bounded worker pool, exercised through the real `ModelServer::run` -----

    #[test]
    fn test_server_run_bounded_pool_serves_many_concurrent_requests() {
        // Unlike `create_test_server` (which hand-rolls a single-threaded request loop just
        // for test convenience), this drives the real `ModelServer::run` -- the code path
        // that now uses a fixed-size worker pool instead of one `thread::spawn` per
        // connection. This doesn't reproduce the pentest's slow-body-never-finishes scenario
        // (deliberately: that needs a real stalled TCP stream and a timeout, which is hard to
        // make fast/deterministic here -- see the report for what was verified manually
        // instead). What it does prove: the refactored `run()` doesn't panic and still
        // correctly serves a burst of concurrent requests larger than a single connection.
        let mut sources = HashMap::new();
        sources.insert(
            "geom".to_string(),
            "fn area_tubo(d: m) = 3.1415926535 * (d / 2)^2\n".to_string(),
        );
        let config = ModelServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            auth_token: None,
        };
        let model_server = ModelServer::new(sources, config).unwrap();
        let http_server = Server::http("127.0.0.1:0").unwrap();
        let addr = http_server.server_addr().to_ip().unwrap();
        let base_url = format!("http://{}", addr);

        std::thread::spawn(move || {
            let _ = model_server.run(http_server);
        });

        let handles: Vec<_> = (0..64)
            .map(|_| {
                let url = base_url.clone();
                std::thread::spawn(move || -> serde_json::Value {
                    ureq::post(&format!("{}/api/v1/geom/area_tubo", url))
                        .send_json(serde_json::json!({ "d": "0.05 m" }))
                        .expect("request must succeed")
                        .into_json()
                        .expect("response must be valid JSON")
                })
            })
            .collect();

        let mut ok_count = 0;
        for h in handles {
            let body = h.join().expect("request thread must not panic");
            assert_eq!(body["status"], "success");
            ok_count += 1;
        }
        assert_eq!(ok_count, 64);
    }
}

