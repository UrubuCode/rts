//! Thread-local `this` binding slot for plain (non-class) user functions.
//!
//! Quando uma fn nao-arrow plain eh chamada via `fn.call(thisArg, ...)` ou
//! `fn.apply(thisArg, args)`, RTS nao consegue passar `this` como param real
//! (mudaria callconv de toda user fn). Em vez disso, a runtime de Function
//! empilha o `thisArg` neste slot antes do invoke e desempilha depois.
//!
//! Stack-based para suportar nesting (`a.call(x, () => b.call(y, ...))`).
//!
//! O slot eh hoje WRITE-ONLY: o leitor (`__RTS_FN_RT_THIS_GET`) foi removido
//! porque o motor passa `this` por outro caminho e ninguem chamava o simbolo.
//! O par push/pop continua vivo — desfazer o `pushed_this_slot` do dispatch de
//! Function eh mudanca propria, com verificacao propria.

use std::cell::RefCell;

thread_local! {
    static THIS_STACK: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) };
}

/// Push thisArg ao topo da pilha. Chamado pela runtime de Function antes do invoke.
#[rtse::abi("__RTS_FN_RT_THIS_PUSH")]
pub fn __RTS_FN_RT_THIS_PUSH(value: i64) {
    THIS_STACK.with(|s| s.borrow_mut().push(value));
}

/// Pop do topo. Chamado pela runtime de Function apos o invoke retornar.
#[rtse::abi("__RTS_FN_RT_THIS_POP")]
pub fn __RTS_FN_RT_THIS_POP() {
    THIS_STACK.with(|s| {
        let _ = s.borrow_mut().pop();
    });
}
