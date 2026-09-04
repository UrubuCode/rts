//! O tamanho TRANSFERIDO de um `<img>` sem `width`/`height` quando o eixo
//! cruzado do contentor flex-ROW que o encolhe já é DEFINITE (Flexbox §9.2,
//! "flex base size ... calculated from its inner cross size and the flex
//! item's intrinsic aspect ratio"): a largura vem de altura × razão em vez
//! do tamanho NATURAL dos pixels, porque é isso que o item vai ocupar depois
//! do `align-items: stretch` (o default) esticar o eixo cruzado.
//!
//! Duas perguntas, uma implementação (`transferido`, abaixo): o ITEM em si —
//! usado no pré-passo de `flex.rs`, onde a altura do contentor já é um
//! parâmetro — e o CONTENTOR que encolhe ao conteúdo
//! (`largura_intrinseca_transferida`, usado por `bloco.rs`/`flex_limites.rs`
//! ANTES do layout real: sem isto um `<div style="display:flex;height:100px">`
//! só com uma imagem lá dentro mede-se pela largura NATURAL da imagem — 1px
//! para um PNG 1×1 — e nunca chega a oferecer ao filho o espaço que ele
//! precisava de ocupar. `bloco.rs`/`medida.rs` estão no tecto de 500/1000
//! linhas e não crescem: só chamam esta função.

use super::*;

/// `(largura outer, altura outer)` de um `<img>` cujo eixo cruzado (a
/// altura, num flex-ROW) já é `cross_h` — ou `None` quando a regra não se
/// aplica: não é `<img>`, já declara `width` OU `height` (CSS ou atributo
/// HTML — a mesma prioridade de `replaced_inline_size`), não tem pixels
/// descodificados (sem razão de aspeto), ou `cross_h` não é positivo. Nesses
/// casos o caminho normal (`replaced_inline_size` a partir do tamanho
/// natural) decide sozinho.
pub(in crate::layout) fn transferido(
    dom: &Dom,
    id: NodeIdx,
    content_w: f32,
    cross_h: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> Option<(f32, f32)> {
    if cross_h <= 0.0 {
        return None;
    }
    let NodeKind::Element { tag } = &dom.node(id).kind else {
        return None;
    };
    if tag != "img" {
        return None;
    }
    let css = dom.computed_style_idx(id).unwrap_or_default();
    if css.width.is_some() || css.height.is_some() {
        return None;
    }
    let node = dom.node(id);
    if node.attr("width").is_some() || node.attr("height").is_some() {
        return None;
    }
    let (nw, nh) = dom.image_dims(id).filter(|(w, h)| *w > 0 && *h > 0)?;
    let font = font_px(&css, font_size);
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let margin_h = css.margin.resolve_h(&resolve);
    let mt = css.margin.top.resolve(&resolve).unwrap_or(0.0);
    let mb = css.margin.bottom.resolve(&resolve).unwrap_or(0.0);
    let bordas = crate::style::borders::resolved_sides(&css);
    let px = |b: crate::style::borders::SideBorder| if b.paints() { b.width } else { 0.0 };
    let (bt, br, bb, bl) = (px(bordas[0]), px(bordas[1]), px(bordas[2]), px(bordas[3]));
    let h_content = (cross_h - mt - mb - bt - bb).max(0.0);
    let w_content = h_content * nw as f32 / nh as f32;
    Some((w_content + margin_h + bl + br, cross_h))
}

/// `(base, h)` de um item do pré-passo de `flex.rs`: a versão TRANSFERIDA
/// (`transferido`, acima) quando o item é um `<img>` esticado no eixo
/// cruzado, senão a de sempre (`flex_base_outer`/`child_outer_height`) — uma
/// chamada só, para o pré-passo (que já está no tecto de 500 linhas) não
/// crescer com uma pergunta que já vive aqui.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn base_e_altura_do_item(
    dom: &Dom,
    child: NodeIdx,
    content_w: f32,
    container_content_h: Option<f32>,
    align_efetivo: crate::style::AlignItems,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    let t = (align_efetivo == crate::style::AlignItems::Stretch)
        .then_some(container_content_h)
        .flatten()
        .and_then(|h| transferido(dom, child, content_w, h, font_size, ctx));
    match t {
        Some((w, h)) => (w, h),
        None => (
            super::flex_limites::flex_base_outer(dom, child, content_w, font_size, ctx),
            super::medida::child_outer_height(dom, child, content_w, container_content_h, css, font_size, ctx),
        ),
    }
}

/// A largura OUTER de um contentor flex-ROW que encolhe ao conteúdo
/// (shrink-to-fit), quando pelo menos um filho `<img>` se beneficia de
/// [`transferido`] — ou `None` para deixar o caminho normal e CACHED
/// (`intrinsic_content_width`) decidir, que é o que faz na esmagadora
/// maioria dos contentores (sem isto o corte seria testar isto em CADA
/// contentor, não só nos que têm uma imagem sem tamanho lá dentro).
///
/// `own_cross_h`: a altura de conteúdo JÁ CONHECIDA de `id`, quando o
/// chamador a tem à mão (o `forced_outer_h` de um esticado por
/// `position:absolute` com os dois insets, `bloco.rs`) — senão `None` e esta
/// função tenta a altura DECLARADA do próprio `id` (`#dentro{height:100px}`
/// como item de outro flex: a altura não depende de nada externo).
pub(in crate::layout) fn largura_intrinseca_transferida(
    dom: &Dom,
    id: NodeIdx,
    font: f32,
    own_cross_h: Option<f32>,
    ctx: &LayoutCtx,
) -> Option<f32> {
    let display = css_display(dom, id);
    if display != crate::block::DISPLAY_HORIZONTAL && display != crate::block::DISPLAY_WRAP {
        return None;
    }
    let h = own_cross_h.or_else(|| {
        let css = dom.computed_style_idx(id)?;
        let resolve = ResolveCtx {
            parent_content_w: 0.0,
            node_font_size: font_px(&css, font),
            root_font_size: crate::style::root_font_size(),
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        resolve_height(css.height, None, &resolve)
    })?;
    if h <= 0.0 {
        return None;
    }
    let resolve = ResolveCtx {
        parent_content_w: ctx.viewport_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let gap = dom
        .computed_style_idx(id)
        .and_then(|c| c.gap)
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let mut sum = 0.0f32;
    let mut count: usize = 0;
    let mut algum_transferido = false;
    for &child in &dom.node(id).children {
        if is_out_of_flow(dom, child) {
            continue;
        }
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) || e_display_none(dom, child) {
                continue;
            }
        }
        let w = match transferido(dom, child, f32::INFINITY, h, font, ctx) {
            Some((w, _)) => {
                algum_transferido = true;
                w
            }
            None => intrinsic_outer_width(dom, child, font, ctx),
        };
        if w > 0.0 {
            count += 1;
        }
        sum += w;
    }
    if !algum_transferido {
        // nenhum filho precisava da regra — o caminho cached decide igual,
        // e ele é o que os outros milhares de contentores já usam.
        return None;
    }
    Some(sum + (count.saturating_sub(1)) as f32 * gap)
}
