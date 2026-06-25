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

pub use dom::{parse_html_to_dom, Attr, Dom, Node, NodeId, NodeIdx, NodeKind};

pub use abi::register;

/// Prelude `.ts` da FACHADA DOM ergonômica (`document` global + `Element`, com a
/// API/nomes do browser) sobre os primitivos do namespace `rts:dom`. Incluído via
/// `Engine::include` na tabela `PRELUDE_TS` — DEPOIS do registro do ns `dom`.
pub const DOM_TS: &str = include_str!("dom.ts");
