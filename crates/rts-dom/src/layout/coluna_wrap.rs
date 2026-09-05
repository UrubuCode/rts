//! FLEX em COLUNA com `flex-wrap`: quebra os itens em VÁRIAS COLUNAS quando o
//! eixo PRINCIPAL (a altura, sempre DEFINIDA aqui — ver o corte no fim) não
//! os comporta — o espelho, no eixo ortogonal, do `wrap` que
//! `layout_children_horizontal` (`flex.rs`) já faz agrupando em `lines`.
//!
//! Algoritmo (Flexbox §9.3/§9.4, eixos trocados por `flex-direction:column`):
//! agrupa os itens em colunas pela BASE do eixo principal (como `flex.rs`
//! agrupa em linhas pela largura), aplica `flex-grow`/`flex-shrink` POR
//! COLUNA (reusa [`super::coluna_shrink`]), a largura de cada coluna é o
//! maior item dela, e o espaço cruzado sobrante entre colunas reparte-se por
//! `align-content` — `normal` (não declarado) estica-as, como `flex.rs` já
//! faz para linhas (achado do lote `flex-coluna-shrink`, mesma regra).
//!
//! **A ORDEM do agrupamento é sempre a do documento (após `order`), NUNCA a
//! invertida por `column-reverse`.** Só a POSIÇÃO dentro de cada coluna
//! espelha com `column-reverse` — confirmado item a item contra os quatro
//! `flexbox_flow-column{,-reverse}-wrap{,-reverse}` do WPT (as referências
//! usam floats reordenados para simular o resultado): agrupar já invertido
//! (o que `flex.rs`/`coluna.rs` fazem para o caso de UMA linha, onde não faz
//! diferença) dá `flexbox_flow-column-reverse-wrap` errado — o par
//! (two,one)/(four,three) que o WPT pede sai (four,three)/(two,one) se a
//! lista inteira for invertida antes de agrupar.
//!
//! `wrap-reverse` é mais simples: só inverte a ORDEM DAS COLUNAS no eixo
//! cruzado (cross-start↔cross-end, spec §5.3) — a primeira coluna calculada
//! vai para o fim do eixo cruzado, nunca troca item de coluna.

use super::*;
use crate::layout::coluna::{align_offset, justify_e_align, justify_offsets};

/// Um item pré-medido: `main` é a altura outer (cresce/encolhe por coluna,
/// mesmo papel de `ColItem::h` em `coluna.rs`); `cross` é a largura NATURAL
/// (shrink-to-fit — para um item com `width` explícito é essa largura, sem
/// depender de quantas colunas existem: por isso pode ser medida ANTES de se
/// saber a largura da coluna).
struct Item {
    node: NodeIdx,
    main: f32,
    cross: f32,
    is_text: bool,
    grow: f32,
    shrink: f32,
    min_main: f32,
    align_self: Option<crate::style::AlignItems>,
    /// só estica no eixo cruzado quando a LARGURA não é explícita — o mesmo
    /// "`can_stretch`" de `FlexItem` em `flex.rs`, só que no eixo trocado.
    can_stretch: bool,
    order: i32,
    /// margens `auto` no eixo PRINCIPAL (vertical) — mesma leitura de
    /// `ColItem::mt_auto`/`mb_auto` em `coluna.rs`, que faltava aqui (corte
    /// dito no cabeçalho do módulo até este lote): uma margem `auto` vence o
    /// `justify-content` da COLUNA que a contém (spec §8.1) — sem isto, um
    /// item de `margin-bottom:auto` numa coluna de `flex-wrap` recebia o
    /// mesmo offset `space-around`/`space-between` das colunas SEM margem
    /// `auto`, em vez de ficar encostado ao início com o livre absorvido no
    /// fim (`flexbox-column-row-gap-001`, WPT).
    mt_auto: bool,
    mb_auto: bool,
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn layout_children_column_wrap(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    // DEFINIDA — quem despacha (`coluna.rs`) só chama este caminho com
    // `Some`; sem uma altura definida não há critério de "a coluna encheu".
    container_content_h: f32,
    css: &ComputedStyle,
    font_size: f32,
    reverse: bool,
    wrap_reverse: bool,
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
    // Mesma regra de `coluna.rs`: o gap ENTRE ITENS (eixo principal, vertical)
    // é SÓ o row-gap; o gap ENTRE COLUNAS (eixo cruzado, horizontal) é o
    // column-gap (`css.gap`) — o espelho exato do que `flex.rs` faz (gap
    // entre itens de uma linha = `css.gap`; row_gap entre LINHAS). Sem
    // `.or(css.gap)`: `column-gap` sozinho não empurra os itens dentro de UMA
    // coluna (lote `flex-gap-2`, `flexbox-column-row-gap-004` do WPT — ver o
    // comentário mais completo em `coluna.rs`, de onde este ficheiro herdou
    // o mesmo fallback ao ser extraído).
    let main_gap = resolve_height(css.row_gap, Some(container_content_h), &resolve)
        .unwrap_or(0.0)
        .max(0.0);
    let cross_gap = css
        .gap
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let (justify, align) = justify_e_align(css, reverse);

    // ── PASSO 1: mede cada filho nos DOIS eixos (mesma base de `coluna.rs`,
    // mais a largura NATURAL para decidir a largura de cada coluna) ─────────
    let mut items: Vec<Item> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        if is_out_of_flow(dom, child) {
            continue;
        }
        if e_display_none(dom, child) {
            continue;
        }
        if matches!(dom.node(child).kind, NodeKind::Text(_)) {
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            let h = crate::inline_box::altura_da_linha(css, font_size, ctx.measurer);
            let w = ctx
                .measurer
                .text_width(&text, font_size, false, false, false);
            items.push(Item {
                node: child,
                main: h,
                cross: w,
                is_text: true,
                grow: 0.0,
                shrink: 0.0, // texto solto não encolhe, como em `coluna.rs`/`flex.rs`.
                min_main: h,
                align_self: None,
                can_stretch: false,
                order: 0,
                mt_auto: false,
                mb_auto: false,
            });
            continue;
        }
        let ccss = dom.computed_style_idx(child).unwrap_or_default();
        let estica = ccss.align_self.unwrap_or(align) == crate::style::AlignItems::Stretch;
        let natural_h = if estica {
            measure_block(dom, child, content_w, Some(container_content_h), None, None, false, ctx).1
        } else {
            child_outer_height(dom, child, content_w, Some(container_content_h), css, font_size, ctx)
        };
        let child_font = font_px(&ccss, font_size);
        let main = super::coluna_shrink::base_outer(
            &ccss,
            natural_h,
            Some(container_content_h),
            content_w,
            child_font,
            ctx,
        );
        let resolve_filho = ResolveCtx {
            parent_content_w: content_w,
            node_font_size: child_font,
            root_font_size: crate::style::root_font_size(),
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        // `coluna_shrink::min_main` (lote `flex-coluna-shrink`, retrabalho):
        // decide entre o declarado, `min-content` (§4.5, não some sob
        // overflow não-visível) e o automático — a mesma pergunta que
        // `coluna.rs` faz, agora numa função só.
        let min_main = super::coluna_shrink::min_main(dom, child, &ccss, natural_h, Some(container_content_h), &resolve_filho, ctx);
        // Largura NATURAL (shrink-to-fit): para um item de `width` explícito
        // é essa largura, qualquer que seja a coluna — layout_block honra o
        // `width` declarado antes de olhar para o `avail_w`. É o que permite
        // medir a largura de CADA item antes de se saber a largura da coluna
        // (que depende do MAIOR item dela).
        let (cross, _) = measure_block(
            dom,
            child,
            content_w,
            Some(container_content_h),
            None,
            None,
            true,
            ctx,
        );
        items.push(Item {
            node: child,
            main,
            cross,
            is_text: false,
            grow: ccss.flex_grow.unwrap_or(0.0),
            shrink: ccss.flex_shrink.unwrap_or(1.0),
            min_main,
            align_self: ccss.align_self,
            can_stretch: ccss.width.is_none(),
            order: ccss.order.unwrap_or(0),
            mt_auto: ccss.margin.top.is_auto(),
            mb_auto: ccss.margin.bottom.is_auto(),
        });
    }
    if items.is_empty() {
        return 0.0;
    }
    // `order` (empate = ordem do documento) — a MESMA ordem em que a
    // grupagem por colunas abaixo lê a lista; ver o comentário do cabeçalho
    // sobre por que `reverse` NÃO entra aqui.
    items.sort_by_key(|it| it.order);

    // ── PASSO 2: agrupa em COLUNAS pela BASE do eixo principal (o mesmo
    // empacotamento guloso de `flex.rs`, eixo trocado) ──────────────────────
    let mut columns: Vec<Vec<Item>> = vec![Vec::new()];
    let mut col_h = 0.0f32;
    for it in items {
        let cur = columns.last_mut().unwrap();
        let with_gap = if cur.is_empty() { 0.0 } else { main_gap };
        if !cur.is_empty() && col_h + with_gap + it.main > container_content_h {
            columns.push(Vec::new());
            col_h = it.main;
        } else {
            col_h += with_gap + it.main;
        }
        columns.last_mut().unwrap().push(it);
    }

    // ── PASSO 3: `flex-grow`/`flex-shrink` no eixo principal, POR COLUNA —
    // mesma iteração de `coluna_shrink::shrink`, agora por grupo em vez de
    // uma vez só (mesma relação que `flex.rs` tem entre uma linha e o
    // container inteiro sem wrap) ────────────────────────────────────────────
    for col in columns.iter_mut() {
        let n = col.len();
        let sum_main: f32 = col.iter().map(|it| it.main).sum();
        let total_gap = (n.saturating_sub(1)) as f32 * main_gap;
        let free = container_content_h - sum_main - total_gap;
        let sum_grow: f32 = col.iter().map(|it| it.grow).sum();
        if free > 0.0 && sum_grow > 0.0 {
            for it in col.iter_mut() {
                if it.grow > 0.0 {
                    it.main += it.grow / sum_grow * free;
                }
            }
        } else if free < 0.0 {
            let bases: Vec<f32> = col.iter().map(|it| it.main).collect();
            let shrinks: Vec<f32> = col.iter().map(|it| it.shrink).collect();
            let mins: Vec<f32> = col.iter().map(|it| it.min_main).collect();
            let mains = super::coluna_shrink::shrink(&bases, &shrinks, &mins, free);
            for (it, m) in col.iter_mut().zip(mains) {
                it.main = m;
            }
        }
    }

    // ── PASSO 4: largura de cada coluna (o maior item dela) e `align-content`
    // no eixo cruzado — `normal` (não declarado) estica as colunas para
    // preencher o espaço sobrante, a mesma regra que `flex.rs` já tem para
    // linhas (achado `claude-gap-row-percentual-eixo`, lote `flex-coluna-shrink`) ──
    let mut col_cross: Vec<f32> = columns
        .iter()
        .map(|c| c.iter().fold(0.0f32, |a, it| a.max(it.cross)))
        .collect();
    let mut leading_cross = 0.0f32;
    let mut between_cross = 0.0f32;
    if content_w > 0.0 {
        let usado: f32 = col_cross.iter().sum::<f32>()
            + (columns.len().saturating_sub(1)) as f32 * cross_gap;
        let free = (content_w - usado).max(0.0);
        match css.align_content {
            // `align-content` declarado não tem efeito com uma LINHA só
            // (Flexbox §8.3, "this property has no effect when the flex
            // container has only a single line") — só distribui espaço
            // ENTRE colunas, que só existe com mais de uma.
            Some(v) if columns.len() > 1 => {
                let (l, b) = justify_offsets(v, free, columns.len());
                leading_cross = l;
                between_cross = b;
            }
            // `normal` (não declarado) comporta-se como `stretch` no eixo
            // cruzado (CSS Box Alignment 3 §8.3) — mesma regra que já valia
            // para várias colunas, agora TAMBÉM com uma só. Isto não é
            // `align-content` a fazer algo com 1 linha (o `Some(v)` acima
            // continua sem efeito nesse caso): é `align-items:stretch`, cujo
            // alvo é a largura da PRÓPRIA coluna — sem isto, uma coluna
            // sozinha ficava na largura NATURAL do maior item (o
            // shrink-to-fit de um item sem `width`, por vezes só a borda) e
            // nunca crescia até `content_w` (`flexbox-flex-wrap-vert-001`,
            // WPT: um item único deveria esticar à largura do contentor).
            None => {
                let extra = free / columns.len() as f32;
                for cw in col_cross.iter_mut() {
                    *cw += extra;
                }
            }
            _ => {}
        }
    }

    // `wrap-reverse`: troca cross-start↔cross-end (spec §5.3) — a PRIMEIRA
    // coluna calculada vai para o FIM do eixo cruzado. Só a ordem das
    // colunas; os itens dentro de cada uma não mudam de posição por isto.
    //
    // `direction:rtl` troca o MESMO par cross-start↔cross-end, porque o eixo
    // cruzado de uma coluna É o eixo inline (Flexbox §4.1 + Writing Modes) —
    // é a mesma pergunta que `coluna_rtl::cross_x` já resolve para a
    // posição de um item DENTRO da sua coluna, agora para a ORDEM das
    // colunas em si (achado `claude-flex-column-wrap-rtl-order`, WPT
    // `flexbox_rtl-order`: a referência hand-authored — floats reordenados —
    // só bate com "three,four" à esquerda/"one,two" à direita quando RTL
    // TAMBÉM inverte cross-start, cancelando o `wrap-reverse` do mesmo teste).
    // As duas trocas são a mesma pergunta feita duas vezes: RTL sozinho
    // inverte, `wrap-reverse` sozinho inverte, os dois juntos cancelam — daí o
    // XOR. Só quando `writing-mode` é horizontal (o único que este motor
    // faz) — a mesma guarda de `coluna_rtl::cross_x`/`rtl_bloco.rs`.
    let rtl_horizontal = matches!(css.direction, Some(crate::style::Direction::Rtl))
        && css.writing_mode.unwrap_or_default().is_horizontal();
    let mut columns = columns;
    if wrap_reverse ^ rtl_horizontal {
        columns.reverse();
        col_cross.reverse();
    }

    // ── PASSO 5: posiciona e pinta, coluna a coluna ─────────────────────────
    let mut x = content_x + leading_cross;
    let mut max_bottom = content_y;
    for (col, cw) in columns.into_iter().zip(col_cross.into_iter()) {
        let n = col.len();
        let sum_main: f32 = col.iter().map(|it| it.main).sum();
        let total_gap = (n.saturating_sub(1)) as f32 * main_gap;
        let free = container_content_h - sum_main - total_gap;
        // Margem `auto` no eixo principal (Flexbox §8.1) vence o
        // `justify-content` DESTA coluna — mesma regra de `coluna.rs`: com
        // pelo menos um `auto` a lista fica encostada ao início (leading=0,
        // between=0) e cada `auto` absorve a sua fatia do livre POSITIVO.
        let auto_count: usize = col
            .iter()
            .map(|it| it.mt_auto as usize + it.mb_auto as usize)
            .sum();
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
        // `column-reverse`: só a ORDEM DE POSIÇÃO dentro desta coluna espelha
        // (ver o comentário do cabeçalho — o agrupamento em colunas já
        // aconteceu na ordem normal, acima).
        let posicoes: Vec<&Item> = if reverse {
            col.iter().rev().collect()
        } else {
            col.iter().collect()
        };
        let mut y = content_y + leading;
        for (j, it) in posicoes.iter().enumerate() {
            if j > 0 {
                y += main_gap + between;
            }
            if it.mt_auto {
                y += auto_size;
            }
            if it.is_text {
                let text = collect_text(dom, it.node);
                list.items.push(DisplayItem::Text {
                    x,
                    y,
                    text: text.into(),
                    color: cor_visivel(css, css.color.unwrap_or(0x000000FF)),
                    size: font_size,
                    mono: false,
                    ahem: css.font_family.as_deref().is_some_and(crate::style::is_ahem_family),
                    bold: css.bold.unwrap_or(false),
                    italic: italico(Some(css), tag_de(dom, it.node), false),
                    letter_spacing: css.letter_spacing.unwrap_or(0.0),
                    decoration: decoration_code(css),
                });
            } else {
                let item_align = it.align_self.unwrap_or(align);
                let stretches =
                    item_align == crate::style::AlignItems::Stretch && it.can_stretch;
                // `direction:rtl` no eixo cruzado (lote `flex-justify-logico`,
                // `coluna_rtl::cross_x`): a mesma pergunta do caminho de UMA
                // coluna, só que aqui o "content-box" a espelhar dentro É a
                // COLUNA (`[x, x+cw]`), não o contentor inteiro — um item
                // alinha-se dentro da SUA coluna, e é essa caixa que o RTL
                // inverte. A ORDEM DAS COLUNAS em si TAMBÉM inverte com RTL
                // desde o lote `flex-reverse-order` (o XOR com `wrap_reverse`
                // acima) — as duas perguntas eram tratadas como distintas até
                // `flexbox_rtl-order` (WPT) mostrar que não são.
                let child_x_ltr = if stretches {
                    x
                } else {
                    x + align_offset(item_align, cw, it.cross)
                };
                let child_x = super::coluna_rtl::cross_x(
                    css.direction,
                    css.writing_mode,
                    x,
                    cw,
                    child_x_ltr,
                    if stretches { cw } else { it.cross },
                );
                // `cw` (a largura da COLUNA), nunca `content_w` (a largura do
                // CONTENTOR inteiro): um item que não estica ainda vive dentro
                // da sua coluna, e é contra essa caixa que `align_offset`
                // (acima) já resolveu `child_x_ltr`. Passar `content_w` aqui
                // dava ao `layout_block` de dentro um `avail_w` bem maior do
                // que o que decidiu o `child_x` — e um item com
                // `margin-left:auto` tem a sua própria resolução de margem
                // AUTO ali dentro (a centragem genérica de bloco), que
                // reabria a conta com esse `avail_w` maior e empurrava o item
                // para fora da coluna (achado: `margin-left:auto` sozinho, ou
                // com `margin-bottom:auto`, num item de `flex-flow:column
                // wrap` — `flexbox-column-row-gap-001`, WPT — punha-o numa
                // 3ª coluna a ~170px à direita da 2ª, e as duas colunas
                // seguintes esticavam para preencher o espaço que sobrava).
                let avail_w = cw;
                layout_block_reusing(
                    dom,
                    it.node,
                    child_x,
                    y,
                    avail_w,
                    Some(it.main),
                    || (0.0, 0.0),
                    None,
                    Some(it.main),
                    true, // hard: o main size de coluna sempre vence o height próprio.
                    !stretches,
                    // Item de flex-column: mesma razão do caminho de UMA
                    // coluna em `coluna.rs` — floats não atravessam um
                    // container flex.
                    &BlockFormattingContext::new(),
                    ctx,
                    list,
                );
            }
            y += it.main;
            if it.mb_auto {
                y += auto_size;
            }
        }
        max_bottom = max_bottom.max(y);
        x += cw + cross_gap + between_cross;
    }
    (max_bottom - content_y).max(0.0)
}

// CORTES documentados (nenhuma fixture medida ou WPT deste lote precisa):
//
// - **Largura shrink-to-fit do CONTENTOR**: `content_w` chega já resolvido
//   (o chamador, `bloco.rs`, resolve a largura do container ANTES de
//   layoutar os filhos) — quando o autor não declara `width` num
//   `flex-column wrap` FORA do fluxo normal (`float`/`inline-flex` sem
//   largura), o browser soma as larguras das colunas; este motor usa a
//   largura que `bloco.rs` já deu (fill-available do bloco, ou o
//   shrink-to-fit de uma coluna só, calculado sem saber que vai haver
//   wrap). Precisa de uma segunda passada (medir o nº de colunas ANTES da
//   largura) — é a circularidade que o WPT `multiline-shrink-to-fit`
//   exercita (casos 2 e 4) e que o `claude-flex-column-shrink.html`
//   (balde "shrink") já tinha marcado como "A MEDIR", não confirmado no
//   Blink.
// - **Margem `auto` no eixo principal** (`margin-top`/`margin-bottom: auto`
//   dentro de uma coluna) não é lida aqui — nenhuma fixture ou WPT deste
//   lote combina os dois; `coluna.rs` (uma coluna) continua a lê-la.
// - **`::before`/`::after` como item flex** (`flex_pseudo.rs`) não entra no
//   caminho de coluna nenhum, com ou sem wrap — gap pré-existente do
//   `coluna.rs` de uma coluna, não deste lote.
// - **RESOLVIDO no lote `flex-reverse-order`**: `direction:rtl` inverte a
//   ORDEM DAS COLUNAS no eixo cruzado, do mesmo jeito que `wrap-reverse`
//   (PASSO 4, XOR das duas) — não só o item DENTRO da coluna
//   (`coluna_rtl::cross_x`, PASSO 5, que continua a fazer a sua parte). Achado
//   pelo WPT `flexbox_rtl-order` (referência hand-authored, floats
//   reordenados): sem o XOR, `direction:rtl` sozinho não tinha efeito nenhum
//   na ordem física das colunas.
