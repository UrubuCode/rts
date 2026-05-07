//! Inlining de fns pequenas — substitui `Inst::CallUser` por copia do
//! corpo do callee inserido in-loco no caller.
//!
//! ## Heuristica de elegibilidade
//!
//! Uma fn callee `f` é elegível para inlining quando:
//!
//! 1. Tem exatamente 1 bloco (sem control flow interno).
//! 2. O Terminator do bloco é `Return(Some(v))` ou `Return(vec![])`.
//! 3. O número de Insts é ≤ `INLINE_BUDGET` (default 16).
//! 4. Não tem `CallUser` para si mesma (auto-recursão).
//! 5. Não tem `CallExtern`, `StrLit`, `AtomicX`, `Fence`, `DeclareGcValue`,
//!    `StackAlloc` — Insts com side effects globais ou ABI cross-fn.
//! 6. Tem assinatura compatível com a chamada (mesma arity, tipos
//!    casam até `==` no `HirType`).
//!
//! Quando inline acontece:
//! - Aloca novos `ValueId` no caller para cada value do callee, criando
//!   um mapeamento `callee_vid → caller_vid`.
//! - Os params do callee são "ligados" aos args da chamada (bypass — o
//!   ValueId no map aponta direto pro arg do caller).
//! - Cada Inst do callee é clonado com seus operands remapeados pelo
//!   mapping e inserido no caller no lugar da CallUser.
//! - O `dst` da CallUser é remapeado pra apontar pro Return value do
//!   callee no map (novo).
//!
//! Termiantor `Return(Some(v))` → `dst = mapped(v)`.
//! Terminator `Return(vec![])` (void) → `dst` da CallUser fica zero const
//! (não deve haver `dst = Some` quando o callee é void; gate filtra).

use std::collections::HashMap;

use rts_hir::HirType;

use crate::ir::*;

/// Budget de Insts por fn elegível. Acima disso bail.
pub const INLINE_BUDGET: usize = 16;

/// Aplica inlining no `caller` consultando `callees` (name → MirFunc).
/// Retorna `true` se algum CallUser foi inlinado, senão `false`.
pub fn inline(caller: &mut MirFunc, callees: &HashMap<String, MirFunc>) -> bool {
    let mut changed = false;
    for block_idx in 0..caller.blocks.len() {
        let mut new_insts: Vec<Inst> = Vec::with_capacity(caller.blocks[block_idx].insts.len());
        let drained: Vec<Inst> =
            std::mem::take(&mut caller.blocks[block_idx].insts);
        for inst in drained {
            if let Inst::CallUser {
                dst,
                name,
                args,
                ret_ty,
                ..
            } = &inst
            {
                if let Some(callee) = callees.get(name) {
                    if eligible(callee, name, args.len()) {
                        if let Some(inlined) = build_inline(
                            caller, callee, args, dst.as_ref().copied(), ret_ty,
                        ) {
                            new_insts.extend(inlined);
                            changed = true;
                            continue;
                        }
                    }
                }
            }
            new_insts.push(inst);
        }
        caller.blocks[block_idx].insts = new_insts;
    }
    changed
}

fn eligible(callee: &MirFunc, name: &str, arity: usize) -> bool {
    // Single block, single return.
    if callee.blocks.len() != 1 {
        return false;
    }
    let block = &callee.blocks[0];
    // Aridade casa.
    if callee.params.len() != arity {
        return false;
    }
    // Auto-recursão / side effects → bail.
    for inst in &block.insts {
        match inst {
            Inst::CallUser { name: n, .. } if n == name => return false,
            Inst::CallExtern { .. }
            | Inst::StrLit { .. }
            | Inst::AtomicLoad { .. }
            | Inst::AtomicStore { .. }
            | Inst::AtomicRmw { .. }
            | Inst::AtomicCas { .. }
            | Inst::Fence { .. }
            | Inst::DeclareGcValue { .. }
            | Inst::StackAlloc { .. } => return false,
            _ => {}
        }
    }
    // Inst budget.
    if block.insts.len() > INLINE_BUDGET {
        return false;
    }
    // Terminator deve ser Return.
    matches!(block.term, Terminator::Return(_))
}

/// Constrói a sequência de Insts inlinados pra substituir uma CallUser.
/// Retorna `None` se algum mapping falhar (ex.: Return sem valor mas dst
/// é Some).
fn build_inline(
    caller: &mut MirFunc,
    callee: &MirFunc,
    args: &[ValueId],
    dst: Option<ValueId>,
    _ret_ty: &HirType,
) -> Option<Vec<Inst>> {
    let block = &callee.blocks[0];

    // Mapping callee_vid → caller_vid.
    let mut map: HashMap<ValueId, ValueId> = HashMap::new();

    // Params do callee → args do caller (bypass direto).
    for (i, (param_vid, _ty)) in callee.params.iter().enumerate() {
        map.insert(*param_vid, args[i]);
    }

    // Block params do entry block — também viram aliases pros args.
    // (Em MIR atual o entry block params duplicam os fn params.)
    for (i, (param_vid, _ty)) in block.params.iter().enumerate() {
        if i < args.len() {
            map.insert(*param_vid, args[i]);
        }
    }

    let mut out: Vec<Inst> = Vec::with_capacity(block.insts.len());

    // Helper: remapa um ValueId — se está no map, retorna o caller value;
    // senão aloca novo no caller usando o tipo do callee.
    fn remap(
        v: ValueId,
        callee: &MirFunc,
        caller: &mut MirFunc,
        map: &mut HashMap<ValueId, ValueId>,
    ) -> ValueId {
        if let Some(&existing) = map.get(&v) {
            return existing;
        }
        let ty = callee
            .values
            .get(v as usize)
            .cloned()
            .unwrap_or(HirType::I64);
        let new_v = caller.new_value(ty);
        map.insert(v, new_v);
        new_v
    }

    // Clona cada Inst remapeando todos os ValueIds.
    for inst in &block.insts {
        let new_inst = clone_remapped(inst, callee, caller, &mut map)?;
        out.push(new_inst);
    }

    // Tratar Return: dst da CallUser recebe o valor retornado.
    if let Terminator::Return(rets) = &block.term {
        if let Some(d) = dst {
            if let Some(&ret_v) = rets.first() {
                let mapped = remap(ret_v, callee, caller, &mut map);
                // Bind via Bitcast pra mesma fn — mas isso aloca novo Value.
                // Mais simples: se dst != mapped, emite um IConst+IAdd pra
                // alias. Solução cleaner: caller já tinha alocado dst antes
                // da CallUser; garantimos que o último valor produzido
                // bate com `dst`. Como o caller usa `dst` em outros lugares,
                // precisamos que o ValueId `dst` exista no MIR. Vamos
                // reescrever o último Inst de `out` pra usar `dst` em vez
                // do `mapped` quando possível.
                rebind_last_to_dst(&mut out, mapped, d);
            } else {
                return None; // void return mas dst Some — incompatível
            }
        }
    } else {
        return None;
    }
    Some(out)
}

fn clone_remapped(
    inst: &Inst,
    callee: &MirFunc,
    caller: &mut MirFunc,
    map: &mut HashMap<ValueId, ValueId>,
) -> Option<Inst> {
    use Inst::*;
    let r = |v: ValueId, map: &mut HashMap<ValueId, ValueId>, caller: &mut MirFunc| {
        if let Some(&existing) = map.get(&v) {
            existing
        } else {
            let ty = callee
                .values
                .get(v as usize)
                .cloned()
                .unwrap_or(HirType::I64);
            let new_v = caller.new_value(ty);
            map.insert(v, new_v);
            new_v
        }
    };
    Some(match inst.clone() {
        IConst { dst, ty, val } => IConst { dst: r(dst, map, caller), ty, val },
        F32Const { dst, val } => F32Const { dst: r(dst, map, caller), val },
        F64Const { dst, val } => F64Const { dst: r(dst, map, caller), val },
        IAdd { dst, lhs, rhs } => IAdd { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        IAddImm { dst, lhs, imm } => IAddImm { dst: r(dst, map, caller), lhs: r(lhs, map, caller), imm },
        ISub { dst, lhs, rhs } => ISub { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        IMul { dst, lhs, rhs } => IMul { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        IMulImm { dst, lhs, imm } => IMulImm { dst: r(dst, map, caller), lhs: r(lhs, map, caller), imm },
        SDiv { dst, lhs, rhs } => SDiv { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        UDiv { dst, lhs, rhs } => UDiv { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        SRem { dst, lhs, rhs } => SRem { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        URem { dst, lhs, rhs } => URem { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        INeg { dst, src } => INeg { dst: r(dst, map, caller), src: r(src, map, caller) },
        BAnd { dst, lhs, rhs } => BAnd { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        BAndImm { dst, lhs, imm } => BAndImm { dst: r(dst, map, caller), lhs: r(lhs, map, caller), imm },
        BOr { dst, lhs, rhs } => BOr { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        BOrImm { dst, lhs, imm } => BOrImm { dst: r(dst, map, caller), lhs: r(lhs, map, caller), imm },
        BXor { dst, lhs, rhs } => BXor { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        BXorImm { dst, lhs, imm } => BXorImm { dst: r(dst, map, caller), lhs: r(lhs, map, caller), imm },
        BNot { dst, src } => BNot { dst: r(dst, map, caller), src: r(src, map, caller) },
        IShl { dst, lhs, rhs } => IShl { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        IShlImm { dst, lhs, imm } => IShlImm { dst: r(dst, map, caller), lhs: r(lhs, map, caller), imm },
        UShr { dst, lhs, rhs } => UShr { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        SShr { dst, lhs, rhs } => SShr { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        SShrImm { dst, lhs, imm } => SShrImm { dst: r(dst, map, caller), lhs: r(lhs, map, caller), imm },
        Rotl { dst, lhs, rhs } => Rotl { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        Rotr { dst, lhs, rhs } => Rotr { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        Clz { dst, src } => Clz { dst: r(dst, map, caller), src: r(src, map, caller) },
        Ctz { dst, src } => Ctz { dst: r(dst, map, caller), src: r(src, map, caller) },
        Popcnt { dst, src } => Popcnt { dst: r(dst, map, caller), src: r(src, map, caller) },
        Bswap { dst, src } => Bswap { dst: r(dst, map, caller), src: r(src, map, caller) },
        FAdd { dst, lhs, rhs } => FAdd { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        FSub { dst, lhs, rhs } => FSub { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        FMul { dst, lhs, rhs } => FMul { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        FDiv { dst, lhs, rhs } => FDiv { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        FNeg { dst, src } => FNeg { dst: r(dst, map, caller), src: r(src, map, caller) },
        FAbs { dst, src } => FAbs { dst: r(dst, map, caller), src: r(src, map, caller) },
        Sqrt { dst, src } => Sqrt { dst: r(dst, map, caller), src: r(src, map, caller) },
        Fma { dst, a, b, c } => Fma { dst: r(dst, map, caller), a: r(a, map, caller), b: r(b, map, caller), c: r(c, map, caller) },
        FMin { dst, lhs, rhs } => FMin { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        FMax { dst, lhs, rhs } => FMax { dst: r(dst, map, caller), lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        Ceil { dst, src } => Ceil { dst: r(dst, map, caller), src: r(src, map, caller) },
        Floor { dst, src } => Floor { dst: r(dst, map, caller), src: r(src, map, caller) },
        Trunc { dst, src } => Trunc { dst: r(dst, map, caller), src: r(src, map, caller) },
        IReduce { dst, src, to } => IReduce { dst: r(dst, map, caller), src: r(src, map, caller), to },
        UExtend { dst, src, to } => UExtend { dst: r(dst, map, caller), src: r(src, map, caller), to },
        SExtend { dst, src, to } => SExtend { dst: r(dst, map, caller), src: r(src, map, caller), to },
        FPromote { dst, src } => FPromote { dst: r(dst, map, caller), src: r(src, map, caller) },
        FDemote { dst, src } => FDemote { dst: r(dst, map, caller), src: r(src, map, caller) },
        CvtFromSint { dst, src, to } => CvtFromSint { dst: r(dst, map, caller), src: r(src, map, caller), to },
        CvtFromUint { dst, src, to } => CvtFromUint { dst: r(dst, map, caller), src: r(src, map, caller), to },
        CvtToSintSat { dst, src, to } => CvtToSintSat { dst: r(dst, map, caller), src: r(src, map, caller), to },
        CvtToUintSat { dst, src, to } => CvtToUintSat { dst: r(dst, map, caller), src: r(src, map, caller), to },
        Bitcast { dst, src, to } => Bitcast { dst: r(dst, map, caller), src: r(src, map, caller), to },
        ICmp { dst, cond, lhs, rhs } => ICmp { dst: r(dst, map, caller), cond, lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        FCmp { dst, cond, lhs, rhs } => FCmp { dst: r(dst, map, caller), cond, lhs: r(lhs, map, caller), rhs: r(rhs, map, caller) },
        Select { dst, cond, on_true, on_false } => Select {
            dst: r(dst, map, caller),
            cond: r(cond, map, caller),
            on_true: r(on_true, map, caller),
            on_false: r(on_false, map, caller),
        },
        Load { dst, ty, ptr, offset, flags } => Load { dst: r(dst, map, caller), ty, ptr: r(ptr, map, caller), offset, flags },
        Store { val, ptr, offset, flags } => Store { val: r(val, map, caller), ptr: r(ptr, map, caller), offset, flags },
        // Demais variantes (CallUser para outras fns, narrow loads, etc.)
        // não devem aparecer dado o eligible() filter — por garantia:
        other => {
            // Conservador: bail se aparecer algo não tratado.
            return Some(other);
        }
    })
}

/// Reescreve o último Inst do vec pra que seu `dst` seja `target_dst`
/// em vez de `mapped`. Isso preserva o ValueId que o caller já alocou
/// pra receber o resultado da CallUser.
///
/// Quando o último Inst tem `dst` que não bate com `mapped` (ex.: ret eh
/// um param do callee que foi bypassed direto pro arg do caller), emite
/// um Inst::Bitcast como alias.
fn rebind_last_to_dst(insts: &mut Vec<Inst>, mapped: ValueId, target_dst: ValueId) {
    if mapped == target_dst {
        return;
    }
    if let Some(last) = insts.last_mut() {
        if let Some(d) = inst_dst_mut(last) {
            if *d == mapped {
                *d = target_dst;
                return;
            }
        }
    }
    // Fallback: emite uma copia explícita via IAddImm 0 (que o fold
    // remove depois) — preserva o tipo dst.
    insts.push(Inst::IAddImm {
        dst: target_dst,
        lhs: mapped,
        imm: 0,
    });
}

fn inst_dst_mut(inst: &mut Inst) -> Option<&mut ValueId> {
    use Inst::*;
    Some(match inst {
        IConst { dst, .. }
        | F32Const { dst, .. }
        | F64Const { dst, .. }
        | IAdd { dst, .. }
        | IAddImm { dst, .. }
        | ISub { dst, .. }
        | IMul { dst, .. }
        | IMulImm { dst, .. }
        | SDiv { dst, .. }
        | UDiv { dst, .. }
        | SRem { dst, .. }
        | URem { dst, .. }
        | INeg { dst, .. }
        | BAnd { dst, .. }
        | BAndImm { dst, .. }
        | BOr { dst, .. }
        | BOrImm { dst, .. }
        | BXor { dst, .. }
        | BXorImm { dst, .. }
        | BNot { dst, .. }
        | IShl { dst, .. }
        | IShlImm { dst, .. }
        | UShr { dst, .. }
        | SShr { dst, .. }
        | SShrImm { dst, .. }
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
        | Select { dst, .. }
        | Load { dst, .. } => dst,
        _ => return None,
    })
}
