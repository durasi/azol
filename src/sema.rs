// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Name resolution, type checking and the memory safety analysis, for
//! the part of the language `azol build` compiles: `i64`, `bool`, `str`,
//! struct and `List` values, functions with read-only and `mut`
//! parameters, `let`/`var`, assignment, `if`, `while`, `for`, `return`.
//!
//! Values have value semantics. Aggregates are passed by reference for
//! speed, but the callee cannot change one unless the parameter is `mut`,
//! and a place can never be changed while something else still refers to
//! it: another argument of the same call, or a loop iterating over it.
//! Together these make a reference indistinguishable from a private copy.
//!
//! A list owns a heap buffer. Every buffer has exactly one owner: a
//! variable, or a temporary that lives until the end of its statement.
//! This pass decides, for each value that is stored somewhere, whether it
//! is moved (a temporary handing its buffer over) or copied (a variable
//! being duplicated), and which temporaries each statement must free.

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
    /// Text: a pointer and a length in bytes. So far only literals exist,
    /// so the bytes always live in the executable itself.
    Str,
    /// Index into `Program::structs`.
    Struct(u32),
    /// A growable array: pointer, length, capacity. The index selects the
    /// element type in `Program::lists`.
    List(u32),
}

impl Ty {
    /// Values that live in memory and are handed around by address.
    pub fn is_aggregate(self) -> bool {
        matches!(self, Ty::Str | Ty::Struct(_) | Ty::List(_))
    }
}

/// Built-in functions and methods, as call resolutions.
pub const BUILTIN_PRINT_I64: u32 = u32::MAX;
pub const BUILTIN_PRINT_BOOL: u32 = u32::MAX - 1;
pub const BUILTIN_PRINT_STR: u32 = u32::MAX - 2;
pub const BUILTIN_LIST_PUSH: u32 = u32::MAX - 3;
pub const BUILTIN_LIST_POP: u32 = u32::MAX - 4;
pub const BUILTIN_LIST_LEN: u32 = u32::MAX - 5;
/// Resolutions at or above this value are built-ins.
pub const FIRST_BUILTIN: u32 = u32::MAX - 5;
/// Set in the resolution of a name whose stack slot holds a pointer.
pub const BY_REF: u32 = 1 << 31;

/// Most parameters a function can take: they travel in registers x0-x7.
pub const MAX_PARAMS: usize = 8;
/// Keeps every local reachable with one load/store instruction.
pub const MAX_SLOTS: u32 = 500;

/// How a `for` loop reaches the list it iterates over.
pub const FOR_IN_PLACE: u8 = 0;
/// The list is a temporary; the loop takes it over and frees it.
pub const FOR_MOVED: u8 = 1;
/// The list is part of a temporary; the loop works on its own copy.
pub const FOR_COPIED: u8 = 2;

#[derive(Debug)]
pub struct StructInfo<'a> {
    pub name: &'a str,
    /// Size in 8-byte slots.
    pub size: u32,
    pub fields: Vec<FieldInfo<'a>>,
    /// Lists inside the struct: slot offset and element size in bytes.
    pub heap: Vec<(u32, u32)>,
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
    /// Passed as a pointer: every `mut` parameter and every aggregate.
    pub by_ref: bool,
    pub mutable: bool,
}

#[derive(Debug)]
pub struct FnInfo {
    pub ret: Ty,
    pub params: Vec<ParamInfo>,
    /// Stack slots for parameters, locals and temporaries.
    pub slots: u32,
    /// Slot that keeps the caller's result address when an aggregate is returned.
    pub sret_slot: Option<u32>,
    /// A slot the code generator may use to park an address.
    pub scratch_slot: u32,
    /// Slots that must hold null on entry: the buffer pointers of
    /// temporaries, which may be freed without ever having been filled.
    pub zero_slots: Vec<u32>,
}

#[derive(Debug)]
pub struct Program<'a> {
    pub structs: Vec<StructInfo<'a>>,
    /// Element type of each list type.
    pub lists: Vec<Ty>,
    /// Parallel to the functions in `Ast::items`.
    pub fns: Vec<FnInfo>,
    pub main: u32,
    /// Per expression: for a name its stack slot (with `BY_REF` when the
    /// slot holds a pointer), for a field access the field's offset, for a
    /// call the function index, for an integer literal the index in `ints`,
    /// for a string literal the index in `strings`.
    pub expr_res: Vec<u32>,
    /// Per expression: its type.
    pub expr_ty: Vec<Ty>,
    /// Per expression that computes an aggregate: the first slot of the
    /// temporary that holds it. For `push`, the slot the element waits in.
    pub expr_tmp: Vec<u32>,
    /// Per expression used as the source of a store: true when its
    /// buffers are handed over instead of copied.
    pub expr_moved: Vec<bool>,
    /// Per `let`/`var`: its first slot. Per assignment that copies an
    /// owning value: a scratch area. Per `for`: the loop's hidden slots.
    pub stmt_slot: Vec<u32>,
    /// Per `for` statement: one of the `FOR_` constants.
    pub stmt_flag: Vec<u8>,
    /// Per statement: range in `drops` of the temporaries to free once
    /// its expression has been evaluated.
    pub stmt_drops: Vec<(u32, u32)>,
    /// Temporaries that own buffers: first slot and type.
    pub drops: Vec<(u32, Ty)>,
    /// Per field initializer in a struct literal: the field's offset.
    pub init_offset: Vec<u32>,
    pub ints: Vec<i64>,
    /// Bytes of all string literals, escapes already decoded.
    pub data: Vec<u8>,
    /// Per string literal: offset in `data` and length.
    pub strings: Vec<(u32, u32)>,
}

impl Program<'_> {
    pub fn size_of(&self, ty: Ty) -> u32 {
        size_of(&self.structs, ty)
    }

    /// The lists a value of this type owns: slot offset and element size
    /// in bytes. Empty for plain data.
    pub fn heap_fields(&self, ty: Ty) -> Vec<(u32, u32)> {
        heap_fields(&self.structs, &self.lists, ty)
    }

    pub fn needs_drop(&self, ty: Ty) -> bool {
        needs_drop(&self.structs, ty)
    }
}

fn size_of(structs: &[StructInfo], ty: Ty) -> u32 {
    match ty {
        Ty::I64 | Ty::Bool => 1,
        Ty::Str => 2,
        Ty::List(_) => 3,
        Ty::Unit => 0,
        Ty::Struct(index) => structs[index as usize].size,
    }
}

fn heap_fields(structs: &[StructInfo], lists: &[Ty], ty: Ty) -> Vec<(u32, u32)> {
    match ty {
        Ty::List(index) => vec![(0, size_of(structs, lists[index as usize]) * 8)],
        Ty::Struct(index) => structs[index as usize].heap.clone(),
        _ => Vec::new(),
    }
}

fn needs_drop(structs: &[StructInfo], ty: Ty) -> bool {
    match ty {
        Ty::List(_) => true,
        Ty::Struct(index) => !structs[index as usize].heap.is_empty(),
        _ => false,
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

/// A variable or a part of one, as a range of slots.
#[derive(Clone, Copy)]
struct Place {
    root: u32,
    start: u32,
    len: u32,
    mutable: bool,
    is_param: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Holder {
    /// An argument of a call that has not finished yet.
    Call,
    /// A `for` loop that is iterating over the place.
    Loop,
}

/// A place something else currently refers to.
struct Borrow {
    place: Place,
    mutable: bool,
    holder: Holder,
}

struct Checker<'a> {
    ast: &'a Ast,
    src: &'a str,
    structs: Vec<StructInfo<'a>>,
    struct_names: HashMap<&'a str, u32>,
    /// True once struct sizes are known.
    laid_out: bool,
    lists: Vec<Ty>,
    /// List types written before struct layout: index and source offset.
    early_lists: Vec<(u32, u32)>,
    fns: Vec<FnInfo>,
    fn_names: HashMap<&'a str, u32>,
    expr_res: Vec<u32>,
    expr_ty: Vec<Ty>,
    expr_tmp: Vec<u32>,
    expr_moved: Vec<bool>,
    stmt_slot: Vec<u32>,
    stmt_flag: Vec<u8>,
    stmt_drops: Vec<(u32, u32)>,
    drops: Vec<(u32, Ty)>,
    init_offset: Vec<u32>,
    ints: Vec<i64>,
    data: Vec<u8>,
    strings: Vec<(u32, u32)>,
    locals: Vec<Local<'a>>,
    borrows: Vec<Borrow>,
    /// Owning temporaries of the statement being checked that nothing
    /// has taken over yet: expression, first slot, type.
    pending: Vec<(ExprId, u32, Ty)>,
    zero_slots: Vec<u32>,
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
        laid_out: false,
        lists: Vec::new(),
        early_lists: Vec::new(),
        fns: Vec::new(),
        fn_names: HashMap::new(),
        expr_res: vec![0; ast.exprs.len()],
        expr_ty: vec![Ty::Unit; ast.exprs.len()],
        expr_tmp: vec![0; ast.exprs.len()],
        expr_moved: vec![false; ast.exprs.len()],
        stmt_slot: vec![0; ast.stmts.len()],
        stmt_flag: vec![0; ast.stmts.len()],
        stmt_drops: vec![(0, 0); ast.stmts.len()],
        drops: Vec::new(),
        init_offset: vec![0; ast.field_inits.len()],
        ints: Vec::new(),
        data: Vec::new(),
        strings: Vec::new(),
        locals: Vec::new(),
        borrows: Vec::new(),
        pending: Vec::new(),
        zero_slots: Vec::new(),
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
        lists: c.lists,
        fns: c.fns,
        main,
        expr_res: c.expr_res,
        expr_ty: c.expr_ty,
        expr_tmp: c.expr_tmp,
        expr_moved: c.expr_moved,
        stmt_slot: c.stmt_slot,
        stmt_flag: c.stmt_flag,
        stmt_drops: c.stmt_drops,
        drops: c.drops,
        init_offset: c.init_offset,
        ints: c.ints,
        data: c.data,
        strings: c.strings,
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
            Ty::Str => "str".to_string(),
            Ty::Unit => "no value".to_string(),
            Ty::Struct(index) => self.structs[index as usize].name.to_string(),
            Ty::List(index) => format!("List[{}]", self.ty_name(self.lists[index as usize])),
        }
    }

    fn size_of(&self, ty: Ty) -> u32 {
        size_of(&self.structs, ty)
    }

    fn needs_drop(&self, ty: Ty) -> bool {
        needs_drop(&self.structs, ty)
    }

    fn list_of(&mut self, elem: Ty) -> Ty {
        let index = match self.lists.iter().position(|&known| known == elem) {
            Some(index) => index,
            None => {
                self.lists.push(elem);
                self.lists.len() - 1
            }
        };
        Ty::List(index as u32)
    }

    /// For now a list's elements must be plain data, so that copying and
    /// freeing a list never has to visit its elements.
    fn check_element(&self, elem: Ty, at: u32) -> SResult<()> {
        match elem {
            Ty::List(_) => err(at, "lists of lists are not supported by `azol build` yet"),
            Ty::Struct(_) if self.needs_drop(elem) => err(
                at,
                format!("a list of `{}` is not supported yet, because `{}` contains a list", self.ty_name(elem), self.ty_name(elem)),
            ),
            _ => Ok(()),
        }
    }

    fn ty(&mut self, id: TypeId) -> SResult<Ty> {
        let ty = self.ast.types[id as usize];
        let name = ty.name.text(self.src);
        if name == "List" {
            let args = self.ast.list(ty.args);
            if args.len() != 1 {
                return err(ty.name.start, "`List` takes one element type, as in `List[i64]`");
            }
            let elem = self.ty(args[0])?;
            let list = self.list_of(elem);
            if self.laid_out {
                self.check_element(elem, ty.name.start)?;
            } else if let Ty::List(index) = list {
                self.early_lists.push((index, ty.name.start));
            }
            return Ok(list);
        }
        if ty.args.len > 0 {
            return err(ty.name.start, "generic types other than `List` are not supported by `azol build` yet");
        }
        match name {
            "i64" => Ok(Ty::I64),
            "bool" => Ok(Ty::Bool),
            "str" => Ok(Ty::Str),
            _ => match self.struct_names.get(name) {
                Some(&index) => Ok(Ty::Struct(index)),
                None => err(ty.name.start, format!("unknown type `{name}`; the built-in types so far are `i64`, `bool`, `str` and `List`")),
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
                    if matches!(text, "i64" | "bool" | "str" | "List") {
                        return err(name.start, format!("`{text}` is a built-in type and cannot be redefined"));
                    }
                    let index = self.structs.len() as u32;
                    if self.struct_names.insert(text, index).is_some() {
                        return err(name.start, format!("struct `{text}` is defined more than once"));
                    }
                    self.structs.push(StructInfo { name: text, size: 0, fields: Vec::new(), heap: Vec::new() });
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
        // With every size known, find the lists each struct owns.
        for index in 0..self.structs.len() {
            self.structs[index].heap = self.collect_heap(Ty::Struct(index as u32));
        }
        self.laid_out = true;
        for &(list, at) in &self.early_lists {
            self.check_element(self.lists[list as usize], at)?;
        }
        Ok(())
    }

    fn collect_heap(&self, ty: Ty) -> Vec<(u32, u32)> {
        match ty {
            Ty::List(index) => vec![(0, self.size_of(self.lists[index as usize]) * 8)],
            Ty::Struct(index) => {
                let mut out = Vec::new();
                for field in &self.structs[index as usize].fields {
                    for (offset, bytes) in self.collect_heap(field.ty) {
                        out.push((field.offset + offset, bytes));
                    }
                }
                out
            }
            _ => Vec::new(),
        }
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
                other => self.size_of(other),
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
                infos.push(ParamInfo { ty, by_ref: param.mutable || ty.is_aggregate(), mutable: param.mutable });
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
            self.fns.push(FnInfo { ret, params: infos, slots: 0, sret_slot: None, scratch_slot: 0, zero_slots: Vec::new() });
        }
        Ok(())
    }

    // ---- functions and statements --------------------------------------

    fn function(&mut self, index: u32, name: Span, params: List, body: List) -> SResult<()> {
        self.locals.clear();
        self.borrows.clear();
        self.pending.clear();
        self.zero_slots.clear();
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
        if self.ret.is_aggregate() {
            self.fns[index as usize].sret_slot = Some(self.alloc(1, name.start)?);
        }
        self.fns[index as usize].scratch_slot = self.alloc(1, name.start)?;
        self.block(body)?;
        if self.ret != Ty::Unit && !self.always_returns(body) {
            return err(
                name.start,
                format!("function `{}` must return a value of type `{}` on every path", name.text(self.src), self.ty_name(self.ret)),
            );
        }
        self.fns[index as usize].slots = self.next_slot;
        self.fns[index as usize].zero_slots = std::mem::take(&mut self.zero_slots);
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

    /// Registers an owning temporary created by `id`.
    fn temp(&mut self, id: ExprId, ty: Ty) -> SResult<u32> {
        let slot = self.alloc(self.size_of(ty), self.ast.expr_pos[id as usize])?;
        if self.needs_drop(ty) {
            self.pending.push((id, slot, ty));
        }
        Ok(slot)
    }

    /// Called where the value of `id` is stored somewhere that will own
    /// it. A temporary hands its buffers over; anything else is copied.
    fn take(&mut self, id: ExprId) -> bool {
        match self.pending.iter().position(|&(expr, ..)| expr == id) {
            Some(at) => {
                self.pending.remove(at);
                self.expr_moved[id as usize] = true;
                true
            }
            None => false,
        }
    }

    /// Ends the expression of statement `id`: the temporaries nothing
    /// took over are freed by the statement.
    fn finish(&mut self, id: StmtId) {
        let start = self.drops.len() as u32;
        for (_, slot, ty) in std::mem::take(&mut self.pending) {
            for (offset, _) in heap_fields(&self.structs, &self.lists, ty) {
                self.zero_slots.push(slot + offset);
            }
            self.drops.push((slot, ty));
        }
        self.stmt_drops[id as usize] = (start, self.drops.len() as u32 - start);
    }

    fn stmt(&mut self, id: StmtId) -> SResult<()> {
        let pos = self.ast.stmt_pos[id as usize];
        match self.ast.stmts[id as usize] {
            Stmt::Let { mutable, name, ty, value } => {
                let declared = match ty {
                    Some(ty) => Some(self.ty(ty)?),
                    None => None,
                };
                let value_ty = self.expr_hint(value, declared)?;
                if value_ty == Ty::Unit {
                    return err(self.ast.expr_pos[value as usize], "this expression has no value to store");
                }
                if let Some(declared) = declared {
                    if declared != value_ty {
                        return err(
                            self.ast.expr_pos[value as usize],
                            format!("expected `{}` but this is `{}`", self.ty_name(declared), self.ty_name(value_ty)),
                        );
                    }
                }
                self.take(value);
                self.finish(id);
                let slot = self.alloc(self.size_of(value_ty), name.start)?;
                self.locals.push(Local { name: name.text(self.src), slot, ty: value_ty, mutable, by_ref: false, is_param: false });
                self.stmt_slot[id as usize] = slot;
            }
            Stmt::Assign { target, value } => {
                let target_ty = self.expr(target)?;
                let Some(place) = self.place(target) else {
                    return err(pos, "only a variable or a part of a variable can be assigned");
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
                self.check_change(target, &place)?;
                let value_ty = self.expr_hint(value, Some(target_ty))?;
                if value_ty != target_ty {
                    return err(
                        self.ast.expr_pos[value as usize],
                        format!("expected `{}` but this is `{}`", self.ty_name(target_ty), self.ty_name(value_ty)),
                    );
                }
                let owning = self.needs_drop(target_ty);
                let moved = owning && self.take(value);
                if target_ty.is_aggregate() && !moved && (owning || self.contains_index(target)) {
                    // The value is copied aside first. That lets `a = a`
                    // read the old value before it is freed, and keeps the
                    // copy safe while a list index on the left is computed.
                    self.stmt_slot[id as usize] = self.alloc(self.size_of(target_ty), pos)?;
                }
                self.finish(id);
            }
            Stmt::Expr(expr) => {
                self.expr(expr)?;
                self.finish(id);
            }
            Stmt::Return(value) => {
                let value_ty = match value {
                    Some(value) => self.expr_hint(value, Some(self.ret))?,
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
                if let Some(value) = value {
                    if self.needs_drop(value_ty) && !self.take(value) {
                        // Returning a whole local variable hands its
                        // buffers to the caller instead of copying them.
                        if let Expr::Ident(name) = self.ast.exprs[value as usize] {
                            let local = self.lookup(name.text(self.src)).expect("checked: the name resolves");
                            if !local.is_param && !local.by_ref {
                                self.expr_moved[value as usize] = true;
                            }
                        }
                    }
                }
                self.finish(id);
            }
            Stmt::If { cond, then_block, else_branch } => {
                self.condition(cond)?;
                self.finish(id);
                self.block(then_block)?;
                match else_branch {
                    Else::None => {}
                    Else::Block(body) => self.block(body)?,
                    Else::If(next) => self.stmt(next)?,
                }
            }
            Stmt::While { cond, body } => {
                self.condition(cond)?;
                self.finish(id);
                self.loop_depth += 1;
                self.block(body)?;
                self.loop_depth -= 1;
            }
            Stmt::For { index, item, iter, body } => {
                let iter_ty = self.expr(iter)?;
                let Ty::List(list) = iter_ty else {
                    return err(
                        self.ast.expr_pos[iter as usize],
                        format!("`for` needs a list to go through but this is `{}`", self.ty_name(iter_ty)),
                    );
                };
                let elem = self.lists[list as usize];
                let place = self.place(iter);
                self.stmt_flag[id as usize] = if self.take(iter) {
                    FOR_MOVED
                } else if place.is_some() {
                    FOR_IN_PLACE
                } else {
                    FOR_COPIED
                };
                self.finish(id);
                // Hidden slots: the position, then room for a list the
                // loop owns, then the loop variables.
                let base = self.alloc(4, pos)?;
                self.stmt_slot[id as usize] = base;
                let scope = self.locals.len();
                let item_slot = self.alloc(self.size_of(elem), item.start)?;
                self.locals.push(Local { name: item.text(self.src), slot: item_slot, ty: elem, mutable: false, by_ref: false, is_param: false });
                if let Some(index) = index {
                    let index_slot = self.alloc(1, index.start)?;
                    self.locals.push(Local { name: index.text(self.src), slot: index_slot, ty: Ty::I64, mutable: false, by_ref: false, is_param: false });
                }
                let borrows = self.borrows.len();
                if self.stmt_flag[id as usize] == FOR_IN_PLACE {
                    let place = place.expect("in-place iteration has a place");
                    self.borrows.push(Borrow { place, mutable: false, holder: Holder::Loop });
                }
                self.loop_depth += 1;
                self.block(body)?;
                self.loop_depth -= 1;
                self.borrows.truncate(borrows);
                self.locals.truncate(scope);
            }
            Stmt::Break | Stmt::Continue => {
                if self.loop_depth == 0 {
                    return err(pos, "`break` and `continue` can only be used inside a loop");
                }
            }
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

    /// The variable, or part of a variable, that an already checked
    /// expression names. `None` for a computed value. An element of a list
    /// counts as the whole list: which element is only known at run time.
    fn place(&self, id: ExprId) -> Option<Place> {
        match self.ast.exprs[id as usize] {
            Expr::Ident(name) => {
                let local = self.lookup(name.text(self.src))?;
                Some(Place { root: local.slot, start: 0, len: self.size_of(local.ty), mutable: local.mutable, is_param: local.is_param })
            }
            Expr::Field { base, .. } => {
                let mut place = self.place(base)?;
                if self.contains_index(base) {
                    return Some(place);
                }
                place.start += self.expr_res[id as usize];
                place.len = self.size_of(self.expr_ty[id as usize]);
                Some(place)
            }
            Expr::Index { base, .. } => self.place(base),
            _ => None,
        }
    }

    fn contains_index(&self, id: ExprId) -> bool {
        match self.ast.exprs[id as usize] {
            Expr::Index { .. } => true,
            Expr::Field { base, .. } => self.contains_index(base),
            _ => false,
        }
    }

    fn root_name(&self, id: ExprId) -> &'a str {
        match self.ast.exprs[id as usize] {
            Expr::Ident(name) => name.text(self.src),
            Expr::Field { base, .. } | Expr::Index { base, .. } => self.root_name(base),
            _ => "this value",
        }
    }

    /// Fails when `place` cannot be changed right now because something
    /// else still refers to it.
    fn check_change(&self, id: ExprId, place: &Place) -> SResult<()> {
        for other in &self.borrows {
            if overlaps(place, &other.place) {
                let name = self.root_name(id);
                return err(
                    self.ast.expr_pos[id as usize],
                    match other.holder {
                        Holder::Loop => format!("cannot change `{name}` while a loop is going through it"),
                        Holder::Call => format!("cannot change `{name}` here: it is also passed to a function in the same expression"),
                    },
                );
            }
        }
        Ok(())
    }

    /// Records that `place` is handed to a call by reference, after
    /// checking it against everything that already refers to it.
    fn borrow(&mut self, id: ExprId, place: Place, mutable: bool) -> SResult<()> {
        for other in &self.borrows {
            if (mutable || other.mutable) && overlaps(&place, &other.place) {
                let name = self.root_name(id);
                return err(
                    self.ast.expr_pos[id as usize],
                    match other.holder {
                        Holder::Loop => format!("cannot pass `{name}` as `mut` while a loop is going through it"),
                        Holder::Call => format!("`{name}` is passed twice in one call and one of the uses is `mut`; copy it into a variable first"),
                    },
                );
            }
        }
        self.borrows.push(Borrow { place, mutable, holder: Holder::Call });
        Ok(())
    }

    // ---- expressions ---------------------------------------------------

    fn expect(&mut self, expr: ExprId, want: Ty, what: &str) -> SResult<()> {
        let ty = self.expr_hint(expr, Some(want))?;
        if ty != want {
            return err(
                self.ast.expr_pos[expr as usize],
                format!("{what} needs `{}` but this is `{}`", self.ty_name(want), self.ty_name(ty)),
            );
        }
        Ok(())
    }

    fn expr(&mut self, id: ExprId) -> SResult<Ty> {
        self.expr_hint(id, None)
    }

    /// Checks an expression. `hint` is the type the surroundings expect,
    /// which is what gives `[]` its element type.
    fn expr_hint(&mut self, id: ExprId, hint: Option<Ty>) -> SResult<Ty> {
        let ty = self.expr_inner(id, hint)?;
        self.expr_ty[id as usize] = ty;
        Ok(ty)
    }

    fn expr_inner(&mut self, id: ExprId, hint: Option<Ty>) -> SResult<Ty> {
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
            Expr::Str(span) => {
                let offset = self.data.len() as u32;
                let text = span.text(self.src);
                let mut bytes = text.as_bytes()[1..text.len() - 1].iter();
                while let Some(&byte) = bytes.next() {
                    // The lexer accepted only these escapes.
                    self.data.push(if byte != b'\\' {
                        byte
                    } else {
                        match bytes.next() {
                            Some(b'n') => b'\n',
                            Some(b't') => b'\t',
                            Some(b'r') => b'\r',
                            Some(b'0') => 0,
                            Some(&other) => other,
                            None => b'\\',
                        }
                    });
                }
                self.expr_res[id as usize] = self.strings.len() as u32;
                self.strings.push((offset, self.data.len() as u32 - offset));
                self.expr_tmp[id as usize] = self.alloc(2, pos)?;
                Ok(Ty::Str)
            }
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
                            Ty::Str => return err(pos, format!("strings cannot be compared with {what} yet")),
                            Ty::List(_) => return err(pos, format!("lists cannot be compared with {what} yet")),
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
                    return err(
                        name.start,
                        if matches!(base_ty, Ty::List(_)) {
                            format!("`{}` has no field `{field_name}`; its methods are `len()`, `push(...)` and `pop()`", self.ty_name(base_ty))
                        } else {
                            format!("`{}` has no fields", self.ty_name(base_ty))
                        },
                    );
                };
                let info = &self.structs[index as usize];
                let Some(field) = info.fields.iter().find(|f| f.name == field_name) else {
                    return err(name.start, format!("struct `{}` has no field `{field_name}`", info.name));
                };
                self.expr_res[id as usize] = field.offset;
                Ok(field.ty)
            }
            Expr::Index { base, index } => {
                let base_ty = self.expr(base)?;
                let Ty::List(list) = base_ty else {
                    return err(pos, format!("only a list can be indexed but this is `{}`", self.ty_name(base_ty)));
                };
                self.expect(index, Ty::I64, "an index")?;
                Ok(self.lists[list as usize])
            }
            Expr::ArrayLit { items } => {
                let item_ids = self.ast.list(items);
                let hinted = match hint {
                    Some(Ty::List(list)) => Some(self.lists[list as usize]),
                    _ => None,
                };
                let elem = match (item_ids.first(), hinted) {
                    (_, Some(elem)) => elem,
                    (Some(&first), None) => self.expr(first)?,
                    (None, None) => {
                        return err(pos, "cannot tell what this empty list holds; give the variable a type, as in `var xs: List[i64] = []`");
                    }
                };
                if elem == Ty::Unit {
                    return err(pos, "this expression has no value to put in a list");
                }
                self.check_element(elem, pos)?;
                for &item in item_ids {
                    self.expect(item, elem, "this list's element")?;
                }
                let ty = self.list_of(elem);
                self.expr_tmp[id as usize] = self.temp(id, ty)?;
                Ok(ty)
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
                    // The new struct takes a temporary's buffers over.
                    self.take(init.value);
                    self.init_offset[init_id as usize] = offset;
                }
                if let Some(missing) = seen.iter().position(|&given| !given) {
                    let field_name = self.structs[index as usize].fields[missing].name;
                    return err(pos, format!("field `{field_name}` of `{text}` is missing"));
                }
                let ty = Ty::Struct(index);
                self.expr_tmp[id as usize] = self.temp(id, ty)?;
                Ok(ty)
            }
            Expr::Call { callee, args } => self.call(id, callee, args),
            Expr::MutArg(_) => err(pos, "`mut` can only be used on a function argument"),
            Expr::Float(_) => err(pos, "floating-point numbers are not supported by `azol build` yet"),
        }
    }

    fn call(&mut self, id: ExprId, callee: ExprId, args: List) -> SResult<Ty> {
        let pos = self.ast.expr_pos[id as usize];
        let arg_ids = self.ast.list(args);
        let name = match self.ast.exprs[callee as usize] {
            Expr::Ident(name) => name,
            Expr::Field { base, name } => return self.method(id, base, name, arg_ids),
            _ => return err(pos, "only functions can be called"),
        };
        let text = name.text(self.src);
        if self.lookup(text).is_some() {
            return err(pos, format!("`{text}` is a variable, not a function"));
        }
        if text == "print" {
            if arg_ids.len() != 1 {
                return err(pos, format!("`print` takes 1 argument but {} were given", arg_ids.len()));
            }
            let arg = arg_ids[0];
            self.expr_res[id as usize] = match self.expr(arg)? {
                Ty::I64 => BUILTIN_PRINT_I64,
                Ty::Bool => BUILTIN_PRINT_BOOL,
                Ty::Str => BUILTIN_PRINT_STR,
                other => {
                    return err(
                        self.ast.expr_pos[arg as usize],
                        format!("`print` takes an `i64`, a `bool` or a `str` but this is `{}`", self.ty_name(other)),
                    );
                }
            };
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
                        return err(arg_pos, "`mut` needs a variable or a part of a variable");
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
        if ret.is_aggregate() {
            self.expr_tmp[id as usize] = self.temp(id, ret)?;
        }
        Ok(ret)
    }

    /// A call written `value.name(...)`. Only lists have methods so far.
    fn method(&mut self, id: ExprId, base: ExprId, name: Span, args: &[u32]) -> SResult<Ty> {
        let pos = self.ast.expr_pos[id as usize];
        let base_ty = self.expr(base)?;
        let text = name.text(self.src);
        let Ty::List(list) = base_ty else {
            return err(name.start, format!("`{}` has no method `{text}`", self.ty_name(base_ty)));
        };
        let elem = self.lists[list as usize];
        let arity = |expected: usize| -> SResult<()> {
            if args.len() == expected {
                Ok(())
            } else {
                err(pos, format!("`{text}` takes {expected} argument{} but {} were given", if expected == 1 { "" } else { "s" }, args.len()))
            }
        };
        match text {
            "len" => {
                arity(0)?;
                self.expr_res[id as usize] = BUILTIN_LIST_LEN;
                Ok(Ty::I64)
            }
            "push" | "pop" => {
                arity(if text == "push" { 1 } else { 0 })?;
                let Some(place) = self.place(base) else {
                    return err(pos, format!("`{text}` changes the list, so it needs a variable or a part of a variable"));
                };
                if !place.mutable {
                    let root = self.root_name(base);
                    return err(
                        pos,
                        if place.is_param {
                            format!("cannot change `{root}`: a parameter is read-only unless it is declared `mut`")
                        } else {
                            format!("cannot change `{root}`: declare it with `var` to make it changeable")
                        },
                    );
                }
                if text == "push" {
                    self.expect(args[0], elem, "`push`")?;
                    self.check_change(base, &place)?;
                    // The element waits here while the list makes room.
                    self.expr_tmp[id as usize] = self.alloc(self.size_of(elem), pos)?;
                    self.expr_res[id as usize] = BUILTIN_LIST_PUSH;
                    Ok(Ty::Unit)
                } else {
                    self.check_change(base, &place)?;
                    if elem.is_aggregate() {
                        self.expr_tmp[id as usize] = self.alloc(self.size_of(elem), pos)?;
                    }
                    self.expr_res[id as usize] = BUILTIN_LIST_POP;
                    Ok(elem)
                }
            }
            _ => err(name.start, format!("`{}` has no method `{text}`; its methods are `len()`, `push(...)` and `pop()`", self.ty_name(base_ty))),
        }
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
        assert_eq!(program.fns[0].slots, 4);
        assert_eq!(program.main, 1);
        assert_eq!(program.ints, vec![1, 2]);
    }

    #[test]
    fn string_literals_are_decoded() {
        let src = "struct Named {\n name: str\n id: i64\n}\nfn main() {\n let a = \"hi\\n\"\n let b = \"q\\\"\\\\\\t\"\n let n = Named { name: a, id: 1 }\n print(n.name)\n}";
        let tokens = lex(src).unwrap();
        let ast = parse(src, &tokens).unwrap();
        let program = check(&ast, src).unwrap();
        assert_eq!(program.data, b"hi\nq\"\\\t");
        assert_eq!(program.strings, vec![(0, 3), (3, 4)]);
        assert_eq!(program.structs[0].size, 3);
        assert_eq!(program.structs[0].fields[1].offset, 2);
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
        assert_eq!(error(&with_point(&in_main("print(Point { x: 1, y: 2 })"))), "`print` takes an `i64`, a `bool` or a `str` but this is `Point`");
        assert_eq!(error(&in_main("print(print(1))")), "`print` takes an `i64`, a `bool` or a `str` but this is `no value`");
        assert_eq!(error(&in_main("let a = \"x\" == \"y\"")), "strings cannot be compared with `==` yet");
        assert_eq!(error(&in_main("let a = \"x\" + 1")), "`+` needs `i64` but this is `str`");
        assert_eq!(error(&in_main("let a: str = 1")), "expected `str` but this is `i64`");
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
        assert_eq!(error("struct A {\n x: Missing\n}\nfn main() {\n}"), "unknown type `Missing`; the built-in types so far are `i64`, `bool`, `str` and `List`");
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
        assert_eq!(error(&main("Point { x: 1, y: 2 }.x = 3")), "only a variable or a part of a variable can be assigned");
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
            "`mut` needs a variable or a part of a variable"
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

    const LISTS: &str = "struct Point {\n x: i64\n y: i64\n}\nstruct Bag {\n items: List[i64]\n name: str\n}\nfn total(xs: List[i64]) -> i64 {\n return 0\n}\nfn grow(xs: mut List[i64]) {\n xs.push(1)\n}\nfn make() -> List[i64] {\n return [1, 2]\n}\n";

    fn list_main(body: &str) -> String {
        format!("{LISTS}{}", in_main(body))
    }

    #[test]
    fn list_programs_that_are_accepted() {
        assert!(check_src(&list_main("var xs = [1, 2, 3]\nxs.push(4)\nxs[0] = xs[1] + xs.len()\nlet last = xs.pop()\nprint(last)")).is_ok());
        assert!(check_src(&list_main("var xs: List[i64] = []\nxs = []\nxs = make()\ngrow(mut xs)\nprint(total(xs) + total([]) + total(make()))")).is_ok());
        assert!(check_src(&list_main("let ps = [Point { x: 1, y: 2 }]\nprint(ps[0].x)\nfor i, p in ps {\nprint(i + p.y)\n}")).is_ok());
        assert!(check_src(&list_main("var b = Bag { items: [1], name: \"a\" }\nb.items.push(2)\nb.items[0] = 5\nlet c = b\nfor x in c.items {\nb.items.push(x)\n}")).is_ok());
        assert!(check_src(&list_main("let names = [\"a\", \"b\"]\nlet flags = [true, false]\nfor n in names {\nprint(n)\n}\nfor f in flags {\nprint(f)\n}")).is_ok());
        assert!(check_src(&list_main("var xs = [1]\nxs.push(xs[0])\nxs.push(xs.len())\nxs.push(xs.pop())")).is_ok());
    }

    #[test]
    fn list_type_errors() {
        assert_eq!(
            error(&list_main("let xs = []")),
            "cannot tell what this empty list holds; give the variable a type, as in `var xs: List[i64] = []`"
        );
        assert_eq!(error(&list_main("let xs = [1, true]")), "this list's element needs `i64` but this is `bool`");
        assert_eq!(error(&list_main("let xs: List[bool] = [1]")), "this list's element needs `bool` but this is `i64`");
        assert_eq!(error(&list_main("var xs = [1]\nxs.push(true)")), "`push` needs `i64` but this is `bool`");
        assert_eq!(error(&list_main("let xs = [1]\nprint(xs[true])")), "an index needs `i64` but this is `bool`");
        assert_eq!(error(&list_main("let n = 1\nprint(n[0])")), "only a list can be indexed but this is `i64`");
        assert_eq!(error(&list_main("let xs = [1]\nprint(xs.size())")), "`List[i64]` has no method `size`; its methods are `len()`, `push(...)` and `pop()`");
        assert_eq!(error(&list_main("let xs = [1]\nprint(xs.len)")), "`List[i64]` has no field `len`; its methods are `len()`, `push(...)` and `pop()`");
        assert_eq!(error(&list_main("let xs = [1]\nprint(xs.len(1))")), "`len` takes 0 arguments but 1 were given");
        assert_eq!(error(&list_main("let n = 1\nn.push(2)")), "`i64` has no method `push`");
        assert_eq!(error(&list_main("let xs = [1]\nif xs == xs {\n}")), "lists cannot be compared with `==` yet");
        assert_eq!(error(&list_main("for x in 5 {\n}")), "`for` needs a list to go through but this is `i64`");
        assert_eq!(error(&list_main("let xs = [[1]]")), "lists of lists are not supported by `azol build` yet");
        assert_eq!(error(&list_main("let bs = [Bag { items: [1], name: \"a\" }]")), "a list of `Bag` is not supported yet, because `Bag` contains a list");
        assert_eq!(error("struct A {\n xs: List[B]\n}\nstruct B {\n ys: List[i64]\n}\nfn main() {\n}"), "a list of `B` is not supported yet, because `B` contains a list");
        assert_eq!(error("fn f(xs: List) {\n}\nfn main() {\n}"), "`List` takes one element type, as in `List[i64]`");
        assert_eq!(error("fn f(xs: Map[i64]) {\n}\nfn main() {\n}"), "generic types other than `List` are not supported by `azol build` yet");
    }

    #[test]
    fn lists_can_only_be_changed_through_changeable_places() {
        assert_eq!(error(&list_main("let xs = [1]\nxs.push(2)")), "cannot change `xs`: declare it with `var` to make it changeable");
        assert_eq!(error(&list_main("let xs = [1]\nxs[0] = 2")), "cannot change `xs`: declare it with `var` to make it changeable");
        assert_eq!(error(&list_main("let xs = [1]\nlet x = xs.pop()")), "cannot change `xs`: declare it with `var` to make it changeable");
        assert_eq!(error(&list_main("make().push(1)")), "`push` changes the list, so it needs a variable or a part of a variable");
        assert_eq!(
            error(&format!("{LISTS}fn f(xs: List[i64]) {{\n xs.push(1)\n}}\nfn main() {{\n}}")),
            "cannot change `xs`: a parameter is read-only unless it is declared `mut`"
        );
        assert_eq!(error(&list_main("let xs = [1]\ngrow(mut xs)")), "cannot pass `xs` as `mut`: declare it with `var` to make it changeable");
        assert_eq!(error(&list_main("for x in [1, 2] {\nx = 3\n}")), "cannot change `x`: declare it with `var` to make it changeable");
    }

    #[test]
    fn a_list_cannot_change_while_something_refers_to_it() {
        let going = "while a loop is going through it";
        assert_eq!(error(&list_main("var xs = [1]\nfor x in xs {\nxs.push(x)\n}")), format!("cannot change `xs` {going}"));
        assert_eq!(error(&list_main("var xs = [1]\nfor x in xs {\nxs[0] = x\n}")), format!("cannot change `xs` {going}"));
        assert_eq!(error(&list_main("var xs = [1]\nfor x in xs {\nxs = [2]\n}")), format!("cannot change `xs` {going}"));
        assert_eq!(error(&list_main("var xs = [1]\nfor x in xs {\nlet y = xs.pop()\n}")), format!("cannot change `xs` {going}"));
        assert_eq!(error(&list_main("var xs = [1]\nfor x in xs {\ngrow(mut xs)\n}")), format!("cannot pass `xs` as `mut` {going}"));
        assert_eq!(error(&list_main("var b = Bag { items: [1], name: \"a\" }\nfor x in b.items {\nb = Bag { items: [], name: \"b\" }\n}")), format!("cannot change `b` {going}"));
        assert_eq!(error(&list_main("var xs = [1]\nfor x in xs {\nif x > 0 {\nwhile true {\nxs.push(1)\n}\n}\n}")), format!("cannot change `xs` {going}"));
        // Reading it, changing another list, or changing it after the loop is fine.
        assert!(check_src(&list_main("var xs = [1]\nvar ys = [2]\nfor x in xs {\nys.push(x + xs.len() + total(xs))\n}\nxs.push(3)")).is_ok());
        assert!(check_src(&list_main("var b = Bag { items: [1], name: \"a\" }\nfor x in b.items {\nb.name = \"c\"\n}")).is_ok());
        assert_eq!(
            error(&format!("{LISTS}fn both(xs: List[i64], n: i64) -> i64 {{\n return n\n}}\n{}", in_main("var xs = [1]\nprint(both(xs, xs.pop()))"))),
            "cannot change `xs` here: it is also passed to a function in the same expression"
        );
        assert_eq!(
            error(&format!("{LISTS}fn both(n: mut i64, xs: List[i64]) {{\n}}\n{}", in_main("var xs = [1]\nboth(mut xs[0], xs)"))),
            "`xs` is passed twice in one call and one of the uses is `mut`; copy it into a variable first"
        );
    }

    #[test]
    fn ownership_decisions() {
        let src = list_main("let a = make()\nlet b = a\nvar c = [1]\nc = a\nc = make()\nlet d = Bag { items: a, name: \"n\" }\nlet e = Bag { items: [7], name: \"n\" }\nprint(total(make()))");
        let tokens = lex(&src).unwrap();
        let ast = parse(&src, &tokens).unwrap();
        let program = check(&ast, &src).unwrap();
        // One move per temporary that is stored: make() twice, [1], [7],
        // the two struct literals, and `[1, 2]` returned inside `make`.
        // Copies of `a` are not moves.
        assert_eq!(program.expr_moved.iter().filter(|&&moved| moved).count(), 7);
        // Only `total(make())` leaves a temporary for its statement to free.
        assert_eq!(program.drops.len(), 1);
        let main = &program.fns[program.main as usize];
        assert_eq!(main.zero_slots.len(), 1);
    }

    #[test]
    fn features_not_built_yet_are_reported_clearly() {
        assert_eq!(error("enum E {\n A\n}\nfn main() {\n}"), "enums are not supported by `azol build` yet");
        assert_eq!(error(&in_main("let a = 1.5")), "floating-point numbers are not supported by `azol build` yet");
        assert_eq!(error("fn f(a: i32) {\n}\nfn main() {\n}"), "unknown type `i32`; the built-in types so far are `i64`, `bool`, `str` and `List`");
    }
}
