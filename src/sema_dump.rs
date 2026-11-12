// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! `azol sema-dump <files...>`: everything the checker hands to the code
//! generators, as text. It exists so that the checker written in azoL
//! (`compiler/sema*.azl`) can be compared with this one, table by table.
//! Nothing else uses it.

use crate::sema::{self, Program, Ty};
use std::fmt::Write;
use std::process::ExitCode;

fn ty(ty: Ty) -> String {
    match ty {
        Ty::I64 => "i64".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Unit => "unit".to_string(),
        Ty::Str => "str".to_string(),
        Ty::Struct(index) => format!("S{index}"),
        Ty::List(index) => format!("L{index}"),
        Ty::Enum(index) => format!("E{index}"),
    }
}

fn tys(list: &[Ty]) -> String {
    list.iter().map(|&t| format!(" {}", ty(t))).collect()
}

fn dump(program: &Program) -> String {
    let mut out = String::new();
    let w = &mut out;
    let _ = writeln!(w, "lists {}", program.lists.len());
    for &elem in &program.lists {
        let _ = writeln!(w, "{}", ty(elem));
    }
    let _ = writeln!(w, "structs {}", program.structs.len());
    for info in &program.structs {
        let _ = writeln!(w, "struct {} template={} size={} args={}", info.name, info.template, info.size, tys(&info.args));
        for field in &info.fields {
            let _ = writeln!(w, "field {} {} {}", field.name, ty(field.ty), field.offset);
        }
        for &(offset, leaf) in &info.heap {
            let _ = writeln!(w, "heap {} {}", offset, ty(leaf));
        }
    }
    let _ = writeln!(w, "enums {}", program.enums.len());
    for info in &program.enums {
        let _ = writeln!(w, "enum {} template={} size={} owning={} args={}", info.name, info.template, info.size, info.owning as u32, tys(&info.args));
        for variant in &info.variants {
            let _ = writeln!(w, "variant");
            for &(field, offset) in &variant.fields {
                let _ = writeln!(w, "vfield {} {}", ty(field), offset);
            }
            for &(offset, leaf) in &variant.heap {
                let _ = writeln!(w, "vheap {} {}", offset, ty(leaf));
            }
        }
    }
    let _ = writeln!(w, "ints {}", program.ints.len());
    for &value in &program.ints {
        let _ = writeln!(w, "{value}");
    }
    let _ = writeln!(w, "strings {}", program.strings.len());
    for &(offset, len) in &program.strings {
        let _ = writeln!(w, "{offset} {len}");
    }
    let _ = write!(w, "data {} ", program.data.len());
    for &byte in &program.data {
        let _ = write!(w, "{byte:02x}");
    }
    let _ = writeln!(w);
    let _ = writeln!(w, "main {}", program.main);
    let _ = writeln!(w, "fns {}", program.fns.len());
    for info in &program.fns {
        let generic = info.generic.map_or("-".to_string(), |g| g.to_string());
        let sret = info.sret_slot.map_or("-".to_string(), |s| s.to_string());
        let _ = writeln!(
            w,
            "fn item={} shift={},{},{},{} generic={} depth={} origin={} ret={} slots={} sret={} scratch={}",
            info.item, info.shift.expr, info.shift.stmt, info.shift.pat, info.shift.init, generic, info.depth, info.origin, ty(info.ret), info.slots, sret, info.scratch_slot
        );
        let _ = writeln!(w, "targs{}", tys(&info.type_args));
        for param in &info.params {
            let _ = writeln!(w, "param {} {} {}", ty(param.ty), param.by_ref as u32, param.mutable as u32);
        }
        for &(slot, register) in &info.registers {
            let _ = writeln!(w, "reg {slot} {register}");
        }
        let _ = write!(w, "zero");
        for &slot in &info.zero_slots {
            let _ = write!(w, " {slot}");
        }
        let _ = writeln!(w);
    }
    let _ = writeln!(w, "exprs {}", program.expr_res.len());
    for i in 0..program.expr_res.len() {
        let _ = writeln!(w, "{} {} {} {} {}", program.expr_res[i], ty(program.expr_ty[i]), program.expr_aux[i], program.expr_tmp[i], program.expr_moved[i] as u32);
    }
    let _ = writeln!(w, "stmts {}", program.stmt_slot.len());
    for i in 0..program.stmt_slot.len() {
        let _ = writeln!(w, "{} {} {} {}", program.stmt_slot[i], program.stmt_flag[i], program.stmt_drops[i].0, program.stmt_drops[i].1);
    }
    let _ = writeln!(w, "pats {}", program.pats.len());
    for pat in &program.pats {
        let _ = writeln!(w, "{} {} {}", pat.kind, pat.res, pat.slot);
    }
    let _ = writeln!(w, "drops {}", program.drops.len());
    for &(slot, leaf) in &program.drops {
        let _ = writeln!(w, "{} {}", slot, ty(leaf));
    }
    let _ = writeln!(w, "inits {}", program.init_offset.len());
    for &offset in &program.init_offset {
        let _ = writeln!(w, "{offset}");
    }
    out
}

/// Prints the dump of a program, or every problem found in it as
/// `syntax <offset> <message>` or `error <offset> <expr> <message>`.
pub fn run(paths: &[&str]) -> ExitCode {
    if paths.is_empty() {
        eprintln!("{}", crate::USAGE);
        return ExitCode::from(2);
    }
    let Some((src, files, prelude_start)) = crate::load(paths, &crate::read_source) else {
        return ExitCode::FAILURE;
    };
    let ast = match crate::parse_program(&src, &files, prelude_start) {
        Ok(ast) => ast,
        Err(problems) => {
            for problem in &problems {
                println!("syntax {} {}", problem.offset, problem.message);
            }
            return ExitCode::FAILURE;
        }
    };
    match sema::check_with_prelude(&ast, &src, prelude_start as u32) {
        Ok(program) => {
            print!("{}", dump(&program));
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in &errors {
                let expr = error.expr.map_or("-".to_string(), |id| id.to_string());
                println!("error {} {} {}", error.offset, expr, error.message);
            }
            ExitCode::FAILURE
        }
    }
}
