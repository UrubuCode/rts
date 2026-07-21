//! `WeakRef` global class (#685 / #217 A1.1) — semântica weak REAL.
//!
//! `Entry::WeakRef(target)` guarda o handle do target mas NÃO é traçado pelo
//! coletor: `Traceable::trace_children` para `Entry` cai no braço `_ => {}`, ou
//! seja, o coletor não marca através do WeakRef — o target morre se não houver
//! outra referência forte. `deref()` então é weak de verdade: só devolve o
//! target se o slot ainda estiver vivo com a MESMA generation (`with_entry`
//! valida gen+slot O(1)); caso contrário devolve `undefined` (0). Isso também
//! corrige o use-after-free latente do v0, que devolvia o handle cru mesmo
//! depois do slot ter sido liberado/reusado. Migrado do `#[rts_class]` (macro)
//! pro modelo builder hand-written do `rts-engine`.

use rts_engine::abi::ty::Handle;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry};

/// Creates a new WeakRef wrapping target.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKREF_NEW(target: Handle) -> Handle {
    alloc_entry(Entry::WeakRef(target))
}

/// Returns the target object, or `undefined` (0) if it was collected/reused.
///
/// Weak de verdade: o target não é mantido vivo pelo WeakRef (não é traçado).
/// Só o devolvemos se o handle ainda apontar para um slot vivo com a mesma
/// generation — `with_entry` rejeita (None) handle stale (slot Free ou gen
/// bumpou). O(1), sem scan.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_WEAKREF_DEREF(wr: Handle) -> Handle {
    let target = with_entry(wr, |e| match e {
        Some(Entry::WeakRef(target)) => *target,
        _ => 0,
    });
    if target == 0 {
        return 0;
    }
    // Target vivo? `with_entry` valida gen+slot; None => coletado/reusado => undefined.
    let alive = with_entry(target, |e| e.is_some());
    if alive { target } else { 0 }
}

/// Registra a classe global `WeakRef` no motor (hand-written, sem macro).
pub fn register_weakref_class_spec(e: &mut Engine) {
    e.class("WeakRef")
        .doc("Built-in WeakRef (#217 A1.1). Weak real: deref devolve undefined após coleta.")
        .member(Member {
            name: "new".to_string(),
            kind: MemberKind::Constructor,
            sig: Sig::new(vec![AbiType::Handle], AbiType::Handle),
            symbol: "__RTS_FN_GL_WEAKREF_NEW".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_WEAKREF_NEW as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "new WeakRef(target: object): WeakRef".to_string(),
            doc: "Creates a new WeakRef wrapping target.".to_string(),
            pure: false,
            emit: None,
        })
        .member(Member {
            name: "deref".to_string(),
            kind: MemberKind::InstanceMethod,
            sig: Sig::new(vec![AbiType::Handle], AbiType::Handle),
            symbol: "__RTS_FN_GL_WEAKREF_DEREF".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_WEAKREF_DEREF as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "deref(): object | undefined".to_string(),
            doc: "Returns the target object, or undefined if it was collected/reused.".to_string(),
            pure: true,
            emit: None,
        })
        .done();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deref_returns_target() {
        let target = alloc_entry(Entry::String(b"hello".to_vec()));
        let wr = __RTS_FN_GL_WEAKREF_NEW(target);
        assert_eq!(__RTS_FN_GL_WEAKREF_DEREF(wr), target);
    }

    #[test]
    fn deref_undefined_for_stale_target() {
        // Um handle com generation 0 (16 bits altos zerados) nunca corresponde a
        // um slot vivo — slots vivos têm generation >= 1. WeakRef sobre ele
        // simula um target já coletado/reusado: deref deve devolver undefined (0),
        // não o handle stale (o use-after-free que o v0 tinha).
        let stale: Handle = 5; // gen=0, slot=5
        let wr = __RTS_FN_GL_WEAKREF_NEW(stale);
        assert_eq!(__RTS_FN_GL_WEAKREF_DEREF(wr), 0);
    }
}
