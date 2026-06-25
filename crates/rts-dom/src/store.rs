//! Store de `Dom`s vivos, chaveados por um handle `u64` — a FONTE ÚNICA DA VERDADE
//! da árvore. Tanto a ABI headless (`abi.rs`, `rts:dom`) quanto um renderer (o
//! `rts-egui`) acessam o MESMO `Dom` por handle através daqui: por isso
//! `document.setText(...)` muda o que a janela pinta — é a mesma árvore.
//!
//! DOUTRINA: o store vive AQUI, no crate dono do DOM — não no `HandleTable` do
//! `rts-engine` (que faria o engine conhecer o DOM, violando a doutrina
//! PRIMORDIAL). É um `thread_local` próprio; o handle `u64` é só uma chave opaca.
//! Como `Dom` é `Send`/`Sync`-irrelevante aqui (uso single-thread, a thread do TS),
//! o `thread_local` é suficiente e evita locking.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::dom::Dom;

thread_local! {
    /// `handle u64 → Dom`. Cresce sob `insert` (parseHtml/createDocument), some
    /// sob `remove` (free). O renderer NÃO copia: empresta via `with_dom`.
    static DOMS: RefCell<HashMap<u64, Dom>> = RefCell::new(HashMap::new());
    /// Próximo handle (começa em 1; 0 = "nenhum DOM").
    static NEXT: RefCell<u64> = const { RefCell::new(1) };
}

/// Aloca um handle e guarda o `Dom`. Retorna o handle (≥ 1).
pub fn insert(dom: Dom) -> u64 {
    let h = NEXT.with(|n| {
        let mut n = n.borrow_mut();
        let h = *n;
        *n += 1;
        h
    });
    DOMS.with(|m| m.borrow_mut().insert(h, dom));
    h
}

/// Libera o `Dom` de `h` (o handle fica inválido).
pub fn remove(h: u64) {
    DOMS.with(|m| {
        m.borrow_mut().remove(&h);
    });
}

/// Empresta o `Dom` de `h` IMUTÁVEL e roda `f`. `None` se o handle não existe.
/// É como o RENDERER (rts-egui) lê a árvore para pintar — sem copiar.
pub fn with_dom<R>(h: u64, f: impl FnOnce(&Dom) -> R) -> Option<R> {
    DOMS.with(|m| m.borrow().get(&h).map(f))
}

/// Empresta o `Dom` de `h` MUTÁVEL e roda `f`. `None` se o handle não existe.
pub fn with_dom_mut<R>(h: u64, f: impl FnOnce(&mut Dom) -> R) -> Option<R> {
    DOMS.with(|m| m.borrow_mut().get_mut(&h).map(f))
}

/// `true` se `h` referencia um `Dom` vivo no store.
pub fn exists(h: u64) -> bool {
    DOMS.with(|m| m.borrow().contains_key(&h))
}
