// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Recursive-descent parser with precedence climbing for expressions.
//! It never backtracks: every decision is made from the current token,
//! plus at most a look past blank lines.

use crate::ast::*;
use crate::token::{Token, TokenKind};
use TokenKind as K;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub offset: u32,
}

type PResult<T> = Result<T, ParseError>;

const MAX_DEPTH: u32 = 200;

pub fn parse(src: &str, tokens: &[Token]) -> Result<Ast, ParseError> {
    parse_into(Ast::default(), src, tokens)
}

/// Parses `tokens` and adds what they declare to `ast`. This is how the
/// standard library's own source joins a program: its tokens refer to
/// the same `src`, further along.
pub fn parse_into(mut ast: Ast, src: &str, tokens: &[Token]) -> Result<Ast, ParseError> {
    debug_assert!(matches!(tokens.last(), Some(t) if t.kind == K::Eof));
    ast.exprs.reserve(tokens.len() / 2);
    ast.expr_pos.reserve(tokens.len() / 2);
    ast.stmts.reserve(tokens.len() / 6);
    ast.stmt_pos.reserve(tokens.len() / 6);
    ast.lists.reserve(tokens.len() / 4);
    let mut parser = Parser {
        src,
        tokens,
        pos: 0,
        ast,
        scratch: Vec::with_capacity(256),
        no_struct: false,
        depth: 0,
    };
    parser.file()?;
    Ok(parser.ast)
}

struct Parser<'a> {
    src: &'a str,
    tokens: &'a [Token],
    pos: usize,
    ast: Ast,
    scratch: Vec<u32>,
    /// True while parsing a condition, where `name {` starts a block and
    /// not a struct literal.
    no_struct: bool,
    depth: u32,
}

impl<'a> Parser<'a> {
    // ---- token helpers -------------------------------------------------

    #[inline(always)]
    fn kind(&self) -> TokenKind {
        self.tokens[self.pos].kind
    }

    #[inline(always)]
    fn at(&self, kind: TokenKind) -> bool {
        self.kind() == kind
    }

    #[inline(always)]
    fn bump(&mut self) -> Token {
        let token = self.tokens[self.pos];
        if token.kind != K::Eof {
            self.pos += 1;
        }
        token
    }

    #[inline(always)]
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> PResult<Token> {
        if self.at(kind) {
            Ok(self.bump())
        } else {
            Err(self.error(format!(
                "expected {} but found {}",
                describe(kind),
                describe(self.kind())
            )))
        }
    }

    fn ident(&mut self, what: &str) -> PResult<Span> {
        if self.at(K::Ident) {
            Ok(span(self.bump()))
        } else {
            Err(self.error(format!("expected {what} but found {}", describe(self.kind()))))
        }
    }

    #[inline(always)]
    fn skip_newlines(&mut self) {
        while self.at(K::Newline) {
            self.pos += 1;
        }
    }

    fn kind_after_newlines(&self) -> TokenKind {
        let mut i = self.pos;
        while self.tokens[i].kind == K::Newline {
            i += 1;
        }
        self.tokens[i].kind
    }

    fn error(&self, message: String) -> ParseError {
        ParseError { message, offset: self.tokens[self.pos].start }
    }

    fn enter(&mut self) -> PResult<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("code is nested too deeply".to_string()));
        }
        Ok(())
    }

    fn finish_list(&mut self, start: usize) -> List {
        let list = List {
            start: self.ast.lists.len() as u32,
            len: (self.scratch.len() - start) as u32,
        };
        self.ast.lists.extend_from_slice(&self.scratch[start..]);
        self.scratch.truncate(start);
        list
    }

    fn push_expr(&mut self, expr: Expr, pos: u32) -> ExprId {
        self.ast.exprs.push(expr);
        self.ast.expr_pos.push(pos);
        (self.ast.exprs.len() - 1) as ExprId
    }

    fn push_stmt(&mut self, stmt: Stmt, pos: u32) -> StmtId {
        self.ast.stmts.push(stmt);
        self.ast.stmt_pos.push(pos);
        (self.ast.stmts.len() - 1) as StmtId
    }

    #[inline(always)]
    fn here(&self) -> u32 {
        self.tokens[self.pos].start
    }

    // ---- items ---------------------------------------------------------

    fn file(&mut self) -> PResult<()> {
        loop {
            self.skip_newlines();
            let item = match self.kind() {
                K::Eof => return Ok(()),
                K::Fn => self.function()?,
                K::Struct => self.structure()?,
                K::Enum => self.enumeration()?,
                other => {
                    return Err(self.error(format!(
                        "expected `fn`, `struct` or `enum` but found {}",
                        describe(other)
                    )));
                }
            };
            self.ast.items.push(item);
            if !self.at(K::Eof) {
                self.line_end()?;
            }
        }
    }

    fn line_end(&mut self) -> PResult<()> {
        if self.eat(K::Newline) {
            Ok(())
        } else {
            Err(self.error(format!("expected end of line but found {}", describe(self.kind()))))
        }
    }

    fn generics(&mut self) -> PResult<List> {
        let start = self.scratch.len();
        if self.eat(K::LBracket) {
            loop {
                let name = self.ident("a type parameter name")?;
                self.ast.names.push(name);
                self.scratch.push((self.ast.names.len() - 1) as u32);
                if !self.eat(K::Comma) {
                    break;
                }
            }
            self.expect(K::RBracket)?;
        }
        Ok(self.finish_list(start))
    }

    fn function(&mut self) -> PResult<Item> {
        self.bump();
        let name = self.ident("a function name")?;
        let generics = self.generics()?;
        self.expect(K::LParen)?;
        let start = self.scratch.len();
        self.skip_newlines();
        while !self.at(K::RParen) {
            let name = self.ident("a parameter name")?;
            self.expect(K::Colon)?;
            let mutable = self.eat(K::Mut);
            let ty = self.ty()?;
            self.ast.params.push(Param { name, mutable, ty });
            self.scratch.push((self.ast.params.len() - 1) as u32);
            self.skip_newlines();
            if !self.eat(K::Comma) {
                break;
            }
            self.skip_newlines();
        }
        self.expect(K::RParen)?;
        let params = self.finish_list(start);
        let ret = if self.eat(K::Arrow) { Some(self.ty()?) } else { None };
        let body = self.block()?;
        Ok(Item::Fn { name, generics, params, ret, body })
    }

    fn structure(&mut self) -> PResult<Item> {
        self.bump();
        let name = self.ident("a struct name")?;
        let generics = self.generics()?;
        self.expect(K::LBrace)?;
        let start = self.scratch.len();
        loop {
            self.skip_newlines();
            if self.at(K::RBrace) {
                break;
            }
            let name = self.ident("a field name")?;
            self.expect(K::Colon)?;
            let ty = self.ty()?;
            self.ast.fields.push(Field { name, ty });
            self.scratch.push((self.ast.fields.len() - 1) as u32);
            self.member_separator()?;
        }
        self.bump();
        let fields = self.finish_list(start);
        Ok(Item::Struct { name, generics, fields })
    }

    fn enumeration(&mut self) -> PResult<Item> {
        self.bump();
        let name = self.ident("an enum name")?;
        let generics = self.generics()?;
        self.expect(K::LBrace)?;
        let start = self.scratch.len();
        loop {
            self.skip_newlines();
            if self.at(K::RBrace) {
                break;
            }
            let name = self.ident("a variant name")?;
            let field_start = self.scratch.len();
            if self.eat(K::LParen) {
                loop {
                    let ty = self.ty()?;
                    self.scratch.push(ty);
                    if !self.eat(K::Comma) {
                        break;
                    }
                }
                self.expect(K::RParen)?;
            }
            let fields = self.finish_list(field_start);
            self.ast.variants.push(Variant { name, fields });
            self.scratch.push((self.ast.variants.len() - 1) as u32);
            self.member_separator()?;
        }
        self.bump();
        let variants = self.finish_list(start);
        Ok(Item::Enum { name, generics, variants })
    }

    /// Members of a struct or enum are separated by a newline or a comma.
    fn member_separator(&mut self) -> PResult<()> {
        if self.eat(K::Comma) || self.at(K::Newline) || self.at(K::RBrace) {
            Ok(())
        } else {
            Err(self.error(format!(
                "expected a new line, `,` or `}}` but found {}",
                describe(self.kind())
            )))
        }
    }

    fn ty(&mut self) -> PResult<TypeId> {
        self.enter()?;
        let name = self.ident("a type")?;
        let start = self.scratch.len();
        if self.eat(K::LBracket) {
            loop {
                let arg = self.ty()?;
                self.scratch.push(arg);
                if !self.eat(K::Comma) {
                    break;
                }
            }
            self.expect(K::RBracket)?;
        }
        let args = self.finish_list(start);
        self.depth -= 1;
        self.ast.types.push(Type { name, args });
        Ok((self.ast.types.len() - 1) as TypeId)
    }

    // ---- statements ----------------------------------------------------

    fn block(&mut self) -> PResult<List> {
        self.enter()?;
        self.expect(K::LBrace)?;
        let saved = self.no_struct;
        self.no_struct = false;
        let start = self.scratch.len();
        loop {
            self.skip_newlines();
            if self.eat(K::RBrace) {
                break;
            }
            if self.at(K::Eof) {
                return Err(self.error("expected `}` to close the block but reached end of file".to_string()));
            }
            let stmt = self.statement()?;
            self.scratch.push(stmt);
            if !self.at(K::RBrace) {
                self.line_end()?;
            }
        }
        self.no_struct = saved;
        self.depth -= 1;
        Ok(self.finish_list(start))
    }

    fn statement(&mut self) -> PResult<StmtId> {
        let pos = self.here();
        let stmt = match self.kind() {
            K::Let | K::Var => {
                let mutable = self.bump().kind == K::Var;
                let name = self.ident("a variable name")?;
                let ty = if self.eat(K::Colon) { Some(self.ty()?) } else { None };
                self.expect(K::Assign)?;
                self.skip_newlines();
                let value = self.expr()?;
                Stmt::Let { mutable, name, ty, value }
            }
            K::Return => {
                self.bump();
                if matches!(self.kind(), K::Newline | K::RBrace | K::Eof) {
                    Stmt::Return(None)
                } else {
                    Stmt::Return(Some(self.expr()?))
                }
            }
            K::If => return self.if_statement(),
            K::While => {
                self.bump();
                let cond = self.condition()?;
                let body = self.block()?;
                Stmt::While { cond, body }
            }
            K::For => {
                self.bump();
                let first = self.ident("a loop variable name")?;
                let (index, item) = if self.eat(K::Comma) {
                    (Some(first), self.ident("a loop variable name")?)
                } else {
                    (None, first)
                };
                self.expect(K::In)?;
                let iter = self.condition()?;
                let body = self.block()?;
                Stmt::For { index, item, iter, body }
            }
            K::Match => self.match_statement()?,
            K::Break => {
                self.bump();
                Stmt::Break
            }
            K::Continue => {
                self.bump();
                Stmt::Continue
            }
            _ => {
                let target_token = self.pos;
                let expr = self.expr()?;
                if self.eat(K::Assign) {
                    if !matches!(
                        self.ast.exprs[expr as usize],
                        Expr::Ident(_) | Expr::Field { .. } | Expr::Index { .. }
                    ) {
                        return Err(ParseError {
                            message: "cannot assign to this expression".to_string(),
                            offset: self.tokens[target_token].start,
                        });
                    }
                    self.skip_newlines();
                    let value = self.expr()?;
                    Stmt::Assign { target: expr, value }
                } else {
                    Stmt::Expr(expr)
                }
            }
        };
        Ok(self.push_stmt(stmt, pos))
    }

    fn if_statement(&mut self) -> PResult<StmtId> {
        self.enter()?;
        let pos = self.here();
        self.bump();
        let cond = self.condition()?;
        let then_block = self.block()?;
        let else_branch = if self.kind_after_newlines() == K::Else {
            self.skip_newlines();
            self.bump();
            if self.at(K::If) {
                Else::If(self.if_statement()?)
            } else {
                Else::Block(self.block()?)
            }
        } else {
            Else::None
        };
        self.depth -= 1;
        Ok(self.push_stmt(Stmt::If { cond, then_block, else_branch }, pos))
    }

    fn match_statement(&mut self) -> PResult<Stmt> {
        self.bump();
        let scrutinee = self.condition()?;
        self.expect(K::LBrace)?;
        let start = self.scratch.len();
        loop {
            self.skip_newlines();
            if self.at(K::RBrace) {
                break;
            }
            let pattern = self.pattern()?;
            self.expect(K::FatArrow)?;
            let body = if self.at(K::LBrace) {
                self.block()?
            } else {
                let body_start = self.scratch.len();
                let saved = self.no_struct;
                self.no_struct = false;
                let stmt = self.statement()?;
                self.no_struct = saved;
                self.scratch.push(stmt);
                self.finish_list(body_start)
            };
            self.ast.arms.push(Arm { pattern, body });
            self.scratch.push((self.ast.arms.len() - 1) as u32);
            self.member_separator()?;
        }
        self.bump();
        let arms = self.finish_list(start);
        Ok(Stmt::Match { scrutinee, arms })
    }

    fn pattern(&mut self) -> PResult<PatId> {
        self.enter()?;
        let pos = self.here();
        let pattern = match self.kind() {
            K::Ident => {
                let name = span(self.bump());
                if self.eat(K::LParen) {
                    let start = self.scratch.len();
                    loop {
                        let arg = self.pattern()?;
                        self.scratch.push(arg);
                        if !self.eat(K::Comma) {
                            break;
                        }
                    }
                    self.expect(K::RParen)?;
                    Pattern::Variant { name, args: self.finish_list(start) }
                } else if name.text(self.src) == "_" {
                    Pattern::Wildcard
                } else {
                    Pattern::Name(name)
                }
            }
            K::Int => Pattern::Int { negative: false, digits: span(self.bump()) },
            K::Minus => {
                self.bump();
                let digits = self.expect(K::Int)?;
                Pattern::Int { negative: true, digits: span(digits) }
            }
            K::Str => Pattern::Str(span(self.bump())),
            K::True => {
                self.bump();
                Pattern::Bool(true)
            }
            K::False => {
                self.bump();
                Pattern::Bool(false)
            }
            other => {
                return Err(self.error(format!("expected a pattern but found {}", describe(other))));
            }
        };
        self.depth -= 1;
        self.ast.patterns.push(pattern);
        self.ast.pat_pos.push(pos);
        Ok((self.ast.patterns.len() - 1) as PatId)
    }

    // ---- expressions ---------------------------------------------------

    fn expr(&mut self) -> PResult<ExprId> {
        self.binary(1)
    }

    /// An expression directly followed by `{`, as in `if`, `while`, `for`
    /// and `match`.
    fn condition(&mut self) -> PResult<ExprId> {
        let saved = self.no_struct;
        self.no_struct = true;
        let result = self.expr();
        self.no_struct = saved;
        result
    }

    fn binary(&mut self, min_prec: u8) -> PResult<ExprId> {
        let mut lhs = self.unary()?;
        loop {
            let (op, prec) = match self.kind() {
                K::OrOr => (BinOp::Or, 1),
                K::AndAnd => (BinOp::And, 2),
                K::Eq => (BinOp::Eq, 3),
                K::NotEq => (BinOp::NotEq, 3),
                K::Lt => (BinOp::Lt, 3),
                K::LtEq => (BinOp::LtEq, 3),
                K::Gt => (BinOp::Gt, 3),
                K::GtEq => (BinOp::GtEq, 3),
                K::Plus => (BinOp::Add, 4),
                K::Minus => (BinOp::Sub, 4),
                K::Star => (BinOp::Mul, 5),
                K::Slash => (BinOp::Div, 5),
                K::Percent => (BinOp::Rem, 5),
                _ => break,
            };
            if prec < min_prec {
                break;
            }
            self.bump();
            self.skip_newlines();
            let rhs = self.binary(prec + 1)?;
            let pos = self.ast.expr_pos[lhs as usize];
            lhs = self.push_expr(Expr::Binary { op, lhs, rhs }, pos);
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<ExprId> {
        let op = match self.kind() {
            K::Minus => UnOp::Neg,
            K::Bang => UnOp::Not,
            _ => return self.postfix(),
        };
        self.enter()?;
        let pos = self.here();
        self.bump();
        let operand = self.unary()?;
        self.depth -= 1;
        Ok(self.push_expr(Expr::Unary { op, operand }, pos))
    }

    fn postfix(&mut self) -> PResult<ExprId> {
        let mut expr = self.primary()?;
        let pos = self.ast.expr_pos[expr as usize];
        loop {
            match self.kind() {
                K::LParen => {
                    self.bump();
                    let saved = self.no_struct;
                    self.no_struct = false;
                    let start = self.scratch.len();
                    self.skip_newlines();
                    while !self.at(K::RParen) {
                        let arg_pos = self.here();
                        let arg = if self.eat(K::Mut) {
                            let value = self.expr()?;
                            self.push_expr(Expr::MutArg(value), arg_pos)
                        } else {
                            self.expr()?
                        };
                        self.scratch.push(arg);
                        self.skip_newlines();
                        if !self.eat(K::Comma) {
                            break;
                        }
                        self.skip_newlines();
                    }
                    self.expect(K::RParen)?;
                    self.no_struct = saved;
                    let args = self.finish_list(start);
                    expr = self.push_expr(Expr::Call { callee: expr, args }, pos);
                }
                K::LBracket => {
                    self.bump();
                    let saved = self.no_struct;
                    self.no_struct = false;
                    self.skip_newlines();
                    let index = self.expr()?;
                    self.skip_newlines();
                    self.expect(K::RBracket)?;
                    self.no_struct = saved;
                    expr = self.push_expr(Expr::Index { base: expr, index }, pos);
                }
                K::Dot => {
                    self.bump();
                    let name = self.ident("a field name")?;
                    expr = self.push_expr(Expr::Field { base: expr, name }, pos);
                }
                // A line that starts with `.` continues the previous one.
                K::Newline if self.kind_after_newlines() == K::Dot => self.skip_newlines(),
                _ => return Ok(expr),
            }
        }
    }

    fn primary(&mut self) -> PResult<ExprId> {
        let pos = self.here();
        let expr = match self.kind() {
            K::Int => Expr::Int(span(self.bump())),
            K::Float => Expr::Float(span(self.bump())),
            K::Str => Expr::Str(span(self.bump())),
            K::True => {
                self.bump();
                Expr::Bool(true)
            }
            K::False => {
                self.bump();
                Expr::Bool(false)
            }
            K::Ident => {
                let name = span(self.bump());
                if self.at(K::LBrace) && !self.no_struct {
                    return self.struct_literal(name);
                }
                Expr::Ident(name)
            }
            K::LParen => {
                self.enter()?;
                self.bump();
                let saved = self.no_struct;
                self.no_struct = false;
                self.skip_newlines();
                let inner = self.expr()?;
                self.skip_newlines();
                self.expect(K::RParen)?;
                self.no_struct = saved;
                self.depth -= 1;
                return Ok(inner);
            }
            K::LBracket => {
                self.enter()?;
                self.bump();
                let saved = self.no_struct;
                self.no_struct = false;
                let start = self.scratch.len();
                self.skip_newlines();
                while !self.at(K::RBracket) {
                    let item = self.expr()?;
                    self.scratch.push(item);
                    self.skip_newlines();
                    if !self.eat(K::Comma) {
                        break;
                    }
                    self.skip_newlines();
                }
                self.expect(K::RBracket)?;
                self.no_struct = saved;
                self.depth -= 1;
                Expr::ArrayLit { items: self.finish_list(start) }
            }
            other => {
                return Err(self.error(format!("expected an expression but found {}", describe(other))));
            }
        };
        Ok(self.push_expr(expr, pos))
    }

    fn struct_literal(&mut self, name: Span) -> PResult<ExprId> {
        self.enter()?;
        self.bump();
        let start = self.scratch.len();
        self.skip_newlines();
        while !self.at(K::RBrace) {
            let field = self.ident("a field name")?;
            self.expect(K::Colon)?;
            let value = self.expr()?;
            self.ast.field_inits.push(FieldInit { name: field, value });
            self.scratch.push((self.ast.field_inits.len() - 1) as u32);
            self.skip_newlines();
            if !self.eat(K::Comma) {
                break;
            }
            self.skip_newlines();
        }
        self.expect(K::RBrace)?;
        self.depth -= 1;
        let fields = self.finish_list(start);
        Ok(self.push_expr(Expr::StructLit { name, fields }, name.start))
    }
}

#[inline(always)]
fn span(token: Token) -> Span {
    Span { start: token.start, len: token.len }
}

pub fn describe(kind: TokenKind) -> &'static str {
    match kind {
        K::Ident => "a name",
        K::Int | K::Float => "a number",
        K::Str => "a string",
        K::Fn => "`fn`",
        K::Struct => "`struct`",
        K::Enum => "`enum`",
        K::Let => "`let`",
        K::Var => "`var`",
        K::Mut => "`mut`",
        K::If => "`if`",
        K::Else => "`else`",
        K::For => "`for`",
        K::In => "`in`",
        K::While => "`while`",
        K::Return => "`return`",
        K::Match => "`match`",
        K::Break => "`break`",
        K::Continue => "`continue`",
        K::True => "`true`",
        K::False => "`false`",
        K::LParen => "`(`",
        K::RParen => "`)`",
        K::LBrace => "`{`",
        K::RBrace => "`}`",
        K::LBracket => "`[`",
        K::RBracket => "`]`",
        K::Comma => "`,`",
        K::Colon => "`:`",
        K::Dot => "`.`",
        K::Arrow => "`->`",
        K::FatArrow => "`=>`",
        K::Assign => "`=`",
        K::Eq => "`==`",
        K::NotEq => "`!=`",
        K::Lt => "`<`",
        K::LtEq => "`<=`",
        K::Gt => "`>`",
        K::GtEq => "`>=`",
        K::Plus => "`+`",
        K::Minus => "`-`",
        K::Star => "`*`",
        K::Slash => "`/`",
        K::Percent => "`%`",
        K::Bang => "`!`",
        K::AndAnd => "`&&`",
        K::OrOr => "`||`",
        K::Newline => "end of line",
        K::Eof => "end of file",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dump::dump;
    use crate::lexer::{lex, line_col};

    fn tree(src: &str) -> String {
        let tokens = lex(src).expect("lexing failed");
        let ast = parse(src, &tokens).expect("parsing failed");
        dump(&ast, src)
    }

    /// Parses statements inside a function and returns them without the header.
    fn body(stmts: &str) -> String {
        let src = format!("fn t() {{\n{stmts}\n}}");
        let out = tree(&src);
        out.lines().skip(1).map(|l| &l[2..]).collect::<Vec<_>>().join("\n")
    }

    fn error(src: &str) -> (String, usize, usize) {
        let tokens = lex(src).expect("lexing failed");
        let err = parse(src, &tokens).expect_err("parsing should fail");
        let (line, col) = line_col(src, err.offset);
        (err.message, line, col)
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(body("x = 1 + 2 * 3"), "x = (+ 1 (* 2 3))");
        assert_eq!(body("x = (1 + 2) * 3"), "x = (* (+ 1 2) 3)");
    }

    #[test]
    fn binary_operators_are_left_associative() {
        assert_eq!(body("x = a - b - c"), "x = (- (- a b) c)");
        assert_eq!(body("x = a / b * c % d"), "x = (% (* (/ a b) c) d)");
    }

    #[test]
    fn logic_and_comparison_precedence() {
        assert_eq!(body("x = !a && b || c"), "x = (|| (&& (not a) b) c)");
        assert_eq!(body("x = a + 1 < b * 2 && c != d"), "x = (&& (< (+ a 1) (* b 2)) (!= c d))");
    }

    #[test]
    fn unary_binds_tighter_than_binary() {
        assert_eq!(body("x = -a * b"), "x = (* (neg a) b)");
        assert_eq!(body("x = - -a"), "x = (neg (neg a))");
    }

    #[test]
    fn postfix_chains() {
        assert_eq!(body("x = a.b.c(1)[2].d"), "x = (index (call a.b.c 1) 2).d");
        assert_eq!(body("grid[i][j] = 0"), "(index (index grid i) j) = 0");
    }

    #[test]
    fn let_and_var_with_types() {
        assert_eq!(body("let a: i32 = 1\nvar b: List[Option[i32]] = c"), "let a: i32 = 1\nvar b: List[Option[i32]] = c");
    }

    #[test]
    fn struct_literal_is_not_parsed_in_conditions() {
        assert_eq!(body("if a == b {\nreturn 1\n}"), "if (== a b)\n  return 1");
        assert_eq!(body("if a == (P { x: 1 }) {\nreturn\n}"), "if (== a (new P x: 1))\n  return");
        assert_eq!(body("while f(P { x: 1 }) {\nbreak\n}"), "while (call f (new P x: 1))\n  break");
    }

    #[test]
    fn else_if_chain_and_else_on_next_line() {
        let out = body("if a {\nb()\n} else if c {\nd()\n}\nelse {\ncontinue\n}");
        assert_eq!(out, "if a\n  (call b)\nelse if c\n  (call d)\nelse\n  continue");
    }

    #[test]
    fn for_loops() {
        assert_eq!(body("for x in xs {\n}"), "for x in xs");
        assert_eq!(body("for i, x in items(a) {\nprint(i)\n}"), "for i, x in (call items a)\n  (call print i)");
    }

    #[test]
    fn match_arms() {
        let out = body("match v {\nSome(Pair(a, _)) => print(a)\n-1 => {\nreturn 0\n}\n\"x\" => return 1, true => break\n_ => x = 2\n}");
        assert_eq!(
            out,
            "match v\n  Some(Pair(a, _)) =>\n    (call print a)\n  -1 =>\n    return 0\n  \"x\" =>\n    return 1\n  true =>\n    break\n  _ =>\n    x = 2"
        );
    }

    #[test]
    fn multi_line_expressions() {
        assert_eq!(body("let x = a +\n    b"), "let x = (+ a b)");
        assert_eq!(body("f(\n  1,\n  mut b,\n)"), "(call f 1 (mut b))");
        assert_eq!(body("let y = list\n    .map(f)\n    .len"), "let y = (call list.map f).len");
        assert_eq!(body("let p = Point {\n  x: 1,\n  y: [\n    2,\n    3\n  ],\n}"), "let p = (new Point x: 1 y: (array 2 3))");
    }

    #[test]
    fn a_new_line_does_not_continue_a_call_or_index() {
        assert_eq!(body("a\n(b)\n[c]"), "a\nb\n(array c)");
    }

    #[test]
    fn items() {
        let out = tree("struct Pair[A, B] { first: A, second: B }\n\nenum Option[T] {\n  Some(T)\n  None\n}\nfn id[T](x: T) -> T {\n  return x\n}");
        assert_eq!(
            out,
            "struct Pair[A, B]\n  first: A\n  second: B\nenum Option[T]\n  Some(T)\n  None\nfn id[T](x: T) -> T\n  return x\n"
        );
    }

    #[test]
    fn empty_file_and_comments_only() {
        assert_eq!(tree(""), "");
        assert_eq!(tree("// nothing here\n\n"), "");
    }

    #[test]
    fn error_missing_closing_paren() {
        let (message, line, col) = error("fn main() {\n  print(1\n}");
        assert_eq!(message, "expected `)` but found `}`");
        assert_eq!((line, col), (3, 1));
    }

    #[test]
    fn error_two_statements_on_one_line() {
        let (message, line, col) = error("fn main() {\n  let a = 1 let b = 2\n}");
        assert_eq!(message, "expected end of line but found `let`");
        assert_eq!((line, col), (2, 13));
    }

    #[test]
    fn error_invalid_assignment_target() {
        let (message, line, col) = error("fn main() {\n  f() = 1\n}");
        assert_eq!(message, "cannot assign to this expression");
        assert_eq!((line, col), (2, 3));
    }

    #[test]
    fn error_statement_outside_function() {
        let (message, ..) = error("let a = 1");
        assert_eq!(message, "expected `fn`, `struct` or `enum` but found `let`");
    }

    #[test]
    fn error_unclosed_block() {
        let (message, ..) = error("fn main() {\n  let a = 1\n");
        assert_eq!(message, "expected `}` to close the block but reached end of file");
    }

    #[test]
    fn error_missing_expression() {
        let (message, line, col) = error("fn main() {\n  let a = \n}");
        assert_eq!(message, "expected an expression but found `}`");
        assert_eq!((line, col), (3, 1));
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_crash() {
        let src = format!("fn main() {{\n  let a = {}1{}\n}}", "(".repeat(5000), ")".repeat(5000));
        let (message, ..) = error(&src);
        assert_eq!(message, "code is nested too deeply");
        let src = format!("fn main() {{\n  let a = {}1\n}}", "-".repeat(5000));
        assert_eq!(error(&src).0, "code is nested too deeply");
    }

    /// Every truncation and many small corruptions of a real program must
    /// produce either a tree or an error, never a panic.
    #[test]
    fn damaged_input_never_panics() {
        let src = include_str!("../examples/point.azl");
        for end in 0..=src.len() {
            if !src.is_char_boundary(end) {
                continue;
            }
            let part = &src[..end];
            if let Ok(tokens) = lex(part) {
                let _ = parse(part, &tokens);
            }
        }
        let junk = ["{", "}", "(", ")", "[", "]", ",", ":", "=>", "=", "mut", "match", "if", "\n", ".", "-", "fn", "else"];
        let mut seed = 0x9E3779B97F4A7C15u64;
        for _ in 0..4000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mut at = (seed as usize) % src.len();
            while !src.is_char_boundary(at) {
                at -= 1;
            }
            let piece = junk[(seed >> 32) as usize % junk.len()];
            let damaged = format!("{}{}{}", &src[..at], piece, &src[at..]);
            if let Ok(tokens) = lex(&damaged) {
                let _ = parse(&damaged, &tokens);
            }
        }
    }
}
