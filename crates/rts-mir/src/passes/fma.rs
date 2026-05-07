//! Detecta padrão `a*b + c` em float e fundede em `Inst::Fma` (Fused
//! Multiply-Add). FMA emite uma única instrução nativa em ISAs que
//! suportam (x86_64 com FMA3, ARM com NEON FMLA), maior throughput
//! e maior precisão (sem rounding intermediário).
//!
//! O pass é conservador: só funde quando o `FMul` produz um `dst`
//! usado **exatamente uma vez** dentro do mesmo bloco — caso
//! contrário substituir o produto por FMA criaria uso fora de FMA.
//! Para evitar tracking de uses globais, conto uses no bloco e
//! aplico só quando tem 1 use que é um FAdd.

use std::collections::HashMap;

use crate::ir::*;

/// Roda a fusão FMA em todos os blocos do `mir`. Retorna `true` se
/// algo foi fundido.
pub fn fma(mir: &mut MirFunc) -> bool {
    let mut changed = false;
    for block in &mut mir.blocks {
        if fma_block(block) {
            changed = true;
        }
    }
    changed
}

fn fma_block(block: &mut BasicBlock) -> bool {
    // Conta uses dentro do bloco (Insts + Terminator).
    let uses = count_uses(block);

    // Mapa dst → (idx, FMul args) pra Insts FMul.
    let mut fmul_at: HashMap<ValueId, (usize, ValueId, ValueId)> = HashMap::new();
    for (i, inst) in block.insts.iter().enumerate() {
        if let Inst::FMul { dst, lhs, rhs } = inst {
            fmul_at.insert(*dst, (i, *lhs, *rhs));
        }
    }

    let mut changed = false;
    let mut to_remove: Vec<usize> = Vec::new();

    for i in 0..block.insts.len() {
        // Procura `FAdd { lhs: fmul_dst, rhs: c }` ou `FAdd { lhs: c, rhs: fmul_dst }`
        // onde fmul_dst tem exatamente 1 use.
        if let Inst::FAdd { dst, lhs, rhs } = block.insts[i] {
            // Tenta lhs = FMul.dst
            if let Some(&(mul_idx, mul_lhs, mul_rhs)) = fmul_at.get(&lhs) {
                if uses.get(&lhs).copied().unwrap_or(0) == 1 && mul_idx < i {
                    // Substitui FAdd por Fma a*b + rhs.
                    block.insts[i] = Inst::Fma {
                        dst,
                        a: mul_lhs,
                        b: mul_rhs,
                        c: rhs,
                    };
                    to_remove.push(mul_idx);
                    changed = true;
                    continue;
                }
            }
            // Tenta rhs = FMul.dst (commutativa)
            if let Some(&(mul_idx, mul_lhs, mul_rhs)) = fmul_at.get(&rhs) {
                if uses.get(&rhs).copied().unwrap_or(0) == 1 && mul_idx < i {
                    block.insts[i] = Inst::Fma {
                        dst,
                        a: mul_lhs,
                        b: mul_rhs,
                        c: lhs,
                    };
                    to_remove.push(mul_idx);
                    changed = true;
                }
            }
        }
    }

    // Remove os FMuls fundidos. Iterar em ordem reversa pra preservar
    // os índices.
    to_remove.sort_unstable();
    to_remove.dedup();
    for &idx in to_remove.iter().rev() {
        block.insts.remove(idx);
    }

    changed
}

/// Conta uses (não dst) de cada ValueId no bloco — Insts + Terminator.
fn count_uses(block: &BasicBlock) -> HashMap<ValueId, usize> {
    let mut uses: HashMap<ValueId, usize> = HashMap::new();
    let mut bump = |v: ValueId, m: &mut HashMap<ValueId, usize>| {
        *m.entry(v).or_insert(0) += 1;
    };
    for inst in &block.insts {
        for_each_operand(inst, |v| bump(v, &mut uses));
    }
    for_each_term_operand(&block.term, |v| bump(v, &mut uses));
    uses
}

fn for_each_operand<F: FnMut(ValueId)>(inst: &Inst, mut f: F) {
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
            f(*lhs);
            f(*rhs);
        }
        IAddImm { lhs, .. } | IMulImm { lhs, .. } | BAndImm { lhs, .. }
        | BOrImm { lhs, .. } | BXorImm { lhs, .. } | IShlImm { lhs, .. }
        | SShrImm { lhs, .. } => f(*lhs),
        INeg { src, .. } | BNot { src, .. } | Clz { src, .. } | Ctz { src, .. }
        | Popcnt { src, .. } | Bswap { src, .. } | FNeg { src, .. } | FAbs { src, .. }
        | Sqrt { src, .. } | Ceil { src, .. } | Floor { src, .. } | Trunc { src, .. }
        | IReduce { src, .. } | UExtend { src, .. } | SExtend { src, .. }
        | FPromote { src, .. } | FDemote { src, .. } | CvtFromSint { src, .. }
        | CvtFromUint { src, .. } | CvtToSintSat { src, .. } | CvtToUintSat { src, .. }
        | Bitcast { src, .. } => f(*src),
        Fma { a, b, c, .. } => {
            f(*a); f(*b); f(*c);
        }
        Load { ptr, .. } | ULoad8 { ptr, .. } | ULoad16 { ptr, .. } | ULoad32 { ptr, .. }
        | SLoad8 { ptr, .. } | SLoad16 { ptr, .. } | SLoad32 { ptr, .. } => f(*ptr),
        Store { val, ptr, .. } | IStore8 { val, ptr, .. } | IStore16 { val, ptr, .. }
        | IStore32 { val, ptr, .. } => {
            f(*val); f(*ptr);
        }
        Select { cond, on_true, on_false, .. } => {
            f(*cond); f(*on_true); f(*on_false);
        }
        CallExtern { args, .. } | CallUser { args, .. } => {
            for a in args { f(*a); }
        }
        AtomicLoad { ptr, .. } => f(*ptr),
        AtomicStore { val, ptr, .. } => { f(*val); f(*ptr); }
        AtomicRmw { ptr, val, .. } => { f(*ptr); f(*val); }
        AtomicCas { ptr, expected, replacement, .. } => {
            f(*ptr); f(*expected); f(*replacement);
        }
        DeclareGcValue { val } => f(*val),
        IConst { .. } | F32Const { .. } | F64Const { .. }
        | StackAlloc { .. } | Fence { .. } | StrLit { .. } => {}
    }
}

fn for_each_term_operand<F: FnMut(ValueId)>(term: &Terminator, mut f: F) {
    match term {
        Terminator::Return(vs) => { for v in vs { f(*v); } }
        Terminator::Jump { args, .. } => { for v in args { f(*v); } }
        Terminator::Brif { cond, then_args, else_args, .. } => {
            f(*cond);
            for v in then_args { f(*v); }
            for v in else_args { f(*v); }
        }
        Terminator::Switch { index, .. } => f(*index),
        Terminator::TailCall { args, .. } => { for v in args { f(*v); } }
        Terminator::TailCallIndirect { fn_ptr, args } => {
            f(*fn_ptr);
            for v in args { f(*v); }
        }
        Terminator::Trap { .. } => {}
    }
}
