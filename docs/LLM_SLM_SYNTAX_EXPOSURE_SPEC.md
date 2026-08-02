# 🤖 Exposing PHS Syntax to LLMs and SLMs — Architecture & Roadmap Specification

**Status**: Architecture Proposal / Next Steps  
**Author**: Alexisrx96 / Irvin Torres & Antigravity AI  
**Date**: August 2026  
**Project**: Physure Meta-Lang (PHS)

---

## 🎯 General Objective

Establish a standardized, automated, and always up-to-date mechanism to expose the syntax, EBNF/Pest grammar, physical unit registry, and built-in function modules of **Physure Script (PHS)** to **Large Language Models (LLMs)** and **Small Language Models (SLMs)** (such as Llama 3, Mistral, Qwen, DeepSeek, Claude, and GPT-4), ensuring that AI-generated PHS code is 100% valid and free of ambiguities.

---

## 🏗️ 4-Layer Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            Physure Core & Engine                            │
│           (SI Unit Registry, Rust Interpreter, Pest Grammar)               │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
            ┌──────────────────────────┼──────────────────────────┐
            ▼                          ▼                          ▼
┌───────────────────────┐  ┌───────────────────────┐  ┌───────────────────────┐
│ 1. CLI Spec & Prompt  │  │ 2. MCP Server Protocol│  │ 3. CI/CD Sync Pipeline│
│   `phs spec --prompt` │  │     `phs mcp`          │  │  GitHub Actions / Docs│
└───────────┬───────────┘  └───────────┬───────────┘  └───────────┬───────────┘
            │                          │                          │
            └──────────────────────────┼──────────────────────────┘
                                       ▼
                     ┌───────────────────────────────────┐
                     │    4. LLMs / SLMs / AI Agents    │
                     │  (Generates 100% synchronized &   │
                     │       valid PHS code)             │
                     └───────────────────────────────────┘
```

---

## 1. CLI Spec & Prompt Context Generator (`phs spec`)

### 1.1 Command `phs spec --prompt`
Generates a condensed Markdown snippet (optimized to fit below ~1,200 tokens) to inject as a *System Instruction* in LLM API calls or AI development assistants:

```bash
phs spec --prompt
```

### 1.2 Command `phs spec --json`
Generates a structured JSON object with the complete syntax specification:
- Registered unit symbols and their SI base dimensions.
- Built-in functions grouped by domain (`calc`, `array`, `plot`, `core`).
- Operator precedence and unit disambiguation rules.

```bash
phs spec --json > docs/phs_schema.json
```

---

## 2. Native MCP Server (`phs mcp`)

Implement a `phs mcp` subcommand that runs a server using the **Model Context Protocol (MCP)** for AI coding agents:

### Exposed MCP Tools:
1. **`phs_validate_script`**:
   - **Parameters**: `code` (string).
   - **Response**: Validates syntax, unit disambiguation, and dimensional compatibility, returning structured JSON diagnostics.
2. **`phs_get_grammar_rules`**:
   - **Response**: Returns current Pest grammar rules in real-time.
3. **`phs_lookup_unit`**:
   - **Parameters**: `symbol` (string).
   - **Response**: Returns category, SI base dimensions, and scale factor.

---

## 3. CI/CD Integration Pipeline

To ensure that the syntax documentation for LLMs remains **100% synchronized** with every commit and release:

### GitHub Actions Workflow (`.github/workflows/sync-llm-spec.yml`):
1. On `push` to the `main` branch or on release tag creation:
2. Build `physure-cli`.
3. Execute `./phs doc --save docs/PHS_SPEC.md` and `./phs spec --json > docs/phs_schema.json`.
4. Automatically commit updated specification files back to the repository.

---

## 4. Standard System Prompt Template for LLMs/SLMs (`docs/PHS_LLM_SYSTEM_PROMPT.md`)

````markdown
# PHS (Physure Script) Language Specification for AI Assistants

You are an expert generator of Physure Script (PHS) code. Follow these strict rules:

## 1. Quantities & Units
- Quantities have a magnitude, optional uncertainty, and physical unit expression:
  `pressure = 100.0 kPa`
  `velocity = 25.0 m / s`
- Measurement uncertainty: `g = 9.81 +/- 0.05 m / s ^ 2`.
- Asymmetric uncertainty: `d = 12.3 +/- (0.5, 0.4) pb`.
- Shadowed unit disambiguation: If a variable `s` is bound earlier in the script, quote the unit:
  `g_exp = 9.81 m / "s ^ 2"` (quotes keep `s` as a unit second instead of variable `s`).

## 2. Unit Conversions
Use the `=>` operator: `v_kmh = 25.0 m / s => km / h`.

## 3. Function Definitions & Algebra
- Direct math functions: `P(x, y) = 100.0 kPa * sin(x / 1.0 m) * cos(y / 1.0 m)`.
- Typed parameter functions: `fn E_k(m: kg, v: m / s) = 0.5 * m * v ^ 2`.
- Function arithmetic: `h = f + g`, `s_fn = g - f`, `c = f(g)`.

## 4. Method Syntax
- Equation solving:
  `eq = "P * V = n * R * T"`
  `eq_T = eq.solve("T")`
- Symbolic calculus:
  `d_pos = "v0 * t + 0.5 * a * t ^ 2".deriv("t")`
  `i_acc = "g * t".integral("t")`
- Vector array operations:
  `v_num = x_vec.gradient(t_vec)`
  `W_total = F_vec.trapz(pos_vec)`

## 5. Domain Imports
- `use * from calc` (unlocked `solve`, `deriv`, `integral`).
- `use * from array` (unlocked `linspace`, `gradient`, `trapz`).
- `use * from plot` (unlocked `plot3d`, `export3d`).
````

---

## 📅 Roadmap & Next Steps

- [ ] **Stage 1**: Add `--prompt` and `--json` flags to the `phs doc` / `phs spec` subcommand in `physure-cli`.
- [ ] **Stage 2**: Create the `docs/PHS_LLM_SYSTEM_PROMPT.md` template file for AI assistant integration.
- [ ] **Stage 3**: Configure the GitHub Actions workflow to keep `docs/PHS_SPEC.md` synchronized on every push.
- [ ] **Stage 4**: Implement the `phs mcp` subcommand in Rust for native Model Context Protocol support.
