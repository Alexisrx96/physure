use std::collections::HashMap;
use std::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use physure_script::PhsLexer;

struct Backend {
    client: Client,
    documents: RwLock<HashMap<Url, String>>,
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

    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
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
            ("let", "let var = expr1 in expr2", "Local variable binding"),
            ("in", "in expr2", "Local binding scope boundary"),
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

        // 3. Units
        let units = vec![
            ("m", "Length [L]"), ("kg", "Mass [M]"), ("s", "Time [T]"),
            ("A", "Current [I]"), ("K", "Temperature [Θ]"), ("mol", "Amount [N]"),
            ("N", "Force [M·L·T⁻²]"), ("Pa", "Pressure [M·L⁻¹·T⁻²]"),
            ("J", "Energy [M·L²·T⁻²]"), ("W", "Power [M·L²·T⁻³]"),
            ("C", "Charge [I·T]"), ("V", "Potential [M·L²·T⁻³·I⁻¹]"),
            ("Hz", "Frequency [T⁻¹]"), ("m/s", "Velocity [L·T⁻¹]"),
            ("m/s^2", "Acceleration [L·T⁻²]"), ("nm", "Length [L]"),
            ("cm", "Length [L]"), ("km", "Length [L]"), ("kPa", "Pressure"),
            ("MPa", "Pressure"), ("kJ", "Energy"), ("kW", "Power"),
        ];

        for (u, desc) in units {
            items.push(CompletionItem {
                label: u.to_string(),
                kind: Some(CompletionItemKind::UNIT),
                detail: Some(desc.to_string()),
                documentation: Some(Documentation::String(format!("Physure physical unit `{}`", u))),
                sort_text: Some(format!("z_unit_{}", u)),
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
            }
        }
        Ok(None)
    }
}

impl Backend {
    async fn on_change(&self, uri: Url, text: String) {
        let mut diagnostics = Vec::new();

        for (line_idx, line) in text.lines().enumerate() {
            let code_line = if let Some(idx) = line.find('#') {
                &line[..idx]
            } else {
                line
            };

            if code_line.trim().is_empty() || code_line.trim().starts_with("```") {
                continue;
            }

            // High-speed tokenization using Rust PhsLexer
            let lexer = PhsLexer::new(code_line);
            match lexer.tokenize() {
                Ok(tokens) => {
                    let mut paren_depth: i32 = 0;
                    for t in tokens {
                        if t.value == "(" {
                            paren_depth += 1;
                        } else if t.value == ")" {
                            paren_depth -= 1;
                            if paren_depth < 0 {
                                diagnostics.push(Diagnostic {
                                    range: Range {
                                        start: Position { line: line_idx as u32, character: t.pos as u32 },
                                        end: Position { line: line_idx as u32, character: (t.pos + 1) as u32 },
                                    },
                                    severity: Some(DiagnosticSeverity::ERROR),
                                    code: None,
                                    code_description: None,
                                    source: Some("physure-lsp".to_string()),
                                    message: "Mismatched closing parenthesis ')'".to_string(),
                                    related_information: None,
                                    tags: None,
                                    data: None,
                                });
                                paren_depth = 0;
                            }
                        }
                    }
                    if paren_depth > 0 {
                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position { line: line_idx as u32, character: 0 },
                                end: Position { line: line_idx as u32, character: line.len() as u32 },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: None,
                            code_description: None,
                            source: Some("physure-lsp".to_string()),
                            message: "Unbalanced parentheses: expected closing ')'".to_string(),
                            related_information: None,
                            tags: None,
                            data: None,
                        });
                    }
                }
                Err(err) => {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position { line: line_idx as u32, character: 0 },
                            end: Position { line: line_idx as u32, character: line.len() as u32 },
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: None,
                        code_description: None,
                        source: Some("physure-lsp".to_string()),
                        message: format!("Syntax Error: {}", err),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                }
            }
        }

        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }
}

fn extract_word_at_pos(line: &str, char_idx: usize) -> String {
    let bytes = line.as_bytes();
    if char_idx >= bytes.len() {
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
        "solve" => Some("**Built-in Function**: `solve(equation, target)`\n\nSolves an equation symbolically for target variable.\n\nRequires `use solve from calc`.".to_string()),
        "deriv" | "diff" => Some("**Built-in Function**: `deriv(expression, variable)`\n\nDifferentiates a mathematical expression symbolically.\n\nRequires `use deriv from calc` (or `use diff from calc`).".to_string()),
        "integral" | "integrate" => Some("**Built-in Function**: `integral(expression, variable)`\n\nComputes indefinite integral symbolically.\n\nRequires `use integral from calc` (or `use integrate from calc`).".to_string()),
        "gradient" => Some("**Built-in Function**: `gradient(y_array, x_array)`\n\nComputes numerical derivative dy/dx across vector arrays.\n\nRequires `use gradient from array`.".to_string()),
        "trapz" => Some("**Built-in Function**: `trapz(y_array, x_array)`\n\nComputes trapezoidal numerical integration across vector arrays.\n\nRequires `use trapz from array`.".to_string()),
        "linspace" => Some("**Built-in Function**: `linspace(start, stop, n)`\n\nReturns a vector of evenly spaced quantities.\n\nRequires `use linspace from array`.".to_string()),
        "plot" => Some("**Built-in Function**: `plot(x_array, y_array)`\n\nPlots one vector array against another.\n\nRequires `use plot from plot`.\n\n(`plot` is also the name of the domain that must be `use`d to unlock this function.)".to_string()),
        "if" => Some("**PHS Keyword**: `if`\n\nConditional expression construct: `if cond then expr1 else expr2`".to_string()),
        "then" => Some("**PHS Keyword**: `then`\n\nConditional then-branch.".to_string()),
        "else" => Some("**PHS Keyword**: `else`\n\nConditional else-branch.".to_string()),
        "let" => Some("**PHS Keyword**: `let`\n\nLocal variable binding construct.".to_string()),
        "use" => Some("**PHS Keyword**: `use`\n\nImports name(s) into scope: `use name1[, name2, ...] from <domain|module>` or `use * from <domain|module>`.\n\nRequired before calling non-core builtins (`calc`, `plot`, `array` domains) or functions from a `.phs` module, native plugin, or Python ext file.".to_string()),
        "from" => Some("**PHS Keyword**: `from`\n\nSource clause of a `use` statement: `use name from <domain|module>`".to_string()),
        "as" => Some("**PHS Keyword**: `as`\n\nAliases an imported name: `use name as alias from <domain|module>`".to_string()),
        "import" => Some("**PHS Keyword**: `import`\n\nImports an entire module: `import \"path/to/module\" [as alias]`".to_string()),
        "export" => Some("**PHS Keyword**: `export`\n\nExports a name from the current module: `export name [as alias]`".to_string()),
        "core" => Some("**Builtin domain**: `core`\n\nAlways available without `use`: format, comparisons, ternary, vector, sqrt, sin, cos, exp, ln, abs, log, tan, floor, ceil, min, max, round.".to_string()),
        "calc" => Some("**Builtin domain**: `calc`\n\n`use ... from calc` unlocks: deriv, diff, integral, integrate, solve.".to_string()),
        "array" => Some("**Builtin domain**: `array`\n\n`use ... from array` unlocks: linspace, gradient, trapz.".to_string()),
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

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: RwLock::new(HashMap::new()),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
