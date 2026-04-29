//! Mark+sweep collector explicito sobre HandleTable.
//!
//! Versao MVP do #155 sem gc-arena: o usuario chama `gc.collect(roots)`
//! com os handles vivos que quer preservar, e o coletor:
//!
//! 1. Mark: marca cada root como vivo, e recursivamente marca handles
//!    referenciados (Map values, Vec elements, Instance fields, Env
//!    slots) que parecem ser handles validos na tabela.
//! 2. Sweep: libera cada slot nao-marcado.
//!
//! Limitacao: o codegen NAO chama collect automaticamente. Ele e' um
//! ponto de quiescencia explicito controlado pelo usuario. Isto cobre
//! programas longos que querem disparar GC manualmente ao fim de
//! batches/epocas, sem o overhead de mark sincrono em todo return.
//!
//! Auto-collect em pontos de quiescencia (return de fn user) e' fase 2,
//! exige que o codegen registre roots ativos no scope corrente.

use std::collections::HashSet;

use super::handles::{Entry, decode, shards};

/// Marca recursivamente `handle` e tudo que ele referencia.
/// `visited` evita ciclos infinitos.
fn mark(handle: u64, visited: &mut HashSet<u64>) {
    if !visited.insert(handle) {
        return;
    }
    // Resolve em qual shard esta e tira snapshot dos refs sem segurar
    // o lock — evita deadlock se um Entry referencia handle de outro
    // shard e a recursao precisa do lock daquele.
    let refs: Vec<u64> = {
        let Some((_, shard_idx, _)) = decode(handle) else {
            return;
        };
        let Some(shard) = shards().get(shard_idx) else {
            return;
        };
        let Ok(table) = shard.lock() else { return };
        let Some(entry) = table.get(handle) else {
            return;
        };
        collect_refs(entry)
    };
    for r in refs {
        mark(r, visited);
    }
}

/// Extrai handles referenciados de um Entry. Conservador: trata cada
/// i64 em Map/Vec/Env/Instance como possivel handle e tenta marcar.
/// `decode + get` na recursao separa handles validos de i64 numericos.
fn collect_refs(entry: &Entry) -> Vec<u64> {
    let mut refs: Vec<u64> = Vec::new();
    match entry {
        Entry::Map(m) => {
            for v in m.values() {
                refs.push(*v as u64);
            }
        }
        Entry::Vec(v) => {
            for x in v.iter() {
                refs.push(*x as u64);
            }
        }
        Entry::Env(slots) => {
            for s in slots.iter() {
                refs.push(*s as u64);
            }
        }
        Entry::Instance(inst) => {
            // class tag e' handle de string
            refs.push(inst.class);
            // bytes pode conter handles em offsets de fields. Tratamento
            // conservador: trata cada u64 alinhado a 8 bytes como possivel
            // handle. Falsos positivos sao ok — eles vao falhar no
            // decode/get e nao causam mark indevido.
            for chunk in inst.bytes.chunks_exact(8) {
                let mut buf = [0u8; 8];
                buf.copy_from_slice(chunk);
                refs.push(u64::from_le_bytes(buf));
            }
        }
        // Strings, BigFixed, Buffer, processo, sockets, etc. nao
        // referenciam handles internos.
        _ => {}
    }
    refs
}

/// Libera todo slot nao-marcado em todos os shards. Retorna numero
/// total de slots liberados.
fn sweep(visited: &HashSet<u64>) -> u64 {
    let mut freed = 0u64;
    for (shard_idx, shard) in shards().iter().enumerate() {
        let Ok(mut table) = shard.lock() else { continue };
        let to_free = table.live_handles_snapshot(shard_idx);
        for h in to_free {
            if !visited.contains(&h) {
                if table.free(h) {
                    freed += 1;
                }
            }
        }
    }
    freed
}

/// Mark+sweep com `roots` como conjunto vivo. Retorna numero de slots
/// liberados.
pub fn collect(roots: &[u64]) -> u64 {
    let mut visited: HashSet<u64> = HashSet::new();
    for r in roots {
        if *r != 0 {
            mark(*r, &mut visited);
        }
    }
    sweep(&visited)
}

// ─── Extern ABI ───────────────────────────────────────────────────────

/// Coleta com um unico root. Retorna numero de slots liberados como i64.
/// Para multi-root, userland passa um Vec handle e o codegen extrai
/// (via gc.collect_vec) — feito separadamente.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_COLLECT(root: u64) -> i64 {
    let roots = if root == 0 { vec![] } else { vec![root] };
    collect(&roots) as i64
}

/// Coleta com roots dado por handle de Vec<i64>. Cada elemento do vec
/// e' tratado como handle. Retorna numero de slots liberados.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_COLLECT_VEC(roots_vec: u64) -> i64 {
    // Snapshot dos roots antes de chamar collect — evita re-entrar
    // no shard durante mark.
    let roots: Vec<u64> = {
        let Some((_, shard_idx, _)) = decode(roots_vec) else {
            return 0;
        };
        let Some(shard) = shards().get(shard_idx) else {
            return 0;
        };
        let Ok(table) = shard.lock() else { return 0 };
        match table.get(roots_vec) {
            Some(Entry::Vec(v)) => v.iter().map(|x| *x as u64).collect(),
            _ => return 0,
        }
    };
    // Inclui o proprio Vec dos roots tambem (senao e' coletado).
    let mut all = roots;
    all.push(roots_vec);
    collect(&all) as i64
}

/// Conta handles vivos atualmente. Util pra benchmarks/testes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_LIVE_COUNT() -> i64 {
    let mut total = 0i64;
    for shard in shards() {
        let Ok(table) = shard.lock() else { continue };
        total += table.live_handle_count() as i64;
    }
    total
}
