// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Turns a checked program into x86-64 machine code for Linux, in one
//! pass over the syntax tree. It follows `arm64.rs` statement for
//! statement: a scalar expression leaves its value in rax, an aggregate
//! expression leaves its address in rax, earlier values wait on the
//! stack, and variables live in the function's frame or in registers.
//!
//! Registers: rax carries values, rcx and rdx second operands, r8-r10
//! are scratch within one step. Arguments travel in rdi, rsi, rdx, rcx,
//! r8, r9, r10 and r11; the address for an aggregate result in rax.
//! The five most used numbers and `bool`s of a function live in rbx and
//! r12-r15, which survive calls: every function saves the ones it uses
//! on entry and restores them on exit. In a function with a loop, the
//! registers left over hold the addresses that arrived as parameters.
//! Values parked on the stack take 16 bytes each and frames are
//! multiples of 16, so the stack pointer is a multiple of 16 at every
//! call. While the right side of an operation needs no call and nothing
//! on the stack, the left side waits in r9, r11, rsi or rdi instead.
//!
//! Inside straight-line code an index is checked against the length of
//! its list once: the generator remembers which checks have been made
//! and what r10 holds (the buffer of a list or the address of one of
//! its elements), and forgets it wherever the list could have changed
//! or another path joins. See `Known` and `Held`.

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
const RBX: u8 = 3;
const RSP: u8 = 4;
const RBP: u8 = 5;
const RSI: u8 = 6;
const RDI: u8 = 7;
const R8: u8 = 8;
const R9: u8 = 9;
const R10: u8 = 10;
const R11: u8 = 11;
const R12: u8 = 12;
const R13: u8 = 13;
const R14: u8 = 14;
const R15: u8 = 15;

/// The registers variables can live in, in the order they are handed
/// out. The checker lists a function's candidates most used first; the
/// first five get these.
const VARIABLE_REGISTERS: [u8; 5] = [RBX, R12, R13, R14, R15];

/// The registers arguments travel in, first to last. The first four are
/// also where the C calling convention, which the runtime uses, puts them.
const ARGS: [u8; 8] = [RDI, RSI, RDX, RCX, R8, R9, R10, R11];

/// Where the left operand of an operation waits while the right one is
/// computed, when that takes no call: one register for each operation
/// that is waiting. Nothing else uses these while a value is computed.
const PARK: [u8; 4] = [R9, R11, RSI, RDI];

// Condition codes, as used in `jcc` and `setcc`.
const CC_B: u8 = 0x2;
const CC_AE: u8 = 0x3;
const CC_A: u8 = 0x7;
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
const OR_I: u8 = 1;
const AND_I: u8 = 4;
const SUB_I: u8 = 5;
const XOR_I: u8 = 6;
const CMP_I: u8 = 7;

/// The multiplier and the shift that turn a signed division by `by`
/// into a multiplication: for every `n`, with `high` the upper 64 bits of
/// `n * multiplier` (plus `n` when `by` is positive and the multiplier
/// negative, minus `n` in the opposite case), `n / by` is
/// `(high >> shift)` plus one if that is negative. `by` is not 0, 1, -1
/// or the smallest number. From Hacker's Delight, figure 10-1.
fn magic_for(by: i64) -> (i64, u32) {
    const TWO63: u64 = 1 << 63;
    let ad = by.unsigned_abs();
    let t = TWO63 + ((by as u64) >> 63);
    let anc = t - 1 - t % ad;
    let mut p = 63;
    let mut q1 = TWO63 / anc;
    let mut r1 = TWO63 - q1 * anc;
    let mut q2 = TWO63 / ad;
    let mut r2 = TWO63 - q2 * ad;
    loop {
        p += 1;
        q1 = q1.wrapping_mul(2);
        r1 = r1.wrapping_mul(2);
        if r1 >= anc {
            q1 = q1.wrapping_add(1);
            r1 -= anc;
        }
        q2 = q2.wrapping_mul(2);
        r2 = r2.wrapping_mul(2);
        if r2 >= ad {
            q2 = q2.wrapping_add(1);
            r2 -= ad;
        }
        let delta = ad - r2;
        if !(q1 < delta || (q1 == delta && r1 == 0)) {
            break;
        }
    }
    let mut magic = q2.wrapping_add(1) as i64;
    if by < 0 {
        magic = magic.wrapping_neg();
    }
    (magic, p - 64)
}

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

/// The right operand of an operation once both have been evaluated.
#[derive(Clone, Copy)]
enum Right {
    Reg(u8),
    /// A constant small enough to sit inside the instruction.
    Constant(i32),
    /// A variable in the frame or a list element, used where it is.
    Mem(Mem),
    /// The right operand is in rax and it is the left one that sits in
    /// memory: it waited in the frame while the right one was computed.
    Waiting(Mem),
}

/// An index as the generator can remember it: a variable that lives in
/// a register, or a small constant.
#[derive(Clone, Copy, PartialEq)]
enum At {
    Reg(u8),
    Constant(i32),
}

/// A fact about the code generated so far: on every path that reaches
/// this point, index `at` has been checked against the length of the
/// list or string in `place`, and neither has changed since. `since` is
/// where in the code the check ended.
struct Known {
    place: Place,
    at: At,
    since: usize,
}

/// What r10 holds: the buffer of the list or string in `place`, or the
/// address of its element `at`, from position `since` in the code on.
#[derive(Clone, Copy)]
struct Held {
    place: Place,
    at: Option<At>,
    since: usize,
}

/// How many checks are remembered at a time.
const KNOWN: usize = 4;

/// A memory operand: `base + (index << scale) + disp`.
#[derive(Clone, Copy)]
struct Mem {
    base: u8,
    index: Option<(u8, u8)>,
    disp: i32,
}

struct LoopFixups {
    breaks: Vec<usize>,
    /// `continue` jumps forward, to the step of a `for` loop or the
    /// test of a `while` loop, so those jumps are patched later.
    continues: Vec<usize>,
    /// Number of owning locals alive when the loop was entered.
    live: usize,
}

/// Where a variable or field path lives: a frame slot, or an offset from
/// the pointer stored in a frame slot.
#[derive(Clone, Copy, PartialEq)]
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
    /// The variables of the function being generated that live in
    /// registers: slot and register. Only the first `register_count`
    /// entries are in use. A parameter that arrives as an address can be
    /// among them: then the register holds that address.
    registers: [(u32, u8); VARIABLE_REGISTERS.len()],
    register_count: usize,
    /// How many of the `PARK` registers hold a waiting operand.
    parked: usize,
    /// How many operands wait in the frame, below the function's slots,
    /// and the most that did at any time in this function.
    waiting: u32,
    waiting_most: u32,
    /// Jumps of the conditions being generated that are not patched yet.
    jumps: Vec<usize>,
    /// The index checks that need not be repeated at this point.
    known: Vec<Known>,
    /// What r10 holds at this point, when it is worth knowing.
    held: Option<Held>,
    /// First frame slot of the area where those registers' old values
    /// are kept.
    saved_slot: u32,
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
        registers: [(0, 0); VARIABLE_REGISTERS.len()],
        register_count: 0,
        parked: 0,
        waiting: 0,
        waiting_most: 0,
        jumps: Vec::new(),
        known: Vec::with_capacity(KNOWN),
        held: None,
        saved_slot: 0,
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

    /// The prefix for an instruction whose memory operand has an index
    /// register as well.
    fn rex_index(&mut self, reg: u8, index: u8, base: u8) {
        self.byte(0x48 | (reg >> 3) << 2 | (index >> 3) << 1 | (base >> 3));
    }

    /// The operand bytes for `reg` and the memory at
    /// `base + (index << scale) + disp`.
    fn mem_index(&mut self, reg: u8, base: u8, index: u8, scale: u8, disp: i32) {
        debug_assert!(index != RSP && scale < 4);
        let mode = if disp == 0 && base & 7 != 5 {
            0
        } else if (-128..=127).contains(&disp) {
            1
        } else {
            2
        };
        self.byte(mode << 6 | (reg & 7) << 3 | 4);
        self.byte(scale << 6 | (index & 7) << 3 | (base & 7));
        match mode {
            1 => self.byte(disp as i8 as u8),
            2 => self.imm32(disp),
            _ => {}
        }
    }

    /// `mov dst, [base + (index << scale) + disp]`
    fn load_index(&mut self, dst: u8, base: u8, index: u8, scale: u8, disp: i32) {
        self.writes(dst);
        self.rex_index(dst, index, base);
        self.byte(0x8B);
        self.mem_index(dst, base, index, scale, disp);
    }

    /// `mov [base + (index << scale) + disp], src`
    fn store_index(&mut self, base: u8, index: u8, scale: u8, disp: i32, src: u8) {
        self.rex_index(src, index, base);
        self.byte(0x89);
        self.mem_index(src, base, index, scale, disp);
    }

    /// `mov qword [base + (index << scale) + disp], value`
    fn store_index_imm(&mut self, base: u8, index: u8, scale: u8, disp: i32, value: i32) {
        self.rex_index(0, index, base);
        self.byte(0xC7);
        self.mem_index(0, base, index, scale, disp);
        self.imm32(value);
    }

    /// `lea dst, [base + (index << scale) + disp]`
    fn lea_index(&mut self, dst: u8, base: u8, index: u8, scale: u8, disp: i32) {
        self.writes(dst);
        self.rex_index(dst, index, base);
        self.byte(0x8D);
        self.mem_index(dst, base, index, scale, disp);
    }

    /// `lea dst, [index << scale]`
    fn lea_scaled(&mut self, dst: u8, index: u8, scale: u8) {
        debug_assert!(index != RSP && scale < 4);
        self.writes(dst);
        self.rex_index(dst, index, 0);
        self.byte(0x8D);
        self.byte((dst & 7) << 3 | 4);
        // No base register: a 32-bit displacement takes its place.
        self.byte(scale << 6 | (index & 7) << 3 | 5);
        self.imm32(0);
    }

    /// `movzx eax, byte [at]`, which clears the rest of rax.
    fn load_byte(&mut self, at: Mem) {
        let index = at.index.map_or(0, |(index, _)| index);
        if at.base >= 8 || index >= 8 {
            self.byte(0x40 | (index >> 3) << 1 | (at.base >> 3));
        }
        self.byte(0x0F);
        self.byte(0xB6);
        match at.index {
            Some((index, scale)) => self.mem_index(RAX, at.base, index, scale, at.disp),
            None => self.mem(RAX, at.base, at.disp),
        }
    }

    /// `mov dst, [at + extra]`
    fn load_at(&mut self, dst: u8, at: Mem, extra: i32) {
        match at.index {
            Some((index, scale)) => self.load_index(dst, at.base, index, scale, at.disp + extra),
            None => self.load_mem(dst, at.base, at.disp + extra),
        }
    }

    /// `mov [at + extra], src`
    fn store_at(&mut self, at: Mem, extra: i32, src: u8) {
        match at.index {
            Some((index, scale)) => self.store_index(at.base, index, scale, at.disp + extra, src),
            None => self.store_mem(at.base, at.disp + extra, src),
        }
    }

    /// `mov qword [at + extra], value`
    fn store_imm_at(&mut self, at: Mem, extra: i32, value: i32) {
        match at.index {
            Some((index, scale)) => self.store_index_imm(at.base, index, scale, at.disp + extra, value),
            None => self.store_imm(at.base, at.disp + extra, value),
        }
    }

    /// `op reg, [at]` for `ADD`, `SUB`, `CMP`, `AND`, `OR` and `XOR`.
    fn op_at(&mut self, op: u8, reg: u8, at: Mem) {
        if op != CMP {
            self.writes(reg);
        }
        // The form that reads its second operand from memory is two up.
        match at.index {
            Some((index, scale)) => {
                self.rex_index(reg, index, at.base);
                self.byte(op + 2);
                self.mem_index(reg, at.base, index, scale, at.disp);
            }
            None => {
                self.rex(reg, at.base);
                self.byte(op + 2);
                self.mem(reg, at.base, at.disp);
            }
        }
    }

    /// `cmp [at], reg`
    fn cmp_at_reg(&mut self, at: Mem, reg: u8) {
        match at.index {
            Some((index, scale)) => {
                self.rex_index(reg, index, at.base);
                self.byte(CMP);
                self.mem_index(reg, at.base, index, scale, at.disp);
            }
            None => {
                self.rex(reg, at.base);
                self.byte(CMP);
                self.mem(reg, at.base, at.disp);
            }
        }
    }

    /// `cmp qword [at], value`
    fn cmp_at_imm(&mut self, at: Mem, value: i32) {
        let small = i8::try_from(value);
        match at.index {
            Some((index, scale)) => {
                self.rex_index(0, index, at.base);
                self.byte(if small.is_ok() { 0x83 } else { 0x81 });
                self.mem_index(CMP_I, at.base, index, scale, at.disp);
            }
            None => {
                self.rex(0, at.base);
                self.byte(if small.is_ok() { 0x83 } else { 0x81 });
                self.mem(CMP_I, at.base, at.disp);
            }
        }
        match small {
            Ok(small) => self.byte(small as u8),
            Err(_) => self.imm32(value),
        }
    }

    /// `imul reg, [at]`
    fn imul_at(&mut self, reg: u8, at: Mem) {
        self.writes(reg);
        match at.index {
            Some((index, scale)) => {
                self.rex_index(reg, index, at.base);
                self.byte(0x0F);
                self.byte(0xAF);
                self.mem_index(reg, at.base, index, scale, at.disp);
            }
            None => {
                self.rex(reg, at.base);
                self.byte(0x0F);
                self.byte(0xAF);
                self.mem(reg, at.base, at.disp);
            }
        }
    }

    /// `lea dst, [at + extra]`
    fn lea_at(&mut self, dst: u8, at: Mem, extra: i32) {
        match at.index {
            Some((index, scale)) => self.lea_index(dst, at.base, index, scale, at.disp + extra),
            None => self.lea(dst, at.base, at.disp + extra),
        }
    }

    // ---- what is known at this point ------------------------------------

    /// Forgets every index check and what r10 holds: from here on a list
    /// may have another length or buffer, or other code may arrive here.
    fn forget(&mut self) {
        self.known.clear();
        self.held = None;
    }

    /// Forgets what was learnt after position `from` in the code: it is
    /// not known on a path that left before, or it concerns a variable
    /// that no longer exists.
    fn forget_since(&mut self, from: usize) {
        self.known.retain(|known| known.since <= from);
        if self.held.is_some_and(|held| held.since > from) {
            self.held = None;
        }
    }

    /// Register `reg` is about to change: forgets what depended on it.
    /// Every instruction that writes a register reports it here.
    fn writes(&mut self, reg: u8) {
        if reg == R10 {
            self.held = None;
        } else if VARIABLE_REGISTERS.contains(&reg) {
            self.known.retain(|known| known.at != At::Reg(reg));
            if self.held.is_some_and(|held| held.at == Some(At::Reg(reg))) {
                self.held = None;
            }
        }
    }

    /// `cmp reg, [base + disp]`
    fn cmp_mem(&mut self, reg: u8, base: u8, disp: i32) {
        self.rex(reg, base);
        self.byte(0x3B);
        self.mem(reg, base, disp);
    }

    /// `mov dst, [base + disp]`
    fn load_mem(&mut self, dst: u8, base: u8, disp: i32) {
        self.writes(dst);
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
        self.store_imm(base, disp, 0);
    }

    /// `mov qword [base + disp], value`
    fn store_imm(&mut self, base: u8, disp: i32, value: i32) {
        self.rex(0, base);
        self.byte(0xC7);
        self.mem(0, base, disp);
        self.imm32(value);
    }

    /// `lea dst, [base + disp]`
    fn lea(&mut self, dst: u8, base: u8, disp: i32) {
        self.writes(dst);
        self.rex(dst, base);
        self.byte(0x8D);
        self.mem(dst, base, disp);
    }

    /// `mov dst, src`
    fn mov(&mut self, dst: u8, src: u8) {
        self.writes(dst);
        self.rex(src, dst);
        self.byte(0x89);
        self.byte(0xC0 | (src & 7) << 3 | (dst & 7));
    }

    /// `op dst, src` for `ADD`, `SUB`, `CMP`, `TEST`, `AND`, `OR` and `XOR`.
    fn op(&mut self, op: u8, dst: u8, src: u8) {
        if op != CMP && op != TEST {
            self.writes(dst);
        }
        self.rex(src, dst);
        self.byte(op);
        self.byte(0xC0 | (src & 7) << 3 | (dst & 7));
    }

    /// `neg reg`
    fn neg(&mut self, reg: u8) {
        self.writes(reg);
        self.rex(0, reg);
        self.byte(0xF7);
        self.byte(0xD8 | (reg & 7));
    }

    /// `shl reg, cl` or, with `right`, `sar reg, cl`.
    fn shift_cl(&mut self, reg: u8, right: bool) {
        self.writes(reg);
        self.rex(0, reg);
        self.byte(0xD3);
        self.byte(if right { 0xF8 } else { 0xE0 } | (reg & 7));
    }

    /// `shl reg, count` or, with `right`, `sar reg, count`.
    fn shift_imm(&mut self, reg: u8, right: bool, count: u8) {
        self.writes(reg);
        self.rex(0, reg);
        self.byte(0xC1);
        self.byte(if right { 0xF8 } else { 0xE0 } | (reg & 7));
        self.byte(count);
    }

    /// `op reg, value` for `ADD_I`, `OR_I`, `AND_I`, `SUB_I`, `XOR_I` and
    /// `CMP_I`. A constant that fits in a byte takes the short form.
    fn op_imm(&mut self, op: u8, reg: u8, value: i32) {
        if op != CMP_I {
            self.writes(reg);
        }
        self.rex(0, reg);
        if let Ok(small) = i8::try_from(value) {
            self.byte(0x83);
            self.byte(0xC0 | op << 3 | (reg & 7));
            self.byte(small as u8);
        } else {
            self.byte(0x81);
            self.byte(0xC0 | op << 3 | (reg & 7));
            self.imm32(value);
        }
    }

    /// `op qword [base + disp], value`
    fn op_mem_imm(&mut self, op: u8, base: u8, disp: i32, value: i32) {
        self.rex(0, base);
        if let Ok(small) = i8::try_from(value) {
            self.byte(0x83);
            self.mem(op, base, disp);
            self.byte(small as u8);
        } else {
            self.byte(0x81);
            self.mem(op, base, disp);
            self.imm32(value);
        }
    }

    /// `imul dst, src`
    fn imul(&mut self, dst: u8, src: u8) {
        self.writes(dst);
        self.rex(dst, src);
        self.byte(0x0F);
        self.byte(0xAF);
        self.byte(0xC0 | (dst & 7) << 3 | (src & 7));
    }

    /// `imul dst, src, value`
    fn imul_imm(&mut self, dst: u8, src: u8, value: i32) {
        self.writes(dst);
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
        self.writes(reg);
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

    /// `reg = value`, in the shortest form. Zero is made with `xor`,
    /// which changes the flags: no caller needs them afterwards.
    fn mov_imm(&mut self, reg: u8, value: i64) {
        self.writes(reg);
        if value == 0 {
            // xor r32, r32 clears the upper half too.
            if reg >= 8 {
                self.byte(0x45);
            }
            self.byte(0x31);
            self.byte(0xC0 | (reg & 7) << 3 | (reg & 7));
        } else if (0..=u32::MAX as i64).contains(&value) {
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
        // What was learnt after the jump is not known on its path.
        self.forget_since(at);
    }

    fn jump_back(&mut self, to: usize) {
        let at = self.jump();
        self.patch_to(at, to);
    }

    /// Emits a `call` and returns the offset of its field.
    fn call_field(&mut self) -> usize {
        // The called code may change any list.
        self.forget();
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
        // The routine ends the program: nothing is forgotten on the
        // path that goes on.
        self.byte(0xE8);
        self.imm32(0);
        self.runtime_calls.push((self.code.len() as u32 - 4, entry));
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

    /// Expression `id`, with the name of a constant read as its value.
    #[inline(always)]
    fn expr(&self, id: ExprId) -> Expr {
        self.prog.expr(self.ast, self.sh, id)
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
        // The most used variables live in registers. Above the slots is
        // room for the caller's values of those registers.
        self.register_count = info.registers.len().min(VARIABLE_REGISTERS.len());
        for (i, &(slot, _)) in info.registers.iter().take(self.register_count).enumerate() {
            self.registers[i] = (slot, VARIABLE_REGISTERS[i]);
        }
        // A parameter that arrives as an address never changes. Where
        // the function has a loop, such addresses take the registers
        // that are left: a loop reads them over and over.
        // When no register is left, the first address still gets one,
        // from the least used of the variables: what a loop goes
        // through is most often a parameter.
        if info.params.iter().any(|param| param.by_ref) && self.has_loop(body) {
            let last = VARIABLE_REGISTERS.len() - 1;
            let mut first = true;
            for (slot, param) in info.params.iter().enumerate() {
                if !param.by_ref {
                    continue;
                }
                if self.register_count < VARIABLE_REGISTERS.len() {
                    self.registers[self.register_count] = (slot as u32, VARIABLE_REGISTERS[self.register_count]);
                    self.register_count += 1;
                } else if first {
                    self.registers[last] = (slot as u32, VARIABLE_REGISTERS[last]);
                }
                first = false;
            }
        }
        self.saved_slot = info.slots;
        self.frame_bytes = (8 * (info.slots + self.register_count as u32)).div_ceil(16) * 16;
        self.ret = info.ret;
        self.sret_slot = info.sret_slot;
        self.scratch_slot = info.scratch_slot;
        self.live.clear();
        self.forget();

        self.waiting = 0;
        self.waiting_most = 0;
        self.push_reg(RBP);
        self.mov(RBP, RSP);
        // sub rsp, the size of the frame: filled in at the end, when it
        // is known how many operands had to wait in the frame.
        self.byte(0x48);
        self.byte(0x81);
        self.byte(0xEC);
        self.imm32(0);
        let frame_field = self.code.len() - 4;
        for i in 0..self.register_count {
            self.store_slot(self.saved_slot + i as u32, self.registers[i].1);
        }
        // The address for an aggregate result arrives in rax; park it
        // before anything else uses that register.
        if let Some(slot) = self.sret_slot {
            self.store_slot(slot, RAX);
        }
        // Each parameter arrives in a register as a value or a pointer,
        // and moves to its slot or to the register it lives in.
        for slot in 0..self.ast.list(params).len() as u32 {
            match self.slot_register(slot) {
                Some(reg) => self.mov(reg, ARGS[slot as usize]),
                None => self.store_slot(slot, ARGS[slot as usize]),
            }
        }
        // A temporary may be freed without having been filled, when the
        // expression that fills it was skipped. Null makes that harmless.
        for &slot in &info.zero_slots {
            self.zero_slot(slot);
        }
        self.block(body);
        if self.ret == Ty::Unit {
            self.mov_imm(RAX, 0);
            self.function_epilogue();
        } else {
            // The checker proved every path returns; this is unreachable.
            self.byte(0x0F);
            self.byte(0x0B); // ud2
        }
        // Waiting operands lie below the slots, 16 bytes at a time to
        // keep the stack pointer a multiple of 16.
        let frame = self.frame_bytes + self.waiting_most.div_ceil(2) * 16;
        self.code[frame_field..frame_field + 4].copy_from_slice(&frame.to_le_bytes());
    }

    /// Takes the next frame slot for an operand that waits while a call
    /// may be made, and returns where it is. `waiting` is lowered again
    /// by whoever took it.
    fn wait_slot(&mut self) -> Mem {
        self.waiting += 1;
        self.waiting_most = self.waiting_most.max(self.waiting);
        Mem { base: RBP, index: None, disp: -(self.frame_bytes as i32) - 8 * self.waiting as i32 }
    }

    fn epilogue(&mut self) {
        self.byte(0xC9); // leave: mov rsp, rbp; pop rbp
        self.byte(0xC3); // ret
    }

    /// Leaves a function of the program: the registers its variables
    /// lived in get the caller's values back first.
    fn function_epilogue(&mut self) {
        for i in 0..self.register_count {
            self.load(self.registers[i].1, self.saved_slot + i as u32);
        }
        self.epilogue();
    }

    /// True when a `while` or `for` loop is somewhere in `body`.
    fn has_loop(&self, body: List) -> bool {
        self.ast.list(body).iter().any(|&id| self.stmt_has_loop(id))
    }

    fn stmt_has_loop(&self, id: StmtId) -> bool {
        match self.ast.stmts[id as usize] {
            Stmt::While { .. } | Stmt::For { .. } => true,
            Stmt::If { then_block, else_branch, .. } => {
                self.has_loop(then_block)
                    || match else_branch {
                        Else::None => false,
                        Else::Block(body) => self.has_loop(body),
                        Else::If(next) => self.stmt_has_loop(next),
                    }
            }
            Stmt::Match { arms, .. } => {
                self.ast.list(arms).iter().any(|&arm| self.has_loop(self.ast.arms[arm as usize].body))
            }
            _ => false,
        }
    }

    /// The register that holds the address a place is reached through:
    /// the one that address lives in, or `scratch` after loading it.
    fn pointer(&mut self, place: Place, scratch: u8) -> u8 {
        debug_assert!(place.by_ref);
        match self.slot_register(place.slot) {
            Some(reg) => reg,
            None => {
                self.load(scratch, place.slot);
                scratch
            }
        }
    }

    /// Where the three slots of a list or string in a fixed place are:
    /// a register and a displacement. Uses no register but `scratch`.
    fn header(&mut self, place: Place, scratch: u8) -> (u8, i32) {
        if place.by_ref {
            (self.pointer(place, scratch), 8 * place.offset as i32)
        } else {
            (RBP, self.disp(place.slot + place.offset))
        }
    }

    /// The register the variable in frame slot `slot` lives in, if any.
    fn slot_register(&self, slot: u32) -> Option<u8> {
        self.registers[..self.register_count].iter().find(|&&(taken, _)| taken == slot).map(|&(_, reg)| reg)
    }

    /// The register a place lives in: only whole variables have one.
    fn register(&self, place: Place) -> Option<u8> {
        if place.by_ref || place.offset != 0 || self.register_count == 0 {
            return None;
        }
        self.slot_register(place.slot)
    }

    /// The register `id` lives in, when it is a variable that has one.
    /// Such a variable cannot change while an expression is evaluated:
    /// only a statement can assign to it, and nothing has its address.
    fn in_register(&self, id: ExprId) -> Option<u8> {
        if self.register_count == 0 {
            return None;
        }
        match self.expr(id) {
            Expr::Ident(_) => self.place(id).and_then(|place| self.register(place)),
            _ => None,
        }
    }

    /// Generates a block and frees the owning locals declared in it.
    fn block(&mut self, body: List) {
        let outer = self.live.len();
        let start = self.code.len();
        for &id in self.ast.list(body) {
            self.stmt(id);
        }
        self.drop_live_from(outer);
        self.live.truncate(outer);
        // Its variables are gone, and their slots may be used again.
        self.forget_since(start);
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
        self.drop_range(start, len, keep_rax);
    }

    /// Frees the temporaries listed in `drops[start..start + len]`.
    fn drop_range(&mut self, start: u32, len: u32, keep_rax: bool) {
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
        // Storing a number or a `bool` changes no list, and an `if`
        // stores nothing itself: what is known about indices carries
        // over those. Every other statement starts and ends knowing
        // nothing.
        let keeps = match self.ast.stmts[id as usize] {
            Stmt::Let { value, .. } => !self.is_aggregate(value),
            Stmt::Assign { target, .. } => !self.is_aggregate(target),
            Stmt::If { .. } => true,
            _ => false,
        };
        if !keeps {
            self.forget();
        }
        self.stmt_inner(id);
        if !keeps {
            self.forget();
        }
    }

    fn stmt_inner(&mut self, id: StmtId) {
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
                let skip_then = self.jumps.len();
                self.test(id, cond, false);
                self.block(then_block);
                match else_branch {
                    Else::None => self.patch_from(skip_then),
                    Else::Block(body) => {
                        let skip_else = self.jump();
                        self.patch_from(skip_then);
                        self.block(body);
                        self.patch(skip_else);
                    }
                    Else::If(next) => {
                        let skip_else = self.jump();
                        self.patch_from(skip_then);
                        self.stmt(next);
                        self.patch(skip_else);
                    }
                }
            }
            Stmt::While { cond, body } => {
                // The test sits below the body, so that going round
                // takes one jump. A test that leaves nothing to free is
                // written a second time above the loop, to get in; any
                // other is reached by a jump.
                let plain = self.prog.stmt_drops[self.sh.s(id)].1 == 0;
                let exits = self.jumps.len();
                let mut enter = None;
                if plain {
                    self.test(id, cond, false);
                } else {
                    enter = Some(self.jump());
                }
                // The code below is reached again from its own end.
                self.forget();
                let top = self.code.len();
                self.loops.push(LoopFixups { breaks: Vec::new(), continues: Vec::new(), live: self.live.len() });
                self.block(body);
                let fixups = self.loops.pop().expect("loop stack is balanced");
                for at in fixups.continues {
                    self.patch(at);
                }
                if let Some(at) = enter {
                    self.patch(at);
                }
                let again = self.jumps.len();
                self.test(id, cond, true);
                for i in again..self.jumps.len() {
                    self.patch_to(self.jumps[i], top);
                }
                self.jumps.truncate(again);
                self.patch_from(exits);
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
                let live = self.loops.last().expect("checked: inside a loop").live;
                self.drop_live_from(live);
                let at = self.jump();
                self.loops.last_mut().expect("checked: inside a loop").continues.push(at);
            }
            Stmt::Match { scrutinee, arms } => self.match_stmt(id, scrutinee, arms),
        }
    }

    /// Generates the test of an `if` or `while`: code that jumps when
    /// the condition is `when` and runs on otherwise. The jumps are
    /// added to `jumps` for the caller to patch.
    fn test(&mut self, id: StmtId, cond: ExprId, when: bool) {
        if self.prog.stmt_drops[self.sh.s(id)].1 == 0 {
            return self.branch(cond, when);
        }
        // The temporaries are freed before either way is taken.
        self.value(cond);
        self.drop_temps(id, true);
        self.op(TEST, RAX, RAX);
        let at = self.jump_if(if when { CC_NE } else { CC_E });
        self.jumps.push(at);
    }

    /// Generates code that jumps when `cond` is `when` and runs on
    /// otherwise, without making a `bool` first where that can be
    /// avoided: a comparison jumps on its flags, and `&&`, `||` and `!`
    /// become jumps themselves. The parts are evaluated exactly as far
    /// and in the same order as for the value.
    fn branch(&mut self, cond: ExprId, when: bool) {
        match self.expr(cond) {
            Expr::Bool(value) => {
                if value == when {
                    let at = self.jump();
                    self.jumps.push(at);
                }
            }
            Expr::Unary { op: UnOp::Not, operand } => self.branch(operand, !when),
            Expr::Binary { op: op @ (BinOp::And | BinOp::Or), lhs, rhs } => {
                // `&&` is false as soon as one side is, `||` true.
                let decides = op == BinOp::Or;
                if when == decides {
                    self.branch(lhs, when);
                    self.branch(rhs, when);
                } else {
                    // The left side alone can settle it the other way:
                    // then the right side is skipped.
                    let skip = self.jumps.len();
                    self.branch(lhs, decides);
                    let rest = self.jumps.len();
                    self.branch(rhs, when);
                    // The left side's jumps land here, past the right
                    // side; the right side's stay for the caller.
                    for i in skip..rest {
                        self.patch(self.jumps[i]);
                    }
                    self.jumps.drain(skip..rest);
                }
            }
            Expr::Binary { op, lhs, rhs } if comparison(op).is_some() && matches!(self.ty(lhs), Ty::I64 | Ty::Bool) => {
                let cc = comparison(op).expect("just checked");
                self.compare_flags(lhs, rhs);
                let at = self.jump_if(if when { cc } else { cc ^ 1 });
                self.jumps.push(at);
            }
            _ => {
                if let Some(reg) = self.in_register(cond) {
                    self.op(TEST, reg, reg);
                } else if self.in_memory(cond) {
                    // A `bool` in memory is tested where it is.
                    let at = self.memory(cond);
                    self.cmp_at_imm(at, 0);
                } else {
                    self.value(cond);
                    self.op(TEST, RAX, RAX);
                }
                let at = self.jump_if(if when { CC_NE } else { CC_E });
                self.jumps.push(at);
            }
        }
    }

    /// Completes the jumps added to `jumps` since it had `from` entries
    /// to come here.
    fn patch_from(&mut self, from: usize) {
        for i in from..self.jumps.len() {
            self.patch(self.jumps[i]);
        }
        self.jumps.truncate(from);
    }

    fn assign(&mut self, id: StmtId, target: ExprId, value: ExprId) {
        if self.prog.stmt_flag[self.sh.s(id)] == APPEND {
            // `s = s + more`: the new text is added to `s` in place. It
            // is computed first; only then is `s` looked at.
            let Expr::Binary { rhs, .. } = self.expr(value) else {
                unreachable!("checked: an append adds something");
            };
            self.address(rhs);
            match self.place(target) {
                // A variable is found without disturbing anything.
                Some(place) => {
                    self.mov(RSI, RAX);
                    self.place_address_in(place, RDI);
                }
                None => {
                    self.push();
                    self.address(target);
                    self.mov(RDI, RAX);
                    self.pop(RSI);
                }
            }
            self.runtime(runtime::STR_APPEND);
            return;
        }
        let ty = self.ty(target);
        let place = self.place(target);
        if !ty.is_aggregate() {
            if let Some(reg) = place.and_then(|place| self.register(place)) {
                self.value_in(value, reg);
                return;
            }
            if place.is_none() {
                if let Some((base, index, offset)) = self.element_path(target) {
                    return self.assign_element(base, index, offset, value);
                }
            }
            let source = self.source(value);
            match place {
                Some(place) if place.by_ref => {
                    let pointer = self.pointer(place, R8);
                    self.store_mem(pointer, 8 * place.offset as i32, source);
                }
                Some(place) => self.store_slot(place.slot + place.offset, source),
                None => {
                    if source != RAX {
                        self.mov(RAX, source);
                    }
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
                let pointer = self.pointer(place, R8);
                if pointer != R8 {
                    self.mov(R8, pointer);
                }
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

    /// Stores a number or `bool` in `base[index]`, or in the part of
    /// that element `offset` bytes in. The value is computed first, then
    /// the index.
    fn assign_element(&mut self, base: ExprId, index: ExprId, offset: i32, value: ExprId) {
        // A constant goes into the store instruction itself.
        let constant = match self.expr(value) {
            Expr::Bool(value) => Some(value as i32),
            _ => self.small_constant(value),
        };
        // A variable in a register is stored from there.
        let register = self.in_register(value);
        let waits = constant.is_none() && register.is_none() && !self.index_keeps_rax(base, index);
        if constant.is_none() && register.is_none() {
            self.value(value);
            if waits {
                self.push();
            }
        }
        let at = self.element(base, index);
        match constant {
            Some(constant) => self.store_imm_at(at, offset, constant),
            None => {
                if waits {
                    self.pop(RAX);
                }
                self.store_at(at, offset, register.unwrap_or(RAX));
            }
        }
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
                    } else if let Expr::Ident(_) = self.expr(value) {
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
        self.function_epilogue();
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

        // The test is written twice, above the loop to get in and below
        // the body to go round: one jump for each round. Both leave the
        // position in rcx and say where the list's three slots are.
        let (header, disp) = self.for_test(in_place, iter, own_list, position);
        let exit = self.jump_if(CC_AE);
        self.forget();
        let top = self.code.len();
        if has_index {
            self.store_slot(index_slot, RCX);
        }
        self.load_mem(R8, header, disp); // buffer
        let (at, scale) = self.scaled(RCX, elem_size);
        for i in 0..elem_size {
            self.load_index(R9, R8, at, scale, 8 * i as i32);
            self.store_slot(item_slot + i, R9);
        }

        self.loops.push(LoopFixups { breaks: Vec::new(), continues: Vec::new(), live: self.live.len() });
        self.block(body);
        let fixups = self.loops.pop().expect("loop stack is balanced");
        for at in fixups.continues {
            self.patch(at);
        }
        let step = self.disp(position);
        self.op_mem_imm(ADD_I, RBP, step, 1);
        let again = self.for_test(in_place, iter, own_list, position);
        debug_assert!(again == (header, disp));
        let round = self.jump_if(CC_B);
        self.patch_to(round, top);
        self.patch(exit);
        for at in fixups.breaks {
            self.patch(at);
        }
        if !in_place {
            self.live.pop();
            self.drop_frame(own_list, list_ty);
        }
    }

    /// Compares the position of a `for` loop, which it leaves in rcx,
    /// with the length of the list, and returns where the list's three
    /// slots are: a register and a displacement.
    fn for_test(&mut self, in_place: bool, iter: ExprId, own_list: u32, position: u32) -> (u8, i32) {
        let (header, disp) = if !in_place {
            (RBP, self.disp(own_list))
        } else if let Some(place) = self.place(iter) {
            self.header(place, RAX)
        } else {
            self.address(iter);
            (RAX, 0)
        };
        self.load(RCX, position);
        self.cmp_mem(RCX, header, disp + 8);
        (header, disp)
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
        } else if let Some(reg) = self.slot_register(slot) {
            self.value_in(value, reg);
        } else {
            let source = self.source(value);
            self.store_slot(slot, source);
        }
    }

    /// Evaluates a number or `bool` that is about to be stored and
    /// returns the register it is in: the one the variable lives in, or
    /// rax.
    fn source(&mut self, value: ExprId) -> u8 {
        match self.in_register(value) {
            Some(reg) => reg,
            None => {
                self.value(value);
                RAX
            }
        }
    }

    /// The frame slot of `id` when it is a whole local variable.
    fn variable_slot(&self, id: ExprId) -> Option<u32> {
        match self.expr(id) {
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

    /// Prepares the address of element `index` of a buffer whose
    /// elements take `elem_size` slots: it is the buffer plus the
    /// returned register shifted left by the returned count. One slot
    /// needs no instruction at all; other sizes are worked out in rcx,
    /// the only register this changes.
    fn scaled(&mut self, index: u8, elem_size: u32) -> (u8, u8) {
        match elem_size {
            1 => return (index, 3),
            2 => self.lea_index(RCX, index, index, 0, 0),
            3 => self.lea_index(RCX, index, index, 1, 0),
            4 => self.lea_scaled(RCX, index, 2),
            5 => self.lea_index(RCX, index, index, 2, 0),
            8 => self.lea_scaled(RCX, index, 3),
            9 => self.lea_index(RCX, index, index, 3, 0),
            _ => self.imul_imm(RCX, index, elem_size as i32),
        }
        (RCX, 3)
    }

    /// When `id` is an element of a list or a part of one, however deep:
    /// the list, the index, and how many bytes into the element the
    /// part starts.
    fn element_path(&self, id: ExprId) -> Option<(ExprId, ExprId, i32)> {
        let mut at = id;
        let mut offset = 0;
        loop {
            match self.expr(at) {
                Expr::Field { base, .. } => {
                    offset += self.prog.expr_res[self.sh.e(at)];
                    at = base;
                }
                Expr::Index { base, index } if matches!(self.ty(base), Ty::List(_)) => {
                    return Some((base, index, 8 * offset as i32));
                }
                _ => return None,
            }
        }
    }

    /// How many slots an element of the list `base` takes.
    fn element_slots(&self, base: ExprId) -> u32 {
        let Ty::List(list) = self.ty(base) else {
            unreachable!("checked: only lists have elements");
        };
        self.prog.size_of(self.prog.lists[list as usize])
    }

    /// Evaluates the index of `base[index]` and checks it against the
    /// length of the list or string, then leaves the buffer in r8 and
    /// returns the register that holds the index: the one it lives in,
    /// or rcx. The index is evaluated first, so that nothing runs
    /// between finding the buffer and using it.
    fn checked_index(&mut self, base: ExprId, index: ExprId) -> u8 {
        let place = self.place(base);
        let at = match self.in_register(index) {
            Some(reg) => reg,
            None => {
                if place.is_some() && self.is_simple(index) {
                    self.load_simple(index, RCX);
                } else {
                    self.value(index);
                    if place.is_some() {
                        self.mov(RCX, RAX);
                    } else {
                        self.push();
                    }
                }
                RCX
            }
        };
        let (header, disp) = match place {
            Some(place) => self.header(place, R8),
            None => {
                self.address(base);
                if at == RCX {
                    self.pop(RCX);
                }
                (RAX, 0)
            }
        };
        self.cmp_mem(at, header, disp + 8); // length
        // An unsigned compare also rejects negatives.
        self.runtime_unless(CC_B, runtime::INDEX_FAIL);
        self.load_mem(R8, header, disp); // buffer
        at
    }

    /// Checks the index of `base[index]` and returns where that element
    /// of the list is. Uses rcx, r8 and r10 besides what the index and
    /// the list take to evaluate.
    fn element(&mut self, base: ExprId, index: ExprId) -> Mem {
        self.indexed(base, index, self.element_slots(base), 3)
    }

    /// The index of `base[index]` as it can be remembered, if it can.
    /// Constants are kept small enough for any offset made from them to
    /// fit in an instruction; a negative one is left to fail its check.
    fn index_key(&self, index: ExprId, slots: u32) -> Option<At> {
        if let Some(reg) = self.in_register(index) {
            return Some(At::Reg(reg));
        }
        match self.small_constant(index) {
            Some(value) if (0..1 << 20).contains(&value) && slots < 256 => Some(At::Constant(value)),
            _ => None,
        }
    }

    /// Checks the index of `base[index]` against the length and returns
    /// where the element is. An element takes `slots` slots; one of a
    /// single slot is `1 << shift` bytes (a list's number, a string's
    /// byte).
    ///
    /// When the list or string is in a fixed place and the index is a
    /// variable in a register or a small constant, a check made before
    /// on every path here is not made again, and r10 is reused when it
    /// still holds the buffer or the element's address.
    fn indexed(&mut self, base: ExprId, index: ExprId, slots: u32, shift: u8) -> Mem {
        let tracked = self.place(base).and_then(|place| self.index_key(index, slots).map(|at| (place, at)));
        let Some((place, at)) = tracked else {
            let reg = self.checked_index(base, index);
            let (reg, shift) = if slots == 1 { (reg, shift) } else { self.scaled(reg, slots) };
            return Mem { base: R8, index: Some((reg, shift)), disp: 0 };
        };
        // An element of several slots is used through its address; of
        // any other only the buffer is kept, which serves every index.
        let whole = slots != 1 && matches!(at, At::Reg(_));
        let wanted = whole.then_some(at);
        let known = self.known.iter().any(|known| known.place == place && known.at == at);
        let held = self.held.is_some_and(|held| held.place == place && held.at == wanted);
        if !known || !held {
            let (header, disp) = self.header(place, R8);
            if !known {
                // An unsigned compare also rejects negatives.
                match at {
                    At::Reg(reg) => {
                        self.cmp_mem(reg, header, disp + 8);
                        self.runtime_unless(CC_B, runtime::INDEX_FAIL);
                    }
                    At::Constant(value) => {
                        self.op_mem_imm(CMP_I, header, disp + 8, value);
                        self.runtime_unless(CC_A, runtime::INDEX_FAIL);
                    }
                }
                if self.known.len() == KNOWN {
                    self.known.remove(0);
                }
                self.known.push(Known { place, at, since: self.code.len() });
            }
            if !held {
                match at {
                    At::Reg(reg) if whole => {
                        self.load_mem(R8, header, disp);
                        let (reg, shift) = self.scaled(reg, slots);
                        self.lea_index(R10, R8, reg, shift, 0);
                    }
                    _ => self.load_mem(R10, header, disp),
                }
                self.held = Some(Held { place, at: wanted, since: self.code.len() });
            }
        }
        match at {
            At::Reg(_) if whole => Mem { base: R10, index: None, disp: 0 },
            At::Reg(reg) => Mem { base: R10, index: Some((reg, shift)), disp: 0 },
            At::Constant(value) => Mem { base: R10, index: None, disp: value * ((slots as i32) << shift) },
        }
    }

    /// True when the number or `bool` `id` sits in memory where an
    /// instruction can use it directly: a variable in the frame or
    /// behind an address, or a list element whose index takes no work.
    fn in_memory(&self, id: ExprId) -> bool {
        match self.expr(id) {
            Expr::Ident(_) | Expr::Field { .. } | Expr::Index { .. } if !self.is_aggregate(id) => match self.place(id) {
                Some(place) => self.register(place).is_none(),
                None => self.element_path(id).is_some_and(|(base, index, _)| self.index_keeps_rax(base, index)),
            },
            _ => false,
        }
    }

    /// Where a value that is `in_memory` sits. For a list element this
    /// checks the index. Changes no register but rcx, r8 and r10.
    fn memory(&mut self, id: ExprId) -> Mem {
        match self.place(id) {
            Some(place) => {
                let (base, disp) = self.header(place, R8);
                Mem { base, index: None, disp }
            }
            None => {
                let (base, index, offset) = self.element_path(id).expect("checked by `in_memory`");
                let mut at = self.element(base, index);
                at.disp += offset;
                at
            }
        }
    }

    /// True when `id` is a list element whose index is the variable in
    /// register `reg`.
    fn indexed_by(&self, id: ExprId, reg: u8) -> bool {
        self.element_path(id).is_some_and(|(_, index, _)| self.in_register(index) == Some(reg))
    }

    /// True when `indexed` leaves rax alone for these two.
    fn index_keeps_rax(&self, base: ExprId, index: ExprId) -> bool {
        self.place(base).is_some() && (self.in_register(index).is_some() || self.is_simple(index))
    }

    /// The location of a variable or of a field path inside a variable,
    /// when it is fixed at compile time. `None` for computed values and
    /// for anything reached through a list index.
    fn place(&self, id: ExprId) -> Option<Place> {
        match self.expr(id) {
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
        self.place_address_in(place, RAX);
    }

    /// Leaves the address of a place in `reg`, using no other register.
    fn place_address_in(&mut self, place: Place, reg: u8) {
        if place.by_ref {
            let pointer = self.pointer(place, reg);
            if place.offset > 0 {
                self.lea(reg, pointer, 8 * place.offset as i32);
            } else if pointer != reg {
                self.mov(reg, pointer);
            }
        } else {
            self.frame_address(reg, place.slot + place.offset);
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
        if let Some((base, index, offset)) = self.element_path(id) {
            let at = self.element(base, index);
            self.lea_at(RAX, at, offset);
            return;
        }
        match self.expr(id) {
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
            Expr::Try(inner) => self.try_expr(id, inner),
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
            // An element of a list was handled above.
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
        match self.expr(id) {
            Expr::Int(_) | Expr::Bool(_) => true,
            Expr::Ident(_) | Expr::Field { .. } => !self.is_aggregate(id) && self.place(id).is_some(),
            _ => false,
        }
    }

    /// Loads a simple value into `reg`, using no other register.
    fn load_simple(&mut self, id: ExprId, reg: u8) {
        match self.expr(id) {
            Expr::Int(_) => self.mov_imm(reg, self.prog.ints[self.prog.expr_res[self.sh.e(id)] as usize]),
            Expr::Bool(value) => self.mov_imm(reg, value as i64),
            _ => {
                let place = self.place(id).expect("a simple value that is not a constant is a place");
                if place.by_ref {
                    // The pointer passes through the same register.
                    let pointer = self.pointer(place, reg);
                    self.load_mem(reg, pointer, 8 * place.offset as i32);
                } else if let Some(home) = self.register(place) {
                    if home != reg {
                        self.mov(reg, home);
                    }
                } else {
                    self.load(reg, place.slot + place.offset);
                }
            }
        }
    }

    /// The constant on the right of an operation when it fits in the
    /// instruction itself.
    fn small_constant(&self, id: ExprId) -> Option<i32> {
        match self.expr(id) {
            Expr::Int(_) => i32::try_from(self.prog.ints[self.prog.expr_res[self.sh.e(id)] as usize]).ok(),
            _ => None,
        }
    }

    /// The value of `id` when it is a number written in the program.
    fn literal(&self, id: ExprId) -> Option<i64> {
        match self.expr(id) {
            Expr::Int(_) => Some(self.prog.ints[self.prog.expr_res[self.sh.e(id)] as usize]),
            _ => None,
        }
    }

    /// Evaluates both operands of an operation on numbers, left one
    /// first, and says where they ended up: the register holding the left
    /// one, and the right one. A variable that lives in a register is
    /// used where it is. `constants` allows a small constant on the right
    /// to stay inside the instruction, and a value in memory to stay
    /// there. Besides variable registers, only
    /// rax, rcx and the `PARK` registers are ever returned.
    fn operands(&mut self, lhs: ExprId, rhs: ExprId, constants: bool) -> (u8, Right) {
        let left = self.in_register(lhs);
        if constants {
            if let Some(value) = self.small_constant(rhs) {
                let left = left.unwrap_or_else(|| {
                    self.value(lhs);
                    RAX
                });
                return (left, Right::Constant(value));
            }
            if self.in_memory(rhs) {
                // Finding it leaves rax and the waiting operands alone.
                let left = left.unwrap_or_else(|| {
                    self.value(lhs);
                    RAX
                });
                return (left, Right::Mem(self.memory(rhs)));
            }
        }
        match (left, self.in_register(rhs)) {
            (Some(left), Some(right)) => (left, Right::Reg(right)),
            (Some(left), None) => {
                self.value(rhs);
                (left, Right::Reg(RAX))
            }
            (None, Some(right)) => {
                self.value(lhs);
                (RAX, Right::Reg(right))
            }
            (None, None) => {
                if !self.is_simple(rhs) && self.parked < PARK.len() && self.fits(rhs, PARK.len() - self.parked - 1) {
                    // The right one needs no call: the left one waits
                    // in a register of its own, where it is computed.
                    let park = PARK[self.parked];
                    self.value_in(lhs, park);
                    self.parked += 1;
                    self.value(rhs);
                    self.parked -= 1;
                    return (park, Right::Reg(RAX));
                }
                self.value(lhs);
                if self.is_simple(rhs) {
                    // Nothing runs in between, so the left one can wait in rcx.
                    self.mov(RCX, RAX);
                    self.load_simple(rhs, RAX);
                } else {
                    // The right one may call: the left one waits in
                    // the frame.
                    let at = self.wait_slot();
                    self.store_at(at, 0, RAX);
                    self.value(rhs);
                    self.waiting -= 1;
                    if constants {
                        return (RAX, Right::Waiting(at));
                    }
                    self.load_at(RCX, at, 0);
                }
                (RCX, Right::Reg(RAX))
            }
        }
    }

    /// True when `value` computes the number or `bool` `id` with no
    /// call and nothing on the stack, in rax, rcx, rdx, r8, r10 and at most
    /// `free` of the `PARK` registers. Whatever is not known to be that
    /// simple is refused.
    fn fits(&self, id: ExprId, free: usize) -> bool {
        match self.expr(id) {
            Expr::Int(_) | Expr::Bool(_) => true,
            Expr::Ident(_) => self.is_simple(id),
            Expr::Index { base, index } if self.ty(base) == Ty::Str => {
                self.place(base).is_some() && self.fits(index, free)
            }
            Expr::Field { .. } | Expr::Index { .. } => {
                self.is_simple(id)
                    || self.element_path(id).is_some_and(|(base, index, _)| {
                        !self.is_aggregate(id) && self.place(base).is_some() && self.fits(index, free)
                    })
            }
            Expr::Unary { operand, .. } => self.fits(operand, free),
            Expr::Binary { lhs, rhs, .. } => {
                matches!(self.ty(lhs), Ty::I64 | Ty::Bool) && free > 0 && self.fits(lhs, free) && self.fits(rhs, free - 1)
            }
            Expr::Call { callee, .. } if self.prog.expr_res[self.sh.e(id)] == BUILTIN_LEN => match self.expr(callee) {
                Expr::Field { base, .. } => self.place(base).is_some(),
                _ => false,
            },
            _ => false,
        }
    }

    /// Compares two numbers or `bool`s and leaves the result in the flags.
    fn compare_flags(&mut self, lhs: ExprId, rhs: ExprId) {
        // A variable in a register is compared with a length, or with a
        // variable in the frame, where that sits in memory.
        if let Some(left) = self.in_register(lhs) {
            match self.expr(rhs) {
                Expr::Call { callee, .. } if self.prog.expr_res[self.sh.e(rhs)] == BUILTIN_LEN => {
                    if let Expr::Field { base, .. } = self.expr(callee) {
                        if let Some(place) = self.place(base) {
                            let (header, disp) = self.header(place, RAX);
                            return self.cmp_mem(left, header, disp + 8);
                        }
                    }
                }
                _ => {}
            }
        } else if self.in_memory(lhs) {
            // A value in memory is compared where it is.
            if let Some(value) = self.small_constant(rhs) {
                let at = self.memory(lhs);
                return self.cmp_at_imm(at, value);
            }
            if let Some(right) = self.in_register(rhs) {
                let at = self.memory(lhs);
                return self.cmp_at_reg(at, right);
            }
        }
        match self.operands(lhs, rhs, true) {
            // Against zero, `test` sets the same flags in fewer bytes.
            (left, Right::Constant(0)) => self.op(TEST, left, left),
            (left, Right::Constant(value)) => self.op_imm(CMP_I, left, value),
            (left, Right::Reg(right)) => self.op(CMP, left, right),
            (left, Right::Mem(at)) => self.op_at(CMP, left, at),
            (_, Right::Waiting(at)) => self.cmp_at_reg(at, RAX),
        }
    }

    /// `inner?`. When `inner` is an `Ok` or a `Some`, what it holds is the
    /// value: a number or `bool` in rax, anything else in this
    /// expression's temporary with its address in rax. Otherwise the
    /// function returns the `Err` or the `None` here and now, after
    /// freeing what a `return` at this point would free.
    fn try_expr(&mut self, id: ExprId, inner: ExprId) {
        let Ty::Enum(index) = self.ty(inner) else {
            unreachable!("checked: `?` follows a `Result` or an `Option`");
        };
        let Ty::Enum(ret_index) = self.ret else {
            unreachable!("checked: the function returns a `Result` or an `Option`");
        };
        let prog = self.prog;
        let (value_ty, value_at) = prog.enums[index as usize].variants[0].fields[0];
        // A temporary gives away what it holds and is left empty (tag
        // 0); what a variable holds is copied.
        let from_temp = prog.expr_aux[self.sh.e(id)] & 1 == 1;
        let (start, len) = (prog.expr_res[self.sh.e(id)], prog.expr_aux[self.sh.e(id)] >> 1);
        let inner_tmp = prog.expr_tmp[self.sh.e(inner)];
        self.address(inner);
        self.op_mem_imm(CMP_I, RAX, 0, 1); // the tag of `Ok` and `Some`
        let ok = self.jump_if(CC_E);

        let ret_slot = self.sret_slot.expect("checked: this function returns an enum");
        self.load(R8, ret_slot);
        if let Some(&(error_ty, error_at)) = prog.enums[index as usize].variants[1].fields.first() {
            let error_to = prog.enums[ret_index as usize].variants[1].fields[0].1;
            for i in 0..prog.size_of(error_ty) {
                self.load_mem(RCX, RAX, ((error_at + i) * 8) as i32);
                self.store_mem(R8, ((error_to + i) * 8) as i32, RCX);
            }
            self.store_imm(R8, 0, 2);
            if prog.needs_drop(error_ty) {
                if from_temp {
                    self.zero_slot(inner_tmp);
                } else {
                    self.lea(RDI, R8, (error_to * 8) as i32);
                    self.clone_value(error_ty);
                }
            }
        } else {
            self.store_imm(R8, 0, 2);
        }
        self.drop_range(start, len, false);
        for i in (0..self.live.len()).rev() {
            let (slot, ty) = self.live[i];
            self.drop_frame(slot, ty);
        }
        self.mov_imm(RAX, 0);
        // Leaving also takes off what the unfinished statement parked
        // on the stack.
        self.function_epilogue();

        self.patch(ok);
        if !value_ty.is_aggregate() {
            self.load_mem(RAX, RAX, (value_at * 8) as i32);
            return;
        }
        let tmp = prog.expr_tmp[self.sh.e(id)];
        self.lea(RAX, RAX, (value_at * 8) as i32);
        self.copy_to_frame(tmp, prog.size_of(value_ty));
        if prog.needs_drop(value_ty) {
            if from_temp {
                self.zero_slot(inner_tmp);
            } else {
                self.clone_frame(tmp, value_ty);
            }
        }
        self.frame_address(RAX, tmp);
    }

    /// Generates code that leaves a scalar value in rax.
    fn value(&mut self, id: ExprId) {
        match self.expr(id) {
            Expr::Try(inner) => self.try_expr(id, inner),
            Expr::Int(_) => self.mov_imm(RAX, self.prog.ints[self.prog.expr_res[self.sh.e(id)] as usize]),
            Expr::Bool(value) => self.mov_imm(RAX, value as i64),
            Expr::Index { base, index } if self.ty(base) == Ty::Str => {
                // A byte of a string, checked like a list index.
                let at = self.indexed(base, index, 1, 0);
                self.load_byte(at);
            }
            Expr::Ident(_) | Expr::Field { .. } | Expr::Index { .. } => match self.place(id) {
                Some(place) if place.by_ref => {
                    let pointer = self.pointer(place, RAX);
                    self.load_mem(RAX, pointer, 8 * place.offset as i32);
                }
                Some(place) => match self.register(place) {
                    Some(reg) => self.mov(RAX, reg),
                    None => self.load(RAX, place.slot + place.offset),
                },
                None => match self.element_path(id) {
                    Some((base, index, offset)) => {
                        // A list element or a part of one, read where it is.
                        let at = self.element(base, index);
                        self.load_at(RAX, at, offset);
                    }
                    None => {
                        // A field of a computed value.
                        self.address(id);
                        self.load_mem(RAX, RAX, 0);
                    }
                },
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
            Expr::Binary { op, lhs, rhs } if matches!(self.ty(lhs), Ty::Enum(_)) => {
                // `==` and `!=` on enums without values compare the tags.
                // The left tag is read before the right side is computed.
                self.address(lhs);
                self.load_mem(RAX, RAX, 0);
                self.push();
                self.address(rhs);
                self.load_mem(RAX, RAX, 0);
                self.pop(RCX);
                self.op(CMP, RCX, RAX);
                // setcc al; movzx rax, al
                self.byte(0x0F);
                self.byte(0x90 | if op == BinOp::Eq { CC_E } else { CC_NE });
                self.byte(0xC0);
                self.byte(0x48);
                self.byte(0x0F);
                self.byte(0xB6);
                self.byte(0xC0);
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
            Expr::Binary { op, lhs, rhs } => self.binary(op, lhs, rhs, RAX),
            Expr::Call { args, .. } => self.call(id, args),
            Expr::MutArg(_) | Expr::Float(_) | Expr::Str(_) | Expr::StructLit { .. } | Expr::ArrayLit { .. } => {
                unreachable!("rejected by the checker or not a scalar")
            }
        }
    }

    /// Generates an operation on two numbers or `bool`s, with the result
    /// in `dest`: rax, or the register of the variable it is stored in.
    fn binary(&mut self, op: BinOp, lhs: ExprId, rhs: ExprId, dest: u8) {
        if let Some(cc) = comparison(op) {
            self.compare_flags(lhs, rhs);
            // setcc al; movzx eax, al
            self.byte(0x0F);
            self.byte(0x90 | cc);
            self.byte(0xC0);
            self.byte(0x0F);
            self.byte(0xB6);
            self.byte(0xC0);
            if dest != RAX {
                self.mov(dest, RAX);
            }
            return;
        }
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                let (mut lhs, mut rhs) = (lhs, rhs);
                if let (Some(constant), None) = (self.small_constant(lhs), self.small_constant(rhs)) {
                    if op == BinOp::Sub {
                        // constant - right = -right + constant
                        self.value_in(rhs, dest);
                        self.neg(dest);
                        if constant != 0 {
                            self.op_imm(ADD_I, dest, constant);
                        }
                        return;
                    }
                    // The other operations give the same either way
                    // round, and a constant has nothing to evaluate.
                    (lhs, rhs) = (rhs, lhs);
                }
                // When the right side is at hand and has nothing to do
                // with the result register, the left side is computed
                // there directly. rax, rcx, r8 and r10 are excluded:
                // finding the right side in memory may use them.
                let at_hand = self.small_constant(rhs).is_some()
                    || self.in_register(rhs).is_some_and(|reg| reg != dest)
                    || (self.in_memory(rhs) && !self.indexed_by(rhs, dest));
                if at_hand && self.in_register(lhs).is_none() && ![RAX, RCX, R8, R10].contains(&dest) {
                    self.value_in(lhs, dest);
                    let right = match (self.small_constant(rhs), self.in_register(rhs)) {
                        (Some(value), _) => Right::Constant(value),
                        (None, Some(reg)) => Right::Reg(reg),
                        (None, None) => Right::Mem(self.memory(rhs)),
                    };
                    return self.combine(op, dest, dest, right);
                }
                let (left, right) = self.operands(lhs, rhs, true);
                self.combine(op, dest, left, right);
            }
            BinOp::Shl | BinOp::Shr => {
                let right = op == BinOp::Shr;
                // The processor uses the low 6 bits of the count.
                if let Some(count) = self.literal(rhs) {
                    let left = self.in_register(lhs).unwrap_or_else(|| {
                        self.value(lhs);
                        RAX
                    });
                    if left != dest {
                        self.mov(dest, left);
                    }
                    self.shift_imm(dest, right, (count & 63) as u8);
                    return;
                }
                let (mut left, count) = self.operands(lhs, rhs, false);
                let Right::Reg(count) = count else {
                    unreachable!("constants were not allowed");
                };
                // The count belongs in cl.
                if left == RCX {
                    // Then the count is in rax: swap the two.
                    self.byte(0x48);
                    self.byte(0x91); // xchg rax, rcx
                    left = RAX;
                } else if count != RCX {
                    self.mov(RCX, count);
                }
                if left != dest {
                    self.mov(dest, left);
                }
                self.shift_cl(dest, right);
            }
            BinOp::Div | BinOp::Rem => {
                let divisor = self.literal(rhs);
                if let Some(by) = divisor.filter(|&by| by != 0 && by != 1 && by != -1 && by != i64::MIN) {
                    return self.divide_by_constant(op, lhs, by, dest);
                }
                let (left, right) = self.operands(lhs, rhs, false);
                let Right::Reg(right) = right else {
                    unreachable!("constants were not allowed");
                };
                // The dividend belongs in rax; the divisor goes to rcx,
                // out of the way of rax and rdx.
                if right == RAX {
                    if left == RCX {
                        self.byte(0x48);
                        self.byte(0x91); // xchg rax, rcx
                    } else {
                        self.mov(RCX, RAX);
                        self.mov(RAX, left);
                    }
                } else {
                    self.mov(RCX, right);
                    if left != RAX {
                        self.mov(RAX, left);
                    }
                }
                // Dividing by a constant other than zero needs no check.
                if divisor.is_none_or(|value| value == 0) {
                    self.op(TEST, RCX, RCX);
                    self.runtime_unless(CC_NE, runtime::DIV_ZERO);
                }
                // The processor refuses to divide the smallest number by
                // -1. Dividing by -1 is negating, with remainder 0, and
                // negating wraps around like every other operation. A
                // constant divisor other than -1 cannot hit that case.
                let mut done = None;
                if divisor.is_none_or(|value| value == -1) {
                    self.op_imm(CMP_I, RCX, -1);
                    let ordinary = self.jump_if(CC_NE);
                    if op == BinOp::Div {
                        self.neg(RAX);
                    } else {
                        self.mov_imm(RAX, 0);
                    }
                    if op == BinOp::Rem {
                        // The ordinary path leaves the result in rdx.
                        self.mov(RDX, RAX);
                    }
                    done = Some(self.jump());
                    self.patch(ordinary);
                }
                self.byte(0x48);
                self.byte(0x99); // cqo: sign-extend rax into rdx
                self.byte(0x48);
                self.byte(0xF7);
                self.byte(0xF9); // idiv rcx
                if let Some(done) = done {
                    self.patch(done);
                }
                let result = if op == BinOp::Rem { RDX } else { RAX };
                if result != dest {
                    self.mov(dest, result);
                }
            }
            _ => unreachable!("comparisons are handled above, `&&` and `||` by the caller"),
        }
    }

    /// `dest = left op right` for `+ - * & | ^`, with the operands where
    /// `operands` left them.
    fn combine(&mut self, op: BinOp, dest: u8, left: u8, right: Right) {
        match right {
            Right::Constant(value) => {
                if op == BinOp::Mul {
                    // Small factors are one address calculation.
                    match value {
                        2 => self.lea_index(dest, left, left, 0, 0),
                        3 => self.lea_index(dest, left, left, 1, 0),
                        4 => self.lea_scaled(dest, left, 2),
                        5 => self.lea_index(dest, left, left, 2, 0),
                        8 => self.lea_scaled(dest, left, 3),
                        9 => self.lea_index(dest, left, left, 3, 0),
                        _ => self.imul_imm(dest, left, value),
                    }
                    return;
                }
                if left != dest {
                    // Adding into another register is one address
                    // calculation.
                    match op {
                        BinOp::Add => return self.lea(dest, left, value),
                        BinOp::Sub if value != i32::MIN => return self.lea(dest, left, -value),
                        _ => self.mov(dest, left),
                    }
                }
                let code = match op {
                    BinOp::Add => ADD_I,
                    BinOp::Sub => SUB_I,
                    BinOp::BitAnd => AND_I,
                    BinOp::BitOr => OR_I,
                    _ => XOR_I,
                };
                self.op_imm(code, dest, value);
            }
            Right::Reg(right) if right == dest && left != dest => {
                // The right operand already sits in the result
                // register: combine the left one into it.
                match op {
                    BinOp::Sub => {
                        // left - right = -right + left
                        self.neg(dest);
                        self.op(ADD, dest, left);
                    }
                    BinOp::Mul => self.imul(dest, left),
                    _ => self.op(Self::opcode(op), dest, left),
                }
            }
            Right::Reg(right) => {
                if left != dest {
                    if op == BinOp::Add {
                        return self.lea_index(dest, left, right, 0, 0);
                    }
                    self.mov(dest, left);
                }
                match op {
                    BinOp::Mul => self.imul(dest, right),
                    _ => self.op(Self::opcode(op), dest, right),
                }
            }
            Right::Mem(at) => {
                // The address must not be made with the register the
                // left operand is about to be moved to: then the work
                // is done in rax, which no address uses.
                let clash = left != dest && (at.base == dest || at.index.is_some_and(|(index, _)| index == dest));
                let target = if clash { RAX } else { dest };
                if left != target {
                    self.mov(target, left);
                }
                match op {
                    BinOp::Mul => self.imul_at(target, at),
                    _ => self.op_at(Self::opcode(op), target, at),
                }
                if target != dest {
                    self.mov(dest, target);
                }
            }
            Right::Waiting(at) => {
                // rax holds the right operand, memory the left one.
                match op {
                    BinOp::Sub => {
                        // left - right = -right + left
                        self.neg(RAX);
                        self.op_at(ADD, RAX, at);
                    }
                    BinOp::Mul => self.imul_at(RAX, at),
                    _ => self.op_at(Self::opcode(op), RAX, at),
                }
                if dest != RAX {
                    self.mov(dest, RAX);
                }
            }
        }
    }

    /// Divides by a constant without the processor's slow division: the
    /// quotient is the high half of a multiplication by a "magic" number,
    /// shifted and corrected by one for negative results (the method in
    /// Hacker's Delight, chapter 10). The remainder follows from it.
    fn divide_by_constant(&mut self, op: BinOp, lhs: ExprId, by: i64, dest: u8) {
        let left = self.in_register(lhs).unwrap_or_else(|| {
            self.value(lhs);
            RAX
        });
        let (magic, shift) = magic_for(by);
        // rax and rdx take the product. The dividend is kept: in the
        // register its variable lives in, or else in rcx.
        let dividend = if left == RAX {
            self.mov(RCX, RAX);
            RCX
        } else {
            self.mov(RAX, left);
            left
        };
        self.mov_imm(R8, magic);
        self.byte(0x49);
        self.byte(0xF7);
        self.byte(0xE8); // imul r8: rdx:rax = rax * r8
        if by > 0 && magic < 0 {
            self.op(ADD, RDX, dividend);
        } else if by < 0 && magic > 0 {
            self.op(SUB, RDX, dividend);
        }
        if shift > 0 {
            self.shift_imm(RDX, true, shift as u8);
        }
        // Add one when the result is negative: its sign bit, as a number.
        self.mov(RAX, RDX);
        self.byte(0x48);
        self.byte(0xC1);
        self.byte(0xE8);
        self.byte(63); // shr rax, 63
        self.op(ADD, RDX, RAX);
        if op == BinOp::Div {
            if dest != RDX {
                self.mov(dest, RDX);
            }
            return;
        }
        // remainder = dividend - quotient * divisor
        match i32::try_from(by) {
            Ok(small) => self.imul_imm(RDX, RDX, small),
            Err(_) => {
                self.mov_imm(R8, by);
                self.imul(RDX, R8);
            }
        }
        if dest == RDX {
            // dividend - product = -product + dividend
            self.neg(RDX);
            self.op(ADD, RDX, dividend);
        } else if dividend == RCX {
            self.op(SUB, RCX, RDX);
            if dest != RCX {
                self.mov(dest, RCX);
            }
        } else {
            // The variable itself must stay as it is, unless it is
            // the one being assigned.
            if dest != dividend {
                self.mov(dest, dividend);
            }
            self.op(SUB, dest, RDX);
        }
    }

    /// The two-operand instruction for an operator that has one.
    fn opcode(op: BinOp) -> u8 {
        match op {
            BinOp::Add => ADD,
            BinOp::Sub => SUB,
            BinOp::BitAnd => AND,
            BinOp::BitOr => OR,
            BinOp::BitXor => XOR,
            _ => unreachable!("only these have a plain two-operand form"),
        }
    }

    /// Generates code that leaves a number or `bool` in register `dest`:
    /// a register a variable lives in, or the one an argument travels
    /// in. Arithmetic and simple values are computed there directly.
    fn value_in(&mut self, id: ExprId, dest: u8) {
        if dest == RAX {
            return self.value(id);
        }
        match self.expr(id) {
            _ if self.is_simple(id) => self.load_simple(id, dest),
            // A list element is read straight into the register, unless
            // finding the element needs that register.
            _ if ![RCX, R8, R10].contains(&dest) && self.in_memory(id) => {
                let at = self.memory(id);
                self.load_at(dest, at, 0);
            }
            // A shift by a variable needs rcx for the count.
            Expr::Binary { op: BinOp::Shl | BinOp::Shr, .. } if dest == RCX => {
                self.value(id);
                self.mov(dest, RAX);
            }
            Expr::Binary { op, lhs, rhs }
                if !matches!(op, BinOp::And | BinOp::Or) && matches!(self.ty(lhs), Ty::I64 | Ty::Bool) =>
            {
                self.binary(op, lhs, rhs, dest)
            }
            _ => {
                self.value(id);
                self.mov(dest, RAX);
            }
        }
    }

    /// True when an argument can be put in its register directly: a
    /// simple value, or the address of a variable or a part of one.
    fn is_simple_arg(&self, arg: ExprId, by_ref: bool) -> bool {
        match self.expr(arg) {
            Expr::MutArg(inner) => self.place(inner).is_some(),
            _ if by_ref => self.place(arg).is_some(),
            _ => self.is_simple(arg),
        }
    }

    fn load_simple_arg(&mut self, arg: ExprId, by_ref: bool, reg: u8) {
        match self.expr(arg) {
            Expr::MutArg(inner) => {
                let place = self.place(inner).expect("checked by `is_simple_arg`");
                self.place_address_in(place, reg);
            }
            _ if by_ref => {
                let place = self.place(arg).expect("checked by `is_simple_arg`");
                self.place_address_in(place, reg);
            }
            _ => self.load_simple(arg, reg),
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
        let Expr::Call { callee, .. } = self.expr(id) else {
            unreachable!("a call is a call");
        };
        // In `value.method(...)` the value is the receiver.
        let receiver = match self.expr(callee) {
            Expr::Field { base, .. } => Some(base),
            _ => None,
        };
        if let BUILTIN_LEN | BUILTIN_LIST_PUSH | BUILTIN_LIST_POP | BUILTIN_STR_SLICE = target {
            let base = receiver.expect("checked: a method call names its receiver");
            return self.list_method(id, target, base, arg_ids);
        }
        // A method the standard library writes in azoL is a function
        // whose first argument is the receiver. Arguments are evaluated
        // in the order written. Those up to the last one that needs real
        // work wait on the stack; the simple ones after it go straight
        // into their registers.
        let all = || receiver.into_iter().chain(arg_ids.iter().copied()).enumerate();
        let mut count = 0;
        let mut worked = 0;
        for (i, arg) in all() {
            if !self.is_simple_arg(arg, self.arg_by_ref(target, i)) {
                worked = i + 1;
            }
            count = i + 1;
        }
        for (i, arg) in all() {
            let by_ref = self.arg_by_ref(target, i);
            if i >= worked {
                self.load_simple_arg(arg, by_ref, ARGS[i]);
                continue;
            }
            let last = i + 1 == worked;
            match self.expr(arg) {
                Expr::MutArg(inner) => self.address(inner),
                _ if by_ref => self.address(arg),
                // The last of them is computed where it travels.
                _ if last => self.value_in(arg, ARGS[i]),
                _ => self.value(arg),
            }
            if !last {
                self.push();
            } else {
                // An address is still in rax: move it into place. Then
                // bring the earlier ones back.
                if by_ref || matches!(self.expr(arg), Expr::MutArg(_)) {
                    self.mov(ARGS[i], RAX);
                }
                for earlier in (0..i).rev() {
                    self.pop(ARGS[earlier]);
                }
            }
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
            let (header, disp) = match self.place(base) {
                Some(place) => self.header(place, RAX),
                None => {
                    self.address(base);
                    (RAX, 0)
                }
            };
            self.load_mem(RAX, header, disp + 8);
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
                }
                // A number or `bool` goes straight into a list that has
                // room; the runtime is only called to make room.
                let mut done = None;
                if !elem.is_aggregate() {
                    if let Some(place) = self.place(base) {
                        let (header, disp) = self.header(place, R10);
                        self.load_mem(R8, header, disp + 8); // length
                        self.cmp_mem(R8, header, disp + 16); // capacity
                        let full = self.jump_if(CC_AE);
                        self.load_mem(R9, header, disp); // buffer
                        self.store_index(R9, R8, 3, 0, RAX);
                        self.op_mem_imm(ADD_I, header, disp + 8, 1);
                        done = Some(self.jump());
                        self.patch(full);
                    }
                    self.store_slot(tmp, RAX);
                }
                self.address(base);
                self.mov(RDI, RAX);
                self.frame_address(RSI, tmp);
                self.mov_imm(RDX, elem_size as i64 * 8);
                self.runtime(runtime::LIST_PUSH);
                if let Some(done) = done {
                    self.patch(done);
                }
                // The list is longer now and may have moved.
                self.forget();
            }
            _ => {
                self.address(base);
                self.load_mem(R8, RAX, 8); // length
                self.op(TEST, R8, R8);
                self.runtime_unless(CC_NE, runtime::POP_EMPTY);
                self.op_imm(SUB_I, R8, 1);
                self.store_mem(RAX, 8, R8);
                // The list is shorter now.
                self.forget();
                self.load_mem(R9, RAX, 0); // buffer
                let (at, scale) = self.scaled(R8, elem_size);
                if elem.is_aggregate() {
                    self.lea_index(RAX, R9, at, scale, 0);
                    self.copy_to_frame(self.prog.expr_tmp[self.sh.e(id)], elem_size);
                } else {
                    self.load_index(RAX, R9, at, scale, 0);
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
        let program = sema::check_with_prelude(&ast, &src, prelude_start as u32).map_err(|mut errors| errors.remove(0).message)?;
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

    /// A generator with no program behind it, to emit single
    /// instructions with.
    fn bare(ast: &Ast) -> Gen<'_> {
        Gen {
            ast,
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
            registers: [(0, 0); VARIABLE_REGISTERS.len()],
            register_count: 0,
            parked: 0,
            waiting: 0,
            waiting_most: 0,
            jumps: Vec::new(),
            known: Vec::new(),
            held: None,
            saved_slot: 0,
            sh: Shift::default(),
        }
    }

    fn check(g: &mut Gen, expected: &[u8], what: &str) {
        assert_eq!(g.code, expected, "{what}");
        g.code.clear();
    }

    /// The addressing forms with an index register, operations that
    /// read or compare memory directly, the short forms for small
    /// constants and `xor` for zero. Expected bytes were produced by
    /// assembling the instructions named with the GNU assembler.
    #[test]
    fn leaner_forms_match_the_reference_assembler() {
        let ast = Ast::default();
        let mut g = bare(&ast);
        g.load_index(RAX, R8, RCX, 3, 0);
        check(&mut g, &[0x49, 0x8B, 0x04, 0xC8], "mov rax, [r8+rcx*8]");
        g.load_index(RAX, R10, RBX, 3, 16);
        check(&mut g, &[0x49, 0x8B, 0x44, 0xDA, 0x10], "mov rax, [r10+rbx*8+16]");
        g.load_index(R9, R8, R12, 3, 24);
        check(&mut g, &[0x4F, 0x8B, 0x4C, 0xE0, 0x18], "mov r9, [r8+r12*8+24]");
        g.load_index(R14, R10, R13, 3, 0);
        check(&mut g, &[0x4F, 0x8B, 0x34, 0xEA], "mov r14, [r10+r13*8]");
        g.load_index(RAX, R13, RCX, 0, 0);
        check(&mut g, &[0x49, 0x8B, 0x44, 0x0D, 0x00], "mov rax, [r13+rcx]");
        g.load_index(RDX, RBP, RAX, 2, -4096);
        check(&mut g, &[0x48, 0x8B, 0x94, 0x85, 0x00, 0xF0, 0xFF, 0xFF], "mov rdx, [rbp+rax*4-4096]");
        g.store_index(R8, RCX, 3, 0, RAX);
        check(&mut g, &[0x49, 0x89, 0x04, 0xC8], "mov [r8+rcx*8], rax");
        g.store_index(R10, R12, 3, 8, R13);
        check(&mut g, &[0x4F, 0x89, 0x6C, 0xE2, 0x08], "mov [r10+r12*8+8], r13");
        g.store_index(R9, R8, 3, 0, RAX);
        check(&mut g, &[0x4B, 0x89, 0x04, 0xC1], "mov [r9+r8*8], rax");
        g.store_index_imm(R10, RBX, 3, 0, 1);
        check(&mut g, &[0x49, 0xC7, 0x04, 0xDA, 0x01, 0x00, 0x00, 0x00], "mov qword ptr [r10+rbx*8], 1");
        g.store_index_imm(R8, RCX, 3, 16, -7);
        check(&mut g, &[0x49, 0xC7, 0x44, 0xC8, 0x10, 0xF9, 0xFF, 0xFF, 0xFF], "mov qword ptr [r8+rcx*8+16], -7");
        g.store_imm(R10, 24, 5);
        check(&mut g, &[0x49, 0xC7, 0x42, 0x18, 0x05, 0x00, 0x00, 0x00], "mov qword ptr [r10+24], 5");
        g.store_imm(RBP, -72, 100000);
        check(&mut g, &[0x48, 0xC7, 0x45, 0xB8, 0xA0, 0x86, 0x01, 0x00], "mov qword ptr [rbp-72], 100000");
        g.lea_index(RAX, R8, RCX, 3, 0);
        check(&mut g, &[0x49, 0x8D, 0x04, 0xC8], "lea rax, [r8+rcx*8]");
        g.lea_index(R10, R8, RCX, 3, 0);
        check(&mut g, &[0x4D, 0x8D, 0x14, 0xC8], "lea r10, [r8+rcx*8]");
        g.lea_index(RCX, RBX, RBX, 0, 0);
        check(&mut g, &[0x48, 0x8D, 0x0C, 0x1B], "lea rcx, [rbx+rbx]");
        g.lea_index(RCX, R13, R13, 1, 0);
        check(&mut g, &[0x4B, 0x8D, 0x4C, 0x6D, 0x00], "lea rcx, [r13+r13*2]");
        g.lea_index(RCX, R12, R12, 2, 0);
        check(&mut g, &[0x4B, 0x8D, 0x0C, 0xA4], "lea rcx, [r12+r12*4]");
        g.lea_index(RAX, RAX, RAX, 3, 0);
        check(&mut g, &[0x48, 0x8D, 0x04, 0xC0], "lea rax, [rax+rax*8]");
        g.lea_index(R12, R9, RAX, 0, 0);
        check(&mut g, &[0x4D, 0x8D, 0x24, 0x01], "lea r12, [r9+rax]");
        g.lea_index(RDI, RBX, R12, 0, 0);
        check(&mut g, &[0x4A, 0x8D, 0x3C, 0x23], "lea rdi, [rbx+r12]");
        g.lea_index(RAX, R8, RCX, 3, 40);
        check(&mut g, &[0x49, 0x8D, 0x44, 0xC8, 0x28], "lea rax, [r8+rcx*8+40]");
        g.lea_scaled(RCX, RBX, 2);
        check(&mut g, &[0x48, 0x8D, 0x0C, 0x9D, 0x00, 0x00, 0x00, 0x00], "lea rcx, [rbx*4]");
        g.lea_scaled(RCX, R12, 3);
        check(&mut g, &[0x4A, 0x8D, 0x0C, 0xE5, 0x00, 0x00, 0x00, 0x00], "lea rcx, [r12*8]");
        g.lea_scaled(R13, RAX, 2);
        check(&mut g, &[0x4C, 0x8D, 0x2C, 0x85, 0x00, 0x00, 0x00, 0x00], "lea r13, [rax*4]");
        g.lea_scaled(RCX, RCX, 3);
        check(&mut g, &[0x48, 0x8D, 0x0C, 0xCD, 0x00, 0x00, 0x00, 0x00], "lea rcx, [rcx*8]");
        g.lea(RDI, RBX, -1);
        check(&mut g, &[0x48, 0x8D, 0x7B, 0xFF], "lea rdi, [rbx-1]");
        g.lea(R12, R12, 0);
        check(&mut g, &[0x4D, 0x8D, 0x24, 0x24], "lea r12, [r12]");
        g.lea(RAX, R13, 2);
        check(&mut g, &[0x49, 0x8D, 0x45, 0x02], "lea rax, [r13+2]");
        g.load_byte(Mem { base: R8, index: Some((RCX, 0)), disp: 0 });
        check(&mut g, &[0x41, 0x0F, 0xB6, 0x04, 0x08], "movzx eax, byte ptr [r8+rcx]");
        g.load_byte(Mem { base: R10, index: Some((RBX, 0)), disp: 0 });
        check(&mut g, &[0x41, 0x0F, 0xB6, 0x04, 0x1A], "movzx eax, byte ptr [r10+rbx]");
        g.load_byte(Mem { base: R10, index: Some((R12, 0)), disp: 0 });
        check(&mut g, &[0x43, 0x0F, 0xB6, 0x04, 0x22], "movzx eax, byte ptr [r10+r12]");
        g.load_byte(Mem { base: R10, index: None, disp: 3 });
        check(&mut g, &[0x41, 0x0F, 0xB6, 0x42, 0x03], "movzx eax, byte ptr [r10+3]");
        g.cmp_mem(RCX, RAX, 8);
        check(&mut g, &[0x48, 0x3B, 0x48, 0x08], "cmp rcx, [rax+8]");
        g.cmp_mem(RBX, R14, 8);
        check(&mut g, &[0x49, 0x3B, 0x5E, 0x08], "cmp rbx, [r14+8]");
        g.cmp_mem(R12, RBP, -112);
        check(&mut g, &[0x4C, 0x3B, 0x65, 0x90], "cmp r12, [rbp-112]");
        g.cmp_mem(R8, RBP, -104);
        check(&mut g, &[0x4C, 0x3B, 0x45, 0x98], "cmp r8, [rbp-104]");
        g.cmp_mem(R13, R12, 8);
        check(&mut g, &[0x4D, 0x3B, 0x6C, 0x24, 0x08], "cmp r13, [r12+8]");
        g.op_at(ADD, R12, Mem { base: R10, index: None, disp: 16 });
        check(&mut g, &[0x4D, 0x03, 0x62, 0x10], "add r12, [r10+16]");
        g.op_at(SUB, RAX, Mem { base: RBP, index: None, disp: -40 });
        check(&mut g, &[0x48, 0x2B, 0x45, 0xD8], "sub rax, [rbp-40]");
        g.op_at(AND, RBX, Mem { base: R10, index: Some((R12, 3)), disp: 0 });
        check(&mut g, &[0x4B, 0x23, 0x1C, 0xE2], "and rbx, [r10+r12*8]");
        g.op_at(OR, RAX, Mem { base: R8, index: Some((RCX, 3)), disp: 8 });
        check(&mut g, &[0x49, 0x0B, 0x44, 0xC8, 0x08], "or rax, [r8+rcx*8+8]");
        g.op_at(XOR, R9, Mem { base: R15, index: None, disp: 8 });
        check(&mut g, &[0x4D, 0x33, 0x4F, 0x08], "xor r9, [r15+8]");
        g.op_at(CMP, R13, Mem { base: RBP, index: None, disp: -96 });
        check(&mut g, &[0x4C, 0x3B, 0x6D, 0xA0], "cmp r13, [rbp-96]");
        g.op_at(ADD, RAX, Mem { base: RBP, index: None, disp: -136 });
        check(&mut g, &[0x48, 0x03, 0x85, 0x78, 0xFF, 0xFF, 0xFF], "add rax, [rbp-136]");
        g.cmp_at_reg(Mem { base: R10, index: Some((RBX, 3)), disp: 0 }, R13);
        check(&mut g, &[0x4D, 0x39, 0x2C, 0xDA], "cmp [r10+rbx*8], r13");
        g.cmp_at_reg(Mem { base: RBP, index: None, disp: -24 }, RAX);
        check(&mut g, &[0x48, 0x39, 0x45, 0xE8], "cmp [rbp-24], rax");
        g.cmp_at_imm(Mem { base: R10, index: Some((R12, 3)), disp: 0 }, 0);
        check(&mut g, &[0x4B, 0x83, 0x3C, 0xE2, 0x00], "cmp qword ptr [r10+r12*8], 0");
        g.cmp_at_imm(Mem { base: RBP, index: None, disp: -24 }, 100000);
        check(&mut g, &[0x48, 0x81, 0x7D, 0xE8, 0xA0, 0x86, 0x01, 0x00], "cmp qword ptr [rbp-24], 100000");
        g.cmp_at_imm(Mem { base: R10, index: None, disp: 8 }, -3);
        check(&mut g, &[0x49, 0x83, 0x7A, 0x08, 0xFD], "cmp qword ptr [r10+8], -3");
        g.imul_at(RAX, Mem { base: RBP, index: None, disp: -40 });
        check(&mut g, &[0x48, 0x0F, 0xAF, 0x45, 0xD8], "imul rax, [rbp-40]");
        g.imul_at(R13, Mem { base: R10, index: Some((RBX, 3)), disp: 24 });
        check(&mut g, &[0x4D, 0x0F, 0xAF, 0x6C, 0xDA, 0x18], "imul r13, [r10+rbx*8+24]");
        g.op_imm(ADD_I, RBX, 1);
        check(&mut g, &[0x48, 0x83, 0xC3, 0x01], "add rbx, 1");
        g.op_imm(SUB_I, R12, 1);
        check(&mut g, &[0x49, 0x83, 0xEC, 0x01], "sub r12, 1");
        g.op_imm(CMP_I, RBX, 2);
        check(&mut g, &[0x48, 0x83, 0xFB, 0x02], "cmp rbx, 2");
        g.op_imm(CMP_I, R12, -128);
        check(&mut g, &[0x49, 0x83, 0xFC, 0x80], "cmp r12, -128");
        g.op_imm(CMP_I, R12, 127);
        check(&mut g, &[0x49, 0x83, 0xFC, 0x7F], "cmp r12, 127");
        g.op_imm(CMP_I, R12, 128);
        check(&mut g, &[0x49, 0x81, 0xFC, 0x80, 0x00, 0x00, 0x00], "cmp r12, 128");
        g.op_imm(AND_I, R12, 255);
        check(&mut g, &[0x49, 0x81, 0xE4, 0xFF, 0x00, 0x00, 0x00], "and r12, 255");
        g.op_imm(XOR_I, RAX, 1);
        check(&mut g, &[0x48, 0x83, 0xF0, 0x01], "xor rax, 1");
        g.op_imm(OR_I, RAX, 16);
        check(&mut g, &[0x48, 0x83, 0xC8, 0x10], "or rax, 16");
        g.op_imm(SUB_I, RSP, 4096);
        check(&mut g, &[0x48, 0x81, 0xEC, 0x00, 0x10, 0x00, 0x00], "sub rsp, 4096");
        g.op_imm(CMP_I, RCX, -1);
        check(&mut g, &[0x48, 0x83, 0xF9, 0xFF], "cmp rcx, -1");
        g.op_mem_imm(ADD_I, RBP, -16, 1);
        check(&mut g, &[0x48, 0x83, 0x45, 0xF0, 0x01], "add qword ptr [rbp-16], 1");
        g.op_mem_imm(ADD_I, R14, 8, 1);
        check(&mut g, &[0x49, 0x83, 0x46, 0x08, 0x01], "add qword ptr [r14+8], 1");
        g.op_mem_imm(CMP_I, RBP, -112, 3);
        check(&mut g, &[0x48, 0x83, 0x7D, 0x90, 0x03], "cmp qword ptr [rbp-112], 3");
        g.op_mem_imm(CMP_I, R12, 8, 1000);
        check(&mut g, &[0x49, 0x81, 0x7C, 0x24, 0x08, 0xE8, 0x03, 0x00, 0x00], "cmp qword ptr [r12+8], 1000");
        g.op_mem_imm(SUB_I, RBP, -24, 300);
        check(&mut g, &[0x48, 0x81, 0x6D, 0xE8, 0x2C, 0x01, 0x00, 0x00], "sub qword ptr [rbp-24], 300");
        g.mov_imm(RAX, 0);
        check(&mut g, &[0x31, 0xC0], "xor eax, eax");
        g.mov_imm(RBX, 0);
        check(&mut g, &[0x31, 0xDB], "xor ebx, ebx");
        g.mov_imm(R12, 0);
        check(&mut g, &[0x45, 0x31, 0xE4], "xor r12d, r12d");
        g.mov_imm(R8, 0);
        check(&mut g, &[0x45, 0x31, 0xC0], "xor r8d, r8d");
        g.mov_imm(RDI, 0);
        check(&mut g, &[0x31, 0xFF], "xor edi, edi");
        g.op(TEST, R12, R12);
        check(&mut g, &[0x4D, 0x85, 0xE4], "test r12, r12");
        g.op(TEST, RAX, RAX);
        check(&mut g, &[0x48, 0x85, 0xC0], "test rax, rax");
    }

    #[test]
    fn encodings_match_the_reference_assembler() {
        // Expected bytes were produced by assembling the same
        // instructions with the GNU assembler.
        let ast = Ast::default();
        let mut g = bare(&ast);
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
        check(&mut g, &[0x49, 0x83, 0xF8, 0x03], "cmp r8, 3");
        g.op_imm(XOR_I, RAX, 1);
        check(&mut g, &[0x48, 0x83, 0xF0, 0x01], "xor rax, 1");
        g.op_imm(CMP_I, RCX, -1);
        check(&mut g, &[0x48, 0x83, 0xF9, 0xFF], "cmp rcx, -1");
        g.op_mem_imm(ADD_I, RBP, -16, 24);
        check(&mut g, &[0x48, 0x83, 0x45, 0xF0, 0x18], "add qword [rbp-16], 24");
        g.op_mem_imm(SUB_I, RBP, -24, 1);
        check(&mut g, &[0x48, 0x83, 0x6D, 0xE8, 0x01], "sub qword [rbp-24], 1");
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
        check(&mut g, &[0x31, 0xC0], "xor eax, eax");
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

        // The registers variables live in.
        g.mov(RBX, RDI);
        check(&mut g, &[0x48, 0x89, 0xFB], "mov rbx, rdi");
        g.mov(R12, RSI);
        check(&mut g, &[0x49, 0x89, 0xF4], "mov r12, rsi");
        g.mov(RAX, R12);
        check(&mut g, &[0x4C, 0x89, 0xE0], "mov rax, r12");
        g.mov(R13, RAX);
        check(&mut g, &[0x49, 0x89, 0xC5], "mov r13, rax");
        g.mov(RCX, R14);
        check(&mut g, &[0x4C, 0x89, 0xF1], "mov rcx, r14");
        g.mov(RDX, RAX);
        check(&mut g, &[0x48, 0x89, 0xC2], "mov rdx, rax");
        g.load_mem(RBX, RBP, -8);
        check(&mut g, &[0x48, 0x8B, 0x5D, 0xF8], "mov rbx, [rbp-8]");
        g.store_mem(RBP, -16, R15);
        check(&mut g, &[0x4C, 0x89, 0x7D, 0xF0], "mov [rbp-16], r15");
        g.load_mem(R12, RBP, -24);
        check(&mut g, &[0x4C, 0x8B, 0x65, 0xE8], "mov r12, [rbp-24]");
        g.load_mem(RDI, RDI, 8);
        check(&mut g, &[0x48, 0x8B, 0x7F, 0x08], "mov rdi, [rdi+8]");
        g.lea(RSI, RSI, 16);
        check(&mut g, &[0x48, 0x8D, 0x76, 0x10], "lea rsi, [rsi+16]");
        g.op_imm(ADD_I, RBX, 1);
        check(&mut g, &[0x48, 0x83, 0xC3, 0x01], "add rbx, 1");
        g.op_imm(AND_I, R12, 255);
        check(&mut g, &[0x49, 0x81, 0xE4, 0xFF, 0x00, 0x00, 0x00], "and r12, 255");
        g.op_imm(OR_I, RAX, 16);
        check(&mut g, &[0x48, 0x83, 0xC8, 0x10], "or rax, 16");
        g.op_imm(SUB_I, R15, 7);
        check(&mut g, &[0x49, 0x83, 0xEF, 0x07], "sub r15, 7");
        g.op_imm(XOR_I, R14, 1);
        check(&mut g, &[0x49, 0x83, 0xF6, 0x01], "xor r14, 1");
        g.op_imm(CMP_I, RBX, 100);
        check(&mut g, &[0x48, 0x83, 0xFB, 0x64], "cmp rbx, 100");
        g.op(CMP, R12, R13);
        check(&mut g, &[0x4D, 0x39, 0xEC], "cmp r12, r13");
        g.op(ADD, RAX, RBX);
        check(&mut g, &[0x48, 0x01, 0xD8], "add rax, rbx");
        g.op(ADD, R12, RAX);
        check(&mut g, &[0x49, 0x01, 0xC4], "add r12, rax");
        g.op(SUB, RBX, R12);
        check(&mut g, &[0x4C, 0x29, 0xE3], "sub rbx, r12");
        g.neg(RAX);
        check(&mut g, &[0x48, 0xF7, 0xD8], "neg rax");
        g.neg(R12);
        check(&mut g, &[0x49, 0xF7, 0xDC], "neg r12");
        g.shift_cl(RBX, false);
        check(&mut g, &[0x48, 0xD3, 0xE3], "shl rbx, cl");
        g.shift_cl(R12, true);
        check(&mut g, &[0x49, 0xD3, 0xFC], "sar r12, cl");
        g.shift_cl(RAX, false);
        check(&mut g, &[0x48, 0xD3, 0xE0], "shl rax, cl");
        g.shift_imm(RAX, false, 3);
        check(&mut g, &[0x48, 0xC1, 0xE0, 3], "shl rax, 3");
        g.shift_imm(R13, true, 63);
        check(&mut g, &[0x49, 0xC1, 0xFD, 63], "sar r13, 63");
        g.imul(RBX, R12);
        check(&mut g, &[0x49, 0x0F, 0xAF, 0xDC], "imul rbx, r12");
        g.imul_imm(RAX, RBX, 10);
        check(&mut g, &[0x48, 0x69, 0xC3, 10, 0, 0, 0], "imul rax, rbx, 10");
        g.imul_imm(R12, R12, 3);
        check(&mut g, &[0x4D, 0x69, 0xE4, 3, 0, 0, 0], "imul r12, r12, 3");
        g.mov_imm(RBX, 5);
        check(&mut g, &[0xBB, 5, 0, 0, 0], "mov ebx, 5");
        g.mov_imm(R12, 0);
        check(&mut g, &[0x45, 0x31, 0xE4], "xor r12d, r12d");
        g.op(ADD, RDX, RCX);
        check(&mut g, &[0x48, 0x01, 0xCA], "add rdx, rcx");
        g.op(SUB, RCX, RDX);
        check(&mut g, &[0x48, 0x29, 0xD1], "sub rcx, rdx");
        g.shift_imm(RDX, true, 2);
        check(&mut g, &[0x48, 0xC1, 0xFA, 2], "sar rdx, 2");
        g.imul_imm(RDX, RDX, 7);
        check(&mut g, &[0x48, 0x69, 0xD2, 7, 0, 0, 0], "imul rdx, rdx, 7");
        g.imul(RDX, R8);
        check(&mut g, &[0x49, 0x0F, 0xAF, 0xD0], "imul rdx, r8");
    }

    /// What the generated instructions compute, step for step.
    fn divide_like_the_generated_code(n: i64, by: i64) -> (i64, i64) {
        let (magic, shift) = magic_for(by);
        let mut high = ((n as i128 * magic as i128) >> 64) as i64;
        if by > 0 && magic < 0 {
            high = high.wrapping_add(n);
        } else if by < 0 && magic > 0 {
            high = high.wrapping_sub(n);
        }
        high >>= shift;
        let quotient = high.wrapping_add(((high as u64) >> 63) as i64);
        (quotient, n.wrapping_sub(quotient.wrapping_mul(by)))
    }

    #[test]
    fn division_by_a_constant_matches_real_division() {
        let mut divisors: Vec<i64> = vec![2, 3, 5, 7, 10, 11, 16, 60, 100, 255, 256, 1000, 4096, 65537, 1_000_000_007];
        divisors.extend([i64::MAX, i64::MAX - 1, 1 << 62, (1 << 62) + 1, 3_000_000_000, 0x1234_5678_9ABC_DEF1]);
        divisors.extend(2..=130);
        let negatives: Vec<i64> = divisors.iter().map(|&by| -by).collect();
        divisors.extend(negatives);
        let mut dividends: Vec<i64> = vec![0, 1, -1, 2, -2, 6, 7, 8, -7, -8, 99, 100, 101, i64::MAX, i64::MIN, i64::MAX - 1, i64::MIN + 1];
        // A simple generator of scattered 64-bit values.
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..4000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            dividends.push(state as i64);
            dividends.push((state >> 40) as i64);
            dividends.push(-((state >> 52) as i64));
        }
        for &by in &divisors {
            // Values around multiples of the divisor are where an
            // off-by-one would show.
            let mut near = dividends.clone();
            for k in [1i64, 2, 3, 1000, -1, -2, -1000] {
                let multiple = by.wrapping_mul(k);
                near.extend([multiple.wrapping_sub(1), multiple, multiple.wrapping_add(1)]);
            }
            for &n in &near {
                assert_eq!(
                    divide_like_the_generated_code(n, by),
                    (n.wrapping_div(by), n.wrapping_rem(by)),
                    "{n} divided by {by}"
                );
            }
        }
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
