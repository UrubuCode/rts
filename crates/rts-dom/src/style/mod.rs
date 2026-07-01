//! Engine de estilo CSS NATIVO (puro RTS) — EGUI-FREE.
//!
//! Tipos PRÓPRIOS, nunca tipos do egui (`Color32`/`FontId`/`Vec2`): a cor é um
//! `u32` RGBA (`0xRRGGBBAA`), o tamanho um `f32`. Isso é deliberado e é uma
//! condição de aceite do roadmap (F0(d)): se este módulo dependesse do egui, a
//! separação "o motor de estilo é independente do backend de render" viraria
//! mentira (cai o argumento anti-`rts-html`). A conversão para os tipos do egui
//! acontece NO RENDER (`frame/render.rs`), não aqui.
//!
//! ## Organização (a antiga `style.rs` monolítica, dividida por responsabilidade)
//!
//! - [`props`] — **a TABELA de propriedades** (`css_props!`): a fonte única que
//!   declara cada propriedade (campo, tipo, herdável?, animável?) e GERA a struct
//!   `ComputedStyle` + merge/herança/diff/interpolação. Propriedade nova começa lá.
//! - [`values`] — os tipos de valor (Rgba, Dimension, Edges, enums de keyword…).
//! - [`parse`] — declarações CSS → campos (o `style=""` e o corpo `{…}` das regras;
//!   shorthands expandem aqui).
//! - [`fmt`] — valores computados → string no formato do browser (getComputedStyle).
//! - [`color`] — parse de cor (hex/rgb()/hsl()/nomes).
//! - [`selector`] — seletores (simples/compostos/combinadores/attr/pseudo).
//! - [`stylesheet`] — as regras do `<style>`, a cascade (normal/!important) e
//!   `@keyframes`.
//! - [`lerp`] — a regra de interpolação POR TIPO ([`lerp::AnimValue`]) que a tabela
//!   usa para animar.
//!
//! Duas fontes de estilo, ambas produzindo o mesmo `ComputedStyle`:
//! - `parse_inline`: parse do atributo `style="..."` (CSS string).
//! - `apply_slot`: aplicação de um SLOT NUMÉRICO OPACO (invariante 4 — o Rust
//!   nunca casa nome CSS na fronteira ABI; o TS mapeia nome→índice). Base do
//!   `defineStyle` (F1).

pub mod color;
pub mod fmt;
pub mod lerp;
pub mod parse;
pub mod props;
pub mod selector;
pub mod stylesheet;
pub mod values;

#[cfg(test)]
mod tests;

// A API pública é a MESMA da antiga `style.rs` monolítica — os consumidores
// (`dom.rs`, `layout.rs`, `abi.rs`, `scrollbar.rs`, `anim.rs`, rts-egui) seguem
// usando `crate::style::X` sem mudança.
pub use color::parse_color;
pub use lerp::{lerp_color, lerp_dimension, lerp_f32};
pub use parse::{is_mono_family, parse_inline, parse_inline_block};
pub use props::{
    define_style, lookup_style, ComputedStyle, SLOT_BG, SLOT_BORDER_COLOR, SLOT_BORDER_WIDTH,
    SLOT_COLOR, SLOT_CORNER_RADIUS, SLOT_FONT_SIZE, SLOT_MARGIN, SLOT_MARGIN_V, SLOT_PADDING,
    SLOT_WIDTH,
};
pub use selector::{
    compound_matches, parse_selector, parse_selector_list, AttrOp, Combinator, ComplexSelector,
    CompoundSelector, PseudoClass, Selector, SimpleSelector,
};
pub use stylesheet::{parse_rules, DeclBlock, MediaQuery, Rule, Stylesheet};
pub use values::{
    clamp_size, AlignItems, BorderStyle, CalcLen, Dimension, DisplayKind, Edges, FlexDirection, FloatSide,
    JustifyContent, LineHeight, Position, ResolveCtx, Rgba, Side, TextAlign, TextTransform,
    WhiteSpace, DIM_BASE_EM, DIM_BASE_PERCENT, DIM_BASE_PX, DIM_BASE_REM, DIM_BASE_VH,
    DIM_BASE_VW, DIM_RANGE,
};
