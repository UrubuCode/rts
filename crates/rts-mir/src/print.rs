//! Pretty-printer for MIR — produces a textual form similar to Cranelift's
//! `function … { block0(v0: i64): … }`. Used for `rts ir` debugging and the
//! `RTS_DUMP_MIR=1` env var.

use std::fmt;

use rts_hir::HirType;

use crate::ir::*;

impl fmt::Display for MirFunc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "function {}(", self.name)?;
        for (i, (_v, t)) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", fmt_ty(t))?;
        }
        write!(f, ") -> {} [{}]", fmt_ty(&self.ret), fmt_conv(&self.conv))?;
        writeln!(f, " {{")?;
        for block in &self.blocks {
            write!(f, "block{}", block.id)?;
            if !block.params.is_empty() {
                write!(f, "(")?;
                for (i, (v, t)) in block.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "v{}: {}", v, fmt_ty(t))?;
                }
                write!(f, ")")?;
            }
            writeln!(f, ":")?;
            for inst in &block.insts {
                writeln!(f, "    {}", fmt_inst(inst))?;
            }
            writeln!(f, "    {}", fmt_term(&block.term))?;
        }
        writeln!(f, "}}")
    }
}

fn fmt_conv(c: &CallConvHint) -> &'static str {
    match c {
        CallConvHint::Tail => "tail",
        CallConvHint::SystemV => "systemv",
        CallConvHint::WindowsFastcall => "windowsfastcall",
        CallConvHint::Cold => "cold",
    }
}

fn fmt_ty(t: &HirType) -> String {
    match t {
        HirType::Number => "number".into(),
        HirType::Bool => "bool".into(),
        HirType::Str => "str".into(),
        HirType::Void => "void".into(),
        HirType::I8 => "i8".into(),
        HirType::I16 => "i16".into(),
        HirType::I32 => "i32".into(),
        HirType::I64 => "i64".into(),
        HirType::I128 => "i128".into(),
        HirType::U8 => "u8".into(),
        HirType::U16 => "u16".into(),
        HirType::U32 => "u32".into(),
        HirType::U64 => "u64".into(),
        HirType::U128 => "u128".into(),
        HirType::F32 => "f32".into(),
        HirType::F64 => "f64".into(),
        HirType::Handle(_) => "handle".into(),
        HirType::Array(inner) => format!("{}[]", fmt_ty(inner)),
        HirType::Function { .. } => "fn".into(),
        HirType::Class(id) => format!("class{}", id.0),
        HirType::Object => "obj".into(),
        HirType::Any => "any".into(),
        HirType::Unknown => "?".into(),
    }
}

fn fmt_inst(inst: &Inst) -> String {
    use Inst::*;
    match inst {
        IConst { dst, ty, val } => format!("v{} = iconst.{} {}", dst, fmt_ty(ty), val),
        F32Const { dst, val } => format!("v{} = f32const {}", dst, val),
        F64Const { dst, val } => format!("v{} = f64const {}", dst, val),
        IAdd { dst, lhs, rhs } => format!("v{} = iadd v{}, v{}", dst, lhs, rhs),
        IAddImm { dst, lhs, imm } => format!("v{} = iadd_imm v{}, {}", dst, lhs, imm),
        ISub { dst, lhs, rhs } => format!("v{} = isub v{}, v{}", dst, lhs, rhs),
        IMul { dst, lhs, rhs } => format!("v{} = imul v{}, v{}", dst, lhs, rhs),
        IMulImm { dst, lhs, imm } => format!("v{} = imul_imm v{}, {}", dst, lhs, imm),
        SDiv { dst, lhs, rhs } => format!("v{} = sdiv v{}, v{}", dst, lhs, rhs),
        UDiv { dst, lhs, rhs } => format!("v{} = udiv v{}, v{}", dst, lhs, rhs),
        SRem { dst, lhs, rhs } => format!("v{} = srem v{}, v{}", dst, lhs, rhs),
        URem { dst, lhs, rhs } => format!("v{} = urem v{}, v{}", dst, lhs, rhs),
        INeg { dst, src } => format!("v{} = ineg v{}", dst, src),
        BAnd { dst, lhs, rhs } => format!("v{} = band v{}, v{}", dst, lhs, rhs),
        BAndImm { dst, lhs, imm } => format!("v{} = band_imm v{}, {}", dst, lhs, imm),
        BOr { dst, lhs, rhs } => format!("v{} = bor v{}, v{}", dst, lhs, rhs),
        BOrImm { dst, lhs, imm } => format!("v{} = bor_imm v{}, {}", dst, lhs, imm),
        BXor { dst, lhs, rhs } => format!("v{} = bxor v{}, v{}", dst, lhs, rhs),
        BXorImm { dst, lhs, imm } => format!("v{} = bxor_imm v{}, {}", dst, lhs, imm),
        BNot { dst, src } => format!("v{} = bnot v{}", dst, src),
        IShl { dst, lhs, rhs } => format!("v{} = ishl v{}, v{}", dst, lhs, rhs),
        IShlImm { dst, lhs, imm } => format!("v{} = ishl_imm v{}, {}", dst, lhs, imm),
        UShr { dst, lhs, rhs } => format!("v{} = ushr v{}, v{}", dst, lhs, rhs),
        SShr { dst, lhs, rhs } => format!("v{} = sshr v{}, v{}", dst, lhs, rhs),
        SShrImm { dst, lhs, imm } => format!("v{} = sshr_imm v{}, {}", dst, lhs, imm),
        Rotl { dst, lhs, rhs } => format!("v{} = rotl v{}, v{}", dst, lhs, rhs),
        Rotr { dst, lhs, rhs } => format!("v{} = rotr v{}, v{}", dst, lhs, rhs),
        Clz { dst, src } => format!("v{} = clz v{}", dst, src),
        Ctz { dst, src } => format!("v{} = ctz v{}", dst, src),
        Popcnt { dst, src } => format!("v{} = popcnt v{}", dst, src),
        Bswap { dst, src } => format!("v{} = bswap v{}", dst, src),
        FAdd { dst, lhs, rhs } => format!("v{} = fadd v{}, v{}", dst, lhs, rhs),
        FSub { dst, lhs, rhs } => format!("v{} = fsub v{}, v{}", dst, lhs, rhs),
        FMul { dst, lhs, rhs } => format!("v{} = fmul v{}, v{}", dst, lhs, rhs),
        FDiv { dst, lhs, rhs } => format!("v{} = fdiv v{}, v{}", dst, lhs, rhs),
        FNeg { dst, src } => format!("v{} = fneg v{}", dst, src),
        FAbs { dst, src } => format!("v{} = fabs v{}", dst, src),
        Sqrt { dst, src } => format!("v{} = sqrt v{}", dst, src),
        Fma { dst, a, b, c } => format!("v{} = fma v{}, v{}, v{}", dst, a, b, c),
        FMin { dst, lhs, rhs } => format!("v{} = fmin v{}, v{}", dst, lhs, rhs),
        FMax { dst, lhs, rhs } => format!("v{} = fmax v{}, v{}", dst, lhs, rhs),
        Ceil { dst, src } => format!("v{} = ceil v{}", dst, src),
        Floor { dst, src } => format!("v{} = floor v{}", dst, src),
        Trunc { dst, src } => format!("v{} = trunc v{}", dst, src),
        IReduce { dst, src, to } => format!("v{} = ireduce.{} v{}", dst, fmt_ty(to), src),
        UExtend { dst, src, to } => format!("v{} = uextend.{} v{}", dst, fmt_ty(to), src),
        SExtend { dst, src, to } => format!("v{} = sextend.{} v{}", dst, fmt_ty(to), src),
        FPromote { dst, src } => format!("v{} = fpromote v{}", dst, src),
        FDemote { dst, src } => format!("v{} = fdemote v{}", dst, src),
        CvtFromSint { dst, src, to } => format!("v{} = fcvt_from_sint.{} v{}", dst, fmt_ty(to), src),
        CvtFromUint { dst, src, to } => format!("v{} = fcvt_from_uint.{} v{}", dst, fmt_ty(to), src),
        CvtToSintSat { dst, src, to } => format!("v{} = fcvt_to_sint_sat.{} v{}", dst, fmt_ty(to), src),
        CvtToUintSat { dst, src, to } => format!("v{} = fcvt_to_uint_sat.{} v{}", dst, fmt_ty(to), src),
        Bitcast { dst, src, to } => format!("v{} = bitcast.{} v{}", dst, fmt_ty(to), src),
        ICmp { dst, cond, lhs, rhs } => format!("v{} = icmp {} v{}, v{}", dst, fmt_intcc(cond), lhs, rhs),
        FCmp { dst, cond, lhs, rhs } => format!("v{} = fcmp {} v{}, v{}", dst, fmt_floatcc(cond), lhs, rhs),
        Load { dst, ty, ptr, offset, .. } => {
            format!("v{} = load.{} v{}+{}", dst, fmt_ty(ty), ptr, offset)
        }
        Store { val, ptr, offset, .. } => format!("store v{} -> v{}+{}", val, ptr, offset),
        ULoad8 { dst, ptr, offset } => format!("v{} = uload8 v{}+{}", dst, ptr, offset),
        ULoad16 { dst, ptr, offset } => format!("v{} = uload16 v{}+{}", dst, ptr, offset),
        ULoad32 { dst, ptr, offset } => format!("v{} = uload32 v{}+{}", dst, ptr, offset),
        SLoad8 { dst, ptr, offset } => format!("v{} = sload8 v{}+{}", dst, ptr, offset),
        SLoad16 { dst, ptr, offset } => format!("v{} = sload16 v{}+{}", dst, ptr, offset),
        SLoad32 { dst, ptr, offset } => format!("v{} = sload32 v{}+{}", dst, ptr, offset),
        IStore8 { val, ptr, offset } => format!("istore8 v{} -> v{}+{}", val, ptr, offset),
        IStore16 { val, ptr, offset } => format!("istore16 v{} -> v{}+{}", val, ptr, offset),
        IStore32 { val, ptr, offset } => format!("istore32 v{} -> v{}+{}", val, ptr, offset),
        StackAlloc { dst, size, align } => format!("v{} = stackalloc {} align {}", dst, size, align),
        Select { dst, cond, on_true, on_false } => {
            format!("v{} = select v{}, v{}, v{}", dst, cond, on_true, on_false)
        }
        CallExtern { dst, sym, args, .. } => {
            let args_s = args.iter().map(|v| format!("v{}", v)).collect::<Vec<_>>().join(", ");
            match dst {
                Some(d) => format!("v{} = call_extern {}({})", d, sym, args_s),
                None => format!("call_extern {}({})", sym, args_s),
            }
        }
        AtomicLoad { dst, ptr, .. } => format!("v{} = atomic_load v{}", dst, ptr),
        AtomicStore { val, ptr, .. } => format!("atomic_store v{} -> v{}", val, ptr),
        AtomicRmw { dst, op, ptr, val, .. } => {
            format!("v{} = atomic_rmw {:?} v{}, v{}", dst, op, ptr, val)
        }
        AtomicCas { dst, ptr, expected, replacement, .. } => {
            format!("v{} = atomic_cas v{}, exp v{}, rep v{}", dst, ptr, expected, replacement)
        }
        Fence { order } => format!("fence {:?}", order),
        DeclareGcValue { val } => format!("declare_gc v{}", val),
        CallUser { dst, name, args, ret_ty, .. } => {
            let args_s = args.iter().map(|v| format!("v{}", v)).collect::<Vec<_>>().join(", ");
            match dst {
                Some(d) => format!("v{} = call_user {}({}) -> {}", d, name, args_s, fmt_ty(ret_ty)),
                None => format!("call_user {}({})", name, args_s),
            }
        }
    }
}

fn fmt_term(term: &Terminator) -> String {
    match term {
        Terminator::Return(vs) => {
            let vs_s = vs.iter().map(|v| format!("v{}", v)).collect::<Vec<_>>().join(", ");
            format!("return {}", vs_s)
        }
        Terminator::Jump { target, args } => {
            let args_s = args.iter().map(|v| format!("v{}", v)).collect::<Vec<_>>().join(", ");
            if args.is_empty() {
                format!("jump block{}", target)
            } else {
                format!("jump block{}({})", target, args_s)
            }
        }
        Terminator::Brif { cond, then_block, then_args, else_block, else_args } => {
            let t = if then_args.is_empty() {
                format!("block{}", then_block)
            } else {
                let s = then_args.iter().map(|v| format!("v{}", v)).collect::<Vec<_>>().join(", ");
                format!("block{}({})", then_block, s)
            };
            let e = if else_args.is_empty() {
                format!("block{}", else_block)
            } else {
                let s = else_args.iter().map(|v| format!("v{}", v)).collect::<Vec<_>>().join(", ");
                format!("block{}({})", else_block, s)
            };
            format!("brif v{}, {}, {}", cond, t, e)
        }
        Terminator::Switch { index, default, cases } => {
            let cs = cases.iter().map(|(k, b)| format!("{} -> block{}", k, b)).collect::<Vec<_>>().join(", ");
            format!("switch v{}, default block{}, [{}]", index, default, cs)
        }
        Terminator::TailCall { sym, args } => {
            let args_s = args.iter().map(|v| format!("v{}", v)).collect::<Vec<_>>().join(", ");
            format!("tail_call {}({})", sym, args_s)
        }
        Terminator::TailCallIndirect { fn_ptr, args } => {
            let args_s = args.iter().map(|v| format!("v{}", v)).collect::<Vec<_>>().join(", ");
            format!("tail_call_indirect v{}({})", fn_ptr, args_s)
        }
        Terminator::Trap { code } => format!("trap {:?}", code),
    }
}

fn fmt_intcc(c: &IntCond) -> &'static str {
    match c {
        IntCond::Eq => "eq",
        IntCond::Ne => "ne",
        IntCond::Slt => "slt",
        IntCond::Sle => "sle",
        IntCond::Sgt => "sgt",
        IntCond::Sge => "sge",
        IntCond::Ult => "ult",
        IntCond::Ule => "ule",
        IntCond::Ugt => "ugt",
        IntCond::Uge => "uge",
    }
}

fn fmt_floatcc(c: &FloatCond) -> &'static str {
    match c {
        FloatCond::Eq => "eq",
        FloatCond::Ne => "ne",
        FloatCond::OLt => "olt",
        FloatCond::OLe => "ole",
        FloatCond::OGt => "ogt",
        FloatCond::OGe => "oge",
        FloatCond::ONe => "one",
        FloatCond::ULt => "ult",
        FloatCond::ULe => "ule",
        FloatCond::UGt => "ugt",
        FloatCond::UGe => "uge",
        FloatCond::UNe => "une",
        FloatCond::Ordered => "ord",
        FloatCond::Unordered => "uno",
    }
}
