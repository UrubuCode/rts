//! `::before`/`::after` de um contentor FLEX são itens flex (CSS Flexbox §4:
//! "each in-flow child, including generated content, becomes a flex item").
//!
//! É o caret `.dropdown-toggle::after` do Bootstrap dentro de um `<button
//! class="d-flex">`: um inline-block só de bordas que não entrava na linha de
//! itens, e o botão media 42 onde o Blink dá 54 (`claude-pseudo-item-flex`).
//! Um pseudo-elemento não é um nó, e o `layout_block` não lhe dá caixa; o que
//! um item gerado precisa é pouco — a sua largura e altura (conteúdo, padding,
//! borda, margem) e uma pintura de fundo, bordas e texto — e é isso que vive
//! aqui, fora de `flex.rs`, que está no teto. CORTE dito: `border-radius`,
//! `flex-basis` e `min/max-width` do pseudo não entram; o texto não quebra.
//!
//! A medição de padding/borda/margem e a pintura são as MESMAS de
//! `pseudo_bloco.rs` — extraídas para `pseudo_caixa.rs` no lote BT-5 (issue
//! #2731), que era onde as duas cópias byte-a-byte viviam antes. O que este
//! ficheiro NÃO partilha com esse, de propósito: a largura por omissão
//! encolhe ao texto (shrink-to-fit, Flexbox §9.2) em vez de encher o
//! contentor, e não há colapso de margem nenhum (Flexbox §4) — um item flex
//! usa `ml`/`mr`/`mt`/`mb` tal como resolvidos, nunca os funde com o vizinho.

use super::*;
use crate::layout::block::pseudo_box::{montar, resolve_arestas};

/// Um item gerado já medido: a caixa OUTER (com margens) que ocupa na linha.
/// Estrutura partilhada com `crate::layout::block::pseudo_block::PseudoBlockBox` — ver
/// `crate::layout::block::pseudo_box::CaixaGerada`.
pub(in crate::layout) type PseudoItem = crate::layout::block::pseudo_box::CaixaGerada;

fn rc(css: &ComputedStyle, base_w: f32, fonte: f32, ctx: &LayoutCtx) -> ResolveCtx {
    ResolveCtx {
        parent_content_w: base_w,
        node_font_size: fonte,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    }
}

/// Mede o pseudo-elemento `pe` do contentor `id` como item flex, se existir
/// (tem `content`) e não for `display: none`.
/// `dono` is the container's box, where the tree put the generated box; the
/// pseudo is taken from there (`crate::layout::block::pseudo_box::da_arvore`), not re-derived.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn medir(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    dono: crate::boxes::BoxId,
    pe: crate::style::PseudoElement,
    base_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> Option<PseudoItem> {
    let (gerada, caixa) = crate::layout::block::pseudo_box::da_arvore(dom, tree, dono, pe)?;
    if caixa.css.effective_display() == Some(crate::style::DisplayKind::None) {
        return None;
    }
    let css = &caixa.css;
    let fonte = font_px(css, font_size);
    let r = rc(css, base_w, fonte, ctx);
    let arestas = resolve_arestas(css, &r);
    let texto = crate::layout::inline::segment::collapse_ws(&caixa.texto, false).into_owned();
    let mono = css.font_family.as_deref().is_some_and(crate::style::is_mono_family);
    let bold = css.bold.unwrap_or(false);
    let tw = if texto.is_empty() {
        0.0
    } else {
        ctx.measurer.text_width_family(&texto, fonte, css.font_family.as_deref(), mono, bold, false)
    };
    // ITEM FLEX: largura AUTO encolhe ao conteúdo (shrink-to-fit) — o oposto
    // do bloco em `pseudo_bloco.rs::medir`, que enche o pai. É a única conta
    // que os dois papéis não partilham (ver `pseudo_caixa.rs`).
    let conteudo_w = css.width.and_then(|d| d.resolve(&r)).unwrap_or(tw);
    let conteudo_h = css.height.and_then(|d| d.resolve(&r)).unwrap_or(if texto.is_empty() {
        0.0
    } else {
        crate::inline_box::altura_da_linha(css, fonte, ctx.measurer)
    });
    // One line, not `linhas_do_texto`: this item's width IS the text's
    // max-content width, and breaking at it could split on a rounding
    // difference between the two measurers (`text_width_family` here,
    // `text_width` in `wrap_runs`) — a line the height above did not count.
    let linhas = if texto.is_empty() { Vec::new() } else { vec![texto] };
    Some(montar((gerada, caixa), arestas, conteudo_w, conteudo_h, linhas, fonte))
}

/// A largura OUTER que o pseudo `pe` acrescenta à largura intrínseca de um
/// contentor flex em linha (zero se não existe).
pub(in crate::layout) fn largura(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    dono: crate::boxes::BoxId,
    pe: crate::style::PseudoElement,
    font_size: f32,
    ctx: &LayoutCtx,
) -> f32 {
    medir(dom, tree, dono, pe, ctx.viewport_w, font_size, ctx).map_or(0.0, |p| p.w)
}

/// Pinta o item gerado com o canto superior esquerdo da sua margin box em
/// (`x`, `y`) — repassa a `crate::layout::block::pseudo_box::pintar`, mantida aqui como um nome
/// próprio porque `flex.rs` chama-a por este caminho.
pub(in crate::layout) fn pintar(list: &mut DisplayList, item: &PseudoItem, x: f32, y: f32, ctx: &LayoutCtx) {
    crate::layout::block::pseudo_box::pintar(list, item, x, y, ctx);
}

/// O item flex de um pseudo-elemento gerado do contentor, se existir.
///
/// The item's box is the GENERATED box. `flex.rs` never hands it to
/// `layout_block` — a generated item is painted by `pintar` from `pseudo` —
/// but it is the box the item is, and the one its geometry is recorded under.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn item_flex(dom: &Dom, tree: &crate::boxes::BoxTree, dono: crate::boxes::BoxId, id: NodeIdx, pe: crate::style::PseudoElement, content_w: f32, font_size: f32, ctx: &LayoutCtx) -> Option<super::row::FlexItem> {
    let p = medir(dom, tree, dono, pe, content_w, font_size, ctx)?;
    let css = &p.caixa.css;
    Some(super::row::FlexItem {
        node: id,
        caixa: p.gerada,
        base: p.w,
        main: p.w,
        h: p.h,
        is_text: false,
        grow: css.flex_grow.unwrap_or(0.0),
        shrink: css.flex_shrink.unwrap_or(1.0),
        align_self: css.align_self,
        order: css.order.unwrap_or(0),
        can_stretch: false,
        min_main: p.w,
        max_main: None,
        auto_esq: false,
        auto_dir: false,
        auto_topo: false,
        auto_fundo: false,
        pseudo: Some(p),
    })
}
