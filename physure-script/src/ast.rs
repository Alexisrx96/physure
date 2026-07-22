#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Sqrt,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    ApproxEq,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Ident(String),
    StringLiteral(String),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    ImplicitMul {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    Ternary {
        cond: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    Let {
        name: String,
        val: Box<Expr>,
        body: Box<Expr>,
    },
    If {
        cond: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    Vector(Vec<Expr>),
    Uncertainty {
        val: Box<Expr>,
        unc: Box<Expr>,
    },
    Convert {
        expr: Box<Expr>,
        target_unit: String,
    },
    FormatSig {
        expr: Box<Expr>,
        spec: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamDef {
    pub name: String,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Assign {
        name: String,
        expr: Expr,
    },
    Query {
        expr: Expr,
    },
    AssignAndQuery {
        name: String,
        expr: Expr,
    },
    Assert {
        left: Expr,
        right: Expr,
        op: BinaryOp,
    },
    FnDef {
        name: String,
        params: Vec<ParamDef>,
        body: Vec<Statement>,
    },
    DisplayText(String),
    ExprStmt(Expr),
}

impl UnaryOp {
    pub fn to_phs(&self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
            UnaryOp::Sqrt => "sqrt",
        }
    }
}

impl BinaryOp {
    pub fn to_phs(&self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Pow => "^",
            BinaryOp::Eq => "==",
            BinaryOp::Neq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::Lte => "<=",
            BinaryOp::Gte => ">=",
            BinaryOp::ApproxEq => "≈",
        }
    }

    pub fn to_latex(&self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "\\cdot",
            BinaryOp::Div => "/",
            BinaryOp::Pow => "^",
            BinaryOp::Eq => "=",
            BinaryOp::Neq => "\\neq",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::Lte => "\\le",
            BinaryOp::Gte => "\\ge",
            BinaryOp::ApproxEq => "\\approx",
        }
    }
}

impl Expr {
    pub fn to_phs(&self) -> String {
        match self {
            Expr::Number(n) => physure_core::quantity::format_float(*n),
            Expr::Ident(s) => s.clone(),
            Expr::StringLiteral(s) => format!("\"{}\"", s),
            Expr::Unary { op: UnaryOp::Sqrt, expr } => format!("sqrt({})", expr.to_phs()),
            Expr::Unary { op, expr } => format!("{}{}", op.to_phs(), expr.to_phs()),
            Expr::Binary { op, left, right } => format!("{} {} {}", left.to_phs(), op.to_phs(), right.to_phs()),
            Expr::ImplicitMul { left, right } => format!("{} {}", left.to_phs(), right.to_phs()),
            Expr::Call { name, args } => {
                let arg_strs: Vec<String> = args.iter().map(|a| a.to_phs()).collect();
                format!("{}({})", name, arg_strs.join(", "))
            }
            Expr::Ternary { cond, then_expr, else_expr } => format!("{} ? {} : {}", cond.to_phs(), then_expr.to_phs(), else_expr.to_phs()),
            Expr::Let { name, val, body } => format!("let {} = {} in {}", name, val.to_phs(), body.to_phs()),
            Expr::If { cond, then_expr, else_expr } => format!("if {} then {} else {}", cond.to_phs(), then_expr.to_phs(), else_expr.to_phs()),
            Expr::Vector(items) => {
                let item_strs: Vec<String> = items.iter().map(|i| i.to_phs()).collect();
                format!("[{}]", item_strs.join(", "))
            }
            Expr::Uncertainty { val, unc } => format!("{} +/- {}", val.to_phs(), unc.to_phs()),
            Expr::Convert { expr, target_unit } => format!("{} => {}", expr.to_phs(), target_unit),
            Expr::FormatSig { expr, spec } => format!("{}: {}", expr.to_phs(), spec),
        }
    }

    pub fn to_latex(&self) -> String {
        match self {
            Expr::Number(n) => {
                let s = physure_core::quantity::format_float(*n);
                if s.contains('e') || s.contains('E') {
                    let parts: Vec<&str> = s.split(['e', 'E']).collect();
                    if parts.len() == 2 {
                        format!("{} \\times 10^{{{}}}", parts[0], parts[1].trim_start_matches('+'))
                    } else {
                        s
                    }
                } else {
                    s
                }
            }
            Expr::Ident(s) => {
                if s.contains('_') {
                    let parts: Vec<&str> = s.split('_').collect();
                    let main = parts[0];
                    let sub = parts[1..].join("\\_");
                    format!("{{{}}}_{{{}}}", main, sub)
                } else {
                    s.clone()
                }
            }
            Expr::StringLiteral(s) => format!("\\text{{\"{}\"}}", s),
            Expr::Unary { op: UnaryOp::Sqrt, expr } => format!("\\sqrt{{{}}}", expr.to_latex()),
            Expr::Unary { op, expr } => format!("{}{}", op.to_phs(), expr.to_latex()),
            Expr::Binary { op: BinaryOp::Div, left, right } => format!("\\frac{{{}}}{{{}}}", left.to_latex(), right.to_latex()),
            Expr::Binary { op: BinaryOp::Pow, left, right } => format!("{{{}}}^{{{}}}", left.to_latex(), right.to_latex()),
            Expr::Binary { op, left, right } => format!("{} {} {}", left.to_latex(), op.to_latex(), right.to_latex()),
            Expr::ImplicitMul { left, right } => format!("{} \\; {}", left.to_latex(), right.to_latex()),
            Expr::Call { name, args } => {
                let arg_strs: Vec<String> = args.iter().map(|a| a.to_latex()).collect();
                format!("\\text{{{}}}({})", name, arg_strs.join(", "))
            }
            Expr::Ternary { cond, then_expr, else_expr } => format!("{} \\text{{ ? }} {} \\text{{ : }} {}", cond.to_latex(), then_expr.to_latex(), else_expr.to_latex()),
            Expr::Let { name, val, body } => format!("\\text{{let }} {} = {} \\text{{ in }} {}", name, val.to_latex(), body.to_latex()),
            Expr::If { cond, then_expr, else_expr } => format!("\\text{{if }} {} \\text{{ then }} {} \\text{{ else }} {}", cond.to_latex(), then_expr.to_latex(), else_expr.to_latex()),
            Expr::Vector(items) => {
                let item_strs: Vec<String> = items.iter().map(|i| i.to_latex()).collect();
                format!("[{}]", item_strs.join(", "))
            }
            Expr::Uncertainty { val, unc } => format!("{} \\pm {}", val.to_latex(), unc.to_latex()),
            Expr::Convert { expr, target_unit } => {
                let clean_u = target_unit.replace('_', "\\_").replace('*', " \\cdot ");
                format!("{} \\implies \\text{{{}}}", expr.to_latex(), clean_u)
            }
            Expr::FormatSig { expr, .. } => expr.to_latex(),
        }
    }
}

impl ParamDef {
    pub fn to_phs(&self) -> String {
        if let Some(ref u) = self.unit {
            format!("{}: {}", self.name, u)
        } else {
            self.name.clone()
        }
    }
}

impl Statement {
    pub fn to_phs(&self) -> String {
        match self {
            Statement::Assign { name, expr } => format!("{} = {}", name, expr.to_phs()),
            Statement::Query { expr } => format!("{}?", expr.to_phs()),
            Statement::AssignAndQuery { name, expr } => format!("{} = {}?", name, expr.to_phs()),
            Statement::Assert { left, right, op } => format!("assert {} {} {}", left.to_phs(), op.to_phs(), right.to_phs()),
            Statement::FnDef { name, params, body } => {
                let param_strs: Vec<String> = params.iter().map(|p| p.to_phs()).collect();
                let body_strs: Vec<String> = body.iter().map(|s| s.to_phs()).collect();
                format!("{}({}) = {}", name, param_strs.join(", "), body_strs.join("; "))
            }
            Statement::DisplayText(txt) => txt.clone(),
            Statement::ExprStmt(expr) => expr.to_phs(),
        }
    }
}
