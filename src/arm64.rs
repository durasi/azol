// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Turns a checked program into ARM64 machine code in one pass over the
//! syntax tree. The scheme is deliberately simple: every expression
//! leaves its value in x0, intermediate values go on the stack, and
//! locals live in the function's frame. It is fast to generate, not fast
//! to run; an optimizing backend comes later.

use crate::ast::*;
use crate::sema::{Program, Ty, BUILTIN_PRINT, BY_REF};

pub struct Code {
    pub words: Vec<u32>,
    /// Index of the first instruction of `main`.
    pub entry: u32,
}

const X0: u32 = 0;
const X1: u32 = 1;
const X2: u32 = 2;
const X8: u32 = 8;
const X9: u32 = 9;
const X10: u32 = 10;
const FP: u32 = 29;

// Condition codes.
const EQ: u32 = 0;
const NE: u32 = 1;
const GE: u32 = 10;
const LT: u32 = 11;
const GT: u32 = 12;
const LE: u32 = 13;

const RET: u32 = 0xD65F_03C0;
const BRK_1: u32 = 0xD420_0020;
const PUSH_X0: u32 = 0xF81F_0FE0; // str x0, [sp, #-16]!

/// `print(i64)`: formats the number into a stack buffer and writes it,
/// followed by a newline, to standard output with the `write` system call.
const PRINT_I64: [u32; 24] = [
    0xD100C3FF, // sub sp, sp, #48
    0x9100BFE9, // add x9, sp, #47
    0x5280014A, // mov w10, #10
    0x3900012A, // strb w10, [x9]
    0xF100001F, // cmp x0, #0
    0xDA80A40B, // cneg x11, x0, lt
    0xD280014D, // mov x13, #10
    0x9ACD096E, // udiv x14, x11, x13
    0x9B0DADCF, // msub x15, x14, x13, x11
    0x1100C1EF, // add w15, w15, #48
    0x381FFD2F, // strb w15, [x9, #-1]!
    0xAA0E03EB, // mov x11, x14
    0xB5FFFF6B, // cbnz x11, -5
    0xB6F80060, // tbz x0, #63, +3
    0x528005AF, // mov w15, #45
    0x381FFD2F, // strb w15, [x9, #-1]!
    0xD2800020, // mov x0, #1
    0xAA0903E1, // mov x1, x9
    0x9100C3E2, // add x2, sp, #48
    0xCB090042, // sub x2, x2, x9
    0xD2800090, // mov x16, #4
    0xD4001001, // svc #0x80
    0x9100C3FF, // add sp, sp, #48
    0xD65F03C0, // ret
];

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

/// Frame slot `n` sits above the saved frame pointer and return address.
const fn frame(slot: u32) -> u32 {
    slot + 2
}

fn cset_x0(cond: u32) -> u32 {
    0x9A9F_07E0 | (cond ^ 1) << 12
}

struct LoopFixups {
    start: usize,
    breaks: Vec<usize>,
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
    loops: Vec<LoopFixups>,
    frame_bytes: u32,
    ret: Ty,
    sret_slot: Option<u32>,
}

pub fn generate(ast: &Ast, prog: &Program) -> Code {
    let mut g = Gen {
        ast,
        prog,
        words: Vec::with_capacity(ast.exprs.len() * 3 + 64),
        fn_starts: Vec::with_capacity(prog.fns.len()),
        calls: Vec::new(),
        loops: Vec::new(),
        frame_bytes: 0,
        ret: Ty::Unit,
        sret_slot: None,
    };
    let mut index = 0;
    for item in &ast.items {
        if let Item::Fn { params, body, .. } = *item {
            g.function(index, params, body);
            index += 1;
        }
    }
    let print_start = g.words.len() as u32;
    g.words.extend_from_slice(&PRINT_I64);
    for &(at, target) in &g.calls {
        let dest = if target == BUILTIN_PRINT { print_start } else { g.fn_starts[target as usize] };
        let delta = dest as i64 - at as i64;
        g.words[at] = 0x9400_0000 | (delta as u32 & 0x03FF_FFFF);
    }
    Code { entry: g.fn_starts[prog.main as usize], words: g.words }
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
        let delta = (self.words.len() - at) as u32;
        let op = if nonzero { 0xB500_0000 } else { 0xB400_0000 };
        self.words[at] = op | (delta & 0x7_FFFF) << 5 | X0;
    }

    fn patch_jump(&mut self, at: usize) {
        let delta = (self.words.len() - at) as u32;
        self.words[at] = 0x1400_0000 | (delta & 0x03FF_FFFF);
    }

    fn jump_back(&mut self, to: usize) {
        let delta = to as i64 - self.words.len() as i64;
        self.emit(0x1400_0000 | (delta as u32 & 0x03FF_FFFF));
    }

    fn mov_imm(&mut self, value: i64) {
        let bits = value as u64;
        let mut first = true;
        for chunk in 0..4 {
            let part = ((bits >> (16 * chunk)) & 0xFFFF) as u32;
            if part == 0 && !(first && chunk == 3) {
                continue;
            }
            let op = if first { 0xD280_0000 } else { 0xF280_0000 }; // movz / movk
            self.emit(op | chunk << 21 | part << 5 | X0);
            first = false;
        }
    }

    fn ty(&self, id: ExprId) -> Ty {
        self.prog.expr_ty[id as usize]
    }

    fn is_struct(&self, id: ExprId) -> bool {
        matches!(self.ty(id), Ty::Struct(_))
    }

    fn function(&mut self, index: u32, params: List, body: List) {
        let info = &self.prog.fns[index as usize];
        self.fn_starts.push(self.words.len() as u32);
        self.frame_bytes = (16 + 8 * info.slots).div_ceil(16) * 16;
        self.ret = info.ret;
        self.sret_slot = info.sret_slot;

        self.emit(0xD100_03FF | self.frame_bytes << 10); // sub sp, sp, #frame
        self.emit(0xA900_7BFD); // stp x29, x30, [sp]
        self.emit(0x9100_03FD); // mov x29, sp
        // Each parameter arrives in a register as a value or a pointer.
        for slot in 0..self.ast.list(params).len() as u32 {
            self.emit(str(slot, FP, frame(slot)));
        }
        if let Some(slot) = self.sret_slot {
            self.emit(str(X8, FP, frame(slot)));
        }
        self.block(body);
        if self.ret == Ty::Unit {
            self.mov_imm(0);
            self.epilogue();
        } else {
            // The checker proved every path returns; this is unreachable.
            self.emit(BRK_1);
        }
    }

    fn epilogue(&mut self) {
        self.emit(0x9100_03BF); // mov sp, x29
        self.emit(0xA940_7BFD); // ldp x29, x30, [sp]
        self.emit(0x9100_03FF | self.frame_bytes << 10); // add sp, sp, #frame
        self.emit(RET);
    }

    fn block(&mut self, body: List) {
        for &id in self.ast.list(body) {
            self.stmt(id);
        }
    }

    fn stmt(&mut self, id: StmtId) {
        match self.ast.stmts[id as usize] {
            Stmt::Let { value, .. } => {
                let slot = self.prog.stmt_slot[id as usize];
                if self.is_struct(value) {
                    self.address(value);
                    self.copy_to_frame(slot, self.prog.size_of(self.ty(value)));
                } else {
                    self.value(value);
                    self.emit(str(X0, FP, frame(slot)));
                }
            }
            Stmt::Assign { target, value } => {
                let place = self.place(target).expect("checked: the target is a place");
                if self.is_struct(target) {
                    let size = self.prog.size_of(self.ty(target));
                    self.address(value);
                    if place.by_ref {
                        self.emit(ldr(X9, FP, frame(place.slot)));
                        self.copy_to_pointer(place.offset, size);
                    } else {
                        self.copy_to_frame(place.slot + place.offset, size);
                    }
                } else {
                    self.value(value);
                    if place.by_ref {
                        self.emit(ldr(X9, FP, frame(place.slot)));
                        self.emit(str(X0, X9, place.offset));
                    } else {
                        self.emit(str(X0, FP, frame(place.slot + place.offset)));
                    }
                }
            }
            Stmt::Expr(expr) => self.eval(expr),
            Stmt::Return(value) => {
                match value {
                    Some(value) if self.is_struct(value) => {
                        let size = self.prog.size_of(self.ty(value));
                        self.address(value);
                        let slot = self.sret_slot.expect("checked: this function returns a struct");
                        self.emit(ldr(X9, FP, frame(slot)));
                        self.copy_to_pointer(0, size);
                    }
                    Some(value) => self.value(value),
                    None => self.mov_imm(0),
                }
                self.epilogue();
            }
            Stmt::If { cond, then_block, else_branch } => {
                self.value(cond);
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
                let exit = self.placeholder();
                self.loops.push(LoopFixups { start, breaks: Vec::new() });
                self.block(body);
                self.jump_back(start);
                self.patch_cbz(exit, false);
                let fixups = self.loops.pop().expect("loop stack is balanced");
                for at in fixups.breaks {
                    self.patch_jump(at);
                }
            }
            Stmt::Break => {
                let at = self.placeholder();
                self.loops.last_mut().expect("checked: inside a loop").breaks.push(at);
            }
            Stmt::Continue => {
                let start = self.loops.last().expect("checked: inside a loop").start;
                self.jump_back(start);
            }
            Stmt::For { .. } | Stmt::Match { .. } => unreachable!("rejected by the checker"),
        }
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

    /// The location of a variable or of a field path inside a variable.
    fn place(&self, id: ExprId) -> Option<Place> {
        match self.ast.exprs[id as usize] {
            Expr::Ident(_) => {
                let res = self.prog.expr_res[id as usize];
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
            self.emit(add_slots(X0, FP, frame(place.slot + place.offset)));
        }
    }

    /// Evaluates an expression for its effects, whatever its type.
    fn eval(&mut self, id: ExprId) {
        if self.is_struct(id) {
            self.address(id);
        } else {
            self.value(id);
        }
    }

    /// Generates code that leaves the address of a struct value in x0.
    /// A computed struct is first stored in its own temporary.
    fn address(&mut self, id: ExprId) {
        if let Some(place) = self.place(id) {
            self.place_address(place);
            return;
        }
        match self.ast.exprs[id as usize] {
            Expr::StructLit { fields, .. } => {
                let tmp = self.prog.expr_tmp[id as usize];
                for &init_id in self.ast.list(fields) {
                    let init = self.ast.field_inits[init_id as usize];
                    let slot = tmp + self.prog.init_offset[init_id as usize];
                    if self.is_struct(init.value) {
                        self.address(init.value);
                        self.copy_to_frame(slot, self.prog.size_of(self.ty(init.value)));
                    } else {
                        self.value(init.value);
                        self.emit(str(X0, FP, frame(slot)));
                    }
                }
                self.emit(add_slots(X0, FP, frame(tmp)));
            }
            Expr::Call { args, .. } => {
                self.call(id, args);
                self.emit(add_slots(X0, FP, frame(self.prog.expr_tmp[id as usize])));
            }
            Expr::Field { base, .. } => {
                self.address(base);
                let offset = self.prog.expr_res[id as usize];
                if offset > 0 {
                    self.emit(add_slots(X0, X0, offset));
                }
            }
            _ => unreachable!("checked: only these expressions produce a struct"),
        }
    }

    /// Generates code that leaves an `i64` or `bool` value in x0.
    fn value(&mut self, id: ExprId) {
        match self.ast.exprs[id as usize] {
            Expr::Int(_) => self.mov_imm(self.prog.ints[self.prog.expr_res[id as usize] as usize]),
            Expr::Bool(value) => self.mov_imm(value as i64),
            Expr::Ident(_) | Expr::Field { .. } => {
                if let Some(place) = self.place(id) {
                    if place.by_ref {
                        self.emit(ldr(X9, FP, frame(place.slot)));
                        self.emit(ldr(X0, X9, place.offset));
                    } else {
                        self.emit(ldr(X0, FP, frame(place.slot + place.offset)));
                    }
                } else {
                    // A field of a computed struct, such as `make().x`.
                    let Expr::Field { base, .. } = self.ast.exprs[id as usize] else {
                        unreachable!("a name is always a place");
                    };
                    self.address(base);
                    self.emit(ldr(X0, X0, self.prog.expr_res[id as usize]));
                }
            }
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
            Expr::MutArg(_)
            | Expr::Float(_)
            | Expr::Str(_)
            | Expr::Index { .. }
            | Expr::StructLit { .. }
            | Expr::ArrayLit { .. } => unreachable!("rejected by the checker or not a scalar"),
        }
    }

    /// Calls a function. A scalar result is left in x0; a struct result is
    /// written to the call's temporary.
    fn call(&mut self, id: ExprId, args: List) {
        let target = self.prog.expr_res[id as usize];
        let arg_ids = self.ast.list(args);
        for (i, &arg) in arg_ids.iter().enumerate() {
            let by_ref = target != BUILTIN_PRINT && self.prog.fns[target as usize].params[i].by_ref;
            match self.ast.exprs[arg as usize] {
                Expr::MutArg(inner) => {
                    let place = self.place(inner).expect("checked: `mut` is given a place");
                    self.place_address(place);
                }
                _ if by_ref => self.address(arg),
                _ => self.value(arg),
            }
            self.emit(PUSH_X0);
        }
        for reg in (0..arg_ids.len() as u32).rev() {
            self.emit(pop(reg));
        }
        if self.is_struct(id) {
            // The callee writes its result where x8 points.
            self.emit(add_slots(X8, FP, frame(self.prog.expr_tmp[id as usize])));
        }
        let at = self.placeholder();
        self.calls.push((at, target));
    }

    fn compare(&mut self, cond: u32) {
        self.emit(0xEB00_003F); // cmp x1, x0
        self.emit(cset_x0(cond));
    }

    /// Division by zero stops the program instead of producing garbage.
    fn trap_if_zero(&mut self) {
        self.emit(0xB500_0040); // cbnz x0, +2
        self.emit(BRK_1);
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
