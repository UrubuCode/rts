//! FLEX em COLUNA: empilhar no eixo vertical, com `justify-content` e
//! `align-items` trocados de eixo.
//!
//! Movido de `layout.rs` na modularização (nessa altura, byte a byte do
//! original). Desde então ganhou o parâmetro `wrap` (lote `flex-column-wrap`,
//! 2026-09-04): com `flex-wrap` e altura definida, delega para
//! `coluna_wrap.rs` — ver o comentário no parâmetro.

use super::*;

/// `justify-content` (`fisico_para_coluna` — físicos/lógicos resolvidos e
/// invariantes a `column-reverse`, ver o comentário lá — e depois espelhado
/// se `reverse`, para os valores que NÃO são físicos) e `align-items`
/// (`Stretch` por default) de um container de coluna. Extraído para que
/// `coluna_wrap.rs` (multi-coluna) leia exatamente a mesma regra em vez de
/// duplicá-la — as duas funções decidem o MESMO container, só a distribuição
/// dos itens muda. Lote `flex-justify-logico`: antes, `left`/`right`
/// colapsavam incondicionalmente em `FlexStart` aqui e o espelho seguinte
/// invertia-os para o FUNDO em `column-reverse` — o mesmo bug que a versão
/// inline em `layout_children_column` corrigia sem passar por aqui; ver o
/// comentário de `fisico_para_coluna` para o porquê de não duplicar duas
/// vezes o mesmo espelho.
pub(in crate::layout) fn justify_e_align(
    css: &ComputedStyle,
    reverse: bool,
) -> (crate::style::JustifyContent, crate::style::AlignItems) {
    let justify_declarado = css
        .justify
        .unwrap_or(crate::style::JustifyContent::FlexStart);
    let justify_declarado = fisico_para_coluna(justify_declarado, reverse);
    let justify = if reverse {
        mirror_justify(justify_declarado)
    } else {
        justify_declarado
    };
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);
    (justify, align)
}

#[allow(clippy::too_many_arguments)]
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
    // `flex-direction: column-reverse` — ver a nota gémea em `flex.rs`.
    reverse: bool,
    // `flex-wrap: wrap`/`wrap-reverse` — só tem efeito com altura DEFINIDA
    // (senão não há critério de "a coluna encheu"). Delega para
    // `coluna_wrap.rs` (o algoritmo de MÚLTIPLAS colunas não cabe aqui sem
    // estourar o teto de 500 linhas).
    wrap: bool,
    // O limiar de QUEBRA do wrap — `height`/`max-height`, NUNCA `min-height`
    // (só um PISO). Mais ESTREITO do que `container_content_h` de propósito
    // — ver `bloco.rs::wrap_definite_h`: um `min-height:0` conta como altura
    // definida para stretch, mas não pode ser o limiar de wrap (abriria uma
    // coluna nova por item).
    wrap_definite_h: Option<f32>,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    if wrap {
        if let Some(h) = wrap_definite_h {
            // `wrap-reverse` × o eixo X físico (`eixos_flex`): um `row`
            // vertical desce aqui já invertido em `vertical-rl`, sem
            // `wrap-reverse` declarado. `direction` forçado a LTR aqui — não
            // porque não conte, mas porque `coluna_wrap::rtl_horizontal` já
            // faz esse XOR (lote `flex-reverse-order`); passar o `direction`
            // real duplicava-o (RTL cancelava consigo mesmo, medido nos
            // testes `flex_reverse_order_corpus`). Em `writing-mode` vertical
            // `eixo_x_forward` já ignora `direction` (o eixo de bloco não o
            // lê), então forçar LTR aqui não perde nada nesse caso.
            let wrap_reverse = super::eixos_flex::wrap_reverse_efetivo(
                css.writing_mode.unwrap_or_default(), crate::style::Direction::Ltr, true, css.flex_wrap,
            );
            return super::coluna_wrap::layout_children_column_wrap(
                dom, id, content_x, content_y, content_w, h, css, font_size, reverse, wrap_reverse, ctx, list,
            );
        }
    }
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // Em column, o espaço entre itens no eixo principal é SÓ o ROW-gap: uma
    // coluna de uma linha só não tem "colunas" para o `column-gap` espaçar
    // (Box Alignment §8; WPT `flexbox-column-row-gap-004`). Havia um
    // `.or(css.gap)` aqui — herdado do `layout.rs` pré-modularização, sem
    // corpus nem teste a exercitá-lo — que lia `column-gap` sozinho como
    // substituto de um `row-gap` em falta; o shorthand `gap: X` não precisa
    // dele, porque já escreve OS DOIS campos no parse.
    //
    // `resolve_height`, não `Dimension::resolve`: `row-gap` é sempre o eixo de
    // BLOCO (aqui, o próprio eixo principal), então uma percentagem resolve
    // contra a ALTURA do container — nunca a largura — e vira `normal` (0)
    // quando essa altura é indefinida (CSS Align 3 §column-row-gap,
    // github.com/w3c/csswg-drafts/issues/5081). Lote `flex-coluna-shrink`:
    // media 10%×64(largura)=6,4 onde a conta é 10%×200(altura)=20.
    let main_gap = resolve_height(css.row_gap, container_content_h, &resolve)
        .unwrap_or(0.0)
        .max(0.0);
    // `column-reverse`: mesmo espelho de `flex.rs` — o main-start visual é o
    // FUNDO do container, não o topo; ver o comentário lá.
    let (justify, align) = justify_e_align(css, reverse);

    // ── PASSO 1: mede a BASE outer de cada filho (flex-basis/height/conteúdo)
    // no eixo principal, + margens auto e os fatores de flex-shrink/grow ──────
    struct ColItem {
        node: NodeIdx,
        /// tamanho BASE outer no eixo principal — antes de grow/shrink; após o
        /// PASSO 2 é o MAIN final (mesmo campo, mesmo papel que `FlexItem::h`
        /// tinha antes deste lote: cresce OU encolhe nele, nunca os dois).
        h: f32,
        is_text: bool,
        mt_auto: bool,
        mb_auto: bool,
        grow: f32,
        /// `flex-shrink` (1 = default do CSS; texto solto não encolhe, como em
        /// `flex.rs`).
        shrink: f32,
        /// piso de `min-height` no eixo principal — `coluna_shrink::min_main`
        /// decide entre o automático (§4.5), `min-content` e o declarado.
        min_main: f32,
        order: i32,
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
                shrink: 0.0, // texto solto não encolhe (mesmo trato do eixo horizontal).
                min_main: crate::inline_box::altura_da_linha(css, font_size, ctx.measurer),
                order: 0,
            });
            continue;
        }
        let ccss = dom.computed_style_idx(child).unwrap_or_default();
        // Um item que ESTICA (align stretch, o default) ocupa a largura do
        // contentor, e a altura tem de ser medida a essa largura — medi-la em
        // shrink-to-fit punha dois floats lado a lado um DEBAIXO do outro (100px
        // de largura em vez de 1280) e o item saía com 70px onde o Blink dá 40
        // (`claude-flex-item-contem-floats`). Só quem não estica mede encolhido.
        let estica = ccss.align_self.unwrap_or(align) == crate::style::AlignItems::Stretch;
        let natural_h = if estica {
            measure_block(dom, child, content_w, container_content_h, None, None, false, ctx).1
        } else {
            child_outer_height(dom, child, content_w, container_content_h, css, font_size, ctx)
        };
        let child_font = font_px(&ccss, font_size);
        let resolve_filho = ResolveCtx {
            parent_content_w: content_w,
            node_font_size: child_font,
            root_font_size: crate::style::root_font_size(),
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        let min_main = super::coluna_shrink::min_main(dom, child, &ccss, natural_h, container_content_h, &resolve_filho, ctx);
        let h = super::coluna_shrink::base_outer(
            &ccss,
            natural_h,
            container_content_h,
            content_w,
            child_font,
            ctx,
        );
        let mt_auto = ccss.margin.top.is_auto();
        let mb_auto = ccss.margin.bottom.is_auto();
        let grow = ccss.flex_grow.unwrap_or(0.0);
        let shrink = ccss.flex_shrink.unwrap_or(1.0); // 1 é o default do CSS.
        let order = ccss.order.unwrap_or(0);
        items.push(ColItem {
            node: child,
            h,
            is_text: false,
            mt_auto,
            mb_auto,
            grow,
            shrink,
            min_main,
            order,
        });
    }
    if items.is_empty() {
        return 0.0;
    }
    // `order` (empate = ordem do documento, sort estável), depois
    // `column-reverse` — mesma dupla operação do eixo horizontal (`flex.rs`).
    items.sort_by_key(|it| it.order);
    if reverse {
        items.reverse();
    }

    // ── PASSO 2: distribui o espaço livre do eixo principal (Y) ──────────────────
    let n = items.len();
    let sum_h: f32 = items.iter().map(|it| it.h).sum();
    let total_gap = (n.saturating_sub(1)) as f32 * main_gap;
    // `wrap_definite_h`, não `container_content_h`: o mesmo limiar do
    // DESPACHO do wrap (nunca `min-height` — um PISO não é um TECTO, e
    // tratá-lo como tal encolhia itens que já o excediam em vez de deixar
    // o contentor crescer acima dele — `flexbox-flex-wrap-vert-002`, WPT).
    let free = wrap_definite_h
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
    } else if free < 0.0 {
        // FLEX-SHRINK no eixo principal (css-flexbox §9.7): quando FALTA
        // espaço (a soma das bases + gaps excede o disponível), o défice
        // reparte-se por `shrink × base`, com piso em `min_main` — o mesmo
        // algoritmo iterativo de congelamento de `flex.rs:319-370`, aqui no
        // eixo vertical (`coluna_shrink::shrink`). Sem isto, um item de
        // coluna maior do que o espaço principal disponível transbordava
        // sempre (achado 2026-09-04, `claude-flex-column-shrink`/
        // `claude-flex-coluna-shrink-overflow`/
        // `claude-flex-basis-percent-shrink-column`).
        let bases: Vec<f32> = items.iter().map(|it| it.h).collect();
        let shrinks: Vec<f32> = items.iter().map(|it| it.shrink).collect();
        let mins: Vec<f32> = items.iter().map(|it| it.min_main).collect();
        let mains = super::coluna_shrink::shrink(&bases, &shrinks, &mins, free);
        for (it, m) in items.iter_mut().zip(mains) {
            it.h = m;
        }
    }
    // Piso pós-distribuição p/ `flex-basis:0` sem grow/shrink (011, WPT).
    for it in &mut items {
        it.h = it.h.max(it.min_main);
    }
    let sum_h: f32 = items.iter().map(|it| it.h).sum();
    // Mesma troca de cima: espaço livre A SÉRIO, não o que sobra até um PISO já ultrapassado.
    let free = wrap_definite_h
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
            // (layout normal de bloco) SE a largura for `auto` — uma largura
            // DECLARADA vence o stretch (spec §8.3, mesmo corte do grid em
            // `grid.rs:355`) e cai no mesmo cross-start físico do flex-start;
            // start/center/end → shrink-to-fit + offset. `direction:rtl`
            // espelha esse físico: numa coluna o eixo cruzado É o eixo
            // inline (Flexbox §4.1 + Writing Modes), então o cross-start
            // passa a ser a borda DIREITA — achado em
            // `claude-flex-column-rtl-cross-start` (WPT `flexbox_rtl-direction`).
            let stretch = align == crate::style::AlignItems::Stretch;
            let ccss = dom.computed_style_idx(it.node).unwrap_or_default();
            // Um `<img>` não enche `avail_w` sozinho — precisa do stretch
            // como `forced_outer_w` explícito (`flex-svg-no-intrinsic-
            // column-001`, WPT); o mesmo vale para `<input type=checkbox|
            // radio>` (o quadrado de 13px de `layout/input.rs`, WPT
            // `stretch-flex-item-checkbox-input`/`-radio-input`) —
            // `flex_stretch_replaced` decide os dois; um campo de texto ou
            // `<table>` já se enchem sozinhos e ficam de fora.
            let precisa_forced_w = super::flex_stretch_replaced::precisa_de_forced_w_no_stretch(dom, it.node);
            let forced_w = (stretch && ccss.width.is_none() && precisa_forced_w).then_some(content_w);
            let child_x = if stretch && ccss.width.is_none() {
                super::coluna_rtl::cross_x(
                    css.direction,
                    css.writing_mode,
                    content_x,
                    content_w,
                    content_x,
                    content_w,
                )
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
                let iw = content_w - free_x; // = min(w, content_w) — só para o `align_offset` LTR.
                let x_ltr = content_x + align_offset(align, content_w, iw);
                // RETRABALHO: o espelho usa `w` (a largura VERDADEIRA, não
                // grampeada) — um item mais largo do que `content_w` dava
                // `iw == content_w` e o espelho devolvia `content_x` (sem
                // transbordo nenhum) onde o Chrome transborda pela esquerda
                // (`claude-rtl-filho-transborda`).
                super::coluna_rtl::cross_x(css.direction, css.writing_mode, content_x, content_w, x_ltr, w)
            };
            // O `main` do PASSO 2 (grow, shrink, ou inalterado) é IMPOSTO ao
            // item como altura outer — DURA (`hard`): vence `height`/
            // `flex-basis` do próprio nó, ao contrário do `forced_outer_h`
            // "mole" do stretch (que só cresce, nunca corta um item mais alto
            // que a linha — a peça que faltava para o encolhimento; espelha
            // `flex.rs:478`, onde `Some(it.main)` já é incondicional no eixo
            // horizontal). A mesma altura vira o containing block (`avail`)
            // dos NETOS com `height:100%` — é o item resolvido, cresceu ou
            // encolheu, que eles têm de ver.
            let avail = Some(it.h);
            let forced_h = Some(it.h);
            // `layout_block_reusing`: mesmo raciocínio do flex-row em
            // `flex.rs` — o container refaz a distribuição toda vez, o item
            // individual bate no cache quando o epoch e a imposição (`avail`/
            // `forced_h`) não mudaram.
            layout_block_reusing(
                dom,
                it.node,
                child_x,
                y,
                content_w,
                avail,
                || (0.0, 0.0),
                forced_w,
                forced_h,
                true, // hard: o main size de coluna sempre vence o height próprio.
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

/// A MESMA ideia de [`super::eixos_flex::fisico_para_eixo`], mas para o eixo PRINCIPAL de uma
/// COLUNA — e o mapa é diferente porque o eixo aí não é o mesmo: `left`/
/// `right` não têm eixo NENHUM numa coluna (Box Alignment §5.1) e os DOIS
/// colapsam em "início" (topo); `start`/`end` continuam assimétricos, porque
/// o eixo de bloco existe (`start`=topo, `end`=fundo, como numa LINHA). Os
/// quatro ficam invariantes a `column-reverse` — só a ORDEM dos itens
/// inverte, nunca o lado do empacotamento.
fn fisico_para_coluna(j: crate::style::JustifyContent, reverse: bool) -> crate::style::JustifyContent {
    use crate::style::JustifyContent as J;
    match j {
        J::Left | J::Right | J::Start => {
            if reverse { J::FlexEnd } else { J::FlexStart }
        }
        J::End => {
            if reverse { J::FlexStart } else { J::FlexEnd }
        }
        j => j,
    }
}

pub(in crate::layout) fn mirror_justify(j: crate::style::JustifyContent) -> crate::style::JustifyContent {
    use crate::style::JustifyContent as J;
    match j {
        J::FlexStart => J::FlexEnd,
        J::FlexEnd => J::FlexStart,
        other => other,
    }
}

/// `Start`/`End` entram aqui só para `align-content` (multi-linha), que chama
/// isto DIRETO com o valor cru — `justify-content` já os resolveu para
/// `FlexStart`/`FlexEnd` em `fisico_para_eixo`/`fisico_para_coluna` antes de
/// chegar aqui. Tratados como `FlexStart`/`FlexEnd` (mesma posição física
/// que já tinham); `wrap-reverse` não os espelha aqui — nenhum valor é
/// espelhado no `align-content` hoje, o que fica fora deste lote.
pub(in crate::layout) fn justify_offsets(j: crate::style::JustifyContent, free: f32, n: usize) -> (f32, f32) {
    use crate::style::JustifyContent as J;
    if free <= 0.0 {
        return match j {
            J::Center => (free / 2.0, 0.0), // leading negativo = transbordo centrado
            J::FlexEnd | J::End => (free, 0.0), // todo o overflow no start
            // flex-start E os space-* → flush no start (fiel ao Chrome em overflow).
            J::FlexStart | J::SpaceBetween | J::SpaceAround | J::SpaceEvenly | J::Left | J::Start => (0.0, 0.0),
            J::Right => (free, 0.0),
        };
    }
    match j {
        J::FlexStart | J::Left | J::Start => (0.0, 0.0),
        J::FlexEnd | J::Right | J::End => (free, 0.0),
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
///
/// `Baseline` cai em `FlexStart`: o alinhamento por baseline REAL (grupo por
/// linha, ascent por item) só está feito no eixo de LINHA
/// (`layout/flex_baseline.rs`, que resolve o offset ANTES de chegar aqui —
/// esta função só vê o `Baseline` de uma coluna, ou de um item cujo grupo não
/// tinha ninguém para partilhar a baseline). É o fallback que a própria spec
/// prevê (Flexbox §8.5) quando o eixo cruzado não tem baseline partilhável.
/// `LastBaseline` cai em `FlexEnd` (a margem INFERIOR) pelo mesmo motivo que
/// `Baseline` cai em `FlexStart` — ver o corte no doc da variante.
pub(in crate::layout) fn align_offset(a: crate::style::AlignItems, line_h: f32, item_h: f32) -> f32 {
    use crate::style::AlignItems as A;
    let free = line_h - item_h;
    match a {
        A::Stretch | A::FlexStart | A::Baseline => 0.0,
        A::FlexEnd | A::LastBaseline => free,
        A::Center => free / 2.0,
    }
}
