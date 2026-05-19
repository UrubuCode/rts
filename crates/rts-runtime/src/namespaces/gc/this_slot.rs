//! Thread-local `this` binding slot for plain (non-class) user functions.
//!
//! Quando uma fn nao-arrow plain eh chamada via `fn.call(thisArg, ...)` ou
//! `fn.apply(thisArg, args)`, RTS nao consegue passar `this` como param real
//! (mudaria callconv de toda user fn). Em vez disso, a runtime de Function
//! empilha o `thisArg` neste slot antes do invoke e desempilha depois;
//! `Expr::This` em fn plain emite call para `__RTS_FN_RT_THIS_GET`.
//!
//! Stack-based para suportar nesting (`a.call(x, () => b.call(y, ...))`).

use std::cell::RefCell;

thread_local! {
    static THIS_STACK: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) };
}

/// Push thisArg ao topo da pilha. Chamado pela runtime de Function antes do invoke.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_THIS_PUSH(value: i64) {
    THIS_STACK.with(|s| s.borrow_mut().push(value));
}

/// Pop do topo. Chamado pela runtime de Function apos o invoke retornar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_THIS_POP() {
    THIS_STACK.with(|s| {
        let _ = s.borrow_mut().pop();
    });
}

/// Le topo da pilha. `i64::MIN + 2` (undefined sentinel) quando nao ha
/// `this` corrente. JS spec strict mode: fn nao-method tem `this === undefined`.
/// (cross-runtime #300)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_THIS_GET() -> i64 {
    THIS_STACK.with(|s| s.borrow().last().copied().unwrap_or(i64::MIN + 2))
}
