// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Turns a checked program into ARM64 machine code in one pass over the
//! syntax tree. The scheme is deliberately simple: every expression
//! leaves its value in x0, intermediate values go on the stack, and
//! locals live in the function's frame. It is fast to generate, not fast
//! to run; an optimizing backend comes later.

use crate::ast::*;
use crate::sema::{Program, BUILTIN_PRINT};

pub struct Code {
    pub words: Vec<u32>,
    /// Index of the first instruction of `main`.
    pub entry: u32,
}

const X0: u32 = 0;
const X1: u32 = 1;
const X2: u32 = 2;
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

fn load_slot(reg: u32, slot: u32) -> u32 {
    0xF940_0000 | (slot + 2) << 10 | FP << 5 | reg // ldr reg, [x29, #16 + 8*slot]
}

fn store_slot(reg: u32, slot: u32) -> u32 {
    0xF900_0000 | (slot + 2) << 10 | FP << 5 | reg // str reg, [x29, #16 + 8*slot]
}

fn cset_x0(cond: u32) -> u32 {
    0x9A9F_07E0 | (cond ^ 1) << 12
}

struct LoopFixups {
    start: usize,
    breaks: Vec<usize>,
}

struct Gen<'a> {
    ast: &'a Ast,
    prog: &'a Program,
    words: Vec<u32>,
    fn_starts: Vec<u32>,
    /// (instruction index, function index) of every `bl` to patch.
    calls: Vec<(usize, u32)>,
    loops: Vec<LoopFixups>,
    frame: u32,
    returns_value: bool,
}

pub fn generate(ast: &Ast, prog: &Program) -> Code {
    let mut g = Gen {
        ast,
        prog,
        words: Vec::with_capacity(ast.exprs.len() * 3 + 64),
        fn_starts: Vec::with_capacity(prog.fns.len()),
        calls: Vec::new(),
        loops: Vec::new(),
        frame: 0,
        returns_value: false,
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

    /// Emits a placeholder branch and returns its index for `patch`.
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

    fn function(&mut self, index: u32, params: List, body: List) {
        let info = &self.prog.fns[index as usize];
        self.fn_starts.push(self.words.len() as u32);
        self.frame = (16 + 8 * info.slots).div_ceil(16) * 16;
        self.returns_value = info.ret != crate::sema::Ty::Unit;

        self.emit(0xD100_03FF | self.frame << 10); // sub sp, sp, #frame
        self.emit(0xA900_7BFD); // stp x29, x30, [sp]
        self.emit(0x9100_03FD); // mov x29, sp
        for slot in 0..self.ast.list(params).len() as u32 {
            self.emit(store_slot(slot, slot));
        }
        self.block(body);
        if self.returns_value {
            // The checker proved every path returns; this is unreachable.
            self.emit(BRK_1);
        } else {
            self.mov_imm(0);
            self.epilogue();
        }
    }

    fn epilogue(&mut self) {
        self.emit(0x9100_03BF); // mov sp, x29
        self.emit(0xA940_7BFD); // ldp x29, x30, [sp]
        self.emit(0x9100_03FF | self.frame << 10); // add sp, sp, #frame
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
                self.expr(value);
                self.emit(store_slot(X0, self.prog.stmt_slot[id as usize]));
            }
            Stmt::Assign { target, value } => {
                self.expr(value);
                self.emit(store_slot(X0, self.prog.expr_res[target as usize]));
            }
            Stmt::Expr(expr) => self.expr(expr),
            Stmt::Return(value) => {
                match value {
                    Some(value) => self.expr(value),
                    None => self.mov_imm(0),
                }
                self.epilogue();
            }
            Stmt::If { cond, then_block, else_branch } => {
                self.expr(cond);
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
                self.expr(cond);
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

    /// Generates code that leaves the value of `id` in x0.
    fn expr(&mut self, id: ExprId) {
        match self.ast.exprs[id as usize] {
            Expr::Int(_) => self.mov_imm(self.prog.ints[self.prog.expr_res[id as usize] as usize]),
            Expr::Bool(value) => self.mov_imm(value as i64),
            Expr::Ident(_) => self.emit(load_slot(X0, self.prog.expr_res[id as usize])),
            Expr::Unary { op, operand } => {
                self.expr(operand);
                self.emit(match op {
                    UnOp::Neg => 0xCB00_03E0, // neg x0, x0
                    UnOp::Not => 0xD240_0000, // eor x0, x0, #1
                });
            }
            Expr::Binary { op: op @ (BinOp::And | BinOp::Or), lhs, rhs } => {
                // Short circuit: the left value is the result when it decides.
                self.expr(lhs);
                let skip = self.placeholder();
                self.expr(rhs);
                self.patch_cbz(skip, op == BinOp::Or);
            }
            Expr::Binary { op, lhs, rhs } => {
                self.expr(lhs);
                self.emit(PUSH_X0);
                self.expr(rhs);
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
            Expr::Call { args, .. } => {
                let arg_ids = self.ast.list(args);
                for &arg in arg_ids {
                    self.expr(arg);
                    self.emit(PUSH_X0);
                }
                for reg in (0..arg_ids.len() as u32).rev() {
                    self.emit(pop(reg));
                }
                let at = self.placeholder();
                self.calls.push((at, self.prog.expr_res[id as usize]));
            }
            Expr::MutArg(_)
            | Expr::Float(_)
            | Expr::Str(_)
            | Expr::Field { .. }
            | Expr::Index { .. }
            | Expr::StructLit { .. }
            | Expr::ArrayLit { .. } => unreachable!("rejected by the checker"),
        }
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
        assert_eq!(load_slot(X0, 1), 0xF9400FA0);
        assert_eq!(store_slot(X0, 1), 0xF9000FA0);
        assert_eq!(store_slot(5, 5), 0xF9001FA5);
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
