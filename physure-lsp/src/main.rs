mod incremental;

use std::collections::HashMap;
use std::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct Backend {
    client: Client,
    documents: RwLock<HashMap<Url, String>>,
    doc_states: RwLock<HashMap<Url, incremental::DocState>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        ":".to_string(),
                        "|".to_string(),
                        ".".to_string(),
                        "\\".to_string(),
                        " ".to_string(),
                    ]),
                    completion_item: None,
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "physure-lsp".to_string(),
                version: Some("0.2.1".to_string()),
            }),
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.documents.write().unwrap().insert(uri.clone(), text.clone());
        self.on_change(uri, text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().last() {
            let text = change.text;
            self.documents.write().unwrap().insert(uri.clone(), text.clone());
            self.on_change(uri, text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let text_opt = self
            .documents
            .read()
            .unwrap()
            .get(&params.text_document.uri)
            .cloned();
        if let Some(text) = text_opt {
            self.on_change(params.text_document.uri, text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().unwrap().remove(&uri);
        self.doc_states.write().unwrap().remove(&uri);
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let trigger_character = params.context.and_then(|c| c.trigger_character);

        let line_prefix = self.documents.read().unwrap().get(&uri).and_then(|text| {
            text.lines()
                .nth(pos.line as usize)
                .map(|line| line.chars().take(pos.character as usize).collect::<String>())
        });

        if let Some(prefix) = line_prefix {
            match use_statement_context(&prefix) {
                UseContext::FromTarget => {
                    return Ok(Some(CompletionResponse::Array(from_target_completions(&uri))));
                }
                UseContext::Names => {
                    return Ok(Some(CompletionResponse::Array(use_name_completions())));
                }
                UseContext::None => {}
            }
        }

        if trigger_character.as_deref() == Some(" ") {
            // Space is only registered as a trigger character so the `use`/`from` cases above
            // pop up automatically; elsewhere a bare space shouldn't spam the full list.
            return Ok(Some(CompletionResponse::Array(Vec::new())));
        }

        let mut items = Vec::new();

        // 1. Built-in Functions
        let builtins = vec![
            ("abs", "abs(x)", "Absolute value of a physical quantity"),
            ("round", "round(x, ndigits?)", "Round quantity to decimal places"),
            ("sqrt", "sqrt(x)", "Square root of a physical quantity"),
            ("sin", "sin(x)", "Sine of an angle or dimensionless quantity"),
            ("cos", "cos(x)", "Cosine of an angle or dimensionless quantity"),
            ("tan", "tan(x)", "Tangent of an angle or dimensionless quantity"),
            ("exp", "exp(x)", "Exponential e^x"),
            ("log", "log(x)", "Natural logarithm"),
            ("ln", "ln(x)", "Natural logarithm (alias)"),
            ("solve", "solve(equation, target)", "Solve an equation symbolically — requires `use solve from calc`"),
            ("deriv", "deriv(expression, variable)", "Symbolic derivative — requires `use deriv from calc`"),
            ("diff", "diff(expression, variable)", "Symbolic derivative (alias) — requires `use diff from calc`"),
            ("integral", "integral(expression, variable)", "Symbolic indefinite integral — requires `use integral from calc`"),
            ("integrate", "integrate(expression, variable)", "Symbolic indefinite integral (alias) — requires `use integrate from calc`"),
            ("gradient", "gradient(y_array, x_array)", "Numerical derivative dy/dx for vector data — requires `use gradient from array`"),
            ("trapz", "trapz(y_array, x_array)", "Numerical integration (area under curve) — requires `use trapz from array`"),
            ("linspace", "linspace(start, stop, n)", "Evenly spaced vector range — requires `use linspace from array`"),
            ("plot", "plot(x_array, y_array)", "Plot one vector array against another — requires `use plot from plot`"),
        ];

        for (name, label, doc) in builtins {
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(label.to_string()),
                documentation: Some(Documentation::String(doc.to_string())),
                insert_text: Some(format!("{}($1)", name)),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                sort_text: Some(format!("m_builtin_{}", name)),
                ..Default::default()
            });
        }

        // 2. Control Keywords
        let keywords = vec![
            ("where", "expr where var = value", "Local binding, `a * b where a = 2 m, b = 3 m`"),
            ("if", "if cond then expr1 else expr2", "Conditional expression"),
            ("then", "then expr1", "Conditional then branch"),
            ("else", "else expr2", "Conditional else branch"),
            ("use", "use name from <domain|module>", "Import name(s) from a domain (calc/plot/array), .phs module, or plugin/ext file"),
            ("from", "use name from <domain|module>", "Source clause of a `use` statement"),
            ("as", "use name as alias from <domain|module>", "Aliases an imported name"),
            ("import", "import \"path/to/module\"", "Imports an entire module"),
            ("export", "export name", "Exports a name from the current module"),
        ];

        for (kw, label, doc) in keywords {
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some(label.to_string()),
                documentation: Some(Documentation::String(doc.to_string())),
                sort_text: Some(format!("m_keyword_{}", kw)),
                ..Default::default()
            });
        }

        // 2b. Builtin domains (targets of `use ... from <domain>`)
        let domains = vec![
            ("core", "Always-available builtins: format, comparisons, ternary, vector, sqrt, sin, cos, exp, ln, abs, log, tan, floor, ceil, min, max, round"),
            ("calc", "Symbolic calculus: deriv, diff, integral, integrate, solve"),
            ("plot", "Plotting: plot"),
            ("array", "Array/numeric helpers: linspace, gradient, trapz"),
        ];

        for (name, doc) in domains {
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::MODULE),
                detail: Some(format!("domain '{}'", name)),
                documentation: Some(Documentation::String(doc.to_string())),
                sort_text: Some(format!("m_domain_{}", name)),
                ..Default::default()
            });
        }

        // 3. Physical Units & Aliases dynamically loaded from UnitRegistry
        let (registry, _) = physure_core::units::conf::build_registry_from_conf();
        let mut seen = std::collections::HashSet::new();

        // 3a. Primary Base Units
        for name in registry.base_units.keys() {
            if seen.insert(name.clone()) {
                let meta = registry.unit_meta.get(name);
                let category = meta.and_then(|m| m.category.as_deref()).unwrap_or("Base Unit");
                let desc = meta.and_then(|m| m.description.as_deref()).unwrap_or("");
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::UNIT),
                    detail: Some(format!("{} • Base Unit", category)),
                    documentation: Some(Documentation::String(format!("Physure base unit `{}`. {}", name, desc))),
                    sort_text: Some(format!("z_unit_0_{}", name)),
                    ..Default::default()
                });
            }
        }

        // 3b. Derived & Scaled Units (V, Pa, Ohm, Ω, kPa, MPa, kN, kJ, kW, etc.)
        for (name, unit) in &registry.derived_units {
            if seen.insert(name.clone()) {
                let meta = registry.unit_meta.get(name);
                let category = meta.and_then(|m| m.category.as_deref()).unwrap_or("Derived Unit");
                let desc = meta.and_then(|m| m.description.as_deref()).unwrap_or("");
                let dim_str = unit.base_repr();
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::UNIT),
                    detail: Some(format!("{} • {}", category, if dim_str.is_empty() { "Unit".into() } else { dim_str })),
                    documentation: Some(Documentation::String(format!("Physure physical unit `{}`. {}", name, desc))),
                    sort_text: Some(format!("z_unit_1_{}", name)),
                    ..Default::default()
                });
            }
        }

        // 3c. Unit Aliases (Volts, Volt, Pascal, Pascals, ohmio, ohmios, voltio, voltios, etc.)
        for (alias, target_symbol) in &registry.aliases {
            if seen.insert(alias.clone()) {
                let meta = registry.unit_meta.get(target_symbol);
                let category = meta.and_then(|m| m.category.as_deref()).unwrap_or("Unit Alias");
                items.push(CompletionItem {
                    label: alias.clone(),
                    kind: Some(CompletionItemKind::UNIT),
                    detail: Some(format!("Alias for {} ({})", target_symbol, category)),
                    documentation: Some(Documentation::String(format!("Physure unit alias `{}` -> `{}`", alias, target_symbol))),
                    sort_text: Some(format!("z_unit_2_{}", alias)),
                    ..Default::default()
                });
            }
        }

        // 4. Greek Letters & Mathematical Symbols (Unicode, names & LaTeX aliases)
        let greek_symbols = vec![
            ("Δ", "Δ", "Delta", "Greek letter Delta (Difference / Change / Variation)"),
            ("delta", "Δ", "Delta (-> Δ)", "Inserts Greek letter Delta (Δ)"),
            ("Delta", "Δ", "Delta (-> Δ)", "Inserts Greek letter Delta (Δ)"),
            ("\\delta", "Δ", "LaTeX \\delta (-> Δ)", "Inserts Greek letter Delta (Δ)"),
            ("σ", "σ", "Sigma", "Greek letter Sigma (Uncertainty / Standard Deviation / Stress)"),
            ("sigma", "σ", "Sigma (-> σ)", "Inserts Greek letter Sigma (σ)"),
            ("\\sigma", "σ", "LaTeX \\sigma (-> σ)", "Inserts Greek letter Sigma (σ)"),
            ("Ω", "Ω", "Omega", "Greek letter Omega (Electric resistance unit Ohm)"),
            ("omega", "Ω", "Omega (-> Ω)", "Inserts Greek letter Omega (Ω)"),
            ("Omega", "Ω", "Omega (-> Ω)", "Inserts Greek letter Omega (Ω)"),
            ("\\omega", "ω", "LaTeX \\omega (-> ω)", "Inserts Greek letter lowercase Omega (ω)"),
            ("\\Omega", "Ω", "LaTeX \\Omega (-> Ω)", "Inserts Greek letter capital Omega (Ω)"),
            ("π", "π", "Pi", "Greek letter Pi (3.14159...)"),
            ("pi", "π", "Pi (-> π)", "Inserts Greek letter Pi (π)"),
            ("\\pi", "π", "LaTeX \\pi (-> π)", "Inserts Greek letter Pi (π)"),
            ("θ", "θ", "Theta", "Greek letter Theta (Angle / Temperature)"),
            ("theta", "θ", "Theta (-> θ)", "Inserts Greek letter Theta (θ)"),
            ("\\theta", "θ", "LaTeX \\theta (-> θ)", "Inserts Greek letter Theta (θ)"),
            ("λ", "λ", "Lambda", "Greek letter Lambda (Wavelength)"),
            ("lambda", "λ", "Lambda (-> λ)", "Inserts Greek letter Lambda (λ)"),
            ("\\lambda", "λ", "LaTeX \\lambda (-> λ)", "Inserts Greek letter Lambda (λ)"),
            ("μ", "μ", "Mu / Micro", "Greek letter Mu / Micro prefix"),
            ("mu", "μ", "Mu (-> μ)", "Inserts Greek letter Mu (μ)"),
            ("micro", "μ", "Micro (-> μ)", "Inserts Micro prefix (μ)"),
            ("\\mu", "μ", "LaTeX \\mu (-> μ)", "Inserts Greek letter Mu (μ)"),
            ("α", "α", "Alpha", "Greek letter Alpha (Coefficient)"),
            ("alpha", "α", "Alpha (-> α)", "Inserts Greek letter Alpha (α)"),
            ("\\alpha", "α", "LaTeX \\alpha (-> α)", "Inserts Greek letter Alpha (α)"),
            ("β", "β", "Beta", "Greek letter Beta"),
            ("beta", "β", "Beta (-> β)", "Inserts Greek letter Beta (β)"),
            ("\\beta", "β", "LaTeX \\beta (-> β)", "Inserts Greek letter Beta (β)"),
            ("γ", "γ", "Gamma", "Greek letter Gamma"),
            ("gamma", "γ", "Gamma (-> γ)", "Inserts Greek letter Gamma (γ)"),
            ("\\gamma", "γ", "LaTeX \\gamma (-> γ)", "Inserts Greek letter Gamma (γ)"),
            ("ε", "ε", "Epsilon", "Greek letter Epsilon (Permittivity / Strain)"),
            ("epsilon", "ε", "Epsilon (-> ε)", "Inserts Greek letter Epsilon (ε)"),
            ("\\epsilon", "ε", "LaTeX \\epsilon (-> ε)", "Inserts Greek letter Epsilon (ε)"),
            ("η", "η", "Eta", "Greek letter Eta (Efficiency)"),
            ("eta", "η", "Eta (-> η)", "Inserts Greek letter Eta (η)"),
            ("\\eta", "η", "LaTeX \\eta (-> η)", "Inserts Greek letter Eta (η)"),
            ("ρ", "ρ", "Rho", "Greek letter Rho (Density / Resistivity)"),
            ("rho", "ρ", "Rho (-> ρ)", "Inserts Greek letter Rho (ρ)"),
            ("\\rho", "ρ", "LaTeX \\rho (-> ρ)", "Inserts Greek letter Rho (ρ)"),
            ("τ", "τ", "Tau", "Greek letter Tau (Torque / Time constant)"),
            ("tau", "τ", "Tau (-> τ)", "Inserts Greek letter Tau (τ)"),
            ("\\tau", "τ", "LaTeX \\tau (-> τ)", "Inserts Greek letter Tau (τ)"),
            ("ϕ", "ϕ", "Phi", "Greek letter Phi (Magnetic flux / Phase)"),
            ("phi", "ϕ", "Phi (-> ϕ)", "Inserts Greek letter Phi (ϕ)"),
            ("\\phi", "ϕ", "LaTeX \\phi (-> ϕ)", "Inserts Greek letter Phi (ϕ)"),
            ("ψ", "ψ", "Psi", "Greek letter Psi (Wavefunction)"),
            ("psi", "ψ", "Psi (-> ψ)", "Inserts Greek letter Psi (ψ)"),
            ("\\psi", "ψ", "LaTeX \\psi (-> ψ)", "Inserts Greek letter Psi (ψ)"),
            ("ω", "ω", "Omega (lowercase)", "Greek letter lowercase Omega (Angular frequency)"),
            ("∞", "∞", "Infinity", "Infinity symbol ∞"),
            ("infinity", "∞", "Infinity (-> ∞)", "Inserts infinity symbol ∞"),
            ("\\infty", "∞", "LaTeX \\infty (-> ∞)", "Inserts infinity symbol ∞"),
            ("±", "±", "Plus-Minus", "Uncertainty operator ±"),
            ("+/-", "±", "Plus-Minus (-> ±)", "Inserts uncertainty operator ±"),
            ("\\pm", "±", "LaTeX \\pm (-> ±)", "Inserts uncertainty operator ±"),
            ("Å", "Å", "Angstrom", "Angstrom length unit Å"),
            ("°", "°", "Degree", "Degree angle symbol °"),
        ];

        for (label, insert_text, detail, doc) in greek_symbols {
            items.push(CompletionItem {
                label: label.to_string(),
                kind: Some(CompletionItemKind::CONSTANT),
                detail: Some(format!("Greek Symbol • {}", detail)),
                documentation: Some(Documentation::String(doc.to_string())),
                insert_text: Some(insert_text.to_string()),
                sort_text: Some(format!("z_greek_{}", label)),
                ..Default::default()
            });
        }

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let text_opt = self.documents.read().unwrap().get(&uri).cloned();
        if let Some(text) = text_opt {
            let line = text.lines().nth(pos.line as usize).unwrap_or("");
            let word = extract_word_at_pos(line, pos.character as usize);

            if let Some(doc) = lookup_hover_doc(&word) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: doc,
                    }),
                    range: None,
                }));
            } else if let Some(user_doc) = extract_user_docstring(&text, &word) {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: user_doc,
                    }),
                    range: None,
                }));
            }
        }
        Ok(None)
    }
}

impl Backend {
    async fn on_change(&self, uri: Url, text: String) {
        // Take ownership of any previous state before the panic guard: on a panic the
        // closure's argument is dropped along with the unwind, which correctly leaves no
        // entry behind (next edit falls back to a full bootstrap run, the same graceful
        // degradation as today).
        let prev = self.doc_states.write().unwrap().remove(&uri);

        // Analysing a half-typed buffer must never take the process down. A panic here used to
        // exit(101); the client restarts a few times, then gives up and the user loses
        // diagnostics for the rest of the session. Degrade to one diagnostic instead.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            incremental::apply_change(prev, &text)
        }));

        let diagnostics = match outcome {
            Ok(outcome) => {
                self.doc_states.write().unwrap().insert(uri.clone(), outcome.state);
                outcome.diagnostics
            }
            Err(_) => vec![Diagnostic {
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 1 },
                },
                severity: Some(DiagnosticSeverity::WARNING),
                code: None,
                code_description: None,
                source: Some("physure-lsp".to_string()),
                message: "Internal error while analysing this file — \
                          diagnostics are unavailable until it changes again. \
                          Please report the buffer contents."
                    .to_string(),
                related_information: None,
                tags: None,
                data: None,
            }],
        };

        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }
}

enum UseContext {
    /// Cursor is between `use` and `from`, expecting imported name(s).
    Names,
    /// Cursor is after `from`, expecting a domain/module/plugin target.
    FromTarget,
    None,
}

/// Classifies the cursor position within a `use name[, ...] [as alias] from <target>` statement,
/// based on the tokens of the line up to the cursor.
fn use_statement_context(line_prefix: &str) -> UseContext {
    let tokens: Vec<&str> = line_prefix.split_whitespace().collect();
    if tokens.first() != Some(&"use") {
        return UseContext::None;
    }
    if tokens.iter().any(|t| *t == "from") {
        UseContext::FromTarget
    } else {
        UseContext::Names
    }
}

/// Completions for the name(s)/wildcard position of a `use` statement: the members of every
/// gated builtin domain (`calc`/`plot`/`array`; `core` is always-on and never a `use` target).
fn use_name_completions() -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for domain in ["calc", "plot", "array"] {
        if let Some(members) = physure_script::builtins::domain_members(domain) {
            for name in members {
                if seen.insert(name.to_string()) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some(format!("from domain '{domain}'")),
                        documentation: Some(Documentation::String(format!(
                            "Member of builtin domain `{domain}`. Completes `use {name} from {domain}`."
                        ))),
                        sort_text: Some(format!("m_use_{domain}_{name}")),
                        ..Default::default()
                    });
                }
            }
        }
    }
    items.push(CompletionItem {
        label: "*".to_string(),
        kind: Some(CompletionItemKind::OPERATOR),
        detail: Some("wildcard import".to_string()),
        documentation: Some(Documentation::String(
            "Imports every member of the target domain or module.".to_string(),
        )),
        sort_text: Some("m_use_wildcard".to_string()),
        ..Default::default()
    });
    items
}

/// Completions for the `from` clause of a `use` statement: gated builtin domains, sibling `.phs`
/// module stems, and native plugin stems discovered under `<dir>/ext/*.<DLL_EXTENSION>`.
/// Python `.py` ext files are intentionally never suggested — they are no longer a valid `use ... from` target.
fn from_target_completions(uri: &Url) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    for (name, doc) in [
        ("calc", "Symbolic calculus: deriv, diff, integral, integrate, solve"),
        ("plot", "Plotting: plot"),
        ("array", "Array/numeric helpers: linspace, gradient, trapz"),
    ] {
        items.push(CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some(format!("builtin domain '{name}'")),
            documentation: Some(Documentation::String(doc.to_string())),
            sort_text: Some(format!("m_from_domain_{name}")),
            ..Default::default()
        });
    }

    if let Ok(path) = uri.to_file_path() {
        if let Some(dir) = path.parent() {
            let current_stem = path.file_stem().and_then(|s| s.to_str());

            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let candidate = entry.path();
                    if candidate.extension().and_then(|e| e.to_str()) != Some("phs") {
                        continue;
                    }
                    let Some(stem) = candidate.file_stem().and_then(|s| s.to_str()) else {
                        continue;
                    };
                    if Some(stem) == current_stem {
                        continue;
                    }
                    items.push(CompletionItem {
                        label: stem.to_string(),
                        kind: Some(CompletionItemKind::MODULE),
                        detail: Some(format!("PhysureScript module `{stem}.phs`")),
                        documentation: Some(Documentation::String(format!(
                            "Local module `{stem}.phs`"
                        ))),
                        sort_text: Some(format!("m_from_module_{stem}")),
                        ..Default::default()
                    });
                }
            }

            let dll_ext = std::env::consts::DLL_EXTENSION;
            if let Ok(entries) = std::fs::read_dir(dir.join("ext")) {
                for entry in entries.flatten() {
                    let candidate = entry.path();
                    if candidate.extension().and_then(|e| e.to_str()) != Some(dll_ext) {
                        continue;
                    }
                    let Some(stem) = candidate.file_stem().and_then(|s| s.to_str()) else {
                        continue;
                    };
                    items.push(CompletionItem {
                        label: stem.to_string(),
                        kind: Some(CompletionItemKind::CLASS),
                        detail: Some(format!("Native plugin `ext/{stem}.{dll_ext}`")),
                        documentation: Some(Documentation::String(format!(
                            "Native `.rs` plugin loaded from `ext/{stem}.{dll_ext}`"
                        ))),
                        sort_text: Some(format!("m_from_plugin_{stem}")),
                        ..Default::default()
                    });
                }
            }
        }
    }

    items
}

// LSP `Position.character` is a UTF-16 code-unit offset, not a byte offset.
fn utf16_offset_to_byte_offset(line: &str, utf16_offset: usize) -> usize {
    let mut utf16_count = 0;
    for (byte_idx, ch) in line.char_indices() {
        if utf16_count >= utf16_offset {
            return byte_idx;
        }
        utf16_count += ch.len_utf16();
    }
    line.len()
}

fn extract_word_at_pos(line: &str, utf16_char_idx: usize) -> String {
    let char_idx = utf16_offset_to_byte_offset(line, utf16_char_idx);
    if char_idx >= line.len() {
        return "".to_string();
    }
    let start = line[..char_idx]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map_or(0, |idx| idx + 1);
    let end = line[char_idx..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map_or(line.len(), |idx| char_idx + idx);
    line[start..end].to_string()
}

fn lookup_hover_doc(word: &str) -> Option<String> {
    match word {
        "solve" => Some("**Built-in Function**: `solve(equation, target)`\n\nSolves an equation symbolically for target variable.\n\n**Domain**: `calc` (requires `use solve from calc`).".to_string()),
        "deriv" | "diff" => Some("**Built-in Function**: `deriv(expression, variable)`\n\nDifferentiates a mathematical expression symbolically.\n\n**Domain**: `calc` (requires `use deriv from calc`).".to_string()),
        "integral" | "integrate" => Some("**Built-in Function**: `integral(expression, variable)`\n\nComputes indefinite integral symbolically.\n\n**Domain**: `calc` (requires `use integral from calc`).".to_string()),
        "gradient" => Some("**Built-in Function**: `gradient(y_array, x_array)`\n\nComputes numerical derivative dy/dx across vector arrays.\n\n**Domain**: `array` (requires `use gradient from array`).".to_string()),
        "trapz" => Some("**Built-in Function**: `trapz(y_array, x_array)`\n\nComputes trapezoidal numerical integration across vector arrays.\n\n**Domain**: `array` (requires `use trapz from array`).".to_string()),
        "linspace" => Some("**Built-in Function**: `linspace(start, stop, n)`\n\nReturns a vector of evenly spaced quantities.\n\n**Domain**: `array` (requires `use linspace from array`).".to_string()),
        "dot" => Some("**Built-in Function**: `dot(vec_a, vec_b)`\n\nComputes dot product of two vectors.\n\n**Domain**: `array` (requires `use dot from array`).".to_string()),
        "cross" => Some("**Built-in Function**: `cross(vec_a, vec_b)`\n\nComputes 3D cross product of two vectors.\n\n**Domain**: `array` (requires `use cross from array`).".to_string()),
        "norm" => Some("**Built-in Function**: `norm(vector)`\n\nComputes Euclidean norm (magnitude) of a vector.\n\n**Domain**: `array` (requires `use norm from array`).".to_string()),
        "transpose" => Some("**Built-in Function**: `transpose(matrix)`\n\nTransposes a 2D matrix.\n\n**Domain**: `array` (requires `use transpose from array`).".to_string()),
        "matmul" => Some("**Built-in Function**: `matmul(matrix_a, matrix_b)`\n\nComputes matrix multiplication of two matrices.\n\n**Domain**: `array` (requires `use matmul from array`).".to_string()),
        "det" => Some("**Built-in Function**: `det(matrix)`\n\nComputes determinant of a square matrix.\n\n**Domain**: `array` (requires `use det from array`).".to_string()),
        "unit_vector" => Some("**Built-in Function**: `unit_vector(vec)`\n\nNormalizes vector to a unit vector.\n\n**Domain**: `array` (requires `use unit_vector from array`).".to_string()),
        "plot" => Some("**Built-in Function**: `plot(x_array, y_array)`\n\nPlots one vector array against another in 2D.\n\n**Domain**: `plot` (requires `use plot from plot`).".to_string()),
        "plot3d" => Some("**Built-in Function**: `plot3d(expr: string, title?: string)`\n\nRenders a 3D physical surface Z = f(x, y) in WebGL with interactive OrbitControls, Colorbar legend, and Raycaster point inspection.\n\n**Domain**: `plot` (requires `use plot3d from plot`).".to_string()),
        "export3d" | "export_3d" => Some("**Built-in Function**: `export3d(expr: string, filename: string, format?: string)`\n\nExports 3D physical surface geometries to standard 3D formats (`html`, `stl`, `obj`, `gltf`, `ply`).\n\n**Domain**: `plot` (requires `use export3d from plot`).".to_string()),
        "substitute" | "sub" => Some("**Built-in Function**: `substitute(expr, var, val)`\n\nSubstitutes variable in symbolic expression.\n\n**Domain**: `calc` (requires `use substitute from calc`).".to_string()),
        "limit" | "lim" => Some("**Built-in Function**: `limit(expr, var, target)`\n\nComputes symbolic limit.\n\n**Domain**: `calc` (requires `use limit from calc`).".to_string()),
        "grad" => Some("**Built-in Function**: `grad(expr, vars)`\n\nComputes symbolic gradient vector.\n\n**Domain**: `calc` (requires `use grad from calc`).".to_string()),
        "div" | "divergence" => Some("**Built-in Function**: `div(vector_field, vars)`\n\nComputes symbolic divergence.\n\n**Domain**: `calc` (requires `use div from calc`).".to_string()),
        "curl" => Some("**Built-in Function**: `curl(vector_field, vars)`\n\nComputes symbolic curl.\n\n**Domain**: `calc` (requires `use curl from calc`).".to_string()),
        "laplacian" => Some("**Built-in Function**: `laplacian(expr, vars)`\n\nComputes symbolic Laplacian.\n\n**Domain**: `calc` (requires `use laplacian from calc`).".to_string()),
        "simplify" => Some("**Built-in Function**: `simplify(expr)`\n\nSimplifies symbolic expression.\n\n**Domain**: `calc` (requires `use simplify from calc`).".to_string()),
        "expand" => Some("**Built-in Function**: `expand(expr)`\n\nExpands algebraic products in expression.\n\n**Domain**: `calc` (requires `use expand from calc`).".to_string()),
        "if" => Some("**PHS Keyword**: `if`\n\nConditional expression construct: `if cond then expr1 else expr2`".to_string()),
        "then" => Some("**PHS Keyword**: `then`\n\nConditional then-branch.".to_string()),
        "else" => Some("**PHS Keyword**: `else`\n\nConditional else-branch.".to_string()),
        "where" => Some("**PHS Keyword**: `where`\n\nLocal binding clause: `expr where name = value[, name2 = value2]`. A later binding can use an earlier one.".to_string()),
        "use" => Some("**PHS Keyword**: `use`\n\nImports name(s) into scope: `use name1[, name2, ...] from <domain|module>` or `use * from <domain|module>`.".to_string()),
        "from" => Some("**PHS Keyword**: `from`\n\nSource clause of a `use` statement: `use name from <domain|module>`".to_string()),
        "as" => Some("**PHS Keyword**: `as`\n\nAliases an imported name: `use name as alias from <domain|module>`".to_string()),
        "import" => Some("**PHS Keyword**: `import`\n\nImports an entire module: `import \"path/to/module\" [as alias]`".to_string()),
        "export" => Some("**PHS Keyword**: `export`\n\nExports a name from the current module: `export name [as alias]`".to_string()),
        "core" => Some("**Builtin domain**: `core`\n\nAlways available without `use`: format, comparisons, ternary, vector, sqrt, sin, cos, exp, ln, abs, log, tan, floor, ceil, min, max, round.".to_string()),
        "calc" => Some("**Builtin domain**: `calc`\n\n`use ... from calc` unlocks: deriv, diff, integral, integrate, solve, substitute, limit, grad, div, curl, laplacian, simplify, expand.".to_string()),
        "array" => Some("**Builtin domain**: `array`\n\n`use ... from array` unlocks: linspace, gradient, trapz, dot, cross, norm, unit_vector, transpose, matmul, det.".to_string()),
        "m" => Some("**Physical Unit**: `m`\n\n* **Quantity**: Length (Longitud)\n* **Dimension**: `[L]`".to_string()),
        "kg" => Some("**Physical Unit**: `kg`\n\n* **Quantity**: Mass (Masa)\n* **Dimension**: `[M]`".to_string()),
        "s" => Some("**Physical Unit**: `s`\n\n* **Quantity**: Time (Tiempo)\n* **Dimension**: `[T]`".to_string()),
        "N" => Some("**Physical Unit**: `N`\n\n* **Quantity**: Force (Fuerza)\n* **SI Base**: `kg·m·s⁻²`\n* **Dimension**: `[M·L·T⁻²]`".to_string()),
        "Pa" => Some("**Physical Unit**: `Pa`\n\n* **Quantity**: Pressure / Stress\n* **SI Base**: `kg·m⁻¹·s⁻²`\n* **Dimension**: `[M·L⁻¹·T⁻²]`".to_string()),
        "J" => Some("**Physical Unit**: `J`\n\n* **Quantity**: Energy / Work\n* **SI Base**: `kg·m²·s⁻²`\n* **Dimension**: `[M·L²·T⁻²]`".to_string()),
        "W" => Some("**Physical Unit**: `W`\n\n* **Quantity**: Power (Potencia)\n* **SI Base**: `kg·m²·s⁻³`\n* **Dimension**: `[M·L²·T⁻³]`".to_string()),
        _ => None,
    }
}

fn normalize_html_to_markdown(input: &str) -> String {
    let mut s = input.to_string();

    // Convert HTML block and inline tags to clean Markdown equivalents for VS Code hover rendering
    s = s.replace("<p>", "\n\n").replace("</p>", "\n\n");
    s = s.replace("<ul>", "\n").replace("</ul>", "\n");
    s = s.replace("<ol>", "\n").replace("</ol>", "\n");
    s = s.replace("<li>", "\n* ").replace("</li>", "");
    s = s.replace("<br>", "\n").replace("<br/>", "\n").replace("<br />", "\n");
    s = s.replace("<b>", "**").replace("</b>", "**");
    s = s.replace("<strong>", "**").replace("</strong>", "**");
    s = s.replace("<i>", "*").replace("</i>", "*");
    s = s.replace("<em>", "*").replace("</em>", "*");
    s = s.replace("<code>", "`").replace("</code>", "`");

    let mut result_lines = Vec::new();
    for line in s.lines() {
        result_lines.push(line.trim_end());
    }
    result_lines.join("\n").trim().to_string()
}

/// Extract user-defined docstrings (`///` doc comments or `'''...'''` / `"""..."""` blocks)
/// for user functions and variables declared in the PHS script.
/// Note: regular `#` comments are ignored as docstrings.
fn extract_user_docstring(text: &str, word: &str) -> Option<String> {
    if word.is_empty() {
        return None;
    }
    let lines: Vec<&str> = text.lines().collect();
    let fn_prefix1 = format!("fn {}", word);
    let var_prefix1 = format!("{} =", word);
    let var_prefix2 = format!("{}=", word);

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let is_fn = trimmed.starts_with(&fn_prefix1) || (trimmed.contains(&format!("{}(", word)) && trimmed.contains('=')) || trimmed.starts_with(&format!("fn {}(", word));
        let is_var = trimmed.starts_with(&var_prefix1) || trimmed.starts_with(&var_prefix2);

        if is_fn || is_var {
            let mut doc_lines = Vec::new();

            // 1. Look upwards ONLY for contiguous `///` doc comment lines (ignoring `#` regular comments)
            if idx > 0 {
                let mut prev_idx = idx;
                while prev_idx > 0 {
                    prev_idx -= 1;
                    let prev_line = lines[prev_idx].trim();
                    if prev_line.starts_with("///") {
                        let content = prev_line.trim_start_matches("///");
                        let clean_line = if content.starts_with(' ') { &content[1..] } else { content };
                        doc_lines.insert(0, clean_line);
                    } else {
                        break;
                    }
                }
            }

            // 2. Look inside function body / definition for `'''` or `"""` docstring block
            let raw_doc_text = if !doc_lines.is_empty() {
                doc_lines.join("\n")
            } else if idx + 1 < lines.len() {
                let next_line = lines[idx + 1].trim();
                if next_line.starts_with("'''") || next_line.starts_with("\"\"\"") {
                    next_line.trim_matches('\'').trim_matches('"').trim().to_string()
                } else {
                    "".to_string()
                }
            } else {
                "".to_string()
            };

            let doc_text = normalize_html_to_markdown(&raw_doc_text);
            let kind = if is_fn { "User Function" } else { "User Variable" };
            let mut markdown = format!("**{}**: `{}`\n\n", kind, trimmed);
            if !doc_text.is_empty() {
                markdown.push_str(&doc_text);
            } else {
                markdown.push_str("*Defined in current PhysureScript module.*");
            }
            return Some(markdown);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_statement_context_detects_names_vs_from_target() {
        assert!(matches!(use_statement_context("let x"), UseContext::None));
        assert!(matches!(use_statement_context("use "), UseContext::Names));
        assert!(matches!(use_statement_context("use solve, deriv"), UseContext::Names));
        assert!(matches!(use_statement_context("use solve from "), UseContext::FromTarget));
        assert!(matches!(use_statement_context("use solve from ca"), UseContext::FromTarget));
    }

    #[test]
    fn use_name_completions_covers_all_gated_domains_plus_wildcard() {
        let items = use_name_completions();
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"solve"));
        assert!(labels.contains(&"plot"));
        assert!(labels.contains(&"linspace"));
        assert!(labels.contains(&"*"));
    }

    #[test]
    fn test_extract_user_docstring_ignores_hash_comments_and_supports_html() {
        let script = r#"
# --- 4. Funciones Definidas por el Usuario y Parámetros con Tipo ---
/// <b>Cálculo de Energía Cinética</b>
/// <p>Calcula la energía de un objeto de masa <i>m</i> a velocidad <i>v</i>.</p>
/// <ul>
///   <li><b>m</b>: Masa del cuerpo [kg]</li>
///   <li><b>v</b>: Velocidad del cuerpo [m/s]</li>
/// </ul>
f(v: m / s) =
    resta = 1 m / s
    v * 2 - resta
"#;
        let doc = extract_user_docstring(script, "f");
        assert!(doc.is_some());
        let doc_str = doc.unwrap();
        assert!(doc_str.contains("**Cálculo de Energía Cinética**"));
        assert!(doc_str.contains("Calcula la energía"));
        assert!(doc_str.contains("* **m**: Masa del cuerpo [kg]"));
        assert!(doc_str.contains("* **v**: Velocidad del cuerpo [m/s]"));
        assert!(!doc_str.contains("Funciones Definidas"));
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: RwLock::new(HashMap::new()),
        doc_states: RwLock::new(HashMap::new()),
    });

    // Serialized, not the default concurrency_level(4): on_change persists a DocState that
    // feeds forward into the next edit's incremental diff, so two edits (or an edit racing a
    // close) for the same document completing out of order could silently corrupt that state
    // -- resurrecting a closed document's entry, or overwriting a newer edit's result with an
    // older one's. tower-lsp dispatches every request AND notification (hover, completion,
    // did_change, did_close, ...) through the same buffer_unordered(concurrency_level) stream,
    // so concurrency_level(1) serializes the whole server, not just edit-related notifications
    // -- a hover/completion request now queues behind an in-flight on_change instead of running
    // alongside it, and this also disables `$/cancelRequest` support (tower-lsp's own doc
    // comment on concurrency_level(1) states this). A per-document lock around just on_change's
    // critical section would avoid both costs, but is real added complexity for a single-user
    // local language server with no real need for concurrent notification handling in the first
    // place -- accepted here as the simpler, safer trade-off, not an oversight.
    Server::new(stdin, stdout, socket)
        .concurrency_level(1)
        .serve(service)
        .await;
}
