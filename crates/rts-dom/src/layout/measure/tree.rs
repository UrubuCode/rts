//! A parte de `measure.rs` que percorre a ÁRVORE DE CAIXAS — extraída para
//! manter aquele ficheiro abaixo do tecto de 500 linhas depois de este lote
//! corrigir 147cb3e53/02bc7088d (que introduziram a travessia, mas com um
//! `continue` a saltar toda caixa sem nó).
//!
//! **O achado que deu forma a este ficheiro.** Corrigir só o `continue` (uma
//! caixa ANÓNIMA passou a contar) não bastava: com esse fix sozinho,
//! `<span>aaaa<div>b</div>cccc</span>` media 58.88 (a soma de "aaaa"+"cccc")
//! em vez de 29.44 (o maior das duas, cada uma na sua linha). A causa raiz é
//! mais funda — [`intrinsic_content_width`](super::intrinsic_content_width)
//! dobrava (`flat_map`) os filhos de TODAS as caixas de um nó com mais de
//! uma, ao ser chamada recursivamente para o NÓ do fragmento (o `<span>`,
//! que gerou duas caixas — uma por lado do `<div>` que o partiu). "aaaa" e
//! "cccc" ficavam na MESMA lista de filhos, como se estivessem na mesma
//! linha, quando CSS 2.1 §9.2.1.1 as põe em duas caixas de bloco ANÓNIMAS
//! diferentes — duas linhas.
//!
//! A resposta é a mesma que `table::widths::min_content` já dava para o seu
//! próprio `caixas => …fold(0.0, f32::max)`: quando o CHAMADOR só conhece o
//! `NodeIdx`, dobra-se pelo MÁXIMO sobre as caixas do nó (cada fragmento é a
//! sua própria linha); quando o chamador já tem uma caixa CONCRETA em mãos —
//! percorrendo a árvore, como o `caixa_filho` de um `for` — usa-se
//! exatamente essa, nunca o `NodeIdx` sozinho. As funções `_de`/`_sem_cache`
//! aqui são esse segundo caminho.

use super::*;

/// O corpo de [`intrinsic_content_width`] sem a cache — chamado também por
/// [`intrinsic_outer_width_of`] quando o chamador já tem uma caixa
/// ESPECÍFICA de `id` em mãos (um fragmento de um nó partido, CSS 2.1
/// §9.2.1.1) e por isso não pode passar pela cache de `id`, que não
/// distingue fragmentos.
///
/// `caixa`:
/// - `Some(c)` — usa exatamente `c` no ramo geral, sem dobrar sobre as
///   outras caixas de `id`. É o caso do chamador que já sabe qual fragmento
///   quer.
/// - `None` — dobra sobre TODAS as caixas de `id` pelo MÁXIMO (ver o
///   comentário em `intrinsic_content_width`). É o caso do chamador externo,
///   que só conhece o nó.
pub(in crate::layout) fn intrinsic_content_width_no_cache(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    id: NodeIdx,
    box_id: Option<crate::boxes::BoxId>,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    // Um elemento REPLACED não tem filhos nem texto, e por isso caía na resposta
    // das folhas vazias: zero. Zero é o que fazia a `<figure>` (`display:table`)
    // encolher a 10px em volta de uma imagem de 252px e a legenda quebrar a um
    // carácter por linha. A largura de um replaced é a que ele declara, e a
    // pergunta faz-se com largura disponível INFINITA porque isto é o
    // max-content: o clamp pela linha é do chamador, e aplicá-lo aqui devolvia o
    // que coubesse em vez do que se quer.
    if let Some(css) = dom.computed_style_idx(id) {
        if let Some((w, _)) =
            crate::inline_box::replaced_inline_size(dom, id, &css, f32::INFINITY, None, (None, None), ctx)
        {
            // `replaced_inline_size` devolve a caixa COM borda (o contrato
            // dela é border-box); esta função devolve CONTEÚDO, como o resto
            // dos ramos abaixo — todo chamador soma o frame (que já tem essa
            // borda) por cima, e sem subtrair aqui ela contava-se DUAS vezes
            // (um `<img>` sem width e com borda inflava a estimativa de
            // shrink-to-fit de quem o continha — `child_outer_width`,
            // `intrinsic_outer_width` — em 2×borda).
            let [_, br, _, bl] = crate::style::borders::used_widths(&css);
            let w = (w - bl - br).max(0.0);
            return w;
        }
    }
    if let Some(w) = dom.computed_style_idx(id).and_then(|css| crate::layout::replaced::input::control_natural_size(dom, id, &css, ctx)).map(|(w, _)| w) { return w; }
    // An element whose children are all text: its lines, run by run
    // (`text_measure`). Each text node answers to its own `white-space`,
    // `tab-size`, weight, family and spacings — this element's, inherited —
    // and the HTML indentation between children collapses as in the flow (CSS
    // Text §4.1): fourteen spaces and newlines gave Bootstrap cover's fixed
    // button 103px too many (`claude-intrinseco-whitespace`). It answers before
    // the table branch below because a text-only element never had cells.
    let only_text = !dom.node(id).children.is_empty()
        && dom.node(id).children.iter().all(|&c| matches!(dom.node(c).kind, NodeKind::Text(_)));
    if only_text {
        let mut lines = super::text::Lines::new(false);
        for &c in &dom.node(id).children {
            lines.text(dom, c, font, ctx);
        }
        return lines.finish(ctx);
    }

    // TABELA: a largura que o conteúdo quer é a SOMA das colunas, e nenhuma das
    // duas regras abaixo a dá — o MAX (bloco) devolveria a linha mais larga e a
    // SOMA (flex) somaria linhas inteiras. Quem sabe é o algoritmo de colunas.
    if used_display(dom, id).is_some_and(crate::style::DisplayKind::is_table_box) {
        return crate::table::max_content_width(dom, id, font, ctx);
    }

    match box_id {
        // O chamador já tem a caixa em mãos (um fragmento específico, ou o
        // caso ubíquo de um nó com uma caixa só, resolvido no `match` de
        // baixo antes de chegar aqui recursivamente): usa exatamente essa.
        Some(c) => intrinsic_content_width_general(dom, tree, id, c, font, ctx),
        // Sem caixa em mãos: dobra sobre as de `id`. This is the ONE place the
        // intrinsic-width family turns a node into its boxes; everything below
        // it takes the exact box (BT-2a).
        None => match tree.boxes_of(id) {
            // O caso ubíquo: um nó normal, uma caixa só — comportamento
            // idêntico ao de antes deste lote (era exatamente este o único
            // caminho que existia).
            [c] => intrinsic_content_width_general(dom, tree, id, *c, font, ctx),
            // Sem caixa nenhuma (a cascata recusou o elemento, ou o split
            // absorveu um inline que só envolvia um bloco): sem filhos para
            // medir. The generated boxes' widths of a row flex used to be
            // asked of the DOM here too; a node with no box has no generated
            // box either, so that was zero, and is not asked.
            [] => 0.0,
            // Um NÓ SPLIT (a própria inline que o CSS 2.1 §9.2.1.1 partiu em
            // vários fragmentos). Cada fragmento vive na sua PRÓPRIA linha
            // (§9.2.1.1: os lados ficam em caixas de bloco anónimas, siblings
            // umas das outras) — por isso a resposta é o MÁXIMO entre elas,
            // na mesma regra "maior das linhas" que já vale dentro de um
            // único fragmento, e NUNCA a soma: um `.fold(0.0, f32::max)`
            // sobre chamadas independentes, cada uma vendo só os filhos
            // DAQUELE fragmento (`tree.children(caixa)`), nunca os dos
            // outros. Sem isto — juntar os filhos de TODOS os fragmentos numa
            // lista só, como a flatmap que existia até este fix fazia —
            // "aaaa" e "cccc" de `<span>aaaa<div>b</div>cccc</span>` eram
            // medidos como se partilhassem uma linha (58.88 = soma de
            // 29.44+29.44), quando na verdade estão em DUAS linhas separadas
            // pelo `<div>` do meio (29.44, o maior das duas).
            caixas => caixas
                .iter()
                .map(|&c| intrinsic_content_width_general(dom, tree, id, c, font, ctx))
                .fold(0.0, f32::max),
        },
    }
}

/// O corpo "geral" de [`intrinsic_content_width`]: soma (flex-row) ou maior
/// das linhas (bloco), sobre os filhos de UMA caixa específica — nunca sobre
/// `id` sozinho, que é ambíguo quando `id` tem mais de uma caixa (ver o
/// comentário no chamador).
fn intrinsic_content_width_general(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    id: NodeIdx,
    box_id: crate::boxes::BoxId,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    // o EIXO em que os filhos se dispõem decide SOMA vs MAX: um flex em COLUNA
    // empilha como um bloco (o maior filho), mesmo com `flex-wrap` — a
    // multi-coluna do wrap é corte dito (`claude-flex-column-shrink-to-fit`).
    let display = css_display(dom, id);
    let in_column = dom.computed_style_idx(id).and_then(|c| c.flex_direction).map(|f| f.is_column()).unwrap_or(false);
    let is_row = (display == crate::block::DISPLAY_HORIZONTAL || display == crate::block::DISPLAY_WRAP) && !in_column;
    let gap = if is_row {
        let resolve = ResolveCtx {
            parent_content_w: ctx.viewport_w,
            node_font_size: font,
            root_font_size: crate::style::root_font_size(),
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        dom.computed_style_idx(id)
            .and_then(|c| c.gap)
            .and_then(|d| d.resolve(&resolve))
            .unwrap_or(0.0)
            .max(0.0)
    } else {
        0.0
    };

    if !is_row {
        let mut lines = super::text::Lines::new(false);
        walk_children(&mut lines, dom, tree, box_id, font, ctx);
        return lines.finish(ctx);
    }
    let mut sum = 0.0f32;
    let mut count: usize = 0;
    let children: &[crate::boxes::BoxId] = tree.children_without_generated(box_id);
    for &child_box in children {
        // `is_row` never meets an anonymous box: only a FLOW container splits
        // (`boxes::build`), never a flex.
        let Some(child) = tree.node_of(child_box) else { continue };
        // fora do fluxo não contribui para a largura intrínseca do container.
        if is_out_of_flow(dom, child) {
            continue;
        }
        // Numa linha de FLEX, um nó de texto só-espaços não é item nenhum — o
        // pré-passo de `layout_children_horizontal` descarta-o (`trim().is_empty()`)
        // e aqui ele contava DUAS vezes: a largura do `"\n\t\t"` e mais um `gap`
        // por ser um item a mais. Eram 155 px no `.vector-header-start` da
        // Wikipédia.
        if matches!(&dom.node(child).kind, NodeKind::Text(t) if t.trim().is_empty()) {
            continue;
        }
        let w = intrinsic_outer_width_of(dom, tree, child, Some(child_box), font, ctx);
        if w > 0.0 {
            count += 1;
        }
        sum += w;
    }
    // `::before`/`::after` de um flex em linha são itens (Flexbox §4) e entram
    // na largura natural do contentor — o caret do botão do Bootstrap.
    for pe in [crate::style::PseudoElement::Before, crate::style::PseudoElement::After] {
        let w = crate::layout::flex::pseudo::width(dom, tree, box_id, pe, font, ctx);
        if w > 0.0 {
            sum += w;
            count += 1;
        }
    }
    // soma + gaps entre os itens. A grid lands here too (its axis code is
    // `wrap`), and a grid item sized by its ratio against a fixed row is as
    // wide as the row makes it — which its empty content cannot say.
    let ratio_floor = crate::layout::grid::aspect::intrinsic_floor(dom, tree, id, box_id, font, ctx);
    (sum + (count.saturating_sub(1)) as f32 * gap).max(ratio_floor.unwrap_or(0.0))
}

/// The lines of one box's in-flow children, fed into `lines`.
///
/// Outside flex the max-content is not the widest child but the widest LINE,
/// and consecutive inline children share one. Measured in Chrome with
/// `width:max-content`:
///
///   two inline-blocks of 50 ............ 100   (summed)
///   two BLOCKs of 50 .................... 50   (a line each)
///   inline 50 + BLOCK 50 + inline 50 .... 50   (three runs, widest 50)
///   two inlines with a <br> between ..... 50   (the <br> ends the run)
///   text "xy" + inline-block 50 ......... 66   (the text joins the run)
///
/// A non-replaced `display:inline` child is walked INTO rather than asked for
/// one width: a forced break inside it ends the parent's line, and its text's
/// collapsible spaces collapse with its neighbours' (`text_measure`). Asked as
/// one opaque width, `a <span style="white-space:pre">B&#10;C</span> d`
/// measured the single line "a B C d".
fn walk_children(
    lines: &mut super::text::Lines,
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    parent_box: crate::boxes::BoxId,
    font: f32,
    ctx: &LayoutCtx,
) {
    for &child_box in tree.children_without_generated(parent_box) {
        let Some(child) = tree.node_of(child_box) else {
            // An ANONYMOUS box (CSS 2.1 §9.2.1.1) has no node but is not
            // transparent: it is the block that wraps a split run, and the text
            // in it ("aaaa"/"cccc" of `<span>aaaa<div>b</div>cccc</span>`)
            // counts, on lines of its own.
            lines.block(anonymous_width(dom, tree, child_box, font, ctx), ctx);
            continue;
        };
        if is_out_of_flow(dom, child) {
            continue;
        }
        if let NodeKind::Text(t) = &dom.node(child).kind {
            // In a flex or grid container a white-space-only text is no item
            // at all (`layout_children_horizontal` drops it); fed to the lines
            // it would pin a space between two items that share none.
            let in_flow = tree.formatting_context(dom, parent_box).inner == crate::boxes::InnerDisplay::Flow;
            if in_flow || !t.trim().is_empty() {
                lines.text(dom, child, font, ctx);
            }
            continue;
        }
        if super::text::is_open_inline(dom, tree, (parent_box, child_box), child, ctx) {
            let css = dom.computed_style_idx(child).unwrap_or_default();
            let f = font_px(&css, font);
            let resolve = ResolveCtx {
                parent_content_w: ctx.viewport_w,
                node_font_size: f,
                root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            let (start, end) = super::text::frame_inline(&css, &resolve);
            if start > 0.0 {
                lines.atom(start, ctx);
            }
            walk_children(lines, dom, tree, child_box, f, ctx);
            if end > 0.0 {
                lines.atom(end, ctx);
            }
            continue;
        }
        // The concrete box travels into the recursion: `child` may itself have
        // several (a nested fragment), and without saying WHICH one is measured
        // here `intrinsic_outer_width` would fall back to `id` alone.
        let w = intrinsic_outer_width_of(dom, tree, child, Some(child_box), font, ctx);
        if matches!(&dom.node(child).kind, NodeKind::Element { tag } if tag == "br") {
            lines.forced_break(ctx);
        } else if close_run(dom, child) {
            lines.block(w, ctx);
        } else if crate::layout::float::float::float_of(dom, child) != crate::style::FloatSide::None {
            lines.float(w, ctx);
        } else if w > 0.0 {
            // A zero-wide child (`display:none`, an empty inline-block) is not
            // placed: placing it would pin a collapsible space before it that
            // the one after it then doubles.
            lines.atom(w, ctx);
        }
    }
}

/// The width of an ANONYMOUS box (CSS 2.1 §9.2.1.1): no node, no width,
/// margin, border or padding of its own (`box-tree.md` §10) — its content is
/// the run that made it, measured by the same widest-line rule as the box
/// around it.
fn anonymous_width(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    box_id: crate::boxes::BoxId,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    // An anonymous TABLE is as wide as the SUM of its columns, not the widest
    // of its cells: measuring the cells as stacked blocks gave a floated row
    // of two 48px cells a width of 48 where Blink gives 96.
    if matches!(tree.kind(box_id), crate::boxes::BoxKind::Anonymous { role: crate::boxes::AnonymousRole::Table, .. }) {
        return crate::table::anonymous_table_widths(dom, tree, box_id, font, ctx).1;
    }
    let mut lines = super::text::Lines::new(false);
    walk_children(&mut lines, dom, tree, box_id, font, ctx);
    lines.finish(ctx)
}

/// Como [`intrinsic_outer_width`](super::intrinsic_outer_width), mas com a
/// caixa CONCRETA de `id` quando o chamador já a tem em mãos — um filho
/// encontrado a percorrer a árvore de caixas, nunca redescoberto por `id`
/// sozinho. É a diferença entre medir SÓ o fragmento em causa e voltar a
/// cair no `.fold(max)` de [`intrinsic_content_width_no_cache`] sobre TODOS
/// os fragmentos de `id`, que é a resposta certa quando não se sabe qual
/// fragmento se quer (o chamador externo, via
/// [`intrinsic_outer_width`](super::intrinsic_outer_width)) e a resposta
/// ERRADA aqui: dentro de UM run específico (uma caixa anónima, ou os
/// filhos de UM fragmento), só o conteúdo DAQUELE fragmento pertence àquela
/// linha.
pub(in crate::layout) fn intrinsic_outer_width_of(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    id: NodeIdx,
    box_id: Option<crate::boxes::BoxId>,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    match &dom.node(id).kind {
        NodeKind::Element { .. } => {
            // metadata (head/style/script) não conta.
            if let NodeKind::Element { tag } = &dom.node(id).kind {
                if is_non_rendered_tag(tag) {
                    return 0.0;
                }
            }
            // `display:none` não gera caixa, logo não tem largura NENHUMA — e
            // contá-la aqui não era um erro pequeno: os quatro menus escondidos
            // do cabeçalho da Wikipédia somavam 520 px à intrínseca do `nav`, o
            // que fazia o `flex-wrap` do `<header>` quebrar linha e empilhar os
            // dois filhos que o Chrome põe lado a lado. A alternativa —
            // filtrá-los em cada CHAMADOR — foi rejeitada por ser a mesma
            // pergunta respondida em cinco sítios; quem sabe que uma caixa não
            // existe é quem mede a caixa.
            if is_display_none(dom, id) {
                return 0.0;
            }
            let css = dom.computed_style_idx(id).unwrap_or_default();
            let f = font_px(&css, parent_font);
            let border_box = css.border_box.unwrap_or(false);
            let resolve = ResolveCtx {
                parent_content_w: ctx.viewport_w,
                node_font_size: f,
                root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            // O frame conta em `resolve_h_intrinseco`: uma percentagem de padding
            // ou margem é contra a largura do containing block, que é o que esta
            // medição existe para ajudar a decidir.
            let frame = css.margin.resolve_h_intrinseco(&resolve)
                + { let [_, r, _, l] = crate::style::borders::used_widths(&css); l + r }
                + css.padding.resolve_h_intrinseco(&resolve);
            // `width` fixo: a caixa tem essa largura. Um `width` em PERCENTAGEM
            // não é fixo — contribui como `auto`, e o conteúdo decide. Sem esta
            // distinção, um `width:50%` respondia metade da VIEWPORT: um item
            // flex com um filho assim ocupava a linha toda e empurrava o irmão
            // para a linha de baixo, que é a origem dos 120px de desvio do `<h1>`
            // da Wikipédia.
            // `max-width`/`min-width` clampam a contribuição ao shrink-to-fit
            // do pai — a EFETIVA, não a crua nem só o natural (CSS2 §10.4);
            // sem isto nem um `width` fixo nem um `min-width` sozinho (sem
            // `width`) entravam na conta (WPT `-min-height-auto-002b/c`).
            let mnw = super::intrinsic_min_max::resolve(css.min_width, dom, id, f, ctx, &resolve);
            let mxw = super::intrinsic_min_max::resolve(css.max_width, dom, id, f, ctx, &resolve);
            if let Some(w) = crate::style::dimensao_absoluta(
                css.width.unwrap_or(crate::style::Dimension::Auto),
                &resolve,
            ) {
                let w = crate::style::clamp_size(w, mnw, mxw);
                return if border_box {
                    w + css.margin.resolve_h_intrinseco(&resolve)
                } else {
                    w + frame
                };
            }
            // senão: a intrínseca do conteúdo, clampada, + frame. SEM cache
            // (`intrinsic_content_width_no_cache`, não o `intrinsic_content_width`
            // público) — `caixa`, quando presente, nomeia um fragmento
            // ESPECÍFICO de `id`, e a cache de `intrinsic_content_width` é
            // chaveada só por `id`: cachear aqui misturaria a resposta de um
            // fragmento com a do outro (ver o comentário da função).
            let content = intrinsic_content_width_no_cache(dom, tree, id, box_id, f, ctx);
            crate::style::clamp_size(content, mnw, mxw) + frame
        }
        // A loose text node: its own lines under its parent element's style —
        // `white-space`, family, weight, slant and spacings, as when the same
        // text is laid out. Measured proportional, bold-less and upright, a
        // text that is its own anonymous cell or flex item came out narrower
        // than it paints — "Some text." at 73.6 where Blink gives 87.97
        // (`claude-linha-so-com-texto`).
        NodeKind::Text(_) => super::text::intrinsic_text_width(dom, id, parent_font, false, ctx),
        _ => 0.0,
    }
}
