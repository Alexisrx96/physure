//! The names a program can never bind (spec `docs/superpowers/specs/2026-11-16-reserved-names-design.md`).
//!
//! Classes decided by the user: the keywords of the grammar, the statement words, the literals, the functions of
//! the core, and, while the program imports them, the functions of a module (`calc`, `array`, `plot`). The
//! constants of the catalog (`pi`, `π`, `[Constants]`) are reserved by [`crate::globals`]. Not reserved: unit
//! names, the words of a format, `e`, single letters.
//!
//! `kind` answers for the classes that do not depend on the program. A module function is reserved by what the
//! interpreter has unlocked, which only the interpreter knows (`PhsInterpreter::module_domain`).

/// The class of a reserved name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reserved {
    /// A keyword of the grammar (`phs.pest`, rule `keyword`). The grammar refuses it before any check runs.
    Keyword,
    /// A word the grammar reads in a statement: `use`, `import`, `fn`...
    StatementWord,
    /// A value the language names: `true`, `None`, `inf`...
    Literal,
    /// A function `eval_core_builtin` answers.
    CoreFunction,
}

/// The rule `keyword` of the grammar, word for word.
pub const KEYWORDS: [&str; 13] =
    ["for", "while", "let", "if", "then", "else", "where", "return", "not", "and", "or", "True", "False"];

/// The words of `import_stmt`, `export_stmt`, `function_def` and `for_expr`. `def` is not a word of PHS.
pub const STATEMENT_WORDS: [&str; 7] = ["use", "from", "import", "export", "as", "fn", "in"];

/// `True` and `False` are keywords; the lower-case forms and `None` are reserved so that they are never a second,
/// different thing. `inf`, `infinity` and `oo` are the infinity literals of the rule `number`. `NaN` is not a
/// literal of the language.
pub const LITERALS: [&str; 7] = ["true", "false", "None", "none", "inf", "infinity", "oo"];

/// The names `eval_core_builtin` answers, without the `op_*` names the operators are rewritten to.
pub const CORE_FUNCTIONS: [&str; 27] = [
    "abs", "approx", "assert", "ceil", "cos", "coverage_factor", "dof", "exact_assert", "exp", "expanded", "floor",
    "format", "from_samples", "if_then_else", "ln", "log", "max", "min", "parallel_map", "round", "sigma", "sin",
    "sqrt", "tan", "ternary", "uncertainty", "vector",
];

/// The class of `name` among the classes that do not depend on the program, `None` when it is free.
pub fn static_kind(name: &str) -> Option<Reserved> {
    if KEYWORDS.contains(&name) {
        Some(Reserved::Keyword)
    } else if STATEMENT_WORDS.contains(&name) {
        Some(Reserved::StatementWord)
    } else if LITERALS.contains(&name) {
        Some(Reserved::Literal)
    } else if CORE_FUNCTIONS.contains(&name) {
        Some(Reserved::CoreFunction)
    } else {
        None
    }
}

impl Reserved {
    fn describe(self) -> &'static str {
        match self {
            Reserved::Keyword | Reserved::StatementWord => "a word of the language",
            Reserved::Literal => "a literal of the language",
            Reserved::CoreFunction => "a function of PHS",
        }
    }
}

/// The error for binding the reserved `name` as `what` ("a variable", "a parameter of `f`", ...).
pub fn binding_error(name: &str, what: &str, kind: Reserved) -> String {
    format!("`{name}` is {} and cannot be bound as {what}; choose another name", kind.describe())
}

/// The error for binding `name`, a function of the module `domain` that the program imports, as `what`.
pub fn module_binding_error(name: &str, domain: &str, what: &str) -> String {
    format!("`{name}` is a function of `{domain}`, which this program imports, and cannot be bound as {what}; choose another name")
}

/// The error for importing the function `name` of `domain` over a name the program already holds.
pub fn module_import_error(name: &str, domain: &str) -> String {
    format!("`{name}` is a function of `{domain}`, which this program imports, and `{name}` is already bound in the program; choose another name for it")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut words: Vec<String>) -> Vec<String> {
        words.sort();
        words
    }

    #[test]
    fn the_keywords_are_those_of_the_grammar() {
        let pest = include_str!("phs.pest");
        let line = pest.lines().find(|l| l.starts_with("keyword")).expect("the rule `keyword`");
        let inside = line.split('(').nth(1).and_then(|rest| rest.split(')').next()).expect("the alternatives");
        let in_grammar: Vec<String> = inside.split('|').map(|w| w.trim().trim_matches('"').to_string()).collect();
        assert_eq!(sorted(in_grammar), sorted(KEYWORDS.iter().map(|w| w.to_string()).collect()));
    }

    #[test]
    fn the_core_functions_are_the_arms_of_eval_core_builtin() {
        let source = include_str!("builtins/core.rs");
        let start = source.find("pub fn eval_core_builtin(").expect("the dispatcher");
        let end = start + source[start..].find("_ => Ok(None),").expect("its last arm");
        let mut arms = Vec::new();
        for line in source[start..end].lines() {
            let line = line.trim_start();
            if !line.starts_with('"') {
                continue;
            }
            for part in line.split("=>").next().unwrap_or("").split('|') {
                let name = part.trim().trim_matches('"');
                if !name.starts_with("op_") {
                    arms.push(name.to_string());
                }
            }
        }
        assert_eq!(sorted(arms), sorted(CORE_FUNCTIONS.iter().map(|w| w.to_string()).collect()));
    }

    #[test]
    fn the_statement_words_and_the_infinity_literals_are_read_by_the_grammar() {
        let pest = include_str!("phs.pest");
        for word in STATEMENT_WORDS.iter().chain(["inf", "infinity", "oo"].iter()) {
            assert!(pest.contains(&format!("\"{word}\"")), "the grammar no longer reads `{word}`");
        }
    }

    #[test]
    fn no_name_is_in_two_classes() {
        let all: Vec<&str> = KEYWORDS.iter().chain(&STATEMENT_WORDS).chain(&LITERALS).chain(&CORE_FUNCTIONS).copied().collect();
        let mut unique = all.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(all.len(), unique.len());
    }

    #[test]
    fn a_name_is_free_unless_a_class_has_it() {
        assert_eq!(static_kind("max"), Some(Reserved::CoreFunction));
        assert_eq!(static_kind("use"), Some(Reserved::StatementWord));
        assert_eq!(static_kind("inf"), Some(Reserved::Literal));
        assert_eq!(static_kind("while"), Some(Reserved::Keyword));
        for free in ["m", "s", "kg", "e", "base", "frac", "x", "limit", "def", "NaN"] {
            assert_eq!(static_kind(free), None, "{free}");
        }
    }

    #[test]
    fn the_messages_name_the_class_and_the_way_out() {
        assert_eq!(
            binding_error("max", "a variable", Reserved::CoreFunction),
            "`max` is a function of PHS and cannot be bound as a variable; choose another name"
        );
        assert_eq!(
            binding_error("use", "a parameter of `f`", Reserved::StatementWord),
            "`use` is a word of the language and cannot be bound as a parameter of `f`; choose another name"
        );
        assert_eq!(
            module_binding_error("limit", "calc", "a variable"),
            "`limit` is a function of `calc`, which this program imports, and cannot be bound as a variable; choose another name"
        );
    }
}
