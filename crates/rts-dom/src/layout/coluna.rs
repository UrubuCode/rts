//! FLEX em COLUNA: empilhar no eixo vertical, com `justify-content` e
//! `align-items` trocados de eixo.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução deste ficheiro é byte a byte a do original.

use super::*;
pub(in crate::layout) fn layout_children_column(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    // altura do CONTENT do container quando explícita — a referência do eixo
    // principal (justify/margin-auto) e o containing block dos filhos (height:%).
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // Em column, o espaço entre itens no eixo principal é o ROW-gap; o shorthand
    // `gap: X` seta os dois, então row_gap cobre o caso comum. (Fallback ao `gap`
    // — column-gap — só quando row_gap não veio, cobrindo `column-gap` usado
    // "errado" sem quebrar o shorthand.)
    let main_gap = css
        .row_gap
        .or(css.gap)
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let justify = css
        .justify
        .unwrap_or(crate::style::JustifyContent::FlexStart);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);

    // ── PASSO 1: mede a altura outer desejada de cada filho + margens auto ───────
    struct ColItem {
        node: NodeIdx,
        h: f32,
        is_text: bool,
        mt_auto: bool,
        mb_auto: bool,
        grow: f32,
    }
    let mut items: Vec<ColItem> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        // fora do fluxo: não é item flex (pintado na passada out-of-flow).
        if is_out_of_flow(dom, child) {
            continue;
        }
        // `display:none` não é item — mesmo motivo do eixo horizontal; aqui o
        // que ele roubava era altura e um `gap` vertical.
        if e_display_none(dom, child) {
            continue;
        }
        // Blockificação, como no eixo horizontal — ver o comentário lá.
        if matches!(dom.node(child).kind, NodeKind::Text(_)) {
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            items.push(ColItem {
                node: child,
                h: crate::inline_box::altura_da_linha(css, font_size, ctx.measurer),
                is_text: true,
                mt_auto: false,
                mb_auto: false,
                grow: 0.0,
            });
            continue;
        }
        let h = child_outer_height(
            dom,
            child,
            content_w,
            container_content_h,
            css,
            font_size,
            ctx,
        );
        let (mt_auto, mb_auto, grow) = dom
            .computed_style_idx(child)
            .map(|c| {
                (
                    c.margin.top.is_auto(),
                    c.margin.bottom.is_auto(),
                    c.flex_grow.unwrap_or(0.0),
                )
            })
            .unwrap_or((false, false, 0.0));
        items.push(ColItem {
            node: child,
            h,
            is_text: false,
            mt_auto,
            mb_auto,
            grow,
        });
    }
    if items.is_empty() {
        return 0.0;
    }

    // ── PASSO 2: distribui o espaço livre do eixo principal (Y) ──────────────────
    let n = items.len();
    let sum_h: f32 = items.iter().map(|it| it.h).sum();
    let total_gap = (n.saturating_sub(1)) as f32 * main_gap;
    let free = container_content_h
        .map(|ch| ch - sum_h - total_gap)
        .unwrap_or(0.0);
    // FLEX-GROW no eixo principal (css-flexbox §9.7): quando há espaço livre
    // positivo e algum item tem flex-grow, cada um cresce em proporção
    // `grow / soma_dos_grows * free` — dando ALTURA aos containers que os filhos
    // com `height:100%` resolvem (o logo/caixa do google centram assim). Consome
    // o `free` (o justify/margin-auto abaixo vê 0). margin:auto tem prioridade.
    let sum_grow: f32 = items.iter().map(|it| it.grow).sum();
    let any_auto = items.iter().any(|it| it.mt_auto || it.mb_auto);
    if free > 0.0 && sum_grow > 0.0 && !any_auto {
        for it in &mut items {
            if it.grow > 0.0 {
                it.h += it.grow / sum_grow * free;
            }
        }
    }
    let sum_h: f32 = items.iter().map(|it| it.h).sum();
    let free = container_content_h
        .map(|ch| ch - sum_h - total_gap)
        .unwrap_or(0.0);
    let auto_count: usize = items
        .iter()
        .map(|it| it.mt_auto as usize + it.mb_auto as usize)
        .sum();
    // margin:auto no eixo main absorve TODO o espaço livre (o justify vira no-op) —
    // spec css-flexbox §8.1. Sem autos, o justify distribui.
    let auto_size = if free > 0.0 && auto_count > 0 {
        free / auto_count as f32
    } else {
        0.0
    };
    let (leading, between) = if auto_count > 0 {
        (0.0, 0.0)
    } else {
        justify_offsets(justify, free, n)
    };

    // ── PASSO 3: posiciona e pinta ────────────────────────────────────────────────
    let mut y = content_y + leading;
    for (j, it) in items.iter().enumerate() {
        if j > 0 {
            y += main_gap + between;
        }
        if it.mt_auto {
            y += auto_size;
        }
        if it.is_text {
            let text = collect_text(dom, it.node);
            list.items.push(DisplayItem::Text {
                x: content_x,
                y,
                text: text.into(),
                color: cor_visivel(&css, css.color.unwrap_or(0x000000FF)),
                size: font_size,
                mono: false,
                bold: css.bold.unwrap_or(false),
                italic: italico(Some(&css), tag_de(dom, it.node), false),
                letter_spacing: css.letter_spacing.unwrap_or(0.0),
                decoration: decoration_code(css),
            });
        } else {
            // CROSS (X): stretch (default) → o item ocupa a largura do container
            // (layout normal de bloco); start/center/end → shrink-to-fit + offset.
            let stretch = align == crate::style::AlignItems::Stretch;
            let child_x = if stretch {
                content_x
            } else {
                let (w, _) = measure_block(
                    dom,
                    it.node,
                    content_w,
                    container_content_h,
                    None,
                    None,
                    true,
                    ctx,
                );
                let free_x = (content_w - w).max(0.0);
                content_x + align_offset(align, content_w, content_w - free_x)
            };
            // Um item que CRESCEU por flex-grow tem altura MAIOR que o conteúdo —
            // passa essa altura como containing block (avail_h) E como outer forçada
            // (forced_outer_h) para os filhos com `height:100%` resolverem contra ela.
            let (avail, forced_h) = if it.grow > 0.0 {
                (Some(it.h), Some(it.h))
            } else {
                (container_content_h, None)
            };
            layout_block(
                dom,
                it.node,
                child_x,
                y,
                content_w,
                avail,
                None,
                forced_h,
                !stretch,
                // Item de flex-column: floats não se aplicam dentro de um
                // container flex (ele já é um BFC próprio), então um contexto
                // novo e isolado por item é a mesma coisa que o `&[]` de antes.
                &BlockFormattingContext::new(),
                ctx,
                list,
            );
        }
        y += it.h;
        if it.mb_auto {
            y += auto_size;
        }
    }
    (y - content_y).max(0.0)
}

/// Calcula (leading, between) do justify-content dado o espaço livre `free` e o nº
/// de itens `n`. `leading` = offset inicial; `between` = espaço EXTRA entre itens
/// (além do gap).
///
/// OVERFLOW (free<=0): VALIDADO contra o Chrome (com `flex-shrink:0` para forçar
/// overflow real — sem isso o flex-shrink encolhe os itens e não há overflow). Os
/// três distribuidores `space-*` caem para FLEX-START ([0,100,200] no teste), e só
/// `center`/`flex-end` mantêm o leading (negativo = transborda dos dois lados/start).
/// NB: a verificação adversarial sugeriu around/evenly→center, mas o Chrome real os
/// trata como flex-start — a medição no browser desempatou.
pub(in crate::layout) fn justify_offsets(j: crate::style::JustifyContent, free: f32, n: usize) -> (f32, f32) {
    use crate::style::JustifyContent as J;
    if free <= 0.0 {
        return match j {
            J::Center => (free / 2.0, 0.0), // leading negativo = transbordo centrado
            J::FlexEnd => (free, 0.0),      // todo o overflow no start
            // flex-start E os space-* → flush no start (fiel ao Chrome em overflow).
            J::FlexStart | J::SpaceBetween | J::SpaceAround | J::SpaceEvenly => (0.0, 0.0),
        };
    }
    match j {
        J::FlexStart => (0.0, 0.0),
        J::FlexEnd => (free, 0.0),
        J::Center => (free / 2.0, 0.0),
        J::SpaceBetween => {
            if n > 1 {
                (0.0, free / (n - 1) as f32)
            } else {
                (0.0, 0.0)
            }
        }
        J::SpaceAround => {
            if n >= 1 {
                (free / (2 * n) as f32, free / n as f32)
            } else {
                (0.0, 0.0)
            }
        }
        J::SpaceEvenly => (free / (n + 1) as f32, free / (n + 1) as f32),
    }
}

/// Offset no eixo cruzado de um item, dado o align-items, a altura da linha `line_h`
/// e a altura outer do item `item_h`. (stretch é tratado como flex-start aqui — o
/// esticar real exige passar altura imposta ao layout_block, fase futura.)
pub(in crate::layout) fn align_offset(a: crate::style::AlignItems, line_h: f32, item_h: f32) -> f32 {
    use crate::style::AlignItems as A;
    let free = line_h - item_h;
    match a {
        A::Stretch | A::FlexStart => 0.0,
        A::FlexEnd => free,
        A::Center => free / 2.0,
    }
}
