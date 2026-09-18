//! A parte de `medida.rs` que percorre a ÁRVORE DE CAIXAS — extraída para
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
/// [`intrinsic_outer_width_de`] quando o chamador já tem uma caixa
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
pub(in crate::layout) fn intrinsic_content_width_sem_cache(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    id: NodeIdx,
    caixa: Option<crate::boxes::BoxId>,
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
            crate::inline_box::replaced_inline_size(dom, id, &css, f32::INFINITY, (None, None), ctx)
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
    if let Some(w) = dom.computed_style_idx(id).and_then(|css| super::input::tamanho_natural_controlo(dom, id, &css, ctx)).map(|(w, _)| w) { return w; }
    // folha de texto puro → largura do texto.
    let own_text = collect_text(dom, id);
    let only_text = !dom.node(id).children.is_empty()
        && dom
            .node(id)
            .children
            .iter()
            .all(|&c| matches!(dom.node(c).kind, NodeKind::Text(_)));
    if (dom.node(id).children.is_empty() || only_text) && !own_text.trim().is_empty() {
        let css = dom.computed_style_idx(id);
        let mono = css
            .as_ref()
            .and_then(|c| c.font_family.as_ref())
            .map(|f| crate::style::is_mono_family(f))
            .unwrap_or(false);
        // o peso importa p/ a largura natural: medir regular mas o wrap/paint usar bold
        // (mais largo) faz o conteúdo não caber na largura natural → quebra indevida.
        let bold = css.as_ref().and_then(|c| c.bold).unwrap_or(false);
        // `letter-spacing` entra na LARGURA e não só na pintura: o medidor não o
        // recebe (a assinatura do trait é partilhada com o backend do egui), por
        // isso soma-se aqui — n espaçamentos para n caracteres, ver
        // `style::text_metrics::spacing_width`. Sem isto, uma caixa que encolhe
        // ao conteúdo ficava com a largura do texto SEM espaçamento e o texto
        // transbordava dela.
        let ls = css.as_ref().and_then(|c| c.letter_spacing).unwrap_or(0.0);
        // `tab-size`/`word-spacing`: a MESMA largura extra que `wrap_runs` soma
        // depois — ver `tabulacao::ajustar_texto_intrinsico` para porquê isto
        // não pode viver só lá. Sem isto, a largura shrink-to-fit (a de um
        // `inline-block`/item flex sem `width`) não respondia a nenhuma das
        // duas: media sempre o texto cru, e é ESSA largura — não a do
        // `wrap_runs`, que só corre depois de a caixa já estar decidida — que
        // vira a caixa de um elemento sem `width`.
        let (own_text, ws_extra) =
            crate::layout::tabulacao::ajustar_texto_intrinsico(own_text, css.as_deref());
        // o hífen suave não pesa na largura natural (`hifen.rs`, regra 1).
        let own_text = super::hifen::sem_shy(&own_text).into_owned();
        // O whitespace colapsa como no fluxo (CSS Text §4.1): a indentação do
        // HTML entre filhos não é conteúdo — catorze espaços e quebras de linha
        // davam 103px a mais ao botão fixo do Bootstrap cover
        // (`claude-intrinseco-whitespace`). Só `pre`/`pre-wrap` os preservam.
        let preserva = css.as_ref().and_then(|c| c.white_space).is_some_and(|w| w.preserves_newlines());
        let own_text = if preserva { own_text } else { super::segmento::collapse_ws(&own_text, false).into_owned() };
        // o mesmo raciocínio do peso vale para o estilo: medir com a família
        // errada muda a largura natural e com ela o sítio onde a linha quebra.
        let italic = italico(css.as_deref(), tag_de(dom, id), false);
        let family = css.as_ref().and_then(|c| c.font_family.as_deref());
        let width = ctx.measurer.text_width_family(&own_text, font, family, mono, bold, italic)
            + crate::style::spacing_width(own_text.chars().count(), ls)
            + ws_extra;
        return width;
    }

    // TABELA: a largura que o conteúdo quer é a SOMA das colunas, e nenhuma das
    // duas regras abaixo a dá — o MAX (bloco) devolveria a linha mais larga e a
    // SOMA (flex) somaria linhas inteiras. Quem sabe é o algoritmo de colunas.
    if used_display(dom, id).is_some_and(crate::style::DisplayKind::is_table_box) {
        return crate::table::max_content_width(dom, id, font, ctx);
    }

    match caixa {
        // O chamador já tem a caixa em mãos (um fragmento específico, ou o
        // caso ubíquo de um nó com uma caixa só, resolvido no `match` de
        // baixo antes de chegar aqui recursivamente): usa exatamente essa.
        Some(c) => intrinsic_content_width_geral(dom, tree, id, Some(c), font, ctx),
        // Sem caixa em mãos: dobra sobre as de `id`.
        None => match tree.boxes_of(id) {
            // O caso ubíquo: um nó normal, uma caixa só — comportamento
            // idêntico ao de antes deste lote (era exatamente este o único
            // caminho que existia).
            [c] => intrinsic_content_width_geral(dom, tree, id, Some(*c), font, ctx),
            // Sem caixa nenhuma (ex.: `display:none`, cascata recusou): sem
            // filhos para medir.
            [] => intrinsic_content_width_geral(dom, tree, id, None, font, ctx),
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
                .map(|&c| intrinsic_content_width_geral(dom, tree, id, Some(c), font, ctx))
                .fold(0.0, f32::max),
        },
    }
}

/// O corpo "geral" de [`intrinsic_content_width`]: soma (flex-row) ou maior
/// das linhas (bloco), sobre os filhos de UMA caixa específica — nunca sobre
/// `id` sozinho, que é ambíguo quando `id` tem mais de uma caixa (ver o
/// comentário no chamador). `caixa` é `None` só quando `id` não gerou caixa
/// nenhuma.
fn intrinsic_content_width_geral(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    id: NodeIdx,
    caixa: Option<crate::boxes::BoxId>,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    // o EIXO em que os filhos se dispõem decide SOMA vs MAX: um flex em COLUNA
    // empilha como um bloco (o maior filho), mesmo com `flex-wrap` — a
    // multi-coluna do wrap é corte dito (`claude-flex-column-shrink-to-fit`).
    let display = css_display(dom, id);
    let em_coluna = dom.computed_style_idx(id).and_then(|c| c.flex_direction).map(|f| f.is_column()).unwrap_or(false);
    let is_row = (display == crate::block::DISPLAY_HORIZONTAL || display == crate::block::DISPLAY_WRAP) && !em_coluna;
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

    let mut sum = 0.0f32;
    let mut count: usize = 0;
    // Fora de flex, o max-content NÃO é o maior filho: é o maior das LINHAS, e
    // filhos inline consecutivos partilham uma. Era o `max` de todos, e por isso
    // `<td><i></i><i></i></td>` com dois `inline-block` de 50 media 50 onde o
    // Chrome mede 100 — a linha soma-os.
    //
    // Medido no Chrome com `width:max-content`:
    //
    //   dois inline-block de 50 ............ 100   (soma)
    //   três inline-block de 50 ............ 150   (soma)
    //   dois BLOCK de 50 .................... 50   (cada um a sua linha)
    //   inline 50 + BLOCK 50 + inline 50 .... 50   (três corridas, máximo 50)
    //   dois inline com <br> no meio ........ 50   (o <br> fecha a corrida)
    //   texto "xy" + inline-block 50 ........ 66   (o texto entra na corrida)
    //
    // A quarta linha é a que prova que não basta somar tudo, e a quinta é a que
    // obriga a olhar para o `<br>`: ele não é de bloco e mesmo assim quebra.
    let mut linha = 0.0f32;
    let mut maior = 0.0f32;
    let filhos: &[crate::boxes::BoxId] = caixa.map(|c| tree.children(c)).unwrap_or(&[]);
    for &caixa_filho in filhos {
        let Some(child) = tree.node_of(caixa_filho) else {
            // Caixa ANÓNIMA (CSS 2.1 §9.2.1.1): não tem nó, mas não é
            // transparente — é o bloco que a norma manda envolver o run
            // partido, e o texto lá dentro ("aaaa"/"cccc" de
            // `<span>aaaa<div>b</div>cccc</span>`) tinha de continuar a
            // contar na largura intrínseca do container. Saltá-la (como o
            // `continue` fazia até este fix) apagava esse texto da conta —
            // 147cb3e53 mudou a travessia para a árvore de caixas e herdou o
            // salto do `node_of(caixa_filho) else { continue }` de
            // `min_content_na_arvore`, sem o tratar. `is_row` nunca é `true`
            // aqui: só um contentor de FLUXO parte (`boxes::build`), nunca um
            // flex, por isso a caixa anónima só pode fechar a corrida como
            // qualquer bloco.
            let w = largura_anonima(dom, tree, caixa_filho, font, ctx);
            maior = maior.max(linha).max(w);
            linha = 0.0;
            continue;
        };
        // fora do fluxo não contribui para a largura intrínseca do container.
        if is_out_of_flow(dom, child) {
            continue;
        }
        // Numa linha de FLEX, um nó de texto só-espaços não é item nenhum — o
        // pré-passo de `layout_children_horizontal` descarta-o (`trim().is_empty()`)
        // e aqui ele contava DUAS vezes: a largura do `"\n\t\t"` e mais um `gap`
        // por ser um item a mais. Eram 155 px no `.vector-header-start` da
        // Wikipédia. Fora de flex não se toca: entre dois inline o espaço é
        // largura real, e essa pergunta é do fluxo inline, não desta função.
        if is_row && matches!(&dom.node(child).kind, NodeKind::Text(t) if t.trim().is_empty()) {
            continue;
        }
        // A caixa concreta (`caixa_filho`) viaja até à recursão: `child` pode
        // ele próprio ter várias caixas (um fragmento aninhado), e sem lhe
        // dizer QUAL delas estamos a medir aqui, `intrinsic_outer_width`
        // voltaria a cair no `id` sozinho — o mesmo tipo de ambiguidade que a
        // caixa `caixas => …` acima existe para resolver.
        let w = intrinsic_outer_width_de(dom, tree, child, Some(caixa_filho), font, ctx);
        if w > 0.0 {
            count += 1;
        }
        sum += w;
        if fecha_a_corrida(dom, child) {
            maior = maior.max(linha).max(w);
            linha = 0.0;
        } else {
            linha += w;
        }
    }
    maior = maior.max(linha);
    // `::before`/`::after` de um flex em linha são itens (Flexbox §4) e entram
    // na largura natural do contentor — o caret do botão do Bootstrap.
    if is_row {
        for pe in [crate::style::PseudoElement::Before, crate::style::PseudoElement::After] {
            let w = super::flex_pseudo::largura(dom, id, pe, font, ctx);
            if w > 0.0 {
                sum += w;
                count += 1;
            }
        }
    }
    if is_row {
        // soma + gaps entre os itens.
        sum + (count.saturating_sub(1)) as f32 * gap
    } else {
        maior
    }
}

/// A largura de uma caixa ANÓNIMA (CSS 2.1 §9.2.1.1): sem nó, sem `width`,
/// margem, borda ou padding próprios (`box-tree.md` §10: "no width to
/// resolve, no margin, no border, no background") — o seu conteúdo é o RUN
/// que a gerou, e mede-se com a MESMA regra "maior das linhas" da caixa que a
/// envolve: um filho de bloco (ou `<br>`) fecha a corrida, os demais somam-se
/// na linha corrente. Recursiva porque uma caixa anónima aninhada, ainda que
/// `boxes::build` hoje não produza uma, não deve voltar a ser saltada por um
/// segundo `continue` no dia em que produzir.
fn largura_anonima(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    caixa: crate::boxes::BoxId,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    // An anonymous TABLE is as wide as the SUM of its columns, not the widest
    // of its cells: measuring the cells as stacked blocks gave a floated row
    // of two 48px cells a width of 48 where Blink gives 96.
    if matches!(tree.kind(caixa), crate::boxes::BoxKind::Anonymous { role: crate::boxes::AnonymousRole::Table, .. }) {
        return crate::table::anonymous_table_widths(dom, tree, caixa, font, ctx).1;
    }
    let mut linha = 0.0f32;
    let mut maior = 0.0f32;
    for &filho in tree.children(caixa) {
        let Some(child) = tree.node_of(filho) else {
            let w = largura_anonima(dom, tree, filho, font, ctx);
            maior = maior.max(linha).max(w);
            linha = 0.0;
            continue;
        };
        if is_out_of_flow(dom, child) {
            continue;
        }
        let w = intrinsic_outer_width_de(dom, tree, child, Some(filho), font, ctx);
        if fecha_a_corrida(dom, child) {
            maior = maior.max(linha).max(w);
            linha = 0.0;
        } else {
            linha += w;
        }
    }
    maior.max(linha)
}

/// Como [`intrinsic_outer_width`](super::intrinsic_outer_width), mas com a
/// caixa CONCRETA de `id` quando o chamador já a tem em mãos — um filho
/// encontrado a percorrer a árvore de caixas, nunca redescoberto por `id`
/// sozinho. É a diferença entre medir SÓ o fragmento em causa e voltar a
/// cair no `.fold(max)` de [`intrinsic_content_width_sem_cache`] sobre TODOS
/// os fragmentos de `id`, que é a resposta certa quando não se sabe qual
/// fragmento se quer (o chamador externo, via
/// [`intrinsic_outer_width`](super::intrinsic_outer_width)) e a resposta
/// ERRADA aqui: dentro de UM run específico (uma caixa anónima, ou os
/// filhos de UM fragmento), só o conteúdo DAQUELE fragmento pertence àquela
/// linha.
pub(in crate::layout) fn intrinsic_outer_width_de(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    id: NodeIdx,
    caixa: Option<crate::boxes::BoxId>,
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
            if e_display_none(dom, id) {
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
            let mnw = super::intrinseco_min_max::resolve(css.min_width, dom, id, f, ctx, &resolve);
            let mxw = super::intrinseco_min_max::resolve(css.max_width, dom, id, f, ctx, &resolve);
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
            // (`intrinsic_content_width_sem_cache`, não o `intrinsic_content_width`
            // público) — `caixa`, quando presente, nomeia um fragmento
            // ESPECÍFICO de `id`, e a cache de `intrinsic_content_width` é
            // chaveada só por `id`: cachear aqui misturaria a resposta de um
            // fragmento com a do outro (ver o comentário da função).
            let conteudo = intrinsic_content_width_sem_cache(dom, tree, id, caixa, f, ctx);
            crate::style::clamp_size(conteudo, mnw, mxw) + frame
        }
        // Um nó de texto solto mede-se COLAPSADO (CSS Text §4.1) — o mesmo
        // motivo de `intrinsic_content_width`; `pre` num pai não é visto aqui
        // (corte dito: mede-se colapsado na mesma).
        NodeKind::Text(t) => ctx.measurer.text_width(
            &super::segmento::collapse_ws(&super::hifen::sem_shy(t), false),
            parent_font,
            false,
            false,
            false,
        ),
        _ => 0.0,
    }
}
