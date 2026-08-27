# PHS boolean logic and condition assertions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `True`/`False` boolean literals and `not`/`and`/`or` logical operators to PHS, and extend `assert` to accept a boolean condition (with an optional message) alongside the existing `assert(Quantity, Quantity)`/`exact_assert(Quantity, Quantity)` forms — across the interpreter and all four transpile targets (Python, Rust, Java, JavaScript/TypeScript).

**Architecture:** `Expr::Bool(bool)` is a new AST literal; `not`/`and`/`or` desugar at parse time into `FunctionCall { name: "op_not"/"op_and"/"op_or", .. }`, exactly like the existing comparison operators (`op_>`, `op_==`, ...). The interpreter special-cases these three names *before* its normal eager argument evaluation, so `and`/`or` short-circuit. Codegen recognizes the same shapes through small shared helpers in `codegen/mod.rs` and maps them onto each target's native `!`/`&&`/`||` (all four are natively short-circuit) and native `if (!cond) throw ...` assertion forms. A single classifier (`is_definitely_bool`) — literal, comparison, logical operator, or a previously-classified identifier — lets codegen decide statically which `assert(...)` overload a call site means, since codegen (unlike the interpreter) has no runtime value to inspect.

**Tech Stack:** Rust (`physure-script` crate: `pest`-based parser, tree-walking interpreter, four codegen backends). No changes needed outside `physure-script` — every target's boolean assertion transpiles to plain native code, not a call into `physure-python`/`physure-java`/`physure-wasm`.

**Design spec:** `docs/superpowers/specs/2026-08-26-phs-boolean-logic-and-assertions-design.md`

---

## File Structure

All changes are inside the existing `physure-script` crate; no new files.

| File | Responsibility |
|---|---|
| `physure-script/src/ast.rs` | New `Expr::Bool(bool)` variant. |
| `physure-script/src/phs.pest` | `True`/`False`/`not`/`and`/`or` keywords, `bool_lit` rule, `not_expr`/`and_expr`/`or_expr` precedence tiers. |
| `physure-script/src/parser/expressions.rs` | Parses the new grammar into `Expr::Bool` and `op_not`/`op_and`/`op_or` `FunctionCall`s. |
| `physure-script/src/value.rs` | `PhsValue::type_name()` helper for type-error messages. |
| `physure-script/src/interpreter/expressions.rs` | `Expr::Bool` evaluation; short-circuit `op_not`/`op_and`/`op_or` interception. |
| `physure-script/src/builtins/core.rs` | Bool/Bool branch for `op_==`/`op_!=`; rewritten `assert`/`exact_assert` dispatch. |
| `physure-script/src/codegen/mod.rs` | Shared `as_logical_op`, `is_definitely_bool`, `AssertShape`/`as_assert_call` helpers every target reuses. |
| `physure-script/src/codegen/python.rs`, `rust.rs`, `java.rs`, `js.rs` | Per-target emission: Bool literal, logical operators, the three `assert` shapes, boolean-typed locals (Java, typed TypeScript). |
| `physure-script/tests/transpile_parity_tests.rs` | New execution-based tests proving Python survives `-O` and Java survives without `-ea`. |
| `docs/tutorials/phs_primer.md` | New tutorial section on booleans, logical operators, and assertions. |

---

### Task 1: AST — `Expr::Bool` and every consumer it forces

Adding a new `Expr` variant makes every *exhaustive* match over `Expr` in the crate fail to compile until it has an arm for it. This task adds the variant and, in the same commit, gives every existing exhaustive match its (real, final) arm — using hand-built `Expr::Bool` nodes in tests so it doesn't need the parser (Task 2/3) to exist yet.

**Files:**
- Modify: `physure-script/src/ast.rs`
- Modify: `physure-script/src/interpreter/expressions.rs`
- Modify: `physure-script/src/codegen/mod.rs`
- Modify: `physure-script/src/codegen/python.rs`, `rust.rs`, `java.rs`, `js.rs`
- Test: inline `#[cfg(test)]` modules in each of the files above

- [ ] **Step 1: Write the failing tests**

In `physure-script/src/ast.rs`, inside the existing `#[cfg(test)] mod tests` block, add (near `test_construct_quantity`):

```rust
    #[test]
    fn test_construct_bool() {
        let expr = Expr::Bool(true);
        assert!(matches!(expr, Expr::Bool(true)));
    }
```

In `physure-script/src/interpreter/tests.rs`, add:

```rust
    #[test]
    fn bool_literal_evaluates_directly_to_a_bool_value() {
        let interp = PhsInterpreter::default();
        let env = std::collections::HashMap::new();
        let result = interp.eval_expr(&Expr::Bool(true), &env).unwrap();
        assert_eq!(result, PhsValue::Bool(true));
    }
```

In `physure-script/src/codegen/mod.rs`'s `const_fold_tests` module, add:

```rust
    #[test]
    fn bool_literal_round_trips_through_expr_to_phs_string() {
        assert_eq!(expr_to_phs_string(&Expr::Bool(true)), "True");
        assert_eq!(expr_to_phs_string(&Expr::Bool(false)), "False");
    }
```

In each of `python.rs`, `rust.rs`, `java.rs`, `js.rs`'s existing `#[cfg(test)] mod tests`, add (Python spells `True`/`False`; the other three spell `true`/`false`):

```rust
    // python.rs
    #[test]
    fn transpiles_bool_literals_python() {
        let tp = PythonTranspiler;
        assert_eq!(tp.generate_expr(&Expr::Bool(true)).unwrap(), "True");
        assert_eq!(tp.generate_expr(&Expr::Bool(false)).unwrap(), "False");
    }
```

```rust
    // rust.rs
    #[test]
    fn transpiles_bool_literals_rust() {
        let tp = RustTranspiler;
        assert_eq!(tp.generate_expr(&Expr::Bool(true)).unwrap(), "true");
        assert_eq!(tp.generate_expr(&Expr::Bool(false)).unwrap(), "false");
    }
```

```rust
    // java.rs
    #[test]
    fn transpiles_bool_literals_java() {
        let tp = JavaTranspiler::default();
        assert_eq!(tp.generate_expr(&Expr::Bool(true)).unwrap(), "true");
        assert_eq!(tp.generate_expr(&Expr::Bool(false)).unwrap(), "false");
    }
```

```rust
    // js.rs
    #[test]
    fn transpiles_bool_literals_js() {
        let tp = JsTranspiler::default();
        assert_eq!(tp.generate_expr(&Expr::Bool(true)).unwrap(), "true");
        assert_eq!(tp.generate_expr(&Expr::Bool(false)).unwrap(), "false");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p physure-script bool_literal 2>&1 | head -50`
Expected: a compile error — `Expr::Bool` doesn't exist yet.

- [ ] **Step 3: Add the `Expr::Bool` variant**

In `physure-script/src/ast.rs`, in the `Expr` enum, add a variant after `Str(String)`:

```rust
    Str(String),
    /// `True` or `False`. Reserved keywords in PHS source — see `phs.pest`'s `keyword` rule.
    Bool(bool),
    BinaryOp {
```

- [ ] **Step 4: Run to see the exhaustive-match compile errors**

Run: `cargo build -p physure-script 2>&1 | grep -A2 "non-exhaustive"`
Expected: one error each in `interpreter/expressions.rs` (`eval_expr`), `codegen/mod.rs` (`expr_to_phs_string`, `inline_bindings`, `rewrite_equation_calls`), and `generate_expr` in `python.rs`, `rust.rs`, `java.rs`, `js.rs`. (`substitute` and `expr_to_unit_string` in `mod.rs`, and `message_text` in `md.rs`, already have a wildcard arm — the compiler will not flag them; leave them as-is.)

- [ ] **Step 5: Add the arm to `interpreter/expressions.rs`**

In `PhsInterpreter::eval_expr`'s match, add a new arm right after the `Expr::Quantity(node) => { ... }` block (before `Expr::Str`):

```rust
            Expr::Bool(value) => Ok(PhsValue::Bool(*value)),
```

- [ ] **Step 6: Add the arms to `codegen/mod.rs`**

In `expr_to_phs_string`, add a new arm after `Expr::Str(s) => s.clone(),`:

```rust
        Expr::Bool(b) => if *b { "True" } else { "False" }.to_string(),
```

In `inline_bindings`, change:

```rust
        Expr::Quantity(_) | Expr::Identifier(_) | Expr::Str(_) => expr.clone(),
```

to:

```rust
        Expr::Quantity(_) | Expr::Identifier(_) | Expr::Str(_) | Expr::Bool(_) => expr.clone(),
```

In `rewrite_equation_calls`, change:

```rust
        Expr::Quantity(_) | Expr::Identifier(_) | Expr::Str(_) => Ok(expr.clone()),
```

to:

```rust
        Expr::Quantity(_) | Expr::Identifier(_) | Expr::Str(_) | Expr::Bool(_) => Ok(expr.clone()),
```

- [ ] **Step 7: Add the arm to each codegen target's `generate_expr`**

In `python.rs`, add right after the `Expr::Str(text) => { ... }` block (before `Expr::Identifier`):

```rust
            Expr::Bool(b) => Ok(if *b { "True" } else { "False" }.to_string()),
```

In `rust.rs`, `java.rs`, and `js.rs`, add in the same position (right after their `Expr::Str(text) => { ... }` block, before `Expr::Identifier`):

```rust
            Expr::Bool(b) => Ok(if *b { "true" } else { "false" }.to_string()),
```

- [ ] **Step 8: Run to verify everything compiles and the new tests pass**

Run: `cargo test -p physure-script bool_literal`
Expected: PASS (6 new tests: `test_construct_bool`, `bool_literal_evaluates_directly_to_a_bool_value`, `bool_literal_round_trips_through_expr_to_phs_string`, and the four per-target `transpiles_bool_literals_*`).

- [ ] **Step 9: Full crate sanity check**

Run: `cargo test -p physure-script`
Expected: PASS, no regressions (this proves no *other* exhaustive match over `Expr` was missed).

- [ ] **Step 10: Commit**

```bash
git add physure-script/src/ast.rs physure-script/src/interpreter/expressions.rs physure-script/src/codegen/mod.rs physure-script/src/codegen/python.rs physure-script/src/codegen/rust.rs physure-script/src/codegen/java.rs physure-script/src/codegen/js.rs
git commit -m "feat(phs): add Expr::Bool literal to AST, interpreter, and all codegen targets"
```

---

### Task 2: Grammar — `phs.pest`

**Files:**
- Modify: `physure-script/src/phs.pest`

- [ ] **Step 1: Reserve the five new keywords**

Change:

```pest
keyword     = @{ ("for" | "while" | "let" | "if" | "then" | "else" | "where" | "return") ~ !(ASCII_ALPHANUMERIC | "_") }
```

to:

```pest
keyword     = @{ ("for" | "while" | "let" | "if" | "then" | "else" | "where" | "return" | "not" | "and" | "or" | "True" | "False") ~ !(ASCII_ALPHANUMERIC | "_") }
```

- [ ] **Step 2: Add `bool_lit` and wire it into `primary_base`**

Add near `string_lit` (after the `raw_block`/`string_lit` definitions, or anywhere before `primary_base`):

```pest
_bool_word     = @{ ("True" | "False") ~ !(ASCII_ALPHANUMERIC | "_") }
_is_bool_start = _{ &_bool_word }
bool_lit       = @{ "True" | "False" }
```

`_is_bool_start` must stay silent (`_{}`, zero pairs) but the literal-plus-boundary-guard check it peeks at must be atomic (`@{}`) — a silent, non-atomic version of this rule lets pest's implicit whitespace-skipping slip in between the literal and the `!(...)` guard, so `"True and False"` fails to parse (the skip eats the space after `True`, then the guard tests `'a'` from `and` instead — alphanumeric, so it wrongly rejects). Factoring the atomic check into its own `_bool_word` rule and having `_is_bool_start` merely peek at it (`&_bool_word`) keeps `_is_bool_start` itself pair-free, matching its siblings `_is_str_start`/`_is_num_start`/`_is_vec_start`.

In `primary_base`, add a `bool_lit` branch before `identifier`:

```pest
primary_base   = _{
    (_is_str_start ~ string_lit)
  | (_is_func_start ~ function_call)
  | (_is_vec_start ~ vector_literal)
  | (_is_bool_start ~ bool_lit)
  | (_is_num_start ~ quantity)
  | for_expr
  | identifier
  | ("(" ~ expr ~ ")")
}
```

- [ ] **Step 3: Add `not`/`and`/`or` keyword tokens and the three precedence tiers**

Add right after `base_expr`'s definition (`base_expr = { conv_expr ~ ... }`), before `ternary_op`:

```pest
_not_kw  = @{ "not" ~ !(ASCII_ALPHANUMERIC | "_") }
_and_kw  = @{ "and" ~ !(ASCII_ALPHANUMERIC | "_") }
_or_kw   = @{ "or" ~ !(ASCII_ALPHANUMERIC | "_") }

// `not` binds tighter than `and`, which binds tighter than `or`; both `and`/`or` are
// left-associative repetitions. Every tier wraps `base_expr` unchanged -- range, format
// spec, arithmetic, conversion, and comparison keep exactly their current precedence, per
// the design's "insert the logical layer after the existing non-logical/comparison layer,
// without changing arithmetic/conversion/range/formatting/ternary/where precedence."
not_expr = { _not_kw* ~ base_expr }
and_expr = { not_expr ~ (_nl ~ _and_kw ~ _nl ~ not_expr)* }
or_expr  = { and_expr ~ (_nl ~ _or_kw ~ _nl ~ and_expr)* }
```

(`_not_kw`/`_and_kw`/`_or_kw` follow this file's existing `_for_kw`/`_while_kw` naming convention for atomic keyword-boundary tokens — a leading underscore even though the rule type is `@` atomic, not `_` silent.)

- [ ] **Step 4: Make `expr` use the new `or_expr` tier instead of `base_expr` directly**

Change:

```pest
expr        = { ((_is_if ~ if_expr) | (base_expr ~ (_is_ternary ~ ternary_op)?)) ~ (_is_where ~ where_expr)? }
```

to:

```pest
expr        = { ((_is_if ~ if_expr) | (or_expr ~ (_is_ternary ~ ternary_op)?)) ~ (_is_where ~ where_expr)? }
```

(`where_bind`'s value and `ternary_op`'s `then`/`else` branches keep using `base_expr` directly, unchanged — a bare `and`/`or` there still needs parentheses, matching "without changing ternary/where precedence.")

- [ ] **Step 5: Sanity-build (parser code doesn't exist yet, so nothing calls the new rules — this just proves the grammar itself compiles)**

Run: `cargo build -p physure-script`
Expected: PASS (the `Rule` enum pest-derives gains `bool_lit`, `not_kw`, `and_kw`, `or_kw`, `not_expr`, `and_expr`, `or_expr`, `_is_bool_start`; nothing references them yet, so no *new* warnings beyond `unused` on the parser side, which Task 3 resolves).

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/phs.pest
git commit -m "feat(phs): add True/False/not/and/or to the PHS grammar"
```

---

### Task 3: Parser — `parser/expressions.rs`

**Files:**
- Modify: `physure-script/src/parser/expressions.rs`
- Test: `physure-script/src/parser/tests.rs`

- [ ] **Step 1: Write the failing parser tests**

Add to `physure-script/src/parser/tests.rs`:

```rust
    #[test]
    fn parses_bool_literals() {
        let prog = parse_phs("True\nFalse").unwrap();
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::Bool(true))));
        assert!(matches!(&prog.statements[1], Statement::Expr(Expr::Bool(false))));
    }

    #[test]
    fn true_and_false_are_reserved_against_identifiers() {
        assert!(parse_phs("True = 5").is_err());
        assert!(parse_phs("False = 5").is_err());
    }

    #[test]
    fn not_and_or_parse_with_the_documented_precedence_and_associativity() {
        // `not pressure > limit and enabled or override`
        //   == `((not (pressure > limit)) and enabled) or override`
        let prog = parse_phs("not a > b and c or d").unwrap();
        let Statement::Expr(expr) = &prog.statements[0] else { panic!("expected expr") };
        let Expr::FunctionCall { name: outer_name, args: outer_args, .. } = expr else { panic!("expected or_ call") };
        assert_eq!(outer_name, "op_or");
        let Expr::FunctionCall { name: and_name, args: and_args, .. } = &outer_args[0] else { panic!("expected and_ call") };
        assert_eq!(and_name, "op_and");
        let Expr::FunctionCall { name: not_name, .. } = &and_args[0] else { panic!("expected not_ call") };
        assert_eq!(not_name, "op_not");
        assert!(matches!(&outer_args[1], Expr::Identifier(d) if d == "d"));
        assert!(matches!(&and_args[1], Expr::Identifier(c) if c == "c"));
    }

    #[test]
    fn repeated_not_nests_correctly() {
        let prog = parse_phs("not not True").unwrap();
        let Statement::Expr(Expr::FunctionCall { name: outer, args: outer_args, .. }) = &prog.statements[0] else { panic!() };
        assert_eq!(outer, "op_not");
        let Expr::FunctionCall { name: inner, args: inner_args, .. } = &outer_args[0] else { panic!() };
        assert_eq!(inner, "op_not");
        assert!(matches!(&inner_args[0], Expr::Bool(true)));
    }

    #[test]
    fn and_or_accept_line_breaks_around_them() {
        let prog = parse_phs("True\n  and\n  False\n  or\n  True").unwrap();
        assert!(matches!(&prog.statements[0], Statement::Expr(Expr::FunctionCall { name, .. }) if name == "op_or"));
    }

    #[test]
    fn a_dangling_and_or_or_is_a_parse_error() {
        assert!(parse_phs("True and").is_err());
        assert!(parse_phs("and True").is_err());
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p physure-script parses_bool_literals -- --exact`
Expected: FAIL — the grammar has `bool_lit`/`not_expr`/etc. but `parse_expr`'s Rust match doesn't handle those `Rule`s yet, so parsing falls through to `Err("Unexpected rule...")` or panics on `unwrap()`.

- [ ] **Step 3: Implement `parse_bool_lit`, `parse_not_expr`, `parse_and_expr`, `parse_or_expr`**

In `physure-script/src/parser/expressions.rs`, add these new functions (near `parse_comp_expr`):

```rust
fn parse_bool_lit(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    Ok(Expr::Bool(pair.as_str() == "True"))
}

/// `_not_kw* ~ base_expr` -- zero or more `not` prefixes wrapping one `base_expr`. Wrapping
/// in a plain loop (rather than tracking parity) keeps `not not x` and `not x` both correct
/// without special-casing an even count.
fn parse_not_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut not_count = 0usize;
    let mut base_pair = None;
    for p in pair.into_inner() {
        if p.as_rule() == Rule::_not_kw {
            not_count += 1;
        } else {
            base_pair = Some(p);
        }
    }
    let base_pair = base_pair.ok_or_else(|| PhysureError::Generic("`not` is missing its operand".into()))?;
    let mut result = parse_base_expr(base_pair)?;
    for _ in 0..not_count {
        result = Expr::FunctionCall { name: "op_not".to_string(), args: vec![result], kwargs: Vec::new() };
    }
    Ok(result)
}

fn parse_and_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let mut left = parse_not_expr(first)?;
    while let Some(kw_pair) = inner.next() {
        debug_assert_eq!(kw_pair.as_rule(), Rule::_and_kw);
        let right_pair = inner
            .next()
            .ok_or_else(|| PhysureError::Generic("`and` is missing its right operand".into()))?;
        let right = parse_not_expr(right_pair)?;
        left = Expr::FunctionCall { name: "op_and".to_string(), args: vec![left, right], kwargs: Vec::new() };
    }
    Ok(left)
}

fn parse_or_expr(pair: pest::iterators::Pair<Rule>) -> PhysureResult<Expr> {
    let mut inner = pair.into_inner();
    let first = inner.next().unwrap();
    let mut left = parse_and_expr(first)?;
    while let Some(kw_pair) = inner.next() {
        debug_assert_eq!(kw_pair.as_rule(), Rule::_or_kw);
        let right_pair = inner
            .next()
            .ok_or_else(|| PhysureError::Generic("`or` is missing its right operand".into()))?;
        let right = parse_and_expr(right_pair)?;
        left = Expr::FunctionCall { name: "op_or".to_string(), args: vec![left, right], kwargs: Vec::new() };
    }
    Ok(left)
}
```

- [ ] **Step 4: Wire `Rule::or_expr` into `parse_expr`'s dispatch**

In `parse_expr`, change:

```rust
    let mut result = match first.as_rule() {
        Rule::if_expr => parse_if_expr(first)?,
        Rule::for_expr => parse_for_expr(first)?,
        Rule::base_expr => parse_base_expr(first)?,
        Rule::conv_expr => parse_conv_expr(first)?,
        _ => parse_comp_expr(first)?,
    };
```

to:

```rust
    let mut result = match first.as_rule() {
        Rule::if_expr => parse_if_expr(first)?,
        Rule::for_expr => parse_for_expr(first)?,
        Rule::or_expr => parse_or_expr(first)?,
        Rule::base_expr => parse_base_expr(first)?,
        Rule::conv_expr => parse_conv_expr(first)?,
        _ => parse_comp_expr(first)?,
    };
```

- [ ] **Step 5: Wire `Rule::bool_lit` into `parse_factor`'s primary match**

In `parse_factor`, add a `Rule::bool_lit` arm to the `match primary_pair.as_rule() { ... }` block, next to `Rule::string_lit`:

```rust
        Rule::string_lit => Expr::Str(primary_pair.as_str().trim_matches('"').to_string()),
        Rule::bool_lit => parse_bool_lit(primary_pair)?,
```

- [ ] **Step 6: Run to verify the parser tests pass**

Run: `cargo test -p physure-script parser::tests`
Expected: PASS (all 6 new tests, plus every pre-existing parser test still green).

- [ ] **Step 7: Run the full crate suite to catch any grammar/keyword regression**

Run: `cargo test -p physure-script`
Expected: PASS. If any pre-existing test used `not`, `and`, `or`, `True`, or `False` as a bare identifier or unit symbol, it will now fail to parse — rename that identifier in the test (the design intentionally makes this a breaking change; see the spec's "Compatibility and migration" section).

- [ ] **Step 8: Commit**

```bash
git add physure-script/src/parser/expressions.rs physure-script/src/parser/tests.rs
git commit -m "feat(phs): parse True/False literals and not/and/or into op_not/op_and/op_or calls"
```

---

### Task 4: Interpreter — short-circuit `not`/`and`/`or`

**Files:**
- Modify: `physure-script/src/value.rs`
- Modify: `physure-script/src/interpreter/expressions.rs`
- Test: `physure-script/src/interpreter/tests.rs`

- [ ] **Step 1: Write the failing tests**

Add to `physure-script/src/interpreter/tests.rs`:

```rust
    #[test]
    fn not_and_or_truth_table() {
        let mut interp = PhsInterpreter::default();
        let cases = [
            ("not True", "False"),
            ("not False", "True"),
            ("True and False", "False"),
            ("True and True", "True"),
            ("False and True", "False"),
            ("True or False", "True"),
            ("False or False", "False"),
            ("False or True", "True"),
        ];
        for (src, expected) in cases {
            let results = interp.eval_str(src).unwrap();
            assert_eq!(results[0].to_string(), expected, "for `{src}`");
        }
    }

    #[test]
    fn and_short_circuits_and_never_evaluates_a_dividing_by_zero_right_side() {
        let mut interp = PhsInterpreter::default();
        // Eager evaluation would raise "Division by zero" instead of returning False.
        let results = interp.eval_str("False and (1 / 0 > 0)").unwrap();
        assert_eq!(results[0].to_string(), "False");
    }

    #[test]
    fn or_short_circuits_and_never_evaluates_a_dividing_by_zero_right_side() {
        let mut interp = PhsInterpreter::default();
        let results = interp.eval_str("True or (1 / 0 > 0)").unwrap();
        assert_eq!(results[0].to_string(), "True");
    }

    #[test]
    fn not_rejects_a_non_bool_operand() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("not 5").unwrap_err();
        assert!(matches!(err, PhysureError::Generic(_)));
    }

    #[test]
    fn and_rejects_a_non_bool_left_operand_without_evaluating_the_right_side() {
        let mut interp = PhsInterpreter::default();
        // If the (invalid) left operand's type error didn't short-circuit, this would raise
        // "Division by zero" from the right side instead of a type error about `5 m`.
        let err = interp.eval_str("5 m and (1 / 0 > 0)").unwrap_err();
        assert!(matches!(err, PhysureError::Generic(_)));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p physure-script not_and_or_truth_table -- --exact`
Expected: FAIL — `op_not`/`op_and`/`op_or` currently fall through to "Undefined function" (they hit the generic `FunctionCall` path, which eagerly evaluates every arg via `eval_core_builtin`/domain builtins/externals and finds no match).

- [ ] **Step 3: Add `PhsValue::type_name()`**

In `physure-script/src/value.rs`, add after the `impl fmt::Display for PhsValue` block:

```rust
impl PhsValue {
    /// A short, human-readable name for this value's PHS type. Used by the strict-Bool
    /// logical operators and the `assert`/`exact_assert` dispatcher, neither of which may
    /// fall back to `is_truthy` -- their errors need to say what type they actually got.
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            PhsValue::None => "None",
            PhsValue::Number(_) => "Number",
            PhsValue::Quantity(_) => "Quantity",
            PhsValue::Bool(_) => "Bool",
            PhsValue::String(_) => "String",
            PhsValue::Vector(_) => "Vector",
            PhsValue::Matrix(_) => "Matrix",
            PhsValue::Function(_) => "Function",
            PhsValue::Sigma(_) => "Sigma",
            PhsValue::SigmaBound(_, _) => "SigmaBound",
            PhsValue::Plot(_) => "Plot",
            PhsValue::Equation(_, _) => "Equation",
            PhsValue::Range(_, _) => "Range",
        }
    }
}
```

- [ ] **Step 4: Intercept `op_not`/`op_and`/`op_or` before eager argument evaluation**

In `physure-script/src/interpreter/expressions.rs`, inside the `Expr::FunctionCall { name, args, kwargs }` arm of `eval_expr`, insert this block immediately **before** the existing:

```rust
                let mut arg_vals = Vec::new();
                for arg in args {
                    arg_vals.push(self.eval_expr(arg, env)?);
                }
```

Insert:

```rust
                // Short-circuit logical operators: language semantics, not an optimization,
                // so they must run before the eager argument evaluation below rather than as
                // ordinary builtins. Strict Bool checking here (never `is_truthy`) is what
                // keeps `5 m and enabled` a type error instead of a silent truthiness test.
                if kwargs.is_empty() && args.len() == 1 && name == "op_not" {
                    let operand = self.eval_expr(&args[0], env)?;
                    let PhsValue::Bool(b) = operand else {
                        return Err(PhysureError::Generic(format!(
                            "`not` expects a Bool operand, got {}", operand.type_name()
                        )));
                    };
                    return Ok(PhsValue::Bool(!b));
                }
                if kwargs.is_empty() && args.len() == 2 && (name == "op_and" || name == "op_or") {
                    let word = if name == "op_and" { "and" } else { "or" };
                    let left = self.eval_expr(&args[0], env)?;
                    let PhsValue::Bool(left_b) = left else {
                        return Err(PhysureError::Generic(format!(
                            "`{}` expects Bool operands, left side was {}", word, left.type_name()
                        )));
                    };
                    if (name == "op_and" && !left_b) || (name == "op_or" && left_b) {
                        return Ok(PhsValue::Bool(left_b));
                    }
                    let right = self.eval_expr(&args[1], env)?;
                    let PhsValue::Bool(right_b) = right else {
                        return Err(PhysureError::Generic(format!(
                            "`{}` expects Bool operands, right side was {}", word, right.type_name()
                        )));
                    };
                    return Ok(PhsValue::Bool(right_b));
                }

```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p physure-script not_and_or_truth_table and_short_circuits or_short_circuits not_rejects and_rejects`
Expected: PASS.

- [ ] **Step 6: Full crate sanity check**

Run: `cargo test -p physure-script`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add physure-script/src/value.rs physure-script/src/interpreter/expressions.rs physure-script/src/interpreter/tests.rs
git commit -m "feat(phs): short-circuit interpreter evaluation for not/and/or"
```

---

### Task 5: Interpreter — Bool equality (`==`/`!=`)

**Files:**
- Modify: `physure-script/src/builtins/core.rs`
- Test: `physure-script/src/interpreter/tests.rs`

- [ ] **Step 1: Write the failing tests**

Add to `physure-script/src/interpreter/tests.rs`:

```rust
    #[test]
    fn bool_equality_and_inequality() {
        let mut interp = PhsInterpreter::default();
        assert_eq!(interp.eval_str("True == True").unwrap()[0].to_string(), "True");
        assert_eq!(interp.eval_str("True == False").unwrap()[0].to_string(), "False");
        assert_eq!(interp.eval_str("True != False").unwrap()[0].to_string(), "True");
        assert_eq!(interp.eval_str("False != False").unwrap()[0].to_string(), "False");
    }

    #[test]
    fn mixed_bool_and_non_bool_equality_is_a_type_error() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("True == 1").unwrap_err();
        assert!(matches!(err, PhysureError::Generic(_)));
        let err = interp.eval_str("1.0 m != False").unwrap_err();
        assert!(matches!(err, PhysureError::Generic(_)));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p physure-script bool_equality_and_inequality -- --exact`
Expected: FAIL — `compare()`'s current fallback silently treats a `Bool` operand as non-comparable and returns `False` rather than comparing correctly or erroring.

- [ ] **Step 3: Add the Bool branch to `op_==`/`op_!=`**

In `physure-script/src/builtins/core.rs`, add this helper right after the `boolean()` function:

```rust
/// The `==`/`!=` Bool branch. `Some(Ok(...))` when at least one side is `Bool` (a matching
/// pair compares directly; a mismatched pair is a type error naming both types); `None` when
/// neither side is `Bool`, so the caller falls through to the existing Quantity/sigma-bound
/// handling unchanged.
fn bool_equality(args: &[PhsValue], symbol: &str, want_equal: bool) -> Option<PhysureResult<Option<PhsValue>>> {
    let l_is_bool = matches!(args.first(), Some(PhsValue::Bool(_)));
    let r_is_bool = matches!(args.get(1), Some(PhsValue::Bool(_)));
    if !l_is_bool && !r_is_bool {
        return None;
    }
    if let (Some(PhsValue::Bool(l)), Some(PhsValue::Bool(r))) = (args.first(), args.get(1)) {
        return Some(Ok(Some(boolean((l == r) == want_equal))));
    }
    Some(Err(PhysureError::Generic(format!(
        "cannot compare {} and {} with {}",
        args.first().map(PhsValue::type_name).unwrap_or("None"),
        args.get(1).map(PhsValue::type_name).unwrap_or("None"),
        symbol,
    ))))
}
```

Then change the `"op_==" | "op_eq"` and `"op_!=" | "op_neq"` arms:

```rust
        "op_==" | "op_eq" => {
            if let Some(res) = bool_equality(args, "==", true) {
                return res;
            }
            // `x == 5.0 +/- 0.2` asks whether x lies within k sigma of the target, so it
            // is a tolerance test rather than an ordinary comparison.
            if let (Some(PhsValue::Quantity(l)), Some(PhsValue::SigmaBound(target_q, k_sigma)))
            | (Some(PhsValue::SigmaBound(target_q, k_sigma)), Some(PhsValue::Quantity(l))) =
                (args.first(), args.get(1))
            {
                let unc = if l.value.std_dev() > 0.0 { l.value.std_dev() } else { 0.05 };
                let (a, b) = comparable(l, target_q)?;
                return Ok(Some(boolean((a - b).abs() <= k_sigma * unc)));
            }
            compare(args, |l, r| (l - r).abs() < 1e-9)
        }
        "op_!=" | "op_neq" => {
            if let Some(res) = bool_equality(args, "!=", false) {
                return res;
            }
            compare(args, |l, r| (l - r).abs() >= 1e-9)
        }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p physure-script bool_equality mixed_bool_and_non_bool`
Expected: PASS.

- [ ] **Step 5: Full crate sanity check**

Run: `cargo test -p physure-script`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/builtins/core.rs physure-script/src/interpreter/tests.rs
git commit -m "feat(phs): add Bool equality to op_==/op_!= with a type error on mixed operands"
```

---

### Task 6: Interpreter — `assert`/`exact_assert` overload dispatch

**Files:**
- Modify: `physure-script/src/builtins/core.rs`
- Test: `physure-script/src/interpreter/tests.rs`

- [ ] **Step 1: Write the failing tests**

Add to `physure-script/src/interpreter/tests.rs` (existing `assert_rejects_non_quantity_arguments` stays as-is — a two-string call is still not a valid `assert(Quantity, Quantity)` and still isn't `Bool`, so it still hits the same catch-all error):

```rust
    #[test]
    fn bool_assert_passes_and_returns_none() {
        let mut interp = PhsInterpreter::default();
        let results = interp.eval_str("assert(True)").unwrap();
        assert_eq!(results[0], PhsValue::None);
    }

    #[test]
    fn bool_assert_fails_with_assertion_failed() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("assert(False)").unwrap_err();
        assert!(matches!(err, PhysureError::AssertionFailed { kind: "assert", .. }));
    }

    #[test]
    fn bool_assert_with_message_includes_it_in_the_failure() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("assert(False, \"scenario id\")").unwrap_err();
        let PhysureError::AssertionFailed { message, .. } = err else {
            panic!("expected AssertionFailed, got {err:?}")
        };
        assert_eq!(message, "scenario id");
    }

    #[test]
    fn invalid_assert_signature_lists_the_accepted_ones() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("assert(1.0, 2.0, 3.0)").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("assert(Bool)") && msg.contains("assert(Quantity, Quantity)"),
            "{msg}"
        );
    }

    #[test]
    fn exact_assert_has_no_bool_overload() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("exact_assert(True)").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("exact_assert(Quantity, Quantity)"), "{msg}");
    }

    #[test]
    fn assert_of_a_comparison_expression_works_for_compatible_converted_units() {
        let mut interp = PhsInterpreter::default();
        let results = interp.eval_str("assert(1.0 km == 1000.0 m)").unwrap();
        assert_eq!(results[0], PhsValue::None);
    }

    #[test]
    fn a_false_sigma_bound_comparison_reaches_bool_assert_and_fails() {
        let mut interp = PhsInterpreter::default();
        let err = interp.eval_str("assert(5.0 == 1.0 +/- 0.1 sigma)").unwrap_err();
        assert!(matches!(err, PhysureError::AssertionFailed { kind: "assert", .. }));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p physure-script bool_assert invalid_assert_signature exact_assert_has_no_bool_overload assert_of_a_comparison a_false_sigma_bound -- `
Expected: FAIL — `eval_core_builtin`'s current `"assert" | "exact_assert"` arm requires exactly 2 `Quantity` args and rejects everything else with a generic "expects two quantities" message.

- [ ] **Step 3: Rewrite the dispatch**

In `physure-script/src/builtins/core.rs`, replace the entire:

```rust
        "assert" | "exact_assert" => {
            if args.len() != 2 {
                return Err(PhysureError::Generic(format!("{name} expects 2 arguments (actual, expected)")));
            }
            match (&args[0], &args[1]) {
                (PhsValue::Quantity(a), PhsValue::Quantity(b)) => {
                    if name == "assert" {
                        a.phs_assert(b)?;
                    } else {
                        a.phs_exact_assert(b)?;
                    }
                    Ok(Some(PhsValue::None))
                }
                _ => Err(PhysureError::Generic(format!("{name} expects two quantities"))),
            }
        }
```

with:

```rust
        "assert" => match args {
            [PhsValue::Bool(cond)] => {
                if *cond {
                    Ok(Some(PhsValue::None))
                } else {
                    Err(PhysureError::AssertionFailed { kind: "assert", message: "condition was False".to_string() })
                }
            }
            [PhsValue::Bool(cond), PhsValue::String(msg)] => {
                if *cond {
                    Ok(Some(PhsValue::None))
                } else {
                    Err(PhysureError::AssertionFailed { kind: "assert", message: msg.clone() })
                }
            }
            [PhsValue::Quantity(a), PhsValue::Quantity(b)] => {
                a.phs_assert(b)?;
                Ok(Some(PhsValue::None))
            }
            _ => Err(PhysureError::Generic(format!(
                "assert received ({}); expected assert(Bool), assert(Bool, String), or assert(Quantity, Quantity)",
                args.iter().map(PhsValue::type_name).collect::<Vec<_>>().join(", ")
            ))),
        },
        "exact_assert" => match args {
            [PhsValue::Quantity(a), PhsValue::Quantity(b)] => {
                a.phs_exact_assert(b)?;
                Ok(Some(PhsValue::None))
            }
            _ => Err(PhysureError::Generic(format!(
                "exact_assert received ({}); expected exact_assert(Quantity, Quantity)",
                args.iter().map(PhsValue::type_name).collect::<Vec<_>>().join(", ")
            ))),
        },
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p physure-script assert`
Expected: PASS (every new test, plus the five pre-existing `assert*`/`exact_assert*` tests in `interpreter/tests.rs` at lines 543-575, which exercise the unchanged `Quantity, Quantity` path).

- [ ] **Step 5: Full crate sanity check**

Run: `cargo test -p physure-script`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add physure-script/src/builtins/core.rs physure-script/src/interpreter/tests.rs
git commit -m "feat(phs): dispatch assert(Bool)/assert(Bool, String)/assert(Quantity, Quantity) by arity and type"
```

---

### Task 7: Shared codegen helpers

**Files:**
- Modify: `physure-script/src/codegen/mod.rs`

- [ ] **Step 1: Write the failing tests**

Add a new test module to `physure-script/src/codegen/mod.rs` (after `const_fold_tests`):

```rust
#[cfg(test)]
mod classifier_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn comparisons_and_logicals_are_definitely_bool() {
        let known = HashSet::new();
        let cmp = Expr::FunctionCall { name: "op_>".into(), args: vec![Expr::Bool(true), Expr::Bool(true)], kwargs: vec![] };
        assert!(is_definitely_bool(&cmp, &known));
        let not_expr = Expr::FunctionCall { name: "op_not".into(), args: vec![Expr::Bool(true)], kwargs: vec![] };
        assert!(is_definitely_bool(&not_expr, &known));
        assert!(is_definitely_bool(&Expr::Bool(false), &known));
        assert!(!is_definitely_bool(&Expr::Identifier("x".into()), &known));
    }

    #[test]
    fn a_known_bool_identifier_is_definitely_bool() {
        let mut known = HashSet::new();
        known.insert("flag".to_string());
        assert!(is_definitely_bool(&Expr::Identifier("flag".to_string()), &known));
    }

    #[test]
    fn assert_call_shape_depends_on_arity_and_the_bool_classifier() {
        let known = HashSet::new();
        let one_arg = Expr::FunctionCall { name: "assert".into(), args: vec![Expr::Bool(true)], kwargs: vec![] };
        assert!(matches!(as_assert_call(&one_arg, &known), Some(AssertShape::Bool { .. })));

        let bool_msg = Expr::FunctionCall {
            name: "assert".into(),
            args: vec![Expr::Bool(false), Expr::Str("boom".into())],
            kwargs: vec![],
        };
        assert!(matches!(as_assert_call(&bool_msg, &known), Some(AssertShape::BoolWithMessage { .. })));

        let quantities = Expr::FunctionCall {
            name: "assert".into(),
            args: vec![Expr::Identifier("a".into()), Expr::Identifier("b".into())],
            kwargs: vec![],
        };
        assert!(matches!(as_assert_call(&quantities, &known), Some(AssertShape::Quantities { kind: "assert", .. })));

        let exact = Expr::FunctionCall {
            name: "exact_assert".into(),
            args: vec![Expr::Identifier("a".into()), Expr::Identifier("b".into())],
            kwargs: vec![],
        };
        assert!(matches!(as_assert_call(&exact, &known), Some(AssertShape::Quantities { kind: "exact_assert", .. })));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo build -p physure-script 2>&1 | head -30`
Expected: compile error — `is_definitely_bool`, `AssertShape`, `LogicalOp`, `as_logical_op` don't exist yet, and the old `as_assert_call` has a different signature/return type.

- [ ] **Step 3: Replace `as_assert_call` and add the new helpers**

In `physure-script/src/codegen/mod.rs`, delete the existing `as_assert_call` function entirely (the one returning `Option<(&'static str, &Expr, &Expr)>`, right after `as_comparison_op`) and replace it with:

```rust
/// The `not`/`and`/`or` shape the parser desugars logical expressions into (see `ast.rs`'s
/// note on `op_>` et al.). Every target maps this straight onto its own native `!`/`&&`/`||`
/// -- all four of Python, Rust, Java, and JavaScript/TypeScript are natively short-circuit,
/// so no target needs its own short-circuit emulation.
pub(crate) enum LogicalOp<'a> {
    Not(&'a Expr),
    And(&'a Expr, &'a Expr),
    Or(&'a Expr, &'a Expr),
}

pub(crate) fn as_logical_op(expr: &Expr) -> Option<LogicalOp<'_>> {
    let Expr::FunctionCall { name, args, kwargs } = expr else { return None };
    if !kwargs.is_empty() {
        return None;
    }
    match (name.as_str(), args.len()) {
        ("op_not", 1) => Some(LogicalOp::Not(&args[0])),
        ("op_and", 2) => Some(LogicalOp::And(&args[0], &args[1])),
        ("op_or", 2) => Some(LogicalOp::Or(&args[0], &args[1])),
        _ => None,
    }
}

/// Recognizes an expression whose PHS type is definitely `Bool` without needing to run the
/// interpreter: a literal, a comparison (`as_comparison_op`), a logical operator
/// (`as_logical_op`), or an identifier previously classified as boolean by `known_bools`.
///
/// This is what lets codegen -- which never sees a runtime `PhsValue` -- decide between the
/// `assert(Bool)`/`assert(Bool, String)` and `assert(Quantity, Quantity)` overloads (the
/// interpreter makes the same decision dynamically, from the actual argument values), and
/// lets Java and typed TypeScript give a local an explicit `boolean` type instead of the
/// default `Quantity`. Deliberately narrow: an opaque function call or an unclassified
/// identifier is never treated as boolean, matching this design's "no general overload
/// resolution" and "no inferring boolean return types for arbitrary functions" scope.
pub(crate) fn is_definitely_bool(expr: &Expr, known_bools: &HashSet<String>) -> bool {
    match expr {
        Expr::Bool(_) => true,
        Expr::Identifier(name) => known_bools.contains(name),
        Expr::FunctionCall { .. } => as_comparison_op(expr).is_some() || as_logical_op(expr).is_some(),
        _ => false,
    }
}

/// The overload an `assert`/`exact_assert` call site resolves to, decided statically from its
/// arity and (for a 2-argument `assert` call) whether the first argument `is_definitely_bool`.
/// Mirrors `builtins::core::eval_core_builtin`'s runtime dispatch, but codegen has no runtime
/// value to inspect -- only the AST plus whatever `known_bools` the caller has tracked from
/// earlier assignments in the same generated scope. Every codegen target intercepts this
/// before falling through to its generic `FunctionCall` handling, since none of them can
/// express PHS's `assert`/`exact_assert` as an ordinary function call.
pub(crate) enum AssertShape<'a> {
    Bool { condition: &'a Expr },
    BoolWithMessage { condition: &'a Expr, message: &'a Expr },
    Quantities { kind: &'static str, actual: &'a Expr, expected: &'a Expr },
}

pub(crate) fn as_assert_call<'a>(expr: &'a Expr, known_bools: &HashSet<String>) -> Option<AssertShape<'a>> {
    let Expr::FunctionCall { name, args, kwargs } = expr else { return None };
    if !kwargs.is_empty() {
        return None;
    }
    match (name.as_str(), args.as_slice()) {
        ("assert", [cond]) if is_definitely_bool(cond, known_bools) => Some(AssertShape::Bool { condition: cond }),
        ("assert", [cond, msg]) if is_definitely_bool(cond, known_bools) => {
            Some(AssertShape::BoolWithMessage { condition: cond, message: msg })
        }
        ("assert", [a, b]) => Some(AssertShape::Quantities { kind: "assert", actual: a, expected: b }),
        ("exact_assert", [a, b]) => Some(AssertShape::Quantities { kind: "exact_assert", actual: a, expected: b }),
        _ => None,
    }
}
```

Note: this is a **breaking signature change** — every existing call site (`python.rs`, `rust.rs` ×2, `java.rs`, `js.rs`) now fails to compile. That's expected; Tasks 8-11 fix each one.

- [ ] **Step 4: Run to confirm the expected downstream breakage (and no other surprise)**

Run: `cargo build -p physure-script 2>&1 | grep "error\[" | sort -u`
Expected: `E0061`/`E0308`-style errors only in `python.rs`, `rust.rs`, `java.rs`, `js.rs` at their `super::as_assert_call(...)` call sites — nothing else.

- [ ] **Step 5: Commit** (the crate will not build again until Task 8 lands — that's fine, this is one logical unit split for reviewability; do not run the full test suite here)

```bash
git add physure-script/src/codegen/mod.rs
git commit -m "feat(phs): add is_definitely_bool classifier and AssertShape to shared codegen helpers"
```

---

### Task 8: Python codegen

**Files:**
- Modify: `physure-script/src/codegen/python.rs`

- [ ] **Step 1: Add the `HashSet` import**

At the top of `physure-script/src/codegen/python.rs`, change:

```rust
use crate::ast::{
    Program, Statement, ImportNode, ImportSpecifier, ExportNode,
    FunctionDefNode, AssignmentNode, Expr, BinaryOp, QuantityNode
};
use super::{CodeGenerator, CodegenError};
```

to:

```rust
use crate::ast::{
    Program, Statement, ImportNode, ImportSpecifier, ExportNode,
    FunctionDefNode, AssignmentNode, Expr, BinaryOp, QuantityNode
};
use super::{CodeGenerator, CodegenError};
use std::collections::HashSet;
```

- [ ] **Step 2: Rewrite `generate_program`'s `Statement::Assignment`/`Statement::Expr` handling**

Replace the whole `fn generate_program` body with:

```rust
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError> {
        let mut out = String::from(concat!(
            "# Generated by PhysureScript (PHS) Compiler v", env!("CARGO_PKG_VERSION"), "\n",
            "import math\n",
            "from physure import Q_, Quantity, PhyEquation, PhyFunction, phy_function\n\n",
            "try:\n",
            "    from physure.builtins import vector, gradient, trapz, solve, deriv, integral\n",
            "except ImportError:\n",
            "    def vector(*args): return list(args)\n\n"
        ));
        let mut known_bools: HashSet<String> = HashSet::new();

        for stmt in &program.statements {
            match stmt {
                Statement::Assignment(node) => {
                    if super::is_definitely_bool(&node.value, &known_bools) {
                        known_bools.insert(node.name.clone());
                    } else {
                        known_bools.remove(&node.name);
                    }
                    let stmt_str = self.generate_assignment(node)?;
                    out.push_str(&stmt_str);
                    out.push('\n');
                    out.push_str(&format!("print(f\"{}: {{{}}}\")\n", node.name, sanitize_identifier(&node.name)));
                }
                Statement::Expr(expr) => {
                    match super::as_assert_call(expr, &known_bools) {
                        Some(super::AssertShape::Bool { condition }) => {
                            let cond_code = self.generate_expr(condition)?;
                            out.push_str(&format!("if not ({}): raise AssertionError(\"assertion failed\")\n", cond_code));
                        }
                        Some(super::AssertShape::BoolWithMessage { condition, message }) => {
                            let cond_code = self.generate_expr(condition)?;
                            let msg_code = self.generate_expr(message)?;
                            out.push_str(&format!("if not ({}): raise AssertionError({})\n", cond_code, msg_code));
                        }
                        Some(super::AssertShape::Quantities { kind, actual, expected }) => {
                            let a_code = self.generate_expr(actual)?;
                            let b_code = self.generate_expr(expected)?;
                            let line = if kind == "assert" {
                                format!(
                                    "if not ({a}).approx_eq({b}, 1e-9, 1e-12): raise AssertionError(f\"assert failed: {{{a}}} != {{{b}}}\")",
                                    a = a_code, b = b_code
                                )
                            } else {
                                format!(
                                    "if not ({a}).exact_eq({b}): raise AssertionError(f\"exact_assert failed: {{{a}}} != {{{b}}}\")",
                                    a = a_code, b = b_code
                                )
                            };
                            out.push_str(&line);
                            out.push('\n');
                        }
                        None => {
                            let expr_str = self.generate_expr(expr)?;
                            out.push_str(&format!("print({})\n", expr_str));
                        }
                    }
                }
                _ => {
                    let stmt_str = self.generate_statement(stmt)?;
                    if !stmt_str.is_empty() {
                        out.push_str(&stmt_str);
                        out.push('\n');
                    }
                }
            }
        }

        Ok(out)
    }
```

- [ ] **Step 3: Add `as_logical_op` handling and widen the nested-assert rejection**

In `generate_expr`'s `Expr::FunctionCall { name, args, kwargs }` arm, change:

```rust
            Expr::FunctionCall { name, args, kwargs } => {
                if let Some((op_sym, l, r)) = super::as_comparison_op(expr) {
                    let l_str = self.generate_expr(l)?;
                    let r_str = self.generate_expr(r)?;
                    return Ok(format!("({} {} {})", l_str, op_sym, r_str));
                }
                if (name == "assert" || name == "exact_assert") && kwargs.is_empty() && args.len() == 2 {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
```

to:

```rust
            Expr::FunctionCall { name, args, kwargs } => {
                if let Some((op_sym, l, r)) = super::as_comparison_op(expr) {
                    let l_str = self.generate_expr(l)?;
                    let r_str = self.generate_expr(r)?;
                    return Ok(format!("({} {} {})", l_str, op_sym, r_str));
                }
                if let Some(logical) = super::as_logical_op(expr) {
                    return Ok(match logical {
                        super::LogicalOp::Not(x) => format!("(not {})", self.generate_expr(x)?),
                        super::LogicalOp::And(l, r) => format!("({} and {})", self.generate_expr(l)?, self.generate_expr(r)?),
                        super::LogicalOp::Or(l, r) => format!("({} or {})", self.generate_expr(l)?, self.generate_expr(r)?),
                    });
                }
                if (name == "assert" || name == "exact_assert") && kwargs.is_empty() && matches!(args.len(), 1 | 2) {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
```

- [ ] **Step 4: Update the two existing assert tests that relied on the old `assert` statement**

Replace:

```rust
    #[test]
    fn transpiles_assert_call_to_a_python_assert_statement() {
        let tp = PythonTranspiler;
        let program = crate::parser::parse_phs("assert(1.0 km, 1000.0 m)").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains("assert "), "expected a Python assert statement:\n{code}");
        assert!(code.contains(".approx_eq("), "expected approx_eq call:\n{code}");
        assert!(!code.contains("print(assert"), "must not fall through to the generic call path:\n{code}");
    }

    #[test]
    fn transpiles_exact_assert_call_to_equality_assert() {
        let tp = PythonTranspiler;
        let program = crate::parser::parse_phs("exact_assert(5.0 m, 5.0 m)").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains("assert ") && code.contains(".exact_eq("), "expected an exact_eq assert:\n{code}");
    }
```

with:

```rust
    #[test]
    fn transpiles_assert_call_to_a_python_conditional_raise() {
        // Python's `assert` statement is removed by `python -O`; the design requires an
        // explicit `if not ...: raise AssertionError(...)` instead so `-O` can't disable it.
        let tp = PythonTranspiler;
        let program = crate::parser::parse_phs("assert(1.0 km, 1000.0 m)").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains("if not"), "expected a conditional raise:\n{code}");
        assert!(code.contains("raise AssertionError"), "expected AssertionError:\n{code}");
        assert!(!code.contains("\nassert "), "must not emit a removable `assert` statement:\n{code}");
        assert!(code.contains(".approx_eq("), "expected approx_eq call:\n{code}");
    }

    #[test]
    fn transpiles_exact_assert_call_to_equality_assert() {
        let tp = PythonTranspiler;
        let program = crate::parser::parse_phs("exact_assert(5.0 m, 5.0 m)").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains("if not") && code.contains(".exact_eq("), "expected an exact_eq conditional raise:\n{code}");
        assert!(!code.contains("\nassert "), "must not emit a removable `assert` statement:\n{code}");
    }
```

- [ ] **Step 5: Add the new tests**

Add to the same `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn transpiles_logical_operators_python() {
        let tp = PythonTranspiler;
        let not_expr = Expr::FunctionCall { name: "op_not".to_string(), args: vec![Expr::Bool(true)], kwargs: vec![] };
        assert_eq!(tp.generate_expr(&not_expr).unwrap(), "(not True)");
        let and_expr = Expr::FunctionCall { name: "op_and".to_string(), args: vec![Expr::Bool(true), Expr::Bool(false)], kwargs: vec![] };
        assert_eq!(tp.generate_expr(&and_expr).unwrap(), "(True and False)");
        let or_expr = Expr::FunctionCall { name: "op_or".to_string(), args: vec![Expr::Bool(false), Expr::Bool(true)], kwargs: vec![] };
        assert_eq!(tp.generate_expr(&or_expr).unwrap(), "(False or True)");
    }

    #[test]
    fn transpiles_one_argument_bool_assert_python() {
        let tp = PythonTranspiler;
        let program = crate::parser::parse_phs("assert(1.0 m > 0.0 m)").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains("if not") && code.contains("raise AssertionError(\"assertion failed\")"), "{code}");
    }

    #[test]
    fn transpiles_bool_assert_with_message_python() {
        let tp = PythonTranspiler;
        let program = crate::parser::parse_phs("assert(1.0 m > 2.0 m, \"too small\")").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains("raise AssertionError(\"too small\")"), "{code}");
    }

    #[test]
    fn rejects_one_argument_assert_nested_in_an_expression_py() {
        let tp = PythonTranspiler;
        let program = crate::parser::parse_phs("x = assert(1.0 m > 0.0 m)").unwrap();
        assert!(tp.generate_program(&program).is_err());
    }

    #[test]
    fn named_bool_assignment_feeds_a_later_assert_python() {
        let tp = PythonTranspiler;
        let program = crate::parser::parse_phs("ok = 1.0 m > 0.0 m\nassert(ok, \"should hold\")").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(
            code.contains("raise AssertionError(\"should hold\")"),
            "expected the Bool+message shape, not a Quantity assert:\n{code}"
        );
    }
```

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p physure-script --lib codegen::python`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add physure-script/src/codegen/python.rs
git commit -m "feat(phs): Python codegen for not/and/or and the Bool assert overloads"
```

---

### Task 9: Rust codegen

**Files:**
- Modify: `physure-script/src/codegen/rust.rs`

- [ ] **Step 1: Thread `known_bools` alongside `declared_vars` through `generate_program`, `generate_statement`, `generate_function_def`, `generate_assignment`**

In `physure-script/src/codegen/rust.rs`, update `generate_program`:

```rust
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError> {
        let mut code = String::from(concat!(
            "// Generated by PhysureScript (PHS) Compiler v", env!("CARGO_PKG_VERSION"), "\n",
            "use physure_core::Quantity;\n\n"
        ));
        let mut top_functions = Vec::new();
        let mut main_statements = Vec::new();
        let mut main_declared_vars = HashSet::new();
        let mut known_bools: HashSet<String> = HashSet::new();

        for stmt in &program.statements {
            match stmt {
                Statement::FunctionDef(node) => {
                    top_functions.push(self.generate_function_def(node)?);
                }
                Statement::Assignment(node) => {
                    main_statements.push(format!("    {}", self.generate_assignment(node, &mut main_declared_vars, &mut known_bools)?));
                    main_statements.push(format!("    println!(\"{}: {{}}\", {});", node.name, node.name));
                }
                Statement::Expr(expr) => {
                    match super::as_assert_call(expr, &known_bools) {
                        Some(super::AssertShape::Bool { condition }) => {
                            let cond_code = self.generate_expr(condition)?;
                            main_statements.push(format!("    assert!({}, \"assertion failed\");", cond_code));
                        }
                        Some(super::AssertShape::BoolWithMessage { condition, message }) => {
                            let cond_code = self.generate_expr(condition)?;
                            let msg_code = self.generate_expr(message)?;
                            main_statements.push(format!("    assert!({}, \"{{}}\", {});", cond_code, msg_code));
                        }
                        Some(super::AssertShape::Quantities { kind, actual, expected }) => {
                            let a_code = self.generate_expr(actual)?;
                            let b_code = self.generate_expr(expected)?;
                            let method = if kind == "assert" { "phs_assert" } else { "phs_exact_assert" };
                            main_statements.push(format!("    ({}).{}(&({}))?;", a_code, method, b_code));
                        }
                        None => {
                            let expr_code = self.generate_expr(expr)?;
                            main_statements.push(format!("    println!(\"{{}}\", {});", expr_code));
                        }
                    }
                }
                Statement::While { .. } => {
                    main_statements.push(format!("    {}", self.generate_statement(stmt, &mut main_declared_vars, &mut known_bools)?));
                }
                _ => {}
            }
        }

        for func in top_functions {
            code.push_str(&func);
            code.push_str("\n\n");
        }

        if !main_statements.is_empty() {
            code.push_str("fn main() -> Result<(), Box<dyn std::error::Error>> {\n");
            code.push_str(&main_statements.join("\n"));
            code.push_str("\n    Ok(())\n}\n");
        }

        Ok(code)
    }
```

Update `generate_statement`, `generate_function_def`, and `generate_assignment`:

```rust
    fn generate_statement(&self, stmt: &Statement, declared_vars: &mut HashSet<String>, known_bools: &mut HashSet<String>) -> Result<String, CodegenError> {
        match stmt {
            Statement::Import(_) => Ok(String::new()),
            Statement::Export(_) => Ok(String::new()),
            Statement::FunctionDef(node) => self.generate_function_def(node),
            Statement::Assignment(node) => self.generate_assignment(node, declared_vars, known_bools),
            Statement::Expr(expr) => {
                match super::as_assert_call(expr, known_bools) {
                    Some(super::AssertShape::Bool { condition }) => {
                        Ok(format!("assert!({}, \"assertion failed\")", self.generate_expr(condition)?))
                    }
                    Some(super::AssertShape::BoolWithMessage { condition, message }) => {
                        Ok(format!("assert!({}, \"{{}}\", {})", self.generate_expr(condition)?, self.generate_expr(message)?))
                    }
                    Some(super::AssertShape::Quantities { kind, actual, expected }) => {
                        let a_code = self.generate_expr(actual)?;
                        let b_code = self.generate_expr(expected)?;
                        let method = if kind == "assert" { "phs_assert" } else { "phs_exact_assert" };
                        Ok(format!("({}).{}(&({}))?", a_code, method, b_code))
                    }
                    None => self.generate_expr(expr),
                }
            }
            Statement::Return(expr) => Ok(format!("return {};", self.generate_expr(expr)?)),
            Statement::GuardReturn { cond, value } => {
                Ok(format!("if {} {{ return {}; }}", self.generate_expr(cond)?, self.generate_expr(value)?))
            }
            Statement::While { cond, body, .. } => {
                let cond_str = self.generate_expr(cond)?;
                let mut lines = Vec::new();
                for s in body {
                    let stmt_code = self.generate_statement(s, declared_vars, known_bools)?;
                    if !stmt_code.is_empty() {
                        let stmt_with_semi = if stmt_code.ends_with(';') || stmt_code.ends_with('}') {
                            stmt_code
                        } else {
                            format!("{};", stmt_code)
                        };
                        for line in stmt_with_semi.lines() {
                            lines.push(format!("  {}", line));
                        }
                    }
                }
                Ok(format!("while {} {{\n{}\n}}", cond_str, lines.join("\n")))
            }
        }
    }

    fn generate_function_def(&self, node: &FunctionDefNode) -> Result<String, CodegenError> {
        let mut params = Vec::new();
        for param in &node.params {
            params.push(format!("{}: Quantity", param));
        }
        let mut declared_vars: HashSet<String> = node.params.iter().cloned().collect();
        let mut known_bools: HashSet<String> = HashSet::new();
        let last_idx = node.body_stmts.len().saturating_sub(1);
        let mut body_lines = Vec::new();
        for (i, stmt) in node.body_stmts.iter().enumerate() {
            if i == last_idx {
                if let Statement::Expr(ref e) = stmt {
                    body_lines.push(format!("    {}", self.generate_expr(e)?));
                } else {
                    body_lines.push(format!("    {}", self.generate_statement(stmt, &mut declared_vars, &mut known_bools)?));
                }
            } else {
                body_lines.push(format!("    {}", self.generate_statement(stmt, &mut declared_vars, &mut known_bools)?));
            }
        }
        Ok(format!(
            "pub fn {}({}) -> Quantity {{\n{}\n}}",
            node.name,
            params.join(", "),
            body_lines.join("\n")
        ))
    }

    fn generate_assignment(&self, node: &AssignmentNode, declared_vars: &mut HashSet<String>, known_bools: &mut HashSet<String>) -> Result<String, CodegenError> {
        if super::is_definitely_bool(&node.value, known_bools) {
            known_bools.insert(node.name.clone());
        } else {
            known_bools.remove(&node.name);
        }
        let value = self.generate_expr(&node.value)?;
        if declared_vars.contains(&node.name) {
            Ok(format!("{} = {};", node.name, value))
        } else {
            declared_vars.insert(node.name.clone());
            Ok(format!("let mut {} = {};", node.name, value))
        }
    }
```

Note: `generate_export_shim` (later in the file) also calls `generate_function_def`, whose signature is unchanged here, so it needs no edit — but re-check it after this step (Step 3 below) since it's the one other place in this file that might independently call `generate_statement`/`generate_assignment`.

- [ ] **Step 2: Add `as_logical_op` handling and widen the nested-assert rejection**

In `generate_expr`'s `Expr::FunctionCall { name, args, kwargs }` arm, change:

```rust
            Expr::FunctionCall { name, args, kwargs } => {
                if let Some((op_sym, l, r)) = super::as_comparison_op(expr) {
                    let l_str = self.generate_expr(l)?;
                    let r_str = self.generate_expr(r)?;
                    return Ok(format!("({}.canonical_magnitude() {} {}.canonical_magnitude())", l_str, op_sym, r_str));
                }
                if !kwargs.is_empty() {
                    return Err(CodegenError::Generic(format!(
                        "Named arguments are not supported in Rust codegen (call to '{}')",
                        name
                    )));
                }
                if (name == "assert" || name == "exact_assert") && args.len() == 2 {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
```

to:

```rust
            Expr::FunctionCall { name, args, kwargs } => {
                if let Some((op_sym, l, r)) = super::as_comparison_op(expr) {
                    let l_str = self.generate_expr(l)?;
                    let r_str = self.generate_expr(r)?;
                    return Ok(format!("({}.canonical_magnitude() {} {}.canonical_magnitude())", l_str, op_sym, r_str));
                }
                if let Some(logical) = super::as_logical_op(expr) {
                    return Ok(match logical {
                        super::LogicalOp::Not(x) => format!("(!{})", self.generate_expr(x)?),
                        super::LogicalOp::And(l, r) => format!("({} && {})", self.generate_expr(l)?, self.generate_expr(r)?),
                        super::LogicalOp::Or(l, r) => format!("({} || {})", self.generate_expr(l)?, self.generate_expr(r)?),
                    });
                }
                if !kwargs.is_empty() {
                    return Err(CodegenError::Generic(format!(
                        "Named arguments are not supported in Rust codegen (call to '{}')",
                        name
                    )));
                }
                if (name == "assert" || name == "exact_assert") && matches!(args.len(), 1 | 2) {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
```

- [ ] **Step 3: Fix any remaining `known_bools`-related compile error**

Run: `cargo build -p physure-script 2>&1 | grep -B2 "error\[E0061\]"`
If `generate_export_shim` (or anything else in `rust.rs`) calls `generate_statement`/`generate_assignment` directly, thread a fresh local `known_bools: HashSet<String> = HashSet::new()` through it the same way Step 1 did for `generate_function_def`.

- [ ] **Step 4: Update the two existing assert tests for the new `as_assert_call` API (they still assert the same generated code, just via the new call path — verify, don't need to change assertions unless they broke)**

Run: `cargo test -p physure-script --lib codegen::rust::tests::transpiles_assert_call_to_phys_assert_call_rust codegen::rust::tests::transpiles_exact_assert_call_to_phys_exact_assert_call_rust 2>&1 | tail -20`

If these existing test names differ from what's above, find them with `grep -n "fn transpiles.*assert" physure-script/src/codegen/rust.rs` and confirm they still pass unmodified (they exercise the unchanged `Quantities` shape, so the assertions should not need to change).

- [ ] **Step 5: Add the new tests**

```rust
    #[test]
    fn transpiles_logical_operators_rust() {
        let tp = RustTranspiler;
        let not_expr = Expr::FunctionCall { name: "op_not".to_string(), args: vec![Expr::Bool(true)], kwargs: vec![] };
        assert_eq!(tp.generate_expr(&not_expr).unwrap(), "(!true)");
        let and_expr = Expr::FunctionCall { name: "op_and".to_string(), args: vec![Expr::Bool(true), Expr::Bool(false)], kwargs: vec![] };
        assert_eq!(tp.generate_expr(&and_expr).unwrap(), "(true && false)");
        let or_expr = Expr::FunctionCall { name: "op_or".to_string(), args: vec![Expr::Bool(false), Expr::Bool(true)], kwargs: vec![] };
        assert_eq!(tp.generate_expr(&or_expr).unwrap(), "(false || true)");
    }

    #[test]
    fn transpiles_one_argument_bool_assert_rust() {
        let tp = RustTranspiler;
        let program = crate::parser::parse_phs("assert(1.0 m > 0.0 m)").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains("assert!("), "{code}");
        assert!(!code.contains("phs_assert"), "a Bool assert must not call the Quantity method:\n{code}");
    }

    #[test]
    fn transpiles_bool_assert_with_message_rust() {
        let tp = RustTranspiler;
        let program = crate::parser::parse_phs("assert(1.0 m > 2.0 m, \"too small\")").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains("assert!(") && code.contains("\"too small\""), "{code}");
    }

    #[test]
    fn rejects_one_argument_assert_nested_in_an_expression_rust() {
        let tp = RustTranspiler;
        let program = crate::parser::parse_phs("x = assert(1.0 m > 0.0 m)").unwrap();
        assert!(tp.generate_program(&program).is_err());
    }
```

(add these to `rust.rs`'s existing `#[cfg(test)] mod tests` block; if there is none at the bottom of the file, check `physure-script/src/codegen/rust.rs`'s tail for where existing Rust codegen tests already live — e.g. via `grep -n "mod tests" physure-script/src/codegen/rust.rs` — and add there.)

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p physure-script --lib codegen::rust`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add physure-script/src/codegen/rust.rs
git commit -m "feat(phs): Rust codegen for not/and/or and the Bool assert overloads"
```

---

### Task 10: Java codegen

**Files:**
- Modify: `physure-script/src/codegen/java.rs`

- [ ] **Step 1: Add a shared `generate_java_assignment` helper and use it from both call sites**

In `physure-script/src/codegen/java.rs`, add this new private method inside `impl JavaTranspiler` (this replaces the assignment-handling logic currently duplicated inline in both `generate_program` and `generate_statement`):

```rust
    /// Emits one Java local-variable declaration or reassignment, tracking `known_bool_vars`
    /// so a later use of this name (another assignment, or an `assert` call) knows whether it
    /// holds a `boolean`. `indent` is the caller's own per-line prefix.
    fn generate_java_assignment(
        &self,
        node: &AssignmentNode,
        indent: &str,
        declared_vars: &mut HashSet<String>,
        known_bool_vars: &mut HashSet<String>,
    ) -> Result<String, CodegenError> {
        let val = self.generate_expr(&node.value)?;
        let var_name = snake_to_camel(&node.name);
        let is_reassign = declared_vars.contains(&var_name);
        let is_bool_now = super::is_definitely_bool(&node.value, known_bool_vars);

        if is_reassign && known_bool_vars.contains(&var_name) && !is_bool_now {
            return Err(CodegenError::Generic(format!(
                "'{}' was previously assigned a Bool expression and cannot be reassigned to a non-Bool value in Java codegen",
                node.name
            )));
        }
        if !is_reassign {
            declared_vars.insert(var_name.clone());
        }
        if is_bool_now {
            known_bool_vars.insert(var_name.clone());
        } else {
            known_bool_vars.remove(&var_name);
        }

        if is_reassign {
            return Ok(format!("{}{} = {};", indent, var_name, val));
        }
        if is_bool_now {
            return Ok(format!("{}boolean {} = {};", indent, var_name, val));
        }
        let literal = match &node.value {
            Expr::Str(text) => Some(text.as_str()),
            _ => None,
        };
        let is_equation = literal.is_some_and(|t| t.contains('=') && !t.contains('{'));
        if literal.is_some() && !is_equation {
            Ok(format!("{}String {} = {};", indent, var_name, val))
        } else if (val.starts_with('"') && val.contains('=')) || val.contains(".solve(") || val.starts_with("PhyEquation") {
            Ok(format!("{}PhyEquation {} = {};", indent, var_name, if val.starts_with('"') { format!("PhyEquation.of({})", val) } else { val }))
        } else if val.starts_with("PhyFunction") {
            Ok(format!("{}PhyFunction {} = {};", indent, var_name, val))
        } else {
            Ok(format!("{}Quantity {} = {};", indent, var_name, val))
        }
    }
```

- [ ] **Step 2: Rewrite `generate_program`**

Replace the whole `fn generate_program` body with:

```rust
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError> {
        let mut out = String::new();
        out.push_str(concat!("// Generated by PhysureScript (PHS) Compiler v", env!("CARGO_PKG_VERSION"), "\n"));
        out.push_str("import com.physure.Quantity;\n");
        out.push_str("import com.physure.PhyEquation;\n");
        out.push_str("import com.physure.PhyFunction;\n");
        out.push_str("import java.util.*;\n\n");
        out.push_str(&format!("public class {} {{\n", self.class_name));

        let mut functions = Vec::new();
        let mut main_stmts = Vec::new();
        let mut main_declared_vars = HashSet::new();
        let mut known_bool_vars: HashSet<String> = HashSet::new();

        for stmt in &program.statements {
            match stmt {
                Statement::FunctionDef(f) => {
                    functions.push(self.generate_function_def_stmt(f)?);
                }
                Statement::Assignment(node) => {
                    main_stmts.push(self.generate_java_assignment(node, "        ", &mut main_declared_vars, &mut known_bool_vars)?);
                    let var_name = snake_to_camel(&node.name);
                    main_stmts.push(format!("        System.out.println(\"{}: \" + {});", node.name, var_name));
                }
                Statement::Expr(expr) => {
                    match super::as_assert_call(expr, &known_bool_vars) {
                        Some(super::AssertShape::Bool { condition }) => {
                            let cond_code = self.generate_expr(condition)?;
                            main_stmts.push(format!("        if (!({})) throw new AssertionError(\"assertion failed\");", cond_code));
                        }
                        Some(super::AssertShape::BoolWithMessage { condition, message }) => {
                            let cond_code = self.generate_expr(condition)?;
                            let msg_code = self.generate_expr(message)?;
                            main_stmts.push(format!("        if (!({})) throw new AssertionError({});", cond_code, msg_code));
                        }
                        Some(super::AssertShape::Quantities { kind, actual, expected }) => {
                            let a_code = self.generate_expr(actual)?;
                            let b_code = self.generate_expr(expected)?;
                            let method = if kind == "assert" { "physAssert" } else { "physExactAssert" };
                            main_stmts.push(format!("        {}.{}({});", a_code, method, b_code));
                        }
                        None => {
                            let val = self.generate_expr(expr)?;
                            main_stmts.push(format!("        System.out.println({});", val));
                        }
                    }
                }
                Statement::While { .. } => {
                    main_stmts.push(format!("        {}", self.generate_statement(stmt, &mut main_declared_vars, &mut known_bool_vars)?));
                }
                _ => {}
            }
        }

        for func in functions {
            out.push_str(&func);
            out.push('\n');
        }

        if !main_stmts.is_empty() {
            out.push_str("    public static void main(String[] args) {\n");
            out.push_str(&main_stmts.join("\n"));
            out.push_str("\n    }\n");
        }

        out.push_str("}\n");
        Ok(out)
    }
```

- [ ] **Step 3: Rewrite `generate_function_def_stmt` and `generate_statement`**

```rust
    fn generate_function_def_stmt(&self, f: &FunctionDefNode) -> Result<String, CodegenError> {
        let mut out = String::new();
        out.push_str(&format!("    public static Quantity {}(", snake_to_camel(&f.name)));
        let params: Vec<String> = f.params.iter().map(|p| format!("Quantity {}", snake_to_camel(p))).collect();
        out.push_str(&params.join(", "));
        out.push_str(") {\n");
        let mut fn_declared_vars = HashSet::new();
        let mut fn_known_bools: HashSet<String> = HashSet::new();
        for p in &f.params {
            fn_declared_vars.insert(snake_to_camel(p));
        }
        let last_idx = f.body_stmts.len().saturating_sub(1);
        for (i, stmt) in f.body_stmts.iter().enumerate() {
            if i == last_idx {
                if let Statement::Expr(ref e) = stmt {
                    out.push_str(&format!("        return {};\n", self.generate_expr(e)?));
                } else {
                    out.push_str(&format!("        {};\n", self.generate_statement(stmt, &mut fn_declared_vars, &mut fn_known_bools)?));
                }
            } else {
                out.push_str(&format!("        {};\n", self.generate_statement(stmt, &mut fn_declared_vars, &mut fn_known_bools)?));
            }
        }
        out.push_str("    }\n");
        Ok(out)
    }

    fn generate_statement(&self, stmt: &Statement, declared_vars: &mut HashSet<String>, known_bool_vars: &mut HashSet<String>) -> Result<String, CodegenError> {
        match stmt {
            Statement::FunctionDef(f) => self.generate_function_def_stmt(f),
            Statement::Assignment(node) => self.generate_java_assignment(node, "", declared_vars, known_bool_vars),
            Statement::Expr(expr) => {
                match super::as_assert_call(expr, known_bool_vars) {
                    Some(super::AssertShape::Bool { condition }) => {
                        Ok(format!("if (!({})) throw new AssertionError(\"assertion failed\")", self.generate_expr(condition)?))
                    }
                    Some(super::AssertShape::BoolWithMessage { condition, message }) => {
                        Ok(format!("if (!({})) throw new AssertionError({})", self.generate_expr(condition)?, self.generate_expr(message)?))
                    }
                    Some(super::AssertShape::Quantities { kind, actual, expected }) => {
                        let a_code = self.generate_expr(actual)?;
                        let b_code = self.generate_expr(expected)?;
                        let method = if kind == "assert" { "physAssert" } else { "physExactAssert" };
                        Ok(format!("{}.{}({})", a_code, method, b_code))
                    }
                    None => self.generate_expr(expr),
                }
            }
            Statement::Return(expr) => Ok(format!("return {}", self.generate_expr(expr)?)),
            Statement::GuardReturn { cond, value } => {
                Ok(format!("if ({}) {{ return {}; }}", self.generate_expr(cond)?, self.generate_expr(value)?))
            }
            Statement::While { cond, body, .. } => {
                let cond_str = self.generate_expr(cond)?;
                let mut lines = Vec::new();
                for s in body {
                    let stmt_code = self.generate_statement(s, declared_vars, known_bool_vars)?;
                    if !stmt_code.is_empty() {
                        let stmt_with_semi = if stmt_code.ends_with(';') || stmt_code.ends_with('}') {
                            stmt_code
                        } else {
                            format!("{};", stmt_code)
                        };
                        for line in stmt_with_semi.lines() {
                            lines.push(format!("  {}", line));
                        }
                    }
                }
                Ok(format!("while ({}) {{\n{}\n}}", cond_str, lines.join("\n")))
            }
            _ => Ok(String::new()),
        }
    }
```

- [ ] **Step 4: Add `as_logical_op` handling and widen the nested-assert rejection**

In `generate_expr`'s `Expr::FunctionCall { name, args, kwargs }` arm, change:

```rust
                if !kwargs.is_empty() {
                    return Err(CodegenError::Generic(format!(
                        "Named arguments are not supported in Java codegen (call to '{}')",
                        name
                    )));
                }
                if (name == "assert" || name == "exact_assert") && args.len() == 2 {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
```

to:

```rust
                if let Some(logical) = super::as_logical_op(expr) {
                    return Ok(match logical {
                        super::LogicalOp::Not(x) => format!("(!{})", self.generate_expr(x)?),
                        super::LogicalOp::And(l, r) => format!("({} && {})", self.generate_expr(l)?, self.generate_expr(r)?),
                        super::LogicalOp::Or(l, r) => format!("({} || {})", self.generate_expr(l)?, self.generate_expr(r)?),
                    });
                }
                if !kwargs.is_empty() {
                    return Err(CodegenError::Generic(format!(
                        "Named arguments are not supported in Java codegen (call to '{}')",
                        name
                    )));
                }
                if (name == "assert" || name == "exact_assert") && matches!(args.len(), 1 | 2) {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
```

- [ ] **Step 5: Add the new tests**

```rust
    #[test]
    fn transpiles_logical_operators_java() {
        let tp = JavaTranspiler::default();
        let and_expr = Expr::FunctionCall { name: "op_and".to_string(), args: vec![Expr::Bool(true), Expr::Bool(false)], kwargs: vec![] };
        assert_eq!(tp.generate_expr(&and_expr).unwrap(), "(true && false)");
    }

    #[test]
    fn named_bool_assignment_gets_an_explicit_boolean_type_java() {
        let tp = JavaTranspiler::default();
        let program = crate::parser::parse_phs("ok = 1.0 m > 0.0 m\nassert(ok, \"should hold\")").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains("boolean ok = "), "expected an explicit boolean declaration:\n{code}");
        assert!(code.contains("throw new AssertionError(\"should hold\")"), "{code}");
    }

    #[test]
    fn reassigning_a_known_bool_variable_to_a_quantity_is_a_codegen_error_java() {
        let tp = JavaTranspiler::default();
        let program = crate::parser::parse_phs("ok = 1.0 m > 0.0 m\nok = 5.0 m\n").unwrap();
        assert!(tp.generate_program(&program).is_err());
    }

    #[test]
    fn rejects_one_argument_assert_nested_in_an_expression_java() {
        let tp = JavaTranspiler::default();
        let program = crate::parser::parse_phs("x = assert(1.0 m > 0.0 m)").unwrap();
        assert!(tp.generate_program(&program).is_err());
    }
```

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test -p physure-script --lib codegen::java`
Expected: PASS — including the pre-existing `transpiles_assert_call_to_a_java_...`/similar tests (`grep -n "fn transpiles.*assert" physure-script/src/codegen/java.rs` to confirm their exact names), which exercise the unchanged `Quantities` shape via the new `generate_java_assignment`/`as_assert_call` path and should need no assertion changes.

- [ ] **Step 7: Commit**

```bash
git add physure-script/src/codegen/java.rs
git commit -m "feat(phs): Java codegen for not/and/or, the Bool assert overloads, and explicit boolean locals"
```

---

### Task 11: JavaScript/TypeScript codegen

**Files:**
- Modify: `physure-script/src/codegen/js.rs`

- [ ] **Step 1: Rewrite `generate_program`'s `Statement::Assignment`/`Statement::Expr` handling**

In `physure-script/src/codegen/js.rs`, change the body of `generate_program`:

```rust
    fn generate_program(&self, program: &Program) -> Result<String, CodegenError> {
        let mut out = String::new();
        out.push_str(concat!("// Generated by PhysureScript (PHS) Compiler v", env!("CARGO_PKG_VERSION"), "\n"));
        out.push_str("import { Quantity } from \"physure\";\n\n");

        let mut functions = Vec::new();
        let mut main_stmts = Vec::new();
        let mut main_declared_vars = HashSet::new();
        let mut known_bools: HashSet<String> = HashSet::new();
        let reassigned = names_reassigned_in_loops(&program.statements);

        for stmt in &program.statements {
            match stmt {
                Statement::FunctionDef(f) => {
                    functions.push(self.generate_function_def_stmt(f)?);
                }
                Statement::Assignment(node) => {
                    let val = self.generate_expr(&node.value)?;
                    let var_name = snake_to_camel(&node.name);
                    let is_str_literal = matches!(&node.value, Expr::Str(_));
                    let is_bool_now = super::is_definitely_bool(&node.value, &known_bools);
                    if is_bool_now {
                        known_bools.insert(node.name.clone());
                    } else {
                        known_bools.remove(&node.name);
                    }
                    main_declared_vars.insert(var_name.clone());
                    let keyword = if reassigned.contains(&var_name) { "let" } else { "const" };
                    if self.typed {
                        let ty = if is_bool_now { "boolean" } else if is_str_literal { "string" } else { "Quantity" };
                        main_stmts.push(format!("{} {}: {} = {};", keyword, var_name, ty, val));
                    } else {
                        main_stmts.push(format!("{} {} = {};", keyword, var_name, val));
                    }
                    main_stmts.push(format!("console.log(`{}: ${{{}}}`);", node.name, var_name));
                }
                Statement::Expr(expr) => {
                    match super::as_assert_call(expr, &known_bools) {
                        Some(super::AssertShape::Bool { condition }) => {
                            let cond_code = self.generate_expr(condition)?;
                            main_stmts.push(format!("if (!({})) throw new Error(\"assertion failed\");", cond_code));
                        }
                        Some(super::AssertShape::BoolWithMessage { condition, message }) => {
                            let cond_code = self.generate_expr(condition)?;
                            let msg_code = self.generate_expr(message)?;
                            main_stmts.push(format!("if (!({})) throw new Error({});", cond_code, msg_code));
                        }
                        Some(super::AssertShape::Quantities { kind, actual, expected }) => {
                            let a_code = self.generate_expr(actual)?;
                            let b_code = self.generate_expr(expected)?;
                            let method = if kind == "assert" { "physAssert" } else { "physExactAssert" };
                            main_stmts.push(format!("{}.{}({});", a_code, method, b_code));
                        }
                        None => {
                            let val = self.generate_expr(expr)?;
                            main_stmts.push(format!("console.log({});", val));
                        }
                    }
                }
                Statement::While { .. } => {
                    main_stmts.push(self.generate_statement(stmt, &mut main_declared_vars)?);
                }
                _ => {}
            }
        }

        for func in functions {
            out.push_str(&func);
            out.push('\n');
        }

        for stmt in main_stmts {
            out.push_str(&stmt);
            out.push('\n');
        }

        Ok(out)
    }
```

(`generate_statement`'s signature is unchanged — it never emits type annotations for either JS or TS, and, matching the pre-existing behavior this design does not change, `assert(...)` inside a function body or `while` loop is not supported for this target, same as before.)

- [ ] **Step 2: Add `as_logical_op` handling and widen the nested-assert rejection**

In `generate_expr`'s `Expr::FunctionCall { name, args, kwargs }` arm, change:

```rust
                if !kwargs.is_empty() {
                    return Err(CodegenError::Generic(format!(
                        "Named arguments are not supported in JS/TS codegen (call to '{}')",
                        name
                    )));
                }
                if (name == "assert" || name == "exact_assert") && args.len() == 2 {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
```

to:

```rust
                if let Some(logical) = super::as_logical_op(expr) {
                    return Ok(match logical {
                        super::LogicalOp::Not(x) => format!("(!{})", self.generate_expr(x)?),
                        super::LogicalOp::And(l, r) => format!("({} && {})", self.generate_expr(l)?, self.generate_expr(r)?),
                        super::LogicalOp::Or(l, r) => format!("({} || {})", self.generate_expr(l)?, self.generate_expr(r)?),
                    });
                }
                if !kwargs.is_empty() {
                    return Err(CodegenError::Generic(format!(
                        "Named arguments are not supported in JS/TS codegen (call to '{}')",
                        name
                    )));
                }
                if (name == "assert" || name == "exact_assert") && matches!(args.len(), 1 | 2) {
                    return Err(CodegenError::Generic(format!(
                        "'{}' can only be used as a standalone statement, not nested inside an expression",
                        name
                    )));
                }
```

- [ ] **Step 3: Add the new tests**

```rust
    #[test]
    fn transpiles_logical_operators_js() {
        let tp = JsTranspiler::default();
        let or_expr = Expr::FunctionCall { name: "op_or".to_string(), args: vec![Expr::Bool(false), Expr::Bool(true)], kwargs: vec![] };
        assert_eq!(tp.generate_expr(&or_expr).unwrap(), "(false || true)");
    }

    #[test]
    fn named_bool_assignment_gets_an_explicit_boolean_type_ts() {
        let tp = JsTranspiler { typed: true };
        let program = crate::parser::parse_phs("ok = 1.0 m > 0.0 m\nassert(ok, \"should hold\")").unwrap();
        let code = tp.generate_program(&program).unwrap();
        assert!(code.contains(": boolean ="), "expected an explicit boolean annotation:\n{code}");
        assert!(code.contains("throw new Error(\"should hold\")"), "{code}");
    }

    #[test]
    fn rejects_one_argument_assert_nested_in_an_expression_js() {
        let tp = JsTranspiler::default();
        let program = crate::parser::parse_phs("x = assert(1.0 m > 0.0 m)").unwrap();
        assert!(tp.generate_program(&program).is_err());
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p physure-script --lib codegen::js`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add physure-script/src/codegen/js.rs
git commit -m "feat(phs): JS/TS codegen for not/and/or, the Bool assert overloads, and explicit boolean TS locals"
```

---

### Task 12: Execution-based parity tests (`python -O`, Java without `-ea`)

**Files:**
- Modify: `physure-script/tests/transpile_parity_tests.rs`

- [ ] **Step 1: Write the failing tests**

Add to `physure-script/tests/transpile_parity_tests.rs` (near the bottom, after `test_rust_transpiler_parity`):

```rust
#[test]
fn test_python_bool_assert_survives_dash_o() {
    let py_dir = repo_root().join("physure-python");
    let importable = Command::new("uv")
        .args(["run", "python", "-c", "import physure"])
        .current_dir(&py_dir)
        .output()
        .is_ok_and(|o| o.status.success());
    if !importable {
        eprintln!("skipping: `uv run python -c 'import physure'` does not work here");
        return;
    }

    // A False boolean assertion with a message. Python's `assert` statement is stripped by
    // `-O`; the generated code must not depend on it (see the "Assertion emission" table in
    // the design spec).
    let program = parse_phs("assert(False, \"boom\")").unwrap();
    let py_code = transpile(&program, Target::Python).unwrap();
    assert!(!py_code.contains("\nassert "), "must not emit a removable `assert` statement:\n{py_code}");

    for flag in [None, Some("-O")] {
        let temp_file = std::env::temp_dir().join(format!("bool_assert_{}.py", flag.unwrap_or("plain")));
        fs::write(&temp_file, &py_code).unwrap();
        let mut args = vec!["run", "python"];
        if let Some(f) = flag {
            args.push(f);
        }
        let file_str = temp_file.to_str().unwrap().to_string();
        args.push(&file_str);
        let output = Command::new("uv").args(&args).current_dir(&py_dir).output().expect("failed to run python");
        let _ = fs::remove_file(&temp_file);
        assert!(
            !output.status.success(),
            "expected assert(False, ...) to fail under {:?}, but it exited 0", flag
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("boom"),
            "expected the assertion message under {:?}, got: {}", flag, String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn test_java_bool_assert_fails_without_dash_ea() {
    let java_src_dir = repo_root().join("physure-java/src/main/java");
    let temp_dir = std::env::temp_dir().join("phs_java_bool_assert");
    let _ = fs::create_dir_all(&temp_dir);

    // The generated program always imports com.physure.* (even unused), so those classes
    // must be on the classpath to compile -- but a Bool-only assert never *calls* into them,
    // so no native library is loaded at runtime and `native_lib_dir()`/`-Djava.library.path`
    // aren't needed here.
    let compile_base = match Command::new("javac")
        .args(["-d", temp_dir.to_str().unwrap(), &format!("{}/com/physure/Quantity.java", java_src_dir.to_str().unwrap())])
        .output()
    {
        Ok(o) => o,
        Err(_) => {
            eprintln!("skipping: javac not found");
            return;
        }
    };
    if !compile_base.status.success() {
        eprintln!("skipping: base com.physure classes failed to compile: {}", String::from_utf8_lossy(&compile_base.stderr));
        return;
    }

    let program = parse_phs("assert(False, \"boom\")").unwrap();
    let java_code = transpile(&program, Target::JavaWithClass("BoolAssert".to_string())).unwrap();
    let gen_file = temp_dir.join("BoolAssert.java");
    fs::write(&gen_file, &java_code).unwrap();

    let compile_gen = Command::new("javac")
        .args(["-cp", temp_dir.to_str().unwrap(), "-d", temp_dir.to_str().unwrap(), gen_file.to_str().unwrap()])
        .output()
        .expect("failed to compile generated java");
    assert!(compile_gen.status.success(), "javac failed: {}", String::from_utf8_lossy(&compile_gen.stderr));

    // Deliberately no `-ea`: JVM assertions are off by default, so if the generated code
    // relied on the language `assert` keyword this would silently pass instead of throwing.
    let run = Command::new("java")
        .args(["-cp", temp_dir.to_str().unwrap(), "BoolAssert"])
        .output()
        .expect("failed to run java");
    let _ = fs::remove_dir_all(&temp_dir);

    assert!(!run.status.success(), "expected assert(False, ...) to fail even without -ea");
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("boom"),
        "expected the assertion message, got: {}", String::from_utf8_lossy(&run.stderr)
    );
}
```

- [ ] **Step 2: Run to verify they pass (or gracefully skip where the toolchain is unavailable)**

Run: `cargo test -p physure-script --test transpile_parity_tests bool_assert -- --nocapture`
Expected: PASS where `uv`/`python`/`javac`/`java` are available; a printed "skipping: ..." line and a passing (no-op) test otherwise. Either outcome is acceptable — do not install toolchains to force execution.

- [ ] **Step 3: Commit**

```bash
git add physure-script/tests/transpile_parity_tests.rs
git commit -m "test(phs): prove generated Bool assertions survive python -O and run without java -ea"
```

---

### Task 13: Full workspace verification

- [ ] **Step 1: Rebuild the Rust core bindings** (required after touching `physure-script`, which `physure-python`'s extension links against)

Run (background, long timeout — a full workspace build/test can take several minutes):

```bash
cd physure-core && maturin develop && cd ..
```

- [ ] **Step 2: Build and test the whole workspace**

Run:

```bash
cargo build --workspace
cargo test -p physure-script
cargo test --workspace
```

Expected: PASS. Pay particular attention to any *other* crate (`physure-cli`, `physure-lsp`, `physure-java`, `physure-wasm`) that might have its own exhaustive match over `physure_script::ast::Expr` — Task 1's Step 4 only checked `physure-script` itself.

- [ ] **Step 3: Grep for pre-existing PHS scripts that used the newly-reserved words as identifiers**

Run:

```bash
grep -rnE '\b(not|and|or|True|False)\s*=[^=]' --include=*.phs . 2>/dev/null
grep -rnE '"\b(not|and|or|True|False)\b[^a-zA-Z_]' physure-script/tests physure-python/tests 2>/dev/null | grep -i "parse_phs\|eval_str"
```

If either turns up a script that assigns to (or declares a function/parameter named) one of these five words, rename it — this is the intentional breaking change the design spec calls out under "Compatibility and migration."

- [ ] **Step 4: Python lint/format/tests**

Run:

```bash
uv run ruff check .
uv run ruff format --check .
uv run pytest
```

Expected: PASS (this feature touches no Python source, but `physure-python`'s test suite exercises the rebuilt Rust extension end-to-end, e.g. anything that calls `phs`/transpile through the Python bindings).

- [ ] **Step 5: Commit** only if any of the above steps required a fix; otherwise there is nothing to commit for this task.

---

### Task 14: Documentation

**Files:**
- Modify: `docs/tutorials/phs_primer.md`

- [ ] **Step 1: Add a new numbered section**

`docs/tutorials/phs_primer.md` currently has no section on comparisons, booleans, or assertions at all (`grep -n "assert\|Boolean" docs/tutorials/phs_primer.md` returns nothing) — this is the closest thing this repo has to a PHS language reference, so add the new content there rather than inventing a new page. Insert a new section **before** "## Break it on purpose" (its current last section), numbered to continue the existing sequence (the file currently ends at "## 5. The closing deliverable..."):

```markdown
## 6. Booleans, `not`/`and`/`or`, and assertions

Comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`, `≈`) return a real boolean, `True` or `False`.
There is no implicit truthiness: a quantity, a number, or a string is never treated as a
condition on its own.

```phs
pressure > 0 Pa
count != 0
not (sensor_disconnected)
denominator != 0 and numerator / denominator > limit
```

`and` and `or` short-circuit: `False and rhs` and `True or rhs` never evaluate `rhs` at all,
so a guard like the denominator check above is safe even when `rhs` would otherwise raise.
`not` binds tighter than `and`, which binds tighter than `or` — parenthesize a mixed condition
so a later reader doesn't have to work it out:

```phs
(not (pressure > limit) and enabled) or override
```

`assert` takes either a boolean condition (with an optional message) or two quantities to
compare directly:

```phs
assert(power > 0 kW)
assert(pressure >= minimum_pressure, "V-PUMP-004: pressure is below the operating range")
assert(actual, expected)          # existing form: dimensional + magnitude tolerance check
exact_assert(actual, expected)    # existing form: exact unit and magnitude match
```

`assert(actual, expected)` and `exact_assert(actual, expected)` still expect two `Quantity`
values — `assert(actual == expected)` is the boolean form, and gives a less specific failure
message (it doesn't know *how much* the two differ, only that they weren't equal), so prefer
naming the comparison you actually mean.

Prefer several small assertions with descriptive messages over one large compound condition —
a boolean built from named domain predicates (`is_within_tolerance and is_powered`) reads
better and fails with a clearer message than one long inline expression.

A sigma-bound condition like `assert(result == reference +/- 2 sigma)` parses and runs in the
interpreter, but its behavior is not yet identical across every transpile target (see
`docs/uncertainty-gum-compliance.md`) — use `assert(actual, expected)` instead when the check
needs to produce the same result in every generated language.
```

- [ ] **Step 2: Verify the docs build (mkdocs `--strict` fails CI on any broken link or nav issue — see this repo's docs link policy in `CLAUDE.md`)**

Run:

```bash
cd physure-python && uv run mkdocs build --strict && cd ..
```

Expected: PASS. This edit adds no new links, so this is a quick regression check.

- [ ] **Step 3: Commit**

```bash
git add docs/tutorials/phs_primer.md
git commit -m "docs(phs): document booleans, not/and/or, and the assert(Bool) forms"
```

---

## Self-review notes

- **Spec coverage:** boolean literals (Task 2-3), logical operators + short-circuit (Task 4), boolean equality (Task 5), all three `assert`/`exact_assert` overloads incl. the accepted-signature diagnostic (Task 6), transpiler parity for literals/operators/assertions across all four targets (Tasks 8-11), named boolean locals for Java/typed-TS (Tasks 10-11), the statement-only rejection widened to the new 1-arg form (Tasks 8-11 Step "widen the nested-assert rejection"), `python -O`/Java-without-`-ea` proof (Task 12), sigma-bound parity caveat (Task 6 test + Task 14 docs). `exact_assert` deliberately gets no Bool overload anywhere (Task 6, Task 7's `as_assert_call` only special-cases `"assert"`).
- **Out of scope, confirmed during design research and intentionally not touched:** `physure-python`, `physure-java`, `physure-wasm`, `physure-lsp` package/binding code — every boolean assertion transpiles to plain native code in the generated file itself, never a call into a binding library method, so none of those crates need changes. Ordering comparisons (`<`, `<=`, `>`, `>=`, `≈`) against a `Bool` operand are left exactly as they behave today (silently non-matching, via the existing `compare()` fallback) since the spec only asks that they "remain invalid," not that a new diagnostic be added there.
- **Placeholder scan:** every step above shows the real, final code (or an exact grep/build command with a stated expected outcome) — the only steps that defer to "whatever the compiler says" are Task 1 Step 4 (find every exhaustive match — a compiler-enumerated, deterministic list, not a vague TODO) and Task 9 Step 3 / Task 13 Step 2 (the same pattern for `physure-script`'s one other call site and any other workspace crate), both scoped to "thread `known_bools`/add an `Expr::Bool` arm exactly like the sibling case already shown."
- **Type/API consistency check:** `AssertShape`/`LogicalOp` (Task 7) are used with the same variant names and field names in every one of Tasks 8-11. `is_definitely_bool(expr, &known_bools)` and `as_assert_call(expr, &known_bools)` take the same argument order everywhere. `PhsValue::type_name()` (Task 4) is reused by both the logical-operator errors (Task 4) and the assert/equality diagnostics (Tasks 5-6).
