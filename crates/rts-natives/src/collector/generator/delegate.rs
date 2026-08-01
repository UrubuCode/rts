//! `yield*` lazy delegation (#477/#211) and `obj[Symbol.iterator]` typing.
//!
//! Instead of materializing the delegated source (eager, and unbounded for an
//! infinite delegate), `yield* SRC` iterates SRC one value at a time and
//! re-yields each, suspending in between. The delegate's iteration state lives
//! runtime-side keyed by handle — for a `GenState` it is that state's own
//! `.done`, for a Vec it is the cursor in [`super::GEN_CURSORS`] — so only the
//! HANDLE enters the outer generator's frame and the delegation survives the
//! outer suspension and resume.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::heap::handles::{Entry, alloc_entry, with_entry};


use super::iterhelp::{__rtsm_global_Array_iterator_fn, __rtsm_global_Iterator_from};

use super::{GEN_CURSORS, UNDEFINED};

// ── yield* lazy delegation (#477/#211) ───────────────────────────────────────
// `yield* SRC` na state-machine: em vez de materializar SRC inteiro (eager,
// estoura em delegado infinito), iteramos SRC um valor por vez e RE-YIELDAMOS
// cada um, suspendendo entre eles. O estado do iterador delegado vive
// runtime-side (keyed pelo handle): para um GenState eh o proprio `.done`; para
// um Vec eh o cursor em GEN_CURSORS. So' o HANDLE entra no frame do generator
// externo, entao a delegacao sobrevive a suspensao/retomada do externo.

/// `undefined` como PolyValue WORD. O sentinela legado `UNDEFINED`
/// (`i64::MIN+2`) NAO e' um word boxado: lido como PolyValue ele e' o double
/// inline `-1e-323`, que foi exatamente o que `const r = yield* g()` devolveu
/// enquanto este default estava errado.
const POLY_UNDEF: i64 = crate::heap::poly::POLY_UNDEFINED as i64;

thread_local! {
    /// Valor de `return X` do delegado EAGER, keyed pelo handle do ITERADOR.
    ///
    /// Necessario porque `GEN_RETS` e' keyed pelo handle do Vec ORIGINAL, e
    /// `DELEGATE_START` devolve para um Vec uma COPIA com cursor
    /// (`Iterator_from`) — outro handle. Sem esta ponte, `yield*` sobre um
    /// generator eager perdia o `return` e devolvia undefined em silencio.
    static DELEGATE_RETS: RefCell<HashMap<u64, i64>> = RefCell::new(HashMap::new());

    /// Done-flag do ultimo DELEGATE_NEXT por handle de iterador delegado.
    static DELEGATE_DONE: RefCell<HashMap<u64, bool>> = RefCell::new(HashMap::new());
}

fn set_delegate_done(h: u64, d: bool) {
    DELEGATE_DONE.with(|c| {
        c.borrow_mut().insert(h, d);
    });
}

/// `__RTS_GEN_DELEGATE_START(src)` — normaliza a fonte de um `yield*` num handle
/// de iterador. Generator (GenState) -> ele mesmo (ja' eh iterador). Array (Vec)
/// -> uma copia com cursor (ITERATOR_FROM). String -> Vec de chars (1 handle por
/// caractere). Outro -> passthrough (assume iterador).
/// Um arg de delegação chega em TRÊS convenções, conforme o call site do SM
/// boxa/coage: um WORD PolyValue NaN-boxed (TAG_OBJECT de um buffer eager), os
/// BITS de um double inline (um GenState HANDLE Int64 boxado como double e
/// repassado verbatim), ou um handle cru legado. Normaliza por TAG.
fn normalize_delegate_word(src: i64) -> u64 {
    let w = src as u64;
    if let Some(real) = rts_engine::heap::poly::poly_handle_normalize(w) {
        return real;
    }
    let f = f64::from_bits(w);
    if f.is_finite() && f >= (1u64 << 48) as f64 && f.fract() == 0.0 {
        return f as u64;
    }
    w
}

#[rtse::abi(native, value = "gen_delegate_start")]
pub fn gen_delegate_start(src: i64) -> i64 {
    let h = normalize_delegate_word(src);
    let kind = with_entry(h, |e| match e {
        Some(Entry::GenState(_)) => 1u8,
        Some(Entry::Vec(_)) => 2,
        Some(Entry::String(_)) => 3,
        _ => 0,
    });
    if std::env::var("RTS_DEBUG_GEN").is_ok() {
        eprintln!("[gen] DELEGATE_START src={src:#x} h={h:#x} kind={kind}");
    }
    match kind {
        // Generator: ja' eh iterador — devolve o HANDLE NORMALIZADO (o `src`
        // cru pode ser o word boxed; NEXT/DONE fazem with_entry direto).
        1 => h as i64,
        2 => {
            let it = __rtsm_global_Iterator_from(h);
            // O eager ja' materializou (e ja' gravou o `return`) no momento da
            // CHAMADA, entao o valor ja' esta' em GEN_RETS quando chegamos aqui.
            if let Some(r) = super::GEN_RETS.with(|c| c.borrow().get(&h).copied()) {
                DELEGATE_RETS.with(|c| {
                    c.borrow_mut().insert(it, r);
                });
            }
            it as i64
        }
        3 => {
            // String iteravel: cada char vira um handle de String de 1 char.
            let s: String = with_entry(h, |e| match e {
                Some(Entry::String(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
                _ => String::new(),
            });
            let chars: Vec<i64> = s
                .chars()
                .map(|c| alloc_entry(Entry::String(c.to_string().into_bytes())) as i64)
                .collect();
            let vh = alloc_entry(Entry::Vec(Box::new(chars)));
            GEN_CURSORS.with(|c| {
                c.borrow_mut().insert(vh, 0);
            });
            vh as i64
        }
        _ => src,
    }
}

/// `__RTS_GEN_DELEGATE_NEXT(it)` — avanca o iterador delegado um passo. Devolve o
/// valor produzido (ou UNDEFINED se esgotou); o flag done fica acessivel via
/// `__RTS_GEN_DELEGATE_DONE(it)`.
#[rtse::abi(native, value = "gen_delegate_next")]
pub fn gen_delegate_next(it: i64) -> i64 {
    let h = normalize_delegate_word(it);
    let kind = with_entry(h, |e| match e {
        Some(Entry::GenState(_)) => 1u8,
        Some(Entry::Vec(_)) => 2,
        _ => 0,
    });
    if std::env::var("RTS_DEBUG_GEN").is_ok() {
        eprintln!("[gen] DELEGATE_NEXT it={it:#x} h={h:#x} kind={kind}");
    }
    match kind {
        1 => {
            let (fn_ptr, done0) = with_entry(h, |e| match e {
                Some(Entry::GenState(g)) => (g.fn_ptr, g.done),
                _ => (0u64, true),
            });
            if fn_ptr == 0 || done0 {
                set_delegate_done(h, true);
                return UNDEFINED;
            }
            let state_fn: extern "C" fn(u64) -> i64 =
                unsafe { std::mem::transmute(fn_ptr as usize) };
            let v = state_fn(h);
            let done = with_entry(h, |e| match e {
                Some(Entry::GenState(g)) => g.done,
                _ => true,
            });
            set_delegate_done(h, done);
            if done { UNDEFINED } else { v }
        }
        2 => {
            let len = with_entry(h, |e| match e {
                Some(Entry::Vec(v)) => v.len(),
                _ => 0,
            });
            let cursor = GEN_CURSORS.with(|c| *c.borrow_mut().entry(h).or_insert(0));
            if cursor < len {
                let val = with_entry(h, |e| match e {
                    Some(Entry::Vec(v)) => v.get(cursor).copied().unwrap_or(UNDEFINED),
                    _ => UNDEFINED,
                });
                GEN_CURSORS.with(|c| {
                    if let Some(cur) = c.borrow_mut().get_mut(&h) {
                        *cur += 1;
                    }
                });
                set_delegate_done(h, false);
                val
            } else {
                set_delegate_done(h, true);
                UNDEFINED
            }
        }
        _ => {
            set_delegate_done(h, true);
            UNDEFINED
        }
    }
}

/// `__RTS_SYMBOL_ITERATOR_OF(obj)` (#222) — leitura de `obj[Symbol.iterator]`
/// ciente do tipo: devolve um handle de Function (para `typeof === "function"`)
/// se `obj` for iteravel (Vec/String/Map), senao UNDEFINED. Sem isto, todo
/// `obj[Symbol.iterator]` rendia uma function (mesmo para numeros), quebrando
/// `typeof item[Symbol.iterator] === "function"` (flatten recursava em numeros).
#[rtse::abi(native, value = "symbol_iterator_of")]
pub fn symbol_iterator_of(obj: i64) -> i64 {
    let h = obj as u64;
    let iterable = with_entry(h, |e| {
        matches!(
            e,
            Some(Entry::Vec(_)) | Some(Entry::String(_)) | Some(Entry::Map(_))
        )
    });
    if iterable {
        __rtsm_global_Array_iterator_fn() as i64
    } else {
        UNDEFINED
    }
}

/// `__RTS_GEN_DELEGATE_DONE(it)` — 1 se o ultimo NEXT esgotou o delegado, senao
/// 0. Inteiro puro (nao sentinela de bool) para ser usado direto como condicao
/// truthy no `if` da state-machine.
#[rtse::abi(native, value = "gen_delegate_done")]
pub fn gen_delegate_done(it: i64) -> i64 {
    let h = normalize_delegate_word(it);
    let done = DELEGATE_DONE.with(|c| c.borrow().get(&h).copied().unwrap_or(false));
    if done { 1 } else { 0 }
}

/// `__RTS_GEN_DELEGATE_RET(it)` — o valor de `return X` do delegado, que é o
/// VALOR da expressão `const r = yield* SRC` (spec: a delegação produz o
/// `value` do resultado `done:true` da fonte).
///
/// Existe porque `__rtsn_generator_get_ret` NÃO serve aqui: ele lê só o mapa do
/// caminho EAGER ([`super::GEN_RETS`], keyed pelo handle do Vec). Num delegado
/// LAZY o `return X` foi gravado pelo `GEN_SM_DONE` em `GenState.ret`, que
/// aquele acessor não olha — reusá-lo compilaria e devolveria `undefined` em
/// silêncio, que é a classe de erro que este projeto persegue.
///
/// Normaliza o argumento pelas mesmas três convenções que
/// [`gen_delegate_next`] (word NaN-boxed, bits de double, handle cru), porque o
/// handle chega pelo frame do generator externo e o call site o boxa.
/// Uma fonte sem valor de retorno (array, string, generator que termina sem
/// `return`) devolve `undefined`, como no JS.
#[rtse::abi(native, value = "gen_delegate_ret")]
pub fn gen_delegate_ret(it: i64) -> i64 {
    let h = normalize_delegate_word(it);
    let from_state = with_entry(h, |e| match e {
        Some(Entry::GenState(g)) => Some(g.ret),
        _ => None,
    });
    match from_state {
        Some(r) if r != UNDEFINED => r,
        // Delegado eager (Vec com cursor): o `return` foi registrado sob o
        // handle do ITERADOR em DELEGATE_START (ver DELEGATE_RETS).
        _ => DELEGATE_RETS
            .with(|c| c.borrow().get(&h).copied())
            .unwrap_or(POLY_UNDEF),
    }
}

/// `__RTS_GEN_DELEGATE_NEXT(g, it)` — avanca o delegado ENCAMINHANDO o valor de
/// `outer.next(v)`.
///
/// A spec manda `yield*` repassar ao delegado o valor que a retomada recebeu; um
/// `const q = yield "ask"` DENTRO do delegado tem de ler o `v` que o chamador
/// passou ao generator EXTERNO. A forma de 1 argumento nao tinha como fazer
/// isso — nao via o generator externo — e o delegado lia `undefined`.
#[rtse::abi(native, value = "gen_delegate_next_sent")]
pub fn gen_delegate_next_sent(g: i64, it: i64) -> i64 {
    let sent = with_entry(g as u64, |e| match e {
        Some(Entry::GenState(gs)) => Some(gs.sent),
        _ => None,
    });
    let h = normalize_delegate_word(it);
    if let Some(v) = sent {
        crate::heap::handles::with_entry_mut(h, |e| {
            if let Some(Entry::GenState(d)) = e {
                d.sent = v;
            }
        });
    }
    let out = __rtsn_gen_delegate_next(it);
    // O `sent` pertence a ESTA retomada: expira no delegado tambem, senao um
    // `next()` sem argumento herdaria o valor do `next(v)` anterior.
    crate::heap::handles::with_entry_mut(h, |e| {
        if let Some(Entry::GenState(d)) = e {
            d.sent = POLY_UNDEF;
        }
    });
    out
}
