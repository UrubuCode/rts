//! `hint` namespace — performance hints (std::hint).
//!
//! **Primeira namespace migrada do `#[rts_namespace]` (macro) para o modelo
//! builder do `rts-engine`** (Fase 2 de `docs/specs/rts-engine-dispatch.md`/`WORKING.md`). Os
//! externs são `#[no_mangle]` à mão; a superfície é registrada via
//! [`register`] no startup (o `register_builtins` do codegen a folda no
//! registry). Não há mais `#[rts_namespace]`/`SPEC` macro-gerado aqui — é o
//! template para migrar as demais namespaces e, no fim, deletar `rts-macro`.

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

/// Hint para spin-wait loop (PAUSE em x86, YIELD em ARM).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HINT_SPIN_LOOP() {
    std::hint::spin_loop();
}

/// Opaque pra otimizador — impede que o valor seja eliminado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HINT_BLACK_BOX_I64(value: i64) -> i64 {
    std::hint::black_box(value)
}

/// Opaque pra otimizador (variante f64).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HINT_BLACK_BOX_F64(value: f64) -> f64 {
    std::hint::black_box(value)
}

/// Marca codigo inalcancavel — em debug aborta, em release eh UB.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HINT_UNREACHABLE() {
    if cfg!(debug_assertions) {
        panic!("hint.unreachable() atingido");
    }
    // SAFETY: caller garantiu que esta linha nao executa.
    unsafe { std::hint::unreachable_unchecked() }
}

/// Assume cond=true sem verificar. Cond falsa = UB em release.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HINT_ASSERT_UNCHECKED(cond: i64) {
    if cfg!(debug_assertions) {
        if cond == 0 {
            panic!("hint.assert_unchecked: cond=false");
        }
        return;
    }
    // SAFETY: caller garantiu cond verdadeira.
    unsafe { std::hint::assert_unchecked(cond != 0) }
}

/// Constrói um membro-função com ts/doc explícitos (preserva o que a macro
/// derivava, para o `rts.d.ts` ficar idêntico).
fn func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `hint` no motor (chamado no seed do registry do codegen).
pub fn register(e: &mut Engine) {
    e.ns("hint")
        .doc("Performance hints (std::hint): spin_loop, black_box, unreachable, assert_unchecked.")
        .member(func(
            "spin_loop",
            "__RTS_FN_NS_HINT_SPIN_LOOP",
            sig!(=> Void),
            "spin_loop(): void",
            "Hint para spin-wait loop (PAUSE em x86, YIELD em ARM).",
            __RTS_FN_NS_HINT_SPIN_LOOP as *const u8,
        ))
        .member(func(
            "black_box_i64",
            "__RTS_FN_NS_HINT_BLACK_BOX_I64",
            sig!(I64 => I64),
            "black_box_i64(value: number): number",
            "Opaque pra otimizador — impede que o valor seja eliminado.",
            __RTS_FN_NS_HINT_BLACK_BOX_I64 as *const u8,
        ))
        .member(func(
            "black_box_f64",
            "__RTS_FN_NS_HINT_BLACK_BOX_F64",
            sig!(F64 => F64),
            "black_box_f64(value: number): number",
            "Opaque pra otimizador (variante f64).",
            __RTS_FN_NS_HINT_BLACK_BOX_F64 as *const u8,
        ))
        .member(func(
            "unreachable",
            "__RTS_FN_NS_HINT_UNREACHABLE",
            sig!(=> Void),
            "unreachable(): never",
            "Marca codigo inalcancavel — em debug aborta, em release eh UB.",
            __RTS_FN_NS_HINT_UNREACHABLE as *const u8,
        ))
        .member(func(
            "assert_unchecked",
            "__RTS_FN_NS_HINT_ASSERT_UNCHECKED",
            sig!(Bool => Void),
            "assert_unchecked(cond: boolean): void",
            "Assume cond=true sem verificar. Cond falsa = UB em release.",
            __RTS_FN_NS_HINT_ASSERT_UNCHECKED as *const u8,
        ))
        .done();
}
