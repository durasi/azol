// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Turns a checked program into x86-64 machine code for Linux, in one
//! pass over the syntax tree. It follows `arm64.rs` statement for
//! statement: a scalar expression leaves its value in rax, an aggregate
//! expression leaves its address in rax, earlier values wait on the
//! stack, and every variable lives in the function's frame.
//!
//! Registers: rax carries values, rcx and rdx second operands, r8-r10
//! are scratch within one step. Arguments travel in rdi, rsi, rdx, rcx,
//! r8, r9, r10 and r11; the address for an aggregate result in rax.
//! Nothing is kept in a register across a call, and rbx and r12-r15 are
//! never touched. Values parked on the stack take 16 bytes each, so the
//! stack pointer is a multiple of 16 at every call.

use crate::ast::*;
use crate::runtime_x64 as runtime;
use crate::sema::{
    idle_slot, Program, Shift, Ty, APPEND, BUILTIN_ARGS, BUILTIN_CHR, BUILTIN_EPRINT, BUILTIN_EXIT, BUILTIN_LEN,
    BUILTIN_LIST_POP, BUILTIN_LIST_PUSH, BUILTIN_PRINT_BOOL, BUILTIN_PRINT_I64, BUILTIN_PRINT_STR, BUILTIN_READ_FILE,
    BUILTIN_SET_EXECUTABLE, BUILTIN_STR_FROM_BOOL, BUILTIN_STR_FROM_I64, BUILTIN_STR_SLICE, BUILTIN_VARIANT,
    BUILTIN_WRITE_FILE, BY_REF,
    COPIED, FIRST_BUILTIN, IN_PLACE, PAT_BIND, PAT_LITERAL, PAT_VARIANT, VARIANT,
};
use std::collections::HashMap;

/// Machine code for one program, before it is placed in an executable.
pub struct Code {
    /// The program's functions. Calls between them are already resolved.
    pub bytes: Vec<u8>,
    /// Byte offset of `main` in `bytes`. It is called like a C function
    /// with no arguments and returns the exit code in rax.
    pub entry: u32,
    /// The program's string literals (`Program::data`), read-only.
    pub data: Vec<u8>,
    /// `call rel32` instructions into the runtime: the offset in `bytes`
    /// of the 4-byte field, and the routine (a `runtime_x64` entry id).
    pub runtime_calls: Vec<(u32, u32)>,
    /// RIP-relative references to `data`: the offset in `bytes` of the
    /// 4-byte field, and the offset in `data` it must point at.
    pub data_refs: Vec<(u32, u32)>,
}

const RAX: u8 = 0;
const RCX: u8 = 1;
const RDX: u8 = 2;
const RSP: u8 = 4;
const RBP: u8 = 5;
const RSI: u8 = 6;
const RDI: u8 = 7;
const R8: u8 = 8;
const R9: u8 = 9;
const R10: u8 = 10;
const R11: u8 = 11;

/// The registers arguments travel in, first to last. The first four are
/// also where the C calling convention, which the runtime uses, puts them.
const ARGS: [u8; 8] = [RDI, RSI, RDX, RCX, R8, R9, R10, R11];

// Condition codes, as used in `jcc` and `setcc`.
const CC_B: u8 = 0x2;
const CC_AE: u8 = 0x3;
const CC_E: u8 = 0x4;
const CC_NE: u8 = 0x5;
const CC_L: u8 = 0xC;
const CC_GE: u8 = 0xD;
const CC_LE: u8 = 0xE;
const CC_G: u8 = 0xF;

// Two-operand instructions in their `op r/m64, r64` form.
const ADD: u8 = 0x01;
const SUB: u8 = 0x29;
const CMP: u8 = 0x39;
const TEST: u8 = 0x85;
const AND: u8 = 0x21;
const OR: u8 = 0x09;
const XOR: u8 = 0x31;

// The same with a constant: the `/n` of `81 /n id`.
const ADD_I: u8 = 0;
const SUB_I: u8 = 5;
const XOR_I: u8 = 6;
const CMP_I: u8 = 7;

/// The condition code of a comparison operator.
fn comparison(op: BinOp) -> Option<u8> {
    Some(match op {
        BinOp::Eq => CC_E,
        BinOp::NotEq => CC_NE,
        BinOp::Lt => CC_L,
        BinOp::LtEq => CC_LE,
        BinOp::Gt => CC_G,
        BinOp::GtEq => CC_GE,
        _ => return None,
    })
}

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
    code: Vec<u8>,
    fn_starts: Vec<u32>,
    /// (offset of the 4-byte field, function index) of every call to patch.
    calls: Vec<(usize, u32)>,
    runtime_calls: Vec<(u32, u32)>,
    data_refs: Vec<(u32, u32)>,
    /// Helper functions the compiler writes itself: one to free and one to
    /// copy each type whose values own other values. `true` means copy.
    glue: Vec<(Ty, bool)>,
    glue_ids: HashMap<(Ty, bool), usize>,
    /// (offset of the 4-byte field, index into `glue`) of every call to a helper.
    glue_calls: Vec<(usize, usize)>,
    loops: Vec<LoopFixups>,
    /// Local variables that own buffers and are in scope: slot and type.
    live: Vec<(u32, Ty)>,
    frame_bytes: u32,
    ret: Ty,
    sret_slot: Option<u32>,
    scratch_slot: u32,
    /// Where the function being generated keeps its entries in the
    /// checker's tables.
    sh: Shift,
}

pub fn generate(ast: &Ast, prog: &Program) -> Code {
    let mut g = Gen {
        ast,
        prog,
        code: Vec::with_capacity(ast.exprs.len() * 12 + 1024),
        fn_starts: Vec::with_capacity(prog.fns.len()),
        calls: Vec::new(),
        runtime_calls: Vec::new(),
        data_refs: Vec::new(),
        glue: Vec::new(),
        glue_ids: HashMap::new(),
        glue_calls: Vec::new(),
        loops: Vec::new(),
        live: Vec::new(),
        frame_bytes: 0,
        ret: Ty::Unit,
        sret_slot: None,
        scratch_slot: 0,
        sh: Shift::default(),
    };
    for index in 0..prog.fns.len() as u32 {
        g.function(index);
    }
    // Then the helpers that free and copy nested values. Writing one can
    // ask for another, so the list grows while it is walked.
    let mut glue_starts = Vec::new();
    let mut next = 0;
    while next < g.glue.len() {
        glue_starts.push(g.code.len() as u32);
        let (ty, copy) = g.glue[next];
        g.glue_function(ty, copy);
        next += 1;
    }
    let calls = std::mem::take(&mut g.calls);
    for (at, target) in calls {
        g.patch_to(at, g.fn_starts[target as usize] as usize);
    }
    let glue_calls = std::mem::take(&mut g.glue_calls);
    for (at, id) in glue_calls {
        g.patch_to(at, glue_starts[id] as usize);
    }
    Code {
        entry: g.fn_starts[prog.main as usize],
        bytes: g.code,
        data: prog.data.clone(),
        runtime_calls: g.runtime_calls,
        data_refs: g.data_refs,
    }
}

impl<'a> Gen<'a> {
    // ---- instruction encoding ------------------------------------------

    #[inline(always)]
    fn byte(&mut self, value: u8) {
        self.code.push(value);
    }

    fn imm32(&mut self, value: i32) {
        self.code.extend_from_slice(&value.to_le_bytes());
    }

    /// The prefix that selects 64-bit operands and the high registers.
    fn rex(&mut self, reg: u8, rm: u8) {
        self.byte(0x48 | (reg >> 3) << 2 | (rm >> 3));
    }

    /// The operand bytes for `reg` and the memory at `base + disp`.
    fn mem(&mut self, reg: u8, base: u8, disp: i32) {
        let rm = base & 7;
        let mode = if disp == 0 && rm != 5 {
            0
        } else if (-128..=127).contains(&disp) {
            1
        } else {
            2
        };
        self.byte(mode << 6 | (reg & 7) << 3 | rm);
        if rm == 4 {
            self.byte(0x24); // rsp and r12 need the longer form
        }
        match mode {
            1 => self.byte(disp as i8 as u8),
            2 => self.imm32(disp),
            _ => {}
        }
    }

    /// `mov dst, [base + disp]`
    fn load_mem(&mut self, dst: u8, base: u8, disp: i32) {
        self.rex(dst, base);
        self.byte(0x8B);
        self.mem(dst, base, disp);
    }

    /// `mov [base + disp], src`
    fn store_mem(&mut self, base: u8, disp: i32, src: u8) {
        self.rex(src, base);
        self.byte(0x89);
        self.mem(src, base, disp);
    }

    /// `mov qword [base + disp], 0`
    fn zero_mem(&mut self, base: u8, disp: i32) {
        self.rex(0, base);
        self.byte(0xC7);
        self.mem(0, base, disp);
        self.imm32(0);
    }

    /// `lea dst, [base + disp]`
    fn lea(&mut self, dst: u8, base: u8, disp: i32) {
        self.rex(dst, base);
        self.byte(0x8D);
        self.mem(dst, base, disp);
    }

    /// `mov dst, src`
    fn mov(&mut self, dst: u8, src: u8) {
        self.rex(src, dst);
        self.byte(0x89);
        self.byte(0xC0 | (src & 7) << 3 | (dst & 7));
    }

    /// `op dst, src` for `ADD`, `SUB`, `CMP`, `TEST`, `AND`, `OR` and `XOR`.
    fn op(&mut self, op: u8, dst: u8, src: u8) {
        self.rex(src, dst);
        self.byte(op);
        self.byte(0xC0 | (src & 7) << 3 | (dst & 7));
    }

    /// `op reg, value` for `ADD_I`, `SUB_I`, `XOR_I` and `CMP_I`.
    fn op_imm(&mut self, op: u8, reg: u8, value: i32) {
        self.rex(0, reg);
        self.byte(0x81);
        self.byte(0xC0 | op << 3 | (reg & 7));
        self.imm32(value);
    }

    /// `op qword [base + disp], value`
    fn op_mem_imm(&mut self, op: u8, base: u8, disp: i32, value: i32) {
        self.rex(0, base);
        self.byte(0x81);
        self.mem(op, base, disp);
        self.imm32(value);
    }

    /// `imul dst, src`
    fn imul(&mut self, dst: u8, src: u8) {
        self.rex(dst, src);
        self.byte(0x0F);
        self.byte(0xAF);
        self.byte(0xC0 | (dst & 7) << 3 | (src & 7));
    }

    /// `imul dst, src, value`
    fn imul_imm(&mut self, dst: u8, src: u8, value: i32) {
        self.rex(dst, src);
        self.byte(0x69);
        self.byte(0xC0 | (dst & 7) << 3 | (src & 7));
        self.imm32(value);
    }

    fn push_reg(&mut self, reg: u8) {
        if reg >= 8 {
            self.byte(0x41);
        }
        self.byte(0x50 | (reg & 7));
    }

    fn pop_reg(&mut self, reg: u8) {
        if reg >= 8 {
            self.byte(0x41);
        }
        self.byte(0x58 | (reg & 7));
    }

    /// Parks rax on the stack. Two pushes keep the stack pointer a
    /// multiple of 16.
    fn push(&mut self) {
        self.push_reg(RAX);
        self.push_reg(RAX);
    }

    /// Takes the value parked last into `reg`.
    fn pop(&mut self, reg: u8) {
        self.pop_reg(reg);
        self.pop_reg(reg);
    }

    fn mov_imm(&mut self, reg: u8, value: i64) {
        if (0..=u32::MAX as i64).contains(&value) {
            // A 32-bit move clears the upper half.
            if reg >= 8 {
                self.byte(0x41);
            }
            self.byte(0xB8 | (reg & 7));
            self.imm32(value as u32 as i32);
        } else if let Ok(small) = i32::try_from(value) {
            self.rex(0, reg);
            self.byte(0xC7);
            self.byte(0xC0 | (reg & 7));
            self.imm32(small);
        } else {
            self.rex(0, reg);
            self.byte(0xB8 | (reg & 7));
            self.code.extend_from_slice(&value.to_le_bytes());
        }
    }

    /// Emits a jump taken on condition `cc` and returns the offset of its
    /// 4-byte field for patching.
    fn jump_if(&mut self, cc: u8) -> usize {
        self.byte(0x0F);
        self.byte(0x80 | cc);
        self.imm32(0);
        self.code.len() - 4
    }

    /// Emits an unconditional jump and returns the offset of its field.
    fn jump(&mut self) -> usize {
        self.byte(0xE9);
        self.imm32(0);
        self.code.len() - 4
    }

    /// Completes the jump or call whose field is at `at` to go to `target`.
    fn patch_to(&mut self, at: usize, target: usize) {
        let delta = (target as i64 - (at as i64 + 4)) as i32;
        self.code[at..at + 4].copy_from_slice(&delta.to_le_bytes());
    }

    /// Completes the jump whose field is at `at` to come here.
    fn patch(&mut self, at: usize) {
        self.patch_to(at, self.code.len());
    }

    fn jump_back(&mut self, to: usize) {
        let at = self.jump();
        self.patch_to(at, to);
    }

    /// Emits a `call` and returns the offset of its field.
    fn call_field(&mut self) -> usize {
        self.byte(0xE8);
        self.imm32(0);
        self.code.len() - 4
    }

    /// Calls a runtime routine.
    fn runtime(&mut self, entry: u32) {
        let at = self.call_field();
        self.runtime_calls.push((at as u32, entry));
    }

    /// Calls a runtime routine that does not return, unless condition
    /// `cc` holds.
    fn runtime_unless(&mut self, cc: u8, entry: u32) {
        self.byte(0x70 | cc); // a short jump over the 5-byte call
        self.byte(5);
        self.runtime(entry);
    }

    // ---- the frame -----------------------------------------------------

    /// Where frame slot `slot` is, relative to rbp. A value's slots lie
    /// at rising addresses, like its fields.
    fn disp(&self, slot: u32) -> i32 {
        8 * slot as i32 - self.frame_bytes as i32
    }

    /// `mov reg, [slot]`
    fn load(&mut self, reg: u8, slot: u32) {
        let disp = self.disp(slot);
        self.load_mem(reg, RBP, disp);
    }

    /// `mov [slot], reg`
    fn store_slot(&mut self, slot: u32, reg: u8) {
        let disp = self.disp(slot);
        self.store_mem(RBP, disp, reg);
    }

    fn zero_slot(&mut self, slot: u32) {
        let disp = self.disp(slot);
        self.zero_mem(RBP, disp);
    }

    /// `reg = address of frame slot`
    fn frame_address(&mut self, reg: u8, slot: u32) {
        let disp = self.disp(slot);
        self.lea(reg, RBP, disp);
    }

    fn ty(&self, id: ExprId) -> Ty {
        self.prog.expr_ty[self.sh.e(id)]
    }

    /// True for values handled by address: structs, enums, strings and lists.
    fn is_aggregate(&self, id: ExprId) -> bool {
        self.ty(id).is_aggregate()
    }

    fn function(&mut self, index: u32) {
        let prog = self.prog;
        let info = &prog.fns[index as usize];
        let Item::Fn { params, body, .. } = self.ast.items[info.item as usize] else {
            unreachable!("a function's item is a function");
        };
        self.sh = info.shift;
        self.fn_starts.push(self.code.len() as u32);
        self.frame_bytes = (8 * info.slots).div_ceil(16) * 16;
        self.ret = info.ret;
        self.sret_slot = info.sret_slot;
        self.scratch_slot = info.scratch_slot;
        self.live.clear();

        self.push_reg(RBP);
        self.mov(RBP, RSP);
        if self.frame_bytes > 0 {
            self.op_imm(SUB_I, RSP, self.frame_bytes as i32);
        }
        // The address for an aggregate result arrives in rax; park it
        // before anything else uses that register.
        if let Some(slot) = self.sret_slot {
            self.store_slot(slot, RAX);
        }
        // Each parameter arrives in a register as a value or a pointer.
        for slot in 0..self.ast.list(params).len() as u32 {
            self.store_slot(slot, ARGS[slot as usize]);
        }
        // A temporary may be freed without having been filled, when the
        // expression that fills it was skipped. Null makes that harmless.
        for &slot in &info.zero_slots {
            self.zero_slot(slot);
        }
        self.block(body);
        if self.ret == Ty::Unit {
            self.mov_imm(RAX, 0);
            self.epilogue();
        } else {
            // The checker proved every path returns; this is unreachable.
            self.byte(0x0F);
            self.byte(0x0B); // ud2
        }
    }

    fn epilogue(&mut self) {
        self.byte(0xC9); // leave: mov rsp, rbp; pop rbp
        self.byte(0xC3); // ret
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

    /// Frees everything owned by the value whose address is in rdi.
    fn drop_value(&mut self, ty: Ty) {
        match ty {
            Ty::Str => self.runtime(runtime::DROP),
            Ty::List(list) if !self.prog.needs_drop(self.prog.lists[list as usize]) => self.runtime(runtime::DROP),
            _ => self.glue_call(ty, false),
        }
    }

    /// The value whose address is in rdi was copied bit by bit from
    /// another owner. Gives it buffers of its own, all the way down.
    fn clone_value(&mut self, ty: Ty) {
        match ty {
            Ty::Str => {
                self.mov_imm(RSI, 1);
                self.runtime(runtime::LIST_CLONE);
            }
            Ty::List(list) if !self.prog.needs_drop(self.prog.lists[list as usize]) => {
                self.mov_imm(RSI, self.prog.size_of(self.prog.lists[list as usize]) as i64 * 8);
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
        let at = self.call_field();
        self.glue_calls.push((at, id));
    }

    /// Writes the helper for one type. It takes the value's address in
    /// rdi and keeps what it needs across calls in a small frame of its
    /// own: the address at [rbp - 8].
    fn glue_function(&mut self, ty: Ty, copy: bool) {
        self.push_reg(RBP);
        self.mov(RBP, RSP);
        self.op_imm(SUB_I, RSP, 32);
        self.store_mem(RBP, -8, RDI);
        match ty {
            Ty::Struct(_) => {
                // Handle each owning part directly inside the struct.
                for (offset, field) in self.prog.owned(ty) {
                    self.load_mem(RDI, RBP, -8);
                    self.lea(RDI, RDI, 8 * offset as i32);
                    if copy {
                        self.clone_value(field);
                    } else {
                        self.drop_value(field);
                    }
                }
            }
            Ty::List(list) => {
                // A list whose elements own something: visit every element.
                let elem = self.prog.lists[list as usize];
                let elem_bytes = self.prog.size_of(elem) as i32 * 8;
                if copy {
                    // First a private buffer holding the same bits.
                    self.mov_imm(RSI, elem_bytes as i64);
                    self.runtime(runtime::LIST_CLONE);
                }
                self.load_mem(RDI, RBP, -8);
                self.load_mem(R8, RDI, 16); // capacity
                self.op(TEST, R8, R8);
                let nothing_owned = self.jump_if(CC_E);
                self.load_mem(R8, RDI, 0);
                self.store_mem(RBP, -16, R8); // walks the elements
                self.load_mem(R8, RDI, 8);
                self.store_mem(RBP, -24, R8); // counts down the length
                let top = self.code.len();
                self.load_mem(R8, RBP, -24);
                self.op(TEST, R8, R8);
                let finished = self.jump_if(CC_E);
                self.load_mem(RDI, RBP, -16);
                if copy {
                    self.clone_value(elem);
                } else {
                    self.drop_value(elem);
                }
                self.op_mem_imm(ADD_I, RBP, -16, elem_bytes);
                self.op_mem_imm(SUB_I, RBP, -24, 1);
                self.jump_back(top);
                self.patch(finished);
                if !copy {
                    self.load_mem(RDI, RBP, -8);
                    self.load_mem(RDI, RDI, 0);
                    self.runtime(runtime::FREE);
                }
                self.patch(nothing_owned);
            }
            Ty::Enum(index) => {
                // Only the variant the value is in owns anything, so the
                // tag selects what to visit.
                let prog = self.prog;
                self.load_mem(R8, RDI, 0); // tag
                let mut done = Vec::new();
                for (variant, info) in prog.enums[index as usize].variants.iter().enumerate() {
                    if info.heap.is_empty() {
                        continue;
                    }
                    self.op_imm(CMP_I, R8, variant as i32 + 1);
                    let next = self.jump_if(CC_NE);
                    for &(offset, leaf) in &info.heap {
                        self.load_mem(RDI, RBP, -8);
                        self.lea(RDI, RDI, 8 * offset as i32);
                        if copy {
                            self.clone_value(leaf);
                        } else {
                            self.drop_value(leaf);
                        }
                    }
                    done.push(self.jump());
                    self.patch(next);
                }
                for at in done {
                    self.patch(at);
                }
            }
            _ => unreachable!("only structs, enums and lists of owning elements need a helper"),
        }
        self.epilogue();
    }

    /// Frees everything owned by the value in frame slot `slot`.
    fn drop_frame(&mut self, slot: u32, ty: Ty) {
        self.frame_address(RDI, slot);
        self.drop_value(ty);
    }

    /// Frees everything owned by the value whose address is parked in
    /// frame slot `pointer_slot`.
    fn drop_pointer(&mut self, pointer_slot: u32, ty: Ty) {
        self.load(RDI, pointer_slot);
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

    /// Frees the temporaries of statement `id`. `keep_rax` preserves a
    /// value the statement still needs.
    fn drop_temps(&mut self, id: StmtId, keep_rax: bool) {
        let (start, len) = self.prog.stmt_drops[self.sh.s(id)];
        if len == 0 {
            return;
        }
        if keep_rax {
            self.push();
        }
        for i in start..start + len {
            let (slot, ty) = self.prog.drops[i as usize];
            self.drop_frame(slot, ty);
            // Inside a loop the same temporary is freed again, and it may
            // not have been refilled: a zero capacity or tag marks it as
            // empty.
            for (offset, leaf) in self.prog.owned(ty) {
                self.zero_slot(slot + offset + idle_slot(leaf));
            }
        }
        if keep_rax {
            self.pop(RAX);
        }
    }

    /// The value in frame slot `slot` was copied bit by bit from another
    /// owner. Gives it buffers of its own.
    fn clone_frame(&mut self, slot: u32, ty: Ty) {
        self.frame_address(RDI, slot);
        self.clone_value(ty);
    }

    /// Like `clone_frame`, for the value whose address is parked in frame
    /// slot `pointer_slot`.
    fn clone_pointer(&mut self, pointer_slot: u32, ty: Ty) {
        self.load(RDI, pointer_slot);
        self.clone_value(ty);
    }

    // ---- statements ----------------------------------------------------

    fn stmt(&mut self, id: StmtId) {
        match self.ast.stmts[id as usize] {
            Stmt::Let { value, .. } => {
                let slot = self.prog.stmt_slot[self.sh.s(id)];
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
                let skip_then = self.branch_if_false(id, cond);
                self.block(then_block);
                match else_branch {
                    Else::None => self.patch(skip_then),
                    Else::Block(body) => {
                        let skip_else = self.jump();
                        self.patch(skip_then);
                        self.block(body);
                        self.patch(skip_else);
                    }
                    Else::If(next) => {
                        let skip_else = self.jump();
                        self.patch(skip_then);
                        self.stmt(next);
                        self.patch(skip_else);
                    }
                }
            }
            Stmt::While { cond, body } => {
                let start = self.code.len();
                let exit = self.branch_if_false(id, cond);
                self.loops.push(LoopFixups { start, breaks: Vec::new(), continues: None, live: self.live.len() });
                self.block(body);
                self.jump_back(start);
                self.patch(exit);
                let fixups = self.loops.pop().expect("loop stack is balanced");
                for at in fixups.breaks {
                    self.patch(at);
                }
            }
            Stmt::For { index, iter, body, .. } => self.for_stmt(id, index.is_some(), iter, body),
            Stmt::Break => {
                let live = self.loops.last().expect("checked: inside a loop").live;
                self.drop_live_from(live);
                let at = self.jump();
                self.loops.last_mut().expect("checked: inside a loop").breaks.push(at);
            }
            Stmt::Continue => {
                let fixups = self.loops.last().expect("checked: inside a loop");
                let (live, start, forward) = (fixups.live, fixups.start, fixups.continues.is_some());
                self.drop_live_from(live);
                if forward {
                    let at = self.jump();
                    let fixups = self.loops.last_mut().expect("checked: inside a loop");
                    fixups.continues.as_mut().expect("a `for` loop collects its continues").push(at);
                } else {
                    self.jump_back(start);
                }
            }
            Stmt::Match { scrutinee, arms } => self.match_stmt(id, scrutinee, arms),
        }
    }

    /// Generates the test of an `if` or `while` and returns the jump
    /// taken when it fails. A comparison jumps on its flags directly.
    fn branch_if_false(&mut self, id: StmtId, cond: ExprId) -> usize {
        if self.prog.stmt_drops[self.sh.s(id)].1 == 0 {
            if let Expr::Binary { op, lhs, rhs } = self.ast.exprs[cond as usize] {
                if let (Some(cc), false) = (comparison(op), self.ty(lhs) == Ty::Str) {
                    self.compare_flags(lhs, rhs);
                    // Jump on the opposite condition.
                    return self.jump_if(cc ^ 1);
                }
            }
        }
        self.value(cond);
        self.drop_temps(id, true);
        self.op(TEST, RAX, RAX);
        self.jump_if(CC_E)
    }

    fn assign(&mut self, id: StmtId, target: ExprId, value: ExprId) {
        if self.prog.stmt_flag[self.sh.s(id)] == APPEND {
            // `s = s + more`: the new text is added to `s` in place. It
            // is computed first; only then is `s` looked at.
            let Expr::Binary { rhs, .. } = self.ast.exprs[value as usize] else {
                unreachable!("checked: an append adds something");
            };
            self.address(rhs);
            self.push();
            self.address(target);
            self.mov(RDI, RAX);
            self.pop(RSI);
            self.runtime(runtime::STR_APPEND);
            return;
        }
        let ty = self.ty(target);
        let place = self.place(target);
        if !ty.is_aggregate() {
            self.value(value);
            match place {
                Some(place) if place.by_ref => {
                    self.load(R8, place.slot);
                    self.store_mem(R8, 8 * place.offset as i32, RAX);
                }
                Some(place) => self.store_slot(place.slot + place.offset, RAX),
                None => {
                    // The target is a list element: its address is only
                    // known once the index has been evaluated.
                    self.push();
                    self.address(target);
                    self.mov(R8, RAX);
                    self.pop(RAX);
                    self.store_mem(R8, 0, RAX);
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
                self.load(R8, place.slot);
                self.copy_to_pointer(place.offset, size);
            } else {
                self.copy_to_frame(place.slot + place.offset, size);
            }
            return;
        }

        // First get the new value into a frame area this statement owns:
        // the temporary that computed it, or a fresh copy of a variable.
        self.address(value);
        let source = if self.prog.expr_moved[self.sh.e(value)] {
            match self.variable_slot(value) {
                // A variable that is not used again: its value moves
                // straight out of its own slots.
                Some(slot) => slot,
                None => self.prog.expr_tmp[self.sh.e(value)],
            }
        } else {
            let scratch = self.prog.stmt_slot[self.sh.s(id)];
            self.copy_to_frame(scratch, size);
            if owning {
                self.clone_frame(scratch, ty);
            }
            scratch
        };
        // Then find the target, free what it holds and move the value in.
        self.address(target);
        self.store_slot(self.scratch_slot, RAX);
        if owning {
            self.drop_pointer(self.scratch_slot, ty);
        }
        self.frame_address(RAX, source);
        self.load(R8, self.scratch_slot);
        self.copy_to_pointer(0, size);
        self.moved_out(value);
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
                self.load(R8, slot);
                self.copy_to_pointer(0, self.prog.size_of(ty));
                if self.prog.needs_drop(ty) {
                    if !self.prog.expr_moved[self.sh.e(value)] {
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
            None => false,
        };
        let frees_something = self.prog.stmt_drops[self.sh.s(id)].1 > 0 || !self.live.is_empty();
        if scalar && frees_something {
            self.push();
        }
        self.drop_temps(id, false);
        for i in (0..self.live.len()).rev() {
            let (slot, ty) = self.live[i];
            if Some(slot) != moved_local {
                self.drop_frame(slot, ty);
            }
        }
        if scalar {
            if frees_something {
                self.pop(RAX);
            }
        } else {
            // Nothing, or an aggregate written through the result
            // address: `main` still hands back exit code 0.
            self.mov_imm(RAX, 0);
        }
        self.epilogue();
    }

    fn for_stmt(&mut self, id: StmtId, has_index: bool, iter: ExprId, body: List) {
        let list_ty = self.ty(iter);
        let Ty::List(list) = list_ty else {
            unreachable!("checked: `for` goes through a list");
        };
        let elem_size = self.prog.size_of(self.prog.lists[list as usize]);
        let base = self.prog.stmt_slot[self.sh.s(id)];
        let (position, own_list, item_slot) = (base, base + 1, base + 4);
        let index_slot = item_slot + elem_size;
        let in_place = self.prog.stmt_flag[self.sh.s(id)] == IN_PLACE;

        if !in_place {
            // The loop owns the list: a temporary it takes over, or its
            // own copy of a list that is part of a temporary.
            self.address(iter);
            self.copy_to_frame(own_list, 3);
            if self.prog.stmt_flag[self.sh.s(id)] == COPIED {
                self.clone_frame(own_list, list_ty);
            }
        }
        self.drop_temps(id, false);
        if !in_place {
            self.live.push((own_list, list_ty));
        }
        self.zero_slot(position);

        let start = self.code.len();
        if in_place {
            self.address(iter);
        } else {
            self.frame_address(RAX, own_list);
        }
        self.load_mem(R8, RAX, 8); // length
        self.load(RCX, position);
        self.op(CMP, RCX, R8);
        let exit = self.jump_if(CC_AE);
        if has_index {
            self.store_slot(index_slot, RCX);
        }
        self.load_mem(R8, RAX, 0); // buffer
        self.element_address(RAX, R8, RCX, elem_size);
        self.copy_to_frame(item_slot, elem_size);

        self.loops.push(LoopFixups { start, breaks: Vec::new(), continues: Some(Vec::new()), live: self.live.len() });
        self.block(body);
        let fixups = self.loops.pop().expect("loop stack is balanced");
        for at in fixups.continues.expect("a `for` loop collects its continues") {
            self.patch(at);
        }
        let disp = self.disp(position);
        self.op_mem_imm(ADD_I, RBP, disp, 1);
        self.jump_back(start);
        self.patch(exit);
        for at in fixups.breaks {
            self.patch(at);
        }
        if !in_place {
            self.live.pop();
            self.drop_frame(own_list, list_ty);
        }
    }

    fn match_stmt(&mut self, id: StmtId, scrutinee: ExprId, arms: List) {
        let ty = self.ty(scrutinee);
        let base = self.prog.stmt_slot[self.sh.s(id)];
        let aggregate = ty.is_aggregate();
        // Slot `base` keeps the value being matched: a number or `bool`
        // itself, the address of anything else.
        let mut owned = false;
        if !aggregate {
            self.value(scrutinee);
        } else {
            self.address(scrutinee);
            if self.prog.stmt_flag[self.sh.s(id)] != IN_PLACE {
                // The statement owns the value: a temporary it takes
                // over, or its own copy of a part of a temporary.
                let own = base + 1;
                self.copy_to_frame(own, self.prog.size_of(ty));
                if self.prog.needs_drop(ty) {
                    if self.prog.stmt_flag[self.sh.s(id)] == COPIED {
                        self.clone_frame(own, ty);
                    }
                    owned = true;
                }
                self.frame_address(RAX, own);
            }
        }
        self.store_slot(base, RAX);
        self.drop_temps(id, false);
        if owned {
            self.live.push((base + 1, ty));
        }

        let arm_ids = self.ast.list(arms);
        let mut ends = Vec::new();
        for (i, &arm_id) in arm_ids.iter().enumerate() {
            let arm = self.ast.arms[arm_id as usize];
            let info = self.prog.pats[self.sh.p(arm.pattern)];
            // The checker proved that the arms cover every case, so when
            // the last arm is reached it matches: no test needed.
            let test = i + 1 < arm_ids.len();
            // A failed test jumps to the next arm.
            let mut next = None;
            match info.kind {
                PAT_VARIANT => {
                    if test {
                        self.load(R8, base);
                        self.load_mem(R8, R8, 0); // tag
                        self.op_imm(CMP_I, R8, info.res as i32 + 1);
                        next = Some(self.jump_if(CC_NE));
                    }
                    if let Pattern::Variant { args, .. } = self.ast.patterns[arm.pattern as usize] {
                        for &sub in self.ast.list(args) {
                            let field = self.prog.pats[self.sh.p(sub)];
                            if field.kind == PAT_BIND {
                                // The name stands for the field where it is.
                                self.load(R8, base);
                                self.lea(R8, R8, 8 * field.res as i32);
                                self.store_slot(field.slot, R8);
                            }
                        }
                    }
                }
                PAT_LITERAL if test => match ty {
                    Ty::I64 => {
                        self.load(RCX, base);
                        self.mov_imm(RAX, self.prog.ints[info.res as usize]);
                        self.op(CMP, RCX, RAX);
                        next = Some(self.jump_if(CC_NE));
                    }
                    Ty::Bool => {
                        self.load(RAX, base);
                        self.op(TEST, RAX, RAX);
                        // `true` is skipped when the value is zero, `false` when it is not.
                        next = Some(self.jump_if(if info.res == 0 { CC_NE } else { CC_E }));
                    }
                    _ => {
                        self.string_value(info.res, info.slot);
                        self.load(RDI, base);
                        self.frame_address(RSI, info.slot);
                        self.runtime(runtime::STR_EQ);
                        self.op(TEST, RAX, RAX);
                        next = Some(self.jump_if(CC_E));
                    }
                },
                PAT_BIND => {
                    // The whole value under a name: a number or `bool` is
                    // copied, anything else is referred to where it is.
                    self.load(R8, base);
                    self.store_slot(info.slot, R8);
                }
                _ => {}
            }
            self.block(arm.body);
            if test {
                ends.push(self.jump());
            }
            if let Some(at) = next {
                self.patch(at);
            }
        }
        for at in ends {
            self.patch(at);
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
            if self.prog.needs_drop(ty) && !self.prog.expr_moved[self.sh.e(value)] {
                self.clone_frame(slot, ty);
            }
            self.moved_out(value);
        } else {
            self.value(value);
            self.store_slot(slot, RAX);
        }
    }

    /// The frame slot of `id` when it is a whole local variable.
    fn variable_slot(&self, id: ExprId) -> Option<u32> {
        match self.ast.exprs[id as usize] {
            Expr::Ident(_) => self.place(id).filter(|place| !place.by_ref).map(|place| place.slot),
            _ => None,
        }
    }

    /// Called after the value of `id` has been stored somewhere. When
    /// `id` is a variable whose buffers the store took over, the variable
    /// is left owning nothing, so that the end of its scope frees nothing.
    fn moved_out(&mut self, id: ExprId) {
        if !self.prog.expr_moved[self.sh.e(id)] {
            return;
        }
        if let Some(slot) = self.variable_slot(id) {
            for (offset, leaf) in self.prog.owned(self.ty(id)) {
                self.zero_slot(slot + offset + idle_slot(leaf));
            }
        }
    }

    /// Writes string literal number `index` into three frame slots.
    fn string_value(&mut self, index: u32, slot: u32) {
        // A string value is a pointer, a length and a capacity.
        let (offset, len) = self.prog.strings[index as usize];
        // lea rax, [rip + data]
        self.byte(0x48);
        self.byte(0x8D);
        self.byte(0x05);
        self.imm32(0);
        self.data_refs.push((self.code.len() as u32 - 4, offset));
        self.store_slot(slot, RAX);
        self.mov_imm(RAX, len as i64);
        self.store_slot(slot + 1, RAX);
        self.zero_slot(slot + 2); // capacity 0: not owned
    }

    /// Builds the enum value of expression `id` in its temporary and
    /// leaves the temporary's address in rax.
    fn enum_value(&mut self, id: ExprId, args: &[u32]) {
        let Ty::Enum(index) = self.ty(id) else {
            unreachable!("checked: a variant makes an enum value");
        };
        let variant = self.prog.expr_aux[self.sh.e(id)];
        let tmp = self.prog.expr_tmp[self.sh.e(id)];
        let prog = self.prog;
        for (i, &arg) in args.iter().enumerate() {
            let offset = prog.enums[index as usize].variants[variant as usize].fields[i].1;
            self.store(arg, tmp + offset);
        }
        // The tag goes in last: until then the temporary counts as empty.
        self.mov_imm(RAX, variant as i64 + 1);
        self.store_slot(tmp, RAX);
        self.frame_address(RAX, tmp);
    }

    /// Copies `size` slots from the address in rax into the frame.
    fn copy_to_frame(&mut self, slot: u32, size: u32) {
        for i in 0..size {
            self.load_mem(R9, RAX, 8 * i as i32);
            self.store_slot(slot + i, R9);
        }
    }

    /// Copies `size` slots from the address in rax to `offset` slots past
    /// the address in r8.
    fn copy_to_pointer(&mut self, offset: u32, size: u32) {
        for i in 0..size {
            self.load_mem(R9, RAX, 8 * i as i32);
            self.store_mem(R8, 8 * (offset + i) as i32, R9);
        }
    }

    /// `dst = buffer + index * element size`. Uses r10; `dst` may be the
    /// same register as an input.
    fn element_address(&mut self, dst: u8, buffer: u8, index: u8, elem_size: u32) {
        debug_assert!(buffer != R10 && index != R10);
        self.imul_imm(R10, index, elem_size as i32 * 8);
        self.op(ADD, R10, buffer);
        self.mov(dst, R10);
    }

    /// The location of a variable or of a field path inside a variable,
    /// when it is fixed at compile time. `None` for computed values and
    /// for anything reached through a list index.
    fn place(&self, id: ExprId) -> Option<Place> {
        match self.ast.exprs[id as usize] {
            Expr::Ident(_) => {
                let res = self.prog.expr_res[self.sh.e(id)];
                if res == VARIANT {
                    // `None` and the like are values, not variables.
                    return None;
                }
                Some(Place { slot: res & !BY_REF, by_ref: res & BY_REF != 0, offset: 0 })
            }
            Expr::Field { base, .. } => {
                let mut place = self.place(base)?;
                place.offset += self.prog.expr_res[self.sh.e(id)];
                Some(place)
            }
            _ => None,
        }
    }

    /// Leaves the address of a place in rax.
    fn place_address(&mut self, place: Place) {
        if place.by_ref {
            self.load(RAX, place.slot);
            if place.offset > 0 {
                self.lea(RAX, RAX, 8 * place.offset as i32);
            }
        } else {
            self.frame_address(RAX, place.slot + place.offset);
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

    /// Generates code that leaves in rax the address of an aggregate
    /// value, or of any expression that names a place. A computed
    /// aggregate is first stored in its own temporary.
    fn address(&mut self, id: ExprId) {
        if let Some(place) = self.place(id) {
            self.place_address(place);
            return;
        }
        match self.ast.exprs[id as usize] {
            Expr::Str(_) => {
                let tmp = self.prog.expr_tmp[self.sh.e(id)];
                self.string_value(self.prog.expr_res[self.sh.e(id)], tmp);
                self.frame_address(RAX, tmp);
            }
            Expr::Ident(_) => self.enum_value(id, &[]),
            Expr::Binary { lhs, rhs, .. } => {
                // `a + b` on strings: a new string in this expression's temporary.
                let tmp = self.prog.expr_tmp[self.sh.e(id)];
                self.address(lhs);
                self.push();
                self.address(rhs);
                self.mov(RSI, RAX);
                self.pop(RDI);
                self.frame_address(RDX, tmp);
                self.runtime(runtime::STR_CONCAT);
                self.frame_address(RAX, tmp);
            }
            Expr::StructLit { fields, .. } => {
                let tmp = self.prog.expr_tmp[self.sh.e(id)];
                for &init_id in self.ast.list(fields) {
                    let init = self.ast.field_inits[init_id as usize];
                    self.store(init.value, tmp + self.prog.init_offset[self.sh.i(init_id)]);
                }
                self.frame_address(RAX, tmp);
            }
            Expr::ArrayLit { items } => self.list_literal(id, items),
            Expr::Call { args, .. } if self.prog.expr_res[self.sh.e(id)] == BUILTIN_VARIANT => {
                self.enum_value(id, self.ast.list(args));
            }
            Expr::Call { args, .. } => {
                self.call(id, args);
                self.frame_address(RAX, self.prog.expr_tmp[self.sh.e(id)]);
            }
            Expr::Field { base, .. } => {
                self.address(base);
                let offset = self.prog.expr_res[self.sh.e(id)];
                if offset > 0 {
                    self.lea(RAX, RAX, 8 * offset as i32);
                }
            }
            Expr::Index { base, index } => {
                // The index is evaluated first, so that nothing runs
                // between reading the buffer pointer and using it.
                let Ty::List(list) = self.ty(base) else {
                    unreachable!("checked: only lists are indexed for an address");
                };
                let elem_size = self.prog.size_of(self.prog.lists[list as usize]);
                self.value(index);
                self.push();
                self.address(base);
                self.pop(RCX);
                self.load_mem(R8, RAX, 8); // length
                self.op(CMP, RCX, R8);
                // An unsigned compare also rejects negatives.
                self.runtime_unless(CC_B, runtime::INDEX_FAIL);
                self.load_mem(R8, RAX, 0); // buffer
                self.element_address(RAX, R8, RCX, elem_size);
            }
            _ => unreachable!("checked: only these expressions have an address"),
        }
    }

    fn list_literal(&mut self, id: ExprId, items: List) {
        let tmp = self.prog.expr_tmp[self.sh.e(id)];
        let item_ids = self.ast.list(items);
        let count = item_ids.len() as u32;
        if count == 0 {
            for i in 0..3 {
                self.zero_slot(tmp + i);
            }
            self.frame_address(RAX, tmp);
            return;
        }
        let elem_size = self.prog.size_of(self.ty(item_ids[0]));
        self.mov_imm(RDI, (count * elem_size * 8) as i64);
        self.runtime(runtime::ALLOC);
        self.store_slot(tmp, RAX);
        for (i, &item) in item_ids.iter().enumerate() {
            let offset = i as u32 * elem_size;
            self.eval(item);
            self.load(R8, tmp);
            if self.is_aggregate(item) {
                self.copy_to_pointer(offset, elem_size);
                let elem = self.ty(item);
                if self.prog.needs_drop(elem) && !self.prog.expr_moved[self.sh.e(item)] {
                    // The element was copied from a variable: give the
                    // copy in the list buffers of its own.
                    self.load(RDI, tmp);
                    self.lea(RDI, RDI, 8 * offset as i32);
                    self.clone_value(elem);
                }
                self.moved_out(item);
            } else {
                self.store_mem(R8, 8 * offset as i32, RAX);
            }
        }
        self.mov_imm(RAX, count as i64);
        self.store_slot(tmp + 1, RAX); // length
        self.store_slot(tmp + 2, RAX); // capacity
        self.frame_address(RAX, tmp);
    }

    /// True for a number or `bool` that one or two instructions can load
    /// into any register without disturbing rax: a constant, a variable,
    /// a field of a variable.
    fn is_simple(&self, id: ExprId) -> bool {
        match self.ast.exprs[id as usize] {
            Expr::Int(_) | Expr::Bool(_) => true,
            Expr::Ident(_) | Expr::Field { .. } => !self.is_aggregate(id) && self.place(id).is_some(),
            _ => false,
        }
    }

    /// Loads a simple value into `reg`. Uses r8 for a value behind a pointer.
    fn load_simple(&mut self, id: ExprId, reg: u8) {
        match self.ast.exprs[id as usize] {
            Expr::Int(_) => self.mov_imm(reg, self.prog.ints[self.prog.expr_res[self.sh.e(id)] as usize]),
            Expr::Bool(value) => self.mov_imm(reg, value as i64),
            _ => {
                let place = self.place(id).expect("a simple value that is not a constant is a place");
                if place.by_ref {
                    self.load(R8, place.slot);
                    self.load_mem(reg, R8, 8 * place.offset as i32);
                } else {
                    self.load(reg, place.slot + place.offset);
                }
            }
        }
    }

    /// The constant on the right of an operation when it fits in the
    /// instruction itself.
    fn small_constant(&self, id: ExprId) -> Option<i32> {
        match self.ast.exprs[id as usize] {
            Expr::Int(_) => i32::try_from(self.prog.ints[self.prog.expr_res[self.sh.e(id)] as usize]).ok(),
            _ => None,
        }
    }

    /// Evaluates both operands of an operation on numbers, left one
    /// first: the left one ends in rcx, the right one in rax.
    fn operands(&mut self, lhs: ExprId, rhs: ExprId) {
        self.value(lhs);
        if self.is_simple(rhs) {
            // Nothing runs in between, so the left one can wait in rcx.
            self.mov(RCX, RAX);
            self.load_simple(rhs, RAX);
        } else {
            self.push();
            self.value(rhs);
            self.pop(RCX);
        }
    }

    /// Compares two numbers or `bool`s and leaves the result in the flags.
    fn compare_flags(&mut self, lhs: ExprId, rhs: ExprId) {
        if let Some(value) = self.small_constant(rhs) {
            self.value(lhs);
            self.op_imm(CMP_I, RAX, value);
        } else {
            self.operands(lhs, rhs);
            self.op(CMP, RCX, RAX);
        }
    }

    /// Generates code that leaves a scalar value in rax.
    fn value(&mut self, id: ExprId) {
        match self.ast.exprs[id as usize] {
            Expr::Int(_) => self.mov_imm(RAX, self.prog.ints[self.prog.expr_res[self.sh.e(id)] as usize]),
            Expr::Bool(value) => self.mov_imm(RAX, value as i64),
            Expr::Index { base, index } if self.ty(base) == Ty::Str => {
                // A byte of a string, checked like a list index.
                self.value(index);
                self.push();
                self.address(base);
                self.pop(RCX);
                self.load_mem(R8, RAX, 8); // length
                self.op(CMP, RCX, R8);
                self.runtime_unless(CC_B, runtime::INDEX_FAIL);
                self.load_mem(R8, RAX, 0); // bytes
                self.op(ADD, R8, RCX);
                // movzx rax, byte [r8]
                self.byte(0x49);
                self.byte(0x0F);
                self.byte(0xB6);
                self.byte(0x00);
            }
            Expr::Ident(_) | Expr::Field { .. } | Expr::Index { .. } => match self.place(id) {
                Some(place) if place.by_ref => {
                    self.load(R8, place.slot);
                    self.load_mem(RAX, R8, 8 * place.offset as i32);
                }
                Some(place) => self.load(RAX, place.slot + place.offset),
                None => {
                    // A list element, or a field of a computed value.
                    self.address(id);
                    self.load_mem(RAX, RAX, 0);
                }
            },
            Expr::Unary { op, operand } => {
                self.value(operand);
                match op {
                    UnOp::Neg => {
                        // neg rax
                        self.byte(0x48);
                        self.byte(0xF7);
                        self.byte(0xD8);
                    }
                    UnOp::Not => self.op_imm(XOR_I, RAX, 1),
                }
            }
            Expr::Binary { op: op @ (BinOp::And | BinOp::Or), lhs, rhs } => {
                // Short circuit: the left value is the result when it decides.
                self.value(lhs);
                self.op(TEST, RAX, RAX);
                let skip = self.jump_if(if op == BinOp::Or { CC_NE } else { CC_E });
                self.value(rhs);
                self.patch(skip);
            }
            Expr::Binary { op, lhs, rhs } if self.ty(lhs) == Ty::Str => {
                // `==` and `!=` on strings compare the bytes.
                self.address(lhs);
                self.push();
                self.address(rhs);
                self.mov(RSI, RAX);
                self.pop(RDI);
                self.runtime(runtime::STR_EQ);
                if op == BinOp::NotEq {
                    self.op_imm(XOR_I, RAX, 1);
                }
            }
            Expr::Binary { op, lhs, rhs } => self.binary(op, lhs, rhs),
            Expr::Call { args, .. } => self.call(id, args),
            Expr::MutArg(_) | Expr::Float(_) | Expr::Str(_) | Expr::StructLit { .. } | Expr::ArrayLit { .. } => {
                unreachable!("rejected by the checker or not a scalar")
            }
        }
    }

    /// Generates an operation on two numbers or `bool`s, result in rax.
    fn binary(&mut self, op: BinOp, lhs: ExprId, rhs: ExprId) {
        if let Some(cc) = comparison(op) {
            self.compare_flags(lhs, rhs);
            // setcc al; movzx rax, al
            self.byte(0x0F);
            self.byte(0x90 | cc);
            self.byte(0xC0);
            self.byte(0x48);
            self.byte(0x0F);
            self.byte(0xB6);
            self.byte(0xC0);
            return;
        }
        if let (BinOp::Add | BinOp::Sub, Some(value)) = (op, self.small_constant(rhs)) {
            self.value(lhs);
            self.op_imm(if op == BinOp::Add { ADD_I } else { SUB_I }, RAX, value);
            return;
        }
        // Here rcx holds the left operand and rax the right one.
        self.operands(lhs, rhs);
        match op {
            BinOp::Add => self.op(ADD, RAX, RCX),
            BinOp::Sub => {
                self.op(SUB, RCX, RAX);
                self.mov(RAX, RCX);
            }
            BinOp::Mul => self.imul(RAX, RCX),
            BinOp::BitAnd => self.op(AND, RAX, RCX),
            BinOp::BitOr => self.op(OR, RAX, RCX),
            BinOp::BitXor => self.op(XOR, RAX, RCX),
            BinOp::Shl | BinOp::Shr => {
                // xchg rax, rcx: the value belongs in rax, the count in cl.
                // The processor uses the low 6 bits of the count.
                self.byte(0x48);
                self.byte(0x91);
                self.byte(0x48);
                self.byte(0xD3);
                self.byte(if op == BinOp::Shl { 0xE0 } else { 0xF8 }); // shl rax, cl / sar rax, cl
            }
            BinOp::Div | BinOp::Rem => {
                // xchg rax, rcx: the dividend belongs in rax
                self.byte(0x48);
                self.byte(0x91);
                self.op(TEST, RCX, RCX);
                self.runtime_unless(CC_NE, runtime::DIV_ZERO);
                // The processor refuses to divide the smallest number by
                // -1. Dividing by -1 is negating, with remainder 0, and
                // negating wraps around like every other operation.
                self.op_imm(CMP_I, RCX, -1);
                let ordinary = self.jump_if(CC_NE);
                if op == BinOp::Div {
                    self.byte(0x48);
                    self.byte(0xF7);
                    self.byte(0xD8); // neg rax
                } else {
                    self.mov_imm(RAX, 0);
                }
                let done = self.jump();
                self.patch(ordinary);
                self.byte(0x48);
                self.byte(0x99); // cqo: sign-extend rax into rdx
                self.byte(0x48);
                self.byte(0xF7);
                self.byte(0xF9); // idiv rcx
                if op == BinOp::Rem {
                    self.mov(RAX, RDX);
                }
                self.patch(done);
            }
            _ => unreachable!("comparisons are handled above, `&&` and `||` by the caller"),
        }
    }

    /// Whether argument `i` of the call target travels as an address.
    fn arg_by_ref(&self, target: u32, i: usize) -> bool {
        if target >= FIRST_BUILTIN {
            matches!(
                target,
                BUILTIN_PRINT_STR | BUILTIN_EPRINT | BUILTIN_READ_FILE | BUILTIN_WRITE_FILE | BUILTIN_SET_EXECUTABLE
            )
        } else {
            self.prog.fns[target as usize].params[i].by_ref
        }
    }

    /// Calls a function or a built-in. A scalar result is left in rax; an
    /// aggregate result is written to the call's temporary.
    fn call(&mut self, id: ExprId, args: List) {
        let target = self.prog.expr_res[self.sh.e(id)];
        let arg_ids = self.ast.list(args);
        let Expr::Call { callee, .. } = self.ast.exprs[id as usize] else {
            unreachable!("a call is a call");
        };
        // In `value.method(...)` the value is the receiver.
        let receiver = match self.ast.exprs[callee as usize] {
            Expr::Field { base, .. } => Some(base),
            _ => None,
        };
        if let BUILTIN_LEN | BUILTIN_LIST_PUSH | BUILTIN_LIST_POP | BUILTIN_STR_SLICE = target {
            let base = receiver.expect("checked: a method call names its receiver");
            return self.list_method(id, target, base, arg_ids);
        }
        // A method the standard library writes in azoL is a function
        // whose first argument is the receiver. Arguments are evaluated
        // in the order written and wait on the stack.
        let mut count = 0;
        for (i, arg) in receiver.into_iter().chain(arg_ids.iter().copied()).enumerate() {
            let by_ref = self.arg_by_ref(target, i);
            match self.ast.exprs[arg as usize] {
                Expr::MutArg(inner) => self.address(inner),
                _ if by_ref => self.address(arg),
                _ => self.value(arg),
            }
            self.push();
            count += 1;
        }
        for i in (0..count).rev() {
            self.pop(ARGS[i]);
        }
        // Where a result that is not a number goes: the runtime takes
        // that address as one more argument, azoL functions in rax.
        let tmp = self.prog.expr_tmp[self.sh.e(id)];
        match target {
            BUILTIN_PRINT_I64 => self.runtime(runtime::PRINT_I64),
            BUILTIN_PRINT_BOOL => self.runtime(runtime::PRINT_BOOL),
            BUILTIN_PRINT_STR => self.runtime(runtime::PRINT_STR),
            BUILTIN_EPRINT => self.runtime(runtime::EPRINT_STR),
            BUILTIN_EXIT => self.runtime(runtime::EXIT),
            BUILTIN_STR_FROM_I64 | BUILTIN_STR_FROM_BOOL | BUILTIN_CHR | BUILTIN_ARGS => {
                self.frame_address(ARGS[count], tmp);
                self.runtime(match target {
                    BUILTIN_STR_FROM_I64 => runtime::STR_FROM_I64,
                    BUILTIN_STR_FROM_BOOL => runtime::STR_FROM_BOOL,
                    BUILTIN_CHR => runtime::CHR,
                    _ => runtime::ARGS,
                });
            }
            BUILTIN_READ_FILE | BUILTIN_WRITE_FILE | BUILTIN_SET_EXECUTABLE => {
                // The routine fills in what the `Result` holds and says
                // which variant it is: 0 for `Ok`, 1 for `Err`.
                self.frame_address(ARGS[count], tmp + 1);
                self.runtime(match target {
                    BUILTIN_READ_FILE => runtime::READ_FILE,
                    BUILTIN_WRITE_FILE => runtime::WRITE_FILE,
                    _ => runtime::SET_EXECUTABLE,
                });
                self.op_imm(ADD_I, RAX, 1); // the tag
                self.store_slot(tmp, RAX);
            }
            _ => {
                if self.is_aggregate(id) {
                    // The callee writes its result where rax points.
                    self.frame_address(RAX, tmp);
                }
                let at = self.call_field();
                self.calls.push((at, target));
            }
        }
    }

    fn list_method(&mut self, id: ExprId, method: u32, base: ExprId, args: &[u32]) {
        if method == BUILTIN_LEN {
            // Lists and strings keep their length in the same place.
            self.address(base);
            self.load_mem(RAX, RAX, 8);
            return;
        }
        if method == BUILTIN_STR_SLICE {
            // The bounds are evaluated first, so that nothing runs
            // between finding the string and reading it.
            self.value(args[0]);
            self.push();
            self.value(args[1]);
            self.push();
            self.address(base);
            self.mov(RDI, RAX);
            self.pop(RDX);
            self.pop(RSI);
            self.frame_address(RCX, self.prog.expr_tmp[self.sh.e(id)]);
            self.runtime(runtime::STR_SLICE);
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
                let tmp = self.prog.expr_tmp[self.sh.e(id)];
                if elem.is_aggregate() {
                    self.address(args[0]);
                    self.copy_to_frame(tmp, elem_size);
                    if self.prog.needs_drop(elem) && !self.prog.expr_moved[self.sh.e(args[0])] {
                        self.clone_frame(tmp, elem);
                    }
                    self.moved_out(args[0]);
                } else {
                    self.value(args[0]);
                    self.store_slot(tmp, RAX);
                }
                self.address(base);
                self.mov(RDI, RAX);
                self.frame_address(RSI, tmp);
                self.mov_imm(RDX, elem_size as i64 * 8);
                self.runtime(runtime::LIST_PUSH);
            }
            _ => {
                self.address(base);
                self.load_mem(R8, RAX, 8); // length
                self.op(TEST, R8, R8);
                self.runtime_unless(CC_NE, runtime::POP_EMPTY);
                self.op_imm(SUB_I, R8, 1);
                self.store_mem(RAX, 8, R8);
                self.load_mem(R9, RAX, 0); // buffer
                self.element_address(RAX, R9, R8, elem_size);
                if elem.is_aggregate() {
                    self.copy_to_frame(self.prog.expr_tmp[self.sh.e(id)], elem_size);
                } else {
                    self.load_mem(RAX, RAX, 0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser, sema};
    use std::fmt::Write as _;

    const NAMES: [&str; runtime::ENTRY_COUNT] = [
        "print_i64", "print_bool", "print_str", "div_zero", "index_fail", "pop_empty", "alloc", "free", "list_push",
        "list_clone", "drop", "str_concat", "str_eq", "str_from_i64", "str_from_bool", "eprint_str", "exit",
        "str_slice", "chr", "args", "read_file", "write_file", "str_append", "set_executable",
    ];

    /// Compiles a program the way `azol build` does, up to machine code.
    fn compile(user: &str) -> Result<Code, String> {
        let mut src = user.to_string();
        let user_len = src.len();
        let prelude_start = user_len + 1;
        src.push('\n');
        src.push_str(include_str!("prelude.azl"));
        let tokens = lexer::lex(&src[..user_len]).map_err(|e| e.message.to_string())?;
        let ast = parser::parse(&src, &tokens).map_err(|e| e.message)?;
        let mut prelude_tokens = lexer::lex(&src[prelude_start..]).expect("the standard library has valid tokens");
        for token in &mut prelude_tokens {
            token.start += prelude_start as u32;
        }
        let ast = parser::parse_into(ast, &src, &prelude_tokens).expect("the standard library parses");
        let program = sema::check_with_prelude(&ast, &src, prelude_start as u32).map_err(|e| e.message)?;
        Ok(generate(&ast, &program))
    }

    /// The code as assembler input: plain bytes, with every reference to
    /// the runtime or to the program's text left for the linker.
    fn assembly(code: &Code) -> String {
        let mut fields: Vec<(u32, String)> = Vec::new();
        for &(at, entry) in &code.runtime_calls {
            fields.push((at, format!("rt_{}", NAMES[entry as usize])));
        }
        for &(at, offset) in &code.data_refs {
            fields.push((at, format!("azol_data + {offset}")));
        }
        fields.sort();
        let mut out = String::from(".text\n.globl azol_main\nazol_code:\n");
        let mut at = 0usize;
        let bytes = |out: &mut String, from: usize, to: usize| {
            for chunk in code.bytes[from..to].chunks(24) {
                let list: Vec<String> = chunk.iter().map(|b| b.to_string()).collect();
                writeln!(out, ".byte {}", list.join(",")).unwrap();
            }
        };
        for (field, target) in fields {
            bytes(&mut out, at, field as usize);
            writeln!(out, ".long {target} - . - 4").unwrap();
            at = field as usize + 4;
        }
        bytes(&mut out, at, code.bytes.len());
        writeln!(out, ".set azol_main, azol_code + {}", code.entry).unwrap();
        out.push_str(".section .rodata\nazol_data:\n.byte 0\n");
        for chunk in code.data.chunks(24) {
            let list: Vec<String> = chunk.iter().map(|b| b.to_string()).collect();
            writeln!(out, ".byte {}", list.join(",")).unwrap();
        }
        // The first `.byte 0` only keeps the label valid for empty data;
        // it must not shift the offsets.
        out.replace("azol_data:\n.byte 0\n", "azol_data:\n") + ".byte 0\n.section .note.GNU-stack,\"\",@progbits\n"
    }

    /// Test harness hook: with AZOL_X64_IN (a directory of `.azl` files or
    /// one file) and AZOL_X64_OUT (a directory), writes `<name>.s` for
    /// every program. Run with `-- --ignored`.
    #[test]
    #[ignore]
    fn dump_for_harness() {
        let input = std::env::var("AZOL_X64_IN").expect("AZOL_X64_IN");
        let output = std::env::var("AZOL_X64_OUT").expect("AZOL_X64_OUT");
        let input = std::path::Path::new(&input);
        let files: Vec<std::path::PathBuf> = if input.is_dir() {
            let mut all: Vec<_> = std::fs::read_dir(input)
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| p.extension().is_some_and(|x| x == "azl"))
                .collect();
            all.sort();
            all
        } else {
            vec![input.to_path_buf()]
        };
        for file in files {
            let name = file.file_stem().unwrap().to_str().unwrap();
            let source = std::fs::read_to_string(&file).unwrap();
            match compile(&source) {
                Ok(code) => std::fs::write(format!("{output}/{name}.s"), assembly(&code)).unwrap(),
                Err(message) => println!("SKIP {name}: {message}"),
            }
        }
    }

    #[test]
    fn encodings_match_the_reference_assembler() {
        // Expected bytes were produced by assembling the same
        // instructions with the GNU assembler.
        let mut g = Gen {
            ast: &Ast::default(),
            prog: unsafe { &*std::ptr::NonNull::<Program>::dangling().as_ptr() },
            code: Vec::new(),
            fn_starts: Vec::new(),
            calls: Vec::new(),
            runtime_calls: Vec::new(),
            data_refs: Vec::new(),
            glue: Vec::new(),
            glue_ids: HashMap::new(),
            glue_calls: Vec::new(),
            loops: Vec::new(),
            live: Vec::new(),
            frame_bytes: 0,
            ret: Ty::Unit,
            sret_slot: None,
            scratch_slot: 0,
            sh: Shift::default(),
        };
        let check = |g: &mut Gen, expected: &[u8], what: &str| {
            assert_eq!(g.code, expected, "{what}");
            g.code.clear();
        };
        g.op(AND, RAX, RCX);
        check(&mut g, &[0x48, 0x21, 0xC8], "and rax, rcx");
        g.op(OR, RAX, RCX);
        check(&mut g, &[0x48, 0x09, 0xC8], "or rax, rcx");
        g.op(XOR, RAX, RCX);
        check(&mut g, &[0x48, 0x31, 0xC8], "xor rax, rcx");
        g.load_mem(RAX, RBP, -8);
        check(&mut g, &[0x48, 0x8B, 0x45, 0xF8], "mov rax, [rbp-8]");
        g.load_mem(R9, RAX, 16);
        check(&mut g, &[0x4C, 0x8B, 0x48, 0x10], "mov r9, [rax+16]");
        g.load_mem(R8, R8, 0);
        check(&mut g, &[0x4D, 0x8B, 0x00], "mov r8, [r8]");
        g.load_mem(RDI, RBP, -4096);
        check(&mut g, &[0x48, 0x8B, 0xBD, 0x00, 0xF0, 0xFF, 0xFF], "mov rdi, [rbp-4096]");
        g.store_mem(RBP, -16, R11);
        check(&mut g, &[0x4C, 0x89, 0x5D, 0xF0], "mov [rbp-16], r11");
        g.store_mem(R8, 800, R9);
        check(&mut g, &[0x4D, 0x89, 0x88, 0x20, 0x03, 0x00, 0x00], "mov [r8+800], r9");
        g.store_mem(RAX, 8, R8);
        check(&mut g, &[0x4C, 0x89, 0x40, 0x08], "mov [rax+8], r8");
        g.zero_mem(RBP, -24);
        check(&mut g, &[0x48, 0xC7, 0x45, 0xE8, 0, 0, 0, 0], "mov qword [rbp-24], 0");
        g.lea(RDI, RDI, 24);
        check(&mut g, &[0x48, 0x8D, 0x7F, 0x18], "lea rdi, [rdi+24]");
        g.lea(RAX, RBP, -300);
        check(&mut g, &[0x48, 0x8D, 0x85, 0xD4, 0xFE, 0xFF, 0xFF], "lea rax, [rbp-300]");
        g.lea(R8, R8, 8);
        check(&mut g, &[0x4D, 0x8D, 0x40, 0x08], "lea r8, [r8+8]");
        g.mov(RBP, RSP);
        check(&mut g, &[0x48, 0x89, 0xE5], "mov rbp, rsp");
        g.mov(RDI, RAX);
        check(&mut g, &[0x48, 0x89, 0xC7], "mov rdi, rax");
        g.mov(R8, RAX);
        check(&mut g, &[0x49, 0x89, 0xC0], "mov r8, rax");
        g.mov(RAX, R10);
        check(&mut g, &[0x4C, 0x89, 0xD0], "mov rax, r10");
        g.op(ADD, RAX, RCX);
        check(&mut g, &[0x48, 0x01, 0xC8], "add rax, rcx");
        g.op(SUB, RCX, RAX);
        check(&mut g, &[0x48, 0x29, 0xC1], "sub rcx, rax");
        g.op(CMP, RCX, R8);
        check(&mut g, &[0x4C, 0x39, 0xC1], "cmp rcx, r8");
        g.op(TEST, R8, R8);
        check(&mut g, &[0x4D, 0x85, 0xC0], "test r8, r8");
        g.op(ADD, R10, R9);
        check(&mut g, &[0x4D, 0x01, 0xCA], "add r10, r9");
        g.op_imm(SUB_I, RSP, 4096);
        check(&mut g, &[0x48, 0x81, 0xEC, 0x00, 0x10, 0x00, 0x00], "sub rsp, 4096");
        g.op_imm(CMP_I, R8, 3);
        check(&mut g, &[0x49, 0x81, 0xF8, 3, 0, 0, 0], "cmp r8, 3");
        g.op_imm(XOR_I, RAX, 1);
        check(&mut g, &[0x48, 0x81, 0xF0, 1, 0, 0, 0], "xor rax, 1");
        g.op_imm(CMP_I, RCX, -1);
        check(&mut g, &[0x48, 0x81, 0xF9, 0xFF, 0xFF, 0xFF, 0xFF], "cmp rcx, -1");
        g.op_mem_imm(ADD_I, RBP, -16, 24);
        check(&mut g, &[0x48, 0x81, 0x45, 0xF0, 24, 0, 0, 0], "add qword [rbp-16], 24");
        g.op_mem_imm(SUB_I, RBP, -24, 1);
        check(&mut g, &[0x48, 0x81, 0x6D, 0xE8, 1, 0, 0, 0], "sub qword [rbp-24], 1");
        g.imul(RAX, RCX);
        check(&mut g, &[0x48, 0x0F, 0xAF, 0xC1], "imul rax, rcx");
        g.imul_imm(R10, RCX, 24);
        check(&mut g, &[0x4C, 0x69, 0xD1, 24, 0, 0, 0], "imul r10, rcx, 24");
        g.imul_imm(R10, R8, 8);
        check(&mut g, &[0x4D, 0x69, 0xD0, 8, 0, 0, 0], "imul r10, r8, 8");
        g.push();
        check(&mut g, &[0x50, 0x50], "push rax; push rax");
        g.pop(RCX);
        check(&mut g, &[0x59, 0x59], "pop rcx; pop rcx");
        g.pop(R10);
        check(&mut g, &[0x41, 0x5A, 0x41, 0x5A], "pop r10; pop r10");
        g.push_reg(RBP);
        check(&mut g, &[0x55], "push rbp");
        g.mov_imm(RAX, 0);
        check(&mut g, &[0xB8, 0, 0, 0, 0], "mov eax, 0");
        g.mov_imm(R8, 7);
        check(&mut g, &[0x41, 0xB8, 7, 0, 0, 0], "mov r8d, 7");
        g.mov_imm(RAX, -1);
        check(&mut g, &[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF], "mov rax, -1");
        g.mov_imm(RSI, 0x1_0000_0000);
        check(&mut g, &[0x48, 0xBE, 0, 0, 0, 0, 1, 0, 0, 0], "movabs rsi, 0x100000000");
        g.mov_imm(R9, i64::MIN);
        check(&mut g, &[0x49, 0xB9, 0, 0, 0, 0, 0, 0, 0, 0x80], "movabs r9, i64::MIN");
        g.epilogue();
        check(&mut g, &[0xC9, 0xC3], "leave; ret");
    }

    #[test]
    fn records_runtime_calls_and_text_references() {
        let code = compile("fn main() {\n    print(\"hi\")\n    print(1 + 2)\n}\n").unwrap();
        // Every recorded field is a zero placeholder after a `call` or a `lea rax, [rip]`.
        assert!(code.runtime_calls.iter().any(|&(_, entry)| entry == runtime::PRINT_STR));
        assert!(code.runtime_calls.iter().any(|&(_, entry)| entry == runtime::PRINT_I64));
        for &(at, _) in &code.runtime_calls {
            assert_eq!(code.bytes[at as usize - 1], 0xE8);
            assert_eq!(code.bytes[at as usize..at as usize + 4], [0, 0, 0, 0]);
        }
        for &(at, _) in &code.data_refs {
            assert_eq!(code.bytes[at as usize - 3..at as usize], [0x48, 0x8D, 0x05]);
        }
        assert!(code.data_refs.iter().any(|&(_, offset)| code.data[offset as usize..].starts_with(b"hi")));
        assert!((code.entry as usize) < code.bytes.len());
    }
}
