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

pub mod background;
pub mod borders;
pub mod calc;
pub mod color;
pub mod cores_nomeadas;
/// O nome de uma declaração (tokens antes do `:`) — ver o módulo.
mod declaracao_nome;
pub mod decoracao;
pub mod effects;
pub mod fmt;
pub mod fmt_values;
pub mod grid_areas;
pub mod grid_lines;
/// As propriedades reconhecidas e deliberadamente não modeladas — ver o módulo.
pub mod inert;
pub mod inherit_kw;
pub mod initial;
pub mod lengths;
pub mod lerp;
/// `inset*` e as bordas lógicas (`-inline-`/`-block-`) — ver o módulo.
pub mod logical;
pub mod painting;
pub mod parse;
pub mod props;
/// Os raios POR CANTO (`border-top-left-radius` e as sete companhias).
pub mod radius;
pub mod root_font;
pub mod ruleindex;
pub mod selector;
pub mod stylesheet;
/// Tokenizer e AST sintáctico lossless do CSS, antes do lowering para a cascade.
pub mod syntax;
/// As propriedades de TABELA e a posição do marcador de lista — ver o módulo.
pub mod tables;
pub mod text;
pub mod text_metrics;
/// As longhands de `transition-*`/`animation-*` — ver o módulo.
pub mod timing;
pub mod ua;
pub mod values;
pub(crate) mod vars;
/// O vocabulário do 2º lote de propriedades (keywords novos) — ver o módulo.
pub mod vocab;

mod aplica;

#[cfg(test)]
mod ast_tests;
#[cfg(test)]
mod afirmacoes_tests;
#[cfg(test)]
mod auditoria_lote_a;
#[cfg(test)]
mod auditoria_lote_b;
#[cfg(test)]
mod computed_tests;
#[cfg(test)]
mod sonda_efetivos;
#[cfg(test)]
mod newprops_tests;
#[cfg(test)]
mod selector_tests;
#[cfg(test)]
mod tests;

// A API pública é a MESMA da antiga `style.rs` monolítica — os consumidores
// (`dom.rs`, `layout.rs`, `abi.rs`, `scrollbar.rs`, `anim.rs`, rts-egui) seguem
// usando `crate::style::X` sem mudança.
pub use background::{BgPosition, BgRepeat, BgSize};
pub use borders::{SideBorder, SideName};
pub use color::parse_color;
pub use grid_areas::{GridArea, GridAreas};
pub use lerp::{lerp_color, lerp_dimension, lerp_f32};
pub use parse::{is_mono_family, parse_inline, parse_inline_block, parse_inline_specified};
pub use props::{
    ComputedStyle, SLOT_BG, SLOT_BORDER_COLOR, SLOT_BORDER_WIDTH, SLOT_COLOR, SLOT_CORNER_RADIUS,
    SLOT_FONT_SIZE, SLOT_MARGIN, SLOT_MARGIN_V, SLOT_PADDING, SLOT_TEXT_ALIGN,
    SLOT_TEXT_DECORATION, SLOT_WIDTH, define_style, define_style_font_px, lookup_style,
};
pub use root_font::{root_font_size, set_root_font_size};
pub use selector::{
    AttrOp, Combinator, ComplexSelector, CompoundSelector, PseudoClass, PseudoElement, Selector,
    SimpleSelector, compound_matches, compound_matches_borrowed, parse_selector,
    parse_selector_list,
};
pub use stylesheet::{
    CustomPropertyRegistry, DeclBlock, HoverReach, MatchedRules, MediaContext, MediaQuery,
    PrefersColorScheme, PropertySyntax, RegisteredProperty, Rule, Stylesheet, parse_rules,
};
pub use syntax::{
    AstItem, BlockAst, ComponentValue, DeclarationAst, Diagnostic, DiagnosticSeverity, SourceSpan,
    SpecifiedStyle, StylesheetAst, Token, TokenKind, tokenize,
};
pub use tables::{BorderCollapse, BorderSpacing, ListStylePosition, TableLayout};
pub use text::{Clear, Direction, ListStyleType, OverflowWrap, VerticalAlign, WordBreak, WritingMode};
pub use text_metrics::{
    ASCENT_RATIO, DESCENT_RATIO, MONO_ADVANCE, PROP_ADVANCE, SUB_OFFSET_RATIO,
    SUPER_OFFSET_RATIO, X_HEIGHT_RATIO, normal_line_height, spacing_width,
};
pub use values::{
    AlignItems, BorderStyle, CalcLen, DIM_BASE_EM, DIM_BASE_PERCENT, DIM_BASE_PX, DIM_BASE_REM,
    DIM_BASE_VH, DIM_BASE_VW, DIM_RANGE, Dimension, DisplayKind, Edges, FlexDirection, FloatSide,
    GridTrack, JustifyContent, LineHeight, Position, ResolveCtx, Rgba, Side, TextAlign, TrackBound,
    TextTransform, WhiteSpace, clamp_size, dimensao_absoluta,
};
