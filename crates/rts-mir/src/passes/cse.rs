//! Common Subexpression Elimination intra-bloco.
//!
//! Para cada bloco, mantém um mapa `key → ValueId` onde `key` é uma
//! representação canônica de uma instrução pura sem efeitos colaterais
//! (operação + operandos). Quando uma instrução com a mesma chave reaparece,
//! seu `dst` é substituído pelo primeiro `dst` (via remap em todas as Insts
//! subsequentes do bloco e no terminator).
//!
//! Cobre: IConst, F32Const, F64Const, IAdd/ISub/IMul/SDiv/UDiv/SRem/URem,
//! IAddImm/IMulImm, BAnd/BOr/BXor/BAndImm/BOrImm/BXorImm, IShl/SShr/UShr/
//! IShlImm/SShrImm, INeg/BNot, ICmp/FCmp, FAdd/FSub/FMul/FDiv, FNeg/FAbs/Sqrt,
//! Ceil/Floor/Trunc, IReduce/UExtend/SExtend/FPromote/FDemote/Cvt*/Bitcast,
//! Clz/Ctz/Popcnt/Bswap.
//!
//! Não cobre: Load/Store (memory side effects), CallExtern/CallUser
//! (side effects), Atomic*, Select (caller pode CSE-ar manualmente),
//! StackAlloc (cada chamada deve ter slot único), DeclareGcValue (marker).

use std::collections::HashMap;

use rts_hir::HirType;

use crate::ir::*;

/// Chave canônica para detectar instruções equivalentes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    IConst(HirType, i64),
    F32Const(u32),
    F64Const(u64),
    Bin(BinKind, ValueId, ValueId),
    BinImm(BinImmKind, ValueId, i64),
    Unary(UnaryKind, ValueId),
    Cmp(CmpKind, ValueId, ValueId),
    Conv(ConvKind, ValueId, HirType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum BinKind {
    IAdd, ISub, IMul, SDiv, UDiv, SRem, URem,
    BAnd, BOr, BXor,
    IShl, UShr, SShr, Rotl, Rotr,
    FAdd, FSub, FMul, FDiv, FMin, FMax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum BinImmKind {
    IAddImm, IMulImm, BAndImm, BOrImm, BXorImm, IShlImm, SShrImm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum UnaryKind {
    INeg, BNot, FNeg, FAbs, Sqrt, Ceil, Floor, Trunc,
    Clz, Ctz, Popcnt, Bswap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CmpKind {
    Icmp(IntCond),
    Fcmp(FloatCond),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ConvKind {
    IReduce, UExtend, SExtend, FPromote, FDemote,
    CvtFromSint, CvtFromUint, CvtToSintSat, CvtToUintSat, Bitcast,
}

/// Roda CSE em todos os blocos do `mir`. Retorna `true` se algo foi
/// dedupliicado.
pub fn cse(mir: &mut MirFunc) -> bool {
    let mut changed = false;
    for block in &mut mir.blocks {
        if cse_block(block) {
            changed = true;
        }
    }
    changed
}

fn cse_block(block: &mut BasicBlock) -> bool {
    let mut seen: HashMap<Key, ValueId> = HashMap::new();
    // Remap aliasado: dst antigo → primeiro dst encontrado pra mesma key.
    let mut remap: HashMap<ValueId, ValueId> = HashMap::new();
    let mut changed = false;

    let n = block.insts.len();
    for i in 0..n {
        // Aplica remap em todos os operandos da inst antes de checar a key.
        remap_operands(&mut block.insts[i], &remap);

        if let Some(key) = inst_key(&block.insts[i]) {
            let dst = inst_dst(&block.insts[i]).expect("key implies dst");
            if let Some(&first) = seen.get(&key) {
                // Duplicate: alias dst → first. Substitui esta Inst por
                // IAddImm 0 (alias trivial, será removido pelo DCE).
                if first != dst {
                    remap.insert(dst, first);
                    block.insts[i] = Inst::IAddImm {
                        dst,
                        lhs: first,
                        imm: 0,
                    };
                    changed = true;
                }
            } else {
                seen.insert(key, dst);
            }
        }
    }
    // Remap operandos do terminator.
    remap_term_operands(&mut block.term, &remap);
    changed
}

fn inst_key(inst: &Inst) -> Option<Key> {
    use Inst::*;
    Some(match inst {
        IConst { ty, val, .. } => Key::IConst(ty.clone(), *val),
        F32Const { val, .. } => Key::F32Const(val.to_bits()),
        F64Const { val, .. } => Key::F64Const(val.to_bits()),

        IAdd { lhs, rhs, .. } => Key::Bin(BinKind::IAdd, *lhs, *rhs),
        ISub { lhs, rhs, .. } => Key::Bin(BinKind::ISub, *lhs, *rhs),
        IMul { lhs, rhs, .. } => Key::Bin(BinKind::IMul, *lhs, *rhs),
        SDiv { lhs, rhs, .. } => Key::Bin(BinKind::SDiv, *lhs, *rhs),
        UDiv { lhs, rhs, .. } => Key::Bin(BinKind::UDiv, *lhs, *rhs),
        SRem { lhs, rhs, .. } => Key::Bin(BinKind::SRem, *lhs, *rhs),
        URem { lhs, rhs, .. } => Key::Bin(BinKind::URem, *lhs, *rhs),
        BAnd { lhs, rhs, .. } => Key::Bin(BinKind::BAnd, *lhs, *rhs),
        BOr { lhs, rhs, .. } => Key::Bin(BinKind::BOr, *lhs, *rhs),
        BXor { lhs, rhs, .. } => Key::Bin(BinKind::BXor, *lhs, *rhs),
        IShl { lhs, rhs, .. } => Key::Bin(BinKind::IShl, *lhs, *rhs),
        UShr { lhs, rhs, .. } => Key::Bin(BinKind::UShr, *lhs, *rhs),
        SShr { lhs, rhs, .. } => Key::Bin(BinKind::SShr, *lhs, *rhs),
        Rotl { lhs, rhs, .. } => Key::Bin(BinKind::Rotl, *lhs, *rhs),
        Rotr { lhs, rhs, .. } => Key::Bin(BinKind::Rotr, *lhs, *rhs),
        FAdd { lhs, rhs, .. } => Key::Bin(BinKind::FAdd, *lhs, *rhs),
        FSub { lhs, rhs, .. } => Key::Bin(BinKind::FSub, *lhs, *rhs),
        FMul { lhs, rhs, .. } => Key::Bin(BinKind::FMul, *lhs, *rhs),
        FDiv { lhs, rhs, .. } => Key::Bin(BinKind::FDiv, *lhs, *rhs),
        FMin { lhs, rhs, .. } => Key::Bin(BinKind::FMin, *lhs, *rhs),
        FMax { lhs, rhs, .. } => Key::Bin(BinKind::FMax, *lhs, *rhs),

        IAddImm { lhs, imm, .. } => Key::BinImm(BinImmKind::IAddImm, *lhs, *imm),
        IMulImm { lhs, imm, .. } => Key::BinImm(BinImmKind::IMulImm, *lhs, *imm),
        BAndImm { lhs, imm, .. } => Key::BinImm(BinImmKind::BAndImm, *lhs, *imm),
        BOrImm { lhs, imm, .. } => Key::BinImm(BinImmKind::BOrImm, *lhs, *imm),
        BXorImm { lhs, imm, .. } => Key::BinImm(BinImmKind::BXorImm, *lhs, *imm),
        IShlImm { lhs, imm, .. } => Key::BinImm(BinImmKind::IShlImm, *lhs, *imm),
        SShrImm { lhs, imm, .. } => Key::BinImm(BinImmKind::SShrImm, *lhs, *imm),

        INeg { src, .. } => Key::Unary(UnaryKind::INeg, *src),
        BNot { src, .. } => Key::Unary(UnaryKind::BNot, *src),
        FNeg { src, .. } => Key::Unary(UnaryKind::FNeg, *src),
        FAbs { src, .. } => Key::Unary(UnaryKind::FAbs, *src),
        Sqrt { src, .. } => Key::Unary(UnaryKind::Sqrt, *src),
        Ceil { src, .. } => Key::Unary(UnaryKind::Ceil, *src),
        Floor { src, .. } => Key::Unary(UnaryKind::Floor, *src),
        Trunc { src, .. } => Key::Unary(UnaryKind::Trunc, *src),
        Clz { src, .. } => Key::Unary(UnaryKind::Clz, *src),
        Ctz { src, .. } => Key::Unary(UnaryKind::Ctz, *src),
        Popcnt { src, .. } => Key::Unary(UnaryKind::Popcnt, *src),
        Bswap { src, .. } => Key::Unary(UnaryKind::Bswap, *src),

        ICmp { cond, lhs, rhs, .. } => Key::Cmp(CmpKind::Icmp(*cond), *lhs, *rhs),
        FCmp { cond, lhs, rhs, .. } => Key::Cmp(CmpKind::Fcmp(*cond), *lhs, *rhs),

        IReduce { src, to, .. } => Key::Conv(ConvKind::IReduce, *src, to.clone()),
        UExtend { src, to, .. } => Key::Conv(ConvKind::UExtend, *src, to.clone()),
        SExtend { src, to, .. } => Key::Conv(ConvKind::SExtend, *src, to.clone()),
        FPromote { src, .. } => Key::Conv(ConvKind::FPromote, *src, HirType::F64),
        FDemote { src, .. } => Key::Conv(ConvKind::FDemote, *src, HirType::F32),
        CvtFromSint { src, to, .. } => Key::Conv(ConvKind::CvtFromSint, *src, to.clone()),
        CvtFromUint { src, to, .. } => Key::Conv(ConvKind::CvtFromUint, *src, to.clone()),
        CvtToSintSat { src, to, .. } => Key::Conv(ConvKind::CvtToSintSat, *src, to.clone()),
        CvtToUintSat { src, to, .. } => Key::Conv(ConvKind::CvtToUintSat, *src, to.clone()),
        Bitcast { src, to, .. } => Key::Conv(ConvKind::Bitcast, *src, to.clone()),

        // Side-effecting ou polimórficos: não CSE.
        _ => return None,
    })
}

fn inst_dst(inst: &Inst) -> Option<ValueId> {
    use Inst::*;
    Some(match inst {
        IConst { dst, .. }
        | F32Const { dst, .. }
        | F64Const { dst, .. }
        | IAdd { dst, .. }
        | ISub { dst, .. }
        | IMul { dst, .. }
        | SDiv { dst, .. }
        | UDiv { dst, .. }
        | SRem { dst, .. }
        | URem { dst, .. }
        | INeg { dst, .. }
        | BAnd { dst, .. }
        | BOr { dst, .. }
        | BXor { dst, .. }
        | BNot { dst, .. }
        | IShl { dst, .. }
        | UShr { dst, .. }
        | SShr { dst, .. }
        | Rotl { dst, .. }
        | Rotr { dst, .. }
        | Clz { dst, .. }
        | Ctz { dst, .. }
        | Popcnt { dst, .. }
        | Bswap { dst, .. }
        | FAdd { dst, .. }
        | FSub { dst, .. }
        | FMul { dst, .. }
        | FDiv { dst, .. }
        | FNeg { dst, .. }
        | FAbs { dst, .. }
        | Sqrt { dst, .. }
        | Fma { dst, .. }
        | FMin { dst, .. }
        | FMax { dst, .. }
        | Ceil { dst, .. }
        | Floor { dst, .. }
        | Trunc { dst, .. }
        | IReduce { dst, .. }
        | UExtend { dst, .. }
        | SExtend { dst, .. }
        | FPromote { dst, .. }
        | FDemote { dst, .. }
        | CvtFromSint { dst, .. }
        | CvtFromUint { dst, .. }
        | CvtToSintSat { dst, .. }
        | CvtToUintSat { dst, .. }
        | Bitcast { dst, .. }
        | ICmp { dst, .. }
        | FCmp { dst, .. }
        | IAddImm { dst, .. }
        | IMulImm { dst, .. }
        | BAndImm { dst, .. }
        | BOrImm { dst, .. }
        | BXorImm { dst, .. }
        | IShlImm { dst, .. }
        | SShrImm { dst, .. } => *dst,
        _ => return None,
    })
}

fn map_v(v: &mut ValueId, remap: &HashMap<ValueId, ValueId>) {
    if let Some(&new) = remap.get(v) {
        *v = new;
    }
}

fn remap_operands(inst: &mut Inst, remap: &HashMap<ValueId, ValueId>) {
    use Inst::*;
    match inst {
        IAdd { lhs, rhs, .. }
        | ISub { lhs, rhs, .. }
        | IMul { lhs, rhs, .. }
        | SDiv { lhs, rhs, .. }
        | UDiv { lhs, rhs, .. }
        | SRem { lhs, rhs, .. }
        | URem { lhs, rhs, .. }
        | BAnd { lhs, rhs, .. }
        | BOr { lhs, rhs, .. }
        | BXor { lhs, rhs, .. }
        | IShl { lhs, rhs, .. }
        | UShr { lhs, rhs, .. }
        | SShr { lhs, rhs, .. }
        | Rotl { lhs, rhs, .. }
        | Rotr { lhs, rhs, .. }
        | FAdd { lhs, rhs, .. }
        | FSub { lhs, rhs, .. }
        | FMul { lhs, rhs, .. }
        | FDiv { lhs, rhs, .. }
        | FMin { lhs, rhs, .. }
        | FMax { lhs, rhs, .. }
        | ICmp { lhs, rhs, .. }
        | FCmp { lhs, rhs, .. } => {
            map_v(lhs, remap);
            map_v(rhs, remap);
        }
        IAddImm { lhs, .. }
        | IMulImm { lhs, .. }
        | BAndImm { lhs, .. }
        | BOrImm { lhs, .. }
        | BXorImm { lhs, .. }
        | IShlImm { lhs, .. }
        | SShrImm { lhs, .. } => map_v(lhs, remap),
        INeg { src, .. }
        | BNot { src, .. }
        | Clz { src, .. }
        | Ctz { src, .. }
        | Popcnt { src, .. }
        | Bswap { src, .. }
        | FNeg { src, .. }
        | FAbs { src, .. }
        | Sqrt { src, .. }
        | Ceil { src, .. }
        | Floor { src, .. }
        | Trunc { src, .. }
        | IReduce { src, .. }
        | UExtend { src, .. }
        | SExtend { src, .. }
        | FPromote { src, .. }
        | FDemote { src, .. }
        | CvtFromSint { src, .. }
        | CvtFromUint { src, .. }
        | CvtToSintSat { src, .. }
        | CvtToUintSat { src, .. }
        | Bitcast { src, .. } => map_v(src, remap),
        Fma { a, b, c, .. } => {
            map_v(a, remap);
            map_v(b, remap);
            map_v(c, remap);
        }
        Load { ptr, .. } | ULoad8 { ptr, .. } | ULoad16 { ptr, .. } | ULoad32 { ptr, .. }
        | SLoad8 { ptr, .. } | SLoad16 { ptr, .. } | SLoad32 { ptr, .. } => map_v(ptr, remap),
        Store { val, ptr, .. }
        | IStore8 { val, ptr, .. }
        | IStore16 { val, ptr, .. }
        | IStore32 { val, ptr, .. } => {
            map_v(val, remap);
            map_v(ptr, remap);
        }
        Select { cond, on_true, on_false, .. } => {
            map_v(cond, remap);
            map_v(on_true, remap);
            map_v(on_false, remap);
        }
        CallExtern { args, .. } | CallUser { args, .. } => {
            for a in args {
                map_v(a, remap);
            }
        }
        AtomicLoad { ptr, .. } => map_v(ptr, remap),
        AtomicStore { val, ptr, .. } => {
            map_v(val, remap);
            map_v(ptr, remap);
        }
        AtomicRmw { ptr, val, .. } => {
            map_v(ptr, remap);
            map_v(val, remap);
        }
        AtomicCas { ptr, expected, replacement, .. } => {
            map_v(ptr, remap);
            map_v(expected, remap);
            map_v(replacement, remap);
        }
        DeclareGcValue { val } => map_v(val, remap),
        VSplat { src, .. } => map_v(src, remap),
        VExtractLane { src, .. } => map_v(src, remap),
        VInsertLane { src, val, .. } => { map_v(src, remap); map_v(val, remap); }
        VAdd { lhs, rhs, .. } | VSub { lhs, rhs, .. } | VMul { lhs, rhs, .. } => {
            map_v(lhs, remap); map_v(rhs, remap);
        }
        IConst { .. } | F32Const { .. } | F64Const { .. }
        | StackAlloc { .. } | Fence { .. } | StrLit { .. } => {}
    }
}

fn remap_term_operands(term: &mut Terminator, remap: &HashMap<ValueId, ValueId>) {
    match term {
        Terminator::Return(vs) => {
            for v in vs {
                map_v(v, remap);
            }
        }
        Terminator::Jump { args, .. } => {
            for v in args {
                map_v(v, remap);
            }
        }
        Terminator::Brif { cond, then_args, else_args, .. } => {
            map_v(cond, remap);
            for v in then_args {
                map_v(v, remap);
            }
            for v in else_args {
                map_v(v, remap);
            }
        }
        Terminator::Switch { index, .. } => map_v(index, remap),
        Terminator::TailCall { args, .. } => {
            for v in args {
                map_v(v, remap);
            }
        }
        Terminator::TailCallIndirect { fn_ptr, args } => {
            map_v(fn_ptr, remap);
            for v in args {
                map_v(v, remap);
            }
        }
        Terminator::Trap { .. } => {}
    }
}
