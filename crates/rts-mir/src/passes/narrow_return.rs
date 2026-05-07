//! Sign-extend de valores signed narrow no `Return` final.
//!
//! O pass `narrow` mascara aritmetica para a largura do tipo (`i8` →
//! `& 0xFF`), preservando overflow unsigned naturalmente. Para tipos
//! **signed** (`i8`/`i16`), porem, o ABI de retorno (`i64`) precisa
//! refletir o bit de sinal — ex: `i8 127 + 1 = -128`, nao `128`.
//!
//! Este pass examina o `ret` da funcao MIR e, se for signed narrow,
//! insere `IShlImm` + `SShrImm` (shift left por `64-width`, depois
//! arithmetic shift right) no bloco que executa o `Return`, antes do
//! terminator. Cobre **somente** o(s) valor(es) retornado(s); a
//! aritmetica intermediaria continua mascarada por `narrow` (que
//! permanece idempotente).
//!
//! Roda apos o pass `narrow` no pipeline de otimizacao — se o pass
//! ja inseriu mask, o sign-extend cobre o "ultimo passo" sem
//! cascata.

use rts_hir::ir::HirType;

use crate::ir::*;

/// Insere sign-extend nos valores retornados quando a fn retorna
/// signed narrow. Retorna `true` se algo foi alterado.
pub fn narrow_return(mir: &mut MirFunc) -> bool {
    let signed_width = match &mir.ret {
        HirType::I8 => 8u8,
        HirType::I16 => 16u8,
        _ => return false,
    };
    let shift = (64 - signed_width) as i64;

    let mut changed = false;
    for block_idx in 0..mir.blocks.len() {
        let returned_vals: Vec<ValueId> = match &mir.blocks[block_idx].term {
            Terminator::Return(vs) => vs.clone(),
            _ => continue,
        };
        if returned_vals.is_empty() {
            continue;
        }
        // Idempotencia: se as ultimas insts do bloco ja sao
        // IShlImm + SShrImm com o shift correto sobre os retornos,
        // pula.
        let n = mir.blocks[block_idx].insts.len();
        if n >= 2 {
            let prev_two = &mir.blocks[block_idx].insts[n - 2..];
            let already = prev_two.iter().enumerate().all(|(i, inst)| match (i, inst) {
                (0, Inst::IShlImm { imm, .. }) if *imm == shift => true,
                (1, Inst::SShrImm { imm, .. }) if *imm == shift => true,
                _ => false,
            });
            if already {
                continue;
            }
        }
        for v in &returned_vals {
            mir.blocks[block_idx].insts.push(Inst::IShlImm {
                dst: *v,
                lhs: *v,
                imm: shift,
            });
            mir.blocks[block_idx].insts.push(Inst::SShrImm {
                dst: *v,
                lhs: *v,
                imm: shift,
            });
            changed = true;
        }
    }
    changed
}
