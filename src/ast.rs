// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Flat syntax tree. Nodes live in typed arenas and refer to each other by
//! `u32` index, so building the tree needs no per-node heap allocation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}

impl Span {
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        let start = self.start as usize;
        &src[start..start + self.len as usize]
    }
}

/// A run of ids stored in `Ast::lists`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct List {
    pub start: u32,
    pub len: u32,
}

pub type ExprId = u32;
pub type StmtId = u32;
pub type TypeId = u32;
pub type PatId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Or => "||",
            BinOp::And => "&&",
            BinOp::Eq => "==",
            BinOp::NotEq => "!=",
            BinOp::Lt => "<",
            BinOp::LtEq => "<=",
            BinOp::Gt => ">",
            BinOp::GtEq => ">=",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Expr {
    Int(Span),
    Float(Span),
    Str(Span),
    Bool(bool),
    Ident(Span),
    Unary { op: UnOp, operand: ExprId },
    Binary { op: BinOp, lhs: ExprId, rhs: ExprId },
    /// `args` is a list of `ExprId`.
    Call { callee: ExprId, args: List },
    /// `mut value` in argument position.
    MutArg(ExprId),
    Field { base: ExprId, name: Span },
    Index { base: ExprId, index: ExprId },
    /// `fields` is a list of ids into `Ast::field_inits`.
    StructLit { name: Span, fields: List },
    /// `items` is a list of `ExprId`.
    ArrayLit { items: List },
}

#[derive(Debug, Clone, Copy)]
pub enum Else {
    None,
    /// List of `StmtId`.
    Block(List),
    /// A `Stmt::If`.
    If(StmtId),
}

#[derive(Debug, Clone, Copy)]
pub enum Stmt {
    Let { mutable: bool, name: Span, ty: Option<TypeId>, value: ExprId },
    Assign { target: ExprId, value: ExprId },
    Expr(ExprId),
    Return(Option<ExprId>),
    If { cond: ExprId, then_block: List, else_branch: Else },
    While { cond: ExprId, body: List },
    For { index: Option<Span>, item: Span, iter: ExprId, body: List },
    /// `arms` is a list of ids into `Ast::arms`.
    Match { scrutinee: ExprId, arms: List },
    Break,
    Continue,
}

#[derive(Debug, Clone, Copy)]
pub struct Arm {
    pub pattern: PatId,
    /// List of `StmtId`.
    pub body: List,
}

#[derive(Debug, Clone, Copy)]
pub enum Pattern {
    Wildcard,
    /// A binding or a variant without fields; name resolution decides.
    Name(Span),
    /// `args` is a list of `PatId`.
    Variant { name: Span, args: List },
    Int { negative: bool, digits: Span },
    Str(Span),
    Bool(bool),
}

#[derive(Debug, Clone, Copy)]
pub struct Type {
    pub name: Span,
    /// List of `TypeId`.
    pub args: List,
}

#[derive(Debug, Clone, Copy)]
pub struct Param {
    pub name: Span,
    pub mutable: bool,
    pub ty: TypeId,
}

#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub name: Span,
    pub ty: TypeId,
}

#[derive(Debug, Clone, Copy)]
pub struct Variant {
    pub name: Span,
    /// List of `TypeId`.
    pub fields: List,
}

#[derive(Debug, Clone, Copy)]
pub struct FieldInit {
    pub name: Span,
    pub value: ExprId,
}

/// The nodes one function is made of: where they start in each arena
/// and how many there are. The parser creates a function's nodes one
/// after another, so each is a single run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Nodes {
    pub exprs: (u32, u32),
    pub stmts: (u32, u32),
    pub patterns: (u32, u32),
    pub field_inits: (u32, u32),
}

#[derive(Debug, Clone, Copy)]
pub enum Item {
    Fn {
        name: Span,
        /// List of ids into `Ast::names`.
        generics: List,
        /// List of ids into `Ast::params`.
        params: List,
        ret: Option<TypeId>,
        /// List of `StmtId`.
        body: List,
        nodes: Nodes,
    },
    Struct {
        name: Span,
        generics: List,
        /// List of ids into `Ast::fields`.
        fields: List,
    },
    Enum {
        name: Span,
        generics: List,
        /// List of ids into `Ast::variants`.
        variants: List,
    },
}

#[derive(Debug, Default)]
pub struct Ast {
    pub items: Vec<Item>,
    pub exprs: Vec<Expr>,
    /// Source offset where each expression starts, parallel to `exprs`.
    pub expr_pos: Vec<u32>,
    pub stmts: Vec<Stmt>,
    /// Source offset where each statement starts, parallel to `stmts`.
    pub stmt_pos: Vec<u32>,
    pub types: Vec<Type>,
    pub patterns: Vec<Pattern>,
    /// Source offset where each pattern starts, parallel to `patterns`.
    pub pat_pos: Vec<u32>,
    pub params: Vec<Param>,
    pub fields: Vec<Field>,
    pub variants: Vec<Variant>,
    pub arms: Vec<Arm>,
    pub field_inits: Vec<FieldInit>,
    pub names: Vec<Span>,
    pub lists: Vec<u32>,
}

impl Ast {
    pub fn list(&self, list: List) -> &[u32] {
        let start = list.start as usize;
        &self.lists[start..start + list.len as usize]
    }

    pub fn node_count(&self) -> usize {
        self.items.len()
            + self.exprs.len()
            + self.stmts.len()
            + self.types.len()
            + self.patterns.len()
            + self.params.len()
            + self.fields.len()
            + self.variants.len()
            + self.arms.len()
            + self.field_inits.len()
    }
}
