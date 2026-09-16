//! `position: relative` num átomo de linha (`Widget`/`Replaced`).
//!
//! `layout_input`/`layout_button` (Widget) e `layout_image`/`layout_canvas`
//! (Replaced) pintam a caixa na posição NATURAL da linha e não sabem de
//! `position:relative` — só `bloco.rs` a aplica, e só para o que passa pelo
//! caminho de bloco (`AtomicKind::Block`, via `layout_block`). Um `<img>` ou
//! `<input>` inline com `left`/`top` era lido pelo cascade e nunca pintado.
//!
//! Foi o que fazia `line-height-069.xht` (WPT, CSS2/linebox) divergir da sua
//! própria referência: `line-height-002-ref.xht` — e as réguas -006/-025/
//! -061/-069/-072/-102/-105 geradas a partir do mesmo padrão, 56 dos
//! ficheiros da família `line-height` — usam DOIS `<img>` lado a lado, o
//! segundo com `position:relative;left:2px`, só para separar visualmente as
//! duas caixas que o teste compara. Sem este deslocamento as duas imagens
//! colam-se (0px de vão em vez de 2px) e a referência deixa de bater com o
//! teste, que posiciona a SUA caixa de comparação por `left` normal (fluxo,
//! não `relative`) — o mesmo vão, por um caminho que já funcionava.
//!
//! Módulo à parte (e não inline em `linha.rs`, que já está no tecto de 500
//! linhas do resto do workspace) para que a lógica nova entre como chamada,
//! não como mais peso no ficheiro que já ultrapassava o limite.

use super::super::relativo::aplica_offset_relativo;
use super::super::{AtomicKind, DisplayList, LayoutCtx, font_px};
use crate::dom::{Dom, NodeIdx};

/// Aplica o deslocamento de `position:relative`, se houver, ao átomo
/// `a_idx` já pintado entre `box_index` e o fim actual de `list.items`.
///
/// Sem efeito para `Marker`/`Break`/`ArestaInicio`/`ArestaFim` (não pintam
/// caixa própria) e para `Block` (já tratado por `bloco.rs`, dentro de
/// `layout_block`) — aplicar aqui também duplicaria o deslocamento.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn aplica_a_atomo(
    dom: &Dom,
    a_idx: NodeIdx,
    kind: AtomicKind,
    font_size: f32,
    content_w: f32,
    box_index: usize,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    if !matches!(kind, AtomicKind::Widget | AtomicKind::Replaced) {
        return;
    }
    let css = dom.computed_style_idx(a_idx).unwrap_or_default();
    let fs = font_px(&css, font_size);
    aplica_offset_relativo(dom, a_idx, &css, content_w, None, fs, box_index, ctx, list);
}
