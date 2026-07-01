//! `rts-dom` — DOM retido (árvore de elementos HTML) como estrutura de dados PURA,
//! independente de qualquer backend de render ou janela.
//!
//! ## Por que um crate separado
//!
//! O DOM (parser HTML + árvore em arena + `NodeId` versionado + query/mutação) não
//! tem nada de UI: é manipulação de uma árvore de dados. Mantê-lo num crate de UI
//! (`rts-egui`) o prendia à janela (o `Dom` vivia no `UiCtx`, toda a API exigia um
//! handle de janela), impedindo reuso headless. Extraído aqui, ele pode ser usado:
//!
//! - **headless pelo TS** via o namespace `rts:dom` (parse/query/mutação SEM abrir
//!   janela — ver [`abi`]); e
//! - **consumido pelo `rts-egui`** para renderizar (o `frame/render.rs` lê esta
//!   árvore; o egui é só mais um consumidor, não o dono).
//!
//! ## Doutrina
//!
//! Este crate NÃO conhece egui/winit/wgpu. Expõe primitivos via ABI de handles
//! `u64` (extern "C") através de [`register`]; a camada ergonômica
//! (`Document`/`Element`) é TS. Depende só de `rts-engine`.

pub mod abi;
mod dom;
mod html;
/// Estado de ESTILO (egui-free): `ComputedStyle`, slots opacos, parse do `style=""`
/// inline, e o registro por-tag (`defineStyle`). O DOM é dono do estilo; o renderer
/// (egui) só LÊ. Os tipos são próprios (`u32` RGBA), nunca tipos de backend.
pub mod style;
/// INTENT de layout por-tag (egui-free): `BlockDef` (display/indent/prefix/flags),
/// o registro (`defineBlock`/`defineInline`) e os códigos de display. O DOM diz
/// "esta tag é um bloco vertical / inline-flow"; o renderer decide como pintar.
pub mod block;
/// Store de `Dom`s vivos por handle — a fonte única da verdade. A ABI headless e
/// um renderer (rts-egui) acessam o MESMO `Dom` por handle (`with_dom`), então
/// mutações pela fachada `document` mudam o que a janela pinta.
pub mod store;
/// Motor de LAYOUT (egui-free): calcula a geometria (x,y,w,h) de cada nó via box
/// model + fluxo normal e emite uma `DisplayList` plana. Decisão 2026-06-27:
/// "processar tudo no DOM, o egui só lê e exibe" — o backend NÃO decide layout,
/// só pinta a display-list. Medição de texto via trait `TextMeasurer` (o backend
/// implementa; reimplementar largura de glifo aqui é a armadilha do roadmap).
pub mod layout;

/// Motor de ANIMAÇÃO (#1776): interpolação de [`style::ComputedStyle`] no tempo +
/// curvas de easing. Núcleo comum de `transition` (2 pontos) e `@keyframes` (N).
/// Egui-free; o tick por frame é dirigido pelo loop de render.
pub mod anim;

/// Estilo da SCROLLBAR via CSS (#1744): `scrollbar-width`/`scrollbar-color` (padrão)
/// + `::-webkit-scrollbar*` (WebKit). Produz um `ScrollbarStyle` neutro que o backend
/// (egui) lê e aplica — o motor não conhece o egui.
pub mod scrollbar;

/// Resolução TEMPORÁRIA de custom properties (`--x`) + `var(--x, fallback)` —
/// textual e global, para desbloquear CSS moderno (Bootstrap usa var() em ~1370
/// lugares). NÃO é a cascade de variáveis fiel à spec; ver o cabeçalho do módulo
/// e a issue de var() completo.
pub mod cssvars;

pub use dom::{parse_html_to_dom, Attr, Dom, Node, NodeId, NodeIdx, NodeKind};

pub use abi::register;

/// Prelude `.ts` da FACHADA DOM ergonômica (`document` global + `Element`, com a
/// API/nomes do browser) sobre os primitivos do namespace `rts:dom`. Incluído via
/// `Engine::include` na tabela `PRELUDE_TS` — DEPOIS do registro do ns `dom`.
pub const DOM_TS: &str = include_str!("dom.ts");
