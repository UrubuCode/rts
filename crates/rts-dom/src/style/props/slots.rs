//! Os números de slot da ABI, o estilo por TAG e o epoch de estilo
//!
//! Extraído de `props.rs` sem alterar uma linha.

use super::*;

// ── Slots numéricos opacos (invariante 4) ──────────────────────────────────────
// O Rust NUNCA casa string CSS (`"background-color"`) na fronteira ABI; o TS mapeia
// nome→índice e chama `defineStyle(tag, slot, val)`. Adicionar `box-shadow` =
// registrar um slot no TS, sem tocar aqui. Estes códigos são o contrato com a
// camada TS.
pub const SLOT_COLOR: i64 = 0;
pub const SLOT_BG: i64 = 1;
pub const SLOT_FONT_SIZE: i64 = 2;
// Box model (F2):
pub const SLOT_PADDING: i64 = 3;
pub const SLOT_MARGIN: i64 = 4;
pub const SLOT_BORDER_WIDTH: i64 = 5;
pub const SLOT_BORDER_COLOR: i64 = 6;
pub const SLOT_CORNER_RADIUS: i64 = 7;
/// `width`: o `val` é a `Dimension` codificada (Px = pontos diretos; Percent =
/// faixa própria; Auto/não-especificado = `-1`). Ver [`Dimension::from_abi`].
pub const SLOT_WIDTH: i64 = 8;
/// `margin_v`: margem VERTICAL apenas (top/bottom), em pontos. A UA-stylesheet usa
/// para separar blocos sem deslocar no eixo horizontal.
pub const SLOT_MARGIN_V: i64 = 9;
/// `text-align`: 0=left 1=center 2=right. A UA-stylesheet usa p/ `<center>`
/// (a tag legada = text-align:center herdável, o suficiente p/ páginas
/// anos-2000 como a home legada do google).
pub const SLOT_TEXT_ALIGN: i64 = 10;
/// `text-decoration`: 0=none 1=underline 2=line-through. UA usa p/ `<a>`.
pub const SLOT_TEXT_DECORATION: i64 = 11;

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    /// Mapa `tag → ComputedStyle`, povoado pelo TS via `defineStyle(tag, slot, val)`.
    /// É o estilo POR-TAG (uma UA-stylesheet de estilo, paralela ao `block::BLOCKS`
    /// de layout). O render consulta `lookup_style(tag)` e aplica antes do
    /// `style=""` inline do nó. Vazio até o TS registrar.
    static STYLES: RefCell<HashMap<String, ComputedStyle>> = RefCell::new(HashMap::new());
    /// EPOCH global de estilo por-tag: bumpado por `defineStyle`/`defineBlock`
    /// (estado que vive FORA do `Dom` mas muda o computed). Entra na
    /// `Dom::render_revision` para invalidar os caches de layout/estilo.
    static STYLE_EPOCH: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// O epoch global do estilo por-tag (ver [`bump_style_epoch`]).
pub fn style_epoch() -> u64 {
    STYLE_EPOCH.with(|e| e.get())
}

/// Bumpa o epoch global — chamado por `defineStyle` (aqui) e `defineBlock`
/// (`crate::block`), que alteram estilo/layout sem passar por um `Dom`.
pub fn bump_style_epoch() {
    STYLE_EPOCH.with(|e| e.set(e.get().wrapping_add(1)));
}

/// Registra/atualiza UM slot de estilo de uma TAG (primitivo `defineStyle`).
/// ACUMULA: chamar com slots diferentes na mesma tag mantém os anteriores
/// (`defineStyle("h1",0,cor)` + `defineStyle("h1",2,tam)` → cor E tamanho). O
/// `(slot, val)` é opaco (invariante 4); o Rust nunca vê o nome CSS.
pub fn define_style(tag: &str, slot: i64, val: i64) {
    STYLES.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.entry(tag.to_ascii_lowercase()).or_default();
        entry.apply_slot(slot, val);
    });
    bump_style_epoch();
}

/// Regista o `font-size` default de uma TAG em px FRACIONÁRIOS.
///
/// Existe ao lado do [`define_style`] por um motivo medido: o slot opaco carrega
/// um `i64`, e a fonte que o browser dá aos controlos de formulário é
/// **13,3333px** — truncá-la para 13 põe o valor computado a 0,33px do Chrome em
/// todo `<input>` da página. O slot continua a ser a via da camada TS (invariante
/// 4); isto é a via da UA-stylesheet interna, que é Rust e não precisa de
/// atravessar a fronteira ABI.
pub fn define_style_font_px(tag: &str, px: f32) {
    STYLES.with(|m| {
        let mut m = m.borrow_mut();
        let entry = m.entry(tag.to_ascii_lowercase()).or_default();
        entry.font_size = Some(Dimension::Px(px));
    });
    bump_style_epoch();
}

/// Consulta o `ComputedStyle` registrado de uma TAG. `None` ⇒ sem estilo de tag.
pub fn lookup_style(tag: &str) -> Option<ComputedStyle> {
    STYLES.with(|m| m.borrow().get(tag).cloned())
}
