// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Name resolution, type checking and the memory safety analysis, for
//! the part of the language `azol build` compiles: `i64`, `bool`, `str`,
//! struct, enum and `List` values, functions with read-only and `mut`
//! parameters, `let`/`var`, assignment, `if`, `while`, `for`, `match`,
//! `return`.
//!
//! A struct, enum or function with type parameters is a declaration
//! that becomes one concrete type or function per combination of types
//! it is used with. A generic function's body is checked once for each.
//!
//! Values have value semantics. Aggregates are passed by reference for
//! speed, but the callee cannot change one unless the parameter is `mut`,
//! and a place can never be changed while something else still refers to
//! it: another argument of the same call, a loop iterating over it, or
//! a `match` looking inside it.
//! Together these make a reference indistinguishable from a private copy.
//!
//! A list owns a heap buffer. Every buffer has exactly one owner: a
//! variable, or a temporary that lives until the end of its statement.
//! This pass decides, for each value that is stored somewhere, whether it
//! is moved (a temporary handing its buffer over) or copied (a variable
//! being duplicated), and which temporaries each statement must free.

use crate::ast::*;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemaError {
    pub message: String,
    pub offset: u32,
    /// The expression the error is about, when it is about one whole
    /// expression; the report then covers all of it.
    pub expr: Option<ExprId>,
    /// An edit that makes this error go away, when one is certain.
    pub fix: Option<Fix>,
}

/// A mechanical repair for an error. Some are given as plain text
/// replacements; the others name a place by its first token and are
/// turned into replacements by whoever has the tokens (`diag`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fix {
    /// Put `text` in place of the source text from `start` to `end`.
    Replace { start: u32, end: u32, text: String },
    /// Add these arms at the end of the `match` statement that starts at
    /// `stmt`.
    MatchArms { stmt: u32, arms: Vec<String> },
    /// Insert `text` right after the name that starts at `at`.
    AfterName { at: u32, text: String },
    /// Remove the empty `()` that follows the name that starts at `at`.
    EmptyParens { at: u32 },
}

impl SemaError {
    fn with_fix(mut self, fix: Option<Fix>) -> SemaError {
        self.fix = fix;
        self
    }
}

type SResult<T> = Result<T, SemaError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ty {
    I64,
    Bool,
    Unit,
    /// Text: pointer, length in bytes, capacity. Capacity 0 means the
    /// bytes live in the executable (a literal) and are not owned.
    Str,
    /// Index into `Program::structs`.
    Struct(u32),
    /// A growable array: pointer, length, capacity. The index selects the
    /// element type in `Program::lists`.
    List(u32),
    /// Index into `Program::enums`: a tag, then room for the largest
    /// variant. `Option[i64]` and `Option[str]` are different entries.
    Enum(u32),
}

impl Ty {
    /// Values that live in memory and are handed around by address.
    pub fn is_aggregate(self) -> bool {
        matches!(self, Ty::Str | Ty::Struct(_) | Ty::List(_) | Ty::Enum(_))
    }
}

/// Built-in functions and methods, as call resolutions.
pub const BUILTIN_PRINT_I64: u32 = u32::MAX;
pub const BUILTIN_PRINT_BOOL: u32 = u32::MAX - 1;
pub const BUILTIN_PRINT_STR: u32 = u32::MAX - 2;
pub const BUILTIN_LIST_PUSH: u32 = u32::MAX - 3;
pub const BUILTIN_LIST_POP: u32 = u32::MAX - 4;
/// `len()` of a list or of a string.
pub const BUILTIN_LEN: u32 = u32::MAX - 5;
pub const BUILTIN_STR_FROM_I64: u32 = u32::MAX - 6;
pub const BUILTIN_STR_FROM_BOOL: u32 = u32::MAX - 7;
/// A call that builds an enum value; `expr_aux` holds the variant.
pub const BUILTIN_VARIANT: u32 = u32::MAX - 8;
pub const BUILTIN_ARGS: u32 = u32::MAX - 9;
pub const BUILTIN_READ_FILE: u32 = u32::MAX - 10;
pub const BUILTIN_WRITE_FILE: u32 = u32::MAX - 11;
pub const BUILTIN_CHR: u32 = u32::MAX - 12;
pub const BUILTIN_EXIT: u32 = u32::MAX - 13;
pub const BUILTIN_EPRINT: u32 = u32::MAX - 14;
/// `slice(start, end)` of a string.
pub const BUILTIN_STR_SLICE: u32 = u32::MAX - 15;
pub const BUILTIN_SET_EXECUTABLE: u32 = u32::MAX - 16;
/// `push(byte)` on a string: one more byte at its end.
pub const BUILTIN_STR_PUSH: u32 = u32::MAX - 17;
/// `set(index, byte)` on a string: one byte replaced.
pub const BUILTIN_STR_SET: u32 = u32::MAX - 18;
/// `remove_file(path)`.
pub const BUILTIN_REMOVE_FILE: u32 = u32::MAX - 19;
/// `rename_file(from, to)`.
pub const BUILTIN_RENAME_FILE: u32 = u32::MAX - 20;
/// `clock()`: nanoseconds on a clock that only moves forward.
pub const BUILTIN_CLOCK: u32 = u32::MAX - 21;
/// `is_utf8()` on a string: whether its bytes are well-formed UTF-8.
pub const BUILTIN_STR_IS_UTF8: u32 = u32::MAX - 22;
/// Resolutions at or above this value are built-ins.
pub const FIRST_BUILTIN: u32 = u32::MAX - 22;

/// In `expr_aux` of a name that is a constant: the expression then
/// stands for a literal of that kind. For a number `expr_res` is its
/// index in `ints`, for a string its index in `strings` (and `expr_tmp`
/// its temporary), exactly as for a literal written in place.
pub const CONST_INT: u32 = u32::MAX;
pub const CONST_TRUE: u32 = u32::MAX - 1;
pub const CONST_FALSE: u32 = u32::MAX - 2;
pub const CONST_STR: u32 = u32::MAX - 3;

/// Names of the built-in functions; a program cannot define these.
const BUILTIN_FUNCTIONS: [&str; 13] = [
    "print",
    "eprint",
    "str",
    "chr",
    "args",
    "read_file",
    "write_file",
    "set_executable",
    "exit",
    "remove_file",
    "rename_file",
    "clock",
    "embed",
];
/// Where the built-in `Option` and `Result` sit among the enum declarations.
const OPTION: u32 = 0;
const RESULT: u32 = 1;
/// Set in the resolution of a name whose stack slot holds a pointer.
pub const BY_REF: u32 = 1 << 31;
/// The resolution of a name that is a variant without fields, such as
/// `None`; `expr_aux` holds the variant.
pub const VARIANT: u32 = 1 << 30;

/// Most parameters a function can take: they travel in registers x0-x7.
pub const MAX_PARAMS: usize = 8;
/// Most stack slots a function can use. A frame of this size is still
/// smaller than the 16 KB guard page below the stack, so running out of
/// stack always stops the program instead of overwriting memory.
pub const MAX_SLOTS: u32 = 2000;
/// Largest struct or enum, in slots: keeps every field reachable with
/// one add instruction.
pub const MAX_SIZE: u32 = 500;

/// The registers variables can live in: x19 to x28, which every function
/// hands back unchanged to its caller.
pub const FIRST_REGISTER: u8 = 19;
pub const REGISTERS: u8 = 10;
/// How much a variable must be used to get a register.
const MIN_REGISTER_USES: u32 = 3;

/// Most variants an enum can have: a tag must fit in one compare.
pub const MAX_VARIANTS: usize = 1000;

/// How a `for` loop or a `match` reaches the value it works on: where
/// it lives, in a variable that cannot change meanwhile.
pub const IN_PLACE: u8 = 0;
/// The value is a temporary; the statement takes it over and frees it.
pub const MOVED: u8 = 1;
/// The value is part of a temporary; the statement works on its own copy.
pub const COPIED: u8 = 2;

/// In `stmt_flag` of an assignment `s = s + more` on strings: the new
/// text is added to `s` where it is instead of building a third string.
pub const APPEND: u8 = 1;

/// A pattern that matches anything and binds nothing.
pub const PAT_ANY: u8 = 0;
/// A pattern that matches anything and gives it a name.
pub const PAT_BIND: u8 = 1;
/// A pattern that matches one variant of an enum.
pub const PAT_VARIANT: u8 = 2;
/// A pattern that matches one number, `bool` or string.
pub const PAT_LITERAL: u8 = 3;

/// What the checker found out about a pattern.
#[derive(Debug, Clone, Copy, Default)]
pub struct PatInfo {
    /// One of the `PAT_` constants.
    pub kind: u8,
    /// For a variant its index. For a literal the index in `ints` or
    /// `strings`, or 0/1 for a `bool`. For a name inside a variant
    /// pattern, the offset of the field it names.
    pub res: u32,
    /// For a name, the slot that holds what it is bound to. For a string
    /// literal, three slots where the string is built to be compared.
    pub slot: u32,
}

/// One struct type. A generic struct has one entry per combination of
/// type arguments that the program uses.
#[derive(Debug)]
pub struct StructInfo<'a> {
    /// As written in messages: `Point`, `Pair[i64, str]`.
    pub name: String,
    /// The declaration this type was made from.
    pub template: u32,
    pub args: Vec<Ty>,
    /// Size in 8-byte slots.
    pub size: u32,
    pub fields: Vec<FieldInfo<'a>>,
    /// The lists, strings and owning enums directly inside the struct:
    /// slot offset and type. What those own in turn is not listed here.
    pub heap: Vec<(u32, Ty)>,
}

/// One enum type. A generic enum has one entry per combination of type
/// arguments that the program uses.
#[derive(Debug)]
pub struct EnumInfo {
    /// As written in messages: `Shape`, `Option[i64]`.
    pub name: String,
    /// The declaration this type was made from.
    pub template: u32,
    pub args: Vec<Ty>,
    /// Size in 8-byte slots: the tag plus the largest variant.
    pub size: u32,
    pub variants: Vec<VariantInfo>,
    /// True when some variant holds a list, a string or an owning enum.
    pub owning: bool,
}

/// A value of variant number `n` has tag `n + 1`; tag 0 marks a
/// temporary that was never filled.
#[derive(Debug)]
pub struct VariantInfo {
    /// Type and slot offset (from the start of the enum) of each field.
    pub fields: Vec<(Ty, u32)>,
    /// What the fields own, like `StructInfo::heap`.
    pub heap: Vec<(u32, Ty)>,
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

/// Where a function's entries sit in the per-node tables of `Program`.
/// A node's entry is at its id plus the shift for its kind. The shift is
/// zero for an ordinary function. A generic function is checked once per
/// combination of types it is used with, and each of those has entries
/// of its own, added at the end of the tables.
#[derive(Debug, Clone, Copy, Default)]
pub struct Shift {
    pub expr: u32,
    pub stmt: u32,
    pub pat: u32,
    pub init: u32,
}

impl Shift {
    #[inline(always)]
    pub fn e(self, id: ExprId) -> usize {
        id.wrapping_add(self.expr) as usize
    }

    #[inline(always)]
    pub fn s(self, id: StmtId) -> usize {
        id.wrapping_add(self.stmt) as usize
    }

    #[inline(always)]
    pub fn p(self, id: PatId) -> usize {
        id.wrapping_add(self.pat) as usize
    }

    #[inline(always)]
    pub fn i(self, id: u32) -> usize {
        id.wrapping_add(self.init) as usize
    }
}

#[derive(Debug)]
pub struct FnInfo {
    /// Index in `Ast::items` of the function's source.
    pub item: u32,
    pub shift: Shift,
    /// For a use of a generic function: which one, and the types its
    /// type parameters stand for here.
    pub generic: Option<u32>,
    pub type_args: Vec<Ty>,
    /// How many uses of generic functions led to this one, each made
    /// while checking the one before.
    pub depth: u32,
    /// For a use of a generic function: where in the program it was
    /// first asked for.
    pub origin: u32,
    pub ret: Ty,
    pub params: Vec<ParamInfo>,
    /// Stack slots for parameters, locals and temporaries.
    pub slots: u32,
    /// Slot that keeps the caller's result address when an aggregate is returned.
    pub sret_slot: Option<u32>,
    /// A slot the code generator may use to park an address.
    pub scratch_slot: u32,
    /// Variables that live in a register instead of the frame: their
    /// slot and the register (x19 to x28). Only numbers and `bool`s whose
    /// address is never needed are chosen, the most used first.
    pub registers: Vec<(u32, u8)>,
    /// Slots that must hold zero on entry: the capacities of temporaries,
    /// which may be freed without ever having been filled.
    pub zero_slots: Vec<u32>,
}

#[derive(Debug)]
pub struct Program<'a> {
    pub structs: Vec<StructInfo<'a>>,
    pub enums: Vec<EnumInfo>,
    /// Element type of each list type.
    pub lists: Vec<Ty>,
    /// Every function to generate code for: the ordinary ones in source
    /// order, then each use of a generic function with new types.
    pub fns: Vec<FnInfo>,
    pub main: u32,
    /// Per expression: for a name its stack slot (with `BY_REF` when the
    /// slot holds a pointer), for a field access the field's offset, for a
    /// call the function index, for an integer literal the index in `ints`,
    /// for a string literal the index in `strings`.
    pub expr_res: Vec<u32>,
    /// Per expression: its type.
    pub expr_ty: Vec<Ty>,
    /// Per expression that builds an enum value: the variant's index.
    pub expr_aux: Vec<u32>,
    /// Per expression that computes an aggregate: the first slot of the
    /// temporary that holds it. For `push`, the slot the element waits in.
    pub expr_tmp: Vec<u32>,
    /// Per expression used as the source of a store: true when its
    /// buffers are handed over instead of copied. That is a temporary,
    /// or a variable that is not used again; the variable is then left
    /// owning nothing.
    pub expr_moved: Vec<bool>,
    /// Per `let`/`var`: its first slot. Per assignment that copies an
    /// owning value: a scratch area. Per `for` and `match`: the
    /// statement's hidden slots.
    pub stmt_slot: Vec<u32>,
    /// Per `for` and `match`: `IN_PLACE`, `MOVED` or `COPIED`. Per
    /// assignment: `APPEND` or 0.
    pub stmt_flag: Vec<u8>,
    /// Parallel to `Ast::patterns`.
    pub pats: Vec<PatInfo>,
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
    /// Expression `id` as the code generators should see it: a name
    /// that is a constant reads as the literal it stands for.
    #[inline(always)]
    pub fn expr(&self, ast: &Ast, shift: Shift, id: ExprId) -> Expr {
        let expr = ast.exprs[id as usize];
        if let Expr::Ident(span) = expr {
            match self.expr_aux[shift.e(id)] {
                CONST_INT => return Expr::Int(span),
                CONST_TRUE => return Expr::Bool(true),
                CONST_FALSE => return Expr::Bool(false),
                CONST_STR => return Expr::Str(span),
                _ => {}
            }
        }
        expr
    }

    pub fn size_of(&self, ty: Ty) -> u32 {
        size_of(&self.structs, &self.enums, ty)
    }

    /// The lists, strings and owning enums directly inside a value of
    /// this type: slot offset and type. Empty for plain data.
    pub fn owned(&self, ty: Ty) -> Vec<(u32, Ty)> {
        owned(&self.structs, &self.enums, ty)
    }

    pub fn needs_drop(&self, ty: Ty) -> bool {
        needs_drop(&self.structs, &self.enums, ty)
    }
}

/// Inside a list, a string or an owning enum, the slot that says "this
/// owns nothing" when it is zero: the capacity, or the enum's tag.
pub fn idle_slot(ty: Ty) -> u32 {
    match ty {
        Ty::Enum(_) => 0,
        _ => 2,
    }
}

fn size_of(structs: &[StructInfo], enums: &[EnumInfo], ty: Ty) -> u32 {
    match ty {
        Ty::I64 | Ty::Bool => 1,
        Ty::Str | Ty::List(_) => 3,
        Ty::Unit => 0,
        Ty::Struct(index) => structs[index as usize].size,
        Ty::Enum(index) => enums[index as usize].size,
    }
}

fn owned(structs: &[StructInfo], enums: &[EnumInfo], ty: Ty) -> Vec<(u32, Ty)> {
    match ty {
        Ty::Str | Ty::List(_) => vec![(0, ty)],
        Ty::Struct(index) => structs[index as usize].heap.clone(),
        Ty::Enum(index) if enums[index as usize].owning => vec![(0, ty)],
        _ => Vec::new(),
    }
}

fn needs_drop(structs: &[StructInfo], enums: &[EnumInfo], ty: Ty) -> bool {
    match ty {
        Ty::Str | Ty::List(_) => true,
        Ty::Struct(index) => !structs[index as usize].heap.is_empty(),
        Ty::Enum(index) => enums[index as usize].owning,
        _ => false,
    }
}

/// A type as written in a declaration, where it may mention the
/// declaration's type parameters.
#[derive(Clone)]
enum Shape {
    /// The declaration's own type parameter with this number.
    Param(u32),
    /// `i64`, `bool`, `str`, or a struct without type parameters once
    /// its one type exists.
    Known(Ty),
    List(Box<Shape>),
    /// A struct declaration and its type arguments.
    Struct(u32, Vec<Shape>),
    /// An enum declaration and its type arguments.
    Enum(u32, Vec<Shape>),
}

/// A struct declaration. Each use with concrete type arguments becomes
/// a `StructInfo`.
struct StructTemplate<'a> {
    name: &'a str,
    pos: u32,
    params: Vec<&'a str>,
    /// Shared, so that making a type from the declaration does not copy it.
    fields: Rc<Vec<(&'a str, Shape)>>,
    /// For a struct without type parameters, its one `StructInfo`.
    only: Option<u32>,
}

/// A function with type parameters. It is checked once for each
/// combination of types it is called with, like a separate function.
struct GenericFn<'a> {
    item: u32,
    name: &'a str,
    type_params: Vec<&'a str>,
    /// Type and `mut` of each parameter.
    params: Vec<(Shape, bool)>,
    ret: Option<Shape>,
}

/// What an argument of a call is checked against.
enum Want<'s> {
    Known(Ty),
    /// The argument has been checked already and has this type.
    Checked(Ty),
    /// A parameter of a generic function: its type may mention type
    /// parameters that this very argument settles.
    Infer(&'s Shape),
}

/// An enum declaration. Each use with concrete type arguments becomes
/// an `EnumInfo`.
struct Template<'a> {
    name: &'a str,
    /// Where it is declared; 0 for the built-in ones.
    pos: u32,
    params: Vec<&'a str>,
    variants: Vec<(&'a str, Vec<Shape>)>,
    builtin: bool,
}

struct Local<'a> {
    name: &'a str,
    slot: u32,
    ty: Ty,
    mutable: bool,
    by_ref: bool,
    is_param: bool,
    /// The variable of a `for` loop or a name bound by a pattern: it
    /// looks at a value that lives somewhere else.
    is_view: bool,
    /// False for a view of data another value owns: a parameter, a loop
    /// variable or a pattern binding.
    owns: bool,
    /// How many loops enclosed the declaration.
    depth: u32,
    /// The latest place where the variable was stored somewhere whole and
    /// could have handed its buffers over instead of being copied. It
    /// stands only if the variable is never mentioned again.
    candidate: Option<ExprId>,
    /// A number or `bool` declared with `let`/`var` or passed by value:
    /// it may live in a register instead of the frame.
    scalar: bool,
    /// How much the variable is used; uses inside loops count for more.
    uses: u32,
    /// Passed as `mut` somewhere, so it needs an address in memory.
    addressed: bool,
    /// Where the `let` that declared it starts; `u32::MAX` for anything
    /// that was not declared with `let` or `var`.
    decl: u32,
}

/// A variable or a part of one, as a range of slots.
#[derive(Clone, Copy)]
struct Place {
    root: u32,
    start: u32,
    len: u32,
    mutable: bool,
    is_param: bool,
    is_view: bool,
    /// `Local::decl` of the variable.
    decl: u32,
}

/// Why a place that is not mutable cannot be changed, as the end of a message.
fn read_only(place: &Place) -> &'static str {
    if place.is_param {
        "a parameter is read-only unless it is declared `mut`"
    } else if place.is_view {
        "a loop variable or pattern binding is read-only"
    } else {
        "declare it with `var` to make it changeable"
    }
}

/// What an expression is being checked for, for error messages.
#[derive(Clone, Copy)]
enum What<'a> {
    Text(&'static str),
    Operator(BinOp),
    Argument(usize, &'a str),
    Field(&'a str),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Holder {
    /// An argument of a call that has not finished yet.
    Call,
    /// The left side of an operator whose right side is being evaluated.
    Operator,
    /// A `for` loop that is iterating over the place.
    Loop,
    /// A `match` whose arms look inside the place.
    Match,
}

/// The value of a constant, known while compiling.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConstValue {
    Int(i64),
    Bool(bool),
    /// Index in `strings`.
    Str(u32),
}

impl ConstValue {
    fn ty(self) -> Ty {
        match self {
            ConstValue::Int(_) => Ty::I64,
            ConstValue::Bool(_) => Ty::Bool,
            ConstValue::Str(_) => Ty::Str,
        }
    }
}

/// A `const` declaration.
struct ConstInfo<'a> {
    name: &'a str,
    pos: u32,
    ty: Option<TypeId>,
    expr: ExprId,
    /// 0 not evaluated yet, 1 being evaluated, 2 done.
    state: u8,
    value: ConstValue,
    /// For a number: its index in `ints`, shared by every use.
    int: u32,
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
    struct_templates: Vec<StructTemplate<'a>>,
    /// Struct name to its declaration in `struct_templates`.
    struct_names: HashMap<&'a str, u32>,
    struct_ids: HashMap<(u32, Vec<Ty>), u32>,
    /// How far the layout of each struct type has got: 0 not started,
    /// 1 in progress, 2 done.
    struct_state: Vec<u8>,
    /// Number of struct types whose layout is finished.
    settled_structs: usize,
    templates: Vec<Template<'a>>,
    template_names: HashMap<&'a str, u32>,
    /// Variant name to its enum declaration and its index there.
    variant_names: HashMap<&'a str, (u32, u32)>,
    /// Which bytes some variant name starts with. Most names in a
    /// program are not variants, and this tells so without hashing.
    variant_starts: [bool; 256],
    consts: Vec<ConstInfo<'a>>,
    const_names: HashMap<&'a str, u32>,
    /// Which bytes some constant's name starts with, like `variant_starts`.
    const_starts: [bool; 256],
    enums: Vec<EnumInfo>,
    enum_ids: HashMap<(u32, Vec<Ty>), u32>,
    enum_state: Vec<u8>,
    /// Number of enum types whose layout is finished.
    settled: usize,
    /// True once every declaration is laid out; enum types made later
    /// are laid out as soon as they appear.
    declared: bool,
    /// How many enum types are being made inside one another.
    nesting: u32,
    lists: Vec<Ty>,
    fns: Vec<FnInfo>,
    fn_names: HashMap<&'a str, u32>,
    /// Where each source file starts in `src`, with its path, in order:
    /// `embed` reads files relative to the one it is written in.
    files: &'a [(u32, &'a str)],
    /// Where the standard library's source starts. Functions from there
    /// on are the library's: `str_find` is the method `find`, and a
    /// function without parameters is called by its name.
    prelude_start: u32,
    generic_fns: Vec<GenericFn<'a>>,
    generic_names: HashMap<&'a str, u32>,
    /// The library's methods by name, as indices into `generic_fns`. The
    /// one whose first parameter fits the receiver is the one called.
    library: HashMap<&'a str, Vec<u32>>,
    /// A generic function and type arguments to its entry in `fns`.
    fn_ids: HashMap<(u32, Vec<Ty>), u32>,
    /// The type parameters of the function being checked, what they
    /// stand for, and its `FnInfo::depth`.
    fn_params: Vec<&'a str>,
    fn_bound: Vec<Option<Ty>>,
    fn_depth: u32,
    fn_origin: u32,
    /// The table shift of the function being checked.
    sh: Shift,
    expr_res: Vec<u32>,
    expr_ty: Vec<Ty>,
    expr_aux: Vec<u32>,
    expr_tmp: Vec<u32>,
    expr_moved: Vec<bool>,
    stmt_slot: Vec<u32>,
    stmt_flag: Vec<u8>,
    pats: Vec<PatInfo>,
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
    /// Expressions that are written before the one being checked but
    /// evaluated after it: the list in `list[index]` while the index is
    /// checked, the target of an assignment while its value is.
    late: Vec<ExprId>,
    /// The `?` expressions of the statement being checked.
    tries: Vec<ExprId>,
    /// How many list literals the expression being checked is inside.
    in_list: u32,
    /// How many parts that own memory have already been stored in the
    /// struct literals and variants the expression being checked is
    /// inside.
    owned_siblings: u32,
    /// Variables of the function being checked that could live in a
    /// register: slot and how much each is used.
    wishes: Vec<(u32, u32)>,
    zero_slots: Vec<u32>,
    ret: Ty,
    next_slot: u32,
    loop_depth: u32,
}

/// Checks a program that stands on its own, without the part of the
/// standard library that is written in azoL.
#[cfg(test)]
pub fn check<'a>(ast: &'a Ast, src: &'a str) -> Result<Program<'a>, SemaError> {
    check_with_prelude(ast, src, u32::MAX).map_err(|mut errors| errors.remove(0))
}

/// Checks a program whose source text continues, from `prelude_start`
/// on, with the standard library's azoL code.
///
/// On failure it returns every problem it can report reliably, in source
/// order: the first one among the declarations, or else the first one in
/// each function. A mistake inside one function does not disturb the
/// checking of the others, so those are independent of each other.
#[cfg(test)]
pub fn check_with_prelude<'a>(ast: &'a Ast, src: &'a str, prelude_start: u32) -> Result<Program<'a>, Vec<SemaError>> {
    check_with_files(ast, src, prelude_start, &[])
}

/// Like `check_with_prelude`, for a program whose files are known:
/// `files` holds where each one starts in `src` and its path, in order.
/// Without them `embed` reads relative to the current folder.
pub fn check_with_files<'a>(
    ast: &'a Ast,
    src: &'a str,
    prelude_start: u32,
    files: &'a [(u32, &'a str)],
) -> Result<Program<'a>, Vec<SemaError>> {
    let mut c = Checker {
        ast,
        src,
        files,
        structs: Vec::new(),
        struct_templates: Vec::new(),
        struct_names: HashMap::new(),
        struct_ids: HashMap::new(),
        struct_state: Vec::new(),
        settled_structs: 0,
        templates: Vec::new(),
        template_names: HashMap::new(),
        variant_names: HashMap::new(),
        variant_starts: [false; 256],
        consts: Vec::new(),
        const_names: HashMap::new(),
        const_starts: [false; 256],
        enums: Vec::new(),
        enum_ids: HashMap::new(),
        enum_state: Vec::new(),
        settled: 0,
        declared: false,
        nesting: 0,
        lists: Vec::new(),
        fns: Vec::new(),
        fn_names: HashMap::new(),
        prelude_start,
        generic_fns: Vec::new(),
        generic_names: HashMap::new(),
        library: HashMap::new(),
        fn_ids: HashMap::new(),
        fn_params: Vec::new(),
        fn_bound: Vec::new(),
        fn_depth: 0,
        fn_origin: 0,
        sh: Shift::default(),
        expr_res: vec![0; ast.exprs.len()],
        expr_ty: vec![Ty::Unit; ast.exprs.len()],
        expr_aux: vec![0; ast.exprs.len()],
        expr_tmp: vec![0; ast.exprs.len()],
        expr_moved: vec![false; ast.exprs.len()],
        stmt_slot: vec![0; ast.stmts.len()],
        stmt_flag: vec![0; ast.stmts.len()],
        pats: vec![PatInfo::default(); ast.patterns.len()],
        stmt_drops: vec![(0, 0); ast.stmts.len()],
        drops: Vec::new(),
        init_offset: vec![0; ast.field_inits.len()],
        ints: Vec::new(),
        data: Vec::new(),
        strings: Vec::new(),
        locals: Vec::new(),
        borrows: Vec::new(),
        pending: Vec::new(),
        late: Vec::new(),
        tries: Vec::new(),
        in_list: 0,
        owned_siblings: 0,
        wishes: Vec::new(),
        zero_slots: Vec::new(),
        ret: Ty::Unit,
        next_slot: 0,
        loop_depth: 0,
    };
    // Later declarations and every function body depend on the
    // declarations, so a mistake there is reported alone.
    c.declare_types().map_err(|error| vec![error])?;
    c.declare_consts().map_err(|error| vec![error])?;
    c.signatures().map_err(|error| vec![error])?;
    // Checking a function can add to the list: each new use of a generic
    // function is one more function to check.
    let mut errors: Vec<SemaError> = Vec::new();
    let mut index = 0;
    while index < c.fns.len() {
        if let Err(error) = c.function(index as u32) {
            // A generic function is checked once per set of types; the
            // same mistake in it is reported once.
            if !errors.iter().any(|known| known.offset == error.offset) {
                errors.push(error);
            }
            // Types the function was in the middle of creating stay as
            // they are; nothing is generated from a program with errors.
            c.settled_structs = c.structs.len();
            c.settled = c.enums.len();
        }
        index += 1;
    }
    if !c.fn_names.contains_key("main") {
        errors.push(SemaError { message: "the program has no `main` function".to_string(), offset: 0, expr: None, fix: None });
    }
    if !errors.is_empty() {
        errors.sort_by_key(|error| error.offset);
        return Err(errors);
    }
    let Some(&main) = c.fn_names.get("main") else {
        return Err(vec![SemaError { message: "the program has no `main` function".to_string(), offset: 0, expr: None, fix: None }]);
    };
    Ok(Program {
        structs: c.structs,
        enums: c.enums,
        lists: c.lists,
        fns: c.fns,
        main,
        expr_res: c.expr_res,
        expr_ty: c.expr_ty,
        expr_aux: c.expr_aux,
        expr_tmp: c.expr_tmp,
        expr_moved: c.expr_moved,
        stmt_slot: c.stmt_slot,
        stmt_flag: c.stmt_flag,
        pats: c.pats,
        stmt_drops: c.stmt_drops,
        drops: c.drops,
        init_offset: c.init_offset,
        ints: c.ints,
        data: c.data,
        strings: c.strings,
    })
}

/// The value of a number that is not written in decimal: `0x1F`,
/// `0b1010` or a character such as `'a'` or `'\n'`. The outer `None`
/// means the text is a decimal number; the inner one that it needs more
/// than 64 bits. All 64 bits may be used, so `0xFFFF_FFFF_FFFF_FFFF` is
/// the `i64` with every bit set, which is -1.
fn special_literal(text: &str) -> Option<Option<i64>> {
    let bytes = text.as_bytes();
    if bytes[0] == b'\'' {
        // The lexer accepted only one ASCII character or one of these escapes.
        return Some(Some(match (bytes[1], bytes[2]) {
            (b'\\', b'n') => b'\n',
            (b'\\', b't') => b'\t',
            (b'\\', b'r') => b'\r',
            (b'\\', b'0') => 0,
            (b'\\', other) => other,
            (plain, _) => plain,
        } as i64));
    }
    let bits_per_digit = match bytes.get(1) {
        Some(b'x') if bytes[0] == b'0' => 4,
        Some(b'b') if bytes[0] == b'0' => 1,
        _ => return None,
    };
    let mut value: u64 = 0;
    for &byte in bytes[2..].iter().filter(|&&b| b != b'_') {
        if value >> (64 - bits_per_digit) != 0 {
            return Some(None);
        }
        let digit = (byte as char).to_digit(16).expect("the lexer accepted only digits") as u64;
        value = value << bits_per_digit | digit;
    }
    Some(Some(value as i64))
}

/// The value of a number literal as written: decimal, hexadecimal,
/// binary or a character. The error is the message to report.
fn int_literal(text: &str) -> Result<i64, &'static str> {
    let bytes = text.as_bytes();
    match special_literal(text) {
        Some(value) => {
            if bytes[0] != b'\'' {
                number_fault(bytes, 2)?;
            }
            value.ok_or("this number does not fit in 64 bits")
        }
        None => {
            let mut value: i64 = 0;
            let mut underscore = false;
            for &byte in bytes {
                if byte == b'_' {
                    underscore = true;
                    continue;
                }
                let digit = (byte - b'0') as i64;
                value = value.checked_mul(10).and_then(|v| v.checked_add(digit)).ok_or("this number does not fit in `i64`")?;
            }
            // Only a number with a `_` or a zero in front can be badly written.
            if underscore || (bytes[0] == b'0' && bytes.len() > 1) {
                number_fault(bytes, 0)?;
            }
            Ok(value)
        }
    }
}

/// Says what is wrong with the way the number `text` is written, if
/// anything: its digits start at `digits`, after any `0x` or `0b`. A `_`
/// stands between two digits, and a decimal number has no zeros in
/// front, so that nothing looks like the octal numbers of other languages.
fn number_fault(text: &[u8], digits: usize) -> Result<(), &'static str> {
    for at in digits..text.len() {
        if text[at] == b'_'
            && (at == digits || at + 1 == text.len() || !text[at - 1].is_ascii_hexdigit() || !text[at + 1].is_ascii_hexdigit())
        {
            return Err("a `_` in a number stands between two digits, as in `1_000`");
        }
    }
    if digits == 0 && text.len() > 1 && text[0] == b'0' {
        return Err("a decimal number does not start with `0`; other bases are written `0x1F` and `0b1010`");
    }
    Ok(())
}

/// The most bytes of text a program may hold with an embedded file.
const MAX_EMBED: u64 = 1 << 30;

/// What the system says went wrong with a file, without the error
/// number Rust adds: `No such file or directory`.
fn os_reason(problem: &std::io::Error) -> String {
    let text = problem.to_string();
    match text.find(" (os error ") {
        Some(end) => text[..end].to_string(),
        None => text,
    }
}

fn err<T>(offset: u32, message: impl Into<String>) -> SResult<T> {
    Err(SemaError { message: message.into(), offset, expr: None, fix: None })
}

/// An error about the whole expression `expr`.
fn err_expr<T>(ast: &Ast, expr: ExprId, message: impl Into<String>) -> SResult<T> {
    Err(SemaError { message: message.into(), offset: ast.expr_pos[expr as usize], expr: Some(expr), fix: None })
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
            Ty::Struct(index) => self.structs[index as usize].name.clone(),
            Ty::List(index) => format!("List[{}]", self.ty_name(self.lists[index as usize])),
            Ty::Enum(index) => self.enums[index as usize].name.clone(),
        }
    }

    fn size_of(&self, ty: Ty) -> u32 {
        size_of(&self.structs, &self.enums, ty)
    }

    fn needs_drop(&self, ty: Ty) -> bool {
        needs_drop(&self.structs, &self.enums, ty)
    }

    fn owned(&self, ty: Ty) -> Vec<(u32, Ty)> {
        owned(&self.structs, &self.enums, ty)
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

    /// Resolves a type written in a function or a struct. Inside a use
    /// of a generic function its type parameters stand for known types.
    fn ty(&mut self, id: TypeId) -> SResult<Ty> {
        let at = self.ast.types[id as usize].name.start;
        let ty = if self.fn_params.is_empty() {
            let shape = self.shape(id, &[])?;
            self.realize(&shape, &[], at)?
        } else {
            let (params, bound) = (self.fn_params.clone(), self.fn_bound.clone());
            let shape = self.shape(id, &params)?;
            self.realize(&shape, &bound, at)?
        };
        self.settle()?;
        Ok(ty)
    }

    /// Reads a type as written. `params` are the type parameters of the
    /// enum declaration it appears in, if any.
    fn shape(&mut self, id: TypeId, params: &[&'a str]) -> SResult<Shape> {
        let ty = self.ast.types[id as usize];
        let name = ty.name.text(self.src);
        let args = self.ast.list(ty.args);
        if let Some(index) = params.iter().position(|&param| param == name) {
            if !args.is_empty() {
                return err(ty.name.start, format!("`{name}` is a type parameter and takes no type arguments"));
            }
            return Ok(Shape::Param(index as u32));
        }
        // The common case first: it needs no lookup.
        let scalar = match name {
            "i64" => Some(Ty::I64),
            "bool" => Some(Ty::Bool),
            "str" => Some(Ty::Str),
            _ => None,
        };
        if let Some(known) = scalar {
            if !args.is_empty() {
                return err(ty.name.start, format!("`{name}` takes no type arguments"));
            }
            return Ok(Shape::Known(known));
        }
        if name == "List" {
            if args.len() != 1 {
                return err(ty.name.start, "`List` takes one element type, as in `List[i64]`");
            }
            return Ok(Shape::List(Box::new(self.shape(args[0], params)?)));
        }
        if let Some(&template) = self.template_names.get(name) {
            let want = self.templates[template as usize].params.len();
            if args.len() != want {
                return err(ty.name.start, Self::wrong_type_arguments(name, want));
            }
            let mut shapes = Vec::with_capacity(want);
            for &arg in args {
                shapes.push(self.shape(arg, params)?);
            }
            return Ok(Shape::Enum(template, shapes));
        }
        let Some(&template) = self.struct_names.get(name) else {
            return err(
                ty.name.start,
                format!("unknown type `{name}`; the built-in types so far are `i64`, `bool`, `str`, `List`, `Option` and `Result`"),
            );
        };
        let want = self.struct_templates[template as usize].params.len();
        if args.len() != want {
            return err(ty.name.start, Self::wrong_type_arguments(name, want));
        }
        if let Some(only) = self.struct_templates[template as usize].only {
            // A struct without type parameters is one known type.
            return Ok(Shape::Known(Ty::Struct(only)));
        }
        let mut shapes = Vec::with_capacity(want);
        for &arg in args {
            shapes.push(self.shape(arg, params)?);
        }
        Ok(Shape::Struct(template, shapes))
    }

    /// The message for a type written with the wrong number of type arguments.
    fn wrong_type_arguments(name: &str, want: usize) -> String {
        if want == 0 {
            format!("`{name}` takes no type arguments")
        } else {
            format!(
                "`{name}` takes {want} type argument{}, as in `{name}[{}]`",
                if want == 1 { "" } else { "s" },
                vec!["i64"; want].join(", ")
            )
        }
    }

    /// True when `shape` mentions no type parameter that is still unknown.
    fn ready(shape: &Shape, bound: &[Option<Ty>]) -> bool {
        match shape {
            Shape::Param(index) => bound[*index as usize].is_some(),
            Shape::Known(_) => true,
            Shape::List(elem) => Self::ready(elem, bound),
            Shape::Struct(_, args) | Shape::Enum(_, args) => args.iter().all(|arg| Self::ready(arg, bound)),
        }
    }

    /// Turns a shape into a type, given what its type parameters stand
    /// for. Every parameter it mentions must be known.
    fn realize(&mut self, shape: &Shape, bound: &[Option<Ty>], at: u32) -> SResult<Ty> {
        match shape {
            Shape::Param(index) => Ok(bound[*index as usize].expect("the caller checked that the parameter is known")),
            Shape::Known(ty) => Ok(*ty),
            Shape::List(elem) => {
                let elem = self.realize(elem, bound, at)?;
                Ok(self.list_of(elem))
            }
            Shape::Struct(template, shapes) | Shape::Enum(template, shapes) => {
                let mut args = Vec::with_capacity(shapes.len());
                for shape in shapes {
                    args.push(self.realize(shape, bound, at)?);
                }
                if matches!(shape, Shape::Struct(..)) {
                    self.struct_instance(*template, args, at)
                } else {
                    self.instance(*template, args, at)
                }
            }
        }
    }

    /// The struct type made from a declaration and type arguments,
    /// created the first time it is asked for. Like `instance`, which
    /// does the same for enums.
    fn struct_instance(&mut self, template: u32, args: Vec<Ty>, at: u32) -> SResult<Ty> {
        let key = (template, args);
        if let Some(&index) = self.struct_ids.get(&key) {
            return Ok(Ty::Struct(index));
        }
        let (template, args) = key;
        if self.nesting >= 200 {
            return err(at, "this type nests structs too deeply");
        }
        let info = &self.struct_templates[template as usize];
        let mut name = info.name.to_string();
        if !args.is_empty() {
            let parts: Vec<String> = args.iter().map(|&arg| self.ty_name(arg)).collect();
            name = format!("{name}[{}]", parts.join(", "));
        }
        let shapes = Rc::clone(&info.fields);
        let index = self.structs.len() as u32;
        self.struct_ids.insert((template, args.clone()), index);
        let bound: Vec<Option<Ty>> = args.iter().map(|&arg| Some(arg)).collect();
        self.structs.push(StructInfo { name, template, args, size: 0, fields: Vec::new(), heap: Vec::new() });
        self.struct_state.push(0);
        self.nesting += 1;
        let mut fields = Vec::with_capacity(shapes.len());
        for (field, shape) in shapes.iter() {
            fields.push(FieldInfo { name: field, ty: self.realize(shape, &bound, at)?, offset: 0 });
        }
        self.nesting -= 1;
        self.structs[index as usize].fields = fields;
        Ok(Ty::Struct(index))
    }

    /// The enum type made from a declaration and type arguments. It is
    /// created the first time it is asked for; its layout follows in
    /// `settle`, once the types it mentions exist too.
    fn instance(&mut self, template: u32, args: Vec<Ty>, at: u32) -> SResult<Ty> {
        let key = (template, args);
        if let Some(&index) = self.enum_ids.get(&key) {
            return Ok(Ty::Enum(index));
        }
        let (template, args) = key;
        // `enum E[T] { A(E[List[T]]) }` would ask for ever larger types.
        if self.nesting >= 200 {
            return err(at, "this type nests enums too deeply");
        }
        let info = &self.templates[template as usize];
        let mut name = info.name.to_string();
        if !args.is_empty() {
            let parts: Vec<String> = args.iter().map(|&arg| self.ty_name(arg)).collect();
            name = format!("{name}[{}]", parts.join(", "));
        }
        let shapes: Vec<Vec<Shape>> = info.variants.iter().map(|(_, fields)| fields.clone()).collect();
        let index = self.enums.len() as u32;
        // Registered before its fields are resolved: they may mention it.
        self.enum_ids.insert((template, args.clone()), index);
        let bound: Vec<Option<Ty>> = args.iter().map(|&arg| Some(arg)).collect();
        self.enums.push(EnumInfo { name, template, args, size: 0, variants: Vec::new(), owning: false });
        self.enum_state.push(0);
        self.nesting += 1;
        let mut variants = Vec::with_capacity(shapes.len());
        for fields in &shapes {
            let mut resolved = Vec::with_capacity(fields.len());
            for field in fields {
                resolved.push((self.realize(field, &bound, at)?, 0));
            }
            variants.push(VariantInfo { fields: resolved, heap: Vec::new() });
        }
        self.nesting -= 1;
        self.enums[index as usize].variants = variants;
        Ok(Ty::Enum(index))
    }

    /// Lays out the struct and enum types made since the last call.
    fn settle(&mut self) -> SResult<()> {
        if !self.declared {
            return Ok(());
        }
        while self.settled_structs < self.structs.len() {
            self.layout_struct(self.settled_structs)?;
            self.settled_structs += 1;
        }
        while self.settled < self.enums.len() {
            self.layout_enum(self.settled)?;
            self.settled += 1;
        }
        Ok(())
    }

    /// How a shape is written, for messages.
    fn shape_name(&self, shape: &Shape, params: &[&str]) -> String {
        match shape {
            Shape::Param(index) => params[*index as usize].to_string(),
            Shape::Known(ty) => self.ty_name(*ty),
            Shape::List(elem) => format!("List[{}]", self.shape_name(elem, params)),
            Shape::Struct(template, args) | Shape::Enum(template, args) => {
                let name = match shape {
                    Shape::Struct(..) => self.struct_templates[*template as usize].name,
                    _ => self.templates[*template as usize].name,
                };
                if args.is_empty() {
                    name.to_string()
                } else {
                    let parts: Vec<String> = args.iter().map(|arg| self.shape_name(arg, params)).collect();
                    format!("{name}[{}]", parts.join(", "))
                }
            }
        }
    }

    /// Matches a shape against a type, learning what unknown type
    /// parameters stand for. False when they cannot be the same type.
    fn unify(&self, shape: &Shape, ty: Ty, bound: &mut [Option<Ty>]) -> bool {
        match shape {
            Shape::Param(index) => match bound[*index as usize] {
                Some(known) => known == ty,
                None => {
                    bound[*index as usize] = Some(ty);
                    true
                }
            },
            Shape::Known(known) => *known == ty,
            Shape::List(elem) => match ty {
                Ty::List(list) => self.unify(elem, self.lists[list as usize], bound),
                _ => false,
            },
            Shape::Struct(template, shapes) => match ty {
                Ty::Struct(index) if self.structs[index as usize].template == *template => {
                    let args = &self.structs[index as usize].args;
                    shapes.iter().zip(args).all(|(shape, &arg)| self.unify(shape, arg, bound))
                }
                _ => false,
            },
            Shape::Enum(template, shapes) => match ty {
                Ty::Enum(index) if self.enums[index as usize].template == *template => {
                    let args = &self.enums[index as usize].args;
                    shapes.iter().zip(args).all(|(shape, &arg)| self.unify(shape, arg, bound))
                }
                _ => false,
            },
        }
    }

    // ---- declarations --------------------------------------------------

    fn add_variant(&mut self, name: &'a str, template: u32, index: u32) {
        self.variant_names.insert(name, (template, index));
        self.variant_starts[name.as_bytes()[0] as usize] = true;
    }

    /// The enum declaration and index of the variant called `name`.
    fn variant(&self, name: &str) -> Option<(u32, u32)> {
        if !self.variant_starts[name.as_bytes()[0] as usize] {
            return None;
        }
        self.variant_names.get(name).copied()
    }

    fn builtin_enum(&mut self, name: &'static str, params: &[&'static str], variants: Vec<(&'static str, Vec<Shape>)>) {
        let index = self.templates.len() as u32;
        self.template_names.insert(name, index);
        for (at, &(variant, _)) in variants.iter().enumerate() {
            self.add_variant(variant, index, at as u32);
        }
        self.templates.push(Template { name, pos: 0, params: params.to_vec(), variants, builtin: true });
    }

    fn declare_types(&mut self) -> SResult<()> {
        self.builtin_enum("Option", &["T"], vec![("Some", vec![Shape::Param(0)]), ("None", Vec::new())]);
        self.builtin_enum("Result", &["T", "E"], vec![("Ok", vec![Shape::Param(0)]), ("Err", vec![Shape::Param(1)])]);

        // First every type name, so that declarations can mention each
        // other in any order.
        let mut struct_decls = Vec::new();
        let mut enum_decls = Vec::new();
        for item in &self.ast.items {
            let (name, is_struct) = match *item {
                Item::Struct { name, .. } => (name, true),
                Item::Enum { name, .. } => (name, false),
                Item::Fn { .. } | Item::Const { .. } => continue,
            };
            let text = name.text(self.src);
            if matches!(text, "i64" | "bool" | "str" | "List") {
                return err(name.start, format!("`{text}` is a built-in type and cannot be redefined"));
            }
            if name.start >= self.prelude_start {
                // The program's own declarations come first, so the
                // clash is with one of them: that is where to report it.
                let earlier = match (self.template_names.get(text), self.struct_names.get(text)) {
                    (Some(&template), _) => Some(self.templates[template as usize].pos),
                    (_, Some(&index)) => Some(self.struct_templates[index as usize].pos),
                    _ => None,
                };
                if let Some(pos) = earlier {
                    return err(pos, format!("`{text}` is a type of the standard library; choose another name"));
                }
            }
            if let Some(&template) = self.template_names.get(text) {
                return err(
                    name.start,
                    if self.templates[template as usize].builtin {
                        format!("`{text}` is built in and cannot be redefined")
                    } else if is_struct {
                        format!("`{text}` is already defined as an enum")
                    } else {
                        format!("enum `{text}` is defined more than once")
                    },
                );
            }
            if self.struct_names.contains_key(text) {
                return err(
                    name.start,
                    if is_struct {
                        format!("struct `{text}` is defined more than once")
                    } else {
                        format!("`{text}` is already defined as a struct")
                    },
                );
            }
            let (Item::Struct { generics, .. } | Item::Enum { generics, .. }) = *item else {
                continue;
            };
            let mut params: Vec<&'a str> = Vec::with_capacity(generics.len as usize);
            for &id in self.ast.list(generics) {
                let param = self.ast.names[id as usize];
                let param_text = param.text(self.src);
                if params.contains(&param_text) {
                    return err(param.start, format!("type parameter `{param_text}` is declared more than once"));
                }
                params.push(param_text);
            }
            match *item {
                Item::Struct { fields, .. } => {
                    let index = self.struct_templates.len() as u32;
                    self.struct_names.insert(text, index);
                    self.struct_templates.push(StructTemplate { name: text, pos: name.start, params, fields: Rc::new(Vec::new()), only: None });
                    struct_decls.push((index, fields));
                }
                Item::Enum { variants, .. } => {
                    let index = self.templates.len() as u32;
                    self.template_names.insert(text, index);
                    self.templates.push(Template { name: text, pos: name.start, params, variants: Vec::new(), builtin: false });
                    enum_decls.push((index, variants));
                }
                Item::Fn { .. } | Item::Const { .. } => {}
            }
        }

        // Then what each enum's variants hold.
        for &(index, variants) in &enum_decls {
            let info = &self.templates[index as usize];
            let (enum_name, pos, params) = (info.name, info.pos, info.params.clone());
            let variant_ids = self.ast.list(variants);
            if variant_ids.is_empty() {
                return err(pos, format!("enum `{enum_name}` has no variants"));
            }
            if variant_ids.len() > MAX_VARIANTS {
                return err(pos, format!("enum `{enum_name}` has more than {MAX_VARIANTS} variants, which is not supported yet"));
            }
            for (at, &id) in variant_ids.iter().enumerate() {
                let variant = self.ast.variants[id as usize];
                let text = variant.name.text(self.src);
                if let Some(&(other, _)) = self.variant_names.get(text) {
                    return err(
                        variant.name.start,
                        if other == index {
                            format!("variant `{text}` is declared more than once")
                        } else {
                            format!(
                                "variant `{text}` already belongs to `{}`; variant names must differ across enums",
                                self.templates[other as usize].name
                            )
                        },
                    );
                }
                self.add_variant(text, index, at as u32);
                let mut shapes = Vec::with_capacity(variant.fields.len as usize);
                for &field in self.ast.list(variant.fields) {
                    shapes.push(self.shape(field, &params)?);
                }
                self.templates[index as usize].variants.push((text, shapes));
            }
        }

        // Then the fields of each struct.
        for &(index, fields) in &struct_decls {
            let params = self.struct_templates[index as usize].params.clone();
            let mut shapes: Vec<(&'a str, Shape)> = Vec::new();
            for &id in self.ast.list(fields) {
                let field = self.ast.fields[id as usize];
                let name = field.name.text(self.src);
                if shapes.iter().any(|&(known, _)| known == name) {
                    return err(field.name.start, format!("field `{name}` is declared more than once"));
                }
                shapes.push((name, self.shape(field.ty, &params)?));
            }
            self.struct_templates[index as usize].fields = Rc::new(shapes);
        }

        // A struct or enum without type parameters is one type; make it
        // now so that it is checked even if nothing uses it.
        for &(index, _) in &struct_decls {
            let info = &self.struct_templates[index as usize];
            if info.params.is_empty() {
                let pos = info.pos;
                if let Ty::Struct(only) = self.struct_instance(index, Vec::new(), pos)? {
                    self.struct_templates[index as usize].only = Some(only);
                }
            }
        }
        for &(index, _) in &enum_decls {
            let info = &self.templates[index as usize];
            if info.params.is_empty() {
                let pos = info.pos;
                self.instance(index, Vec::new(), pos)?;
            }
        }

        // Sizes and offsets, depth first. A type that contains itself
        // would have no finite size.
        self.declared = true;
        self.settle()
    }

    /// The size of a type in slots, laying it out first if needed.
    fn layout(&mut self, ty: Ty) -> SResult<u32> {
        match ty {
            Ty::Struct(index) => self.layout_struct(index as usize),
            Ty::Enum(index) => self.layout_enum(index as usize),
            other => Ok(self.size_of(other)),
        }
    }

    fn layout_struct(&mut self, index: usize) -> SResult<u32> {
        let pos = self.struct_templates[self.structs[index].template as usize].pos;
        match self.struct_state[index] {
            2 => return Ok(self.structs[index].size),
            1 => {
                return err(
                    pos,
                    format!("struct `{}` contains itself, so it would have no finite size", self.structs[index].name),
                );
            }
            _ => {}
        }
        self.struct_state[index] = 1;
        let mut offset = 0;
        let mut heap = Vec::new();
        for field in 0..self.structs[index].fields.len() {
            let ty = self.structs[index].fields[field].ty;
            let size = self.layout(ty)?;
            self.structs[index].fields[field].offset = offset;
            for (inner, leaf) in self.owned(ty) {
                heap.push((offset + inner, leaf));
            }
            offset += size;
        }
        if offset > MAX_SIZE {
            return err(pos, format!("struct `{}` is too large for now", self.structs[index].name));
        }
        self.structs[index].size = offset;
        self.structs[index].heap = heap;
        self.struct_state[index] = 2;
        Ok(offset)
    }

    fn layout_enum(&mut self, index: usize) -> SResult<u32> {
        let pos = self.templates[self.enums[index].template as usize].pos;
        match self.enum_state[index] {
            2 => return Ok(self.enums[index].size),
            1 => {
                return err(
                    pos,
                    format!(
                        "enum `{}` contains itself, so it would have no finite size; put the inner value in a `List`",
                        self.enums[index].name
                    ),
                );
            }
            _ => {}
        }
        self.enum_state[index] = 1;
        // Slot 0 is the tag; every variant's fields start right after it.
        let mut size = 1;
        let mut owning = false;
        for variant in 0..self.enums[index].variants.len() {
            let mut offset = 1;
            let mut heap = Vec::new();
            for field in 0..self.enums[index].variants[variant].fields.len() {
                let ty = self.enums[index].variants[variant].fields[field].0;
                let field_size = self.layout(ty)?;
                self.enums[index].variants[variant].fields[field].1 = offset;
                for (inner, leaf) in self.owned(ty) {
                    heap.push((offset + inner, leaf));
                }
                offset += field_size;
            }
            owning |= !heap.is_empty();
            self.enums[index].variants[variant].heap = heap;
            size = size.max(offset);
        }
        if size > MAX_SIZE {
            return err(pos, format!("enum `{}` is too large for now", self.enums[index].name));
        }
        self.enums[index].size = size;
        self.enums[index].owning = owning;
        self.enum_state[index] = 2;
        Ok(size)
    }

    fn signatures(&mut self) -> SResult<()> {
        for (item_index, item) in self.ast.items.iter().enumerate() {
            let Item::Fn { name, generics, params, ret, .. } = *item else {
                continue;
            };
            let text = name.text(self.src);
            let in_prelude = name.start >= self.prelude_start;
            if !in_prelude {
                if BUILTIN_FUNCTIONS.contains(&text) {
                    return err(name.start, format!("`{text}` is built in and cannot be redefined"));
                }
                if let Some(&(template, _)) = self.variant_names.get(text) {
                    return err(name.start, format!("`{text}` is already a variant of `{}`", self.templates[template as usize].name));
                }
                if self.const_names.contains_key(text) {
                    return err(name.start, format!("`{text}` is already a constant"));
                }
            }
            let param_ids = self.ast.list(params);
            if param_ids.len() > MAX_PARAMS {
                return err(name.start, format!("function `{text}` has more than {MAX_PARAMS} parameters, which is not supported yet"));
            }
            if generics.len > 0 || in_prelude {
                if text == "main" {
                    return err(name.start, "`main` cannot have type parameters");
                }
                let mut type_params: Vec<&'a str> = Vec::with_capacity(generics.len as usize);
                for &id in self.ast.list(generics) {
                    let param = self.ast.names[id as usize];
                    let param_text = param.text(self.src);
                    if type_params.contains(&param_text) {
                        return err(param.start, format!("type parameter `{param_text}` is declared more than once"));
                    }
                    type_params.push(param_text);
                }
                // What the types mean is settled per use; here they are
                // only read, which also reports unknown type names.
                let mut shapes = Vec::with_capacity(param_ids.len());
                for &id in param_ids {
                    let param = self.ast.params[id as usize];
                    shapes.push((self.shape(param.ty, &type_params)?, param.mutable));
                }
                let ret = match ret {
                    Some(id) => Some(self.shape(id, &type_params)?),
                    None => None,
                };
                let index = self.generic_fns.len() as u32;
                let mut shown = text;
                if in_prelude {
                    if shapes.is_empty() {
                        // A plain function of the library. One of the
                        // program's own with the same name hides it.
                        if self.fn_names.contains_key(text) || self.generic_names.contains_key(text) {
                            continue;
                        }
                        self.generic_names.insert(text, index);
                    } else {
                        let (_, method) = text.split_once('_').expect("a standard library method is named `type_method`");
                        self.library.entry(method).or_default().push(index);
                        shown = method;
                    }
                } else if self.fn_names.contains_key(text) || self.generic_names.insert(text, index).is_some() {
                    return err(name.start, format!("function `{text}` is defined more than once"));
                }
                self.generic_fns.push(GenericFn { item: item_index as u32, name: shown, type_params, params: shapes, ret });
                continue;
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
            if self.generic_names.contains_key(text) || self.fn_names.insert(text, index).is_some() {
                return err(name.start, format!("function `{text}` is defined more than once"));
            }
            self.fns.push(FnInfo {
                item: item_index as u32,
                shift: Shift::default(),
                generic: None,
                type_args: Vec::new(),
                depth: 0,
                origin: name.start,
                ret,
                params: infos,
                slots: 0,
                sret_slot: None,
                scratch_slot: 0,
                registers: Vec::new(),
                zero_slots: Vec::new(),
            });
        }
        Ok(())
    }

    // ---- functions and statements --------------------------------------

    /// Checks the body of function `index`. A mistake inside a generic
    /// function may only show with some types, so the message names them.
    fn function(&mut self, index: u32) -> SResult<()> {
        let info = &self.fns[index as usize];
        self.sh = info.shift;
        self.fn_depth = info.depth;
        self.fn_origin = info.origin;
        self.fn_bound = info.type_args.iter().map(|&ty| Some(ty)).collect();
        let Some(generic) = info.generic else {
            self.fn_params.clear();
            return self.function_body(index);
        };
        self.fn_params = self.generic_fns[generic as usize].type_params.clone();
        let result = self.function_body(index);
        result.map_err(|mut error| {
            let types: Vec<String> = self
                .fn_params
                .iter()
                .zip(&self.fns[index as usize].type_args)
                .map(|(param, &ty)| {
                    let mut name = self.ty_name(ty);
                    if name.len() > 60 {
                        name.truncate(57);
                        name.push_str("...");
                    }
                    format!("`{param}` = `{name}`")
                })
                .collect();
            if !types.is_empty() {
                error.message = format!("{} (in `{}` with {})", error.message, self.generic_fns[generic as usize].name, types.join(", "));
            }
            // A mistake inside the standard library's code is shown where
            // the program asked for these types.
            if error.offset >= self.prelude_start {
                error.offset = self.fns[index as usize].origin;
            }
            error
        })
    }

    fn function_body(&mut self, index: u32) -> SResult<()> {
        let Item::Fn { name, params, body, .. } = self.ast.items[self.fns[index as usize].item as usize] else {
            unreachable!("a function's item is a function");
        };
        self.locals.clear();
        self.borrows.clear();
        self.pending.clear();
        self.late.clear();
        self.tries.clear();
        self.in_list = 0;
        self.owned_siblings = 0;
        self.wishes.clear();
        self.zero_slots.clear();
        self.next_slot = 0;
        self.loop_depth = 0;
        self.ret = self.fns[index as usize].ret;
        for (i, &id) in self.ast.list(params).iter().enumerate() {
            let param = self.ast.params[id as usize];
            let info = self.fns[index as usize].params[i];
            let slot = self.alloc(1, param.name.start)?;
            self.locals.push(Local {
                name: self.binding(param.name)?,
                slot,
                ty: info.ty,
                mutable: info.mutable,
                by_ref: info.by_ref,
                is_param: true,
                is_view: false,
                owns: false,
                depth: self.loop_depth,
                candidate: None,
                scalar: !info.by_ref,
                uses: 0,
                addressed: false,
                decl: u32::MAX,
            });
        }
        if self.ret.is_aggregate() {
            self.fns[index as usize].sret_slot = Some(self.alloc(1, name.start)?);
        }
        self.fns[index as usize].scratch_slot = self.alloc(1, name.start)?;
        self.block(body)?;
        self.close_scope(0);
        if self.ret != Ty::Unit && !self.always_returns(body) {
            return err(
                name.start,
                format!("function `{}` must return a value of type `{}` on every path", name.text(self.src), self.ty_name(self.ret)),
            );
        }
        self.fns[index as usize].slots = self.next_slot;
        self.fns[index as usize].zero_slots = std::mem::take(&mut self.zero_slots);
        // The most used variables get the registers. One that is hardly
        // used is not worth saving and restoring a register for.
        self.wishes.retain(|&(_, uses)| uses >= MIN_REGISTER_USES);
        self.wishes.sort_by_key(|&(slot, uses)| (std::cmp::Reverse(uses), slot));
        self.fns[index as usize].registers =
            self.wishes.iter().take(REGISTERS as usize).enumerate().map(|(i, &(slot, _))| (slot, FIRST_REGISTER + i as u8)).collect();
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
            // `exit` ends the program; nothing comes back from it. The
            // name is built in, so no function of the program has it.
            Stmt::Expr(expr) => match self.ast.exprs[expr as usize] {
                Expr::Call { callee, .. } => matches!(self.ast.exprs[callee as usize], Expr::Ident(name) if name.text(self.src) == "exit"),
                _ => false,
            },
            Stmt::If { then_block, else_branch, .. } => {
                self.always_returns(then_block)
                    && match else_branch {
                        Else::None => false,
                        Else::Block(body) => self.always_returns(body),
                        Else::If(next) => self.stmt_returns(next),
                    }
            }
            // The checker makes sure the arms cover every case.
            Stmt::Match { arms, .. } => {
                let arms = self.ast.list(arms);
                !arms.is_empty() && arms.iter().all(|&arm| self.always_returns(self.ast.arms[arm as usize].body))
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

    /// The name of the constant an already checked expression is, if it is one.
    fn constant_name(&self, id: ExprId) -> Option<&'a str> {
        match self.ast.exprs[id as usize] {
            Expr::Ident(name) if self.expr_aux[self.sh.e(id)] >= CONST_STR => Some(name.text(self.src)),
            _ => None,
        }
    }

    /// The text of a name that is about to become a variable. A variant
    /// name cannot be one: `None` must always mean the variant.
    fn binding(&self, name: Span) -> SResult<&'a str> {
        let text = name.text(self.src);
        if let Some((template, _)) = self.variant(text) {
            return err(
                name.start,
                format!("`{text}` is a variant of `{}` and cannot be used as a variable name", self.templates[template as usize].name),
            );
        }
        if self.constant(name).is_some() {
            return err(name.start, format!("`{text}` is a constant and cannot be used as a variable name"));
        }
        Ok(text)
    }

    /// The constant that the name written at `name` means, if any. The
    /// standard library's own code never means a program's constants,
    /// so its variables may share a name with one.
    fn constant(&self, name: Span) -> Option<u32> {
        let text = name.text(self.src);
        if name.start >= self.prelude_start || !self.const_starts[text.as_bytes()[0] as usize] {
            return None;
        }
        self.const_names.get(text).copied()
    }

    /// Registers every `const` and computes its value. Constants may
    /// use each other in any order, but not in a circle.
    fn declare_consts(&mut self) -> SResult<()> {
        for item in &self.ast.items {
            let Item::Const { name, ty, value } = *item else {
                continue;
            };
            let text = name.text(self.src);
            if BUILTIN_FUNCTIONS.contains(&text) {
                return err(name.start, format!("`{text}` is built in and cannot be redefined"));
            }
            if let Some(&(template, _)) = self.variant_names.get(text) {
                return err(name.start, format!("`{text}` is already a variant of `{}`", self.templates[template as usize].name));
            }
            if self.const_names.insert(text, self.consts.len() as u32).is_some() {
                return err(name.start, format!("constant `{text}` is defined more than once"));
            }
            self.const_starts[text.as_bytes()[0] as usize] = true;
            self.consts.push(ConstInfo { name: text, pos: name.start, ty, expr: value, state: 0, value: ConstValue::Bool(false), int: 0 });
        }
        for index in 0..self.consts.len() {
            self.const_value(index as u32)?;
        }
        Ok(())
    }

    /// The value of constant `index`, computed the first time it is needed.
    fn const_value(&mut self, index: u32) -> SResult<ConstValue> {
        let info = &self.consts[index as usize];
        let (name, pos, ty, expr) = (info.name, info.pos, info.ty, info.expr);
        match info.state {
            2 => return Ok(info.value),
            1 => return err(pos, format!("constant `{name}` is defined in terms of itself")),
            _ => {}
        }
        self.consts[index as usize].state = 1;
        let value = self.const_eval(expr)?;
        if let Some(ty) = ty {
            let declared = self.ty(ty)?;
            if declared != value.ty() {
                return err_expr(self.ast, expr,
                    format!("expected `{}` but this is `{}`", self.ty_name(declared), self.ty_name(value.ty())),
                );
            }
        }
        if let ConstValue::Int(number) = value {
            self.consts[index as usize].int = self.ints.len() as u32;
            self.ints.push(number);
        }
        self.consts[index as usize].value = value;
        self.consts[index as usize].state = 2;
        Ok(value)
    }

    /// Computes a constant expression: literals, other constants and
    /// operators on numbers and `bool`s, with the results the same
    /// operators give when the program runs.
    fn const_eval(&mut self, id: ExprId) -> SResult<ConstValue> {
        let pos = self.ast.expr_pos[id as usize];
        let wrong = |c: &Self, at: ExprId, what: &str, want: Ty, got: ConstValue| -> SResult<ConstValue> {
            err(c.ast.expr_pos[at as usize], format!("{what} needs `{}` but this is `{}`", c.ty_name(want), c.ty_name(got.ty())))
        };
        match self.ast.exprs[id as usize] {
            Expr::Int(span) => match int_literal(span.text(self.src)) {
                Ok(value) => Ok(ConstValue::Int(value)),
                Err(message) => err(pos, message),
            },
            Expr::Bool(value) => Ok(ConstValue::Bool(value)),
            Expr::Str(span) => Ok(ConstValue::Str(self.string_literal(span))),
            Expr::Ident(name) => {
                let text = name.text(self.src);
                match self.const_names.get(text) {
                    Some(&index) => self.const_value(index),
                    None => err(pos, format!("`{text}` is not a constant; a constant is made of literals and other constants")),
                }
            }
            Expr::Unary { op: UnOp::Neg, operand } if self.is_min_literal(operand) => Ok(ConstValue::Int(i64::MIN)),
            Expr::Unary { op, operand } => {
                let value = self.const_eval(operand)?;
                match (op, value) {
                    (UnOp::Neg, ConstValue::Int(number)) => Ok(ConstValue::Int(number.wrapping_neg())),
                    (UnOp::Not, ConstValue::Bool(flag)) => Ok(ConstValue::Bool(!flag)),
                    (UnOp::Neg, other) => wrong(self, operand, "`-`", Ty::I64, other),
                    (UnOp::Not, other) => wrong(self, operand, "`!`", Ty::Bool, other),
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let left = self.const_eval(lhs)?;
                let right = self.const_eval(rhs)?;
                let what = format!("`{}`", op.symbol());
                if let ConstValue::Str(first) = left {
                    // Two string constants joined into a new one.
                    if op != BinOp::Add {
                        return err(pos, "string constants can be joined with `+`; no other operator works on them in a constant");
                    }
                    let ConstValue::Str(second) = right else {
                        return wrong(self, rhs, &what, Ty::Str, right);
                    };
                    let offset = self.data.len() as u32;
                    for part in [first, second] {
                        let (start, len) = self.strings[part as usize];
                        self.data.extend_from_within(start as usize..(start + len) as usize);
                    }
                    self.strings.push((offset, self.data.len() as u32 - offset));
                    return Ok(ConstValue::Str(self.strings.len() as u32 - 1));
                }
                match op {
                    BinOp::And | BinOp::Or => {
                        let (ConstValue::Bool(a), ConstValue::Bool(b)) = (left, right) else {
                            return match left {
                                ConstValue::Bool(_) => wrong(self, rhs, &what, Ty::Bool, right),
                                _ => wrong(self, lhs, &what, Ty::Bool, left),
                            };
                        };
                        Ok(ConstValue::Bool(if op == BinOp::And { a && b } else { a || b }))
                    }
                    BinOp::Eq | BinOp::NotEq => {
                        if left.ty() != right.ty() {
                            return wrong(self, rhs, &what, left.ty(), right);
                        }
                        Ok(ConstValue::Bool((left == right) == (op == BinOp::Eq)))
                    }
                    _ => {
                        let (ConstValue::Int(a), ConstValue::Int(b)) = (left, right) else {
                            return match left {
                                ConstValue::Int(_) => wrong(self, rhs, &what, Ty::I64, right),
                                _ => wrong(self, lhs, &what, Ty::I64, left),
                            };
                        };
                        if matches!(op, BinOp::Div | BinOp::Rem) && b == 0 {
                            return err_expr(self.ast, rhs, "division by zero in a constant");
                        }
                        Ok(match op {
                            BinOp::Add => ConstValue::Int(a.wrapping_add(b)),
                            BinOp::Sub => ConstValue::Int(a.wrapping_sub(b)),
                            BinOp::Mul => ConstValue::Int(a.wrapping_mul(b)),
                            BinOp::Div => ConstValue::Int(a.wrapping_div(b)),
                            BinOp::Rem => ConstValue::Int(a.wrapping_rem(b)),
                            BinOp::BitAnd => ConstValue::Int(a & b),
                            BinOp::BitOr => ConstValue::Int(a | b),
                            BinOp::BitXor => ConstValue::Int(a ^ b),
                            // The low 6 bits of the count, as at run time.
                            BinOp::Shl => ConstValue::Int(a.wrapping_shl(b as u32)),
                            BinOp::Shr => ConstValue::Int(a.wrapping_shr(b as u32)),
                            BinOp::Lt => ConstValue::Bool(a < b),
                            BinOp::LtEq => ConstValue::Bool(a <= b),
                            BinOp::Gt => ConstValue::Bool(a > b),
                            BinOp::GtEq => ConstValue::Bool(a >= b),
                            BinOp::And | BinOp::Or | BinOp::Eq | BinOp::NotEq => unreachable!("handled above"),
                        })
                    }
                }
            }
            Expr::Call { callee, args } if self.is_embed(callee) => self.embed(id, args),
            _ => err(pos, "a constant must be made of literals and other constants"),
        }
    }

    /// Whether `callee` is the name `embed`, the one call a constant may
    /// contain.
    fn is_embed(&self, callee: ExprId) -> bool {
        matches!(self.ast.exprs[callee as usize], Expr::Ident(name) if name.text(self.src) == "embed")
    }

    /// `embed("path")` in a constant: the bytes of that file, read now,
    /// as a string constant. The path is relative to the folder of the
    /// source file the call is written in.
    fn embed(&mut self, id: ExprId, args: List) -> SResult<ConstValue> {
        let pos = self.ast.expr_pos[id as usize];
        let args = self.ast.list(args);
        if args.len() != 1 {
            return err(pos, format!("`embed` takes 1 argument but {} were given", args.len()));
        }
        let name = match self.const_eval(args[0])? {
            ConstValue::Str(index) => {
                let (start, len) = self.strings[index as usize];
                String::from_utf8_lossy(&self.data[start as usize..(start + len) as usize]).into_owned()
            }
            other => {
                return err(self.ast.expr_pos[args[0] as usize], format!("argument 1 of `embed` needs `str` but this is `{}`", self.ty_name(other.ty())));
            }
        };
        // The file this call is written in is the last one that starts
        // at or before it.
        let here = self.files.iter().rev().find(|&&(start, _)| start <= pos).map(|&(_, path)| path).unwrap_or("");
        let path = std::path::Path::new(here).parent().unwrap_or(std::path::Path::new("")).join(&name);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let offset = self.data.len() as u32;
                if bytes.len() as u64 + offset as u64 > MAX_EMBED {
                    return err(pos, format!("cannot embed `{name}`: the file is too large for now"));
                }
                self.data.extend_from_slice(&bytes);
                self.strings.push((offset, bytes.len() as u32));
                Ok(ConstValue::Str(self.strings.len() as u32 - 1))
            }
            Err(problem) => err(pos, format!("cannot embed `{name}`: {}", os_reason(&problem))),
        }
    }

    /// True for the digits `9223372036854775808`, which are a number
    /// only with a `-` in front: the smallest `i64`.
    fn is_min_literal(&self, id: ExprId) -> bool {
        match self.ast.exprs[id as usize] {
            Expr::Int(span) => {
                let text = span.text(self.src);
                text.len() >= 19 && text.bytes().filter(|&byte| byte != b'_').eq("9223372036854775808".bytes())
            }
            _ => false,
        }
    }

    fn block(&mut self, body: List) -> SResult<()> {
        let scope = self.locals.len();
        for &id in self.ast.list(body) {
            self.stmt(id)?;
        }
        self.close_scope(scope);
        Ok(())
    }

    /// Ends the scope of the variables declared since `scope`. A store
    /// that was the last mention of a variable takes its buffers over.
    fn close_scope(&mut self, scope: usize) {
        for at in scope..self.locals.len() {
            let local = &self.locals[at];
            if let Some(id) = local.candidate {
                self.expr_moved[self.sh.e(id)] = true;
            }
            if local.scalar && !local.addressed {
                self.wishes.push((local.slot, local.uses));
            }
        }
        self.locals.truncate(scope);
    }

    /// True when `name` is written anywhere inside expression `id`.
    fn mentions(&self, id: ExprId, name: &str) -> bool {
        match self.ast.exprs[id as usize] {
            Expr::Ident(span) => span.text(self.src) == name,
            Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) => false,
            Expr::Unary { operand, .. } => self.mentions(operand, name),
            Expr::Binary { lhs, rhs, .. } => self.mentions(lhs, name) || self.mentions(rhs, name),
            Expr::Call { callee, args } => {
                self.mentions(callee, name) || self.ast.list(args).iter().any(|&arg| self.mentions(arg, name))
            }
            Expr::MutArg(inner) | Expr::Try(inner) => self.mentions(inner, name),
            Expr::Field { base, .. } => self.mentions(base, name),
            Expr::Index { base, index } => self.mentions(base, name) || self.mentions(index, name),
            Expr::StructLit { fields, .. } => {
                self.ast.list(fields).iter().any(|&init| self.mentions(self.ast.field_inits[init as usize].value, name))
            }
            Expr::ArrayLit { items } => self.ast.list(items).iter().any(|&item| self.mentions(item, name)),
        }
    }

    /// Called where the value of `id` is stored in a new owner. Like
    /// `take`, and in addition: a whole variable that owns its value may
    /// hand its buffers over instead of being copied, if it turns out
    /// that it is never mentioned again. That is safe only when nothing
    /// can run the store twice (a loop that started after the variable
    /// was declared), nothing is looking into the variable, and nothing
    /// written earlier reads it later.
    fn store_from(&mut self, id: ExprId) -> bool {
        if self.take(id) {
            return true;
        }
        let Expr::Ident(name) = self.ast.exprs[id as usize] else {
            return false;
        };
        let text = name.text(self.src);
        let Some(at) = self.locals.iter().rposition(|local| local.name == text) else {
            return false;
        };
        let local = &self.locals[at];
        if !local.owns || local.by_ref || local.depth != self.loop_depth || !self.needs_drop(local.ty) {
            return false;
        }
        let place = Place { root: local.slot, start: 0, len: self.size_of(local.ty), mutable: false, is_param: false, is_view: false, decl: u32::MAX };
        let watched = self.borrows.iter().any(|other| overlaps(&place, &other.place));
        if !watched && !self.late.iter().any(|&late| self.mentions(late, text)) {
            self.locals[at].candidate = Some(id);
        }
        false
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
        let literal = match self.ast.exprs[id as usize] {
            Expr::Str(_) => true,
            Expr::Ident(_) => self.expr_aux[self.sh.e(id)] == CONST_STR,
            _ => false,
        };
        if literal {
            // A literal owns nothing, so there is nothing to copy either.
            self.expr_moved[self.sh.e(id)] = true;
            return true;
        }
        match self.pending.iter().position(|&(expr, ..)| expr == id) {
            Some(at) => {
                self.pending.remove(at);
                self.expr_moved[self.sh.e(id)] = true;
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
            for (offset, leaf) in self.owned(ty) {
                self.zero_slots.push(slot + offset + idle_slot(leaf));
            }
            self.drops.push((slot, ty));
        }
        let len = self.drops.len() as u32 - start;
        self.stmt_drops[self.sh.s(id)] = (start, len);
        // A `?` that leaves the function frees the same temporaries.
        for at in std::mem::take(&mut self.tries) {
            self.expr_res[self.sh.e(at)] = start;
            self.expr_aux[self.sh.e(at)] |= len << 1;
        }
    }

    /// Notes that `value` was stored in a struct literal or a variant
    /// that is still being put together.
    fn sibling(&mut self, value: ExprId) {
        if self.needs_drop(self.expr_ty[self.sh.e(value)]) {
            self.owned_siblings += 1;
        }
    }

    /// `value?`: what `Ok` or `Some` holds, or an early return of the
    /// `Err` or the `None`.
    fn try_expr(&mut self, id: ExprId, inner: ExprId) -> SResult<Ty> {
        let pos = self.ast.expr_pos[id as usize];
        let inner_ty = self.expr(inner)?;
        let index = match inner_ty {
            Ty::Enum(index) if self.enums[index as usize].template == OPTION || self.enums[index as usize].template == RESULT => index,
            other => return err_expr(self.ast, inner, format!("`?` needs a `Result` or an `Option` but this is `{}`", self.ty_name(other))),
        };
        let template = self.enums[index as usize].template;
        let fits = match self.ret {
            Ty::Enum(ret) => {
                let ret = &self.enums[ret as usize];
                ret.template == template && (template == OPTION || ret.args[1] == self.enums[index as usize].args[1])
            }
            _ => false,
        };
        if !fits {
            let wanted = if template == OPTION {
                "an `Option`".to_string()
            } else {
                format!("a `Result` with the error type `{}`", self.ty_name(self.enums[index as usize].args[1]))
            };
            let returns = match self.ret {
                Ty::Unit => "nothing".to_string(),
                ret => format!("`{}`", self.ty_name(ret)),
            };
            return err(pos, format!("`?` on `{}` needs a function that returns {wanted}, but this function returns {returns}; use `match`", self.ty_name(inner_ty)));
        }
        // Leaving in the middle of a list literal would lose its buffer,
        // and leaving after a part of a struct or variant was stored
        // would lose what that part owns.
        if self.in_list > 0 {
            return err(pos, "`?` cannot be used inside a list literal; store its value in a variable first");
        }
        if self.owned_siblings > 0 {
            return err(pos, "`?` cannot be used after a part of the same value that owns memory; store its value in a variable first");
        }
        let value_ty = self.enums[index as usize].args[0];
        // A temporary gives what it holds away; anything else is copied.
        let from_temp = self.pending.iter().any(|&(expr, ..)| expr == inner);
        self.expr_aux[self.sh.e(id)] = from_temp as u32;
        self.tries.push(id);
        if value_ty.is_aggregate() {
            self.expr_tmp[self.sh.e(id)] = self.temp(id, value_ty)?;
        }
        Ok(value_ty)
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
                    return err_expr(self.ast, value, "this expression has no value to store");
                }
                if let Some(declared) = declared {
                    if declared != value_ty {
                        return err_expr(self.ast, value,
                            format!("expected `{}` but this is `{}`", self.ty_name(declared), self.ty_name(value_ty)),
                        );
                    }
                }
                self.store_from(value);
                self.finish(id);
                let slot = self.alloc(self.size_of(value_ty), name.start)?;
                self.locals.push(Local {
                    name: self.binding(name)?,
                    slot,
                    ty: value_ty,
                    mutable,
                    by_ref: false,
                    is_param: false,
                    is_view: false,
                    owns: true,
                    depth: self.loop_depth,
                    candidate: None,
                    scalar: matches!(value_ty, Ty::I64 | Ty::Bool),
                    uses: 0,
                    addressed: false,
                    decl: pos,
                });
                self.stmt_slot[self.sh.s(id)] = slot;
            }
            Stmt::Assign { target, value } => {
                let target_ty = self.expr(target)?;
                if let Expr::Index { base, .. } = self.ast.exprs[target as usize] {
                    if self.expr_ty[self.sh.e(base)] == Ty::Str {
                        return err(pos, "a string cannot be changed in place; build a new one with `slice` and `+`");
                    }
                }
                let Some(place) = self.place(target) else {
                    if let Some(name) = self.constant_name(target) {
                        return err(pos, format!("cannot change `{name}`: it is a constant"));
                    }
                    return err(pos, "only a variable or a part of a variable can be assigned");
                };
                if !place.mutable {
                    let message = format!("cannot change `{}`: {}", self.root_name(target), read_only(&place));
                    return err(pos, message).map_err(|error: SemaError| error.with_fix(self.var_fix(&place)));
                }
                self.check_change(target, &place)?;
                // The target is found after the value has been computed.
                self.late.push(target);
                let value_ty = self.expr_hint(value, Some(target_ty))?;
                if value_ty != target_ty {
                    return err_expr(self.ast, value,
                        format!("expected `{}` but this is `{}`", self.ty_name(target_ty), self.ty_name(value_ty)),
                    );
                }
                if let (Ty::Str, Expr::Binary { op: BinOp::Add, lhs, .. }) = (target_ty, self.ast.exprs[value as usize]) {
                    // `s = s + more`: add to `s` where it is. The right
                    // side cannot change `s`, so the result is the same.
                    let same = match self.place(lhs) {
                        Some(left) => left.root == place.root && left.start == place.start,
                        None => false,
                    };
                    if same && !self.contains_index(target) && !self.contains_index(lhs) {
                        self.late.pop();
                        self.pending.retain(|&(expr, ..)| expr != value);
                        self.stmt_flag[self.sh.s(id)] = APPEND;
                        self.finish(id);
                        return Ok(());
                    }
                }
                let owning = self.needs_drop(target_ty);
                let moved = owning && self.store_from(value);
                self.late.pop();
                if target_ty.is_aggregate() && !moved && (owning || self.contains_index(target)) {
                    // The value is copied aside first. That lets `a = a`
                    // read the old value before it is freed, and keeps the
                    // copy safe while a list index on the left is computed.
                    self.stmt_slot[self.sh.s(id)] = self.alloc(self.size_of(target_ty), pos)?;
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
                            if self.lookup(name.text(self.src)).is_some_and(|local| local.owns) {
                                self.expr_moved[self.sh.e(value)] = true;
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
                // The condition runs on every round, like the body.
                self.loop_depth += 1;
                self.condition(cond)?;
                self.finish(id);
                self.block(body)?;
                self.loop_depth -= 1;
            }
            Stmt::For { index, item, iter, body } => {
                let iter_ty = self.expr(iter)?;
                let Ty::List(list) = iter_ty else {
                    return err_expr(self.ast, iter,
                        format!("`for` needs a list to go through but this is `{}`", self.ty_name(iter_ty)),
                    );
                };
                let elem = self.lists[list as usize];
                let place = self.place(iter);
                self.stmt_flag[self.sh.s(id)] = if self.take(iter) {
                    MOVED
                } else if place.is_some() {
                    IN_PLACE
                } else {
                    COPIED
                };
                self.finish(id);
                // Hidden slots: the position, then room for a list the
                // loop owns, then the loop variables.
                let base = self.alloc(4, pos)?;
                self.stmt_slot[self.sh.s(id)] = base;
                let scope = self.locals.len();
                let item_slot = self.alloc(self.size_of(elem), item.start)?;
                // The loop variable looks at the element; the list keeps owning it.
                self.locals.push(Local { name: self.binding(item)?, slot: item_slot, ty: elem, mutable: false, by_ref: false, is_param: false, is_view: true, owns: false, depth: self.loop_depth, candidate: None, scalar: false, uses: 0, addressed: false, decl: u32::MAX });
                if let Some(index) = index {
                    let index_slot = self.alloc(1, index.start)?;
                    self.locals.push(Local { name: self.binding(index)?, slot: index_slot, ty: Ty::I64, mutable: false, by_ref: false, is_param: false, is_view: true, owns: false, depth: self.loop_depth, candidate: None, scalar: false, uses: 0, addressed: false, decl: u32::MAX });
                }
                let borrows = self.borrows.len();
                if self.stmt_flag[self.sh.s(id)] == IN_PLACE {
                    let place = place.expect("in-place iteration has a place");
                    self.borrows.push(Borrow { place, mutable: false, holder: Holder::Loop });
                }
                self.loop_depth += 1;
                self.block(body)?;
                self.loop_depth -= 1;
                self.borrows.truncate(borrows);
                self.close_scope(scope);
            }
            Stmt::Break | Stmt::Continue => {
                if self.loop_depth == 0 {
                    return err(pos, "`break` and `continue` can only be used inside a loop");
                }
            }
            Stmt::Match { scrutinee, arms } => self.match_stmt(id, scrutinee, arms)?,
        }
        Ok(())
    }

    /// Checks a `match`. The value is looked at where it lives when it is
    /// a variable (which then cannot change inside the arms), and is
    /// owned by the statement when it was computed. Names bound by a
    /// pattern are read-only views into it.
    fn match_stmt(&mut self, id: StmtId, scrutinee: ExprId, arms: List) -> SResult<()> {
        let pos = self.ast.stmt_pos[id as usize];
        let at = self.ast.expr_pos[scrutinee as usize];
        let ty = self.expr(scrutinee)?;
        match ty {
            Ty::Unit => return err(at, "this expression has no value to match on"),
            Ty::Struct(_) | Ty::List(_) => {
                return err(at, format!("`match` works on enums, `i64`, `bool` and `str` but this is `{}`", self.ty_name(ty)));
            }
            Ty::I64 | Ty::Bool | Ty::Str | Ty::Enum(_) => {}
        }
        let aggregate = ty.is_aggregate();
        let place = if aggregate { self.place(scrutinee) } else { None };
        let mode = if !aggregate {
            COPIED
        } else if self.take(scrutinee) {
            MOVED
        } else if place.is_some() {
            IN_PLACE
        } else {
            COPIED
        };
        self.stmt_flag[self.sh.s(id)] = mode;
        self.finish(id);
        // Hidden slots: the value itself (a number or `bool`) or its
        // address, then room for a value the statement owns.
        let base = self.alloc(1, pos)?;
        if aggregate && mode != IN_PLACE {
            self.alloc(self.size_of(ty), pos)?;
        }
        self.stmt_slot[self.sh.s(id)] = base;
        let outer_borrows = self.borrows.len();
        if let (IN_PLACE, Some(place)) = (mode, place) {
            self.borrows.push(Borrow { place, mutable: false, holder: Holder::Match });
        }

        let variant_count = match ty {
            Ty::Enum(index) => self.enums[index as usize].variants.len(),
            _ => 0,
        };
        // What the arms so far cover.
        let mut seen_variants = vec![false; variant_count];
        let mut seen_bools = [false; 2];
        let mut seen_ints: Vec<i64> = Vec::new();
        let mut seen_strings: Vec<u32> = Vec::new();
        let mut catch_all = false;
        let mut literal_slot = None;

        for &arm_id in self.ast.list(arms) {
            let arm = self.ast.arms[arm_id as usize];
            let pat = arm.pattern as usize;
            let pat_pos = self.ast.pat_pos[pat];
            let complete = catch_all
                || match ty {
                    Ty::Enum(_) => seen_variants.iter().all(|&seen| seen),
                    Ty::Bool => seen_bools == [true, true],
                    _ => false,
                };
            if complete {
                return err(pat_pos, "this arm can never run: the arms above already cover every case");
            }
            let scope = self.locals.len();
            // A pattern that stands for one value: a literal, or the name
            // of a constant. With it, how the value is written.
            let literal: Option<(ConstValue, String)> = match self.ast.patterns[pat] {
                Pattern::Int { negative, digits } => {
                    let written = digits.text(self.src).as_bytes();
                    if written[0] != b'\'' {
                        let prefixed = written.len() > 1 && matches!(written[1], b'x' | b'b');
                        if let Err(message) = number_fault(written, if prefixed { 2 } else { 0 }) {
                            return err(pat_pos, message);
                        }
                    }
                    let mut value: i128 = 0;
                    match special_literal(digits.text(self.src)) {
                        Some(Some(special)) => value = special as i128,
                        Some(None) => return err(pat_pos, "this number does not fit in 64 bits"),
                        None => {
                            for byte in digits.text(self.src).bytes().filter(|&b| b != b'_') {
                                value = value * 10 + (byte - b'0') as i128;
                                if value > 1 << 63 {
                                    break;
                                }
                            }
                        }
                    }
                    if negative {
                        value = -value;
                    }
                    let Ok(value) = i64::try_from(value) else {
                        return err(pat_pos, "this number does not fit in `i64`");
                    };
                    Some((ConstValue::Int(value), format!("`{value}`")))
                }
                Pattern::Bool(value) => Some((ConstValue::Bool(value), format!("`{value}`"))),
                Pattern::Str(span) => Some((ConstValue::Str(self.string_literal(span)), span.text(self.src).to_string())),
                Pattern::Name(name) => self.constant(name).map(|index| {
                    (self.consts[index as usize].value, format!("`{}`", name.text(self.src)))
                }),
                _ => None,
            };
            if let Some((value, shown)) = literal {
                let kind = match value {
                    ConstValue::Int(_) => "a number",
                    ConstValue::Bool(_) => "a `bool`",
                    ConstValue::Str(_) => "a string",
                };
                if ty != value.ty() {
                    return err(pat_pos, format!("this pattern is {kind} but the `match` is on `{}`", self.ty_name(ty)));
                }
                let seen_before = match value {
                    ConstValue::Int(number) => {
                        let seen = seen_ints.contains(&number);
                        seen_ints.push(number);
                        seen
                    }
                    ConstValue::Bool(flag) => std::mem::replace(&mut seen_bools[flag as usize], true),
                    ConstValue::Str(index) => {
                        let bytes = |c: &Self, index: u32| {
                            let (offset, len) = c.strings[index as usize];
                            c.data[offset as usize..(offset + len) as usize].to_vec()
                        };
                        let text = bytes(self, index);
                        let seen = seen_strings.iter().any(|&earlier| bytes(self, earlier) == text);
                        seen_strings.push(index);
                        seen
                    }
                };
                if seen_before {
                    return err(pat_pos, format!("this arm can never run: {shown} is already handled above"));
                }
                self.pats[self.sh.p(arm.pattern)] = match value {
                    ConstValue::Int(number) => {
                        self.ints.push(number);
                        PatInfo { kind: PAT_LITERAL, res: self.ints.len() as u32 - 1, slot: 0 }
                    }
                    ConstValue::Bool(flag) => PatInfo { kind: PAT_LITERAL, res: flag as u32, slot: 0 },
                    ConstValue::Str(index) => {
                        // One area is enough: the strings are compared one at a time.
                        let slot = match literal_slot {
                            Some(slot) => slot,
                            None => {
                                let slot = self.alloc(3, pat_pos)?;
                                literal_slot = Some(slot);
                                slot
                            }
                        };
                        PatInfo { kind: PAT_LITERAL, res: index, slot }
                    }
                };
                self.block(arm.body)?;
                self.close_scope(scope);
                continue;
            }
            match self.ast.patterns[pat] {
                Pattern::Wildcard => {
                    self.pats[self.sh.p(arm.pattern)] = PatInfo { kind: PAT_ANY, res: 0, slot: 0 };
                    catch_all = true;
                }
                Pattern::Name(name) if !self.variant_names.contains_key(name.text(self.src)) => {
                    // A plain name matches anything and stands for the whole value.
                    let slot = self.alloc(1, name.start)?;
                    self.locals.push(Local {
                        name: name.text(self.src),
                        slot,
                        ty,
                        mutable: false,
                        by_ref: aggregate,
                        is_param: false,
                        is_view: true,
                        owns: false,
                        depth: self.loop_depth,
                        candidate: None,
                        scalar: false,
                        uses: 0,
                        addressed: false,
                        decl: u32::MAX,
                    });
                    self.pats[self.sh.p(arm.pattern)] = PatInfo { kind: PAT_BIND, res: 0, slot };
                    catch_all = true;
                }
                Pattern::Name(name) | Pattern::Variant { name, .. } => {
                    let text = name.text(self.src);
                    let Some(&(template, variant)) = self.variant_names.get(text) else {
                        return err(pat_pos, format!("unknown variant `{text}`"));
                    };
                    let index = match ty {
                        Ty::Enum(index) if self.enums[index as usize].template == template => index as usize,
                        _ => {
                            return err(
                                pat_pos,
                                format!(
                                    "`{text}` is a variant of `{}` but this `match` is on `{}`",
                                    self.templates[template as usize].name,
                                    self.ty_name(ty)
                                ),
                            );
                        }
                    };
                    let want = self.enums[index].variants[variant as usize].fields.len();
                    let plural = if want == 1 { "" } else { "s" };
                    let sub_patterns = match self.ast.patterns[pat] {
                        Pattern::Variant { args, .. } => {
                            if want == 0 {
                                return err(pat_pos, format!("`{text}` holds no value; write it without `()`"));
                            }
                            self.ast.list(args)
                        }
                        _ => {
                            if want > 0 {
                                let fields = vec!["_"; want].join(", ");
                                let fix = Fix::AfterName { at: pat_pos, text: format!("({fields})") };
                                return err(pat_pos, format!("`{text}` holds {want} value{plural}; write `{text}({fields})` to match it"))
                                    .map_err(|error: SemaError| error.with_fix(Some(fix)));
                            }
                            &[]
                        }
                    };
                    if sub_patterns.len() != want {
                        return err(pat_pos, format!("`{text}` holds {want} value{plural} but this pattern lists {}", sub_patterns.len()));
                    }
                    if seen_variants[variant as usize] {
                        return err(pat_pos, format!("this arm can never run: `{text}` is already handled above"));
                    }
                    seen_variants[variant as usize] = true;
                    for (i, &sub) in sub_patterns.iter().enumerate() {
                        match self.ast.patterns[sub as usize] {
                            Pattern::Wildcard => {}
                            Pattern::Name(binding)
                                if !self.variant_names.contains_key(binding.text(self.src))
                                    && self.constant(binding).is_none() =>
                            {
                                let binding_text = binding.text(self.src);
                                if self.locals[scope..].iter().any(|local| local.name == binding_text) {
                                    return err(binding.start, format!("`{binding_text}` is used twice in this pattern"));
                                }
                                let (field, offset) = self.enums[index].variants[variant as usize].fields[i];
                                // The name refers to the field where it is.
                                let slot = self.alloc(1, binding.start)?;
                                self.locals.push(Local {
                                    name: binding_text,
                                    slot,
                                    ty: field,
                                    mutable: false,
                                    by_ref: true,
                                    is_param: false,
                                    is_view: true,
                                    owns: false,
                                    depth: self.loop_depth,
                                    candidate: None,
                                    scalar: false,
                                    uses: 0,
                                    addressed: false,
                                    decl: u32::MAX,
                                });
                                self.pats[self.sh.p(sub)] = PatInfo { kind: PAT_BIND, res: offset, slot };
                            }
                            _ => {
                                return err(
                                    self.ast.pat_pos[sub as usize],
                                    "a pattern inside a pattern is not supported yet; give the value a name and `match` on it inside the arm",
                                );
                            }
                        }
                    }
                    self.pats[self.sh.p(arm.pattern)] = PatInfo { kind: PAT_VARIANT, res: variant, slot: 0 };
                }
                Pattern::Int { .. } | Pattern::Bool(_) | Pattern::Str(_) => unreachable!("literal patterns are handled above"),
            }
            self.block(arm.body)?;
            self.close_scope(scope);
        }
        self.borrows.truncate(outer_borrows);

        if catch_all {
            return Ok(());
        }
        match ty {
            Ty::Enum(index) => {
                let template = &self.templates[self.enums[index as usize].template as usize];
                let missing: Vec<String> = seen_variants
                    .iter()
                    .enumerate()
                    .filter(|&(_, &seen)| !seen)
                    .map(|(variant, _)| format!("`{}`", template.variants[variant].0))
                    .collect();
                // The arms that would complete the `match`, doing nothing.
                let arms: Vec<String> = seen_variants
                    .iter()
                    .enumerate()
                    .filter(|&(_, &seen)| !seen)
                    .map(|(variant, _)| {
                        let (name, fields) = &template.variants[variant];
                        match fields.len() {
                            0 => format!("{name} => {{}}"),
                            count => format!("{name}({}) => {{}}", vec!["_"; count].join(", ")),
                        }
                    })
                    .collect();
                let fix = Some(Fix::MatchArms { stmt: pos, arms });
                match missing.split_last() {
                    None => Ok(()),
                    Some((only, [])) => err(pos, format!("this `match` does not handle {only}; add an arm for it or a final `_ =>` arm"))
                        .map_err(|error: SemaError| error.with_fix(fix)),
                    Some((last, rest)) => err(
                        pos,
                        format!("this `match` does not handle {} and {last}; add arms for them or a final `_ =>` arm", rest.join(", ")),
                    )
                    .map_err(|error: SemaError| error.with_fix(fix)),
                }
            }
            Ty::Bool if seen_bools == [true, true] => Ok(()),
            Ty::Bool => {
                let missing = !seen_bools[1];
                let fix = Fix::MatchArms { stmt: pos, arms: vec![format!("{missing} => {{}}")] };
                err(pos, format!("this `match` does not handle `{missing}`; add an arm for it or a final `_ =>` arm"))
                    .map_err(|error: SemaError| error.with_fix(Some(fix)))
            }
            _ => {
                let fix = Fix::MatchArms { stmt: pos, arms: vec!["_ => {}".to_string()] };
                err(pos, format!("a `match` on `{}` needs a final `_ =>` arm: the arms cannot list every value", self.ty_name(ty)))
                    .map_err(|error: SemaError| error.with_fix(Some(fix)))
            }
        }
    }

    fn condition(&mut self, cond: ExprId) -> SResult<()> {
        let ty = self.expr(cond)?;
        if ty != Ty::Bool {
            return err_expr(self.ast, cond, format!("a condition must be `bool` but this is `{}`", self.ty_name(ty)));
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
                Some(Place {
                    root: local.slot,
                    start: 0,
                    len: self.size_of(local.ty),
                    mutable: local.mutable,
                    is_param: local.is_param,
                    is_view: local.is_view,
                    decl: local.decl,
                })
            }
            Expr::Field { base, .. } => {
                let mut place = self.place(base)?;
                if self.contains_index(base) {
                    return Some(place);
                }
                place.start += self.expr_res[self.sh.e(id)];
                place.len = self.size_of(self.expr_ty[self.sh.e(id)]);
                Some(place)
            }
            // A byte of a string is a number read from it, not a place.
            Expr::Index { base, .. } if self.expr_ty[self.sh.e(base)] == Ty::Str => None,
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

    /// The repair for changing a variable declared with `let`: declare it
    /// with `var`. `None` for parameters, loop variables and bindings,
    /// which have no such one-word repair.
    fn var_fix(&self, place: &Place) -> Option<Fix> {
        let start = place.decl as usize;
        if place.is_param || place.is_view || place.decl == u32::MAX || !self.src[start..].starts_with("let ") {
            return None;
        }
        Some(Fix::Replace { start: place.decl, end: place.decl + 3, text: "var".to_string() })
    }

    /// Fails when `place` cannot be changed right now because something
    /// else still refers to it.
    fn check_change(&self, id: ExprId, place: &Place) -> SResult<()> {
        for other in &self.borrows {
            if overlaps(place, &other.place) {
                let name = self.root_name(id);
                return err_expr(self.ast, id,
                    match other.holder {
                        Holder::Loop => format!("cannot change `{name}` while a loop is going through it"),
                        Holder::Match => format!("cannot change `{name}` while a `match` is looking inside it"),
                        Holder::Call => format!("cannot change `{name}` here: it is also passed to a function in the same expression"),
                        Holder::Operator => format!("cannot change `{name}` here: it is still being read earlier in the same expression"),
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
                return err_expr(self.ast, id,
                    match other.holder {
                        Holder::Loop => format!("cannot pass `{name}` as `mut` while a loop is going through it"),
                        Holder::Match => format!("cannot pass `{name}` as `mut` while a `match` is looking inside it"),
                        Holder::Call => format!("`{name}` is passed twice in one call and one of the uses is `mut`; copy it into a variable first"),
                        Holder::Operator => format!("cannot pass `{name}` as `mut` here: it is still being read earlier in the same expression"),
                    },
                );
            }
        }
        self.borrows.push(Borrow { place, mutable, holder: Holder::Call });
        Ok(())
    }

    // ---- expressions ---------------------------------------------------

    /// Checks that `expr` has type `want`. The description of what needs
    /// that type is only put into words when the check fails.
    fn expect(&mut self, expr: ExprId, want: Ty, what: What) -> SResult<()> {
        let ty = self.expr_hint(expr, Some(want))?;
        if ty != want {
            let what = match what {
                What::Text(text) => text.to_string(),
                What::Operator(op) => format!("`{}`", op.symbol()),
                What::Argument(number, function) => format!("argument {number} of `{function}`"),
                What::Field(name) => format!("field `{name}`"),
            };
            return err_expr(self.ast, expr,
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
        self.expr_ty[self.sh.e(id)] = ty;
        Ok(ty)
    }

    fn expr_inner(&mut self, id: ExprId, hint: Option<Ty>) -> SResult<Ty> {
        let pos = self.ast.expr_pos[id as usize];
        match self.ast.exprs[id as usize] {
            Expr::Int(span) => {
                let value = match int_literal(span.text(self.src)) {
                    Ok(value) => value,
                    Err(message) => return err(pos, message),
                };
                self.expr_res[self.sh.e(id)] = self.ints.len() as u32;
                self.ints.push(value);
                Ok(Ty::I64)
            }
            Expr::Bool(_) => Ok(Ty::Bool),
            Expr::Try(inner) => self.try_expr(id, inner),
            Expr::Str(span) => {
                self.expr_res[self.sh.e(id)] = self.string_literal(span);
                self.expr_tmp[self.sh.e(id)] = self.alloc(3, pos)?;
                Ok(Ty::Str)
            }
            Expr::Ident(name) => {
                let text = name.text(self.src);
                let Some(at) = self.locals.iter().rposition(|local| local.name == text) else {
                    if let Some((template, variant)) = self.variant(text) {
                        return self.construct(id, template, variant, None, hint);
                    }
                    if let Some(index) = self.constant(name) {
                        // From here on the name reads as the literal.
                        let (value, int) = (self.consts[index as usize].value, self.consts[index as usize].int);
                        let at = self.sh.e(id);
                        match value {
                            ConstValue::Int(_) => {
                                self.expr_res[at] = int;
                                self.expr_aux[at] = CONST_INT;
                            }
                            ConstValue::Bool(flag) => self.expr_aux[at] = if flag { CONST_TRUE } else { CONST_FALSE },
                            ConstValue::Str(string) => {
                                self.expr_res[at] = string;
                                self.expr_aux[at] = CONST_STR;
                                self.expr_tmp[at] = self.alloc(3, pos)?;
                            }
                        }
                        return Ok(value.ty());
                    }
                    return if self.fn_names.contains_key(text) || self.generic_names.contains_key(text) || BUILTIN_FUNCTIONS.contains(&text) {
                        err(pos, format!("`{text}` is a function; call it with `{text}(...)`"))
                    } else if self.struct_names.contains_key(text) {
                        err(pos, format!("`{text}` is a struct; create a value with `{text} {{ ... }}`"))
                    } else if self.template_names.contains_key(text) {
                        err(pos, format!("`{text}` is an enum; create a value with one of its variants"))
                    } else {
                        err(pos, format!("unknown variable `{text}`"))
                    };
                };
                let weight = 1 << (3 * self.loop_depth.min(6));
                let local = &mut self.locals[at];
                // Mentioned again: an earlier store was not its last use.
                local.candidate = None;
                local.uses = local.uses.saturating_add(weight);
                let (slot, ty, by_ref) = (local.slot, local.ty, local.by_ref);
                self.expr_res[self.sh.e(id)] = slot | if by_ref { BY_REF } else { 0 };
                Ok(ty)
            }
            Expr::Unary { op, operand } => match op {
                UnOp::Neg if self.is_min_literal(operand) => {
                    // The smallest number has no positive twin to write
                    // down; negating it gives it back.
                    self.expr_res[self.sh.e(operand)] = self.ints.len() as u32;
                    self.ints.push(i64::MIN);
                    self.expr_ty[self.sh.e(operand)] = Ty::I64;
                    Ok(Ty::I64)
                }
                UnOp::Neg => {
                    self.expect(operand, Ty::I64, What::Text("`-`"))?;
                    Ok(Ty::I64)
                }
                UnOp::Not => {
                    self.expect(operand, Ty::Bool, What::Text("`!`"))?;
                    Ok(Ty::Bool)
                }
            },
            Expr::Binary { op, lhs, rhs } => {
                let what = What::Operator(op);
                match op {
                    BinOp::Add => {
                        let left = self.expr(lhs)?;
                        if left != Ty::Str {
                            if left != Ty::I64 {
                                return err_expr(self.ast, lhs,
                                    format!("`+` needs `i64` or `str` but this is `{}`", self.ty_name(left)),
                                );
                            }
                            self.expect(rhs, Ty::I64, what)?;
                            return Ok(Ty::I64);
                        }
                        self.string_operands(lhs, rhs, what)?;
                        // Joining makes a new string, which this temporary owns.
                        self.expr_tmp[self.sh.e(id)] = self.temp(id, Ty::Str)?;
                        Ok(Ty::Str)
                    }
                    BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Rem
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::Shl
                    | BinOp::Shr => {
                        self.expect(lhs, Ty::I64, what)?;
                        self.expect(rhs, Ty::I64, what)?;
                        Ok(Ty::I64)
                    }
                    BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => {
                        // Strings are ordered byte by byte; a string
                        // that another one starts with comes first.
                        let left = self.expr_hint(lhs, Some(Ty::I64))?;
                        if left == Ty::Str {
                            self.string_operands(lhs, rhs, what)?;
                            return Ok(Ty::Bool);
                        }
                        if left != Ty::I64 {
                            return err_expr(self.ast, lhs,
                                format!("`{}` needs `i64` but this is `{}`", op.symbol(), self.ty_name(left)),
                            );
                        }
                        self.expect(rhs, Ty::I64, what)?;
                        Ok(Ty::Bool)
                    }
                    BinOp::And | BinOp::Or => {
                        self.expect(lhs, Ty::Bool, what)?;
                        self.expect(rhs, Ty::Bool, what)?;
                        Ok(Ty::Bool)
                    }
                    BinOp::Eq | BinOp::NotEq => {
                        let left = self.expr(lhs)?;
                        match left {
                            Ty::Unit => return err_expr(self.ast, lhs, "this expression has no value to compare"),
                            Ty::Struct(_) => return err(pos, format!("structs cannot be compared with `{}` yet; compare their fields", op.symbol())),
                            Ty::Str => {
                                self.string_operands(lhs, rhs, what)?;
                                return Ok(Ty::Bool);
                            }
                            Ty::List(_) => return err(pos, format!("lists cannot be compared with `{}` yet", op.symbol())),
                            // Without values in any variant, the tags say it all.
                            Ty::Enum(index) if self.enums[index as usize].size == 1 => {}
                            Ty::Enum(_) => {
                                return err(
                                    pos,
                                    format!(
                                        "`{}` works only on enums whose variants hold no values; use `match` for `{}`",
                                        op.symbol(),
                                        self.ty_name(left)
                                    ),
                                );
                            }
                            Ty::I64 | Ty::Bool => {}
                        }
                        self.expect(rhs, left, what)?;
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
                        } else if matches!(base_ty, Ty::Enum(_)) {
                            format!("`{}` has no fields; use `match` to look inside it", self.ty_name(base_ty))
                        } else {
                            format!("`{}` has no fields", self.ty_name(base_ty))
                        },
                    );
                };
                let info = &self.structs[index as usize];
                let Some(field) = info.fields.iter().find(|f| f.name == field_name) else {
                    return err(name.start, format!("struct `{}` has no field `{field_name}`", info.name));
                };
                self.expr_res[self.sh.e(id)] = field.offset;
                Ok(field.ty)
            }
            Expr::Index { base, index } => {
                let base_ty = self.expr(base)?;
                let elem = match base_ty {
                    Ty::List(list) => self.lists[list as usize],
                    // A string gives its bytes, as numbers from 0 to 255.
                    Ty::Str => Ty::I64,
                    _ => return err(pos, format!("only a list or a string can be indexed but this is `{}`", self.ty_name(base_ty))),
                };
                // The index is computed before the list is looked at.
                self.late.push(base);
                self.expect(index, Ty::I64, What::Text("an index"))?;
                self.late.pop();
                Ok(elem)
            }
            Expr::ArrayLit { items } => {
                let item_ids = self.ast.list(items);
                let hinted = match hint {
                    Some(Ty::List(list)) => Some(self.lists[list as usize]),
                    _ => None,
                };
                self.in_list += 1;
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
                for (i, &item) in item_ids.iter().enumerate() {
                    if i > 0 || hinted.is_some() {
                        self.expect(item, elem, What::Text("this list's element"))?;
                    }
                    // The list takes a temporary's buffers over.
                    self.store_from(item);
                }
                self.in_list -= 1;
                let ty = self.list_of(elem);
                self.expr_tmp[self.sh.e(id)] = self.temp(id, ty)?;
                Ok(ty)
            }
            Expr::StructLit { name, fields } => self.struct_literal(id, name, fields, hint),
            Expr::Call { callee, args } => self.call(id, callee, args, hint),
            Expr::MutArg(inner) => {
                let fix = Fix::Replace { start: pos, end: self.ast.expr_pos[inner as usize], text: String::new() };
                err(pos, "`mut` can only be used on a function argument").map_err(|error: SemaError| error.with_fix(Some(fix)))
            }
            Expr::Float(_) => err(pos, "there are no floating-point numbers yet; the only number type is `i64`"),
        }
    }

    /// Checks the right operand of a string `+`, `==` or `!=` whose left
    /// operand is already checked. The left string is read after the
    /// right side has run, so it must not change in between.
    fn string_operands(&mut self, lhs: ExprId, rhs: ExprId, what: What) -> SResult<()> {
        let outer = self.borrows.len();
        if let Some(place) = self.place(lhs) {
            self.borrows.push(Borrow { place, mutable: false, holder: Holder::Operator });
        }
        self.expect(rhs, Ty::Str, what)?;
        self.borrows.truncate(outer);
        Ok(())
    }

    /// Checks `Name { field: value, ... }`. The type arguments of a
    /// generic struct come from the type the surroundings expect, or
    /// else from the values given.
    fn struct_literal(&mut self, id: ExprId, name: Span, fields: List, hint: Option<Ty>) -> SResult<Ty> {
        let siblings = self.owned_siblings;
        let pos = self.ast.expr_pos[id as usize];
        let text = name.text(self.src);
        let Some(&template) = self.struct_names.get(text) else {
            return err(pos, format!("unknown struct `{text}`"));
        };
        if let Some(index) = self.struct_templates[template as usize].only {
            // The common case: every field's type is already known.
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
                self.expect(init.value, want, What::Field(field_name))?;
                // The new struct takes a temporary's buffers over.
                self.store_from(init.value);
                self.sibling(init.value);
                self.init_offset[self.sh.i(init_id)] = offset;
            }
            if let Some(missing) = seen.iter().position(|&given| !given) {
                let field_name = self.structs[index as usize].fields[missing].name;
                return err(pos, format!("field `{field_name}` of `{text}` is missing"));
            }
            let ty = Ty::Struct(index);
            self.owned_siblings = siblings;
            self.expr_tmp[self.sh.e(id)] = self.temp(id, ty)?;
            return Ok(ty);
        }
        let info = &self.struct_templates[template as usize];
        let (params, shapes) = (info.params.clone(), Rc::clone(&info.fields));
        let mut bound: Vec<Option<Ty>> = match hint {
            Some(Ty::Struct(index)) if self.structs[index as usize].template == template => {
                self.structs[index as usize].args.iter().map(|&arg| Some(arg)).collect()
            }
            _ => vec![None; params.len()],
        };
        let mut seen = vec![false; shapes.len()];
        // Field initializers and the fields they fill, for the offsets.
        let mut filled = Vec::with_capacity(shapes.len());
        for &init_id in self.ast.list(fields) {
            let init = self.ast.field_inits[init_id as usize];
            let field_name = init.name.text(self.src);
            let Some(at) = shapes.iter().position(|&(known, _)| known == field_name) else {
                return err(init.name.start, format!("struct `{text}` has no field `{field_name}`"));
            };
            if seen[at] {
                return err(init.name.start, format!("field `{field_name}` is given more than once"));
            }
            seen[at] = true;
            let shape = &shapes[at].1;
            if Self::ready(shape, &bound) {
                let want = self.realize(shape, &bound, pos)?;
                self.settle()?;
                self.expect(init.value, want, What::Field(field_name))?;
            } else {
                // This value decides what a type parameter stands for.
                let value_pos = self.ast.expr_pos[init.value as usize];
                let got = self.expr(init.value)?;
                if got == Ty::Unit {
                    return err(value_pos, "this expression has no value to store");
                }
                if !self.unify(shape, got, &mut bound) {
                    return err(
                        value_pos,
                        format!("field `{field_name}` needs `{}` but this is `{}`", self.shape_name(shape, &params), self.ty_name(got)),
                    );
                }
            }
            // The new struct takes a temporary's buffers over.
            self.store_from(init.value);
            self.sibling(init.value);
            filled.push((init_id, at));
        }
        if let Some(missing) = seen.iter().position(|&given| !given) {
            return err(pos, format!("field `{}` of `{text}` is missing", shapes[missing].0));
        }
        if bound.iter().any(Option::is_none) {
            return err(
                pos,
                format!(
                    "cannot tell which `{text}` this is; give the variable a type, as in `let x: {text}[{}] = ...`",
                    vec!["i64"; params.len()].join(", ")
                ),
            );
        }
        let ty = self.struct_instance(template, bound.into_iter().flatten().collect(), pos)?;
        self.settle()?;
        let Ty::Struct(index) = ty else {
            unreachable!("a struct declaration makes struct types");
        };
        for (init_id, at) in filled {
            self.init_offset[self.sh.i(init_id)] = self.structs[index as usize].fields[at].offset;
        }
        self.owned_siblings = siblings;
        self.expr_tmp[self.sh.e(id)] = self.temp(id, ty)?;
        Ok(ty)
    }

    /// Stores the bytes of a string literal, escapes decoded, and
    /// returns its index in `strings`.
    fn string_literal(&mut self, span: Span) -> u32 {
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
        self.strings.push((offset, self.data.len() as u32 - offset));
        self.strings.len() as u32 - 1
    }

    /// Checks an expression that builds an enum value: `Some(x)`, or just
    /// `None` when `args` is absent. The type arguments of a generic enum
    /// come from the type the surroundings expect, or else from the
    /// values given.
    fn construct(&mut self, id: ExprId, template: u32, variant: u32, args: Option<&[u32]>, hint: Option<Ty>) -> SResult<Ty> {
        let siblings = self.owned_siblings;
        let pos = self.ast.expr_pos[id as usize];
        let info = &self.templates[template as usize];
        let (name, shapes) = (info.variants[variant as usize].0, info.variants[variant as usize].1.clone());
        let (enum_name, params) = (info.name, info.params.clone());
        let want = shapes.len();
        let plural = |count: usize| if count == 1 { "" } else { "s" };
        let arg_ids = match args {
            None if want > 0 => {
                return err(
                    pos,
                    format!("`{name}` holds {want} value{}; write `{name}({})`", plural(want), vec!["..."; want].join(", ")),
                );
            }
            None => &[][..],
            Some(given) if want == 0 => {
                // Without arguments the `()` can simply go.
                let fix = given.is_empty().then_some(Fix::EmptyParens { at: pos });
                return err(pos, format!("`{name}` holds no value; write it without `()`")).map_err(|error: SemaError| error.with_fix(fix));
            }
            Some(ids) => ids,
        };
        if arg_ids.len() != want {
            return err(pos, format!("`{name}` holds {want} value{} but {} were given", plural(want), arg_ids.len()));
        }

        let expected = if params.is_empty() {
            Some(self.instance(template, Vec::new(), pos)?)
        } else {
            match hint {
                Some(Ty::Enum(index)) if self.enums[index as usize].template == template => hint,
                _ => None,
            }
        };
        let ty = match expected {
            Some(ty) => {
                let Ty::Enum(index) = ty else {
                    unreachable!("an enum type was selected just above");
                };
                for (i, &arg) in arg_ids.iter().enumerate() {
                    let field = self.enums[index as usize].variants[variant as usize].fields[i].0;
                    self.expect(arg, field, What::Argument(i + 1, name))?;
                    // The new value takes a temporary's buffers over.
                    self.store_from(arg);
                    self.sibling(arg);
                }
                ty
            }
            None => {
                // Nothing says which `Option` or `Result` is meant, so
                // the values decide what the type parameters stand for.
                let mut bound: Vec<Option<Ty>> = vec![None; params.len()];
                for (i, &arg) in arg_ids.iter().enumerate() {
                    let shape = &shapes[i];
                    if Self::ready(shape, &bound) {
                        let field = self.realize(shape, &bound, pos)?;
                        self.settle()?;
                        self.expect(arg, field, What::Argument(i + 1, name))?;
                    } else {
                        let arg_pos = self.ast.expr_pos[arg as usize];
                        let got = self.expr(arg)?;
                        if got == Ty::Unit {
                            return err(arg_pos, "this expression has no value to store");
                        }
                        if !self.unify(shape, got, &mut bound) {
                            return err(
                                arg_pos,
                                format!(
                                    "argument {} of `{name}` needs `{}` but this is `{}`",
                                    i + 1,
                                    self.shape_name(shape, &params),
                                    self.ty_name(got)
                                ),
                            );
                        }
                    }
                    self.store_from(arg);
                    self.sibling(arg);
                }
                if bound.iter().any(Option::is_none) {
                    return err(
                        pos,
                        format!(
                            "cannot tell which `{enum_name}` this is; give the variable a type, as in `let x: {enum_name}[{}] = ...`",
                            vec!["i64"; params.len()].join(", ")
                        ),
                    );
                }
                self.instance(template, bound.into_iter().flatten().collect(), pos)?
            }
        };
        self.settle()?;
        self.owned_siblings = siblings;
        self.expr_res[self.sh.e(id)] = if args.is_some() { BUILTIN_VARIANT } else { VARIANT };
        self.expr_aux[self.sh.e(id)] = variant;
        self.expr_tmp[self.sh.e(id)] = self.temp(id, ty)?;
        Ok(ty)
    }

    fn call(&mut self, id: ExprId, callee: ExprId, args: List, hint: Option<Ty>) -> SResult<Ty> {
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
        if let Some((template, variant)) = self.variant(text) {
            return self.construct(id, template, variant, Some(arg_ids), hint);
        }
        if text == "print" {
            if arg_ids.len() != 1 {
                return err(pos, format!("`print` takes 1 argument but {} were given", arg_ids.len()));
            }
            let arg = arg_ids[0];
            self.expr_res[self.sh.e(id)] = match self.expr(arg)? {
                Ty::I64 => BUILTIN_PRINT_I64,
                Ty::Bool => BUILTIN_PRINT_BOOL,
                Ty::Str => BUILTIN_PRINT_STR,
                other => {
                    return err_expr(self.ast, arg,
                        format!("`print` takes an `i64`, a `bool` or a `str` but this is `{}`", self.ty_name(other)),
                    );
                }
            };
            return Ok(Ty::Unit);
        }
        if text == "str" {
            if arg_ids.len() != 1 {
                return err(pos, format!("`str` takes 1 argument but {} were given", arg_ids.len()));
            }
            let arg = arg_ids[0];
            self.expr_res[self.sh.e(id)] = match self.expr(arg)? {
                Ty::I64 => BUILTIN_STR_FROM_I64,
                Ty::Bool => BUILTIN_STR_FROM_BOOL,
                other => {
                    return err_expr(self.ast, arg,
                        format!("`str` turns an `i64` or a `bool` into text but this is `{}`", self.ty_name(other)),
                    );
                }
            };
            self.expr_tmp[self.sh.e(id)] = self.temp(id, Ty::Str)?;
            return Ok(Ty::Str);
        }
        let Some(&index) = self.fn_names.get(text) else {
            if BUILTIN_FUNCTIONS.contains(&text) {
                return self.builtin(id, text, arg_ids);
            }
            if let Some(&generic) = self.generic_names.get(text) {
                return self.call_generic(id, generic, text, arg_ids, hint, false);
            }
            return if self.struct_names.contains_key(text) {
                err(pos, format!("`{text}` is a struct; create a value with `{text} {{ ... }}`"))
            } else {
                err(pos, format!("unknown function `{text}`"))
            };
        };
        self.call_function(id, index, text, arg_ids, false)
    }

    /// The built-in functions that talk to the system.
    fn builtin(&mut self, id: ExprId, text: &'a str, arg_ids: &[u32]) -> SResult<Ty> {
        let pos = self.ast.expr_pos[id as usize];
        if text == "embed" {
            return err(pos, "`embed` works only in a constant: write `const NAME = embed(\"file\")` and use the constant");
        }
        let (params, target): (&[Ty], u32) = match text {
            "eprint" => (&[Ty::Str], BUILTIN_EPRINT),
            "exit" => (&[Ty::I64], BUILTIN_EXIT),
            "chr" => (&[Ty::I64], BUILTIN_CHR),
            "args" => (&[], BUILTIN_ARGS),
            "read_file" => (&[Ty::Str], BUILTIN_READ_FILE),
            "write_file" => (&[Ty::Str, Ty::Str], BUILTIN_WRITE_FILE),
            "set_executable" => (&[Ty::Str], BUILTIN_SET_EXECUTABLE),
            "remove_file" => (&[Ty::Str], BUILTIN_REMOVE_FILE),
            "rename_file" => (&[Ty::Str, Ty::Str], BUILTIN_RENAME_FILE),
            "clock" => (&[], BUILTIN_CLOCK),
            _ => unreachable!("`print` and `str` are handled by the caller"),
        };
        if arg_ids.len() != params.len() {
            return err(
                pos,
                format!("`{text}` takes {} argument{} but {} were given", params.len(), if params.len() == 1 { "" } else { "s" }, arg_ids.len()),
            );
        }
        for (i, (&arg, &param)) in arg_ids.iter().zip(params).enumerate() {
            self.expect(arg, param, What::Argument(i + 1, text))?;
        }
        self.expr_res[self.sh.e(id)] = target;
        let ty = match target {
            BUILTIN_EPRINT | BUILTIN_EXIT => return Ok(Ty::Unit),
            // A number: nothing to keep in a temporary.
            BUILTIN_CLOCK => return Ok(Ty::I64),
            BUILTIN_CHR => Ty::Str,
            BUILTIN_ARGS => self.list_of(Ty::Str),
            // The text of the file, or what the system says went wrong.
            BUILTIN_READ_FILE => self.instance(RESULT, vec![Ty::Str, Ty::Str], pos)?,
            // The number of bytes written (0 for `set_executable`,
            // `remove_file` and `rename_file`), or what went wrong.
            _ => self.instance(RESULT, vec![Ty::I64, Ty::Str], pos)?,
        };
        self.settle()?;
        self.expr_tmp[self.sh.e(id)] = self.temp(id, ty)?;
        Ok(ty)
    }

    /// Checks the arguments of a call to function `index`. For a method
    /// call, `arg_ids` starts with the receiver, which is already checked.
    fn call_function(&mut self, id: ExprId, index: u32, text: &'a str, arg_ids: &[u32], method: bool) -> SResult<Ty> {
        let pos = self.ast.expr_pos[id as usize];
        let receivers = method as usize;
        let expected = self.fns[index as usize].params.len();
        if arg_ids.len() != expected {
            let expected = expected - receivers;
            return err(
                pos,
                format!(
                    "`{text}` takes {expected} argument{} but {} were given",
                    if expected == 1 { "" } else { "s" },
                    arg_ids.len() - receivers
                ),
            );
        }

        // Places handed over by reference stay borrowed until this call
        // returns, so later arguments are checked against them.
        let outer_borrows = self.borrows.len();
        for (i, &arg) in arg_ids.iter().enumerate() {
            let param = self.fns[index as usize].params[i];
            let want = if method && i == 0 { Want::Checked(param.ty) } else { Want::Known(param.ty) };
            self.argument(text, i + 1 - receivers, arg, param.mutable, want, &mut [], &[])?;
        }
        self.borrows.truncate(outer_borrows);
        self.finish_call(id, index)
    }

    /// Checks one argument of a call: that `mut` is written exactly where
    /// the parameter is `mut`, the type, and that the place it names is
    /// free to be handed over. For a generic function, `bound` is what its
    /// type parameters (`type_params`) are known to stand for so far.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn argument(
        &mut self,
        text: &'a str,
        number: usize,
        arg: ExprId,
        mutable: bool,
        want: Want,
        bound: &mut [Option<Ty>],
        type_params: &[&'a str],
    ) -> SResult<()> {
        let arg_pos = self.ast.expr_pos[arg as usize];
        let what = What::Argument(number, text);
        // The receiver of a method that changes it is written plainly,
        // `xs.extend(ys)`, like the receiver of `push`.
        let receiver = number == 0;
        let value = match (mutable, self.ast.exprs[arg as usize]) {
            (true, Expr::MutArg(inner)) => inner,
            (true, _) if receiver => arg,
            (true, _) => {
                // Writing `mut` in front is the whole repair when the
                // argument is a changeable variable of the right type.
                let known = match want {
                    Want::Known(ty) | Want::Checked(ty) => Some(ty),
                    Want::Infer(_) => None,
                };
                let fix = match self.ast.exprs[arg as usize] {
                    Expr::Ident(name) => self
                        .lookup(name.text(self.src))
                        .filter(|local| local.mutable && known.is_none_or(|ty| ty == local.ty))
                        .map(|_| Fix::Replace { start: arg_pos, end: arg_pos, text: "mut ".to_string() }),
                    _ => None,
                };
                return err(
                    arg_pos,
                    format!("argument {number} of `{text}` can be changed by the function, so it must be written `mut <variable>`"),
                )
                .map_err(|error: SemaError| error.with_fix(fix));
            }
            (false, Expr::MutArg(inner)) => {
                let fix = Fix::Replace { start: arg_pos, end: self.ast.expr_pos[inner as usize], text: String::new() };
                return err(arg_pos, format!("argument {number} of `{text}` is not a `mut` parameter; remove `mut`"))
                    .map_err(|error: SemaError| error.with_fix(Some(fix)));
            }
            (false, _) => arg,
        };
        let ty = match want {
            Want::Known(ty) => {
                self.expect(value, ty, what)?;
                ty
            }
            Want::Checked(ty) => ty,
            Want::Infer(shape) if Self::ready(shape, bound) => {
                let ty = self.realize(shape, bound, arg_pos)?;
                self.settle()?;
                self.expect(value, ty, what)?;
                ty
            }
            Want::Infer(shape) => {
                // This argument decides what a type parameter stands for.
                let got = self.expr(value)?;
                if got == Ty::Unit {
                    return err_expr(self.ast, value, "this expression has no value to pass");
                }
                if !self.unify(shape, got, bound) {
                    return err_expr(self.ast, value,
                        format!(
                            "argument {number} of `{text}` needs `{}` but this is `{}`",
                            self.shape_name(shape, type_params),
                            self.ty_name(got)
                        ),
                    );
                }
                got
            }
        };
        if mutable {
            self.expr_ty[self.sh.e(arg)] = ty;
            let Some(place) = self.place(value) else {
                if receiver {
                    return err(arg_pos, format!("`{text}` changes its receiver, so it needs a variable or a part of a variable"));
                }
                if let Some(name) = self.constant_name(value) {
                    return err(arg_pos, format!("cannot pass `{name}` as `mut`: it is a constant"));
                }
                return err(arg_pos, "`mut` needs a variable or a part of a variable");
            };
            if !place.mutable {
                let message = if receiver {
                    format!("cannot change `{}`: {}", self.root_name(value), read_only(&place))
                } else {
                    format!("cannot pass `{}` as `mut`: {}", self.root_name(value), read_only(&place))
                };
                return err(arg_pos, message).map_err(|error: SemaError| error.with_fix(self.var_fix(&place)));
            }
            // The function gets its address, so it must stay in memory.
            if let Some(local) = self.locals.iter_mut().rev().find(|local| local.slot == place.root) {
                local.addressed = true;
            }
            self.borrow(value, place, true).map_err(|mut error| {
                // A receiver is not written `mut`, so the message says
                // what the method does to it.
                if receiver && let Some(rest) = error.message.strip_prefix("cannot pass ") {
                    error.message = format!("cannot change {}", rest.replacen(" as `mut`", "", 1));
                }
                error
            })?;
        } else if ty.is_aggregate() {
            // Passed by reference: the place must not change meanwhile.
            if let Some(place) = self.place(arg) {
                self.borrow(arg, place, false)?;
            }
        }
        Ok(())
    }

    /// Records that call `id` goes to function `index` and gives the
    /// call's type.
    fn finish_call(&mut self, id: ExprId, index: u32) -> SResult<Ty> {
        let ret = self.fns[index as usize].ret;
        self.expr_res[self.sh.e(id)] = index;
        if ret.is_aggregate() {
            self.expr_tmp[self.sh.e(id)] = self.temp(id, ret)?;
        }
        Ok(ret)
    }

    /// A call to a generic function. What its type parameters stand for
    /// comes from the arguments, read left to right; a parameter no
    /// argument mentions comes from the type expected of the result.
    fn call_generic(
        &mut self,
        id: ExprId,
        generic: u32,
        text: &'a str,
        arg_ids: &[u32],
        hint: Option<Ty>,
        method: bool,
    ) -> SResult<Ty> {
        let pos = self.ast.expr_pos[id as usize];
        let info = &self.generic_fns[generic as usize];
        let (type_params, params, ret) = (info.type_params.clone(), info.params.clone(), info.ret.clone());
        // For a method call, `arg_ids` starts with the receiver, which is
        // already checked and not counted as an argument.
        let receivers = method as usize;
        if arg_ids.len() != params.len() {
            let expected = params.len() - receivers;
            return err(
                pos,
                format!(
                    "`{text}` takes {expected} argument{} but {} were given",
                    if expected == 1 { "" } else { "s" },
                    arg_ids.len() - receivers
                ),
            );
        }
        let mut bound: Vec<Option<Ty>> = vec![None; type_params.len()];
        let outer_borrows = self.borrows.len();
        for (i, &arg) in arg_ids.iter().enumerate() {
            let want = if method && i == 0 {
                let ty = self.expr_ty[self.sh.e(arg)];
                let fits = self.unify(&params[0].0, ty, &mut bound);
                debug_assert!(fits, "the caller matched the receiver");
                Want::Checked(ty)
            } else {
                Want::Infer(&params[i].0)
            };
            self.argument(text, i + 1 - receivers, arg, params[i].1, want, &mut bound, &type_params)?;
        }
        self.borrows.truncate(outer_borrows);
        // What the arguments leave open, the expected type may settle.
        if let (Some(ret), Some(hint), true) = (&ret, hint, bound.iter().any(Option::is_none)) {
            let mut trial = bound.clone();
            if self.unify(ret, hint, &mut trial) {
                bound = trial;
            }
        }
        if let Some(unknown) = bound.iter().position(Option::is_none) {
            return err(
                pos,
                format!(
                    "cannot tell what `{}` is in this call to `{text}`: no argument settles it; store the result in a variable with a type",
                    type_params[unknown]
                ),
            );
        }
        let index = self.fn_instance(generic, bound.into_iter().flatten().collect(), pos)?;
        self.finish_call(id, index)
    }

    /// The function a generic function becomes with these types. It is
    /// added to the list of functions the first time it is asked for, and
    /// its body is checked when its turn comes.
    fn fn_instance(&mut self, generic: u32, type_args: Vec<Ty>, at: u32) -> SResult<u32> {
        let key = (generic, type_args);
        if let Some(&index) = self.fn_ids.get(&key) {
            return Ok(index);
        }
        let (generic, type_args) = key;
        let info = &self.generic_fns[generic as usize];
        let (item, name, shapes, ret_shape) = (info.item, info.name, info.params.clone(), info.ret.clone());
        // `fn f[T](x: T) { f([x]) }` would ask for ever larger types.
        let depth = self.fn_depth + 1;
        if depth > 40 {
            return err(at, format!("`{name}` is used with types that grow without end"));
        }
        let bound: Vec<Option<Ty>> = type_args.iter().map(|&ty| Some(ty)).collect();
        let mut params = Vec::with_capacity(shapes.len());
        for (shape, mutable) in &shapes {
            let ty = self.realize(shape, &bound, at)?;
            params.push(ParamInfo { ty, by_ref: *mutable || ty.is_aggregate(), mutable: *mutable });
        }
        let ret = match &ret_shape {
            Some(shape) => self.realize(shape, &bound, at)?,
            None => Ty::Unit,
        };
        self.settle()?;
        let Item::Fn { nodes, .. } = self.ast.items[item as usize] else {
            unreachable!("a function's item is a function");
        };
        // Entries of its own in every per-node table. A library function
        // without type parameters is used with one set of types only and
        // keeps the entries its nodes already have.
        let shift = if type_args.is_empty() {
            Shift::default()
        } else {
            let shift = Shift {
                expr: (self.expr_res.len() as u32).wrapping_sub(nodes.exprs.0),
                stmt: (self.stmt_slot.len() as u32).wrapping_sub(nodes.stmts.0),
                pat: (self.pats.len() as u32).wrapping_sub(nodes.patterns.0),
                init: (self.init_offset.len() as u32).wrapping_sub(nodes.field_inits.0),
            };
            let exprs = self.expr_res.len() + nodes.exprs.1 as usize;
            self.expr_res.resize(exprs, 0);
            self.expr_ty.resize(exprs, Ty::Unit);
            self.expr_aux.resize(exprs, 0);
            self.expr_tmp.resize(exprs, 0);
            self.expr_moved.resize(exprs, false);
            let stmts = self.stmt_slot.len() + nodes.stmts.1 as usize;
            self.stmt_slot.resize(stmts, 0);
            self.stmt_flag.resize(stmts, 0);
            self.stmt_drops.resize(stmts, (0, 0));
            self.pats.resize(self.pats.len() + nodes.patterns.1 as usize, PatInfo::default());
            self.init_offset.resize(self.init_offset.len() + nodes.field_inits.1 as usize, 0);
            shift
        };

        let index = self.fns.len() as u32;
        self.fn_ids.insert((generic, type_args.clone()), index);
        self.fns.push(FnInfo {
            item,
            shift,
            generic: Some(generic),
            type_args,
            depth,
            origin: if at >= self.prelude_start { self.fn_origin } else { at },
            ret,
            params,
            slots: 0,
            sret_slot: None,
            scratch_slot: 0,
            registers: Vec::new(),
            zero_slots: Vec::new(),
        });
        Ok(index)
    }

    /// A method the standard library defines in azoL for this type: an
    /// ordinary function that takes the receiver as its first argument.
    fn library_method(&mut self, id: ExprId, base: ExprId, base_ty: Ty, name: Span, args: &[u32]) -> SResult<Ty> {
        let text = name.text(self.src);
        let found = self.library.get(text).and_then(|candidates| {
            candidates.iter().copied().find(|&generic| {
                let info = &self.generic_fns[generic as usize];
                self.unify(&info.params[0].0, base_ty, &mut vec![None; info.type_params.len()])
            })
        });
        let Some(generic) = found else {
            return err(name.start, format!("`{}` has no method `{text}`", self.ty_name(base_ty)));
        };
        let mut all = Vec::with_capacity(args.len() + 1);
        all.push(base);
        all.extend_from_slice(args);
        self.call_generic(id, generic, text, &all, None, true)
    }

    /// `s.push(byte)` and `s.set(index, byte)`: a string used as a
    /// buffer of bytes. Both change the string where it is, so it must
    /// be a variable that can change, like the list of a `push`. The
    /// byte is the low eight bits of the number.
    fn byte_method(&mut self, id: ExprId, base: ExprId, text: &str, args: &[u32]) -> SResult<Ty> {
        let pos = self.ast.expr_pos[id as usize];
        let expected = if text == "push" { 1 } else { 2 };
        if args.len() != expected {
            return err(pos, format!("`{text}` takes {expected} argument{} but {} were given", if expected == 1 { "" } else { "s" }, args.len()));
        }
        let Some(place) = self.place(base) else {
            return err(pos, format!("`{text}` changes its receiver, so it needs a variable or a part of a variable"));
        };
        if !place.mutable {
            let message = format!("cannot change `{}`: {}", self.root_name(base), read_only(&place));
            return err(pos, message).map_err(|error: SemaError| error.with_fix(self.var_fix(&place)));
        }
        // The numbers are computed before the string is looked at.
        self.late.push(base);
        for (i, &arg) in args.iter().enumerate() {
            self.expect(arg, Ty::I64, What::Argument(i + 1, text))?;
        }
        self.late.pop();
        self.check_change(base, &place)?;
        // Room for what the runtime needs when the string has to get a
        // buffer of its own or a larger one.
        self.expr_tmp[self.sh.e(id)] = self.alloc(6, pos)?;
        self.expr_res[self.sh.e(id)] = if text == "push" { BUILTIN_STR_PUSH } else { BUILTIN_STR_SET };
        Ok(Ty::Unit)
    }

    /// A call written `value.name(...)`.
    fn method(&mut self, id: ExprId, base: ExprId, name: Span, args: &[u32]) -> SResult<Ty> {
        let pos = self.ast.expr_pos[id as usize];
        let base_ty = self.expr(base)?;
        let text = name.text(self.src);
        let arity = |expected: usize| -> SResult<()> {
            if args.len() == expected {
                Ok(())
            } else {
                err(pos, format!("`{text}` takes {expected} argument{} but {} were given", if expected == 1 { "" } else { "s" }, args.len()))
            }
        };
        if base_ty == Ty::Str {
            return match text {
                "len" => {
                    arity(0)?;
                    self.expr_res[self.sh.e(id)] = BUILTIN_LEN;
                    Ok(Ty::I64)
                }
                "slice" => {
                    arity(2)?;
                    // The bounds are computed before the string is looked at.
                    self.late.push(base);
                    self.expect(args[0], Ty::I64, What::Argument(1, "slice"))?;
                    self.expect(args[1], Ty::I64, What::Argument(2, "slice"))?;
                    self.late.pop();
                    // The bytes are copied into a string of their own.
                    self.expr_tmp[self.sh.e(id)] = self.temp(id, Ty::Str)?;
                    self.expr_res[self.sh.e(id)] = BUILTIN_STR_SLICE;
                    Ok(Ty::Str)
                }
                "push" | "set" => self.byte_method(id, base, text, args),
                "is_utf8" => {
                    arity(0)?;
                    self.expr_res[self.sh.e(id)] = BUILTIN_STR_IS_UTF8;
                    Ok(Ty::Bool)
                }
                _ => self.library_method(id, base, base_ty, name, args),
            };
        }
        let Ty::List(list) = base_ty else {
            return self.library_method(id, base, base_ty, name, args);
        };
        let elem = self.lists[list as usize];
        match text {
            "len" => {
                arity(0)?;
                self.expr_res[self.sh.e(id)] = BUILTIN_LEN;
                Ok(Ty::I64)
            }
            "push" | "pop" => {
                arity(if text == "push" { 1 } else { 0 })?;
                let Some(place) = self.place(base) else {
                    return err(pos, format!("`{text}` changes the list, so it needs a variable or a part of a variable"));
                };
                if !place.mutable {
                    let message = format!("cannot change `{}`: {}", self.root_name(base), read_only(&place));
                    return err(pos, message).map_err(|error: SemaError| error.with_fix(self.var_fix(&place)));
                }
                if text == "push" {
                    // The element is computed before the list is looked at.
                    self.late.push(base);
                    self.expect(args[0], elem, What::Text("`push`"))?;
                    // The list takes a temporary's buffers over.
                    self.store_from(args[0]);
                    self.late.pop();
                    self.check_change(base, &place)?;
                    // The element waits here while the list makes room.
                    self.expr_tmp[self.sh.e(id)] = self.alloc(self.size_of(elem), pos)?;
                    self.expr_res[self.sh.e(id)] = BUILTIN_LIST_PUSH;
                    Ok(Ty::Unit)
                } else {
                    self.check_change(base, &place)?;
                    if elem.is_aggregate() {
                        // The element leaves the list; this temporary owns it now.
                        self.expr_tmp[self.sh.e(id)] = self.temp(id, elem)?;
                    }
                    self.expr_res[self.sh.e(id)] = BUILTIN_LIST_POP;
                    Ok(elem)
                }
            }
            _ => self.library_method(id, base, base_ty, name, args),
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

    #[test]
    fn embedded_files() {
        let folder = std::env::temp_dir().join(format!("azol_embed_test_{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("note.txt");
        std::fs::write(&file, b"two\nlines \xff\x00").unwrap();
        // An absolute path is used as it is.
        let src = format!("const NOTE = embed(\"{0}\")\nconst TWICE = NOTE + embed(\"{0}\")\nfn main() {{\n print(NOTE)\n print(TWICE)\n}}", file.display());
        let tokens = lex(&src).unwrap();
        let ast = parse(&src, &tokens).unwrap();
        let program = check(&ast, &src).unwrap();
        let text = |index: usize| {
            let (start, len) = program.strings[index];
            program.data[start as usize..(start + len) as usize].to_vec()
        };
        assert!((0..program.strings.len()).any(|index| text(index) == b"two\nlines \xff\x00"));
        assert!((0..program.strings.len()).any(|index| text(index) == b"two\nlines \xff\x00two\nlines \xff\x00"));
        // A relative path starts at the folder of the source file.
        let src = "const NOTE = embed(\"note.txt\")\nfn main() {\n print(NOTE)\n}";
        let tokens = lex(src).unwrap();
        let ast = parse(src, &tokens).unwrap();
        let path = folder.join("main.azl");
        let files = [(0, path.to_str().unwrap())];
        assert!(check_with_files(&ast, src, u32::MAX, &files).is_ok());
        let elsewhere = [(0, "/azol_no_such_folder/main.azl")];
        let errors = check_with_files(&ast, src, u32::MAX, &elsewhere).err().unwrap();
        assert_eq!(errors[0].message, "cannot embed `note.txt`: No such file or directory");
        std::fs::remove_dir_all(&folder).unwrap();

        assert_eq!(error("const A = embed(\"/azol_no_such_folder/a\")\nfn main() {\n}"), "cannot embed `/azol_no_such_folder/a`: No such file or directory");
        assert_eq!(error("const A = embed(7)\nfn main() {\n}"), "argument 1 of `embed` needs `str` but this is `i64`");
        assert_eq!(error("const A = embed()\nfn main() {\n}"), "`embed` takes 1 argument but 0 were given");
        assert_eq!(error("const A = embed(\"a\", \"b\")\nfn main() {\n}"), "`embed` takes 1 argument but 2 were given");
        assert_eq!(error("const A = embed(B)\nconst B = embed(A)\nfn main() {\n}"), "constant `A` is defined in terms of itself");
        let outside = "`embed` works only in a constant: write `const NAME = embed(\"file\")` and use the constant";
        assert_eq!(error(&in_main("print(embed(\"a.txt\"))")), outside);
        assert_eq!(error(&in_main("let text = embed(\"a.txt\")")), outside);
        assert_eq!(error("fn embed() {\n}\nfn main() {\n}"), "`embed` is built in and cannot be redefined");
    }

    #[test]
    fn is_utf8_is_a_method_of_strings() {
        assert!(check_src(&in_main("let s = \"a\"\nlet ok: bool = s.is_utf8()\nif (s + \"b\").is_utf8() && !chr(200).is_utf8() {\n}")).is_ok());
        assert_eq!(error(&in_main("let ok = \"a\".is_utf8(1)")), "`is_utf8` takes 0 arguments but 1 were given");
        assert_eq!(error(&in_main("let n: i64 = \"a\".is_utf8()")), "expected `i64` but this is `bool`");
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
        assert_eq!(program.structs[0].size, 4);
        assert_eq!(program.structs[0].fields[1].offset, 3);
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
        assert_eq!(error(&in_main("let a = \"x\" + 1")), "`+` needs `str` but this is `i64`");
        assert_eq!(error(&in_main("let a = 1 + \"x\"")), "`+` needs `i64` but this is `str`");
        assert_eq!(error(&in_main("let a = true + 1")), "`+` needs `i64` or `str` but this is `bool`");
        assert_eq!(error(&in_main("let a = \"x\" == 1")), "`==` needs `str` but this is `i64`");
        assert_eq!(error(&in_main("let a = \"x\" - \"y\"")), "`-` needs `i64` but this is `str`");
        assert_eq!(error(&in_main("let a = \"x\" < 1")), "`<` needs `str` but this is `i64`");
        assert_eq!(error(&in_main("let a = [1] < [2]")), "`<` needs `i64` but this is `List[i64]`");
        assert_eq!(error(&in_main("let a = true >= false")), "`>=` needs `i64` but this is `bool`");
        assert_eq!(error(&in_main("let s = \"x\"\ns.push(1)")), "cannot change `s`: declare it with `var` to make it changeable");
        assert_eq!(error(&in_main("var s = \"x\"\ns.set(1)")), "`set` takes 2 arguments but 1 were given");
        assert_eq!(error(&in_main("var s = \"x\"\ns.push(\"y\")")), "argument 1 of `push` needs `i64` but this is `str`");
        assert_eq!(error(&in_main("\"x\".push(1)")), "`push` changes its receiver, so it needs a variable or a part of a variable");
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
        assert_eq!(error("struct A {\n x: Missing\n}\nfn main() {\n}"), "unknown type `Missing`; the built-in types so far are `i64`, `bool`, `str`, `List`, `Option` and `Result`");
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
        assert_eq!(error(&list_main("let n = 1\nprint(n[0])")), "only a list or a string can be indexed but this is `i64`");
        assert_eq!(error(&list_main("let xs = [1]\nprint(xs.size())")), "`List[i64]` has no method `size`");
        assert_eq!(error(&list_main("let xs = [1]\nprint(xs.len)")), "`List[i64]` has no field `len`; its methods are `len()`, `push(...)` and `pop()`");
        assert_eq!(error(&list_main("let xs = [1]\nprint(xs.len(1))")), "`len` takes 0 arguments but 1 were given");
        assert_eq!(error(&list_main("let n = 1\nn.push(2)")), "`i64` has no method `push`");
        assert_eq!(error(&list_main("let xs = [1]\nif xs == xs {\n}")), "lists cannot be compared with `==` yet");
        assert_eq!(error(&list_main("for x in 5 {\n}")), "`for` needs a list to go through but this is `i64`");
        assert_eq!(error(&list_main("let xs = [[1], [true]]")), "this list's element needs `i64` but this is `bool`");
        assert_eq!(error(&list_main("var xs = [[1]]\nxs.push(2)")), "`push` needs `List[i64]` but this is `i64`");
        assert_eq!(error("fn f(xs: List) {\n}\nfn main() {\n}"), "`List` takes one element type, as in `List[i64]`");
        assert_eq!(error("fn f(xs: Map[i64]) {\n}\nfn main() {\n}"), "unknown type `Map`; the built-in types so far are `i64`, `bool`, `str`, `List`, `Option` and `Result`");
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
        assert_eq!(error(&list_main("for x in [1, 2] {\nx = 3\n}")), "cannot change `x`: a loop variable or pattern binding is read-only");
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
        // the two struct literals, `[1, 2]` returned inside `make`, and
        // the two string literals, which own nothing and are never
        // copied. The first two copies of `a` are real copies; its last
        // use, in the struct literal, takes its buffer over.
        assert_eq!(program.expr_moved.iter().filter(|&&moved| moved).count(), 10);
        // Only `total(make())` leaves a temporary for its statement to free.
        assert_eq!(program.drops.len(), 1);
        let main = &program.fns[program.main as usize];
        assert_eq!(main.zero_slots.len(), 1);
    }

    #[test]
    fn strings_and_nested_lists_are_accepted() {
        assert!(check_src(&in_main("let a = \"x\" + \"y\"\nlet b = a + str(1) + str(true)\nif a == b || a != \"xy\" {\nprint(a.len())\n}")).is_ok());
        assert!(check_src(&list_main("var grid = [[1, 2], [3]]\ngrid.push([4])\ngrid[0].push(5)\ngrid[1][0] = 6\nlet row = grid.pop()\nfor line in grid {\nfor x in line {\nprint(x + row.len())\n}\n}")).is_ok());
        assert!(check_src(&list_main("var bags = [Bag { items: [1], name: \"a\" }]\nbags[0].items.push(2)\nbags.push(bags[0])\nvar names: List[str] = []\nfor b in bags {\nnames.push(b.name + \"!\")\n}")).is_ok());
        assert!(check_src("struct Node {\n value: i64\n children: List[Node]\n}\nfn size(n: Node) -> i64 {\n var total = 1\n for child in n.children {\n total = total + size(child)\n }\n return total\n}\nfn main() {\n let leaf = Node { value: 1, children: [] }\n let root = Node { value: 0, children: [leaf, leaf] }\n print(size(root))\n}").is_ok());
    }

    #[test]
    fn string_errors() {
        assert_eq!(error(&in_main("let a = str(\"x\")")), "`str` turns an `i64` or a `bool` into text but this is `str`");
        assert_eq!(error(&in_main("let a = str()")), "`str` takes 1 argument but 0 were given");
        assert_eq!(error(&in_main("let a = \"x\"\nprint(a.size())")), "`str` has no method `size`");
        assert_eq!(error("fn str(a: i64) {\n}\nfn main() {\n}"), "`str` is built in and cannot be redefined");
        assert_eq!(error(&in_main("var a = \"x\"\na = 1")), "expected `str` but this is `i64`");
    }

    #[test]
    fn a_string_cannot_change_while_an_operator_still_reads_it() {
        let fns = "fn grow(xs: mut List[str]) -> str {\n xs.push(\"more\")\n return \"x\"\n}\nfn set(s: mut str) -> str {\n s = \"new\"\n return s\n}\n";
        let main = |body: &str| format!("{fns}{}", in_main(body));
        let reading = "it is still being read earlier in the same expression";
        assert_eq!(error(&main("var xs = [\"a\"]\nlet s = xs[0] + grow(mut xs)")), format!("cannot pass `xs` as `mut` here: {reading}"));
        assert_eq!(error(&main("var s = \"a\"\nif s == set(mut s) {\n}")), format!("cannot pass `s` as `mut` here: {reading}"));
        assert_eq!(error(&main("var xs = [\"a\"]\nlet s = xs[0] + xs.pop()")), format!("cannot change `xs` here: {reading}"));
        // The other order is fine: the call has finished before the read.
        assert!(check_src(&main("var xs = [\"a\"]\nlet s = grow(mut xs) + xs[0]")).is_ok());
        // So is this: `s + s` is finished and stored before `set` runs.
        assert!(check_src(&main("var s = \"a\"\nlet t = s + s + set(mut s)")).is_ok());
        assert_eq!(error(&main("var s = \"a\"\nlet t = s + (s + set(mut s))")), format!("cannot pass `s` as `mut` here: {reading}"));
    }

    #[test]
    fn a_loop_variable_is_a_view_and_is_never_moved_out() {
        let src = "fn pick(rows: List[List[i64]]) -> List[i64] {\n for row in rows {\n return row\n }\n return []\n}\nfn main() {\n}";
        let tokens = lex(src).unwrap();
        let ast = parse(src, &tokens).unwrap();
        let program = check(&ast, src).unwrap();
        // `return row` must copy: only the `[]` is moved.
        assert_eq!(program.expr_moved.iter().filter(|&&moved| moved).count(), 1);
    }

    #[test]
    fn enum_layout_and_ownership() {
        let src = "enum Shape {\n Circle(i64)\n Rect(i64, i64)\n Empty\n}\nstruct Named {\n name: Option[str]\n id: i64\n}\nfn main() {\n let a: Option[i64] = None\n let b: Result[Shape, str] = Err(\"no\")\n let c: Option[i64] = Some(1)\n}";
        let tokens = lex(src).unwrap();
        let ast = parse(src, &tokens).unwrap();
        let prog = check(&ast, src).unwrap();
        let find = |name: &str| prog.enums.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no enum {name}"));
        // A tag, then room for the largest variant.
        let shape = find("Shape");
        assert_eq!((shape.size, shape.owning, shape.variants.len()), (3, false, 3));
        assert_eq!(shape.variants[1].fields, vec![(Ty::I64, 1), (Ty::I64, 2)]);
        let text = find("Option[str]");
        assert_eq!((text.size, text.owning), (4, true));
        assert_eq!(text.variants[0].heap, vec![(1, Ty::Str)]);
        assert!(text.variants[1].heap.is_empty());
        let result = find("Result[Shape, str]");
        assert_eq!((result.size, result.owning), (4, true));
        // The same type arguments give the same type.
        assert_eq!(prog.enums.iter().filter(|e| e.name == "Option[i64]").count(), 1);
        assert_eq!((find("Option[i64]").size, find("Option[i64]").owning), (2, false));
        // The struct owns through its enum field, whose tag marks it as empty.
        assert_eq!(prog.structs[0].size, 5);
        assert_eq!(prog.structs[0].heap.len(), 1);
        assert_eq!(prog.structs[0].heap[0].0, 0);
        assert_eq!(idle_slot(prog.structs[0].heap[0].1), 0);
        assert_eq!(idle_slot(Ty::Str), 2);
    }

    #[test]
    fn enum_declaration_errors() {
        assert_eq!(error("enum T {\n Leaf\n Node(T)\n}\nfn main() {\n}"), "enum `T` contains itself, so it would have no finite size; put the inner value in a `List`");
        assert_eq!(error("struct S {\n o: Option[S]\n}\nfn main() {\n}"), "struct `S` contains itself, so it would have no finite size");
        assert!(check_src("enum T {\n Leaf\n Node(List[T])\n}\nstruct S {\n o: List[Option[S]]\n}\nfn main() {\n}").is_ok());
        assert_eq!(error("enum E[T] {\n A(E[List[T]])\n B\n}\nfn main() {\n let e: E[i64] = B\n}"), "this type nests enums too deeply");
        assert_eq!(error("enum E {\n}\nfn main() {\n}"), "enum `E` has no variants");
        assert_eq!(error("enum E {\n A\n}\nenum E {\n B\n}\nfn main() {\n}"), "enum `E` is defined more than once");
        assert_eq!(error("enum E {\n A\n A\n}\nfn main() {\n}"), "variant `A` is declared more than once");
        assert_eq!(error("enum E {\n A\n}\nenum F {\n A\n}\nfn main() {\n}"), "variant `A` already belongs to `E`; variant names must differ across enums");
        assert_eq!(error("enum E {\n Some\n}\nfn main() {\n}"), "variant `Some` already belongs to `Option`; variant names must differ across enums");
        assert_eq!(error("enum Option {\n A\n}\nfn main() {\n}"), "`Option` is built in and cannot be redefined");
        assert_eq!(error("struct Result {\n}\nfn main() {\n}"), "`Result` is built in and cannot be redefined");
        assert_eq!(error("struct E {\n}\nenum E {\n A\n}\nfn main() {\n}"), "`E` is already defined as a struct");
        assert_eq!(error("enum E {\n A\n}\nstruct E {\n}\nfn main() {\n}"), "`E` is already defined as an enum");
        assert_eq!(error("enum E[T, T] {\n A(T)\n}\nfn main() {\n}"), "type parameter `T` is declared more than once");
        assert_eq!(error("enum E[T] {\n A(T[i64])\n}\nfn main() {\n}"), "`T` is a type parameter and takes no type arguments");
        assert_eq!(error("enum E {\n A(Missing)\n}\nfn main() {\n}"), "unknown type `Missing`; the built-in types so far are `i64`, `bool`, `str`, `List`, `Option` and `Result`");
        assert_eq!(error("fn f(o: Option) {\n}\nfn main() {\n}"), "`Option` takes 1 type argument, as in `Option[i64]`");
        assert_eq!(error("fn f(o: Result[i64]) {\n}\nfn main() {\n}"), "`Result` takes 2 type arguments, as in `Result[i64, i64]`");
        assert_eq!(error("enum E {\n A\n}\nfn f(e: E[i64]) {\n}\nfn main() {\n}"), "`E` takes no type arguments");
        assert_eq!(error("fn f(a: i64[bool]) {\n}\nfn main() {\n}"), "`i64` takes no type arguments");
        assert_eq!(error("fn Some() {\n}\nfn main() {\n}"), "`Some` is already a variant of `Option`");
        assert_eq!(error(&in_main("let None = 1")), "`None` is a variant of `Option` and cannot be used as a variable name");
        assert_eq!(error("fn f(Ok: i64) {\n}\nfn main() {\n}"), "`Ok` is a variant of `Result` and cannot be used as a variable name");
        assert_eq!(error(&in_main("for Err in [1] {\n}")), "`Err` is a variant of `Result` and cannot be used as a variable name");
    }

    #[test]
    fn enum_value_errors() {
        assert_eq!(error(&in_main("let x = None")), "cannot tell which `Option` this is; give the variable a type, as in `let x: Option[i64] = ...`");
        assert_eq!(error(&in_main("let x = Ok(1)")), "cannot tell which `Result` this is; give the variable a type, as in `let x: Result[i64, i64] = ...`");
        assert_eq!(error(&in_main("let x: Option[i64] = Some(\"a\")")), "argument 1 of `Some` needs `i64` but this is `str`");
        assert_eq!(error(&in_main("let x: Option[i64] = Ok(1)")), "cannot tell which `Result` this is; give the variable a type, as in `let x: Result[i64, i64] = ...`");
        assert_eq!(error(&in_main("let x: Option[bool] = Some(1)")), "argument 1 of `Some` needs `bool` but this is `i64`");
        assert_eq!(error(&in_main("let x: Option[str] = Some(1 > 2)\nlet y: Option[i64] = x")), "argument 1 of `Some` needs `str` but this is `bool`");
        assert_eq!(error(&in_main("let x = Some(1)\nlet y: Option[bool] = x")), "expected `Option[bool]` but this is `Option[i64]`");
        assert_eq!(error(&in_main("let x = Some(1, 2)")), "`Some` holds 1 value but 2 were given");
        assert_eq!(error(&in_main("let x = Some")), "`Some` holds 1 value; write `Some(...)`");
        assert_eq!(error(&in_main("let x: Option[i64] = None()")), "`None` holds no value; write it without `()`");
        assert_eq!(error(&in_main("let x = Some(print(1))")), "this expression has no value to store");
        assert_eq!(error(&in_main("let x = Some(1)\nif x == x {\n}")), "`==` works only on enums whose variants hold no values; use `match` for `Option[i64]`");
        assert_eq!(error(&in_main("let x = Some(1)\nprint(x.value)")), "`Option[i64]` has no fields; use `match` to look inside it");
        assert_eq!(error(&in_main("let x = Some(1)\nprint(x)")), "`print` takes an `i64`, a `bool` or a `str` but this is `Option[i64]`");
        assert_eq!(error("enum E {\n A\n}\nfn main() {\n let e = E\n}"), "`E` is an enum; create a value with one of its variants");
        assert_eq!(error("enum Pair[T] {\n Two(T, T)\n}\nfn main() {\n let p = Two(1, true)\n}"), "argument 2 of `Two` needs `i64` but this is `bool`");
        assert_eq!(error("enum Bag[T] {\n Full(List[T])\n}\nfn main() {\n let b = Full(1)\n}"), "argument 1 of `Full` needs `List[T]` but this is `i64`");
        // The values decide the type arguments when nothing else does.
        assert!(check_src("enum Bag[T] {\n Full(List[T])\n Nested(Option[Option[T]])\n}\nfn main() {\n let b = Full([\"a\"])\n let c: Bag[str] = b\n let d = Nested(Some(Some(1)))\n let e: Bag[i64] = d\n}").is_ok());
        // The expected type reaches arguments, fields, elements and returns.
        assert!(check_src("struct S {\n o: Option[i64]\n}\nfn f(o: Option[str]) -> Result[i64, str] {\n return Ok(1)\n}\nfn main() {\n let s = S { o: None }\n var xs: List[Option[i64]] = [None, Some(1)]\n xs.push(None)\n xs[0] = None\n var r = f(None)\n r = Err(\"e\")\n}").is_ok());
    }

    #[test]
    fn match_errors() {
        let opt = |arms: &str| in_main(&format!("var o = Some(1)\nmatch o {{\n{arms}\n}}"));
        assert_eq!(error(&opt("Some(v) => print(v)")), "this `match` does not handle `None`; add an arm for it or a final `_ =>` arm");
        assert_eq!(error(&opt("")), "this `match` does not handle `Some` and `None`; add arms for them or a final `_ =>` arm");
        assert_eq!(error("enum E {\n A\n B\n C\n D\n}\nfn main() {\n let e = A\n match e {\n B => {}\n }\n}"), "this `match` does not handle `A`, `C` and `D`; add arms for them or a final `_ =>` arm");
        assert_eq!(error(&opt("Some(v) => {}\nSome(w) => {}\nNone => {}")), "this arm can never run: `Some` is already handled above");
        assert_eq!(error(&opt("_ => {}\nNone => {}")), "this arm can never run: the arms above already cover every case");
        assert_eq!(error(&opt("whole => {}\n_ => {}")), "this arm can never run: the arms above already cover every case");
        assert_eq!(error(&opt("Some(v) => {}\nNone => {}\n_ => {}")), "this arm can never run: the arms above already cover every case");
        assert_eq!(error(&opt("Some => {}\nNone => {}")), "`Some` holds 1 value; write `Some(_)` to match it");
        assert_eq!(error(&opt("Some(v) => {}\nNone(x) => {}")), "`None` holds no value; write it without `()`");
        assert_eq!(error(&opt("Some(a, b) => {}\nNone => {}")), "`Some` holds 1 value but this pattern lists 2");
        assert_eq!(error(&opt("Ok(v) => {}\n_ => {}")), "`Ok` is a variant of `Result` but this `match` is on `Option[i64]`");
        assert_eq!(error(&opt("Nope(v) => {}\n_ => {}")), "unknown variant `Nope`");
        assert_eq!(error(&opt("1 => {}\n_ => {}")), "this pattern is a number but the `match` is on `Option[i64]`");
        assert_eq!(error(&opt("Some(1) => {}\n_ => {}")), "a pattern inside a pattern is not supported yet; give the value a name and `match` on it inside the arm");
        assert_eq!(error(&opt("Some(None) => {}\n_ => {}")), "a pattern inside a pattern is not supported yet; give the value a name and `match` on it inside the arm");
        assert_eq!(error("enum P {\n Two(i64, i64)\n}\nfn main() {\n let p = Two(1, 2)\n match p {\n Two(a, a) => {}\n }\n}"), "`a` is used twice in this pattern");
        // What a pattern binds is a read-only view, and the matched value stays put.
        assert_eq!(error(&opt("Some(v) => v = 2\nNone => {}")), "cannot change `v`: a loop variable or pattern binding is read-only");
        assert_eq!(error(&opt("Some(v) => o = None\nNone => {}")), "cannot change `o` while a `match` is looking inside it");
        assert_eq!(error(&format!("fn f(o: mut Option[i64]) {{\n}}\n{}", opt("Some(v) => f(mut o)\nNone => {}"))), "cannot pass `o` as `mut` while a `match` is looking inside it");
        assert_eq!(error(&in_main("var xs = [Some(1)]\nmatch xs[0] {\nSome(v) => xs.push(None)\nNone => {}\n}")), "cannot change `xs` while a `match` is looking inside it");
        assert!(check_src(&opt("Some(v) => print(v)\nNone => {}\n}\no = None\nmatch o {\n_ => o = Some(2)")).is_err());
        assert!(check_src(&in_main("var o = Some(1)\nmatch o {\nSome(v) => print(v)\nNone => {}\n}\no = None")).is_ok());
        // A computed value belongs to the statement, so the variable is free.
        assert!(check_src("fn id(o: Option[i64]) -> Option[i64] {\n return o\n}\nfn main() {\n var o = Some(1)\n match id(o) {\n Some(v) => o = Some(v + 1)\n None => {}\n }\n}").is_ok());

        assert_eq!(error(&in_main("match 3 {\n1 => {}\n}")), "a `match` on `i64` needs a final `_ =>` arm: the arms cannot list every value");
        assert_eq!(error(&in_main("match \"a\" {\n\"a\" => {}\n}")), "a `match` on `str` needs a final `_ =>` arm: the arms cannot list every value");
        assert_eq!(error(&in_main("match 3 {\n1 => {}\n1 => {}\n_ => {}\n}")), "this arm can never run: `1` is already handled above");
        assert_eq!(error(&in_main("match 3 {\n-1 => {}\n- 1 => {}\n_ => {}\n}")), "this arm can never run: `-1` is already handled above");
        assert_eq!(error(&in_main("match \"a\" {\n\"a\\n\" => {}\n\"a\\n\" => {}\n_ => {}\n}")), "this arm can never run: \"a\\n\" is already handled above");
        assert_eq!(error(&in_main("match 3 {\n9223372036854775808 => {}\n_ => {}\n}")), "this number does not fit in `i64`");
        assert!(check_src(&in_main("match 3 {\n-9223372036854775808 => {}\n9223372036854775807 => {}\n_ => {}\n}")).is_ok());
        assert_eq!(error(&in_main("match true {\ntrue => {}\n}")), "this `match` does not handle `false`; add an arm for it or a final `_ =>` arm");
        assert_eq!(error(&in_main("match true {\nfalse => {}\n}")), "this `match` does not handle `true`; add an arm for it or a final `_ =>` arm");
        assert_eq!(error(&in_main("match true {\ntrue => {}\ntrue => {}\n}")), "this arm can never run: `true` is already handled above");
        assert_eq!(error(&in_main("match true {\ntrue => {}\nfalse => {}\n_ => {}\n}")), "this arm can never run: the arms above already cover every case");
        assert_eq!(error(&in_main("match 3 {\ntrue => {}\n_ => {}\n}")), "this pattern is a `bool` but the `match` is on `i64`");
        assert_eq!(error(&in_main("match 3 {\n\"a\" => {}\n_ => {}\n}")), "this pattern is a string but the `match` is on `i64`");
        assert_eq!(error(&in_main("match 3 {\nSome(v) => {}\n_ => {}\n}")), "`Some` is a variant of `Option` but this `match` is on `i64`");
        assert_eq!(error(&in_main("match [1] {\n_ => {}\n}")), "`match` works on enums, `i64`, `bool` and `str` but this is `List[i64]`");
        assert_eq!(error(&in_main("match print(1) {\n_ => {}\n}")), "this expression has no value to match on");
        assert_eq!(error(&in_main("match 3 {\nn => n = 4\n}")), "cannot change `n`: a loop variable or pattern binding is read-only");
        // Arms that all return make the `match` return.
        assert!(check_src("fn f(o: Option[i64]) -> i64 {\n match o {\n Some(v) => return v\n None => return 0\n }\n}\nfn main() {\n}").is_ok());
        assert_eq!(error("fn f(o: Option[i64]) -> i64 {\n match o {\n Some(v) => return v\n None => {}\n }\n}\nfn main() {\n}"), "function `f` must return a value of type `i64` on every path");
    }

    /// Checks a program together with the standard library's azoL code.
    fn library_error(body: &str) -> Option<String> {
        library_error_with("", body)
    }

    /// The same, with declarations before `main`.
    fn library_error_with(declarations: &str, body: &str) -> Option<String> {
        let user = format!("{declarations}\n{}", in_main(body));
        let src = format!("{user}\n{}", include_str!("../lib/prelude.azl"));
        let tokens = lex(&src).expect("lexing failed");
        let ast = parse(&src, &tokens).expect("parsing failed");
        check_with_prelude(&ast, &src, user.len() as u32 + 1).err().map(|mut errors| errors.remove(0).message)
    }

    #[test]
    fn system_functions() {
        assert!(check_src(&in_main("let a: List[str] = args()\nlet r: Result[str, str] = read_file(\"a\")\nlet w: Result[i64, str] = write_file(\"a\", \"b\")\nlet c: str = chr(65)\neprint(c)\nexit(2)")).is_ok());
        assert_eq!(error(&in_main("args(1)")), "`args` takes 0 arguments but 1 were given");
        assert_eq!(error(&in_main("read_file()")), "`read_file` takes 1 argument but 0 were given");
        assert_eq!(error(&in_main("read_file(1)")), "argument 1 of `read_file` needs `str` but this is `i64`");
        assert_eq!(error(&in_main("write_file(\"a\", 1)")), "argument 2 of `write_file` needs `str` but this is `i64`");
        assert_eq!(error(&in_main("eprint(1)")), "argument 1 of `eprint` needs `str` but this is `i64`");
        assert_eq!(error(&in_main("exit(\"a\")")), "argument 1 of `exit` needs `i64` but this is `str`");
        assert_eq!(error(&in_main("let x = exit(1)")), "this expression has no value to store");
        assert_eq!(error(&in_main("let x = args")), "`args` is a function; call it with `args(...)`");
        assert!(check_src(&in_main("let r: Result[i64, str] = set_executable(\"a\")\nset_executable(\"b\" + \"c\")")).is_ok());
        assert_eq!(error(&in_main("set_executable()")), "`set_executable` takes 1 argument but 0 were given");
        assert_eq!(error(&in_main("set_executable(\"a\", \"b\")")), "`set_executable` takes 1 argument but 2 were given");
        assert_eq!(error(&in_main("set_executable(7)")), "argument 1 of `set_executable` needs `str` but this is `i64`");
        assert_eq!(error(&in_main("let r: Result[str, str] = set_executable(\"a\")")), "expected `Result[str, str]` but this is `Result[i64, str]`");
        assert_eq!(error(&in_main("let f = set_executable")), "`set_executable` is a function; call it with `set_executable(...)`");
        assert!(check_src(&in_main("let r: Result[i64, str] = remove_file(\"a\")\nremove_file(\"b\" + \"c\")")).is_ok());
        assert!(check_src(&in_main("let r: Result[i64, str] = rename_file(\"a\", \"b\")\nrename_file(\"b\" + \"c\", \"d\")")).is_ok());
        assert!(check_src(&in_main("let t: i64 = clock()\nlet d = clock() - t\nclock()")).is_ok());
        assert_eq!(error(&in_main("remove_file()")), "`remove_file` takes 1 argument but 0 were given");
        assert_eq!(error(&in_main("remove_file(7)")), "argument 1 of `remove_file` needs `str` but this is `i64`");
        assert_eq!(error(&in_main("rename_file(\"a\")")), "`rename_file` takes 2 arguments but 1 were given");
        assert_eq!(error(&in_main("rename_file(\"a\", 7)")), "argument 2 of `rename_file` needs `str` but this is `i64`");
        assert_eq!(error(&in_main("let r: Result[str, str] = rename_file(\"a\", \"b\")")), "expected `Result[str, str]` but this is `Result[i64, str]`");
        assert_eq!(error(&in_main("clock(1)")), "`clock` takes 0 arguments but 1 were given");
        assert_eq!(error(&in_main("let s: str = clock()")), "expected `str` but this is `i64`");
        for name in ["print", "eprint", "str", "chr", "args", "read_file", "write_file", "set_executable", "exit", "remove_file", "rename_file", "clock"] {
            assert_eq!(error(&format!("fn {name}() {{\n}}\nfn main() {{\n}}")), format!("`{name}` is built in and cannot be redefined"));
        }
    }

    #[test]
    fn string_bytes_and_slices() {
        assert!(check_src(&in_main("let s = \"abc\"\nlet b: i64 = s[0]\nlet t: str = s.slice(0, b)\nlet u = (s + t)[1] + s.slice(1, 2).len()")).is_ok());
        assert_eq!(error(&in_main("var s = \"abc\"\ns[0] = 65")), "a string cannot be changed in place; build a new one with `slice` and `+`");
        assert_eq!(error("fn f(b: mut i64) {\n}\nfn main() {\n var s = \"abc\"\n f(mut s[0])\n}"), "`mut` needs a variable or a part of a variable");
        assert_eq!(error(&in_main("let s = \"abc\"\nlet b = s[true]")), "an index needs `i64` but this is `bool`");
        assert_eq!(error(&in_main("let s = \"abc\"\nlet t = s.slice(1)")), "`slice` takes 2 arguments but 1 were given");
        assert_eq!(error(&in_main("let s = \"abc\"\nlet t = s.slice(1, \"b\")")), "argument 2 of `slice` needs `i64` but this is `str`");
        assert_eq!(error(&in_main("let b = true[0]")), "only a list or a string can be indexed but this is `bool`");
        // The library's methods need the library.
        assert_eq!(error(&in_main("let s = \"abc\"\nlet t = s.trim()")), "`str` has no method `trim`");
        assert_eq!(error(&in_main("let n = 1\nlet t = n.trim()")), "`i64` has no method `trim`");
        assert_eq!(error(&in_main("let xs = [1]\nxs.sort()")), "`List[i64]` has no method `sort`");
    }

    #[test]
    fn library_methods() {
        assert_eq!(library_error("let s = \" a,b \"\nlet t: str = s.trim()\nlet p: List[str] = t.split(\",\")\nlet j: str = p.join(\"-\")\nlet f: Option[i64] = s.find(\"a\")\nlet n: Option[i64] = \"12\".to_i64()\nlet b: bool = s.contains(\"a\") && s.starts_with(\" \") && s.ends_with(\" \")\nlet l: List[str] = (s + s).lines()"), None);
        assert_eq!(library_error("let s = \"a\"\nlet t = s.find()").as_deref(), Some("`find` takes 1 argument but 0 were given"));
        assert_eq!(library_error("let s = \"a\"\nlet t = s.find(1)").as_deref(), Some("argument 1 of `find` needs `str` but this is `i64`"));
        assert_eq!(library_error("let s = \"a\"\nlet t = s.trim(1)").as_deref(), Some("`trim` takes 0 arguments but 1 were given"));
        assert_eq!(library_error("let xs = [1]\nlet t = xs.join(\",\")").as_deref(), Some("`List[i64]` has no method `join`"));
        assert_eq!(library_error("let s = \"a\"\nlet t = s.nothing()").as_deref(), Some("`str` has no method `nothing`"));
        // Lists get generic methods, which fit any element type that can do what they need.
        assert_eq!(library_error("let xs = [1]\nlet a: bool = xs.contains(1)\nlet b: Option[i64] = xs.first()\nlet c: List[i64] = xs.reversed()\nlet d: Option[str] = [\"a\"].last()\nlet e: List[List[i64]] = [xs].reversed()"), None);
        assert_eq!(library_error("let xs = [1]\nlet a = xs.contains(\"a\")").as_deref(), Some("argument 1 of `contains` needs `i64` but this is `str`"));
        assert_eq!(library_error("let xs = [1]\nlet a = xs.contains()").as_deref(), Some("`contains` takes 1 argument but 0 were given"));
        assert_eq!(library_error("let xs = [1]\nlet a = xs.first(1)").as_deref(), Some("`first` takes 0 arguments but 1 were given"));
        assert_eq!(library_error("let n = 1\nlet a = n.first()").as_deref(), Some("`i64` has no method `first`"));
        assert_eq!(library_error("let xs = [[1]]\nlet a = xs.contains([1])").as_deref(), Some("lists cannot be compared with `==` yet (in `contains` with `T` = `List[i64]`)"));
        // The library's functions are methods only: their names stay free.
        assert_eq!(library_error("let t = str_find(\"a\", \"a\")").as_deref(), Some("unknown function `str_find`"));
        assert_eq!(library_error("let t = find(\"a\", \"a\")").as_deref(), Some("unknown function `find`"));
        let user = "fn str_find(a: i64) -> i64 {\n return a\n}\nfn find(a: i64) -> i64 {\n return str_find(a)\n}\nfn main() {\n let n = find(1)\n let at = \"abc\".find(\"c\")\n}";
        let src = format!("{user}\n{}", include_str!("../lib/prelude.azl"));
        let tokens = lex(&src).unwrap();
        let ast = parse(&src, &tokens).unwrap();
        assert!(check_with_prelude(&ast, &src, user.len() as u32 + 1).is_ok());
    }

    #[test]
    fn library_methods_that_change_their_receiver() {
        assert_eq!(library_error("var xs = [1]\nxs.extend([2, 3])\nxs.insert(0, 9)\nxs.truncate(2)\nlet n = xs.remove_at(0)"), None);
        assert_eq!(library_error("var m: Map[i64] = new_map()\nm.set(\"a\", 1)\nlet gone = m.remove(\"a\")\nlet n = m.len()"), None);
        // The receiver follows the rules of a `mut` argument.
        assert_eq!(library_error("let xs = [1]\nxs.extend([2])").as_deref(), Some("cannot change `xs`: declare it with `var` to make it changeable"));
        assert_eq!(library_error("[1].extend([2])").as_deref(), Some("`extend` changes its receiver, so it needs a variable or a part of a variable"));
        assert_eq!(library_error("var xs = [1]\nxs.extend(xs)").as_deref(), Some("`xs` is passed twice in one call and one of the uses is `mut`; copy it into a variable first"));
        assert_eq!(library_error("var xs = [1]\nfor x in xs {\n xs.extend([x])\n}").as_deref(), Some("cannot change `xs` while a loop is going through it"));
        assert_eq!(library_error("var xs = [1]\nxs.extend(mut [2])").as_deref(), Some("argument 1 of `extend` is not a `mut` parameter; remove `mut`"));
        assert_eq!(library_error_with("fn f(xs: List[i64]) {\n xs.truncate(0)\n}", "").as_deref(), Some("cannot change `xs`: a parameter is read-only unless it is declared `mut`"));
        assert_eq!(library_error_with("fn f(xs: mut List[i64]) {\n xs.truncate(0)\n}", "var xs = [1]\nf(mut xs)"), None);
        assert_eq!(library_error("let n = 5\nn.extend([1])").as_deref(), Some("`i64` has no method `extend`"));
        // The same method name on different types.
        assert_eq!(library_error("let a = 3.min(4)\nlet b = [3, 4].min()\nlet c = \"\".is_empty()\nlet d = [1].is_empty()"), None);
        assert_eq!(library_error("let a = true.is_empty()").as_deref(), Some("`bool` has no method `is_empty`"));
        assert_eq!(library_error("let a = 5.replace(\"a\", \"b\")").as_deref(), Some("`i64` has no method `replace`"));
    }

    #[test]
    fn how_numbers_are_written() {
        let underscore = "a `_` in a number stands between two digits, as in `1_000`";
        let zero = "a decimal number does not start with `0`; other bases are written `0x1F` and `0b1010`";
        for number in ["1__0", "1_", "0x_F", "0xF_", "0xF__F", "0b_1", "0b1_"] {
            assert_eq!(error(&in_main(&format!("let a = {number}"))), underscore, "{number}");
        }
        for number in ["007", "00", "0_1"] {
            assert_eq!(error(&in_main(&format!("let a = {number}"))), zero, "{number}");
        }
        assert!(check_src(&in_main("let a = 0 + 10 + 1_000 + 0x0F + 0xff_ff + 0b0_1 + 'a' + '_'")).is_ok());
        // In a pattern and in a constant too.
        assert_eq!(error(&in_main("match 7 {\n 007 => {}\n _ => {}\n}")), zero);
        assert_eq!(error("const A = 1__0\nfn main() {\n}"), underscore);
    }

    #[test]
    fn the_smallest_number_can_be_written() {
        assert!(check_src(&in_main("let a = -9223372036854775808\nlet b = -9_223_372_036_854_775_808\nlet c = - 9223372036854775808")).is_ok());
        assert!(check_src("const LOW = -9223372036854775808\nconst NEXT = LOW + 1\nfn main() {\n}").is_ok());
        assert_eq!(error(&in_main("let a = 9223372036854775808")), "this number does not fit in `i64`");
        assert_eq!(error(&in_main("let a = 1 - 9223372036854775808")), "this number does not fit in `i64`");
        assert_eq!(error(&in_main("let a = -9223372036854775809")), "this number does not fit in `i64`");
    }

    #[test]
    fn the_try_operator() {
        let number = "fn number(s: str) -> Result[i64, str] {\n return Ok(1)\n}\nfn words(s: str) -> Result[List[str], str] {\n return Ok([s])\n}\nfn maybe() -> Option[i64] {\n return None\n}\nstruct P {\n a: List[str]\n n: i64\n}\nstruct Q {\n n: i64\n a: List[str]\n}";
        let ok = |function: &str| check_src(&format!("{number}\n{function}\nfn main() {{\n}}")).is_ok();
        let bad = |function: &str| error(&format!("{number}\n{function}\nfn main() {{\n}}"));
        assert!(ok("fn f() -> Result[i64, str] {\n let n = number(\"1\")? + number(\"2\")?\n number(\"3\")?\n return Ok(n)\n}"));
        assert!(ok("fn f() -> Result[str, str] {\n let w = words(\"a\")?\n return Ok(w[0])\n}"));
        assert!(ok("fn f() -> Option[str] {\n let n = maybe()?\n return Some(str(n))\n}"));
        assert!(ok("fn f(r: Result[List[i64], str]) -> Result[i64, str] {\n return Ok(r?.len())\n}"));
        // The first part of a struct or variant, and after parts that own nothing.
        assert!(ok("fn f() -> Result[Q, str] {\n return Ok(Q { n: number(\"1\")?, a: words(\"a\")? })\n}"));
        assert!(ok("fn f() -> Result[P, str] {\n return Ok(P { a: words(\"a\")?, n: 1 })\n}"));
        // What it follows, and what the function returns.
        assert_eq!(bad("fn f() -> Result[i64, str] {\n let n = 1?\n return Ok(n)\n}"), "`?` needs a `Result` or an `Option` but this is `i64`");
        assert_eq!(bad("fn f() -> i64 {\n return number(\"1\")?\n}"), "`?` on `Result[i64, str]` needs a function that returns a `Result` with the error type `str`, but this function returns `i64`; use `match`");
        assert_eq!(bad("fn f() {\n number(\"1\")?\n}"), "`?` on `Result[i64, str]` needs a function that returns a `Result` with the error type `str`, but this function returns nothing; use `match`");
        assert_eq!(bad("fn f() -> Result[i64, i64] {\n return Ok(number(\"1\")?)\n}"), "`?` on `Result[i64, str]` needs a function that returns a `Result` with the error type `str`, but this function returns `Result[i64, i64]`; use `match`");
        assert_eq!(bad("fn f() -> Result[i64, str] {\n return Ok(maybe()?)\n}"), "`?` on `Option[i64]` needs a function that returns an `Option`, but this function returns `Result[i64, str]`; use `match`");
        assert_eq!(bad("fn f() -> Option[i64] {\n return Some(number(\"1\")?)\n}"), "`?` on `Result[i64, str]` needs a function that returns a `Result` with the error type `str`, but this function returns `Option[i64]`; use `match`");
        // Where leaving would lose memory.
        assert_eq!(bad("fn f() -> Result[List[i64], str] {\n return Ok([number(\"1\")?])\n}"), "`?` cannot be used inside a list literal; store its value in a variable first");
        assert_eq!(bad("fn f() -> Result[i64, str] {\n let n = [1, 2].len() + [number(\"1\")?].len()\n return Ok(n)\n}"), "`?` cannot be used inside a list literal; store its value in a variable first");
        assert_eq!(bad("fn f() -> Result[P, str] {\n return Ok(P { a: words(\"a\")?, n: number(\"1\")? })\n}"), "`?` cannot be used after a part of the same value that owns memory; store its value in a variable first");
        assert_eq!(bad("fn f(s: str) -> Result[P, str] {\n return Ok(P { a: [s], n: number(s)? })\n}"), "`?` cannot be used after a part of the same value that owns memory; store its value in a variable first");
        // A list literal that is finished does not count.
        assert!(ok("fn f() -> Result[i64, str] {\n let n = [1, 2].len() + number(\"1\")?\n return Ok(n)\n}"));
        assert!(ok("fn f(s: str) -> Result[Q, str] {\n let q = Q { n: 1, a: [s] }\n return Ok(Q { n: number(s)?, a: q.a })\n}"));
    }

    #[test]
    fn library_names_and_the_programs_own() {
        // A type of the library cannot be declared again.
        assert_eq!(library_error_with("struct Map {\n n: i64\n}", "").as_deref(), Some("`Map` is a type of the standard library; choose another name"));
        assert_eq!(library_error_with("enum Map {\n A\n}", "").as_deref(), Some("`Map` is a type of the standard library; choose another name"));
        // A plain function of the library gives way to the program's.
        assert_eq!(library_error_with("fn new_map() -> i64 {\n return 7\n}", "let n: i64 = new_map()"), None);
        assert_eq!(library_error_with("fn new_map[T](x: T) -> T {\n return x\n}", "let n: i64 = new_map(1)"), None);
        assert_eq!(library_error("let m = new_map()").as_deref(), Some("cannot tell what `V` is in this call to `new_map`: no argument settles it; store the result in a variable with a type"));
        // Methods are not plain functions, and a program may use their names.
        assert_eq!(library_error("var m: Map[i64] = new_map()\nmap_set(mut m, \"a\", 1)").as_deref(), Some("unknown function `map_set`"));
    }

    #[test]
    fn generic_functions() {
        let with = |functions: &str, body: &str| format!("{functions}\n{}", in_main(body));
        let id = "fn id[T](x: T) -> T {\n return x\n}";
        assert!(check_src(&with(id, "let a: i64 = id(1)\nlet b: str = id(\"x\")\nlet c: List[bool] = id([true])\nlet d: i64 = id(id(2))")).is_ok());
        assert_eq!(error(&with(id, "let a = id()")), "`id` takes 1 argument but 0 were given");
        assert_eq!(error(&with(id, "let a: bool = id(1)")), "expected `bool` but this is `i64`");
        assert_eq!(error(&with(id, "let a = id(print(1))")), "this expression has no value to pass");
        assert_eq!(error(&with(id, "let a = id")), "`id` is a function; call it with `id(...)`");
        assert_eq!(error(&with("fn same[T](a: T, b: T) {\n}", "same(1, \"x\")")), "argument 2 of `same` needs `i64` but this is `str`");
        assert_eq!(error(&with("fn head[T](xs: List[T]) {\n}", "head(1)")), "argument 1 of `head` needs `List[T]` but this is `i64`");
        assert_eq!(error(&with("fn head[T](xs: List[T]) -> T {\n return xs[0]\n}", "let a: i64 = head([\"a\"])")), "expected `i64` but this is `str`");
        // A type parameter no argument mentions comes from the expected type.
        let none = "fn none[T]() -> Option[T] {\n return None\n}";
        assert!(check_src(&with(none, "let a: Option[i64] = none()\nvar b: Option[str] = none()\nb = none()")).is_ok());
        assert_eq!(error(&with(none, "let a = none()")), "cannot tell what `T` is in this call to `none`: no argument settles it; store the result in a variable with a type");
        // Each use is checked with its own types, and mistakes name them.
        let show = "fn show[T](x: T) {\n print(x)\n}";
        assert!(check_src(&with(show, "show(1)\nshow(true)\nshow(\"x\")")).is_ok());
        assert_eq!(error(&with(show, "show(1)\nshow([1])")), "`print` takes an `i64`, a `bool` or a `str` but this is `List[i64]` (in `show` with `T` = `List[i64]`)");
        assert_eq!(error(&with("fn two[A, B](a: A, b: B) -> A {\n return b\n}", "two(1, true)")), "this function returns `i64` but this is `bool` (in `two` with `A` = `i64`, `B` = `bool`)");
        assert!(check_src(&with("fn two[A, B](a: A, b: B) -> A {\n return b\n}", "two(1, 2)")).is_ok());
        assert!(error(&with("fn grow[T](x: T) {\n grow([x])\n}", "grow(1)")).starts_with("`grow` is used with types that grow without end (in `grow` with `T` = `List[List["));
        // `mut` parameters and the exclusivity rules work as in any function.
        let add = "fn add[T](xs: mut List[T], x: T) {\n xs.push(x)\n}";
        assert!(check_src(&with(add, "var xs = [1]\nadd(mut xs, 2)")).is_ok());
        assert_eq!(error(&with(add, "var xs = [1]\nadd(xs, 2)")), "argument 1 of `add` can be changed by the function, so it must be written `mut <variable>`");
        assert_eq!(error(&with(add, "let xs = [1]\nadd(mut xs, 2)")), "cannot pass `xs` as `mut`: declare it with `var` to make it changeable");
        assert_eq!(error(&with(add, "var xs = [[1]]\nadd(mut xs, xs[0])")), "`xs` is passed twice in one call and one of the uses is `mut`; copy it into a variable first");
        assert_eq!(error(&with(add, "var xs = [1]\nadd(mut xs, \"a\")")), "argument 2 of `add` needs `i64` but this is `str`");
        assert_eq!(error(&with(add, "var xs = [1]\nfor x in xs {\n add(mut xs, x)\n}")), "cannot pass `xs` as `mut` while a loop is going through it");
        // Declarations.
        assert_eq!(error("fn f[T, T](x: T) {\n}\nfn main() {\n}"), "type parameter `T` is declared more than once");
        assert_eq!(error("fn main[T]() {\n}"), "`main` cannot have type parameters");
        assert_eq!(error("fn f[T](x: T) {\n}\nfn f(x: i64) {\n}\nfn main() {\n}"), "function `f` is defined more than once");
        assert_eq!(error("fn f(x: i64) {\n}\nfn f[T](x: T) {\n}\nfn main() {\n}"), "function `f` is defined more than once");
        assert_eq!(error("fn f[T](x: Missing) {\n}\nfn main() {\n}"), "unknown type `Missing`; the built-in types so far are `i64`, `bool`, `str`, `List`, `Option` and `Result`");
        assert_eq!(error("fn print[T](x: T) {\n}\nfn main() {\n}"), "`print` is built in and cannot be redefined");
        // The same types give the same function; new types give a new one.
        let src = with(id, "let a = id(1)\nlet b = id(2)\nlet c = id(true)");
        let tokens = lex(&src).unwrap();
        let ast = parse(&src, &tokens).unwrap();
        let prog = check(&ast, &src).unwrap();
        assert_eq!(prog.fns.len(), 3);
        assert_eq!(prog.fns.iter().filter(|f| f.generic.is_some()).map(|f| f.type_args.clone()).collect::<Vec<_>>(), vec![vec![Ty::I64], vec![Ty::Bool]]);
    }

    #[test]
    fn generic_structs() {
        let with = |body: &str| format!("struct Pair[A, B] {{\n first: A\n second: B\n}}\nstruct Stack[T] {{\n items: List[T]\n}}\n{}", in_main(body));
        assert!(check_src(&with("let p = Pair { first: 1, second: \"x\" }\nlet q: Pair[i64, str] = p\nlet a: i64 = p.first\nlet b: str = q.second\nlet s: Stack[bool] = Stack { items: [] }\nlet t = Stack { items: [p] }\nlet u: Stack[Pair[i64, str]] = t")).is_ok());
        assert_eq!(error(&with("let p = Pair { first: 1, second: \"x\" }\nlet q: Pair[str, i64] = p")), "expected `Pair[str, i64]` but this is `Pair[i64, str]`");
        assert_eq!(error(&with("let p: Pair[i64, i64] = Pair { first: 1, second: \"x\" }")), "field `second` needs `i64` but this is `str`");
        assert_eq!(error(&with("let p = Pair { first: 1 }")), "field `second` of `Pair` is missing");
        assert_eq!(error(&with("let p = Pair { first: 1, second: 2, third: 3 }")), "struct `Pair` has no field `third`");
        assert_eq!(error(&with("let p = Pair { first: 1, first: 2 }")), "field `first` is given more than once");
        assert_eq!(error(&with("let s = Stack { items: [] }")), "cannot tell what this empty list holds; give the variable a type, as in `var xs: List[i64] = []`");
        assert_eq!(error(&with("let s = Stack { items: 1 }")), "field `items` needs `List[T]` but this is `i64`");
        assert_eq!(error(&with("let p = Pair { first: print(1), second: 2 }")), "this expression has no value to store");
        assert_eq!(error(&format!("fn f(p: Pair) {{\n}}\n{}", with(""))), "`Pair` takes 2 type arguments, as in `Pair[i64, i64]`");
        assert_eq!(error(&format!("fn f(p: Stack[i64, i64]) {{\n}}\n{}", with(""))), "`Stack` takes 1 type argument, as in `Stack[i64]`");
        assert_eq!(error("struct S[T, T] {\n a: T\n}\nfn main() {\n}"), "type parameter `T` is declared more than once");
        assert_eq!(error("struct S[T] {\n a: S[T]\n}\nfn main() {\n let s: S[i64] = S { a: 1 }\n}"), "struct `S[i64]` contains itself, so it would have no finite size");
        assert_eq!(error("struct S[T] {\n a: S[List[T]]\n}\nfn main() {\n let s: Option[S[i64]] = None\n}"), "this type nests structs too deeply");
        assert_eq!(error("struct Unused[T] {\n a: T\n}\nstruct P {\n x: i64\n}\nfn main() {\n let p = P { x: 1 }\n let q = p.y\n}"), "struct `P` has no field `y`");
        // One type per combination of type arguments, laid out on its own.
        let src = with("let a = Pair { first: 1, second: \"x\" }\nlet b = Pair { first: \"y\", second: [1] }\nlet c = Pair { first: 2, second: \"z\" }");
        let tokens = lex(&src).unwrap();
        let ast = parse(&src, &tokens).unwrap();
        let prog = check(&ast, &src).unwrap();
        let shapes: Vec<(String, u32, usize)> = prog.structs.iter().map(|s| (s.name.clone(), s.size, s.heap.len())).collect();
        assert_eq!(shapes, vec![("Pair[i64, str]".to_string(), 4, 1), ("Pair[str, List[i64]]".to_string(), 6, 2)]);
        assert_eq!(prog.structs[1].fields[1].offset, 3);
    }

    /// The variables whose value is handed over, not copied, somewhere in `body`.
    fn moved_variables(body: &str) -> Vec<String> {
        let src = format!("struct Bag {{\n items: List[i64]\n name: str\n}}\nfn make() -> List[i64] {{\n return [1]\n}}\nfn size(o: Option[List[i64]]) -> i64 {{\n return 0\n}}\nfn change(xs: mut List[i64]) {{\n}}\n{}", in_main(body));
        let tokens = lex(&src).unwrap();
        let ast = parse(&src, &tokens).unwrap();
        let program = check(&ast, &src).unwrap_or_else(|e| panic!("{}", e.message));
        let mut names = Vec::new();
        for (id, expr) in ast.exprs.iter().enumerate() {
            if let Expr::Ident(name) = expr {
                if program.expr_moved[id] {
                    names.push(name.text(&src).to_string());
                }
            }
        }
        names
    }

    #[test]
    fn last_use_of_a_variable_moves() {
        let none: Vec<String> = Vec::new();
        // Stored and never mentioned again: handed over.
        assert_eq!(moved_variables("let a = make()\nlet b = a"), ["a"]);
        assert_eq!(moved_variables("let a = make()\nvar xs = [make()]\nxs.push(a)"), ["a"]);
        assert_eq!(moved_variables("let a = make()\nlet b = Bag { items: a, name: \"n\" }"), ["a"]);
        assert_eq!(moved_variables("let a = make()\nlet b = Some(a)"), ["a"]);
        assert_eq!(moved_variables("let a = make()\nlet b = [a]"), ["a"]);
        assert_eq!(moved_variables("let a = make()\nvar b = make()\nb = a"), ["a"]);
        assert_eq!(moved_variables("let a = make()\nlet b = a\nlet c = b"), ["a", "b"]);
        // Mentioned again afterwards, in any way: copied. Only the last store counts.
        assert_eq!(moved_variables("let a = make()\nlet b = a\nprint(a.len())"), none);
        assert_eq!(moved_variables("let a = make()\nlet b = a\nlet c = a"), ["a"]);
        assert_eq!(moved_variables("let a = make()\nlet b = [a, a]").len(), 1);
        assert_eq!(moved_variables("var a = make()\nlet b = a\na = make()"), none);
        assert_eq!(moved_variables("let a = make()\nif a.len() > 0 {\n let b = a\n} else {\n let c = a\n}").len(), 1);
        // A store that may run again must copy.
        assert_eq!(moved_variables("let a = make()\nvar i = 0\nwhile i < 2 {\n let b = a\n i = i + 1\n}"), none);
        assert_eq!(moved_variables("let a = make()\nfor x in [1, 2] {\n let b = a\n}"), none);
        assert_eq!(moved_variables("let a = make()\nwhile size(Some(a)) > 0 {\n}"), none);
        assert_eq!(moved_variables("for x in [1, 2] {\n let a = make()\n let b = a\n}"), ["a"]);
        assert_eq!(moved_variables("var i = 0\nwhile i < 2 {\n let a = make()\n var xs = [make()]\n xs.push(a)\n i = i + 1\n}"), ["a"]);
        // Something still looks into the variable: copied.
        assert_eq!(moved_variables("let a = Some(make())\nmatch a {\n Some(inner) => {\n  let b = a\n  print(inner.len())\n }\n None => {}\n}"), none);
        assert_eq!(moved_variables("let a = [make()]\nfor inner in a {\n let b = a\n}"), none);
        // Written earlier but read later: copied.
        assert_eq!(moved_variables("var a = [make()]\na[size(Some(a[0]))] = make()"), none);
        assert_eq!(moved_variables("var a = make()\nlet n = a[size(Some(a))]"), none);
        assert_eq!(moved_variables("var a = [make()]\nlet b = make()\na[0] = b"), ["b"]);
        assert_eq!(moved_variables("var a = make()\nvar b = [make()]\nb[a[0]] = a"), none);
        assert_eq!(moved_variables("var a = [make()]\na.push(a[0])"), none);
        // Parts of variables, parameters and views are never moved from.
        assert_eq!(moved_variables("let a = Bag { items: make(), name: \"n\" }\nlet b = a.items"), none);
        assert_eq!(moved_variables("let a = [make()]\nlet b = a[0]"), none);
        assert_eq!(moved_variables("let a = 1\nlet b = a"), none);
        // A variable with the same name declared later is a different one.
        assert_eq!(moved_variables("let a = make()\nlet a = a\nprint(a.len())"), ["a"]);
    }

    #[test]
    fn appending_to_a_string_in_place() {
        let flags = |body: &str| -> Vec<u8> {
            let src = format!("struct Named {{\n name: str\n}}\nfn grow(s: mut str) {{\n s = s + \"!\"\n}}\n{}", in_main(body));
            let tokens = lex(&src).unwrap();
            let ast = parse(&src, &tokens).unwrap();
            let program = check(&ast, &src).unwrap_or_else(|e| panic!("{}", e.message));
            ast.stmts.iter().enumerate().filter(|(_, s)| matches!(s, Stmt::Assign { .. })).map(|(id, _)| program.stmt_flag[id]).collect()
        };
        // The first assignment is the one inside `grow`.
        assert_eq!(flags("var s = \"a\"\ns = s + \"b\"\ns = s + s\ns = s + s.slice(0, 1)"), [APPEND, APPEND, APPEND, APPEND]);
        // Only the simple form: `s + a + b` adds to `s + a`, a new string.
        assert_eq!(flags("var s = \"a\"\ns = s + \"b\" + \"c\""), [APPEND, 0]);
        assert_eq!(flags("var n = Named { name: \"a\" }\nn.name = n.name + \"b\""), [APPEND, APPEND]);
        // Anything else builds a new string.
        assert_eq!(flags("var s = \"a\"\nvar t = \"b\"\ns = t + s\ns = \"x\" + s\nt = s"), [APPEND, 0, 0, 0]);
        assert_eq!(flags("var xs = [\"a\"]\nxs[0] = xs[0] + \"b\""), [APPEND, 0]);
        assert_eq!(flags("var n = 1\nn = n + 1"), [APPEND, 0]);
    }

    #[test]
    fn bit_operators_and_number_forms() {
        assert!(check_src(&in_main("let a: i64 = 1 & 2 | 3 ^ 4 << 5 >> 6\nlet b: bool = a & 1 == 1 && a | 2 > 0\nvar c = 0xFF\nc = c << 'a' - 0b1")).is_ok());
        assert_eq!(error(&in_main("let a = true & false")), "`&` needs `i64` but this is `bool`");
        assert_eq!(error(&in_main("let a = 1 | true")), "`|` needs `i64` but this is `bool`");
        assert_eq!(error(&in_main("let a = \"x\" ^ 1")), "`^` needs `i64` but this is `str`");
        assert_eq!(error(&in_main("let a = 1 << \"x\"")), "`<<` needs `i64` but this is `str`");
        assert_eq!(error(&in_main("let a = [1] >> 1")), "`>>` needs `i64` but this is `List[i64]`");
        assert_eq!(error(&in_main("let a: bool = 1 & 1")), "expected `bool` but this is `i64`");
        // `&` binds tighter than `==`, so this compares two numbers.
        assert_eq!(error(&in_main("let a = 1 & 2 == true")), "`==` needs `i64` but this is `bool`");

        let values = |body: &str| -> Vec<i64> {
            let src = in_main(body);
            let tokens = lex(&src).unwrap();
            let ast = parse(&src, &tokens).unwrap();
            check(&ast, &src).unwrap_or_else(|e| panic!("{}", e.message)).ints
        };
        assert_eq!(values("let a = 0x1F + 0xffff_FFFF + 0b1010_0001 + 0x0 + 0b0 + 12"), [31, 4294967295, 161, 0, 0, 12]);
        assert_eq!(values("let a = 0x7FFF_FFFF_FFFF_FFFF"), [i64::MAX]);
        assert_eq!(values("let a = 0xFFFF_FFFF_FFFF_FFFF + 0x8000_0000_0000_0000"), [-1, i64::MIN]);
        assert_eq!(values("let a = 0b1111111111111111111111111111111111111111111111111111111111111111"), [-1]);
        assert_eq!(values("let a = 0x0000_0000_0000_0000_0001"), [1]);
        assert_eq!(values("let a = 'a' + ' ' + '~' + '\\n' + '\\t' + '\\r' + '\\0' + '\\\\' + '\\'' + '\"'"), [97, 32, 126, 10, 9, 13, 0, 92, 39, 34]);
        assert_eq!(error(&in_main("let a = 0x1_0000_0000_0000_0000")), "this number does not fit in 64 bits");
        assert_eq!(error(&in_main("let a = 0b10000000000000000000000000000000000000000000000000000000000000000")), "this number does not fit in 64 bits");
        assert_eq!(error(&in_main("let a = 9223372036854775808")), "this number does not fit in `i64`");
        assert_eq!(error(&in_main("let a: bool = 'a'")), "expected `bool` but this is `i64`");

        // The same forms in patterns.
        assert_eq!(values("match 3 {\n0x10 => {}\n-0x10 => {}\n0b11 => {}\n'a' => {}\n-'a' => {}\n0xFFFF_FFFF_FFFF_FFFF => {}\n_ => {}\n}"), [3, 16, -16, 3, 97, -97, -1]);
        assert_eq!(error(&in_main("match 3 {\n16 => {}\n0x10 => {}\n_ => {}\n}")), "this arm can never run: `16` is already handled above");
        assert_eq!(error(&in_main("match 3 {\n'a' => {}\n97 => {}\n_ => {}\n}")), "this arm can never run: `97` is already handled above");
        assert_eq!(error(&in_main("match 3 {\n0x1_0000_0000_0000_0000 => {}\n_ => {}\n}")), "this number does not fit in 64 bits");
        assert_eq!(error(&in_main("match 3 {\n-0x8000_0000_0000_0000 => {}\n_ => {}\n}")), "this number does not fit in `i64`");
        assert_eq!(error(&in_main("match true {\n'a' => {}\n_ => {}\n}")), "this pattern is a number but the `match` is on `bool`");
    }

    #[test]
    fn constants() {
        let with = |consts: &str, body: &str| format!("{consts}\n{}", in_main(body));
        // All three types, every operator, any order of declaration.
        assert!(check_src(&with(
            "const B = A + 1\nconst A = 1\nconst MASK: i64 = (1 << 4) - 1 | 0x100 ^ 0b11 & 'a'\nconst YES: bool = !false && B >= 2 || A != 1\nconst NAME: str = \"n\"\nconst Q = 7 / 2 * 3 % 5 - -1 >> 1\nconst SAME = true == YES",
            "let a: i64 = A + B * MASK\nlet b: bool = YES && SAME\nlet c: str = NAME + NAME\nvar xs = [A, B]\nxs.push(Q)\nif A < B {\n print(NAME)\n}\nprint(A)\nprint(YES)"
        ))
        .is_ok());
        // The value is the one the program would compute.
        let src = with("const A = 1 << 65\nconst B = -9223372036854775807 - 1\nconst C = B / -1\nconst D = B % -1\nconst E = 9223372036854775807 + 1\nconst F = -8 >> 1\nconst G = 0xFFFF_FFFF_FFFF_FFFF", "print(A + B + C + D + E + F + G)");
        let tokens = lex(&src).unwrap();
        let ast = parse(&src, &tokens).unwrap();
        let prog = check(&ast, &src).unwrap();
        assert_eq!(&prog.ints[..7], &[2, i64::MIN, i64::MIN, 0, i64::MIN, -4, -1]);
        // A use reads as the literal: the name is marked, the value shared.
        let marks: Vec<u32> = prog.expr_aux.iter().copied().filter(|&aux| aux == CONST_INT).collect();
        assert_eq!(marks.len(), 7);
        // Declaration errors.
        assert_eq!(error(&with("const A = B\nconst B = A", "")), "constant `A` is defined in terms of itself");
        assert_eq!(error(&with("const A = A", "")), "constant `A` is defined in terms of itself");
        assert_eq!(error(&with("const A = 1\nconst A = 2", "")), "constant `A` is defined more than once");
        assert_eq!(error(&with("const A = 1 / 0", "")), "division by zero in a constant");
        assert_eq!(error(&with("const A = 1 % (2 - 2)", "")), "division by zero in a constant");
        assert_eq!(error(&with("const A = x", "")), "`x` is not a constant; a constant is made of literals and other constants");
        assert_eq!(error(&with("const A = [1]", "")), "a constant must be made of literals and other constants");
        assert_eq!(error("fn f() -> i64 {\n return 1\n}\nconst A = f()\nfn main() {\n}"), "a constant must be made of literals and other constants");
        assert!(check_src(&with("const A = \"a\" + \"b\"\nconst B = A + A + \"!\"", "let s: str = B")).is_ok());
        assert_eq!(error(&with("const A = \"a\" == \"a\"", "")), "string constants can be joined with `+`; no other operator works on them in a constant");
        assert_eq!(error(&with("const A = \"a\" + 1", "")), "`+` needs `str` but this is `i64`");
        assert_eq!(error(&with("const A = 1 + \"a\"", "")), "`+` needs `i64` but this is `str`");
        // `exit` ends a path like `return`.
        assert!(check_src("fn f(n: i64) -> i64 {\n if n > 0 {\n  return n\n }\n exit(2)\n}\nfn main() {\n}").is_ok());
        assert_eq!(error("fn f(n: i64) -> i64 {\n if n > 0 {\n  return n\n }\n eprint(\"no\")\n}\nfn main() {\n}"), "function `f` must return a value of type `i64` on every path");
        assert_eq!(error(&with("const A = 1 + true", "")), "`+` needs `i64` but this is `bool`");
        assert_eq!(error(&with("const A = true + 1", "")), "`+` needs `i64` but this is `bool`");
        assert_eq!(error(&with("const A = 1 && true", "")), "`&&` needs `bool` but this is `i64`");
        assert_eq!(error(&with("const A = true || 1", "")), "`||` needs `bool` but this is `i64`");
        assert_eq!(error(&with("const A = 1 == true", "")), "`==` needs `i64` but this is `bool`");
        assert_eq!(error(&with("const A = 1 << \"a\"", "")), "`<<` needs `i64` but this is `str`");
        assert_eq!(error(&with("const A = -true", "")), "`-` needs `i64` but this is `bool`");
        assert_eq!(error(&with("const A = !1", "")), "`!` needs `bool` but this is `i64`");
        assert_eq!(error(&with("const A: bool = 1", "")), "expected `bool` but this is `i64`");
        assert_eq!(error(&with("const A: str = true", "")), "expected `str` but this is `bool`");
        assert_eq!(error(&with("const A: Missing = 1", "")), "unknown type `Missing`; the built-in types so far are `i64`, `bool`, `str`, `List`, `Option` and `Result`");
        assert_eq!(error(&with("const A = 99999999999999999999", "")), "this number does not fit in `i64`");
        assert_eq!(error(&with("const A = 0x1_0000_0000_0000_0000", "")), "this number does not fit in 64 bits");
        // One name, one meaning.
        assert_eq!(error(&with("const A = 1\nfn A() {\n}", "")), "`A` is already a constant");
        assert_eq!(error(&with("fn A[T](x: T) {\n}\nconst A = 1", "")), "`A` is already a constant");
        assert_eq!(error(&with("const Some = 1", "")), "`Some` is already a variant of `Option`");
        assert_eq!(error(&with("enum E {\n A\n}\nconst A = 1", "")), "`A` is already a variant of `E`");
        assert_eq!(error(&with("const print = 1", "")), "`print` is built in and cannot be redefined");
        assert_eq!(error(&with("const A = 1", "let A = 2")), "`A` is a constant and cannot be used as a variable name");
        assert_eq!(error(&with("const A = 1", "for A in [1] {\n}")), "`A` is a constant and cannot be used as a variable name");
        assert_eq!(error(&with("const A = 1", "for i, A in [1] {\n}")), "`A` is a constant and cannot be used as a variable name");
        assert_eq!(error("const a = 1\nfn f(a: i64) {\n}\nfn main() {\n}"), "`a` is a constant and cannot be used as a variable name");
        // A constant is a value, not a place.
        assert_eq!(error(&with("const A = 1", "A = 2")), "cannot change `A`: it is a constant");
        assert_eq!(error(&with("const A = \"a\"", "A = A + \"b\"")), "cannot change `A`: it is a constant");
        assert_eq!(error("const A = 1\nfn f(x: mut i64) {\n}\nfn main() {\n f(mut A)\n}"), "cannot pass `A` as `mut`: it is a constant");
        assert_eq!(error(&with("const A = 1", "let b: bool = A")), "expected `bool` but this is `i64`");
        assert_eq!(error(&with("const A = 1", "A(2)")), "unknown function `A`");
        // In patterns a constant matches its value instead of binding a name.
        assert!(check_src(&with("const A = 1\nconst S = \"s\"\nconst T = true", "match 3 {\nA => {}\n2 => {}\n_ => {}\n}\nmatch \"x\" {\nS => {}\n_ => {}\n}\nmatch true {\nT => {}\nfalse => {}\n}")).is_ok());
        assert_eq!(error(&with("const A = 1", "match 3 {\nA => {}\n}")), "a `match` on `i64` needs a final `_ =>` arm: the arms cannot list every value");
        assert_eq!(error(&with("const A = 1", "match 3 {\n1 => {}\nA => {}\n_ => {}\n}")), "this arm can never run: `A` is already handled above");
        assert_eq!(error(&with("const A = 1\nconst B = 2 - 1", "match 3 {\nA => {}\nB => {}\n_ => {}\n}")), "this arm can never run: `B` is already handled above");
        assert_eq!(error(&with("const A = 1", "match 3 {\nA => {}\n1 => {}\n_ => {}\n}")), "this arm can never run: `1` is already handled above");
        assert_eq!(error(&with("const T = true", "match true {\nT => {}\n}")), "this `match` does not handle `false`; add an arm for it or a final `_ =>` arm");
        assert_eq!(error(&with("const A = 1", "match \"s\" {\nA => {}\n_ => {}\n}")), "this pattern is a number but the `match` is on `str`");
        assert_eq!(error(&with("const S = \"s\"", "match 1 {\nS => {}\n_ => {}\n}")), "this pattern is a string but the `match` is on `i64`");
        assert_eq!(error(&with("const A = 1", "let o = Some(1)\nmatch o {\nA => {}\n_ => {}\n}")), "this pattern is a number but the `match` is on `Option[i64]`");
        assert_eq!(error(&with("const A = 1", "let o = Some(1)\nmatch o {\nSome(A) => {}\n_ => {}\n}")), "a pattern inside a pattern is not supported yet; give the value a name and `match` on it inside the arm");
        // The standard library's own names never clash with a program's constants.
        assert_eq!(library_error_with("const n = 2\nconst part = 3\nconst at = 4\nconst s = 5\nconst i = 6", "let found = \"a,b\".split(\",\").len() + n + part + at + s + i\nlet c = \"abc\".contains(\"c\")\nlet t = \" x \".trim()"), None);
    }

    #[test]
    fn equality_of_enums_without_values() {
        let with = |body: &str| format!("enum Kind {{\n Ident\n Number\n Plus\n}}\nenum Shape {{\n Dot\n Circle(i64)\n}}\nenum Flag[T] {{\n On\n Off\n}}\n{}", in_main(body));
        assert!(check_src(&with("let k = Ident\nlet same: bool = k == Number\nlet other: bool = k != Plus\nif Ident == k && Plus != Number {\n}\nlet f: Flag[i64] = On\nlet g: bool = f == Off\nvar ks = [Ident, Plus]\nwhile ks[0] == ks[1] {\n}")).is_ok());
        assert_eq!(error(&with("let s = Dot\nlet b = s == Dot")), "`==` works only on enums whose variants hold no values; use `match` for `Shape`");
        assert_eq!(error(&with("let s = Circle(1)\nlet b = s != s")), "`!=` works only on enums whose variants hold no values; use `match` for `Shape`");
        assert_eq!(error(&with("let k = Ident\nlet b = k == 1")), "`==` needs `Kind` but this is `i64`");
        assert_eq!(error(&with("let k = Ident\nlet b = k == Dot")), "`==` needs `Kind` but this is `Shape`");
        assert_eq!(error(&with("let k = Ident\nlet f: Flag[i64] = On\nlet b = f == k")), "`==` needs `Flag[i64]` but this is `Kind`");
        assert_eq!(error(&with("let f: Flag[i64] = On\nlet g: Flag[bool] = On\nlet b = f == g")), "`==` needs `Flag[i64]` but this is `Flag[bool]`");
        assert_eq!(error(&with("let k = Ident\nlet b = k < Number")), "`<` needs `i64` but this is `Kind`");
    }

    #[test]
    fn features_not_built_yet_are_reported_clearly() {
        assert_eq!(error(&in_main("let a = 1.5")), "there are no floating-point numbers yet; the only number type is `i64`");
        assert_eq!(error("fn f(a: i32) {\n}\nfn main() {\n}"), "unknown type `i32`; the built-in types so far are `i64`, `bool`, `str`, `List`, `Option` and `Result`");
    }
}
