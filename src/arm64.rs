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
//! result, x9-x11 are scratch within one step. The most used numbers
//! and `bool`s of a function live in x19-x28, which survive calls: every
//! function saves the ones it uses on entry and restores them on exit.
//! Registers left over go to the function's `for` loops and, in a
//! function with a loop, to the addresses that arrived as parameters.
//! While the right side of an operation, or the later arguments of a
//! call, call nothing, the value worked out before them waits in x13,
//! x14 or x15 instead of on the stack.
//!
//! Inside straight-line code an index is checked against the length of
//! its list once: the generator remembers which checks have been made
//! and what x12 holds (the buffer of a list or the address of one of
//! its elements), and forgets it wherever the list could have changed
//! or another path joins. See `Known` and `Held`.

use crate::ast::*;
use crate::runtime::{self, RuntimeReloc, RUNTIME, RUNTIME_DATA, RUNTIME_DATA_OFFSETS, RUNTIME_RELOCS};
use crate::sema::{
    idle_slot, Program, Shift, Ty, APPEND, BUILTIN_ARGS, BUILTIN_CHR, BUILTIN_EPRINT, BUILTIN_EXIT, BUILTIN_LEN, BUILTIN_LIST_POP,
    BUILTIN_LIST_PUSH, BUILTIN_PRINT_BOOL, BUILTIN_PRINT_I64, BUILTIN_PRINT_STR, BUILTIN_READ_FILE,
    BUILTIN_CLOCK, BUILTIN_STR_IS_UTF8, BUILTIN_REMOVE_FILE, BUILTIN_RENAME_FILE, BUILTIN_SET_EXECUTABLE, BUILTIN_STR_FROM_BOOL, BUILTIN_STR_FROM_I64, BUILTIN_STR_PUSH, BUILTIN_STR_SET,
    BUILTIN_STR_SLICE, BUILTIN_VARIANT,
    BUILTIN_WRITE_FILE, BY_REF,
    COPIED, FIRST_BUILTIN, FIRST_REGISTER, IN_PLACE, PAT_BIND, PAT_LITERAL, PAT_VARIANT, REGISTERS, VARIANT,
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
/// Holds the buffer of a list or the address of one of its elements
/// between the steps of straight-line code; see `Held`.
const X12: u32 = 12;
/// The first of the registers where a value waits while code that calls
/// nothing works out the next one: x13, x14, x15.
const X13: u32 = 13;
/// How many of them there are.
const PARKS: u32 = 3;
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
/// Unsigned lower.
const LO: u32 = 3;
const HI: u32 = 8;
const LS: u32 = 9;

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

/// Largest slot offset the first of a pair of loads or stores can carry.
const MAX_PAIR: u32 = 63;

/// `ldp rt, rt2, [rn, #8 * slots]`
fn ldp(rt: u32, rt2: u32, rn: u32, slots: u32) -> u32 {
    0xA940_0000 | slots << 15 | rt2 << 10 | rn << 5 | rt
}

/// `stp rt, rt2, [rn, #8 * slots]`
fn stp(rt: u32, rt2: u32, rn: u32, slots: u32) -> u32 {
    0xA900_0000 | slots << 15 | rt2 << 10 | rn << 5 | rt
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

/// `add rd, rn, rm, lsl #shift`
fn add_shifted(rd: u32, rn: u32, rm: u32, shift: u32) -> u32 {
    0x8B00_0000 | rm << 16 | shift << 10 | rn << 5 | rd
}

/// `ldrb rt, [rn, rm]`: one byte, the rest of the register cleared.
fn ldrb_index(rt: u32, rn: u32, rm: u32) -> u32 {
    0x3860_6800 | rm << 16 | rn << 5 | rt
}

/// `ldrb rt, [rn, #bytes]`
fn ldrb(rt: u32, rn: u32, bytes: u32) -> u32 {
    0x3940_0000 | bytes << 10 | rn << 5 | rt
}

/// `neg rd, rm`
fn neg(rd: u32, rm: u32) -> u32 {
    0xCB00_03E0 | rm << 16 | rd
}

/// `b.hi` over the next instruction.
const SKIP_IF_HIGHER: u32 = 0x5400_0048;

/// `madd rd, rn, rm, ra`: rd = ra + rn * rm
fn madd(rd: u32, rn: u32, rm: u32, ra: u32) -> u32 {
    0x9B00_0000 | rm << 16 | ra << 10 | rn << 5 | rd
}

/// Frame slot `n` sits above the saved frame pointer and return address.
/// The largest element, in slots, that `push` copies into a list
/// with room without calling the runtime.
const INLINE_PUSH: u32 = 8;

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

/// Largest frame, in bytes, that one `stp`/`ldp` can set up and tear down.
const SMALL_FRAME: u32 = 496;

/// Largest slot offset a single load or store can carry.
const MAX_OFFSET: u32 = 4095;

/// Where the two operands of a binary operation are once both have been
/// evaluated.
enum Operands {
    /// The registers holding the left and the right one.
    Registers(u32, u32),
    /// The register holding the left one; the right one is this small
    /// constant.
    Constant(u32, u32),
}

/// `lsl to, from, #count`, with the low 6 bits of the count as when
/// shifting by a register.
fn lsl_imm(to: u32, from: u32, count: u32) -> u32 {
    let count = count & 63;
    0xD340_0000 | ((64 - count) & 63) << 16 | (63 - count) << 10 | from << 5 | to
}

/// `asr to, from, #count`, with the low 6 bits of the count.
fn asr_imm(to: u32, from: u32, count: u32) -> u32 {
    0x9340_FC00 | (count & 63) << 16 | from << 5 | to
}

/// `mov to, from`
fn mov(to: u32, from: u32) -> u32 {
    0xAA00_03E0 | from << 16 | to
}

/// `and to, from, #(2^bits - 1)`: the low `bits` bits, 1 to 63 of them.
fn low_bits(to: u32, from: u32, bits: u32) -> u32 {
    0x9240_0000 | (bits - 1) << 10 | from << 5 | to
}

/// `tst from, #(2^bits - 1)`
fn tst_low_bits(from: u32, bits: u32) -> u32 {
    0xF240_001F | (bits - 1) << 10 | from << 5
}

/// `ldr rt, [buffer, index, lsl #3]`
fn ldr_scaled(rt: u32, buffer: u32, index: u32) -> u32 {
    0xF860_7800 | index << 16 | buffer << 5 | rt
}

/// `str rt, [buffer, index, lsl #3]`
fn str_scaled(rt: u32, buffer: u32, index: u32) -> u32 {
    0xF820_7800 | index << 16 | buffer << 5 | rt
}

/// `cmp left, right`
fn cmp(left: u32, right: u32) -> u32 {
    0xEB00_001F | right << 16 | left << 5
}

/// A branch whose destination is filled in later: the index of its
/// placeholder and what kind of branch it is.
#[derive(Clone, Copy)]
enum Jump {
    /// `cbz reg` (or `cbnz reg`): on a register being zero (or not).
    Zero { at: usize, reg: u32, nonzero: bool },
    /// `b.<code>`: on the flags of a compare.
    Flags { at: usize, code: u32 },
    /// `b`.
    Always { at: usize },
}

/// The condition code of a comparison operator.
fn comparison(op: BinOp) -> Option<u32> {
    Some(match op {
        BinOp::Eq => EQ,
        BinOp::NotEq => NE,
        BinOp::Lt => LT,
        BinOp::LtEq => LE,
        BinOp::Gt => GT,
        BinOp::GtEq => GE,
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

/// An index as the generator can remember it: a variable that lives in
/// a register, or a small constant.
#[derive(Clone, Copy, PartialEq)]
enum At {
    Reg(u32),
    Constant(u32),
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

/// What x12 holds: the buffer of the list or string in `place`, or the
/// address of its element `at`, from position `since` in the code on.
#[derive(Clone, Copy)]
struct Held {
    place: Place,
    at: Option<At>,
    since: usize,
}

/// How many checks are remembered at a time.
const KNOWN: usize = 4;

/// An element of a list (or a field of one, or a byte of a string) whose
/// list is in a fixed place and whose index takes no work to evaluate.
#[derive(Clone, Copy)]
struct Tracked {
    place: Place,
    at: At,
    /// Slots in one element; 1 for a string, whose elements are bytes.
    size: u32,
    /// Slots from the start of the element to the part meant.
    offset: u32,
    bytes: bool,
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
    /// The pending branches of the conditions being generated: those of
    /// an outer `if` lie below those of the statements inside it.
    jumps: Vec<Jump>,
    /// Local variables that own buffers and are in scope: slot and type.
    live: Vec<(u32, Ty)>,
    frame_bytes: u32,
    ret: Ty,
    sret_slot: Option<u32>,
    scratch_slot: u32,
    /// The variables of the function being generated that live in
    /// registers: slot and register.
    registers: &'a [(u32, u8)],
    /// Registers the function has to spare, given to its `for` loops for
    /// their position, item and index: slot and register.
    loop_registers: Vec<(u32, u8)>,
    /// In a function with a loop, the registers still left over hold
    /// the addresses that arrived as parameters: slot and register.
    param_registers: Vec<(u32, u8)>,
    /// First frame slot of the area where those registers' old values
    /// are kept.
    saved_slot: u32,
    /// Where the function being generated keeps its entries in the
    /// checker's tables.
    sh: Shift,
    /// The index checks that need not be made again here.
    known: Vec<Known>,
    /// What x12 holds here, when that is worth knowing.
    held: Option<Held>,
    /// How many of the waiting registers are taken at this point.
    parked: u32,
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
        jumps: Vec::new(),
        live: Vec::new(),
        frame_bytes: 0,
        ret: Ty::Unit,
        sret_slot: None,
        scratch_slot: 0,
        registers: &[],
        loop_registers: Vec::new(),
        param_registers: Vec::new(),
        saved_slot: 0,
        sh: Shift::default(),
        known: Vec::with_capacity(KNOWN),
        held: None,
        parked: 0,
    };
    for index in 0..prog.fns.len() as u32 {
        g.function(index);
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
        self.forget_since(at);
    }

    /// Completes a conditional branch at `at` to jump here.
    fn patch_branch_if(&mut self, at: usize, cond: u32) {
        let delta = (self.words.len() - at) as u32;
        self.words[at] = 0x5400_0000 | (delta & 0x7_FFFF) << 5 | cond;
        self.forget_since(at);
    }

    fn patch_jump(&mut self, at: usize) {
        let delta = (self.words.len() - at) as u32;
        self.words[at] = 0x1400_0000 | (delta & 0x03FF_FFFF);
        self.forget_since(at);
    }

    // ---- what is known at this point ------------------------------------
    //
    // Inside straight-line code an index is checked against the length
    // of its list once, and x12 keeps the buffer or the element's address
    // for the next use. Both are forgotten wherever the list could have
    // changed (a call, `push`, `pop`, any statement that stores more than
    // a number or a `bool`), where the index changes, and where another
    // path joins that has not made the check.

    /// Forgets every index check and what x12 holds.
    fn forget(&mut self) {
        self.known.clear();
        self.held = None;
    }

    /// Forgets what was learnt after position `from` in the code: it is
    /// not known on a path that left before, or it concerns a variable
    /// that no longer exists.
    fn forget_since(&mut self, from: usize) {
        if self.known.is_empty() && self.held.is_none() {
            return;
        }
        self.known.retain(|known| known.since <= from);
        if self.held.is_some_and(|held| held.since > from) {
            self.held = None;
        }
    }

    /// The variable that lives in `reg` has just been given a new value.
    fn writes(&mut self, reg: u32) {
        self.known.retain(|known| known.at != At::Reg(reg));
        if self.held.is_some_and(|held| held.at == Some(At::Reg(reg))) {
            self.held = None;
        }
    }

    fn jump_back(&mut self, to: usize) {
        let delta = to as i64 - self.words.len() as i64;
        self.emit(0x1400_0000 | (delta as u32 & 0x03FF_FFFF));
    }

    /// Calls a runtime routine.
    fn runtime(&mut self, entry: u32) {
        // Any routine that comes back may have changed a list.
        if !matches!(entry, runtime::INDEX_FAIL | runtime::POP_EMPTY | runtime::DIV_ZERO) {
            self.forget();
        }
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
        self.prog.expr_ty[self.sh.e(id)]
    }

    /// Expression `id`, with the name of a constant read as its value.
    #[inline(always)]
    fn expr(&self, id: ExprId) -> Expr {
        self.prog.expr(self.ast, self.sh, id)
    }

    /// True for values handled by address: structs, strings and lists.
    fn is_aggregate(&self, id: ExprId) -> bool {
        self.ty(id).is_aggregate()
    }

    fn function(&mut self, index: u32) {
        let prog = self.prog;
        let info = &prog.fns[index as usize];
        let Item::Fn { params, body, nodes, .. } = self.ast.items[info.item as usize] else {
            unreachable!("a function's item is a function");
        };
        self.sh = info.shift;
        self.fn_starts.push(self.words.len() as u32);
        self.registers = &info.registers;
        self.loop_registers.clear();
        // Registers the checker left unused go to the `for` loops, in the
        // order the loops are written: the position first, then the item
        // when it is a number or `bool`, then the index.
        let mut next = FIRST_REGISTER + info.registers.len() as u8;
        let end = FIRST_REGISTER + REGISTERS;
        if next < end {
            let first = nodes.stmts.0 as usize;
            for (i, stmt) in self.ast.stmts[first..first + nodes.stmts.1 as usize].iter().enumerate() {
                let &Stmt::For { index, iter, .. } = stmt else {
                    continue;
                };
                let id = (first + i) as StmtId;
                let Ty::List(list) = self.ty(iter) else {
                    unreachable!("checked: `for` goes through a list");
                };
                let elem = prog.lists[list as usize];
                let elem_size = prog.size_of(elem);
                let base = prog.stmt_slot[self.sh.s(id)];
                // Only a number or `bool` can live in a register; other
                // items are looked at through their address.
                let mut wanted = [Some(base), (!elem.is_aggregate()).then_some(base + 4), index.map(|_| base + 4 + elem_size)];
                for slot in wanted.iter_mut().filter_map(Option::take) {
                    if next < end {
                        self.loop_registers.push((slot, next));
                        next += 1;
                    }
                }
                if next == end {
                    break;
                }
            }
        }
        self.param_registers.clear();
        if next < end && info.params.iter().any(|param| param.by_ref) {
            let first = nodes.stmts.0 as usize;
            let stmts = &self.ast.stmts[first..first + nodes.stmts.1 as usize];
            if stmts.iter().any(|stmt| matches!(stmt, Stmt::While { .. } | Stmt::For { .. })) {
                // A register costs a save and a restore on every call, so
                // it goes only to an address the function uses more than once.
                let exprs = nodes.exprs.0..nodes.exprs.0 + nodes.exprs.1;
                for (slot, param) in info.params.iter().enumerate() {
                    if !param.by_ref || next == end {
                        continue;
                    }
                    let res = slot as u32 | BY_REF;
                    let uses = exprs
                        .clone()
                        .filter(|&id| prog.expr_res[self.sh.e(id)] == res && matches!(self.expr(id), Expr::Ident(_)))
                        .take(2)
                        .count();
                    if uses == 2 {
                        self.param_registers.push((slot as u32, next));
                        next += 1;
                    }
                }
            }
        }
        // Above the slots, room for the caller's values of those registers.
        self.saved_slot = info.slots;
        let saved = (info.registers.len() + self.loop_registers.len() + self.param_registers.len()) as u32;
        self.frame_bytes = (16 + 8 * (info.slots + saved)).div_ceil(16) * 16;
        self.ret = info.ret;
        self.sret_slot = info.sret_slot;
        self.scratch_slot = info.scratch_slot;
        self.live.clear();
        self.forget();

        // A function that begins by sending some calls straight back,
        // `if n < 2 { return n }`, decides that before it sets up its
        // frame: those calls then cost a compare, a branch and `ret`.
        let guarded = self.guard(body, info.params.as_slice());

        if self.frame_bytes <= SMALL_FRAME {
            // One instruction makes room and saves the two registers.
            self.emit(0xA980_7BFD | ((self.frame_bytes / 8).wrapping_neg() & 0x7F) << 15); // stp x29, x30, [sp, #-frame]!
        } else {
            // A large frame takes two steps: the amount is split into what
            // fits one instruction shifted by 12 bits and what fits one plain.
            if self.frame_bytes >> 12 > 0 {
                self.emit(0xD140_03FF | (self.frame_bytes >> 12) << 10); // sub sp, sp, #high, lsl #12
            }
            self.emit(0xD100_03FF | (self.frame_bytes & 0xFFF) << 10); // sub sp, sp, #low
            self.emit(0xA900_7BFD); // stp x29, x30, [sp]
        }
        self.emit(0x9100_03FD); // mov x29, sp
        // The registers in use are x19, x20, ... without gaps.
        self.saved_registers(saved, true);
        // Each parameter arrives in a register as a value or a pointer,
        // and moves to its slot or to the register it lives in.
        for slot in 0..self.ast.list(params).len() as u32 {
            let param = self.param_registers.iter().find(|&&(taken, _)| taken == slot).map(|&(_, reg)| reg as u32);
            match self.slot_register(slot).or(param) {
                Some(reg) => self.emit(mov(reg, slot)),
                None => self.emit(str(slot, FP, frame(slot))),
            }
        }
        if let Some(slot) = self.sret_slot {
            self.emit(str(X8, FP, frame(slot)));
        }
        // A temporary may be freed without having been filled, when the
        // expression that fills it was skipped. Null makes that harmless.
        for &slot in &info.zero_slots {
            self.emit(str(XZR, FP, frame(slot)));
        }
        self.block_from(body, guarded as usize);
        if self.ret == Ty::Unit {
            self.mov_imm(X0, 0);
            self.epilogue();
        } else {
            // The checker proved every path returns; this is unreachable.
            self.emit(BRK_1);
        }
    }

    /// Saves, or brings back, the caller's values of the first `count`
    /// registers that variables live in, two at a time where the frame
    /// is small enough for that.
    fn saved_registers(&mut self, count: u32, save: bool) {
        let first = FIRST_REGISTER as u32;
        let mut i = 0;
        while i < count {
            let slot = frame(self.saved_slot + i);
            if i + 1 < count && slot + 1 <= MAX_PAIR {
                self.emit(if save { stp(first + i, first + i + 1, FP, slot) } else { ldp(first + i, first + i + 1, FP, slot) });
                i += 2;
            } else {
                self.emit(if save { str(first + i, FP, slot) } else { ldr(first + i, FP, slot) });
                i += 1;
            }
        }
    }

    fn epilogue(&mut self) {
        self.saved_registers((self.registers.len() + self.loop_registers.len() + self.param_registers.len()) as u32, false);
        // Everything parked on the stack during a statement has been
        // taken off again by now, so sp is where the frame starts.
        if self.frame_bytes <= SMALL_FRAME {
            self.emit(0xA8C0_7BFD | (self.frame_bytes / 8) << 15); // ldp x29, x30, [sp], #frame
        } else {
            self.emit(0xA940_7BFD); // ldp x29, x30, [sp]
            self.emit(0x9100_03FF | (self.frame_bytes & 0xFFF) << 10); // add sp, sp, #low
            if self.frame_bytes >> 12 > 0 {
                self.emit(0x9140_03FF | (self.frame_bytes >> 12) << 10); // add sp, sp, #high, lsl #12
            }
        }
        self.emit(RET);
    }

    /// Generates a block and frees the owning locals declared in it.
    /// Whether a block ends by leaving it, so that nothing runs past its
    /// last statement and no jump over what follows is needed.
    fn leaves(&self, body: List) -> bool {
        match self.ast.list(body).last() {
            Some(&last) => matches!(self.ast.stmts[last as usize], Stmt::Return(_) | Stmt::Break | Stmt::Continue),
            None => false,
        }
    }

    /// The register a parameter arrives in, when `id` names one that is
    /// a number or a `bool` passed by value.
    fn arriving(&self, id: ExprId, params: &[crate::sema::ParamInfo]) -> Option<u32> {
        let Expr::Ident(_) = self.expr(id) else {
            return None;
        };
        let place = self.place(id)?;
        let param = params.get(place.slot as usize)?;
        (!place.by_ref && !param.by_ref && matches!(param.ty, Ty::I64 | Ty::Bool)).then_some(place.slot)
    }

    /// When the function's first statement is `if a < b { return c }`
    /// with parameters and constants only, generates it ahead of the
    /// frame, straight from the registers the arguments arrive in, and
    /// says so: the body then starts at its second statement.
    fn guard(&mut self, body: List, params: &[crate::sema::ParamInfo]) -> bool {
        let Some(&first) = self.ast.list(body).first() else {
            return false;
        };
        let Stmt::If { cond, then_block, else_branch: Else::None } = self.ast.stmts[first as usize] else {
            return false;
        };
        let &[only] = self.ast.list(then_block) else {
            return false;
        };
        let Stmt::Return(value) = self.ast.stmts[only as usize] else {
            return false;
        };
        if self.prog.stmt_drops[self.sh.s(first)].1 != 0 || self.prog.stmt_drops[self.sh.s(only)].1 != 0 {
            return false;
        }
        let Expr::Binary { op, lhs, rhs } = self.expr(cond) else {
            return false;
        };
        let (Some(code), Some(left)) = (comparison(op), self.arriving(lhs, params)) else {
            return false;
        };
        // What to compare with: another parameter or a small constant.
        let right = match self.expr(rhs) {
            Expr::Int(_) => {
                let value = self.prog.ints[self.prog.expr_res[self.sh.e(rhs)] as usize];
                if !(0..4096).contains(&value) {
                    return false;
                }
                Err(value as u32)
            }
            _ => match self.arriving(rhs, params) {
                Some(reg) => Ok(reg),
                None => return false,
            },
        };
        // What to hand back: nothing, a parameter or a constant.
        let result = match value {
            None => Err(0),
            Some(value) => match self.expr(value) {
                Expr::Int(_) => Err(self.prog.ints[self.prog.expr_res[self.sh.e(value)] as usize]),
                Expr::Bool(value) => Err(value as i64),
                _ => match self.arriving(value, params) {
                    Some(reg) => Ok(reg),
                    None => return false,
                },
            },
        };
        match right {
            Ok(right) => self.emit(cmp(left, right)),
            Err(value) => self.emit(0xF100_001F | value << 10 | left << 5), // cmp left, #value
        }
        let on = self.placeholder();
        match result {
            Ok(X0) => {}
            Ok(reg) => self.emit(mov(X0, reg)),
            Err(value) => self.mov_imm(X0, value),
        }
        self.emit(RET);
        self.patch_branch_if(on, code ^ 1);
        true
    }

    fn block(&mut self, body: List) {
        self.block_from(body, 0);
    }

    /// Generates a block from its statement number `skip` on.
    fn block_from(&mut self, body: List, skip: usize) {
        let outer = self.live.len();
        let start = self.words.len();
        for &id in &self.ast.list(body)[skip..] {
            self.stmt(id);
        }
        self.drop_live_from(outer);
        self.live.truncate(outer);
        // Its variables are gone, and their slots may be used again.
        self.forget_since(start);
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
        self.forget();
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
        let (start, len) = self.prog.stmt_drops[self.sh.s(id)];
        self.drop_range(start, len, keep_x0);
    }

    /// Frees the temporaries listed in `drops[start..start + len]`.
    fn drop_range(&mut self, start: u32, len: u32, keep_x0: bool) {
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
                let skip_then = self.branch_if_false(id, cond);
                self.block(then_block);
                match else_branch {
                    Else::None => self.patch_here(skip_then),
                    Else::Block(body) => {
                        let skip_else = (!self.leaves(then_block)).then(|| self.placeholder());
                        self.patch_here(skip_then);
                        self.block(body);
                        if let Some(at) = skip_else {
                            self.patch_jump(at);
                        }
                    }
                    Else::If(next) => {
                        let skip_else = (!self.leaves(then_block)).then(|| self.placeholder());
                        self.patch_here(skip_then);
                        self.stmt(next);
                        if let Some(at) = skip_else {
                            self.patch_jump(at);
                        }
                    }
                }
            }
            Stmt::While { cond, body } if self.prog.stmt_drops[self.sh.s(id)].1 == 0 => {
                // The test sits below the body, so that every round takes
                // one branch: back up when the condition holds. The loop
                // is entered by jumping to the test.
                let enter = self.placeholder();
                // The code below is reached again from its own end.
                self.forget();
                let top = self.words.len();
                self.loops.push(LoopFixups { start: top, breaks: Vec::new(), continues: Some(Vec::new()), live: self.live.len() });
                self.block(body);
                let fixups = self.loops.pop().expect("loop stack is balanced");
                for at in fixups.continues.expect("this loop collects its continues") {
                    self.patch_jump(at);
                }
                self.patch_jump(enter);
                let mark = self.jumps.len();
                let mut jumps = std::mem::take(&mut self.jumps);
                self.condition_jumps(cond, true, &mut jumps);
                self.jumps = jumps;
                self.patch_to(mark, top);
                for at in fixups.breaks {
                    self.patch_jump(at);
                }
            }
            Stmt::While { cond, body } => {
                // The condition leaves temporaries to free on every round,
                // which is simplest with the test on top.
                self.forget();
                let start = self.words.len();
                let exit = self.branch_if_false(id, cond);
                self.loops.push(LoopFixups { start, breaks: Vec::new(), continues: None, live: self.live.len() });
                self.block(body);
                self.jump_back(start);
                self.patch_here(exit);
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
        if self.prog.stmt_flag[self.sh.s(id)] == APPEND {
            // `s = s + more`: the new text is added to `s` in place. It
            // is computed first; only then is `s` looked at.
            let Expr::Binary { rhs, .. } = self.expr(value) else {
                unreachable!("checked: an append adds something");
            };
            if self.place(target).is_some() || self.place(rhs).is_some() {
                // (`two_addresses` puts the first of its two in x0.)
                self.two_addresses(target, rhs);
            } else {
                self.address(rhs);
                self.emit(PUSH_X0);
                self.address(target);
                self.emit(pop(X1));
            }
            self.runtime(runtime::STR_APPEND);
            return;
        }
        let ty = self.ty(target);
        let place = self.place(target);
        if !ty.is_aggregate() {
            if let Some(reg) = place.and_then(|place| self.register(place)) {
                self.value_in(value, reg);
                self.writes(reg);
                return;
            }
            if place.is_none() {
                if let Some(element) = self.tracked(target) {
                    // The value first, then the check of the index, as
                    // when the element's address is worked out.
                    let from = self.source(value);
                    self.store_element(element, from);
                    return;
                }
            }
            if self.is_nothing(value) {
                // Zero and `false` are stored from the zero register.
                match place {
                    Some(place) if place.by_ref => {
                        let pointer = self.pointer(place.slot, X9);
                        self.emit(str(XZR, pointer, place.offset));
                        return;
                    }
                    Some(place) => {
                        self.emit(str(XZR, FP, frame(place.slot + place.offset)));
                        return;
                    }
                    None => {}
                }
            }
            self.value(value);
            match place {
                Some(place) if place.by_ref => {
                    let pointer = self.pointer(place.slot, X9);
                    self.emit(str(X0, pointer, place.offset));
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
                let pointer = self.pointer(place.slot, X9);
                self.copy(pointer, place.offset, size);
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
        self.emit(str(X0, FP, frame(self.scratch_slot)));
        if owning {
            self.drop_pointer(self.scratch_slot, ty);
        }
        self.frame_address(X0, source);
        self.emit(ldr(X9, FP, frame(self.scratch_slot)));
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
                self.emit(ldr(X9, FP, frame(slot)));
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
            None => {
                self.mov_imm(X0, 0);
                false
            }
        };
        let frees_something = self.prog.stmt_drops[self.sh.s(id)].1 > 0 || !self.live.is_empty();
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
        // The position, the item and the index live in registers when
        // the function has some to spare.
        let position_reg = self.slot_register(position);
        match position_reg {
            Some(reg) => self.mov_imm(reg, 0),
            None => self.emit(str(XZR, FP, frame(position))),
        }
        // The test sits below the body: one branch per round. It leaves
        // the list's address in x0 and the position in `at` for the top.
        let at = position_reg.unwrap_or(X1);
        let enter = self.placeholder();
        self.forget();
        let top = self.words.len();
        if has_index {
            match self.slot_register(index_slot) {
                Some(reg) => self.emit(mov(reg, at)),
                None => self.emit(str(at, FP, frame(index_slot))),
            }
        }
        self.emit(ldr(X9, X0, 0)); // buffer
        if elem_size == 1 {
            match self.slot_register(item_slot) {
                Some(reg) => self.emit(ldr_scaled(reg, X9, at)),
                None => {
                    self.emit(ldr_scaled(X10, X9, at));
                    self.emit(str(X10, FP, frame(item_slot)));
                }
            }
        } else {
            self.element_address(X0, X9, at, elem_size);
            self.copy_to_frame(item_slot, elem_size);
        }

        self.loops.push(LoopFixups { start: top, breaks: Vec::new(), continues: Some(Vec::new()), live: self.live.len() });
        self.block(body);
        let fixups = self.loops.pop().expect("loop stack is balanced");
        for at in fixups.continues.expect("a `for` loop collects its continues") {
            self.patch_jump(at);
        }
        match position_reg {
            Some(reg) => self.emit(0x9100_0400 | reg << 5 | reg), // add reg, reg, #1
            None => {
                self.emit(ldr(X0, FP, frame(position)));
                self.emit(0x9100_0400); // add x0, x0, #1
                self.emit(str(X0, FP, frame(position)));
            }
        }
        self.patch_jump(enter);
        if in_place {
            self.address(iter);
        } else {
            self.frame_address(X0, own_list);
        }
        self.emit(ldr(X9, X0, 1)); // length
        if position_reg.is_none() {
            self.emit(ldr(X1, FP, frame(position)));
        }
        self.emit(cmp(at, X9));
        let again = self.placeholder();
        self.patch_one(Jump::Flags { at: again, code: LO }, top);
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
                self.frame_address(X0, own);
            }
        }
        self.emit(str(X0, FP, frame(base)));
        // Until more code is written, x0 still holds what slot `base` does.
        let fresh = self.words.len();
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
            let mut next: Option<(usize, Option<bool>)> = None;
            match info.kind {
                PAT_VARIANT => {
                    if test {
                        if self.words.len() == fresh {
                            self.emit(ldr(X9, X0, 0)); // tag
                        } else {
                            self.emit(ldr(X9, FP, frame(base)));
                            self.emit(ldr(X9, X9, 0)); // tag
                        }
                        self.emit(cmp_x9_imm(info.res + 1));
                        next = Some((self.placeholder(), None));
                    }
                    if let Pattern::Variant { args, .. } = self.ast.patterns[arm.pattern as usize] {
                        for &sub in self.ast.list(args) {
                            let field = self.prog.pats[self.sh.p(sub)];
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
            if test && !self.leaves(arm.body) {
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
            if self.prog.needs_drop(ty) && !self.prog.expr_moved[self.sh.e(value)] {
                self.clone_frame(slot, ty);
            }
            self.moved_out(value);
        } else if let Some(reg) = self.slot_register(slot) {
            self.value_in(value, reg);
            self.writes(reg);
        } else if self.is_nothing(value) {
            self.emit(str(XZR, FP, frame(slot)));
        } else {
            self.value(value);
            self.emit(str(X0, FP, frame(slot)));
        }
    }

    /// The register the variable in frame slot `slot` lives in, if any.
    fn slot_register(&self, slot: u32) -> Option<u32> {
        let found = |list: &[(u32, u8)]| list.iter().find(|&&(taken, _)| taken == slot).map(|&(_, reg)| reg as u32);
        found(self.registers).or_else(|| found(&self.loop_registers))
    }

    /// The register a place lives in: only whole variables have one.
    fn register(&self, place: Place) -> Option<u32> {
        if place.by_ref || place.offset != 0 || (self.registers.is_empty() && self.loop_registers.is_empty()) {
            return None;
        }
        self.slot_register(place.slot)
    }

    /// The register `id` lives in, when it is a variable that has one.
    /// Such a variable cannot change while an expression is evaluated:
    /// only a statement can assign to it, and nothing has its address.
    fn in_register(&self, id: ExprId) -> Option<u32> {
        if self.registers.is_empty() && self.loop_registers.is_empty() {
            return None;
        }
        match self.expr(id) {
            Expr::Ident(_) => self.place(id).and_then(|place| self.register(place)),
            _ => None,
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
                self.emit(str(XZR, FP, frame(slot + offset + idle_slot(leaf))));
            }
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
        let variant = self.prog.expr_aux[self.sh.e(id)];
        let tmp = self.prog.expr_tmp[self.sh.e(id)];
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
        self.copy(FP, frame(slot), size);
    }

    /// Copies `size` slots from the address in x0 to `first` slots past
    /// the address in `to`, two slots at a time where the offsets allow.
    /// Uses x10 and x11.
    fn copy(&mut self, to: u32, first: u32, size: u32) {
        let mut i = 0;
        while i < size {
            if i + 1 < size && i + 1 <= MAX_PAIR && first + i + 1 <= MAX_PAIR {
                self.emit(ldp(X10, X11, X0, i));
                self.emit(stp(X10, X11, to, first + i));
                i += 2;
            } else {
                self.emit(ldr(X10, X0, i));
                self.emit(str(X10, to, first + i));
                i += 1;
            }
        }
    }

    /// Copies `size` slots from the address in x0 to `offset` slots past
    /// the address in x9.
    fn copy_to_pointer(&mut self, offset: u32, size: u32) {
        self.copy(X9, offset, size);
    }

    /// `rd = buffer + index * element size`. Uses x10 when the element is
    /// larger than one slot; `rd` may be the same register as an input.
    fn element_address(&mut self, rd: u32, buffer: u32, index: u32, elem_size: u32) {
        if elem_size == 1 {
            self.emit(add_scaled(rd, buffer, index));
        } else if (elem_size * 8).is_power_of_two() {
            self.emit(add_shifted(rd, buffer, index, (elem_size * 8).trailing_zeros()));
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

    /// `id` as an element the generator can keep track of: `xs[i]`,
    /// `xs[i].field` or a byte `text[i]`, where the list or string is a
    /// variable or a part of one and the index is a variable that lives
    /// in a register or a small constant. Reaching such an element
    /// disturbs only x9 to x12.
    fn tracked(&self, id: ExprId) -> Option<Tracked> {
        let mut offset = 0;
        let mut at = id;
        loop {
            match self.expr(at) {
                Expr::Field { base, .. } => {
                    offset += self.prog.expr_res[self.sh.e(at)];
                    at = base;
                }
                Expr::Index { base, index } => {
                    let (size, bytes) = match self.ty(base) {
                        Ty::Str => (1, true),
                        Ty::List(list) => (self.prog.size_of(self.prog.lists[list as usize]), false),
                        _ => return None,
                    };
                    let place = self.place(base)?;
                    let at = match self.in_register(index) {
                        Some(reg) => At::Reg(reg),
                        None => {
                            let Expr::Int(_) = self.expr(index) else {
                                return None;
                            };
                            // Small enough for any offset made from it
                            // to fit in an instruction; a negative one
                            // is left to fail its check the long way.
                            let value = self.prog.ints[self.prog.expr_res[self.sh.e(index)] as usize];
                            if value < 0 || (value + 1).saturating_mul(size as i64) > MAX_OFFSET as i64 {
                                return None;
                            }
                            At::Constant(value as u32)
                        }
                    };
                    return Some(Tracked { place, at, size, offset, bytes });
                }
                _ => return None,
            }
        }
    }

    /// Checks the index of a tracked element against the length, unless
    /// that has been done on every path here, and makes x12 hold what
    /// reaches the element. Returns how to address it from x12: an index
    /// register (scaled by the element's size) or none, and an offset in
    /// slots (bytes for a string). Uses x9 to x12 only.
    fn element(&mut self, element: Tracked) -> (Option<u32>, u32) {
        let Tracked { place, at, size, offset, .. } = element;
        // An element of several slots is used through its address; of
        // any other only the buffer is kept, which serves every index.
        let whole = size != 1 && matches!(at, At::Reg(_));
        let wanted = whole.then_some(at);
        let known = self.known.iter().any(|known| known.place == place && known.at == at);
        let held = self.held.is_some_and(|held| held.place == place && held.at == wanted);
        if !known || !held {
            // The list's three slots: in the frame, or behind a pointer.
            let (header, first) = if place.by_ref {
                (self.pointer(place.slot, X10), place.offset)
            } else {
                (FP, frame(place.slot + place.offset))
            };
            // When both are wanted, one instruction reads the buffer and
            // the length, which lie side by side.
            let both = !known && !held && first + 1 <= MAX_PAIR;
            if !known {
                if both {
                    self.emit(ldp(X12, X9, header, first));
                } else {
                    self.emit(ldr(X9, header, first + 1)); // length
                }
                match at {
                    At::Reg(reg) => {
                        self.emit(cmp(reg, X9));
                        self.emit(0x5400_0043); // b.lo +2: an unsigned compare also rejects negatives
                    }
                    At::Constant(value) => {
                        self.emit(cmp_x9_imm(value));
                        self.emit(SKIP_IF_HIGHER);
                    }
                }
                self.runtime(runtime::INDEX_FAIL);
                if self.known.len() == KNOWN {
                    self.known.remove(0);
                }
                self.known.push(Known { place, at, since: self.words.len() });
            }
            if !held {
                match at {
                    At::Reg(reg) if whole => {
                        let buffer = if both {
                            X12
                        } else {
                            self.emit(ldr(X9, header, first));
                            X9
                        };
                        let bytes = size * 8;
                        if bytes.is_power_of_two() {
                            self.emit(add_shifted(X12, buffer, reg, bytes.trailing_zeros()));
                        } else {
                            self.mov_imm(X11, bytes as i64);
                            self.emit(madd(X12, reg, X11, buffer));
                        }
                    }
                    _ if both => {}
                    _ => self.emit(ldr(X12, header, first)),
                }
                self.held = Some(Held { place, at: wanted, since: self.words.len() });
            }
        }
        match at {
            At::Reg(_) if whole => (None, offset),
            At::Reg(reg) => (Some(reg), 0),
            At::Constant(value) => (None, value * size + offset),
        }
    }

    /// Loads a tracked number, `bool` or byte into `reg`.
    fn load_element(&mut self, element: Tracked, reg: u32) {
        let (index, offset) = self.element(element);
        self.emit(match (index, element.bytes) {
            (Some(index), false) => ldr_scaled(reg, X12, index),
            (None, false) => ldr(reg, X12, offset),
            (Some(index), true) => ldrb_index(reg, X12, index),
            (None, true) => ldrb(reg, X12, offset),
        });
    }

    /// Stores the number or `bool` in `reg` into a tracked list element.
    fn store_element(&mut self, element: Tracked, reg: u32) {
        let (index, offset) = self.element(element);
        self.emit(match index {
            Some(index) => str_scaled(reg, X12, index),
            None => str(reg, X12, offset),
        });
    }

    /// Leaves the address of a tracked list element, or of a part of it,
    /// in x0.
    fn element_address_tracked(&mut self, element: Tracked) {
        let (index, offset) = self.element(element);
        match index {
            Some(index) => self.emit(add_scaled(X0, X12, index)),
            None if offset == 0 => self.emit(mov(X0, X12)),
            None if offset * 8 < 4096 => self.emit(add_slots(X0, X12, offset)),
            None => {
                self.mov_imm(X0, offset as i64 * 8);
                self.emit(add_reg(X0, X12, X0));
            }
        }
    }

    /// The register holding the address that the by-reference place in
    /// frame slot `slot` starts from: the register a parameter lives in,
    /// or `scratch` once the address has been loaded from the frame.
    fn pointer(&mut self, slot: u32, scratch: u32) -> u32 {
        if let Some(&(_, reg)) = self.param_registers.iter().find(|&&(taken, _)| taken == slot) {
            return reg as u32;
        }
        self.emit(ldr(scratch, FP, frame(slot)));
        scratch
    }

    /// Whether the expression is the constant 0 or `false`.
    fn is_nothing(&self, id: ExprId) -> bool {
        match self.expr(id) {
            Expr::Bool(value) => !value,
            _ => self.is_zero(id),
        }
    }

    /// Works out the number or `bool` `id` and says which register it is
    /// in: the zero register, the one the variable lives in, or x0.
    fn source(&mut self, id: ExprId) -> u32 {
        if self.is_nothing(id) {
            return XZR;
        }
        self.in_register(id).unwrap_or_else(|| {
            self.value(id);
            X0
        })
    }

    /// Leaves the address of a place in x0.
    fn place_address(&mut self, place: Place) {
        self.place_address_in(place, X0);
    }

    /// Leaves the address of a place in `reg`, using no other register.
    fn place_address_in(&mut self, place: Place, reg: u32) {
        if place.by_ref {
            let pointer = self.pointer(place.slot, reg);
            if place.offset > 0 {
                self.emit(add_slots(reg, pointer, place.offset));
            } else if pointer != reg {
                self.emit(mov(reg, pointer));
            }
        } else {
            self.frame_address(reg, place.slot + place.offset);
        }
    }

    // ---- shortcuts -----------------------------------------------------
    //
    // The general scheme computes every value in x0 and parks earlier
    // ones on the stack. A value that is a constant or sits in a variable
    // can instead be loaded straight into the register where it is
    // needed, after everything written before it has been evaluated.

    /// True for a number or `bool` that one or two instructions can load
    /// into any register without disturbing the others: a constant, a
    /// variable, a field of a variable.
    fn is_simple(&self, id: ExprId) -> bool {
        match self.expr(id) {
            Expr::Int(_) | Expr::Bool(_) => true,
            Expr::Ident(_) => !self.is_aggregate(id) && self.place(id).is_some(),
            Expr::Field { .. } | Expr::Index { .. } => {
                !self.is_aggregate(id) && (self.place(id).is_some() || self.tracked(id).is_some())
            }
            Expr::Call { .. } => self.len_of_place(id).is_some(),
            _ => false,
        }
    }

    /// Loads a simple value into `reg`. Uses x9 for a value behind a
    /// pointer and x9 to x12 for a list element.
    fn load_simple(&mut self, id: ExprId, reg: u32) {
        match self.expr(id) {
            Expr::Int(_) => self.mov_imm(reg, self.prog.ints[self.prog.expr_res[self.sh.e(id)] as usize]),
            Expr::Bool(value) => self.mov_imm(reg, value as i64),
            Expr::Call { .. } => {
                let place = self.len_of_place(id).expect("a simple call is the length of a variable");
                self.load_len(place, reg);
            }
            _ => {
                let Some(place) = self.place(id) else {
                    let element = self.tracked(id).expect("a simple value that is no constant or place is an element");
                    self.load_element(element, reg);
                    return;
                };
                if place.by_ref {
                    let pointer = self.pointer(place.slot, X9);
                    self.emit(ldr(reg, pointer, place.offset));
                } else if let Some(home) = self.register(place) {
                    self.emit(mov(reg, home));
                } else {
                    self.emit(ldr(reg, FP, frame(place.slot + place.offset)));
                }
            }
        }
    }

    /// The variable or part of one whose length `id` asks for, when `id`
    /// is such a `len()`: one or two loads give it.
    fn len_of_place(&self, id: ExprId) -> Option<Place> {
        let Expr::Call { callee, .. } = self.expr(id) else {
            return None;
        };
        if self.prog.expr_res[self.sh.e(id)] != BUILTIN_LEN {
            return None;
        }
        match self.expr(callee) {
            Expr::Field { base, .. } => self.place(base),
            _ => None,
        }
    }

    /// Loads the length of the list or string in `place` into `reg`.
    /// Uses x9 for one behind a pointer.
    fn load_len(&mut self, place: Place, reg: u32) {
        if place.by_ref {
            let pointer = self.pointer(place.slot, X9);
            self.emit(ldr(reg, pointer, place.offset + 1));
        } else {
            self.emit(ldr(reg, FP, frame(place.slot + place.offset + 1)));
        }
    }

    /// True when evaluating the number or `bool` `id` calls nothing that
    /// comes back: registers that calls do not preserve keep their
    /// values across it. (A failed check does not come back.)
    fn calls_nothing(&self, id: ExprId) -> bool {
        match self.expr(id) {
            Expr::Int(_) | Expr::Bool(_) | Expr::Ident(_) => true,
            Expr::Field { base, .. } => self.calls_nothing(base),
            Expr::Index { base, index } => self.calls_nothing(base) && self.calls_nothing(index),
            Expr::Unary { operand, .. } => self.calls_nothing(operand),
            Expr::Binary { lhs, rhs, .. } => {
                // Comparing strings is the runtime's work.
                self.ty(lhs) != Ty::Str && self.calls_nothing(lhs) && self.calls_nothing(rhs)
            }
            Expr::Call { .. } => self.len_of_place(id).is_some(),
            _ => false,
        }
    }

    /// Leaves the address of the string `lhs` in x0 and that of `rhs` in
    /// x1. Finding a variable does nothing that shows, so when one side
    /// is a variable the other is worked out first and nothing waits on
    /// the stack.
    fn two_addresses(&mut self, lhs: ExprId, rhs: ExprId) {
        if let Some(place) = self.place(rhs) {
            self.address(lhs);
            self.place_address_in(place, X1);
        } else if let Some(place) = self.place(lhs) {
            self.address(rhs);
            self.emit(mov(X1, X0));
            self.place_address_in(place, X0);
        } else {
            self.address(lhs);
            self.emit(PUSH_X0);
            self.address(rhs);
            self.emit(mov(X1, X0));
            self.emit(pop(X0));
        }
    }

    /// Evaluates the operands of a binary operation on numbers and says
    /// which registers they ended up in. A variable that lives in a
    /// register is used where it is. `constants` allows a small constant
    /// on the right to stay inside the instruction.
    fn operands(&mut self, lhs: ExprId, rhs: ExprId, constants: bool) -> Operands {
        let left = self.in_register(lhs);
        if constants {
            if let Expr::Int(_) = self.expr(rhs) {
                let value = self.prog.ints[self.prog.expr_res[self.sh.e(rhs)] as usize];
                if (0..4096).contains(&value) {
                    let left = left.unwrap_or_else(|| {
                        self.value(lhs);
                        X0
                    });
                    return Operands::Constant(left, value as u32);
                }
            }
        }
        match (left, self.in_register(rhs)) {
            (Some(left), Some(right)) => Operands::Registers(left, right),
            (Some(left), None) => {
                self.value(rhs);
                Operands::Registers(left, X0)
            }
            (None, Some(right)) => {
                self.value(lhs);
                Operands::Registers(X0, right)
            }
            (None, None) if self.is_simple(lhs) && self.is_simple(rhs) => {
                // Two plain loads: each goes straight where it is wanted.
                self.load_simple(lhs, X1);
                self.load_simple(rhs, X0);
                Operands::Registers(X1, X0)
            }
            (None, None) if self.is_simple(rhs) => {
                // Loading the right one leaves x0 alone.
                self.value(lhs);
                self.load_simple(rhs, X1);
                Operands::Registers(X0, X1)
            }
            (None, None) if self.parked < PARKS && self.calls_nothing(rhs) => {
                // The left one waits in a register that code without
                // calls leaves alone.
                let park = X13 + self.parked;
                self.value_in(lhs, park);
                self.parked += 1;
                self.value(rhs);
                self.parked -= 1;
                Operands::Registers(park, X0)
            }
            (None, None) => {
                self.value(lhs);
                self.emit(PUSH_X0);
                self.value(rhs);
                self.emit(pop(X1));
                Operands::Registers(X1, X0)
            }
        }
    }

    /// Compares two numbers or `bool`s and leaves the result in the flags.
    fn compare_flags(&mut self, op: BinOp, lhs: ExprId, rhs: ExprId) {
        // `a % 8 == 0` asks whether the low bits of `a` are clear, for
        // negative numbers too: no remainder needs computing.
        if matches!(op, BinOp::Eq | BinOp::NotEq) && self.is_zero(rhs) {
            if let Expr::Binary { op: BinOp::Rem, lhs: number, rhs: by } = self.expr(lhs) {
                if let Some(bits) = self.power_of_two(by) {
                    let reg = self.in_register(number).unwrap_or_else(|| {
                        self.value(number);
                        X0
                    });
                    self.emit(tst_low_bits(reg, bits));
                    return;
                }
            }
        }
        match self.operands(lhs, rhs, true) {
            Operands::Constant(left, value) => self.emit(0xF100_001F | value << 10 | left << 5), // cmp left, #value
            Operands::Registers(left, right) => self.emit(0xEB00_001F | right << 16 | left << 5), // cmp left, right
        }
    }

    /// Generates the test of an `if` or `while`. The branches taken when
    /// it fails are left on `jumps`; the result marks where they start,
    /// for `patch_here`. Comparisons and `&&`, `||`, `!` branch directly
    /// instead of first becoming a 0 or 1.
    fn branch_if_false(&mut self, id: StmtId, cond: ExprId) -> usize {
        let mark = self.jumps.len();
        if self.prog.stmt_drops[self.sh.s(id)].1 == 0 {
            let mut jumps = std::mem::take(&mut self.jumps);
            self.condition_jumps(cond, false, &mut jumps);
            self.jumps = jumps;
        } else {
            // The statement has temporaries to free before it branches.
            self.value(cond);
            self.drop_temps(id, true);
            let at = self.placeholder();
            self.jumps.push(Jump::Zero { at, reg: X0, nonzero: false });
        }
        mark
    }

    /// Generates code that jumps when `cond` comes out as `when` and
    /// falls through otherwise. The jumps are added to `out`, to be
    /// pointed somewhere by the caller. Operands are evaluated in the
    /// order written, and the right side of `&&` and `||` only when the
    /// left side does not decide, exactly as when the value is computed.
    fn condition_jumps(&mut self, cond: ExprId, when: bool, out: &mut Vec<Jump>) {
        match self.expr(cond) {
            Expr::Bool(value) => {
                if value == when {
                    let at = self.placeholder();
                    out.push(Jump::Always { at });
                }
            }
            Expr::Unary { op: UnOp::Not, operand } => self.condition_jumps(operand, !when, out),
            Expr::Binary { op: op @ (BinOp::And | BinOp::Or), lhs, rhs } => {
                // `a && b` is false as soon as `a` is; `a || b` is true
                // as soon as `a` is.
                let decides = op == BinOp::Or;
                if when == decides {
                    self.condition_jumps(lhs, decides, out);
                    self.condition_jumps(rhs, decides, out);
                } else {
                    // The left side deciding means: do not jump, go on
                    // after the whole condition.
                    let mut over = Vec::new();
                    self.condition_jumps(lhs, decides, &mut over);
                    self.condition_jumps(rhs, when, out);
                    let here = self.words.len();
                    for jump in over {
                        self.patch_one(jump, here);
                    }
                }
            }
            Expr::Binary { op, lhs, rhs } if comparison(op).is_some() && !self.is_aggregate(lhs) => {
                let code = comparison(op).expect("just checked");
                // Comparing with zero for equality needs no compare.
                // A remainder by a power of two is left to `compare_flags`.
                if matches!(op, BinOp::Eq | BinOp::NotEq)
                    && self.is_zero(rhs)
                    && !matches!(self.expr(lhs), Expr::Binary { op: BinOp::Rem, rhs: by, .. } if self.power_of_two(by).is_some())
                {
                    let reg = self.in_register(lhs).unwrap_or_else(|| {
                        self.value(lhs);
                        X0
                    });
                    let at = self.placeholder();
                    out.push(Jump::Zero { at, reg, nonzero: (op == BinOp::NotEq) == when });
                } else {
                    self.compare_flags(op, lhs, rhs);
                    let at = self.placeholder();
                    out.push(Jump::Flags { at, code: if when { code } else { code ^ 1 } });
                }
            }
            _ => {
                // Any other `bool`: a variable is tested where it lives.
                let reg = self.in_register(cond).unwrap_or_else(|| {
                    self.value(cond);
                    X0
                });
                let at = self.placeholder();
                out.push(Jump::Zero { at, reg, nonzero: when });
            }
        }
    }

    /// Completes one pending branch to jump to instruction `target`,
    /// which may lie before or after it.
    fn patch_one(&mut self, jump: Jump, target: usize) {
        if target == self.words.len() {
            // What was learnt after the jump is not known on its path.
            // (A jump backwards lands where nothing is known.)
            let (Jump::Zero { at, .. } | Jump::Flags { at, .. } | Jump::Always { at }) = jump;
            self.forget_since(at);
        }
        match jump {
            Jump::Zero { at, reg, nonzero } => {
                let delta = (target as i64 - at as i64) as u32;
                let op = if nonzero { 0xB500_0000 } else { 0xB400_0000 };
                self.words[at] = op | (delta & 0x7_FFFF) << 5 | reg;
            }
            Jump::Flags { at, code } => {
                let delta = (target as i64 - at as i64) as u32;
                self.words[at] = 0x5400_0000 | (delta & 0x7_FFFF) << 5 | code;
            }
            Jump::Always { at } => {
                let delta = (target as i64 - at as i64) as u32;
                self.words[at] = 0x1400_0000 | (delta & 0x03FF_FFFF);
            }
        }
    }

    /// Points the branches pending since `mark` at instruction `target`.
    fn patch_to(&mut self, mark: usize, target: usize) {
        while self.jumps.len() > mark {
            let jump = self.jumps.pop().expect("just checked");
            self.patch_one(jump, target);
        }
    }

    /// Points the branches pending since `mark` at the next instruction.
    fn patch_here(&mut self, mark: usize) {
        self.patch_to(mark, self.words.len());
    }

    /// Whether argument `i` of the call target travels as an address.
    fn arg_by_ref(&self, target: u32, i: usize) -> bool {
        if target >= FIRST_BUILTIN {
            matches!(
                target,
                BUILTIN_PRINT_STR | BUILTIN_EPRINT | BUILTIN_READ_FILE | BUILTIN_WRITE_FILE | BUILTIN_SET_EXECUTABLE
                    | BUILTIN_REMOVE_FILE
                    | BUILTIN_RENAME_FILE
                    | BUILTIN_STR_IS_UTF8
            )
        } else {
            self.prog.fns[target as usize].params[i].by_ref
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

    fn load_simple_arg(&mut self, arg: ExprId, by_ref: bool, reg: u32) {
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
        let expr = self.expr(id);
        if let Expr::Field { .. } | Expr::Index { .. } = expr {
            if let Some(element) = self.tracked(id).filter(|element| !element.bytes) {
                self.element_address_tracked(element);
                return;
            }
        }
        match expr {
            Expr::Str(_) => {
                let tmp = self.prog.expr_tmp[self.sh.e(id)];
                self.string_value(self.prog.expr_res[self.sh.e(id)], tmp);
                self.frame_address(X0, tmp);
            }
            Expr::Ident(_) => self.enum_value(id, &[]),
            Expr::Binary { lhs, rhs, .. } => {
                // `a + b` on strings: a new string in this expression's temporary.
                let tmp = self.prog.expr_tmp[self.sh.e(id)];
                self.two_addresses(lhs, rhs);
                self.frame_address(X8, tmp);
                self.runtime(runtime::STR_CONCAT);
                self.frame_address(X0, tmp);
            }
            Expr::StructLit { fields, .. } => {
                let tmp = self.prog.expr_tmp[self.sh.e(id)];
                for &init_id in self.ast.list(fields) {
                    let init = self.ast.field_inits[init_id as usize];
                    self.store(init.value, tmp + self.prog.init_offset[self.sh.i(init_id)]);
                }
                self.frame_address(X0, tmp);
            }
            Expr::ArrayLit { items } => self.list_literal(id, items),
            Expr::Try(inner) => self.try_expr(id, inner),
            Expr::Call { args, .. } if self.prog.expr_res[self.sh.e(id)] == BUILTIN_VARIANT => {
                self.enum_value(id, self.ast.list(args));
            }
            Expr::Call { args, .. } => {
                self.call(id, args);
                self.frame_address(X0, self.prog.expr_tmp[self.sh.e(id)]);
            }
            Expr::Field { base, .. } => {
                self.address(base);
                let offset = self.prog.expr_res[self.sh.e(id)];
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
                let at = self.checked_index(base, index);
                self.element_address(X0, X9, at, elem_size);
            }
            _ => unreachable!("checked: only these expressions have an address"),
        }
    }

    /// Evaluates `index`, then finds the list or string `base`, and stops
    /// the program unless the index is inside it. Leaves the buffer in x9
    /// and returns the register that holds the index. The index is
    /// evaluated first, so that nothing runs between reading the buffer
    /// pointer and using it.
    fn checked_index(&mut self, base: ExprId, index: ExprId) -> u32 {
        if let Some(place) = self.place(base) {
            // A variable's length and buffer are read where they are; the
            // index stays in the register it was worked out in.
            let at = self.in_register(index).unwrap_or_else(|| {
                self.value(index);
                X0
            });
            let (header, first) = if place.by_ref {
                (self.pointer(place.slot, X10), place.offset)
            } else {
                (FP, frame(place.slot + place.offset))
            };
            if first + 1 <= MAX_PAIR {
                // The buffer and the length in one instruction.
                self.emit(ldp(X9, X10, header, first));
                self.emit(cmp(at, X10));
                self.emit(0x5400_0043); // b.lo +2: an unsigned compare also rejects negatives
                self.runtime(runtime::INDEX_FAIL);
            } else {
                self.emit(ldr(X9, header, first + 1)); // length
                self.emit(cmp(at, X9));
                self.emit(0x5400_0043); // b.lo +2
                self.runtime(runtime::INDEX_FAIL);
                self.emit(ldr(X9, header, first)); // buffer
            }
            return at;
        }
        let at = if let Some(reg) = self.in_register(index) {
            // A variable in a register stays where it is.
            self.address(base);
            reg
        } else {
            self.value(index);
            self.emit(PUSH_X0);
            self.address(base);
            self.emit(pop(X1));
            X1
        };
        self.emit(ldr(X9, X0, 1)); // length
        self.emit(cmp(at, X9));
        self.emit(0x5400_0043); // b.lo +2: an unsigned compare also rejects negatives
        self.runtime(runtime::INDEX_FAIL);
        self.emit(ldr(X9, X0, 0)); // buffer
        at
    }

    fn list_literal(&mut self, id: ExprId, items: List) {
        let tmp = self.prog.expr_tmp[self.sh.e(id)];
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
                if self.prog.needs_drop(elem) && !self.prog.expr_moved[self.sh.e(item)] {
                    // The element was copied from a variable: give the
                    // copy in the list buffers of its own.
                    self.emit(ldr(X0, FP, frame(tmp)));
                    self.mov_imm(X11, i as i64 * elem_size as i64 * 8);
                    self.emit(add_reg(X0, X0, X11));
                    self.clone_value(elem);
                }
                self.moved_out(item);
            } else {
                self.emit(str(X0, X9, offset));
            }
        }
        self.mov_imm(X0, count as i64);
        self.emit(str(X0, FP, frame(tmp + 1))); // length
        self.emit(str(X0, FP, frame(tmp + 2))); // capacity
        self.frame_address(X0, tmp);
    }

    /// `inner?`. When `inner` is an `Ok` or a `Some`, what it holds is the
    /// value: a number or `bool` in x0, anything else in this expression's
    /// temporary with its address in x0. Otherwise the function returns
    /// the `Err` or the `None` here and now, after freeing what a `return`
    /// at this point would free.
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
        self.emit(ldr(X9, X0, 0));
        self.emit(0xD100_0529); // sub x9, x9, #1: zero for `Ok` and `Some`
        let ok = self.placeholder();

        let ret_slot = self.sret_slot.expect("checked: this function returns an enum");
        self.emit(ldr(X10, FP, frame(ret_slot)));
        if let Some(&(error_ty, error_at)) = prog.enums[index as usize].variants[1].fields.first() {
            let error_to = prog.enums[ret_index as usize].variants[1].fields[0].1;
            for i in 0..prog.size_of(error_ty) {
                self.emit(ldr(X11, X0, error_at + i));
                self.emit(str(X11, X10, error_to + i));
            }
            self.mov_imm(X11, 2);
            self.emit(str(X11, X10, 0));
            if prog.needs_drop(error_ty) {
                if from_temp {
                    self.emit(str(XZR, FP, frame(inner_tmp)));
                } else {
                    self.emit(add_slots(X0, X10, error_to));
                    self.clone_value(error_ty);
                }
            }
        } else {
            self.mov_imm(X11, 2);
            self.emit(str(X11, X10, 0));
        }
        self.drop_range(start, len, false);
        for i in (0..self.live.len()).rev() {
            let (slot, ty) = self.live[i];
            self.drop_frame(slot, ty);
        }
        // Values of the unfinished statement may be parked on the stack.
        self.emit(0x9100_03BF); // mov sp, x29
        self.epilogue();

        self.patch_cbz_reg(ok, false, X9);
        if !value_ty.is_aggregate() {
            self.emit(ldr(X0, X0, value_at));
            return;
        }
        let tmp = prog.expr_tmp[self.sh.e(id)];
        self.emit(add_slots(X0, X0, value_at));
        self.copy_to_frame(tmp, prog.size_of(value_ty));
        if prog.needs_drop(value_ty) {
            if from_temp {
                self.emit(str(XZR, FP, frame(inner_tmp)));
            } else {
                self.clone_frame(tmp, value_ty);
            }
        }
        self.frame_address(X0, tmp);
    }

    /// With the addresses of two strings in x0 and x1: leaves in x0
    /// whether the first comes before, or after, the second, as `op`
    /// asks. The bytes are compared as numbers from 0 to 255 up to the
    /// first difference; when one string is the start of the other, the
    /// shorter one comes first.
    fn string_order(&mut self, op: BinOp) {
        self.forget();
        self.emit(ldr(X9, X0, 0)); // the bytes of the first
        self.emit(ldr(X10, X0, 1)); // its length
        self.emit(ldr(X11, X1, 0)); // the bytes of the second
        self.emit(ldr(X12, X1, 1)); // its length
        self.emit(cmp(X10, X12));
        self.emit(0x9A8C_3142); // csel x2, x10, x12, lo: the shorter length
        self.emit(0xD280_0003); // mov x3, #0
        self.emit(cmp(3, 2)); // compare: cmp x3, x2
        self.emit(0x5400_00E2); // b.hs lengths
        self.emit(0x3863_6924); // ldrb w4, [x9, x3]
        self.emit(0x3863_6965); // ldrb w5, [x11, x3]
        self.emit(0x9100_0463); // add x3, x3, #1
        self.emit(cmp(4, 5));
        self.emit(0x5400_0000 | ((-6i32 as u32) & 0x7_FFFF) << 5); // b.eq compare
        self.emit(0x1400_0002); // b over
        self.emit(cmp(X10, X12)); // lengths: every shared byte is equal
        self.emit(cset_x0(match op {
            BinOp::Lt => LO,
            BinOp::LtEq => LS,
            BinOp::Gt => HI,
            _ => HS,
        }));
    }

    /// Generates code that leaves a scalar value in x0.
    fn value(&mut self, id: ExprId) {
        let expr = self.expr(id);
        if let Expr::Field { .. } | Expr::Index { .. } = expr {
            if let Some(element) = self.tracked(id) {
                self.load_element(element, X0);
                return;
            }
        }
        match expr {
            Expr::Try(inner) => self.try_expr(id, inner),
            Expr::Int(_) => self.mov_imm(X0, self.prog.ints[self.prog.expr_res[self.sh.e(id)] as usize]),
            Expr::Bool(value) => self.mov_imm(X0, value as i64),
            Expr::Index { base, index } if self.ty(base) == Ty::Str => {
                // A byte of a string, checked like a list index.
                let at = self.checked_index(base, index);
                self.emit(0x3860_6920 | at << 16); // ldrb w0, [x9, at]
            }
            Expr::Index { base, index } if !self.is_aggregate(id) => {
                // A number or `bool` in a list: one slot, loaded directly.
                let at = self.checked_index(base, index);
                self.emit(ldr_scaled(X0, X9, at));
            }
            Expr::Ident(_) | Expr::Field { .. } | Expr::Index { .. } => match self.place(id) {
                Some(place) if place.by_ref => {
                    let pointer = self.pointer(place.slot, X9);
                    self.emit(ldr(X0, pointer, place.offset));
                }
                Some(place) => match self.register(place) {
                    Some(reg) => self.emit(mov(X0, reg)),
                    None => self.emit(ldr(X0, FP, frame(place.slot + place.offset))),
                },
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
            Expr::Binary { op, lhs, rhs } if matches!(self.ty(lhs), Ty::Enum(_)) => {
                // `==` and `!=` on enums without values compare the tags.
                // The left tag is read before the right side is computed.
                self.address(lhs);
                self.emit(ldr(X0, X0, 0));
                self.emit(PUSH_X0);
                self.address(rhs);
                self.emit(ldr(X0, X0, 0));
                self.emit(pop(X1));
                self.emit(0xEB00_003F); // cmp x1, x0
                self.emit(cset_x0(if op == BinOp::Eq { EQ } else { NE }));
            }
            Expr::Binary { op: op @ (BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq), lhs, rhs } if self.ty(lhs) == Ty::Str => {
                self.two_addresses(lhs, rhs);
                self.string_order(op);
            }
            Expr::Binary { op, lhs, rhs } if self.ty(lhs) == Ty::Str => {
                // `==` and `!=` on strings compare the bytes.
                self.two_addresses(lhs, rhs);
                self.runtime(runtime::STR_EQ);
                if op == BinOp::NotEq {
                    self.emit(0xD240_0000); // eor x0, x0, #1
                }
            }
            Expr::Binary { op, lhs, rhs } => self.binary(op, lhs, rhs, X0),
            Expr::Call { args, .. } => self.call(id, args),
            Expr::MutArg(_) | Expr::Float(_) | Expr::Str(_) | Expr::StructLit { .. } | Expr::ArrayLit { .. } => {
                unreachable!("rejected by the checker or not a scalar")
            }
        }
    }

    /// Generates an operation on two numbers or `bool`s, with the result
    /// in register `dest`.
    fn binary(&mut self, op: BinOp, lhs: ExprId, rhs: ExprId, dest: u32) {
        if let Some(code) = comparison(op) {
            self.compare_flags(op, lhs, rhs);
            self.emit(cset_x0(code) | dest);
            return;
        }
        if op == BinOp::Sub && self.is_zero(lhs) {
            // `0 - x`
            let right = self.in_register(rhs).unwrap_or_else(|| {
                self.value(rhs);
                X0
            });
            self.emit(neg(dest, right));
            return;
        }
        // A constant on the left of an operation that can be turned
        // round goes to the right, where it can stay inside the
        // instruction. A constant does nothing when it is evaluated, so
        // the order of the two does not show.
        let turn = matches!(op, BinOp::Add | BinOp::Mul | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor)
            && matches!(self.expr(lhs), Expr::Int(_))
            && !matches!(self.expr(rhs), Expr::Int(_));
        let (lhs, rhs) = if turn { (rhs, lhs) } else { (lhs, rhs) };
        if let Some(bits) = if matches!(op, BinOp::Mul | BinOp::Div | BinOp::Rem) { self.power_of_two(rhs) } else { None } {
            // By a power of two: shifts and masks instead of multiplying
            // and dividing. Division rounds toward zero and a remainder
            // takes the sign of the left side, as with any other divisor.
            let left = self.in_register(lhs).unwrap_or_else(|| {
                self.value(lhs);
                X0
            });
            match op {
                BinOp::Mul => self.emit(lsl_imm(dest, left, bits)),
                BinOp::Div => {
                    // A negative number is first raised by divisor - 1.
                    let raise = (1i64 << bits) - 1;
                    if raise < 4096 {
                        self.emit(0x9100_0000 | (raise as u32) << 10 | left << 5 | X9); // add x9, left, #raise
                    } else {
                        self.mov_imm(X9, raise);
                        self.emit(0x8B00_0000 | X9 << 16 | left << 5 | X9); // add x9, left, x9
                    }
                    self.emit(0xF100_001F | left << 5); // cmp left, #0
                    self.emit(0x9A80_B129 | left << 16); // csel x9, x9, left, lt
                    self.emit(asr_imm(dest, X9, bits));
                }
                _ => {
                    self.emit(0xEB00_03E9 | left << 16); // negs x9, left
                    self.emit(low_bits(X10, left, bits));
                    self.emit(low_bits(X9, X9, bits));
                    self.emit(0xDA89_4540 | dest); // csneg dest, x10, x9, mi
                }
            }
            return;
        }
        match self.operands(lhs, rhs, matches!(op, BinOp::Add | BinOp::Sub | BinOp::Shl | BinOp::Shr)) {
            Operands::Constant(left, value) => self.emit(match op {
                BinOp::Add => 0x9100_0000 | value << 10 | left << 5 | dest, // add dest, left, #value
                BinOp::Sub => 0xD100_0000 | value << 10 | left << 5 | dest, // sub dest, left, #value
                BinOp::Shl => lsl_imm(dest, left, value),
                _ => asr_imm(dest, left, value),
            }),
            Operands::Registers(left, right) => {
                let both = right << 16 | left << 5;
                // Dividing by a constant other than zero needs no check.
                let safe = match self.expr(rhs) {
                    Expr::Int(_) => self.prog.ints[self.prog.expr_res[self.sh.e(rhs)] as usize] != 0,
                    _ => false,
                };
                match op {
                    BinOp::Add => self.emit(0x8B00_0000 | both | dest), // add dest, left, right
                    BinOp::Sub => self.emit(0xCB00_0000 | both | dest), // sub dest, left, right
                    BinOp::Mul => self.emit(0x9B00_7C00 | both | dest), // mul dest, left, right
                    BinOp::BitAnd => self.emit(0x8A00_0000 | both | dest), // and dest, left, right
                    BinOp::BitOr => self.emit(0xAA00_0000 | both | dest), // orr dest, left, right
                    BinOp::BitXor => self.emit(0xCA00_0000 | both | dest), // eor dest, left, right
                    // Shifts by a register use the low 6 bits of the count.
                    BinOp::Shl => self.emit(0x9AC0_2000 | both | dest), // lsl dest, left, right
                    BinOp::Shr => self.emit(0x9AC0_2800 | both | dest), // asr dest, left, right
                    BinOp::Div => {
                        if !safe {
                            self.trap_if_zero(right);
                        }
                        self.emit(0x9AC0_0C00 | both | dest); // sdiv dest, left, right
                    }
                    BinOp::Rem => {
                        if !safe {
                            self.trap_if_zero(right);
                        }
                        self.emit(0x9AC0_0C00 | both | X2); // sdiv x2, left, right
                        self.emit(0x9B00_8000 | right << 16 | left << 10 | X2 << 5 | dest); // msub dest, x2, right, left
                    }
                    _ => unreachable!("comparisons are handled above, `&&` and `||` by the caller"),
                }
            }
        }
    }

    /// For a constant that is a power of two from 2 up: which power.
    /// Whether the expression is the literal 0.
    fn is_zero(&self, id: ExprId) -> bool {
        match self.expr(id) {
            Expr::Int(_) => self.prog.ints[self.prog.expr_res[self.sh.e(id)] as usize] == 0,
            _ => false,
        }
    }

    fn power_of_two(&self, id: ExprId) -> Option<u32> {
        let Expr::Int(_) = self.expr(id) else {
            return None;
        };
        let value = self.prog.ints[self.prog.expr_res[self.sh.e(id)] as usize];
        (value >= 2 && value & (value - 1) == 0).then(|| value.trailing_zeros())
    }

    /// Generates code that leaves a number or `bool` in register `dest`,
    /// one of the registers variables live in. Arithmetic and constants
    /// are computed there directly.
    fn value_in(&mut self, id: ExprId, dest: u32) {
        match self.expr(id) {
            Expr::Int(_) | Expr::Bool(_) => self.load_simple(id, dest),
            Expr::Binary { op, lhs, rhs } if !matches!(op, BinOp::And | BinOp::Or) && !self.is_aggregate(lhs) => {
                self.binary(op, lhs, rhs, dest)
            }
            // A variable or a list element is loaded where it is wanted.
            _ if self.is_simple(id) => self.load_simple(id, dest),
            _ => {
                self.value(id);
                self.emit(mov(dest, X0));
            }
        }
    }

    /// Calls a function or a built-in. A scalar result is left in x0; an
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
        if let BUILTIN_STR_PUSH | BUILTIN_STR_SET = target {
            let base = receiver.expect("checked: a method call names its receiver");
            return self.byte_method(id, target, base, arg_ids);
        }
        if let BUILTIN_LEN | BUILTIN_LIST_PUSH | BUILTIN_LIST_POP | BUILTIN_STR_SLICE = target {
            let base = receiver.expect("checked: a method call names its receiver");
            return self.list_method(id, target, base, arg_ids);
        }
        // A method the standard library writes in azoL is a function
        // whose first argument is the receiver.
        let all = || receiver.into_iter().chain(arg_ids.iter().copied()).enumerate();
        // Arguments are evaluated in the order written. Those up to the
        // last one that needs real work wait on the stack; the simple
        // ones after it go straight into their registers.
        let mut worked = 0;
        for (i, arg) in all() {
            if !self.is_simple_arg(arg, self.arg_by_ref(target, i)) {
                worked = i + 1;
            }
        }
        // From which argument on the rest call nothing that comes back:
        // a number worked out before those can wait in a register
        // instead of on the stack.
        let mut quiet = worked;
        let first = receiver.is_some() as usize;
        for i in (0..worked.min(8)).rev() {
            if worked < 2 {
                // Nothing waits for a later argument.
                break;
            }
            let arg = if i < first { receiver.expect("just checked") } else { arg_ids[i - first] };
            let calm = match self.expr(arg) {
                Expr::MutArg(inner) => self.place(inner).is_some(),
                _ if self.arg_by_ref(target, i) => self.place(arg).is_some(),
                _ => self.calls_nothing(arg),
            };
            if !calm {
                break;
            }
            quiet = i;
        }
        // Where each argument that had to wait is: a register, or the stack.
        let mut waits = [None; 8];
        let before = self.parked;
        for (i, arg) in all() {
            let by_ref = self.arg_by_ref(target, i);
            if i >= worked {
                self.load_simple_arg(arg, by_ref, i as u32);
                continue;
            }
            let number = !by_ref && !matches!(self.expr(arg), Expr::MutArg(_));
            if i + 1 < worked {
                if number && i + 1 >= quiet && self.parked < PARKS {
                    let park = X13 + self.parked;
                    self.value_in(arg, park);
                    self.parked += 1;
                    waits[i] = Some(park);
                    continue;
                }
                match self.expr(arg) {
                    Expr::MutArg(inner) => self.address(inner),
                    _ if by_ref => self.address(arg),
                    _ => self.value(arg),
                }
                self.emit(PUSH_X0);
            } else {
                // The last of them goes into its place (a number is
                // worked out there: its register is written last), and
                // the earlier ones are brought back.
                match self.expr(arg) {
                    Expr::MutArg(inner) => self.address(inner),
                    _ if by_ref => self.address(arg),
                    _ if i > 0 => self.value_in(arg, i as u32),
                    _ => self.value(arg),
                }
                if i > 0 && !number {
                    self.emit(mov(i as u32, X0));
                }
                for reg in (0..i).rev() {
                    match waits[reg] {
                        Some(park) => self.emit(mov(reg as u32, park)),
                        None => self.emit(pop(reg as u32)),
                    }
                }
                self.parked = before;
            }
        }
        match target {
            BUILTIN_PRINT_I64 => self.runtime(runtime::PRINT_I64),
            BUILTIN_PRINT_BOOL => self.runtime(runtime::PRINT_BOOL),
            BUILTIN_PRINT_STR => self.runtime(runtime::PRINT_STR),
            BUILTIN_EPRINT => self.runtime(runtime::EPRINT_STR),
            BUILTIN_EXIT => self.runtime(runtime::EXIT),
            BUILTIN_CHR | BUILTIN_ARGS => {
                self.frame_address(X8, self.prog.expr_tmp[self.sh.e(id)]);
                self.runtime(if target == BUILTIN_CHR { runtime::CHR } else { runtime::ARGS });
            }
            BUILTIN_CLOCK => self.runtime(runtime::CLOCK),
            BUILTIN_STR_IS_UTF8 => self.runtime(runtime::STR_IS_UTF8),
            BUILTIN_READ_FILE | BUILTIN_WRITE_FILE | BUILTIN_SET_EXECUTABLE | BUILTIN_REMOVE_FILE | BUILTIN_RENAME_FILE => {
                // The routine fills in what the `Result` holds and says
                // which variant it is: 0 for `Ok`, 1 for `Err`.
                let tmp = self.prog.expr_tmp[self.sh.e(id)];
                self.frame_address(X8, tmp + 1);
                self.runtime(match target {
                    BUILTIN_READ_FILE => runtime::READ_FILE,
                    BUILTIN_WRITE_FILE => runtime::WRITE_FILE,
                    BUILTIN_REMOVE_FILE => runtime::REMOVE_FILE,
                    BUILTIN_RENAME_FILE => runtime::RENAME_FILE,
                    _ => runtime::SET_EXECUTABLE,
                });
                self.emit(0x9100_0400); // add x0, x0, #1: the tag
                self.emit(str(X0, FP, frame(tmp)));
            }
            BUILTIN_STR_FROM_I64 | BUILTIN_STR_FROM_BOOL => {
                self.frame_address(X8, self.prog.expr_tmp[self.sh.e(id)]);
                self.runtime(if target == BUILTIN_STR_FROM_I64 { runtime::STR_FROM_I64 } else { runtime::STR_FROM_BOOL });
            }
            _ => {
                if self.is_aggregate(id) {
                    // The callee writes its result where x8 points.
                    self.frame_address(X8, self.prog.expr_tmp[self.sh.e(id)]);
                }
                // The called code may change any list.
                self.forget();
                let at = self.placeholder();
                self.calls.push((at, target));
            }
        }
    }

    /// `s.push(byte)` and `s.set(index, byte)` on a string. The numbers
    /// are worked out first; the byte is stored right here when the
    /// string owns a buffer with room, and the runtime is asked only to
    /// give it one.
    fn byte_method(&mut self, id: ExprId, method: u32, base: ExprId, args: &[u32]) {
        let tmp = self.prog.expr_tmp[self.sh.e(id)];
        for &arg in args {
            self.value(arg);
            self.emit(PUSH_X0);
        }
        self.address(base);
        self.forget();
        self.emit(pop(X11)); // the byte
        if method == BUILTIN_STR_PUSH {
            self.emit(ldr(X9, X0, 1)); // length
            self.emit(ldr(X10, X0, 2)); // capacity
            self.emit(cmp(X9, X10));
            let grow = self.placeholder();
            self.emit(ldr(X10, X0, 0)); // buffer
            self.emit(0x3829_694B); // strb w11, [x10, x9]
            self.emit(0x9100_0529); // add x9, x9, #1
            self.emit(str(X9, X0, 1));
            let done = self.placeholder();
            self.patch_branch_if(grow, HS);
            self.one_byte_string(tmp);
            self.runtime(runtime::STR_APPEND);
            self.patch_jump(done);
        } else {
            self.emit(pop(X9)); // the index
            self.emit(ldr(X10, X0, 1)); // length
            self.emit(cmp(X9, X10));
            self.emit(0x5400_0043); // b.lo +2: an unsigned compare also rejects negatives
            self.runtime(runtime::INDEX_FAIL);
            self.emit(ldr(X10, X0, 2)); // capacity
            let owned = self.placeholder();
            // The bytes live in the executable: adding one byte makes
            // the runtime copy them into a buffer the string owns, and
            // the length is then put back.
            self.emit(str(X9, FP, frame(tmp + 4)));
            self.emit(str(X0, FP, frame(tmp + 5)));
            self.one_byte_string(tmp);
            self.runtime(runtime::STR_APPEND);
            self.emit(ldr(X0, FP, frame(tmp + 5)));
            self.emit(ldr(X9, FP, frame(tmp + 4)));
            self.emit(ldr(X11, FP, frame(tmp)));
            self.emit(ldr(X10, X0, 1));
            self.emit(0xD100_054A); // sub x10, x10, #1
            self.emit(str(X10, X0, 1));
            self.patch_cbz_reg(owned, true, X10);
            self.emit(ldr(X10, X0, 0)); // buffer
            self.emit(0x3829_694B); // strb w11, [x10, x9]
        }
        self.forget();
    }

    /// Makes the byte in x11 a string of one byte in the frame slots
    /// from `tmp` (the byte, then pointer, length and capacity 0) and
    /// leaves its address in x1. x0 is kept.
    fn one_byte_string(&mut self, tmp: u32) {
        self.emit(str(X11, FP, frame(tmp)));
        self.frame_address(X9, tmp);
        self.emit(str(X9, FP, frame(tmp + 1)));
        self.mov_imm(X9, 1);
        self.emit(str(X9, FP, frame(tmp + 2)));
        self.emit(str(XZR, FP, frame(tmp + 3)));
        self.frame_address(X1, tmp + 1);
    }

    fn list_method(&mut self, id: ExprId, method: u32, base: ExprId, args: &[u32]) {
        if method == BUILTIN_LEN {
            // Lists and strings keep their length in the same place.
            match self.place(base) {
                Some(place) => self.load_len(place, X0),
                None => {
                    self.address(base);
                    self.emit(ldr(X0, X0, 1));
                }
            }
            return;
        }
        if method == BUILTIN_STR_SLICE {
            // The bounds are evaluated first, so that nothing runs
            // between finding the string and reading it.
            match self.place(base) {
                Some(place) if self.is_simple(args[1]) => {
                    // Nothing after the first bound disturbs x1.
                    self.value_in(args[0], X1);
                    self.load_simple(args[1], X2);
                    self.place_address(place);
                }
                _ => {
                    self.value(args[0]);
                    self.emit(PUSH_X0);
                    self.value(args[1]);
                    self.emit(PUSH_X0);
                    self.address(base);
                    self.emit(pop(X2));
                    self.emit(pop(X1));
                }
            }
            self.frame_address(X8, self.prog.expr_tmp[self.sh.e(id)]);
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
                } else if let Some(place) = self.place(base) {
                    let from = self.source(args[0]);
                    // A number or `bool` goes straight into a list
                    // that has room; it is only put aside when the
                    // list has to grow.
                    let (header, first) = if place.by_ref {
                        (self.pointer(place.slot, X11), place.offset)
                    } else {
                        (FP, frame(place.slot + place.offset))
                    };
                    if first + 2 <= MAX_PAIR {
                        self.emit(ldp(X9, X10, header, first + 1)); // length and capacity
                    } else {
                        self.emit(ldr(X9, header, first + 1)); // length
                        self.emit(ldr(X10, header, first + 2)); // capacity
                    }
                    self.emit(cmp(X9, X10));
                    let grow = self.placeholder();
                    self.emit(ldr(X10, header, first)); // buffer
                    self.emit(str_scaled(from, X10, X9));
                    self.emit(0x9100_0529); // add x9, x9, #1
                    self.emit(str(X9, header, first + 1));
                    let done = self.placeholder();
                    self.patch_branch_if(grow, HS);
                    self.emit(str(from, FP, frame(tmp)));
                    self.place_address(place);
                    self.frame_address(X1, tmp);
                    self.mov_imm(X2, elem_size as i64 * 8);
                    self.runtime(runtime::LIST_PUSH);
                    self.patch_jump(done);
                    // The list is longer now and may have moved.
                    self.forget();
                    return;
                } else {
                    self.value(args[0]);
                    self.emit(str(X0, FP, frame(tmp)));
                }
                self.address(base);
                let mut done = None;
                if elem_size == 1 {
                    // With room left, a one-slot element is stored right
                    // here; only growing the list needs the runtime.
                    self.emit(ldr(X9, X0, 1)); // length
                    self.emit(ldr(X10, X0, 2)); // capacity
                    self.emit(cmp(X9, X10));
                    let grow = self.placeholder();
                    self.emit(ldr(X10, X0, 0)); // buffer
                    self.emit(ldr(X11, FP, frame(tmp)));
                    self.emit(str_scaled(X11, X10, X9));
                    self.emit(0x9100_0529); // add x9, x9, #1
                    self.emit(str(X9, X0, 1));
                    done = Some(self.placeholder());
                    self.patch_branch_if(grow, HS);
                } else if elem_size <= INLINE_PUSH {
                    // A small struct, enum, string or list is copied in
                    // the same way, slot by slot.
                    self.forget();
                    self.emit(ldr(X9, X0, 1)); // length
                    self.emit(ldr(X10, X0, 2)); // capacity
                    self.emit(cmp(X9, X10));
                    let grow = self.placeholder();
                    self.emit(ldr(X10, X0, 0)); // buffer
                    self.mov_imm(X11, elem_size as i64 * 8);
                    self.emit(0x9B0B_292B); // madd x11, x9, x11, x10
                    for slot in 0..elem_size {
                        self.emit(ldr(X12, FP, frame(tmp + slot)));
                        self.emit(str(X12, X11, slot));
                    }
                    self.emit(0x9100_0529); // add x9, x9, #1
                    self.emit(str(X9, X0, 1));
                    done = Some(self.placeholder());
                    self.patch_branch_if(grow, HS);
                }
                self.frame_address(X1, tmp);
                self.mov_imm(X2, elem_size as i64 * 8);
                self.runtime(runtime::LIST_PUSH);
                if let Some(done) = done {
                    self.patch_jump(done);
                }
                // The list is longer now and may have moved.
                self.forget();
            }
            _ => {
                self.address(base);
                self.emit(ldr(X9, X0, 1)); // length
                self.emit(0xB500_0049); // cbnz x9, +2
                self.runtime(runtime::POP_EMPTY);
                self.emit(0xD100_0529); // sub x9, x9, #1
                self.emit(str(X9, X0, 1));
                // The list is shorter now.
                self.forget();
                self.emit(ldr(X10, X0, 0)); // buffer
                self.element_address(X0, X10, X9, elem_size);
                if elem.is_aggregate() {
                    self.copy_to_frame(self.prog.expr_tmp[self.sh.e(id)], elem_size);
                } else {
                    self.emit(ldr(X0, X0, 0));
                }
            }
        }
    }

    /// Division by zero stops the program with a message instead of
    /// producing garbage.
    fn trap_if_zero(&mut self, divisor: u32) {
        self.emit(0xB500_0040 | divisor); // cbnz divisor, +2
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
        // Two slots at a time.
        assert_eq!(ldp(X10, X11, X0, 2), 0xA9412C0A); // ldp x10, x11, [x0, #16]
        assert_eq!(stp(X10, X11, X9, MAX_PAIR), 0xA91FAD2A); // stp x10, x11, [x9, #504]
        assert_eq!(stp(X19, X20, FP, 25), 0xA90CD3B3); // stp x19, x20, [x29, #200]
        assert_eq!(ldp(X21, 22, FP, 0), 0xA9405BB5); // ldp x21, x22, [x29]
        // List elements through x12, and values that wait in x13.
        assert_eq!(ldr(X12, X10, 1), 0xF940054C); // ldr x12, [x10, #8]
        assert_eq!(str_scaled(X0, X12, X19), 0xF8337980); // str x0, [x12, x19, lsl #3]
        assert_eq!(ldr(X1, X12, 3), 0xF9400D81); // ldr x1, [x12, #24]
        assert_eq!(mov(X13, X0), 0xAA0003ED); // mov x13, x0
        assert_eq!(add_reg(X13, X13, X0), 0x8B0001AD); // add x13, x13, x0
        assert_eq!(cmp_x9_imm(7), 0xF1001D3F); // cmp x9, #7
        assert_eq!(cmp(X1, X2), 0xEB02003F); // cmp x1, x2
        assert_eq!(add_shifted(X12, X9, X19, 5), 0x8B13152C); // add x12, x9, x19, lsl #5
        assert_eq!(add_shifted(X0, X9, X1, 4), 0x8B011120); // add x0, x9, x1, lsl #4
        assert_eq!(ldrb_index(X0, X12, X19), 0x38736980); // ldrb w0, [x12, x19]
        assert_eq!(ldrb(X1, X12, 4094), 0x397FF981); // ldrb w1, [x12, #4094]
        assert_eq!(neg(X20, X21), 0xCB1503F4); // neg x20, x21
        assert_eq!(neg(X0, X0), 0xCB0003E0); // neg x0, x0
        assert_eq!(SKIP_IF_HIGHER, 0x54000048); // b.hi .+8
        assert_eq!(add_reg(X9, X9, X10), 0x8B0A0129);
        assert_eq!(str(XZR, FP, 6), 0xF9001BBF);
        assert_eq!(add_slots(X0, X19, 5), 0x9100A260);
        assert_eq!(ldr(X9, X19, 2), 0xF9400A69);
        assert_eq!(ldr(X20, X19, 0), 0xF9400274);
        assert_eq!(ldr(X21, X19, 1), 0xF9400675);
        assert_eq!(ldr(X0, X19, 0), 0xF9400260);
        assert_eq!(0x9100_0294u32 | 24 << 10, 0x91006294);
        assert_eq!(add_reg(X20, X20, X10), 0x8B0A0294);
        assert_eq!(0xA980_7BFDu32 | ((32u32 / 8).wrapping_neg() & 0x7F) << 15, 0xA9BE7BFD); // stp x29, x30, [sp, #-32]!
        assert_eq!(0xA980_7BFDu32 | ((496u32 / 8).wrapping_neg() & 0x7F) << 15, 0xA9A17BFD); // stp x29, x30, [sp, #-496]!
        assert_eq!(0xA8C0_7BFDu32 | (32 / 8) << 15, 0xA8C27BFD); // ldp x29, x30, [sp], #32
        assert_eq!(0xA8C0_7BFDu32 | (496 / 8) << 15, 0xA8DF7BFD); // ldp x29, x30, [sp], #496
        assert_eq!(0x8A00_0000u32 | 20 << 16 | 19 << 5, 0x8A140260); // and x0, x19, x20
        assert_eq!(0xAA00_0000u32 | 1 << 5 | 3, 0xAA000023); // orr x3, x1, x0
        assert_eq!(0xCA00_0000u32 | 22 << 16 | 21, 0xCA160015); // eor x21, x0, x22
        assert_eq!(0x9AC0_2000u32 | 1 << 5, 0x9AC02020); // lsl x0, x1, x0
        assert_eq!(0x9AC0_2800u32 | 21 << 16 | 20 << 5 | 19, 0x9AD52A93); // asr x19, x20, x21
        assert_eq!(lsl_imm(X0, 19, 0), 0xD340FE60);
        assert_eq!(lsl_imm(X0, 19, 1), 0xD37FFA60);
        assert_eq!(lsl_imm(5, X1, 63), 0xD3410025);
        assert_eq!(lsl_imm(X0, X1, 36), 0xD35C6C20);
        assert_eq!(lsl_imm(X0, X1, 100), 0xD35C6C20); // the low 6 bits of the count
        assert_eq!(asr_imm(X0, 19, 0), 0x9340FE60);
        assert_eq!(asr_imm(7, X1, 63), 0x937FFC27);
        assert_eq!(asr_imm(X0, X1, 36), 0x9364FC20);
        assert_eq!(asr_imm(X0, X1, 100), 0x9364FC20);
        assert_eq!(mov(X0, 19), 0xAA1303E0);
        assert_eq!(mov(19, X0), 0xAA0003F3);
        assert_eq!(mov(24, 3), 0xAA0303F8);
        assert_eq!(mov(5, 28), 0xAA1C03E5);
        assert_eq!(mov(X1, X0), 0xAA0003E1);
        assert_eq!(0x9100_0000u32 | 7 << 10 | 19 << 5, 0x91001E60); // add x0, x19, #7
        assert_eq!(0xD100_0000u32 | 7 << 10 | 20 << 5, 0xD1001E80); // sub x0, x20, #7
        assert_eq!(0xF100_001Fu32 | 7 << 10 | 21 << 5, 0xF1001EBF); // cmp x21, #7
        assert_eq!(0x8B00_0000u32 | 20 << 16 | 19 << 5, 0x8B140260); // add x0, x19, x20
        assert_eq!(0x9B00_7C00u32 | 22 << 16, 0x9B167C00); // mul x0, x0, x22
        assert_eq!(0xB500_0040u32 | 23, 0xB5000057); // cbnz x23, +2
        assert_eq!(0x9AC0_0C00u32 | 23 << 16 | 19 << 5 | X2, 0x9AD70E62); // sdiv x2, x19, x23
        assert_eq!(0x9B00_8000u32 | 23 << 16 | 19 << 10 | X2 << 5, 0x9B17CC40); // msub x0, x2, x23, x19
        assert_eq!(0xEB00_001Fu32 | 20 << 16 | 19 << 5, 0xEB14027F); // cmp x19, x20
        assert_eq!(low_bits(X0, 19, 4), 0x92400E60); // and x0, x19, #0xf
        assert_eq!(tst_low_bits(X0, 1), 0xF240001F); // tst x0, #1
        assert_eq!(tst_low_bits(21, 3), 0xF2400ABF); // tst x21, #7
        assert_eq!(tst_low_bits(3, 32), 0xF2407C7F); // tst x3, #0xffffffff
        assert_eq!(low_bits(X10, X0, 10), 0x9240240A); // and x10, x0, #0x3ff
        assert_eq!(low_bits(X9, X9, 62), 0x9240F529); // and x9, x9, #0x3fffffffffffffff
        assert_eq!(0xEB00_03E9u32 | 19 << 16, 0xEB1303E9); // negs x9, x19
        assert_eq!(0xDA89_4540u32 | 20, 0xDA894554); // csneg x20, x10, x9, mi
        assert_eq!(0x9A80_B129u32 | 19 << 16, 0x9A93B129); // csel x9, x9, x19, lt
        assert_eq!(0x8B00_0000u32 | X9 << 16 | X9, 0x8B090009); // add x9, x0, x9
        assert_eq!(ldr_scaled(X0, X9, X1), 0xF8617920); // ldr x0, [x9, x1, lsl #3]
        assert_eq!(ldr_scaled(X0, X9, 20), 0xF8747920); // ldr x0, [x9, x20, lsl #3]
        assert_eq!(str_scaled(X11, X10, X9), 0xF829794B); // str x11, [x10, x9, lsl #3]
        assert_eq!(cmp(20, X9), 0xEB09029F); // cmp x20, x9
        assert_eq!(cmp(X1, X9), 0xEB09003F);
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
