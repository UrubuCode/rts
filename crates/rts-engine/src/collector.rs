//! Contrato do GC/collector — a INTERFACE congelada do gerenciamento de memória
//! do RTS. (Design: ver `WORKING.md` §"DESIGN GC".)
//!
//! Princípio: **congele a interface, não a política.** O codegen emite ops contra
//! este contrato estável; a *política* (hoje mark+sweep no `rts-runtime`; futuro
//! RC + coletor de ciclos) vive ATRÁS dele e pode evoluir sem tocar o codegen.
//! É por isso que o contrato mora no `rts-engine` (núcleo cru, crate base de que
//! `rts-runtime`/`rts-browser` dependem) — não no codegen (que dependeria do
//! runtime → ciclo) nem no runtime (que carrega `Entry` com tokio/regex/rustls).
//!
//! # O op-set ABI (estável, idêntico em AOT e JIT)
//!
//! Cada op é um símbolo `extern "C"` que o codegen chama por símbolo literal
//! (como os `__RTS_FN_NS_GC_*` de hoje). A *implementação* desses símbolos é
//! fornecida pela camada de heap (backend: `rts-runtime`), nunca pelo navegador.
//!
//! | Op | Assinatura lógica | Papel |
//! |----|-------------------|-------|
//! | `alloc`     | `(type_tag) -> handle`        | aloca um slot, retorna handle u64 |
//! | `retain`    | `(handle)`                    | +1 na contagem (RC) / no-op no mark+sweep |
//! | `release`   | `(handle)`                    | -1; libera + `finalize` em 0 (RC) |
//! | `trace`     | `(handle, visit)`             | enumera os handles-filho (ver [`Traceable`]) |
//! | `finalize`  | `(handle)`                    | roda o destrutor de domínio (libera socket/thread/...) |
//! | `roots`     | `() -> &[handle]`             | enumera as raízes (stack maps + globais + thread regs) |
//!
//! Enquanto a política for mark+sweep, `retain`/`release` são no-ops e `roots`+
//! `trace` dirigem o mark. Ao migrar para RC, `retain`/`release` passam a contar
//! e `trace` só é usado pelo coletor de ciclos — **sem mudança no codegen**,
//! porque os símbolos e a forma das ops não mudam.

pub use crate::abi::handles::{
    HANDLE_GEN_SHIFT, HANDLE_N_SHARDS, HANDLE_SHARD_BITS, HANDLE_SHARD_MASK, HANDLE_SLOT_MASK,
};

/// Índice do shard de um handle (low bits) — O(1), sem lock.
#[inline]
pub fn handle_shard(handle: u64) -> usize {
    (handle & HANDLE_SHARD_MASK) as usize
}

/// Geração (16 bits altos) de um handle — distingue reuso de slot (ABA).
#[inline]
pub fn handle_generation(handle: u64) -> u16 {
    (handle >> HANDLE_GEN_SHIFT) as u16
}

/// Slot (sem o shard) dentro da tabela do shard.
#[inline]
pub fn handle_slot(handle: u64) -> u64 {
    (handle & HANDLE_SLOT_MASK) >> HANDLE_SHARD_BITS
}

/// Um objeto gerenciado pelo collector sabe enumerar os **handles-filho** que
/// mantém vivos (campos que são, eles próprios, handles GC). É o protocolo que
/// deixa o coletor genérico (no engine) andar o grafo de objetos **sem conhecer
/// as variants concretas** — a impl de domínio (`Entry` no `rts-runtime`, com
/// suas variants pesadas tokio/regex/rustls) fornece o `trace_children`/`finalize`.
///
/// `visit` é chamado uma vez por handle-filho. Um objeto-folha (string, número
/// boxed) não chama `visit` nenhuma vez. Não deve reportar o próprio handle.
pub trait Traceable {
    /// Enumera cada handle-filho deste objeto, chamando `visit(child)`.
    fn trace_children(&self, visit: &mut dyn FnMut(u64));

    /// Destrutor de domínio: roda quando o objeto é liberado (RC → 0, ou sweep).
    /// É aqui que recursos do SO (socket, JoinHandle, ProcessChild) são fechados
    /// — determinístico no RC (finalização pronta), não num tick aleatório.
    /// Default no-op para objetos sem recurso externo.
    fn finalize(&mut self) {}
}

/// Marcador do payload de um slot do collector: traçável + movível entre threads.
/// A `HandleTable` genérica (a migrar pro engine no SPLIT) guarda `Box<dyn
/// GcPayload>` + gen/mark; as variants concretas vivem na camada de heap.
pub trait GcPayload: Traceable + Send + 'static {}

impl<T: Traceable + Send + 'static> GcPayload for T {}

#[cfg(test)]
mod tests {
    use super::*;

    struct Leaf;
    impl Traceable for Leaf {
        fn trace_children(&self, _visit: &mut dyn FnMut(u64)) {}
    }

    struct Pair(u64, u64);
    impl Traceable for Pair {
        fn trace_children(&self, visit: &mut dyn FnMut(u64)) {
            visit(self.0);
            visit(self.1);
        }
    }

    #[test]
    fn leaf_has_no_children() {
        let mut seen = Vec::new();
        Leaf.trace_children(&mut |h| seen.push(h));
        assert!(seen.is_empty());
    }

    #[test]
    fn pair_traces_both_children() {
        let mut seen = Vec::new();
        Pair(7, 9).trace_children(&mut |h| seen.push(h));
        assert_eq!(seen, vec![7, 9]);
    }

    #[test]
    fn handle_decode_roundtrip() {
        // shard 5, slot tal, gen 3 — confere que os campos não se sobrepõem.
        let shard = 5u64;
        let generation = 3u64;
        let slot_field = 42u64 << HANDLE_SHARD_BITS;
        let handle = (generation << HANDLE_GEN_SHIFT) | slot_field | shard;
        assert_eq!(handle_shard(handle), shard as usize);
        assert_eq!(handle_generation(handle), generation as u16);
        assert_eq!(handle_slot(handle), 42);
    }

    #[test]
    fn gc_payload_blanket_impl() {
        fn assert_payload<T: GcPayload>(_: &T) {}
        assert_payload(&Leaf);
    }
}
