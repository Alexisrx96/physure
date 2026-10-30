# Draft: a validation surface for agents and language models (N5)

**Status:** DRAFT, not approved. Source: next step N5 of `2026-10-30-post-a1-inventory.md`. It builds on the proposal in `docs/LLM_SLM_SYNTAX_EXPOSURE_SPEC.md`, whose own header says "Architecture Proposal / Next Steps".

## What exists, and what does not

- `docs/LLM_SLM_SYNTAX_EXPOSURE_SPEC.md` (139 lines, August 2026) is a proposal for exposing the grammar, the unit registry and the built-in functions of PHS to models. Nothing in it is implemented as a product feature.
- `.mcp.json` at the repository root configures **the SonarQube MCP server for development** (`sonarqube-api-mcp`, project key `physure`). It is not Physure support for MCP. A search of the Rust crates and the Python package found no MCP server code.
- `phs doc [--save]` already generates the full Markdown language and syntax specification from the implementation, which is the "always up to date" mechanism the proposal asks for.
- `phs serve [dir|file] [--port] [--host] [--token]` runs a model REST server; `phs pack ... --allow-execution` shows that running code from a package is already treated as something to opt into.
- The CLI has no `check` or `validate` subcommand. `phs <script.phs>` runs the script.
- Python can load and call PHS (`physure.module`, `load_phs`).

## Goal

Give an agent a deterministic way to ask "is this equation dimensionally right, and what is its value with units and uncertainty?" and get a structured answer instead of free text, so a model checks its work against the engine instead of recalling numbers.

## Options

| Option | Surface | For | Against |
|---|---|---|---|
| A | `phs eval --json` and `phs check --json` in `physure-cli`: input from a file or stdin, output `{value, unit, uncertainty, diagnostics}` | smallest; reuses the interpreter; any agent that can run a command can use it | one process per call; `check` is only as good as N2 |
| B | An MCP server exposing tools (`evaluate`, `convert`, `check_dimensions`, `describe_units`) and the grammar as a resource | the native way for MCP-capable agents | a new binary and protocol surface to maintain and secure |
| C | Document `phs serve` as the agent surface and publish its schema | no new code if the REST server already fits | to be checked against what it returns today |

## Recommendation

A first. Then B as a thin wrapper over A's JSON, if the maintainer wants MCP. `check` is useful from day one for what the interpreter can already tell without running (names, unit syntax) and improves when N2 lands.

## The safety question

PHS can load native plugins (`plugin.rs`, a native plugin ABI), read packages and serve over HTTP. An outside caller must not get that by default. The surface needs a **safe subset** that is checked, not assumed: no plugins, no file or network access, bounded iterations (the settings `max_loop_iterations` and `max_pipeline_steps` already exist), a timeout, and an output size limit.

## Open questions

- What is in the safe subset, exactly, and is it enforced by a flag of the existing interpreter or by a separate entry point?
- Is a determinism guarantee wanted (same input, same output, including for Monte Carlo propagation, which would need a fixed seed)?
- Does the maintainer want MCP at all, or is a command-line tool enough for the audience?
- Should `phs doc` also emit the machine-readable grammar and unit list the proposal asks for?

## Acceptance

- A documented JSON schema for the output, with a test that parses it for success, for a dimensional error, for a syntax error and for an unknown unit.
- A test that a script using a plugin, a file read or an endless loop is refused or stopped in safe mode.
- No change to the behaviour of `phs <script.phs>` without the new flags.

## Size and dependencies

Size M for A, risk medium (executing code from outside). Better after N2 for `check`; the conformance contract of F3 helps before anything is exposed to outside callers.
