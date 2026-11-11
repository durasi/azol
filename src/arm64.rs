// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Turns a checked program into ARM64 machine code in one pass over the
//! syntax tree. The scheme is deliberately simple: a scalar expression
//! leaves its value in x0, an aggregate expression leaves its address in
//! x0, intermediate values go on the stack, and locals live in the
//! function's frame. It is fast to generate, not fast to run; an
//! optimizing backend comes later.
//!
//! An enum value is a tag in its first slot (the variant's number plus
//! one) followed by that variant's fields.
//!
//! Registers: x0-x2 carry values, x8 the address for an aggregate
//! result, x9-x11 are scratch within one step. Nothing is kept in a
//! register across a call.

use crate::ast::*;
use crate::runtime::{self, RuntimeReloc, RUNTIME, RUNTIME_DATA, RUNTIME_DATA_OFFSETS, RUNTIME_RELOCS};
use crate::sema::{
    idle_slot, Program, Ty, BUILTIN_LEN, BUILTIN_LIST_POP, BUILTIN_LIST_PUSH, BUILTIN_PRINT_BOOL, BUILTIN_PRINT_I64,
    BUILTIN_PRINT_STR, BUILTIN_STR_FROM_BOOL, BUILTIN_STR_FROM_I64, BUILTIN_VARIANT, BY_REF, COPIED, FIRST_BUILTIN,
    IN_PLACE, PAT_BIND, PAT_LITERAL, PAT_VARIANT, VARIANT,
};
use std::collections::HashMap;

pub struct Code {
    pub words: Vec<u32>,
    /// Index of the first instruction of `main`.
    pub entry: u32,
    /// Read-only bytes placed after the code: runtime text, then the
    /// program's string literals.
    pub data: Vec<u8>,
    /// Instructions the linker completes once addresses are known.
    pub relocs: Vec<Reloc>,
}

pub enum Reloc {
    /// Three words at `at`: load an imported function's address and call it.
    Import { at: u32, import: u32 },
    /// Two words at `at`: put the address of `data[offset]` in `reg`.
    Data { at: u32, offset: u32, reg: u32 },
}

const X0: u32 = 0;
const X1: u32 = 1;
const X2: u32 = 2;
const X8: u32 = 8;
const X9: u32 = 9;
const X10: u32 = 10;
const X11: u32 = 11;
const X19: u32 = 19;
const X20: u32 = 20;
const X21: u32 = 21;
const FP: u32 = 29;
const XZR: u32 = 31;

// Condition codes.
const EQ: u32 = 0;
const NE: u32 = 1;
const GE: u32 = 10;
const LT: u32 = 11;
const GT: u32 = 12;
const LE: u32 = 13;
/// Unsigned higher or same.
const HS: u32 = 2;

const RET: u32 = 0xD65F_03C0;
const BRK_1: u32 = 0xD420_0020;
const PUSH_X0: u32 = 0xF81F_0FE0; // str x0, [sp, #-16]!

fn pop(reg: u32) -> u32 {
    0xF841_07E0 | reg // ldr reg, [sp], #16
}

/// `ldr rt, [rn, #8 * slots]`
fn ldr(rt: u32, rn: u32, slots: u32) -> u32 {
    0xF940_0000 | slots << 10 | rn << 5 | rt
}

/// `str rt, [rn, #8 * slots]`
fn str(rt: u32, rn: u32, slots: u32) -> u32 {
    0xF900_0000 | slots << 10 | rn << 5 | rt
}

/// `add rd, rn, #8 * slots`
fn add_slots(rd: u32, rn: u32, slots: u32) -> u32 {
    0x9100_0000 | (slots * 8) << 10 | rn << 5 | rd
}

/// `add rd, rn, rm`
fn add_reg(rd: u32, rn: u32, rm: u32) -> u32 {
    0x8B00_0000 | rm << 16 | rn << 5 | rd
}

/// `add rd, rn, rm, lsl #3`
fn add_scaled(rd: u32, rn: u32, rm: u32) -> u32 {
    0x8B00_0C00 | rm << 16 | rn << 5 | rd
}

/// `madd rd, rn, rm, ra`: rd = ra + rn * rm
fn madd(rd: u32, rn: u32, rm: u32, ra: u32) -> u32 {
    0x9B00_0000 | rm << 16 | ra << 10 | rn << 5 | rd
}

/// Frame slot `n` sits above the saved frame pointer and return address.
const fn frame(slot: u32) -> u32 {
    slot + 2
}

/// `cmp x9, #value`
fn cmp_x9_imm(value: u32) -> u32 {
    0xF100_013F | value << 10
}

fn cset_x0(cond: u32) -> u32 {
    0x9A9F_07E0 | (cond ^ 1) << 12
}

/// Largest slot offset a single load or store can carry.
const MAX_OFFSET: u32 = 4095;

struct LoopFixups {
    start: usize,
    breaks: Vec<usize>,
    /// In a `for` loop `continue` jumps forward to the step, so those
    /// jumps are patched later. `None` for `while`.
    continues: Option<Vec<usize>>,
    /// Number of owning locals alive when the loop was entered.
    live: usize,
}

/// Where a variable or field path lives: a frame slot, or an offset from
/// the pointer stored in a frame slot.
#[derive(Clone, Copy)]
struct Place {
    slot: u32,
    by_ref: bool,
    offset: u32,
}

struct Gen<'a> {
    ast: &'a Ast,
    prog: &'a Program<'a>,
    words: Vec<u32>,
    fn_starts: Vec<u32>,
    /// (instruction index, function index) of every `bl` to patch.
    calls: Vec<(usize, u32)>,
    /// (instruction index, runtime entry) of every `bl` into the runtime.
    runtime_calls: Vec<(usize, u32)>,
    /// Helper functions the compiler writes itself: one to free and one to
    /// copy each type whose values own other values, and each enum that
    /// owns anything. `true` means copy.
    glue: Vec<(Ty, bool)>,
    glue_ids: HashMap<(Ty, bool), usize>,
    /// (instruction index, index into `glue`) of every `bl` to a helper.
    glue_calls: Vec<(usize, usize)>,
    relocs: Vec<Reloc>,
    loops: Vec<LoopFixups>,
    /// Local variables that own buffers and are in scope: slot and type.
    live: Vec<(u32, Ty)>,
    frame_bytes: u32,
    ret: Ty,
    sret_slot: Option<u32>,
    scratch_slot: u32,
}

pub fn generate(ast: &Ast, prog: &Program) -> Code {
    let mut g = Gen {
        ast,
        prog,
        words: Vec::with_capacity(ast.exprs.len() * 3 + 256),
        fn_starts: Vec::with_capacity(prog.fns.len()),
        calls: Vec::new(),
        runtime_calls: Vec::new(),
        glue: Vec::new(),
        glue_ids: HashMap::new(),
        glue_calls: Vec::new(),
        relocs: Vec::new(),
        loops: Vec::new(),
        live: Vec::new(),
        frame_bytes: 0,
        ret: Ty::Unit,
        sret_slot: None,
        scratch_slot: 0,
    };
    let mut index = 0;
    for item in &ast.items {
        if let Item::Fn { params, body, .. } = *item {
            g.function(index, params, body);
            index += 1;
        }
    }
    // Then the helpers that free and copy nested values. Writing one can
    // ask for another, so the list grows while it is walked.
    let mut glue_starts = Vec::new();
    let mut next = 0;
    while next < g.glue.len() {
        glue_starts.push(g.words.len() as u32);
        let (ty, copy) = g.glue[next];
        g.glue_function(ty, copy);
        next += 1;
    }
    // The runtime follows.
    let runtime_start = g.words.len() as u32;
    g.words.extend_from_slice(&RUNTIME);
    for reloc in &RUNTIME_RELOCS {
        g.relocs.push(match *reloc {
            RuntimeReloc::Import { at, import } => Reloc::Import { at: runtime_start + at, import },
            RuntimeReloc::Data { at, data, reg } => {
                Reloc::Data { at: runtime_start + at, offset: RUNTIME_DATA_OFFSETS[data as usize], reg }
            }
        });
    }
    let bl = |from: usize, to: u32| 0x9400_0000 | ((to as i64 - from as i64) as u32 & 0x03FF_FFFF);
    for &(at, target) in &g.calls {
        g.words[at] = bl(at, g.fn_starts[target as usize]);
    }
    for &(at, entry) in &g.runtime_calls {
        g.words[at] = bl(at, runtime_start + entry);
    }
    for &(at, id) in &g.glue_calls {
        g.words[at] = bl(at, glue_starts[id]);
    }
    let mut data = Vec::with_capacity(RUNTIME_DATA.len() + prog.data.len());
    data.extend_from_slice(RUNTIME_DATA);
    data.extend_from_slice(&prog.data);
    Code { entry: g.fn_starts[prog.main as usize], words: g.words, data, relocs: g.relocs }
}

impl<'a> Gen<'a> {
    #[inline(always)]
    fn emit(&mut self, word: u32) {
        self.words.push(word);
    }

    /// Emits a placeholder branch and returns its index for patching.
    fn placeholder(&mut self) -> usize {
        self.words.push(0);
        self.words.len() - 1
    }

    fn patch_cbz(&mut self, at: usize, nonzero: bool) {
        self.patch_cbz_reg(at, nonzero, X0);
    }

    /// Completes a `cbz reg` (or `cbnz reg`) at `at` to jump here.
    fn patch_cbz_reg(&mut self, at: usize, nonzero: bool, reg: u32) {
        let delta = (self.words.len() - at) as u32;
        let op = if nonzero { 0xB500_0000 } else { 0xB400_0000 };
        self.words[at] = op | (delta & 0x7_FFFF) << 5 | reg;
    }

    /// Completes a conditional branch at `at` to jump here.
    fn patch_branch_if(&mut self, at: usize, cond: u32) {
        let delta = (self.words.len() - at) as u32;
        self.words[at] = 0x5400_0000 | (delta & 0x7_FFFF) << 5 | cond;
    }

    fn patch_jump(&mut self, at: usize) {
        let delta = (self.words.len() - at) as u32;
        self.words[at] = 0x1400_0000 | (delta & 0x03FF_FFFF);
    }

    fn jump_back(&mut self, to: usize) {
        let delta = to as i64 - self.words.len() as i64;
        self.emit(0x1400_0000 | (delta as u32 & 0x03FF_FFFF));
    }

    /// Calls a runtime routine.
    fn runtime(&mut self, entry: u32) {
        let at = self.placeholder();
        self.runtime_calls.push((at, entry));
    }

    fn mov_imm(&mut self, reg: u32, value: i64) {
        let bits = value as u64;
        let mut first = true;
        for chunk in 0..4 {
            let part = ((bits >> (16 * chunk)) & 0xFFFF) as u32;
            if part == 0 && !(first && chunk == 3) {
                continue;
            }
            let op = if first { 0xD280_0000 } else { 0xF280_0000 }; // movz / movk
            self.emit(op | chunk << 21 | part << 5 | reg);
            first = false;
        }
    }

    /// `reg = address of frame slot`. Slots far from the frame pointer
    /// need two instructions.
    fn frame_address(&mut self, reg: u32, slot: u32) {
        let bytes = frame(slot) * 8;
        if bytes < 4096 {
            self.emit(add_slots(reg, FP, frame(slot)));
        } else {
            self.emit(0x9140_0000 | (bytes >> 12) << 10 | FP << 5 | reg); // add reg, x29, #high, lsl #12
            self.emit(0x9100_0000 | (bytes & 0xFFF) << 10 | reg << 5 | reg); // add reg, reg, #low
        }
    }

    fn ty(&self, id: ExprId) -> Ty {
        self.prog.expr_ty[id as usize]
    }

    /// True for values handled by address: structs, strings and lists.
    fn is_aggregate(&self, id: ExprId) -> bool {
        self.ty(id).is_aggregate()
    }

    fn function(&mut self, index: u32, params: List, body: List) {
        let info = &self.prog.fns[index as usize];
        self.fn_starts.push(self.words.len() as u32);
        self.frame_bytes = (16 + 8 * info.slots).div_ceil(16) * 16;
        self.ret = info.ret;
        self.sret_slot = info.sret_slot;
        self.scratch_slot = info.scratch_slot;
        self.live.clear();

        // A large frame takes two steps: the amount is split into what
        // fits one instruction shifted by 12 bits and what fits one plain.
        if self.frame_bytes >> 12 > 0 {
            self.emit(0xD140_03FF | (self.frame_bytes >> 12) << 10); // sub sp, sp, #high, lsl #12
        }
        self.emit(0xD100_03FF | (self.frame_bytes & 0xFFF) << 10); // sub sp, sp, #low
        self.emit(0xA900_7BFD); // stp x29, x30, [sp]
        self.emit(0x9100_03FD); // mov x29, sp
        // Each parameter arrives in a register as a value or a pointer.
        for slot in 0..self.ast.list(params).len() as u32 {
            self.emit(str(slot, FP, frame(slot)));
        }
        if let Some(slot) = self.sret_slot {
            self.emit(str(X8, FP, frame(slot)));
        }
        // A temporary may be freed without having been filled, when the
        // expression that fills it was skipped. Null makes that harmless.
        for &slot in &info.zero_slots {
            self.emit(str(XZR, FP, frame(slot)));
        }
        self.block(body);
        if self.ret == Ty::Unit {
            self.mov_imm(X0, 0);
            self.epilogue();
        } else {
            // The checker proved every path returns; this is unreachable.
            self.emit(BRK_1);
        }
    }

    fn epilogue(&mut self) {
        self.emit(0x9100_03BF); // mov sp, x29
        self.emit(0xA940_7BFD); // ldp x29, x30, [sp]
        self.emit(0x9100_03FF | (self.frame_bytes & 0xFFF) << 10); // add sp, sp, #low
        if self.frame_bytes >> 12 > 0 {
            self.emit(0x9140_03FF | (self.frame_bytes >> 12) << 10); // add sp, sp, #high, lsl #12
        }
        self.emit(RET);
    }

    /// Generates a block and frees the owning locals declared in it.
    fn block(&mut self, body: List) {
        let outer = self.live.len();
        for &id in self.ast.list(body) {
            self.stmt(id);
        }
        self.drop_live_from(outer);
        self.live.truncate(outer);
    }

    // ---- ownership -----------------------------------------------------

    /// Frees everything owned by the value whose address is in x0.
    fn drop_value(&mut self, ty: Ty) {
        match ty {
            Ty::Str => self.runtime(runtime::DROP),
            Ty::List(list) if !self.prog.needs_drop(self.prog.lists[list as usize]) => self.runtime(runtime::DROP),
            _ => self.glue_call(ty, false),
        }
    }

    /// The value whose address is in x0 was copied bit by bit from another
    /// owner. Gives it buffers of its own, all the way down.
    fn clone_value(&mut self, ty: Ty) {
        match ty {
            Ty::Str => {
                self.mov_imm(X1, 1);
                self.runtime(runtime::LIST_CLONE);
            }
            Ty::List(list) if !self.prog.needs_drop(self.prog.lists[list as usize]) => {
                self.mov_imm(X1, self.prog.size_of(self.prog.lists[list as usize]) as i64 * 8);
                self.runtime(runtime::LIST_CLONE);
            }
            _ => self.glue_call(ty, true),
        }
    }

    /// Calls the helper that frees (or copies) a `ty`, asking for it to
    /// be written if this is its first use.
    fn glue_call(&mut self, ty: Ty, copy: bool) {
        let next = self.glue.len();
        let id = *self.glue_ids.entry((ty, copy)).or_insert(next);
        if id == next {
            self.glue.push((ty, copy));
        }
        let at = self.placeholder();
        self.glue_calls.push((at, id));
    }

    /// Writes the helper for one type. It takes the value's address in x0.
    fn glue_function(&mut self, ty: Ty, copy: bool) {
        match ty {
            Ty::Struct(_) => {
                // Handle each list and string directly inside the struct.
                self.emit(0xA9BE_7BFD); // stp x29, x30, [sp, #-32]!
                self.emit(0x9100_03FD); // mov x29, sp
                self.emit(0xF900_0BF3); // str x19, [sp, #16]
                self.emit(0xAA00_03F3); // mov x19, x0
                for (offset, field) in self.prog.owned(ty) {
                    self.emit(add_slots(X0, X19, offset));
                    if copy {
                        self.clone_value(field);
                    } else {
                        self.drop_value(field);
                    }
                }
                self.emit(0xF940_0BF3); // ldr x19, [sp, #16]
                self.emit(0xA8C2_7BFD); // ldp x29, x30, [sp], #32
                self.emit(RET);
            }
            Ty::List(list) => {
                // A list whose elements own something: visit every element.
                let elem = self.prog.lists[list as usize];
                let elem_bytes = self.prog.size_of(elem) * 8;
                self.emit(0xA9BD_7BFD); // stp x29, x30, [sp, #-48]!
                self.emit(0x9100_03FD); // mov x29, sp
                self.emit(0xA901_53F3); // stp x19, x20, [sp, #16]
                self.emit(0xF900_13F5); // str x21, [sp, #32]
                self.emit(0xAA00_03F3); // mov x19, x0
                if copy {
                    // First a private buffer holding the same bits.
                    self.mov_imm(X1, elem_bytes as i64);
                    self.runtime(runtime::LIST_CLONE);
                }
                self.emit(ldr(X9, X19, 2)); // capacity
                let nothing_owned = self.placeholder();
                self.emit(ldr(X20, X19, 0)); // x20 walks the elements
                self.emit(ldr(X21, X19, 1)); // x21 counts down the length
                let top = self.words.len();
                let finished = self.placeholder();
                self.emit(0xAA14_03E0); // mov x0, x20
                if copy {
                    self.clone_value(elem);
                } else {
                    self.drop_value(elem);
                }
                if elem_bytes < 4096 {
                    self.emit(0x9100_0294 | elem_bytes << 10); // add x20, x20, #bytes
                } else {
                    self.mov_imm(X10, elem_bytes as i64);
                    self.emit(add_reg(X20, X20, X10));
                }
                self.emit(0xD100_06B5); // sub x21, x21, #1
                self.jump_back(top);
                self.patch_cbz_reg(finished, false, X21);
                if !copy {
                    self.emit(ldr(X0, X19, 0));
                    self.runtime(runtime::FREE);
                }
                self.patch_cbz_reg(nothing_owned, false, X9);
                self.emit(0xF940_13F5); // ldr x21, [sp, #32]
                self.emit(0xA941_53F3); // ldp x19, x20, [sp, #16]
                self.emit(0xA8C3_7BFD); // ldp x29, x30, [sp], #48
                self.emit(RET);
            }
            Ty::Enum(index) => {
                // Only the variant the value is in owns anything, so the
                // tag selects what to visit.
                let prog = self.prog;
                self.emit(0xA9BE_7BFD); // stp x29, x30, [sp, #-32]!
                self.emit(0x9100_03FD); // mov x29, sp
                self.emit(0xF900_0BF3); // str x19, [sp, #16]
                self.emit(0xAA00_03F3); // mov x19, x0
                self.emit(ldr(X9, X19, 0)); // tag
                let mut done = Vec::new();
                for (variant, info) in prog.enums[index as usize].variants.iter().enumerate() {
                    if info.heap.is_empty() {
                        continue;
                    }
                    self.emit(cmp_x9_imm(variant as u32 + 1));
                    let next = self.placeholder();
                    for &(offset, leaf) in &info.heap {
                        self.emit(add_slots(X0, X19, offset));
                        if copy {
                            self.clone_value(leaf);
                        } else {
                            self.drop_value(leaf);
                        }
                    }
                    done.push(self.placeholder());
                    self.patch_branch_if(next, NE);
                }
                for at in done {
                    self.patch_jump(at);
                }
                self.emit(0xF940_0BF3); // ldr x19, [sp, #16]
                self.emit(0xA8C2_7BFD); // ldp x29, x30, [sp], #32
                self.emit(RET);
            }
            _ => unreachable!("only structs, enums and lists of owning elements need a helper"),
        }
    }

    /// Frees everything owned by the value in frame slot `slot`.
    fn drop_frame(&mut self, slot: u32, ty: Ty) {
        self.frame_address(X0, slot);
        self.drop_value(ty);
    }

    /// Frees everything owned by the value whose address is parked in
    /// frame slot `pointer_slot`.
    fn drop_pointer(&mut self, pointer_slot: u32, ty: Ty) {
        self.emit(ldr(X0, FP, frame(pointer_slot)));
        self.drop_value(ty);
    }

    /// Frees the owning locals declared since `from`, newest first,
    /// without forgetting them: used on every way out of a scope.
    fn drop_live_from(&mut self, from: usize) {
        for i in (from..self.live.len()).rev() {
            let (slot, ty) = self.live[i];
            self.drop_frame(slot, ty);
        }
    }

    /// Frees the temporaries of statement `id`. `keep_x0` preserves a
    /// value the statement still needs.
    fn drop_temps(&mut self, id: StmtId, keep_x0: bool) {
        let (start, len) = self.prog.stmt_drops[id as usize];
        if len == 0 {
            return;
        }
        if keep_x0 {
            self.emit(PUSH_X0);
        }
        for i in start..start + len {
            let (slot, ty) = self.prog.drops[i as usize];
            self.drop_frame(slot, ty);
            // Inside a loop the same temporary is freed again, and it may
            // not have been refilled: a zero capacity or tag marks it as
            // empty.
            for (offset, leaf) in self.prog.owned(ty) {
                self.emit(str(XZR, FP, frame(slot + offset + idle_slot(leaf))));
            }
        }
        if keep_x0 {
            self.emit(pop(X0));
        }
    }

    /// The value in frame slot `slot` was copied bit by bit from another
    /// owner. Gives it buffers of its own.
    fn clone_frame(&mut self, slot: u32, ty: Ty) {
        self.frame_address(X0, slot);
        self.clone_value(ty);
    }

    /// Like `clone_frame`, for the value whose address is parked in frame
    /// slot `pointer_slot`.
    fn clone_pointer(&mut self, pointer_slot: u32, ty: Ty) {
        self.emit(ldr(X0, FP, frame(pointer_slot)));
        self.clone_value(ty);
    }

    // ---- statements ----------------------------------------------------

    fn stmt(&mut self, id: StmtId) {
        match self.ast.stmts[id as usize] {
            Stmt::Let { value, .. } => {
                let slot = self.prog.stmt_slot[id as usize];
                let ty = self.ty(value);
                self.store(value, slot);
                self.drop_temps(id, false);
                if self.prog.needs_drop(ty) {
                    self.live.push((slot, ty));
                }
            }
            Stmt::Assign { target, value } => {
                self.assign(id, target, value);
                self.drop_temps(id, false);
            }
            Stmt::Expr(expr) => {
                self.eval(expr);
                self.drop_temps(id, false);
            }
            Stmt::Return(value) => self.return_stmt(id, value),
            Stmt::If { cond, then_block, else_branch } => {
                self.value(cond);
                self.drop_temps(id, true);
                let skip_then = self.placeholder();
                self.block(then_block);
                match else_branch {
                    Else::None => self.patch_cbz(skip_then, false),
                    Else::Block(body) => {
                        let skip_else = self.placeholder();
                        self.patch_cbz(skip_then, false);
                        self.block(body);
                        self.patch_jump(skip_else);
                    }
                    Else::If(next) => {
                        let skip_else = self.placeholder();
                        self.patch_cbz(skip_then, false);
                        self.stmt(next);
                        self.patch_jump(skip_else);
                    }
                }
            }
            Stmt::While { cond, body } => {
                let start = self.words.len();
                self.value(cond);
                self.drop_temps(id, true);
                let exit = self.placeholder();
                self.loops.push(LoopFixups { start, breaks: Vec::new(), continues: None, live: self.live.len() });
                self.block(body);
                self.jump_back(start);
                self.patch_cbz(exit, false);
                let fixups = self.loops.pop().expect("loop stack is balanced");
                for at in fixups.breaks {
                    self.patch_jump(at);
                }
            }
            Stmt::For { index, iter, body, .. } => self.for_stmt(id, index.is_some(), iter, body),
            Stmt::Break => {
                let live = self.loops.last().expect("checked: inside a loop").live;
                self.drop_live_from(live);
                let at = self.placeholder();
                self.loops.last_mut().expect("checked: inside a loop").breaks.push(at);
            }
            Stmt::Continue => {
                let fixups = self.loops.last().expect("checked: inside a loop");
                let (live, start, forward) = (fixups.live, fixups.start, fixups.continues.is_some());
                self.drop_live_from(live);
                if forward {
                    let at = self.placeholder();
                    let fixups = self.loops.last_mut().expect("checked: inside a loop");
                    fixups.continues.as_mut().expect("a `for` loop collects its continues").push(at);
                } else {
                    self.jump_back(start);
                }
            }
            Stmt::Match { scrutinee, arms } => self.match_stmt(id, scrutinee, arms),
        }
    }

    fn assign(&mut self, id: StmtId, target: ExprId, value: ExprId) {
        let ty = self.ty(target);
        let place = self.place(target);
        if !ty.is_aggregate() {
            self.value(value);
            match place {
                Some(place) if place.by_ref => {
                    self.emit(ldr(X9, FP, frame(place.slot)));
                    self.emit(str(X0, X9, place.offset));
                }
                Some(place) => self.emit(str(X0, FP, frame(place.slot + place.offset))),
                None => {
                    // The target is a list element: its address is only
                    // known once the index has been evaluated.
                    self.emit(PUSH_X0);
                    self.address(target);
                    self.emit(0xAA00_03E9); // mov x9, x0
                    self.emit(pop(X0));
                    self.emit(str(X0, X9, 0));
                }
            }
            return;
        }

        let size = self.prog.size_of(ty);
        let owning = self.prog.needs_drop(ty);
        if let (Some(place), false) = (place, owning) {
            // Plain data into a fixed place: copy straight across.
            self.address(value);
            if place.by_ref {
                self.emit(ldr(X9, FP, frame(place.slot)));
                self.copy_to_pointer(place.offset, size);
            } else {
                self.copy_to_frame(place.slot + place.offset, size);
            }
            return;
        }

        // First get the new value into a frame area this statement owns:
        // the temporary that computed it, or a fresh copy of a variable.
        self.address(value);
        let source = if self.prog.expr_moved[value as usize] {
            self.prog.expr_tmp[value as usize]
        } else {
            let scratch = self.prog.stmt_slot[id as usize];
            self.copy_to_frame(scratch, size);
            if owning {
                self.clone_frame(scratch, ty);
            }
            scratch
        };
        // Then find the target, free what it holds and move the value in.
        self.address(target);
        self.emit(str(X0, FP, frame(self.scratch_slot)));
        if owning {
            self.drop_pointer(self.scratch_slot, ty);
        }
        self.frame_address(X0, source);
        self.emit(ldr(X9, FP, frame(self.scratch_slot)));
        self.copy_to_pointer(0, size);
    }

    fn return_stmt(&mut self, id: StmtId, value: Option<ExprId>) {
        // A local that is returned whole hands its buffers to the caller
        // and so must not be freed here.
        let mut moved_local = None;
        let scalar = match value {
            Some(value) if self.is_aggregate(value) => {
                let ty = self.ty(value);
                let slot = self.sret_slot.expect("checked: this function returns an aggregate");
                self.address(value);
                self.emit(ldr(X9, FP, frame(slot)));
                self.copy_to_pointer(0, self.prog.size_of(ty));
                if self.prog.needs_drop(ty) {
                    if !self.prog.expr_moved[value as usize] {
                        self.clone_pointer(slot, ty);
                    } else if let Expr::Ident(_) = self.ast.exprs[value as usize] {
                        moved_local = self.place(value).map(|place| place.slot);
                    }
                }
                false
            }
            Some(value) => {
                self.value(value);
                true
            }
            None => {
                self.mov_imm(X0, 0);
                false
            }
        };
        let frees_something = self.prog.stmt_drops[id as usize].1 > 0 || !self.live.is_empty();
        if scalar && frees_something {
            self.emit(PUSH_X0);
        }
        self.drop_temps(id, false);
        for i in (0..self.live.len()).rev() {
            let (slot, ty) = self.live[i];
            if Some(slot) != moved_local {
                self.drop_frame(slot, ty);
            }
        }
        if scalar && frees_something {
            self.emit(pop(X0));
        }
        self.epilogue();
    }

    fn for_stmt(&mut self, id: StmtId, has_index: bool, iter: ExprId, body: List) {
        let list_ty = self.ty(iter);
        let Ty::List(list) = list_ty else {
            unreachable!("checked: `for` goes through a list");
        };
        let elem_size = self.prog.size_of(self.prog.lists[list as usize]);
        let base = self.prog.stmt_slot[id as usize];
        let (position, own_list, item_slot) = (base, base + 1, base + 4);
        let index_slot = item_slot + elem_size;
        let in_place = self.prog.stmt_flag[id as usize] == IN_PLACE;

        if !in_place {
            // The loop owns the list: a temporary it takes over, or its
            // own copy of a list that is part of a temporary.
            self.address(iter);
            self.copy_to_frame(own_list, 3);
            if self.prog.stmt_flag[id as usize] == COPIED {
                self.clone_frame(own_list, list_ty);
            }
        }
        self.drop_temps(id, false);
        if !in_place {
            self.live.push((own_list, list_ty));
        }
        self.emit(str(XZR, FP, frame(position)));

        let start = self.words.len();
        if in_place {
            self.address(iter);
        } else {
            self.frame_address(X0, own_list);
        }
        self.emit(ldr(X9, X0, 1)); // length
        self.emit(ldr(X1, FP, frame(position)));
        self.emit(0xEB09_003F); // cmp x1, x9
        let exit = self.placeholder();
        if has_index {
            self.emit(str(X1, FP, frame(index_slot)));
        }
        self.emit(ldr(X9, X0, 0)); // buffer
        self.element_address(X0, X9, X1, elem_size);
        self.copy_to_frame(item_slot, elem_size);

        self.loops.push(LoopFixups { start, breaks: Vec::new(), continues: Some(Vec::new()), live: self.live.len() });
        self.block(body);
        let fixups = self.loops.pop().expect("loop stack is balanced");
        for at in fixups.continues.expect("a `for` loop collects its continues") {
            self.patch_jump(at);
        }
        self.emit(ldr(X0, FP, frame(position)));
        self.emit(0x9100_0400); // add x0, x0, #1
        self.emit(str(X0, FP, frame(position)));
        self.jump_back(start);
        self.patch_branch_if(exit, HS);
        for at in fixups.breaks {
            self.patch_jump(at);
        }
        if !in_place {
            self.live.pop();
            self.drop_frame(own_list, list_ty);
        }
    }

    fn match_stmt(&mut self, id: StmtId, scrutinee: ExprId, arms: List) {
        let ty = self.ty(scrutinee);
        let base = self.prog.stmt_slot[id as usize];
        let aggregate = ty.is_aggregate();
        // Slot `base` keeps the value being matched: a number or `bool`
        // itself, the address of anything else.
        let mut owned = false;
        if !aggregate {
            self.value(scrutinee);
        } else {
            self.address(scrutinee);
            if self.prog.stmt_flag[id as usize] != IN_PLACE {
                // The statement owns the value: a temporary it takes
                // over, or its own copy of a part of a temporary.
                let own = base + 1;
                self.copy_to_frame(own, self.prog.size_of(ty));
                if self.prog.needs_drop(ty) {
                    if self.prog.stmt_flag[id as usize] == COPIED {
                        self.clone_frame(own, ty);
                    }
                    owned = true;
                }
                self.frame_address(X0, own);
            }
        }
        self.emit(str(X0, FP, frame(base)));
        self.drop_temps(id, false);
        if owned {
            self.live.push((base + 1, ty));
        }

        let arm_ids = self.ast.list(arms);
        let mut ends = Vec::new();
        for (i, &arm_id) in arm_ids.iter().enumerate() {
            let arm = self.ast.arms[arm_id as usize];
            let info = self.prog.pats[arm.pattern as usize];
            // The checker proved that the arms cover every case, so when
            // the last arm is reached it matches: no test needed.
            let test = i + 1 < arm_ids.len();
            // A failed test jumps to the next arm.
            let mut next: Option<(usize, Option<bool>)> = None;
            match info.kind {
                PAT_VARIANT => {
                    if test {
                        self.emit(ldr(X9, FP, frame(base)));
                        self.emit(ldr(X9, X9, 0)); // tag
                        self.emit(cmp_x9_imm(info.res + 1));
                        next = Some((self.placeholder(), None));
                    }
                    if let Pattern::Variant { args, .. } = self.ast.patterns[arm.pattern as usize] {
                        for &sub in self.ast.list(args) {
                            let field = self.prog.pats[sub as usize];
                            if field.kind == PAT_BIND {
                                // The name stands for the field where it is.
                                self.emit(ldr(X9, FP, frame(base)));
                                self.emit(add_slots(X9, X9, field.res));
                                self.emit(str(X9, FP, frame(field.slot)));
                            }
                        }
                    }
                }
                PAT_LITERAL if test => match ty {
                    Ty::I64 => {
                        self.emit(ldr(X1, FP, frame(base)));
                        self.mov_imm(X0, self.prog.ints[info.res as usize]);
                        self.emit(0xEB00_003F); // cmp x1, x0
                        next = Some((self.placeholder(), None));
                    }
                    Ty::Bool => {
                        self.emit(ldr(X0, FP, frame(base)));
                        // `true` is skipped when the value is zero, `false` when it is not.
                        next = Some((self.placeholder(), Some(info.res == 0)));
                    }
                    _ => {
                        self.string_value(info.res, info.slot);
                        self.emit(ldr(X0, FP, frame(base)));
                        self.frame_address(X1, info.slot);
                        self.runtime(runtime::STR_EQ);
                        next = Some((self.placeholder(), Some(false)));
                    }
                },
                PAT_BIND => {
                    // The whole value under a name: a number or `bool` is
                    // copied, anything else is referred to where it is.
                    self.emit(ldr(X9, FP, frame(base)));
                    self.emit(str(X9, FP, frame(info.slot)));
                }
                _ => {}
            }
            self.block(arm.body);
            if test {
                ends.push(self.placeholder());
            }
            match next {
                Some((at, None)) => self.patch_branch_if(at, NE),
                Some((at, Some(nonzero))) => self.patch_cbz(at, nonzero),
                None => {}
            }
        }
        for at in ends {
            self.patch_jump(at);
        }
        if owned {
            self.live.pop();
            self.drop_frame(base + 1, ty);
        }
    }

    // ---- memory helpers ------------------------------------------------

    /// Evaluates `value` into the frame at `slot`. An owning value that
    /// comes from a variable is copied; a temporary's buffers move in.
    fn store(&mut self, value: ExprId, slot: u32) {
        let ty = self.ty(value);
        if ty.is_aggregate() {
            self.address(value);
            self.copy_to_frame(slot, self.prog.size_of(ty));
            if self.prog.needs_drop(ty) && !self.prog.expr_moved[value as usize] {
                self.clone_frame(slot, ty);
            }
        } else {
            self.value(value);
            self.emit(str(X0, FP, frame(slot)));
        }
    }

    /// Writes string literal number `index` into three frame slots.
    fn string_value(&mut self, index: u32, slot: u32) {
        // A string value is a pointer, a length and a capacity.
        let (offset, len) = self.prog.strings[index as usize];
        let at = self.words.len() as u32;
        self.emit(0);
        self.emit(0);
        self.relocs.push(Reloc::Data { at, offset: RUNTIME_DATA.len() as u32 + offset, reg: X0 });
        self.emit(str(X0, FP, frame(slot)));
        self.mov_imm(X0, len as i64);
        self.emit(str(X0, FP, frame(slot + 1)));
        self.emit(str(XZR, FP, frame(slot + 2))); // capacity 0: not owned
    }

    /// Builds the enum value of expression `id` in its temporary and
    /// leaves the temporary's address in x0.
    fn enum_value(&mut self, id: ExprId, args: &[u32]) {
        let Ty::Enum(index) = self.ty(id) else {
            unreachable!("checked: a variant makes an enum value");
        };
        let variant = self.prog.expr_aux[id as usize];
        let tmp = self.prog.expr_tmp[id as usize];
        let prog = self.prog;
        for (i, &arg) in args.iter().enumerate() {
            let offset = prog.enums[index as usize].variants[variant as usize].fields[i].1;
            self.store(arg, tmp + offset);
        }
        // The tag goes in last: until then the temporary counts as empty.
        self.mov_imm(X0, variant as i64 + 1);
        self.emit(str(X0, FP, frame(tmp)));
        self.frame_address(X0, tmp);
    }

    /// Copies `size` slots from the address in x0 into the frame.
    fn copy_to_frame(&mut self, slot: u32, size: u32) {
        for i in 0..size {
            self.emit(ldr(X10, X0, i));
            self.emit(str(X10, FP, frame(slot + i)));
        }
    }

    /// Copies `size` slots from the address in x0 to `offset` slots past
    /// the address in x9.
    fn copy_to_pointer(&mut self, offset: u32, size: u32) {
        for i in 0..size {
            self.emit(ldr(X10, X0, i));
            self.emit(str(X10, X9, offset + i));
        }
    }

    /// `rd = buffer + index * element size`. Uses x10 when the element is
    /// larger than one slot; `rd` may be the same register as an input.
    fn element_address(&mut self, rd: u32, buffer: u32, index: u32, elem_size: u32) {
        if elem_size == 1 {
            self.emit(add_scaled(rd, buffer, index));
        } else {
            debug_assert!(buffer != X11 && index != X11);
            self.mov_imm(X11, elem_size as i64 * 8);
            self.emit(madd(rd, index, X11, buffer));
        }
    }

    /// The location of a variable or of a field path inside a variable,
    /// when it is fixed at compile time. `None` for computed values and
    /// for anything reached through a list index.
    fn place(&self, id: ExprId) -> Option<Place> {
        match self.ast.exprs[id as usize] {
            Expr::Ident(_) => {
                let res = self.prog.expr_res[id as usize];
                if res == VARIANT {
                    // `None` and the like are values, not variables.
                    return None;
                }
                Some(Place { slot: res & !BY_REF, by_ref: res & BY_REF != 0, offset: 0 })
            }
            Expr::Field { base, .. } => {
                let mut place = self.place(base)?;
                place.offset += self.prog.expr_res[id as usize];
                Some(place)
            }
            _ => None,
        }
    }

    /// Leaves the address of a place in x0.
    fn place_address(&mut self, place: Place) {
        if place.by_ref {
            self.emit(ldr(X0, FP, frame(place.slot)));
            if place.offset > 0 {
                self.emit(add_slots(X0, X0, place.offset));
            }
        } else {
            self.frame_address(X0, place.slot + place.offset);
        }
    }

    // ---- expressions ---------------------------------------------------

    /// Evaluates an expression for its effects, whatever its type.
    fn eval(&mut self, id: ExprId) {
        if self.is_aggregate(id) {
            self.address(id);
        } else {
            self.value(id);
        }
    }

    /// Generates code that leaves in x0 the address of an aggregate value,
    /// or of any expression that names a place. A computed aggregate is
    /// first stored in its own temporary.
    fn address(&mut self, id: ExprId) {
        if let Some(place) = self.place(id) {
            self.place_address(place);
            return;
        }
        match self.ast.exprs[id as usize] {
            Expr::Str(_) => {
                let tmp = self.prog.expr_tmp[id as usize];
                self.string_value(self.prog.expr_res[id as usize], tmp);
                self.frame_address(X0, tmp);
            }
            Expr::Ident(_) => self.enum_value(id, &[]),
            Expr::Binary { lhs, rhs, .. } => {
                // `a + b` on strings: a new string in this expression's temporary.
                let tmp = self.prog.expr_tmp[id as usize];
                self.address(lhs);
                self.emit(PUSH_X0);
                self.address(rhs);
                self.emit(0xAA00_03E1); // mov x1, x0
                self.emit(pop(X0));
                self.frame_address(X8, tmp);
                self.runtime(runtime::STR_CONCAT);
                self.frame_address(X0, tmp);
            }
            Expr::StructLit { fields, .. } => {
                let tmp = self.prog.expr_tmp[id as usize];
                for &init_id in self.ast.list(fields) {
                    let init = self.ast.field_inits[init_id as usize];
                    self.store(init.value, tmp + self.prog.init_offset[init_id as usize]);
                }
                self.frame_address(X0, tmp);
            }
            Expr::ArrayLit { items } => self.list_literal(id, items),
            Expr::Call { args, .. } if self.prog.expr_res[id as usize] == BUILTIN_VARIANT => {
                self.enum_value(id, self.ast.list(args));
            }
            Expr::Call { args, .. } => {
                self.call(id, args);
                self.frame_address(X0, self.prog.expr_tmp[id as usize]);
            }
            Expr::Field { base, .. } => {
                self.address(base);
                let offset = self.prog.expr_res[id as usize];
                if offset > 0 {
                    self.emit(add_slots(X0, X0, offset));
                }
            }
            Expr::Index { base, index } => {
                // The index is evaluated first, so that nothing runs
                // between reading the buffer pointer and using it.
                let Ty::List(list) = self.ty(base) else {
                    unreachable!("checked: only lists are indexed");
                };
                let elem_size = self.prog.size_of(self.prog.lists[list as usize]);
                self.value(index);
                self.emit(PUSH_X0);
                self.address(base);
                self.emit(pop(X1));
                self.emit(ldr(X9, X0, 1)); // length
                self.emit(0xEB09_003F); // cmp x1, x9
                self.emit(0x5400_0043); // b.lo +2: an unsigned compare also rejects negatives
                self.runtime(runtime::INDEX_FAIL);
                self.emit(ldr(X9, X0, 0)); // buffer
                self.element_address(X0, X9, X1, elem_size);
            }
            _ => unreachable!("checked: only these expressions have an address"),
        }
    }

    fn list_literal(&mut self, id: ExprId, items: List) {
        let tmp = self.prog.expr_tmp[id as usize];
        let item_ids = self.ast.list(items);
        let count = item_ids.len() as u32;
        if count == 0 {
            for i in 0..3 {
                self.emit(str(XZR, FP, frame(tmp + i)));
            }
            self.frame_address(X0, tmp);
            return;
        }
        let elem_size = self.prog.size_of(self.ty(item_ids[0]));
        self.mov_imm(X0, (count * elem_size * 8) as i64);
        self.runtime(runtime::ALLOC);
        self.emit(str(X0, FP, frame(tmp)));
        for (i, &item) in item_ids.iter().enumerate() {
            let mut offset = i as u32 * elem_size;
            self.eval(item);
            self.emit(ldr(X9, FP, frame(tmp)));
            if offset + elem_size > MAX_OFFSET {
                self.mov_imm(X11, offset as i64 * 8);
                self.emit(add_reg(X9, X9, X11));
                offset = 0;
            }
            if self.is_aggregate(item) {
                self.copy_to_pointer(offset, elem_size);
                let elem = self.ty(item);
                if self.prog.needs_drop(elem) && !self.prog.expr_moved[item as usize] {
                    // The element was copied from a variable: give the
                    // copy in the list buffers of its own.
                    self.emit(ldr(X0, FP, frame(tmp)));
                    self.mov_imm(X11, i as i64 * elem_size as i64 * 8);
                    self.emit(add_reg(X0, X0, X11));
                    self.clone_value(elem);
                }
            } else {
                self.emit(str(X0, X9, offset));
            }
        }
        self.mov_imm(X0, count as i64);
        self.emit(str(X0, FP, frame(tmp + 1))); // length
        self.emit(str(X0, FP, frame(tmp + 2))); // capacity
        self.frame_address(X0, tmp);
    }

    /// Generates code that leaves a scalar value in x0.
    fn value(&mut self, id: ExprId) {
        match self.ast.exprs[id as usize] {
            Expr::Int(_) => self.mov_imm(X0, self.prog.ints[self.prog.expr_res[id as usize] as usize]),
            Expr::Bool(value) => self.mov_imm(X0, value as i64),
            Expr::Ident(_) | Expr::Field { .. } | Expr::Index { .. } => match self.place(id) {
                Some(place) if place.by_ref => {
                    self.emit(ldr(X9, FP, frame(place.slot)));
                    self.emit(ldr(X0, X9, place.offset));
                }
                Some(place) => self.emit(ldr(X0, FP, frame(place.slot + place.offset))),
                None => {
                    // A list element, or a field of a computed value.
                    self.address(id);
                    self.emit(ldr(X0, X0, 0));
                }
            },
            Expr::Unary { op, operand } => {
                self.value(operand);
                self.emit(match op {
                    UnOp::Neg => 0xCB00_03E0, // neg x0, x0
                    UnOp::Not => 0xD240_0000, // eor x0, x0, #1
                });
            }
            Expr::Binary { op: op @ (BinOp::And | BinOp::Or), lhs, rhs } => {
                // Short circuit: the left value is the result when it decides.
                self.value(lhs);
                let skip = self.placeholder();
                self.value(rhs);
                self.patch_cbz(skip, op == BinOp::Or);
            }
            Expr::Binary { op, lhs, rhs } if self.ty(lhs) == Ty::Str => {
                // `==` and `!=` on strings compare the bytes.
                self.address(lhs);
                self.emit(PUSH_X0);
                self.address(rhs);
                self.emit(0xAA00_03E1); // mov x1, x0
                self.emit(pop(X0));
                self.runtime(runtime::STR_EQ);
                if op == BinOp::NotEq {
                    self.emit(0xD240_0000); // eor x0, x0, #1
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                self.value(lhs);
                self.emit(PUSH_X0);
                self.value(rhs);
                self.emit(pop(X1));
                // Here x1 holds the left operand and x0 the right one.
                match op {
                    BinOp::Add => self.emit(0x8B00_0020), // add x0, x1, x0
                    BinOp::Sub => self.emit(0xCB00_0020), // sub x0, x1, x0
                    BinOp::Mul => self.emit(0x9B00_7C20), // mul x0, x1, x0
                    BinOp::Div => {
                        self.trap_if_zero();
                        self.emit(0x9AC0_0C20); // sdiv x0, x1, x0
                    }
                    BinOp::Rem => {
                        self.trap_if_zero();
                        self.emit(0x9AC0_0C20 | X2); // sdiv x2, x1, x0
                        self.emit(0x9B00_8440); // msub x0, x2, x0, x1
                    }
                    BinOp::Eq => self.compare(EQ),
                    BinOp::NotEq => self.compare(NE),
                    BinOp::Lt => self.compare(LT),
                    BinOp::LtEq => self.compare(LE),
                    BinOp::Gt => self.compare(GT),
                    BinOp::GtEq => self.compare(GE),
                    BinOp::And | BinOp::Or => unreachable!("handled above"),
                }
            }
            Expr::Call { args, .. } => self.call(id, args),
            Expr::MutArg(_) | Expr::Float(_) | Expr::Str(_) | Expr::StructLit { .. } | Expr::ArrayLit { .. } => {
                unreachable!("rejected by the checker or not a scalar")
            }
        }
    }

    /// Calls a function or a built-in. A scalar result is left in x0; an
    /// aggregate result is written to the call's temporary.
    fn call(&mut self, id: ExprId, args: List) {
        let target = self.prog.expr_res[id as usize];
        let arg_ids = self.ast.list(args);
        match target {
            BUILTIN_LEN | BUILTIN_LIST_PUSH | BUILTIN_LIST_POP => {
                let Expr::Call { callee, .. } = self.ast.exprs[id as usize] else {
                    unreachable!("a call is a call");
                };
                let Expr::Field { base, .. } = self.ast.exprs[callee as usize] else {
                    unreachable!("checked: a method call names its receiver");
                };
                return self.list_method(id, target, base, arg_ids);
            }
            _ => {}
        }
        for (i, &arg) in arg_ids.iter().enumerate() {
            let by_ref = if target >= FIRST_BUILTIN {
                target == BUILTIN_PRINT_STR
            } else {
                self.prog.fns[target as usize].params[i].by_ref
            };
            match self.ast.exprs[arg as usize] {
                Expr::MutArg(inner) => self.address(inner),
                _ if by_ref => self.address(arg),
                _ => self.value(arg),
            }
            self.emit(PUSH_X0);
        }
        for reg in (0..arg_ids.len() as u32).rev() {
            self.emit(pop(reg));
        }
        match target {
            BUILTIN_PRINT_I64 => self.runtime(runtime::PRINT_I64),
            BUILTIN_PRINT_BOOL => self.runtime(runtime::PRINT_BOOL),
            BUILTIN_PRINT_STR => self.runtime(runtime::PRINT_STR),
            BUILTIN_STR_FROM_I64 | BUILTIN_STR_FROM_BOOL => {
                self.frame_address(X8, self.prog.expr_tmp[id as usize]);
                self.runtime(if target == BUILTIN_STR_FROM_I64 { runtime::STR_FROM_I64 } else { runtime::STR_FROM_BOOL });
            }
            _ => {
                if self.is_aggregate(id) {
                    // The callee writes its result where x8 points.
                    self.frame_address(X8, self.prog.expr_tmp[id as usize]);
                }
                let at = self.placeholder();
                self.calls.push((at, target));
            }
        }
    }

    fn list_method(&mut self, id: ExprId, method: u32, base: ExprId, args: &[u32]) {
        if method == BUILTIN_LEN {
            // Lists and strings keep their length in the same place.
            self.address(base);
            self.emit(ldr(X0, X0, 1));
            return;
        }
        let Ty::List(list) = self.ty(base) else {
            unreachable!("checked: the receiver is a list");
        };
        let elem = self.prog.lists[list as usize];
        let elem_size = self.prog.size_of(elem);
        match method {
            BUILTIN_LIST_PUSH => {
                // The element is copied aside first: it may live in this
                // very list, whose buffer can move while it grows.
                let tmp = self.prog.expr_tmp[id as usize];
                if elem.is_aggregate() {
                    self.address(args[0]);
                    self.copy_to_frame(tmp, elem_size);
                    if self.prog.needs_drop(elem) && !self.prog.expr_moved[args[0] as usize] {
                        self.clone_frame(tmp, elem);
                    }
                } else {
                    self.value(args[0]);
                    self.emit(str(X0, FP, frame(tmp)));
                }
                self.address(base);
                self.frame_address(X1, tmp);
                self.mov_imm(X2, elem_size as i64 * 8);
                self.runtime(runtime::LIST_PUSH);
            }
            _ => {
                self.address(base);
                self.emit(ldr(X9, X0, 1)); // length
                self.emit(0xB500_0049); // cbnz x9, +2
                self.runtime(runtime::POP_EMPTY);
                self.emit(0xD100_0529); // sub x9, x9, #1
                self.emit(str(X9, X0, 1));
                self.emit(ldr(X10, X0, 0)); // buffer
                self.element_address(X0, X10, X9, elem_size);
                if elem.is_aggregate() {
                    self.copy_to_frame(self.prog.expr_tmp[id as usize], elem_size);
                } else {
                    self.emit(ldr(X0, X0, 0));
                }
            }
        }
    }

    fn compare(&mut self, cond: u32) {
        self.emit(0xEB00_003F); // cmp x1, x0
        self.emit(cset_x0(cond));
    }

    /// Division by zero stops the program with a message instead of
    /// producing garbage.
    fn trap_if_zero(&mut self) {
        self.emit(0xB500_0040); // cbnz x0, +2
        self.runtime(runtime::DIV_ZERO);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodings_match_the_reference_assembler() {
        // Expected values were produced by assembling the same
        // instructions with clang's integrated assembler.
        assert_eq!(pop(X1), 0xF84107E1);
        assert_eq!(pop(3), 0xF84107E3);
        assert_eq!(ldr(X0, FP, frame(1)), 0xF9400FA0);
        assert_eq!(str(X0, FP, frame(1)), 0xF9000FA0);
        assert_eq!(str(5, FP, frame(5)), 0xF9001FA5);
        assert_eq!(str(X8, FP, frame(2)), 0xF90013A8);
        assert_eq!(ldr(X9, FP, 3), 0xF9400FA9);
        assert_eq!(ldr(X0, X9, 2), 0xF9400920);
        assert_eq!(ldr(X0, X0, 4), 0xF9401000);
        assert_eq!(str(X0, X9, 2), 0xF9000920);
        assert_eq!(ldr(X10, X0, 1), 0xF940040A);
        assert_eq!(str(X10, X9, 1), 0xF900052A);
        assert_eq!(str(X10, FP, 6), 0xF9001BAA);
        assert_eq!(add_slots(X0, FP, 5), 0x9100A3A0);
        assert_eq!(add_slots(X8, FP, 501), 0x913EA3A8);
        assert_eq!(add_slots(X0, X0, 3), 0x91006000);
        assert_eq!(add_slots(X9, X9, 1), 0x91002129);
        assert_eq!(add_slots(X1, FP, 7), 0x9100E3A1);
        assert_eq!(add_scaled(X0, X9, X1), 0x8B010D20);
        assert_eq!(add_scaled(X0, X10, X9), 0x8B090D40);
        assert_eq!(madd(X0, X1, X10, X9), 0x9B0A2420);
        assert_eq!(madd(X0, X9, X11, X10), 0x9B0B2920);
        assert_eq!(add_reg(X9, X9, X10), 0x8B0A0129);
        assert_eq!(str(XZR, FP, 6), 0xF9001BBF);
        assert_eq!(add_slots(X0, X19, 5), 0x9100A260);
        assert_eq!(ldr(X9, X19, 2), 0xF9400A69);
        assert_eq!(ldr(X20, X19, 0), 0xF9400274);
        assert_eq!(ldr(X21, X19, 1), 0xF9400675);
        assert_eq!(ldr(X0, X19, 0), 0xF9400260);
        assert_eq!(0x9100_0294u32 | 24 << 10, 0x91006294);
        assert_eq!(add_reg(X20, X20, X10), 0x8B0A0294);
        assert_eq!(cmp_x9_imm(1), 0xF100053F);
        assert_eq!(cmp_x9_imm(1000), 0xF10FA13F);
        assert_eq!(cset_x0(EQ), 0x9A9F17E0);
        assert_eq!(cset_x0(NE), 0x9A9F07E0);
        assert_eq!(cset_x0(LT), 0x9A9FA7E0);
        assert_eq!(cset_x0(LE), 0x9A9FC7E0);
        assert_eq!(cset_x0(GT), 0x9A9FD7E0);
        assert_eq!(cset_x0(GE), 0x9A9FB7E0);
        assert_eq!(0xD100_03FFu32 | 96 << 10, 0xD10183FF);
        assert_eq!(0x9100_03FFu32 | 96 << 10, 0x910183FF);
    }
}
