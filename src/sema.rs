// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Name resolution, type checking and the first part of the memory
//! safety analysis, for the part of the language `azol build` compiles:
//! `i64`, `bool` and struct values, functions with read-only and `mut`
//! parameters, `let`/`var`, assignment, `if`, `while`, `return`, `print`.
//!
//! Values have value semantics. A struct argument is passed by reference
//! for speed, but the callee cannot change it unless the parameter is
//! `mut`, and one call can never be handed two overlapping places when
//! one of them is `mut`. Together these make a reference indistinguishable
//! from a private copy.

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
    /// Index into `Program::structs`.
    Struct(u32),
}

/// Index of the built-in `print` in call resolutions.
pub const BUILTIN_PRINT: u32 = u32::MAX;
/// Set in the resolution of a name whose stack slot holds a pointer.
pub const BY_REF: u32 = 1 << 31;

/// Most parameters a function can take: they travel in registers x0-x7.
pub const MAX_PARAMS: usize = 8;
/// Keeps every local reachable with one load/store instruction.
pub const MAX_SLOTS: u32 = 500;

#[derive(Debug)]
pub struct StructInfo<'a> {
    pub name: &'a str,
    /// Size in 8-byte slots.
    pub size: u32,
    pub fields: Vec<FieldInfo<'a>>,
}

#[derive(Debug)]
pub struct FieldInfo<'a> {
    pub name: &'a str,
    pub ty: Ty,
    /// Offset in slots from the start of the struct.
    pub offset: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ParamInfo {
    pub ty: Ty,
    /// Passed as a pointer: every `mut` parameter and every struct.
    pub by_ref: bool,
    pub mutable: bool,
}

#[derive(Debug)]
pub struct FnInfo {
    pub ret: Ty,
    pub params: Vec<ParamInfo>,
    /// Stack slots for parameters, locals and temporaries.
    pub slots: u32,
    /// Slot that keeps the caller's result address when a struct is returned.
    pub sret_slot: Option<u32>,
}

#[derive(Debug)]
pub struct Program<'a> {
    pub structs: Vec<StructInfo<'a>>,
    /// Parallel to the functions in `Ast::items`.
    pub fns: Vec<FnInfo>,
    pub main: u32,
    /// Per expression: for a name its stack slot (with `BY_REF` when the
    /// slot holds a pointer), for a field access the field's offset, for a
    /// call the function index, for an integer literal the index in `ints`.
    pub expr_res: Vec<u32>,
    /// Per expression: its type.
    pub expr_ty: Vec<Ty>,
    /// Per struct-valued call or struct literal: the first slot of the
    /// temporary that holds its result.
    pub expr_tmp: Vec<u32>,
    /// Per `let`/`var` statement: its first stack slot.
    pub stmt_slot: Vec<u32>,
    /// Per field initializer in a struct literal: the field's offset.
    pub init_offset: Vec<u32>,
    pub ints: Vec<i64>,
}

impl Program<'_> {
    pub fn size_of(&self, ty: Ty) -> u32 {
        match ty {
            Ty::I64 | Ty::Bool => 1,
            Ty::Unit => 0,
            Ty::Struct(index) => self.structs[index as usize].size,
        }
    }
}

struct Local<'a> {
    name: &'a str,
    slot: u32,
    ty: Ty,
    mutable: bool,
    by_ref: bool,
    is_param: bool,
}

/// A variable or a field path inside one, as a range of slots.
#[derive(Clone, Copy)]
struct Place {
    root: u32,
    start: u32,
    len: u32,
    mutable: bool,
    is_param: bool,
}

/// A place handed by reference to a call that has not finished yet.
struct Borrow {
    place: Place,
    mutable: bool,
}

struct Checker<'a> {
    ast: &'a Ast,
    src: &'a str,
    structs: Vec<StructInfo<'a>>,
    struct_names: HashMap<&'a str, u32>,
    fns: Vec<FnInfo>,
    fn_names: HashMap<&'a str, u32>,
    expr_res: Vec<u32>,
    expr_ty: Vec<Ty>,
    expr_tmp: Vec<u32>,
    stmt_slot: Vec<u32>,
    init_offset: Vec<u32>,
    ints: Vec<i64>,
    locals: Vec<Local<'a>>,
    borrows: Vec<Borrow>,
    ret: Ty,
    next_slot: u32,
    loop_depth: u32,
}

pub fn check<'a>(ast: &'a Ast, src: &'a str) -> Result<Program<'a>, SemaError> {
    let mut c = Checker {
        ast,
        src,
        structs: Vec::new(),
        struct_names: HashMap::new(),
        fns: Vec::new(),
        fn_names: HashMap::new(),
        expr_res: vec![0; ast.exprs.len()],
        expr_ty: vec![Ty::Unit; ast.exprs.len()],
        expr_tmp: vec![0; ast.exprs.len()],
        stmt_slot: vec![0; ast.stmts.len()],
        init_offset: vec![0; ast.field_inits.len()],
        ints: Vec::new(),
        locals: Vec::new(),
        borrows: Vec::new(),
        ret: Ty::Unit,
        next_slot: 0,
        loop_depth: 0,
    };
    c.declare_structs()?;
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
    Ok(Program {
        structs: c.structs,
        fns: c.fns,
        main,
        expr_res: c.expr_res,
        expr_ty: c.expr_ty,
        expr_tmp: c.expr_tmp,
        stmt_slot: c.stmt_slot,
        init_offset: c.init_offset,
        ints: c.ints,
    })
}

fn err<T>(offset: u32, message: impl Into<String>) -> SResult<T> {
    Err(SemaError { message: message.into(), offset })
}

fn overlaps(a: &Place, b: &Place) -> bool {
    a.root == b.root && a.start < b.start + b.len.max(1) && b.start < a.start + a.len.max(1)
}

impl<'a> Checker<'a> {
    fn ty_name(&self, ty: Ty) -> String {
        match ty {
            Ty::I64 => "i64".to_string(),
            Ty::Bool => "bool".to_string(),
            Ty::Unit => "no value".to_string(),
            Ty::Struct(index) => self.structs[index as usize].name.to_string(),
        }
    }

    fn size_of(&self, ty: Ty) -> u32 {
        match ty {
            Ty::I64 | Ty::Bool => 1,
            Ty::Unit => 0,
            Ty::Struct(index) => self.structs[index as usize].size,
        }
    }

    fn ty(&self, id: TypeId) -> SResult<Ty> {
        let ty = self.ast.types[id as usize];
        let name = ty.name.text(self.src);
        if ty.args.len > 0 {
            return err(ty.name.start, "generic types are not supported by `azol build` yet");
        }
        match name {
            "i64" => Ok(Ty::I64),
            "bool" => Ok(Ty::Bool),
            _ => match self.struct_names.get(name) {
                Some(&index) => Ok(Ty::Struct(index)),
                None => err(ty.name.start, format!("unknown type `{name}`; the built-in types so far are `i64` and `bool`")),
            },
        }
    }

    // ---- declarations --------------------------------------------------

    fn declare_structs(&mut self) -> SResult<()> {
        let mut decls = Vec::new();
        for item in &self.ast.items {
            match *item {
                Item::Struct { name, generics, fields } => {
                    let text = name.text(self.src);
                    if generics.len > 0 {
                        return err(name.start, "generic structs are not supported by `azol build` yet");
                    }
                    if text == "i64" || text == "bool" {
                        return err(name.start, format!("`{text}` is a built-in type and cannot be redefined"));
                    }
                    let index = self.structs.len() as u32;
                    if self.struct_names.insert(text, index).is_some() {
                        return err(name.start, format!("struct `{text}` is defined more than once"));
                    }
                    self.structs.push(StructInfo { name: text, size: 0, fields: Vec::new() });
                    decls.push((name, fields));
                }
                Item::Enum { name, .. } => {
                    return err(name.start, "enums are not supported by `azol build` yet");
                }
                Item::Fn { .. } => {}
            }
        }
        // Field types may name structs declared later, so resolve them
        // only after every struct name is known.
        for (index, &(_, fields)) in decls.iter().enumerate() {
            let mut infos: Vec<FieldInfo<'a>> = Vec::new();
            for &id in self.ast.list(fields) {
                let field = self.ast.fields[id as usize];
                let name = field.name.text(self.src);
                if infos.iter().any(|f| f.name == name) {
                    return err(field.name.start, format!("field `{name}` is declared more than once"));
                }
                infos.push(FieldInfo { name, ty: self.ty(field.ty)?, offset: 0 });
            }
            self.structs[index].fields = infos;
        }
        // Sizes and offsets, depth first. A struct that contains itself
        // would have no finite size.
        let mut state = vec![0u8; self.structs.len()];
        for index in 0..self.structs.len() {
            self.layout(index, &mut state, &decls)?;
        }
        Ok(())
    }

    fn layout(&mut self, index: usize, state: &mut [u8], decls: &[(Span, List)]) -> SResult<u32> {
        match state[index] {
            2 => return Ok(self.structs[index].size),
            1 => {
                return err(
                    decls[index].0.start,
                    format!("struct `{}` contains itself, so it would have no finite size", self.structs[index].name),
                );
            }
            _ => {}
        }
        state[index] = 1;
        let mut offset = 0;
        for field in 0..self.structs[index].fields.len() {
            let size = match self.structs[index].fields[field].ty {
                Ty::Struct(inner) => self.layout(inner as usize, state, decls)?,
                _ => 1,
            };
            self.structs[index].fields[field].offset = offset;
            offset += size;
        }
        if offset > MAX_SLOTS {
            return err(decls[index].0.start, format!("struct `{}` is too large for now", self.structs[index].name));
        }
        self.structs[index].size = offset;
        state[index] = 2;
        Ok(offset)
    }

    fn signatures(&mut self) -> SResult<()> {
        for item in &self.ast.items {
            let Item::Fn { name, generics, params, ret, .. } = *item else {
                continue;
            };
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
            let mut infos = Vec::with_capacity(param_ids.len());
            for &id in param_ids {
                let param = self.ast.params[id as usize];
                let ty = self.ty(param.ty)?;
                infos.push(ParamInfo { ty, by_ref: param.mutable || matches!(ty, Ty::Struct(_)), mutable: param.mutable });
            }
            let ret = match ret {
                Some(id) => self.ty(id)?,
                None => Ty::Unit,
            };
            if text == "main" {
                if !infos.is_empty() {
                    return err(name.start, "`main` cannot take parameters");
                }
                if !matches!(ret, Ty::I64 | Ty::Unit) {
                    return err(name.start, "`main` must return `i64` or nothing");
                }
            }
            let index = self.fns.len() as u32;
            if self.fn_names.insert(text, index).is_some() {
                return err(name.start, format!("function `{text}` is defined more than once"));
            }
            self.fns.push(FnInfo { ret, params: infos, slots: 0, sret_slot: None });
        }
        Ok(())
    }

    // ---- functions and statements --------------------------------------

    fn function(&mut self, index: u32, name: Span, params: List, body: List) -> SResult<()> {
        self.locals.clear();
        self.borrows.clear();
        self.next_slot = 0;
        self.loop_depth = 0;
        self.ret = self.fns[index as usize].ret;
        for (i, &id) in self.ast.list(params).iter().enumerate() {
            let param = self.ast.params[id as usize];
            let info = self.fns[index as usize].params[i];
            let slot = self.alloc(1, param.name.start)?;
            self.locals.push(Local {
                name: param.name.text(self.src),
                slot,
                ty: info.ty,
                mutable: info.mutable,
                by_ref: info.by_ref,
                is_param: true,
            });
        }
        if matches!(self.ret, Ty::Struct(_)) {
            self.fns[index as usize].sret_slot = Some(self.alloc(1, name.start)?);
        }
        self.block(body)?;
        if self.ret != Ty::Unit && !self.always_returns(body) {
            return err(
                name.start,
                format!("function `{}` must return a value of type `{}` on every path", name.text(self.src), self.ty_name(self.ret)),
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

    fn alloc(&mut self, slots: u32, at: u32) -> SResult<u32> {
        if self.next_slot + slots > MAX_SLOTS {
            return err(at, format!("this function needs more than {MAX_SLOTS} slots of stack, which is not supported yet"));
        }
        let start = self.next_slot;
        self.next_slot += slots;
        Ok(start)
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
                            format!("expected `{}` but this is `{}`", self.ty_name(declared), self.ty_name(value_ty)),
                        );
                    }
                }
                let slot = self.alloc(self.size_of(value_ty), name.start)?;
                self.locals.push(Local { name: name.text(self.src), slot, ty: value_ty, mutable, by_ref: false, is_param: false });
                self.stmt_slot[id as usize] = slot;
            }
            Stmt::Assign { target, value } => {
                let target_ty = self.expr(target)?;
                let Some(place) = self.place(target) else {
                    return err(pos, "only a variable or a field of a variable can be assigned");
                };
                if !place.mutable {
                    let root = self.root_name(target);
                    return err(
                        pos,
                        if place.is_param {
                            format!("cannot change `{root}`: a parameter is read-only unless it is declared `mut`")
                        } else {
                            format!("cannot change `{root}`: declare it with `var` to make it changeable")
                        },
                    );
                }
                let value_ty = self.expr(value)?;
                if value_ty != target_ty {
                    return err(
                        self.ast.expr_pos[value as usize],
                        format!("expected `{}` but this is `{}`", self.ty_name(target_ty), self.ty_name(value_ty)),
                    );
                }
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
                            (ret, Ty::Unit) => format!("this function must return a value of type `{}`", self.ty_name(ret)),
                            (ret, got) => format!("this function returns `{}` but this is `{}`", self.ty_name(ret), self.ty_name(got)),
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
            return err(self.ast.expr_pos[cond as usize], format!("a condition must be `bool` but this is `{}`", self.ty_name(ty)));
        }
        Ok(())
    }

    // ---- places --------------------------------------------------------

    /// The variable, or field path inside a variable, that an already
    /// checked expression names. `None` for a computed value.
    fn place(&self, id: ExprId) -> Option<Place> {
        match self.ast.exprs[id as usize] {
            Expr::Ident(name) => {
                let local = self.lookup(name.text(self.src))?;
                Some(Place { root: local.slot, start: 0, len: self.size_of(local.ty), mutable: local.mutable, is_param: local.is_param })
            }
            Expr::Field { base, .. } => {
                let mut place = self.place(base)?;
                place.start += self.expr_res[id as usize];
                place.len = self.size_of(self.expr_ty[id as usize]);
                Some(place)
            }
            _ => None,
        }
    }

    fn root_name(&self, id: ExprId) -> &'a str {
        match self.ast.exprs[id as usize] {
            Expr::Ident(name) => name.text(self.src),
            Expr::Field { base, .. } => self.root_name(base),
            _ => "this value",
        }
    }

    /// Records that `place` is handed to a call by reference, after
    /// checking it against the places other unfinished calls hold.
    fn borrow(&mut self, id: ExprId, place: Place, mutable: bool) -> SResult<()> {
        for other in &self.borrows {
            if (mutable || other.mutable) && overlaps(&place, &other.place) {
                let name = self.root_name(id);
                return err(
                    self.ast.expr_pos[id as usize],
                    format!("`{name}` is passed twice in one call and one of the uses is `mut`; copy it into a variable first"),
                );
            }
        }
        self.borrows.push(Borrow { place, mutable });
        Ok(())
    }

    // ---- expressions ---------------------------------------------------

    fn expect(&mut self, expr: ExprId, want: Ty, what: &str) -> SResult<()> {
        let ty = self.expr(expr)?;
        if ty != want {
            return err(
                self.ast.expr_pos[expr as usize],
                format!("{what} needs `{}` but this is `{}`", self.ty_name(want), self.ty_name(ty)),
            );
        }
        Ok(())
    }

    fn expr(&mut self, id: ExprId) -> SResult<Ty> {
        let ty = self.expr_inner(id)?;
        self.expr_ty[id as usize] = ty;
        Ok(ty)
    }

    fn expr_inner(&mut self, id: ExprId) -> SResult<Ty> {
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
                    } else if self.struct_names.contains_key(text) {
                        err(pos, format!("`{text}` is a struct; create a value with `{text} {{ ... }}`"))
                    } else {
                        err(pos, format!("unknown variable `{text}`"))
                    };
                };
                let (slot, ty, by_ref) = (local.slot, local.ty, local.by_ref);
                self.expr_res[id as usize] = slot | if by_ref { BY_REF } else { 0 };
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
                        match left {
                            Ty::Unit => return err(self.ast.expr_pos[lhs as usize], "this expression has no value to compare"),
                            Ty::Struct(_) => return err(pos, format!("structs cannot be compared with {what} yet; compare their fields")),
                            Ty::I64 | Ty::Bool => {}
                        }
                        self.expect(rhs, left, &what)?;
                        Ok(Ty::Bool)
                    }
                }
            }
            Expr::Field { base, name } => {
                let base_ty = self.expr(base)?;
                let field_name = name.text(self.src);
                let Ty::Struct(index) = base_ty else {
                    return err(name.start, format!("`{}` has no fields", self.ty_name(base_ty)));
                };
                let info = &self.structs[index as usize];
                let Some(field) = info.fields.iter().find(|f| f.name == field_name) else {
                    return err(name.start, format!("struct `{}` has no field `{field_name}`", info.name));
                };
                self.expr_res[id as usize] = field.offset;
                Ok(field.ty)
            }
            Expr::StructLit { name, fields } => {
                let text = name.text(self.src);
                let Some(&index) = self.struct_names.get(text) else {
                    return err(pos, format!("unknown struct `{text}`"));
                };
                let field_count = self.structs[index as usize].fields.len();
                let mut seen = vec![false; field_count];
                for &init_id in self.ast.list(fields) {
                    let init = self.ast.field_inits[init_id as usize];
                    let field_name = init.name.text(self.src);
                    let Some(at) = self.structs[index as usize].fields.iter().position(|f| f.name == field_name) else {
                        return err(init.name.start, format!("struct `{text}` has no field `{field_name}`"));
                    };
                    if seen[at] {
                        return err(init.name.start, format!("field `{field_name}` is given more than once"));
                    }
                    seen[at] = true;
                    let (want, offset) = {
                        let field = &self.structs[index as usize].fields[at];
                        (field.ty, field.offset)
                    };
                    self.expect(init.value, want, &format!("field `{field_name}`"))?;
                    self.init_offset[init_id as usize] = offset;
                }
                if let Some(missing) = seen.iter().position(|&given| !given) {
                    let field_name = self.structs[index as usize].fields[missing].name;
                    return err(pos, format!("field `{field_name}` of `{text}` is missing"));
                }
                let ty = Ty::Struct(index);
                self.expr_tmp[id as usize] = self.alloc(self.size_of(ty), pos)?;
                Ok(ty)
            }
            Expr::Call { callee, args } => self.call(id, callee, args),
            Expr::MutArg(_) => err(pos, "`mut` can only be used on a function argument"),
            Expr::Float(_) => err(pos, "floating-point numbers are not supported by `azol build` yet"),
            Expr::Str(_) => err(pos, "strings are not supported by `azol build` yet"),
            Expr::Index { .. } | Expr::ArrayLit { .. } => err(pos, "arrays are not supported by `azol build` yet"),
        }
    }

    fn call(&mut self, id: ExprId, callee: ExprId, args: List) -> SResult<Ty> {
        let pos = self.ast.expr_pos[id as usize];
        let Expr::Ident(name) = self.ast.exprs[callee as usize] else {
            return err(pos, "only functions can be called");
        };
        let text = name.text(self.src);
        let arg_ids = self.ast.list(args);
        if self.lookup(text).is_some() {
            return err(pos, format!("`{text}` is a variable, not a function"));
        }
        if text == "print" {
            if arg_ids.len() != 1 {
                return err(pos, format!("`print` takes 1 argument but {} were given", arg_ids.len()));
            }
            self.expect(arg_ids[0], Ty::I64, "`print`")?;
            self.expr_res[id as usize] = BUILTIN_PRINT;
            return Ok(Ty::Unit);
        }
        let Some(&index) = self.fn_names.get(text) else {
            return if self.struct_names.contains_key(text) {
                err(pos, format!("`{text}` is a struct; create a value with `{text} {{ ... }}`"))
            } else {
                err(pos, format!("unknown function `{text}`"))
            };
        };
        let expected = self.fns[index as usize].params.len();
        if arg_ids.len() != expected {
            return err(
                pos,
                format!("`{text}` takes {expected} argument{} but {} were given", if expected == 1 { "" } else { "s" }, arg_ids.len()),
            );
        }

        // Places handed over by reference stay borrowed until this call
        // returns, so later arguments are checked against them.
        let outer_borrows = self.borrows.len();
        for (i, &arg) in arg_ids.iter().enumerate() {
            let param = self.fns[index as usize].params[i];
            let arg_pos = self.ast.expr_pos[arg as usize];
            let what = format!("argument {} of `{text}`", i + 1);
            match (param.mutable, self.ast.exprs[arg as usize]) {
                (true, Expr::MutArg(inner)) => {
                    self.expect(inner, param.ty, &what)?;
                    self.expr_ty[arg as usize] = param.ty;
                    let Some(place) = self.place(inner) else {
                        return err(arg_pos, "`mut` needs a variable or a field of a variable");
                    };
                    if !place.mutable {
                        let root = self.root_name(inner);
                        return err(
                            arg_pos,
                            if place.is_param {
                                format!("cannot pass `{root}` as `mut`: a parameter is read-only unless it is declared `mut`")
                            } else {
                                format!("cannot pass `{root}` as `mut`: declare it with `var` to make it changeable")
                            },
                        );
                    }
                    self.borrow(inner, place, true)?;
                }
                (true, _) => {
                    return err(arg_pos, format!("{what} can be changed by the function, so it must be written `mut <variable>`"));
                }
                (false, Expr::MutArg(_)) => {
                    return err(arg_pos, format!("{what} is not a `mut` parameter; remove `mut`"));
                }
                (false, _) => {
                    self.expect(arg, param.ty, &what)?;
                    if param.by_ref {
                        if let Some(place) = self.place(arg) {
                            self.borrow(arg, place, false)?;
                        }
                    }
                }
            }
        }
        self.borrows.truncate(outer_borrows);

        let ret = self.fns[index as usize].ret;
        self.expr_res[id as usize] = index;
        if matches!(ret, Ty::Struct(_)) {
            self.expr_tmp[id as usize] = self.alloc(self.size_of(ret), pos)?;
        }
        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{lex, line_col};
    use crate::parser::parse;

    fn check_src(src: &str) -> Result<(), (String, usize, usize)> {
        let tokens = lex(src).expect("lexing failed");
        let ast = parse(src, &tokens).expect("parsing failed");
        check(&ast, src).map(|_| ()).map_err(|e| {
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

    const POINT: &str = "struct Point {\n x: i64\n y: i64\n}\nstruct Line {\n from: Point\n to: Point\n}\n";

    fn with_point(rest: &str) -> String {
        format!("{POINT}{rest}")
    }

    #[test]
    fn accepts_a_valid_program_and_counts_slots() {
        let src = "fn add(a: i64, b: i64) -> i64 {\n let c = a + b\n return c\n}\nfn main() -> i64 {\n return add(1, 2)\n}";
        let tokens = lex(src).unwrap();
        let ast = parse(src, &tokens).unwrap();
        let program = check(&ast, src).unwrap();
        assert_eq!(program.fns.len(), 2);
        assert_eq!(program.fns[0].slots, 3);
        assert_eq!(program.main, 1);
        assert_eq!(program.ints, vec![1, 2]);
    }

    #[test]
    fn struct_layout() {
        let src = with_point("fn main() {\n}");
        let tokens = lex(&src).unwrap();
        let ast = parse(&src, &tokens).unwrap();
        let program = check(&ast, &src).unwrap();
        assert_eq!(program.structs[0].size, 2);
        assert_eq!(program.structs[1].size, 4);
        assert_eq!(program.structs[1].fields[1].offset, 2);
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
        assert_eq!(error(&in_main("let a = 1\na = 2")), "cannot change `a`: declare it with `var` to make it changeable");
        assert_eq!(error(&in_main("var a = 1\na = false")), "expected `i64` but this is `bool`");
        assert_eq!(error(&in_main("if true {\nlet inner = 1\n}\nprint(inner)")), "unknown variable `inner`");
        assert_eq!(error(&in_main("let a = main")), "`main` is a function; call it with `main(...)`");
        assert_eq!(error("fn f(a: i64) {\n a = 1\n}\nfn main() {\n}"), "cannot change `a`: a parameter is read-only unless it is declared `mut`");
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
    fn struct_declaration_errors() {
        assert_eq!(error("struct A {\n b: B\n}\nstruct B {\n a: A\n}\nfn main() {\n}"), "struct `A` contains itself, so it would have no finite size");
        assert_eq!(error("struct A {\n x: i64\n x: bool\n}\nfn main() {\n}"), "field `x` is declared more than once");
        assert_eq!(error("struct A {\n}\nstruct A {\n}\nfn main() {\n}"), "struct `A` is defined more than once");
        assert_eq!(error("struct A {\n x: Missing\n}\nfn main() {\n}"), "unknown type `Missing`; the built-in types so far are `i64` and `bool`");
        assert_eq!(error("struct i64 {\n}\nfn main() {\n}"), "`i64` is a built-in type and cannot be redefined");
    }

    #[test]
    fn struct_value_errors() {
        let main = |body: &str| with_point(&in_main(body));
        assert_eq!(error(&main("let p = Point { x: 1 }")), "field `y` of `Point` is missing");
        assert_eq!(error(&main("let p = Point { x: 1, y: 2, z: 3 }")), "struct `Point` has no field `z`");
        assert_eq!(error(&main("let p = Point { x: 1, x: 2, y: 3 }")), "field `x` is given more than once");
        assert_eq!(error(&main("let p = Point { x: true, y: 2 }")), "field `x` needs `i64` but this is `bool`");
        assert_eq!(error(&main("let p = Nope { x: 1 }")), "unknown struct `Nope`");
        assert_eq!(error(&main("let p = Point { x: 1, y: 2 }\nprint(p.z)")), "struct `Point` has no field `z`");
        assert_eq!(error(&main("let n = 1\nprint(n.x)")), "`i64` has no fields");
        assert_eq!(error(&main("let p = Point { x: 1, y: 2 }\np.x = 5")), "cannot change `p`: declare it with `var` to make it changeable");
        assert_eq!(error(&main("var p = Point { x: 1, y: 2 }\np.x = true")), "expected `i64` but this is `bool`");
        assert_eq!(error(&main("var p = Point { x: 1, y: 2 }\np = 3")), "expected `Point` but this is `i64`");
        assert_eq!(error(&main("let p = Point { x: 1, y: 2 }\nif p == p {\n}")), "structs cannot be compared with `==` yet; compare their fields");
        assert_eq!(error(&main("let p = Point")), "`Point` is a struct; create a value with `Point { ... }`");
        assert_eq!(error(&main("Point { x: 1, y: 2 }.x = 3")), "only a variable or a field of a variable can be assigned");
        assert!(check_src(&main("var l = Line { from: Point { x: 1, y: 2 }, to: Point { x: 3, y: 4 } }\nl.to.y = l.from.x\nl.from = l.to")).is_ok());
    }

    #[test]
    fn parameters_are_read_only_unless_mut() {
        let with_fn = |f: &str, body: &str| with_point(&format!("{f}\n{}", in_main(body)));
        assert_eq!(
            error(&with_fn("fn f(p: Point) {\n p.x = 1\n}", "")),
            "cannot change `p`: a parameter is read-only unless it is declared `mut`"
        );
        assert!(check_src(&with_fn("fn f(p: mut Point, n: mut i64) {\n p.x = 1\n n = n + 1\n}", "")).is_ok());
        let shift = "fn shift(p: mut Point, dx: i64) {\n p.x = p.x + dx\n}";
        assert!(check_src(&with_fn(shift, "var p = Point { x: 1, y: 2 }\nshift(mut p, 3)")).is_ok());
        assert_eq!(
            error(&with_fn(shift, "var p = Point { x: 1, y: 2 }\nshift(p, 3)")),
            "argument 1 of `shift` can be changed by the function, so it must be written `mut <variable>`"
        );
        assert_eq!(
            error(&with_fn(shift, "let p = Point { x: 1, y: 2 }\nshift(mut p, 3)")),
            "cannot pass `p` as `mut`: declare it with `var` to make it changeable"
        );
        assert_eq!(
            error(&with_fn(shift, "var p = Point { x: 1, y: 2 }\nshift(mut p, mut p.x)")),
            "argument 2 of `shift` is not a `mut` parameter; remove `mut`"
        );
        assert_eq!(
            error(&with_fn(shift, "shift(mut Point { x: 1, y: 2 }, 3)")),
            "`mut` needs a variable or a field of a variable"
        );
        assert_eq!(
            error(&with_fn(&format!("{shift}\nfn g(q: Point) {{\n shift(mut q, 1)\n}}"), "")),
            "cannot pass `q` as `mut`: a parameter is read-only unless it is declared `mut`"
        );
    }

    #[test]
    fn one_call_cannot_get_overlapping_places_when_one_is_mut() {
        let fns = "fn swap(a: mut Point, b: mut Point) {\n}\nfn mix(a: mut Point, b: Point) {\n}\nfn both(a: Point, b: Point) {\n}\nfn bump(n: mut i64, by: i64) {\n}\nfn two(a: mut i64, b: mut i64) {\n}\nfn id(p: Point) -> Point {\n return p\n}\nfn touch(p: mut Point) -> i64 {\n return 1\n}\nfn sum(p: Point, n: i64) -> i64 {\n return n\n}";
        let main = |body: &str| with_point(&format!("{fns}\n{}", in_main(&format!("var p = Point {{ x: 1, y: 2 }}\nvar q = Point {{ x: 3, y: 4 }}\nvar l = Line {{ from: p, to: q }}\n{body}"))));
        let twice = "is passed twice in one call and one of the uses is `mut`; copy it into a variable first";
        assert_eq!(error(&main("swap(mut p, mut p)")), format!("`p` {twice}"));
        assert_eq!(error(&main("mix(mut p, p)")), format!("`p` {twice}"));
        assert_eq!(error(&main("mix(mut l.from, l.from)")), format!("`l` {twice}"));
        assert_eq!(error(&main("swap(mut l, mut l)")), "argument 1 of `swap` needs `Point` but this is `Line`");
        assert_eq!(error(&main("two(mut p.x, mut p.x)")), format!("`p` {twice}"));
        assert_eq!(error(&main("mix(mut p, id(p))")), format!("`p` {twice}"));
        assert_eq!(error(&main("print(sum(p, touch(mut p)))")), format!("`p` {twice}"));
        // Different variables, different fields, read-only twice, by-value
        // reads and already finished calls are all fine.
        assert!(check_src(&main("swap(mut p, mut q)")).is_ok());
        assert!(check_src(&main("swap(mut l.from, mut l.to)")).is_ok());
        assert!(check_src(&main("two(mut p.x, mut p.y)")).is_ok());
        assert!(check_src(&main("both(p, p)")).is_ok());
        assert!(check_src(&main("bump(mut p.x, p.x)")).is_ok());
        assert!(check_src(&main("print(sum(id(p), touch(mut p)))")).is_ok());
        assert!(check_src(&main("mix(mut p, q)\nmix(mut q, p)")).is_ok());
    }

    #[test]
    fn features_not_built_yet_are_reported_clearly() {
        assert_eq!(error("enum E {\n A\n}\nfn main() {\n}"), "enums are not supported by `azol build` yet");
        assert_eq!(error(&in_main("let a = \"text\"")), "strings are not supported by `azol build` yet");
        assert_eq!(error(&in_main("let a = 1.5")), "floating-point numbers are not supported by `azol build` yet");
        assert_eq!(error(&in_main("for x in y {\n}")), "`for` loops are not supported by `azol build` yet; use `while`");
        assert_eq!(error("fn f(a: i32) {\n}\nfn main() {\n}"), "unknown type `i32`; the built-in types so far are `i64` and `bool`");
    }
}
