//! `window.location.hash =` — o único membro deste namespace (lote O).
//!
//! Módulo À PARTE em vez de mais uma entrada em `events.rs`: aquele já está
//! acima do teto de 500 linhas, e um `:target` não é um evento — é estado de
//! DOCUMENTO (o fragmento da URL), mais perto do que `scroll.rs` já separa por
//! razão igual.

use rts_core::entry::Provided;

use crate::value::{handle, nothing, text};

pub const MEMBERS: &[(&str, Provided)] = &[("setLocationHash", set_location_hash)];

/// `setLocationHash(doc, hash)` — liga `window.location.hash = v` (em
/// `window.ts`) a `Dom::set_location_hash`, que alimenta `:target`.
extern "C" fn set_location_hash(_e: u64, _t: u64, doc: u64, hash: u64, _b: u64, _c: u64) -> u64 {
    let h = handle(doc);
    let hash = text(hash);
    rts_dom::store::with_dom_mut(h, |d| d.set_location_hash(&hash));
    nothing()
}
