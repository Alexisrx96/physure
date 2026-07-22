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
