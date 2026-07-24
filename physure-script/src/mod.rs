pub mod ast;
pub mod builtins;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod value;
pub mod function;

pub use ast::{Expr, Statement};
pub use interpreter::PhsInterpreter;
pub use lexer::{PhsLexer, PhsToken, TokenKind};
pub use parser::PhsParser;
pub use value::PhsValue;
pub use function::PhyFunction;

pub fn parse_phs(input: &str) -> physure_core::error::PhysureResult<Vec<Statement>> {
    let lexer = PhsLexer::new(input);
    let tokens = lexer.tokenize()?;
    let mut parser = PhsParser::new(&tokens);
    parser.parse_statements()
}

pub fn eval_phs(input: &str) -> physure_core::error::PhysureResult<Vec<PhsValue>> {
    let mut interpreter = PhsInterpreter::new();
    let mut results = Vec::new();
    let mut pos = 0;

    while let Some(start_idx) = input[pos..].find("```") {
        let abs_start = pos + start_idx;
        let prefix = &input[pos..abs_start];
        run_segment(prefix, &mut interpreter, &mut results)?;

        let text_start = abs_start + 3;
        if let Some(end_idx) = input[text_start..].find("```") {
            let abs_end = text_start + end_idx;
            let text_content = &input[text_start..abs_end];
            let clean_text = text_content.strip_prefix('\n').unwrap_or(text_content);
            let clean_text = clean_text.strip_suffix('\n').unwrap_or(clean_text);
            let val = interpreter.run_statement(&Statement::DisplayText(clean_text.to_string()))?;
            results.push(val);
            pos = abs_end + 3;
        } else {
            let text_content = &input[text_start..];
            let val = interpreter.run_statement(&Statement::DisplayText(text_content.to_string()))?;
            results.push(val);
            pos = input.len();
            break;
        }
    }

    if pos < input.len() {
        let tail = &input[pos..];
        run_segment(tail, &mut interpreter, &mut results)?;
    }

    Ok(results)
}

fn run_segment(
    segment: &str,
    interpreter: &mut PhsInterpreter,
    results: &mut Vec<PhsValue>,
) -> physure_core::error::PhysureResult<()> {
    let lines: Vec<&str> = segment.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let comment_split = line.split('#').next().unwrap_or("").trim_end();
        if comment_split.trim().is_empty() {
            i += 1;
            continue;
        }

        let trimmed = comment_split.trim();

        if (trimmed.ends_with('=') || trimmed.ends_with("->")) && trimmed.contains('(') && trimmed.contains(')') {
            let header = trimmed;
            let mut body_parts = Vec::new();
            i += 1;
            while i < lines.len() {
                let next_line = lines[i];
                let next_trim = next_line.split('#').next().unwrap_or("").trim();
                if next_trim.is_empty() {
                    i += 1;
                    continue;
                }
                if next_line.starts_with(' ') || next_line.starts_with('\t') {
                    body_parts.push(next_trim);
                    i += 1;
                } else {
                    break;
                }
            }

            let fn_str = if body_parts.is_empty() {
                header.to_string()
            } else {
                format!("{} {}", header, body_parts.join(" ; "))
            };

            let lexer = PhsLexer::new(&fn_str);
            let tokens = lexer.tokenize()?;
            let mut parser = PhsParser::new(&tokens);
            let stmts = parser.parse_statements()?;
            for stmt in stmts {
                let res = interpreter.run_statement(&stmt)?;
                results.push(res);
            }
            continue;
        }

        for sub_stmt in trimmed.split(';') {
            let stmt_str = sub_stmt.trim();
            if stmt_str.is_empty() {
                continue;
            }
            let lexer = PhsLexer::new(stmt_str);
            let tokens = lexer.tokenize()?;
            if tokens.is_empty() {
                continue;
            }
            let mut parser = PhsParser::new(&tokens);
            let stmts = parser.parse_statements()?;
            for stmt in stmts {
                let res = interpreter.run_statement(&stmt)?;
                results.push(res);
            }
        }
        i += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_simple_expression() {
        let results = eval_phs("10 + 20").unwrap();
        assert_eq!(results, vec![PhsValue::Number(30.0)]);
    }

    #[test]
    fn test_eval_variables_and_functions() {
        let source = r#"
            x = 5
            y = 10
            f(a, b) = a * b + 2
            f(x, y)
        "#;
        let results = eval_phs(source).unwrap();
        assert_eq!(results.last(), Some(&PhsValue::Number(52.0)));
    }

    #[test]
    fn test_eval_ternary_and_let() {
        let source = "let z = 3 in z > 2 ? 100 : 200";
        let results = eval_phs(source).unwrap();
        assert_eq!(results, vec![PhsValue::Number(100.0)]);
    }
}
