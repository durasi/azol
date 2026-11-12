// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Prints a syntax tree as indented text. Expressions are written in
//! prefix form with explicit parentheses, so precedence is visible.

use crate::ast::*;
use std::fmt::Write;

pub fn dump(ast: &Ast, src: &str) -> String {
    let mut d = Dumper { ast, src, out: String::new() };
    for item in &ast.items {
        d.item(item);
    }
    d.out
}

struct Dumper<'a> {
    ast: &'a Ast,
    src: &'a str,
    out: String,
}

impl<'a> Dumper<'a> {
    fn indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.out.push_str("  ");
        }
    }

    fn generics(&mut self, generics: List) {
        if generics.len == 0 {
            return;
        }
        self.out.push('[');
        for (i, &id) in self.ast.list(generics).iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.out.push_str(self.ast.names[id as usize].text(self.src));
        }
        self.out.push(']');
    }

    fn ty(&mut self, id: TypeId) {
        let ty = self.ast.types[id as usize];
        self.out.push_str(ty.name.text(self.src));
        if ty.args.len > 0 {
            self.out.push('[');
            for (i, &arg) in self.ast.list(ty.args).iter().enumerate() {
                if i > 0 {
                    self.out.push_str(", ");
                }
                self.ty(arg);
            }
            self.out.push(']');
        }
    }

    fn item(&mut self, item: &Item) {
        match *item {
            Item::Fn { name, generics, params, ret, body, .. } => {
                let _ = write!(self.out, "fn {}", name.text(self.src));
                self.generics(generics);
                self.out.push('(');
                for (i, &id) in self.ast.list(params).iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    let param = self.ast.params[id as usize];
                    let _ = write!(self.out, "{}: ", param.name.text(self.src));
                    if param.mutable {
                        self.out.push_str("mut ");
                    }
                    self.ty(param.ty);
                }
                self.out.push(')');
                if let Some(ret) = ret {
                    self.out.push_str(" -> ");
                    self.ty(ret);
                }
                self.out.push('\n');
                self.block(body, 1);
            }
            Item::Struct { name, generics, fields } => {
                let _ = write!(self.out, "struct {}", name.text(self.src));
                self.generics(generics);
                self.out.push('\n');
                for &id in self.ast.list(fields) {
                    let field = self.ast.fields[id as usize];
                    let _ = write!(self.out, "  {}: ", field.name.text(self.src));
                    self.ty(field.ty);
                    self.out.push('\n');
                }
            }
            Item::Const { name, ty, value } => {
                let _ = write!(self.out, "const {}", name.text(self.src));
                if let Some(ty) = ty {
                    self.out.push_str(": ");
                    self.ty(ty);
                }
                self.out.push_str(" = ");
                self.expr(value);
                self.out.push('\n');
            }
            Item::Enum { name, generics, variants } => {
                let _ = write!(self.out, "enum {}", name.text(self.src));
                self.generics(generics);
                self.out.push('\n');
                for &id in self.ast.list(variants) {
                    let variant = self.ast.variants[id as usize];
                    let _ = write!(self.out, "  {}", variant.name.text(self.src));
                    if variant.fields.len > 0 {
                        self.out.push('(');
                        for (i, &ty) in self.ast.list(variant.fields).iter().enumerate() {
                            if i > 0 {
                                self.out.push_str(", ");
                            }
                            self.ty(ty);
                        }
                        self.out.push(')');
                    }
                    self.out.push('\n');
                }
            }
        }
    }

    fn block(&mut self, body: List, depth: usize) {
        for &id in self.ast.list(body) {
            self.stmt(id, depth);
        }
    }

    fn stmt(&mut self, id: StmtId, depth: usize) {
        self.indent(depth);
        match self.ast.stmts[id as usize] {
            Stmt::Let { mutable, name, ty, value } => {
                let _ = write!(self.out, "{} {}", if mutable { "var" } else { "let" }, name.text(self.src));
                if let Some(ty) = ty {
                    self.out.push_str(": ");
                    self.ty(ty);
                }
                self.out.push_str(" = ");
                self.expr(value);
                self.out.push('\n');
            }
            Stmt::Assign { target, value } => {
                self.expr(target);
                self.out.push_str(" = ");
                self.expr(value);
                self.out.push('\n');
            }
            Stmt::Expr(expr) => {
                self.expr(expr);
                self.out.push('\n');
            }
            Stmt::Return(value) => {
                self.out.push_str("return");
                if let Some(value) = value {
                    self.out.push(' ');
                    self.expr(value);
                }
                self.out.push('\n');
            }
            Stmt::If { .. } => self.if_chain(id, depth),
            Stmt::While { cond, body } => {
                self.out.push_str("while ");
                self.expr(cond);
                self.out.push('\n');
                self.block(body, depth + 1);
            }
            Stmt::For { index, item, iter, body } => {
                self.out.push_str("for ");
                if let Some(index) = index {
                    let _ = write!(self.out, "{}, ", index.text(self.src));
                }
                let _ = write!(self.out, "{} in ", item.text(self.src));
                self.expr(iter);
                self.out.push('\n');
                self.block(body, depth + 1);
            }
            Stmt::Match { scrutinee, arms } => {
                self.out.push_str("match ");
                self.expr(scrutinee);
                self.out.push('\n');
                for &arm_id in self.ast.list(arms) {
                    let arm = self.ast.arms[arm_id as usize];
                    self.indent(depth + 1);
                    self.pattern(arm.pattern);
                    self.out.push_str(" =>\n");
                    self.block(arm.body, depth + 2);
                }
            }
            Stmt::Break => self.out.push_str("break\n"),
            Stmt::Continue => self.out.push_str("continue\n"),
        }
    }

    fn if_chain(&mut self, mut id: StmtId, depth: usize) {
        loop {
            let Stmt::If { cond, then_block, else_branch } = self.ast.stmts[id as usize] else {
                unreachable!("if chain contains only if statements");
            };
            self.out.push_str("if ");
            self.expr(cond);
            self.out.push('\n');
            self.block(then_block, depth + 1);
            match else_branch {
                Else::None => return,
                Else::Block(body) => {
                    self.indent(depth);
                    self.out.push_str("else\n");
                    self.block(body, depth + 1);
                    return;
                }
                Else::If(next) => {
                    self.indent(depth);
                    self.out.push_str("else ");
                    id = next;
                }
            }
        }
    }

    fn pattern(&mut self, id: PatId) {
        match self.ast.patterns[id as usize] {
            Pattern::Wildcard => self.out.push('_'),
            Pattern::Name(name) => self.out.push_str(name.text(self.src)),
            Pattern::Variant { name, args } => {
                self.out.push_str(name.text(self.src));
                self.out.push('(');
                for (i, &arg) in self.ast.list(args).iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    self.pattern(arg);
                }
                self.out.push(')');
            }
            Pattern::Int { negative, digits } => {
                if negative {
                    self.out.push('-');
                }
                self.out.push_str(digits.text(self.src));
            }
            Pattern::Str(text) => self.out.push_str(text.text(self.src)),
            Pattern::Bool(value) => self.out.push_str(if value { "true" } else { "false" }),
        }
    }

    fn expr(&mut self, id: ExprId) {
        match self.ast.exprs[id as usize] {
            Expr::Int(s) | Expr::Float(s) | Expr::Str(s) | Expr::Ident(s) => {
                self.out.push_str(s.text(self.src));
            }
            Expr::Bool(value) => self.out.push_str(if value { "true" } else { "false" }),
            Expr::Unary { op, operand } => {
                self.out.push_str(match op {
                    UnOp::Neg => "(neg ",
                    UnOp::Not => "(not ",
                });
                self.expr(operand);
                self.out.push(')');
            }
            Expr::Binary { op, lhs, rhs } => {
                let _ = write!(self.out, "({} ", op.symbol());
                self.expr(lhs);
                self.out.push(' ');
                self.expr(rhs);
                self.out.push(')');
            }
            Expr::Call { callee, args } => {
                self.out.push_str("(call ");
                self.expr(callee);
                for &arg in self.ast.list(args) {
                    self.out.push(' ');
                    self.expr(arg);
                }
                self.out.push(')');
            }
            Expr::MutArg(value) => {
                self.out.push_str("(mut ");
                self.expr(value);
                self.out.push(')');
            }
            Expr::Field { base, name } => {
                self.expr(base);
                self.out.push('.');
                self.out.push_str(name.text(self.src));
            }
            Expr::Index { base, index } => {
                self.out.push_str("(index ");
                self.expr(base);
                self.out.push(' ');
                self.expr(index);
                self.out.push(')');
            }
            Expr::StructLit { name, fields } => {
                let _ = write!(self.out, "(new {}", name.text(self.src));
                for &field_id in self.ast.list(fields) {
                    let field = self.ast.field_inits[field_id as usize];
                    let _ = write!(self.out, " {}: ", field.name.text(self.src));
                    self.expr(field.value);
                }
                self.out.push(')');
            }
            Expr::ArrayLit { items } => {
                self.out.push_str("(array");
                for &item in self.ast.list(items) {
                    self.out.push(' ');
                    self.expr(item);
                }
                self.out.push(')');
            }
        }
    }
}
