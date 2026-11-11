// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Name resolution and type checking for the part of the language that
//! `azol build` can compile so far: `i64` and `bool` values, functions,
//! `let`/`var`, assignment, `if`, `while`, `return` and `print`.

use crate::ast::*;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemaError {
    pub message: String,
    pub offset: u32,
}

type SResult<T> = Result<T, SemaError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    I64,
    Bool,
    Unit,
}

impl Ty {
    fn name(self) -> &'static str {
        match self {
            Ty::I64 => "i64",
            Ty::Bool => "bool",
            Ty::Unit => "no value",
        }
    }
}

/// Index of the built-in `print` in call resolutions.
pub const BUILTIN_PRINT: u32 = u32::MAX;

/// Most parameters a function can take: they travel in registers x0-x7.
pub const MAX_PARAMS: usize = 8;
/// Keeps every local reachable with one load/store instruction.
pub const MAX_SLOTS: u32 = 500;

#[derive(Debug)]
pub struct FnInfo {
    pub ret: Ty,
    pub params: Vec<Ty>,
    /// Stack slots for parameters and locals.
    pub slots: u32,
}

#[derive(Debug)]
pub struct Program {
    /// Parallel to the functions in `Ast::items`.
    pub fns: Vec<FnInfo>,
    pub main: u32,
    /// Per expression: the stack slot for a name, the function index for
    /// a call, the index into `ints` for an integer literal.
    pub expr_res: Vec<u32>,
    /// Per `let`/`var` statement: its stack slot.
    pub stmt_slot: Vec<u32>,
    pub ints: Vec<i64>,
}

struct Local<'a> {
    name: &'a str,
    slot: u32,
    ty: Ty,
    mutable: bool,
}

struct Checker<'a> {
    ast: &'a Ast,
    src: &'a str,
    fns: Vec<FnInfo>,
    fn_names: HashMap<&'a str, u32>,
    expr_res: Vec<u32>,
    stmt_slot: Vec<u32>,
    ints: Vec<i64>,
    locals: Vec<Local<'a>>,
    ret: Ty,
    next_slot: u32,
    loop_depth: u32,
}

pub fn check(ast: &Ast, src: &str) -> Result<Program, SemaError> {
    let mut c = Checker {
        ast,
        src,
        fns: Vec::new(),
        fn_names: HashMap::new(),
        expr_res: vec![0; ast.exprs.len()],
        stmt_slot: vec![0; ast.stmts.len()],
        ints: Vec::new(),
        locals: Vec::new(),
        ret: Ty::Unit,
        next_slot: 0,
        loop_depth: 0,
    };
    c.signatures()?;
    let mut index = 0;
    for item in &ast.items {
        if let Item::Fn { name, params, body, .. } = *item {
            c.function(index, name, params, body)?;
            index += 1;
        }
    }
    let Some(&main) = c.fn_names.get("main") else {
        return Err(SemaError { message: "the program has no `main` function".to_string(), offset: 0 });
    };
    Ok(Program { fns: c.fns, main, expr_res: c.expr_res, stmt_slot: c.stmt_slot, ints: c.ints })
}

fn err<T>(offset: u32, message: impl Into<String>) -> SResult<T> {
    Err(SemaError { message: message.into(), offset })
}

impl<'a> Checker<'a> {
    fn ty(&self, id: TypeId) -> SResult<Ty> {
        let ty = self.ast.types[id as usize];
        let name = ty.name.text(self.src);
        if ty.args.len > 0 {
            return err(ty.name.start, "generic types are not supported by `azol build` yet");
        }
        match name {
            "i64" => Ok(Ty::I64),
            "bool" => Ok(Ty::Bool),
            _ => err(ty.name.start, format!("type `{name}` is not supported by `azol build` yet; use `i64` or `bool`")),
        }
    }

    fn signatures(&mut self) -> SResult<()> {
        for item in &self.ast.items {
            match *item {
                Item::Struct { name, .. } => {
                    return err(name.start, "structs are not supported by `azol build` yet");
                }
                Item::Enum { name, .. } => {
                    return err(name.start, "enums are not supported by `azol build` yet");
                }
                Item::Fn { name, generics, params, ret, .. } => {
                    let text = name.text(self.src);
                    if generics.len > 0 {
                        return err(name.start, "generic functions are not supported by `azol build` yet");
                    }
                    if text == "print" {
                        return err(name.start, "`print` is built in and cannot be redefined");
                    }
                    let param_ids = self.ast.list(params);
                    if param_ids.len() > MAX_PARAMS {
                        return err(name.start, format!("function `{text}` has more than {MAX_PARAMS} parameters, which is not supported yet"));
                    }
                    let mut tys = Vec::with_capacity(param_ids.len());
                    for &id in param_ids {
                        let param = self.ast.params[id as usize];
                        if param.mutable {
                            return err(param.name.start, "`mut` parameters are not supported by `azol build` yet");
                        }
                        tys.push(self.ty(param.ty)?);
                    }
                    let ret = match ret {
                        Some(id) => self.ty(id)?,
                        None => Ty::Unit,
                    };
                    if text == "main" {
                        if !tys.is_empty() {
                            return err(name.start, "`main` cannot take parameters");
                        }
                        if ret == Ty::Bool {
                            return err(name.start, "`main` must return `i64` or nothing");
                        }
                    }
                    let index = self.fns.len() as u32;
                    if self.fn_names.insert(text, index).is_some() {
                        return err(name.start, format!("function `{text}` is defined more than once"));
                    }
                    self.fns.push(FnInfo { ret, params: tys, slots: 0 });
                }
            }
        }
        Ok(())
    }

    fn function(&mut self, index: u32, name: Span, params: List, body: List) -> SResult<()> {
        self.locals.clear();
        self.next_slot = 0;
        self.loop_depth = 0;
        self.ret = self.fns[index as usize].ret;
        for (i, &id) in self.ast.list(params).iter().enumerate() {
            let param = self.ast.params[id as usize];
            let ty = self.fns[index as usize].params[i];
            self.declare(param.name, ty, false)?;
        }
        self.block(body)?;
        if self.ret != Ty::Unit && !self.always_returns(body) {
            return err(
                name.start,
                format!("function `{}` must return a value of type `{}` on every path", name.text(self.src), self.ret.name()),
            );
        }
        self.fns[index as usize].slots = self.next_slot;
        Ok(())
    }

    fn always_returns(&self, body: List) -> bool {
        let Some(&last) = self.ast.list(body).last() else {
            return false;
        };
        self.stmt_returns(last)
    }

    fn stmt_returns(&self, id: StmtId) -> bool {
        match self.ast.stmts[id as usize] {
            Stmt::Return(_) => true,
            Stmt::If { then_block, else_branch, .. } => {
                self.always_returns(then_block)
                    && match else_branch {
                        Else::None => false,
                        Else::Block(body) => self.always_returns(body),
                        Else::If(next) => self.stmt_returns(next),
                    }
            }
            _ => false,
        }
    }

    fn declare(&mut self, name: Span, ty: Ty, mutable: bool) -> SResult<u32> {
        if self.next_slot >= MAX_SLOTS {
            return err(name.start, format!("a function cannot have more than {MAX_SLOTS} variables yet"));
        }
        let slot = self.next_slot;
        self.next_slot += 1;
        self.locals.push(Local { name: name.text(self.src), slot, ty, mutable });
        Ok(slot)
    }

    fn lookup(&self, name: &str) -> Option<&Local<'a>> {
        self.locals.iter().rev().find(|local| local.name == name)
    }

    fn block(&mut self, body: List) -> SResult<()> {
        let scope = self.locals.len();
        for &id in self.ast.list(body) {
            self.stmt(id)?;
        }
        self.locals.truncate(scope);
        Ok(())
    }

    fn stmt(&mut self, id: StmtId) -> SResult<()> {
        let pos = self.ast.stmt_pos[id as usize];
        match self.ast.stmts[id as usize] {
            Stmt::Let { mutable, name, ty, value } => {
                let value_ty = self.expr(value)?;
                if value_ty == Ty::Unit {
                    return err(self.ast.expr_pos[value as usize], "this expression has no value to store");
                }
                if let Some(ty) = ty {
                    let declared = self.ty(ty)?;
                    if declared != value_ty {
                        return err(
                            self.ast.expr_pos[value as usize],
                            format!("expected `{}` but this is `{}`", declared.name(), value_ty.name()),
                        );
                    }
                }
                let slot = self.declare(name, value_ty, mutable)?;
                self.stmt_slot[id as usize] = slot;
            }
            Stmt::Assign { target, value } => {
                let Expr::Ident(name) = self.ast.exprs[target as usize] else {
                    return err(pos, "only plain variables can be assigned by `azol build` yet");
                };
                let text = name.text(self.src);
                let Some(local) = self.lookup(text) else {
                    return err(name.start, format!("unknown variable `{text}`"));
                };
                let (slot, ty, mutable) = (local.slot, local.ty, local.mutable);
                if !mutable {
                    return err(name.start, format!("cannot assign to `{text}`: declare it with `var` to make it changeable"));
                }
                let value_ty = self.expr(value)?;
                if value_ty != ty {
                    return err(
                        self.ast.expr_pos[value as usize],
                        format!("`{text}` is `{}` but this is `{}`", ty.name(), value_ty.name()),
                    );
                }
                self.expr_res[target as usize] = slot;
            }
            Stmt::Expr(expr) => {
                self.expr(expr)?;
            }
            Stmt::Return(value) => {
                let value_ty = match value {
                    Some(value) => self.expr(value)?,
                    None => Ty::Unit,
                };
                if value_ty != self.ret {
                    let at = value.map_or(pos, |v| self.ast.expr_pos[v as usize]);
                    return err(
                        at,
                        match (self.ret, value_ty) {
                            (Ty::Unit, _) => "this function returns nothing, so `return` cannot have a value".to_string(),
                            (ret, Ty::Unit) => format!("this function must return a value of type `{}`", ret.name()),
                            (ret, got) => format!("this function returns `{}` but this is `{}`", ret.name(), got.name()),
                        },
                    );
                }
            }
            Stmt::If { cond, then_block, else_branch } => {
                self.condition(cond)?;
                self.block(then_block)?;
                match else_branch {
                    Else::None => {}
                    Else::Block(body) => self.block(body)?,
                    Else::If(next) => self.stmt(next)?,
                }
            }
            Stmt::While { cond, body } => {
                self.condition(cond)?;
                self.loop_depth += 1;
                self.block(body)?;
                self.loop_depth -= 1;
            }
            Stmt::Break | Stmt::Continue => {
                if self.loop_depth == 0 {
                    return err(pos, "`break` and `continue` can only be used inside a loop");
                }
            }
            Stmt::For { .. } => return err(pos, "`for` loops are not supported by `azol build` yet; use `while`"),
            Stmt::Match { .. } => return err(pos, "`match` is not supported by `azol build` yet"),
        }
        Ok(())
    }

    fn condition(&mut self, cond: ExprId) -> SResult<()> {
        let ty = self.expr(cond)?;
        if ty != Ty::Bool {
            return err(self.ast.expr_pos[cond as usize], format!("a condition must be `bool` but this is `{}`", ty.name()));
        }
        Ok(())
    }

    fn expect(&mut self, expr: ExprId, want: Ty, what: &str) -> SResult<()> {
        let ty = self.expr(expr)?;
        if ty != want {
            return err(
                self.ast.expr_pos[expr as usize],
                format!("{what} needs `{}` but this is `{}`", want.name(), ty.name()),
            );
        }
        Ok(())
    }

    fn expr(&mut self, id: ExprId) -> SResult<Ty> {
        let pos = self.ast.expr_pos[id as usize];
        match self.ast.exprs[id as usize] {
            Expr::Int(span) => {
                let digits: String = span.text(self.src).chars().filter(|&c| c != '_').collect();
                let Ok(value) = digits.parse::<i64>() else {
                    return err(pos, "this number does not fit in `i64`");
                };
                self.expr_res[id as usize] = self.ints.len() as u32;
                self.ints.push(value);
                Ok(Ty::I64)
            }
            Expr::Bool(_) => Ok(Ty::Bool),
            Expr::Ident(name) => {
                let text = name.text(self.src);
                let Some(local) = self.lookup(text) else {
                    return if self.fn_names.contains_key(text) || text == "print" {
                        err(pos, format!("`{text}` is a function; call it with `{text}(...)`"))
                    } else {
                        err(pos, format!("unknown variable `{text}`"))
                    };
                };
                let (slot, ty) = (local.slot, local.ty);
                self.expr_res[id as usize] = slot;
                Ok(ty)
            }
            Expr::Unary { op, operand } => match op {
                UnOp::Neg => {
                    self.expect(operand, Ty::I64, "`-`")?;
                    Ok(Ty::I64)
                }
                UnOp::Not => {
                    self.expect(operand, Ty::Bool, "`!`")?;
                    Ok(Ty::Bool)
                }
            },
            Expr::Binary { op, lhs, rhs } => {
                let what = format!("`{}`", op.symbol());
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                        self.expect(lhs, Ty::I64, &what)?;
                        self.expect(rhs, Ty::I64, &what)?;
                        Ok(Ty::I64)
                    }
                    BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => {
                        self.expect(lhs, Ty::I64, &what)?;
                        self.expect(rhs, Ty::I64, &what)?;
                        Ok(Ty::Bool)
                    }
                    BinOp::And | BinOp::Or => {
                        self.expect(lhs, Ty::Bool, &what)?;
                        self.expect(rhs, Ty::Bool, &what)?;
                        Ok(Ty::Bool)
                    }
                    BinOp::Eq | BinOp::NotEq => {
                        let left = self.expr(lhs)?;
                        if left == Ty::Unit {
                            return err(self.ast.expr_pos[lhs as usize], "this expression has no value to compare");
                        }
                        self.expect(rhs, left, &what)?;
                        Ok(Ty::Bool)
                    }
                }
            }
            Expr::Call { callee, args } => {
                let Expr::Ident(name) = self.ast.exprs[callee as usize] else {
                    return err(pos, "only functions can be called");
                };
                let text = name.text(self.src);
                let arg_ids = self.ast.list(args);
                let (index, ret) = if self.lookup(text).is_some() {
                    return err(pos, format!("`{text}` is a variable, not a function"));
                } else if text == "print" {
                    if arg_ids.len() != 1 {
                        return err(pos, format!("`print` takes 1 argument but {} were given", arg_ids.len()));
                    }
                    self.expect(arg_ids[0], Ty::I64, "`print`")?;
                    (BUILTIN_PRINT, Ty::Unit)
                } else if let Some(&index) = self.fn_names.get(text) {
                    let expected = self.fns[index as usize].params.len();
                    if arg_ids.len() != expected {
                        return err(
                            pos,
                            format!("`{text}` takes {expected} argument{} but {} were given", if expected == 1 { "" } else { "s" }, arg_ids.len()),
                        );
                    }
                    for (i, &arg) in arg_ids.iter().enumerate() {
                        if let Expr::MutArg(_) = self.ast.exprs[arg as usize] {
                            return err(self.ast.expr_pos[arg as usize], "`mut` arguments are not supported by `azol build` yet");
                        }
                        let want = self.fns[index as usize].params[i];
                        self.expect(arg, want, &format!("argument {} of `{text}`", i + 1))?;
                    }
                    (index, self.fns[index as usize].ret)
                } else {
                    return err(pos, format!("unknown function `{text}`"));
                };
                self.expr_res[id as usize] = index;
                Ok(ret)
            }
            Expr::MutArg(_) => err(pos, "`mut` can only be used on a function argument"),
            Expr::Float(_) => err(pos, "floating-point numbers are not supported by `azol build` yet"),
            Expr::Str(_) => err(pos, "strings are not supported by `azol build` yet"),
            Expr::Field { .. } | Expr::StructLit { .. } => err(pos, "structs are not supported by `azol build` yet"),
            Expr::Index { .. } | Expr::ArrayLit { .. } => err(pos, "arrays are not supported by `azol build` yet"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{lex, line_col};
    use crate::parser::parse;

    fn check_src(src: &str) -> Result<Program, (String, usize, usize)> {
        let tokens = lex(src).expect("lexing failed");
        let ast = parse(src, &tokens).expect("parsing failed");
        check(&ast, src).map_err(|e| {
            let (line, col) = line_col(src, e.offset);
            (e.message, line, col)
        })
    }

    fn error(src: &str) -> String {
        check_src(src).expect_err("checking should fail").0
    }

    fn in_main(body: &str) -> String {
        format!("fn main() {{\n{body}\n}}")
    }

    #[test]
    fn accepts_a_valid_program_and_counts_slots() {
        let program = check_src("fn add(a: i64, b: i64) -> i64 {\n let c = a + b\n return c\n}\nfn main() -> i64 {\n return add(1, 2)\n}").unwrap();
        assert_eq!(program.fns.len(), 2);
        assert_eq!(program.fns[0].slots, 3);
        assert_eq!(program.main, 1);
        assert_eq!(program.ints, vec![1, 2]);
    }

    #[test]
    fn reports_position_of_the_problem() {
        let result = check_src("fn main() {\n    let a = 1\n    let b = a + true\n}");
        assert_eq!(result.unwrap_err(), ("`+` needs `i64` but this is `bool`".to_string(), 3, 17));
    }

    #[test]
    fn type_errors() {
        assert_eq!(error(&in_main("if 1 {\n}")), "a condition must be `bool` but this is `i64`");
        assert_eq!(error(&in_main("let a: bool = 1")), "expected `bool` but this is `i64`");
        assert_eq!(error(&in_main("let a = !1")), "`!` needs `bool` but this is `i64`");
        assert_eq!(error(&in_main("let a = 1 == true")), "`==` needs `i64` but this is `bool`");
        assert_eq!(error(&in_main("let a = 1 < 2 < 3")), "`<` needs `i64` but this is `bool`");
        assert_eq!(error(&in_main("print(true)")), "`print` needs `i64` but this is `bool`");
        assert_eq!(error(&in_main("let a = print(1)")), "this expression has no value to store");
    }

    #[test]
    fn variable_errors() {
        assert_eq!(error(&in_main("print(x)")), "unknown variable `x`");
        assert_eq!(error(&in_main("let a = 1\na = 2")), "cannot assign to `a`: declare it with `var` to make it changeable");
        assert_eq!(error(&in_main("var a = 1\na = false")), "`a` is `i64` but this is `bool`");
        assert_eq!(error(&in_main("if true {\nlet inner = 1\n}\nprint(inner)")), "unknown variable `inner`");
        assert_eq!(error(&in_main("let a = main")), "`main` is a function; call it with `main(...)`");
    }

    #[test]
    fn function_errors() {
        assert_eq!(error(&in_main("missing(1)")), "unknown function `missing`");
        assert_eq!(error("fn f(a: i64) {\n}\nfn main() {\n f()\n}"), "`f` takes 1 argument but 0 were given");
        assert_eq!(error("fn f(a: i64) {\n}\nfn main() {\n f(true)\n}"), "argument 1 of `f` needs `i64` but this is `bool`");
        assert_eq!(error("fn f() {\n}\nfn f() {\n}\nfn main() {\n}"), "function `f` is defined more than once");
        assert_eq!(error("fn f() {\n}"), "the program has no `main` function");
        assert_eq!(error("fn main(a: i64) {\n}"), "`main` cannot take parameters");
        assert_eq!(error("fn print(a: i64) {\n}\nfn main() {\n}"), "`print` is built in and cannot be redefined");
    }

    #[test]
    fn return_errors() {
        assert_eq!(error("fn main() -> i64 {\n}"), "function `main` must return a value of type `i64` on every path");
        assert_eq!(error("fn main() -> i64 {\n if true {\n return 1\n }\n}"), "function `main` must return a value of type `i64` on every path");
        assert_eq!(error("fn main() -> i64 {\n return true\n}"), "this function returns `i64` but this is `bool`");
        assert_eq!(error("fn main() -> i64 {\n return\n}"), "this function must return a value of type `i64`");
        assert_eq!(error(&in_main("return 1")), "this function returns nothing, so `return` cannot have a value");
        assert!(check_src("fn f(a: i64) -> i64 {\n if a > 0 {\n return 1\n } else if a < 0 {\n return 2\n } else {\n return 3\n }\n}\nfn main() {\n}").is_ok());
    }

    #[test]
    fn loop_and_literal_errors() {
        assert_eq!(error(&in_main("break")), "`break` and `continue` can only be used inside a loop");
        assert_eq!(error(&in_main("let a = 9223372036854775808")), "this number does not fit in `i64`");
        assert!(check_src(&in_main("let a = 9_223_372_036_854_775_807")).is_ok());
    }

    #[test]
    fn features_not_built_yet_are_reported_clearly() {
        assert_eq!(error("struct P {\n x: i64\n}\nfn main() {\n}"), "structs are not supported by `azol build` yet");
        assert_eq!(error(&in_main("let a = \"text\"")), "strings are not supported by `azol build` yet");
        assert_eq!(error(&in_main("let a = 1.5")), "floating-point numbers are not supported by `azol build` yet");
        assert_eq!(error(&in_main("for x in y {\n}")), "`for` loops are not supported by `azol build` yet; use `while`");
        assert_eq!(error("fn f(a: i32) {\n}\nfn main() {\n}"), "type `i32` is not supported by `azol build` yet; use `i64` or `bool`");
    }
}
