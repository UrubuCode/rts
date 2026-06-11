//! Mapa thread-local `handle -> stack_text` para `Error.prototype.stack` (#745).
//!
//! Movido do `collector/error` do `rts-runtime` pro motor porque o consumidor
//! (`globals/error`) migra pra camada universal (`rts-shared`) e precisa ler o
//! stack via uma fn Rust (não dá pra extern-decl `Option<String>`). O *slot* de
//! erro pendente (ERROR_SLOT, `__RTS_FN_RT_ERROR_*`) fica no runtime; só este
//! mapa puro (sem GC) sobe pro engine. `record` é chamado pelo `ERROR_CLEAR` do
//! runtime; `stack_for_handle` pelo `globals/error`.

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static ERR_STACKS: RefCell<HashMap<u64, String>> = RefCell::new(HashMap::new());
}

/// Salva o texto do stack associado ao handle do valor lançado, para que
/// `e.stack` possa lê-lo dentro do catch body depois que o stack handle
/// pendente for liberado.
pub fn record(handle: u64, text: String) {
    ERR_STACKS.with(|m| m.borrow_mut().insert(handle, text));
}

/// Recupera o texto do stack salvo para um handle de erro, se houver.
pub fn stack_for_handle(handle: u64) -> Option<String> {
    ERR_STACKS.with(|m| m.borrow().get(&handle).cloned())
}
