//! `::before`/`::after` com `display:block` (ou outro papel de bloco) e
//! `content` NÃO-VAZIO: uma caixa de BLOCO própria, medida e pintada como um
//! filho de bloco normal — o que faltava ao lado do caminho inline
//! (`runs.rs`, um átomo de texto) e do clearfix (`clearfix.rs`, que só lê o
//! EFEITO de um `content:""` com `clear`, nunca desenha uma caixa).
//!
//! É a referência do WPT `flexbox_nested-flex.html`: um `<div>` sem filhos
//! reais e `::after{content:"x";display:block;width:200px;height:40px;
//! margin:8px}` — a caixa do pai (`claude-pseudo-after-display-block`) tem
//! de refletir a caixa GERADA (56 = 8+40+8, contida porque `overflow:hidden`
//! estabelece um BFC próprio e a margem do pseudo não escapa para fora).
//!
//! O pseudo entra na MESMA máquina de colapso de margem que um filho real —
//! `Strut`/`join_strut`/`collapsed_strut`/`collapses_through`, reusadas de
//! `vertical_flow.rs` — porque ele PARTICIPA do fluxo como qualquer outro bloco
//! (CSS 2.1 §12.1: "generated content ... treated ... as if inserted...
//! immediately before/after the ... content"), só que sem nó DOM próprio para
//! caches de fragmento ou layout recursivo. O colapso de margem é a razão
//! deste ficheiro EXISTIR separado de `flex/pseudo.rs`, apesar de os dois
//! partilharem a medição de padding/borda/margem e a pintura inteira —
//! ver `pseudo_box.rs`, que é onde essa parte comum vive agora (lote BT-5,
//! issue #2731): um item flex nunca colapsa margem (Flexbox §4), um bloco
//! colapsa sempre, e essa diferença não tem como desaparecer numa função só
//! sem deixar de ser a diferença que a spec pede.
//!
//! CORTE dito (como `flex/pseudo.rs`, o mesmo padrão para o eixo flex): sem
//! `border-radius`, sem `flex-basis`/min/max no eixo do pseudo (the text
//! does wrap now, at the content width — `pseudo_box::text_lines`),
//! e o papel `display:flex`/`grid` do
//! pseudo é tratado como um bloco simples — não faz o layout flex/grid dos
//! SEUS conteúdos (que hoje é só texto, então não há filhos a dispor).

use super::*;
use super::pseudo_box::{build, resolve_edges};

/// A caixa de bloco de um pseudo-elemento, já medida: outer w/h com margens,
/// as margens por eixo (para o colapso), as arestas (borda+padding) por lado
/// e o texto a pintar. Estrutura partilhada com `crate::layout::flex::pseudo::PseudoItem` —
/// ver `pseudo_box::GeneratedBox`.
pub(in crate::layout) type PseudoBlockBox = super::pseudo_box::GeneratedBox;

/// O pseudo `pseudo_el` de `id`, se existe, tem conteúdo NÃO-VAZIO e é de BLOCO — a
/// mesma pergunta de `clearfix.rs::clearfix_bottom`, sem exigir `clear` e
/// exigindo texto (um `content:""` é o caso do clearfix, já coberto por ele;
/// os dois nunca disputam o mesmo pseudo).
#[allow(clippy::too_many_arguments)]
fn measure_pseudo(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    owner: crate::boxes::BoxId,
    pseudo_el: crate::style::PseudoElement,
    content_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> Option<PseudoBlockBox> {
    let (generated, box_id) = super::pseudo_box::from_tree(dom, tree, owner, pseudo_el)?;
    if box_id.text.is_empty() {
        return None;
    }
    let block_level = matches!(
        box_id.css.effective_display(),
        Some(
            crate::style::DisplayKind::Block
                | crate::style::DisplayKind::Flex
                | crate::style::DisplayKind::Grid
        )
    );
    if !block_level {
        return None;
    }
    let css = &box_id.css;
    let font = font_px(css, font_size);
    let r = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let edges = resolve_edges(css, &r);
    let source_text = crate::layout::inline::segment::collapse_ws(&box_id.text, false).into_owned();
    // BLOCO: largura AUTO enche o content-box do pai (menos as margens) — o
    // default de qualquer bloco sem `width`. `flex/pseudo.rs::measure` encolhe
    // ao conteúdo porque ali o pseudo é um ITEM flex (shrink-to-fit); este é
    // o outro papel, o de CONTENTOR de bloco normal. É a única conta que os
    // dois ficheiros não partilham (ver `pseudo_box.rs`).
    let inner_w = css.width.and_then(|d| d.resolve(&r)).unwrap_or_else(|| {
        (content_w - edges.ml - edges.mr - edges.values[1] - edges.values[3]).max(0.0)
    });
    let lines = super::pseudo_box::text_lines(css, &source_text, inner_w, font, ctx);
    let inner_h = css
        .height
        .and_then(|d| d.resolve(&r))
        .unwrap_or_else(|| super::pseudo_box::lines_height(css, &lines, font, ctx));
    Some(build((generated, box_id), edges, inner_w, inner_h, lines, font))
}

/// Mede, posiciona e pinta o pseudo `pseudo_el` de `id` como o próximo (`::before`)
/// ou o último (`::after`) filho do fluxo vertical — a MESMA máquina de
/// colapso de margem que `vertical_flow.rs` usa para um filho real de bloco
/// (`border`/`strut`/`child_y` são os três valores dela). Não faz nada
/// (`border`/`strut`/`child_y` inalterados) quando o pseudo não existe, tem
/// `content` vazio ou não é de bloco — ver [`measure_pseudo`].
///
/// Gancho de UMA chamada em `vertical_flow.rs`, que não cresce: a lógica inteira
/// vive aqui.
pub(in crate::layout) fn apply(
    dom: &Dom,
    // The box of `id` this flow is the children of — where the tree put the
    // generated box (`boxes/build/generated.rs`). `None` without a tree.
    owner: crate::boxes::BoxId,
    id: NodeIdx,
    pseudo_el: crate::style::PseudoElement,
    content_x: f32,
    content_w: f32,
    font_size: f32,
    border: &mut f32,
    strut: &mut super::vertical_flow::Strut,
    child_y: &mut f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    use super::vertical_flow::{collapses_through, join_strut, collapsed_strut};
    let from_tree = std::rc::Rc::clone(&list.tree);
    let Some(box_id) = measure_pseudo(dom, &from_tree, owner, pseudo_el, content_w, font_size, ctx) else {
        return;
    };
    let (m, m_bottom) = (box_id.mt, box_id.mb);
    let with_top = join_strut(*strut, m);
    let edge = *border + collapsed_strut(with_top);
    let y = edge - m;
    super::pseudo_box::paint(list, &box_id, content_x, y, ctx);
    if collapses_through(box_id.h, m, m_bottom) {
        *strut = join_strut(with_top, m_bottom);
    } else {
        *border = edge + (box_id.h - m - m_bottom);
        *strut = join_strut((0.0, 0.0), m_bottom);
    }
    *child_y = *border + collapsed_strut(*strut);
}
