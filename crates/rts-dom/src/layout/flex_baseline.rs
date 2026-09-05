//! `align-items: baseline` / `align-self: baseline` no flex (Flexbox §8.5) e
//! a ordem das LINHAS sob `flex-wrap: wrap-reverse` (§8.3) — os dois ganchos
//! que `flex.rs` chama para não crescer além do tecto do crate.
//!
//! O ascent de um item (topo da MARGIN-BOX até à baseline do seu conteúdo) é
//! `margin-top + ascent_do_item` — [`super::linha_ib::ascent_do_item`] já
//! resolve a distância do topo da BORDER-BOX (borda+padding+meia-entrelinha+
//! ascent da fonte, ou o fundo quando o item não tem conteúdo próprio); só
//! não soma a margem do item, que aqui é conhecida por linha. O grupo
//! baseline de uma linha alinha pelo maior ascent (Flexbox §8.5: "the
//! largest of the baselines... is used"); os outros descem
//! `max_ascent − ascent_i` para partilhar essa baseline.
//!
//! O CROSS-SIZE de uma linha com grupo baseline não é o maior item — é o
//! ENVELOPE (Flexbox §9.4 passo 8): `max_ascent + max_descent`, onde
//! `descent_i = altura_outer_i − ascent_i`. Um item de ascent grande e
//! descent pequeno ao lado de um de ascent pequeno e descent grande somam
//! mais do que qualquer um dos dois sozinho — exatamente o padrão de
//! `multiline-reverse-wrap-baseline` (`#third`, `margin-top:5px`, empurra o
//! grupo 5px para baixo SEM alterar a altura de `#fourth`; a linha total
//! cresce os mesmos 5px, não fica do tamanho do maior item). Medido: sem
//! isto a 2ª linha (wrap-reverse) começava 5px cedo demais porque `items_h`
//! só olhava para a altura crua de cada item.
//!
//! CORTES declarados: só a linha (`flex-direction: row`) participa —
//! `coluna.rs::align_offset` trata `Baseline` como `FlexStart`, o fallback
//! que a própria spec prevê quando o eixo cruzado não tem baseline
//! partilhável; um `::before`/`::after` de pseudo-item e um nó de texto
//! solto nunca entram no grupo (a ascent de um pseudo seria a do
//! CONTENTOR, não a dele); `align-content` distribui sobre a ordem já
//! invertida do `wrap-reverse` sem reespelhar `flex-start`/`flex-end`
//! físicos — nenhuma fixture medida exige nenhum dos dois. `last baseline`
//! tem variante PRÓPRIA (`style::AlignItems::LastBaseline`) e nunca entra
//! neste grupo — cai direto no fallback físico de `align_offset`, dito lá.

use super::*;

/// Filhos ELEMENTO deste contentor que são itens de flex EM FLUXO — a mesma
/// filtragem que `flex.rs` aplica antes de construir cada `FlexItem` (tag não
/// renderizável, fora do fluxo, `display:none`), MENOS texto solto e pseudo:
/// nenhum dos dois tem uma baseline própria que [`ascent_do_contentor`] saiba
/// medir — o mesmo corte que o doc deste módulo já declara para o GRUPO da
/// linha ("pseudo-item e texto solto fora do grupo").
fn filhos_flex_em_fluxo(dom: &Dom, id: NodeIdx) -> Vec<NodeIdx> {
    dom.node(id)
        .children
        .iter()
        .copied()
        .filter(|&c| matches!(&dom.node(c).kind, NodeKind::Element { tag } if !is_non_rendered_tag(tag)))
        .filter(|&c| !is_out_of_flow(dom, c) && !e_display_none(dom, c))
        .collect()
}

/// O ascent de um FILHO para efeitos de [`ascent_do_contentor`]: a sua margem
/// própria + a distância do topo da sua BORDER-BOX à baseline — a mesma soma
/// que [`ascent_do_item_flex`] faz, só que sem precisar do `outer_h` do item:
/// o ascent de um item de conteúdo normal (ancorado ao TOPO da própria caixa)
/// não muda com a altura final que o `stretch`/`grow` do eixo cruzado lhe dão
/// — só a margem e a fonte decidem, e é por isso que este cálculo não precisa
/// de reexecutar o algoritmo de flex do filho para saber a resposta. Reusa
/// `linha_ib::ascent_do_item` com uma altura que nunca é o limite — é essa
/// função que, RECURSIVAMENTE, volta para `ascent_do_contentor` quando `id`
/// é, ele próprio, um flex/inline-flex (um flex dentro de outro).
fn ascent_do_item_neto(dom: &Dom, id: NodeIdx, content_w: f32, ctx: &LayoutCtx) -> f32 {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let font = font_px(&css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let mt = css.margin.top.resolve(&resolve).unwrap_or(0.0);
    mt + super::linha_ib::ascent_do_item(dom, id, f32::MAX, content_w, ctx)
}

/// Os filhos, na ORDEM DE FLUXO que a spec usa para decidir "qual item é o
/// primeiro"/"qual linha é a primeira" — por `order` (Flexbox §5.4.1),
/// ascendente, empates pela ordem do documento (`sort_by_key` é ESTÁVEL, e
/// `filhos` já chega em ordem de documento — sem precisar de índice
/// explícito para o desempate). RETRABALHO (2026-09-05, WPT
/// `flex-order-wrap-reverse-baseline`): antes disto, `ascent_do_contentor`
/// lia os filhos pela ordem do DOM crua, e um `order` que trocasse a
/// sequência (o `.a{order:1}`/`.b{order:2}` do WPT, que aqui coincide com a
/// ordem do DOM, mas o `flex-wrap:wrap-reverse` do teste TROCA qual linha é
/// a primeira — ver [`ascent_do_contentor`]) dava uma resposta que não batia
/// com a REFERÊNCIA (que não declara `order` nem `align-self`, e por isso
/// caía sempre no PRIMEIRO item — o item ERRADO sob `wrap-reverse`).
fn em_ordem_de_flex(dom: &Dom, filhos: &[NodeIdx]) -> Vec<NodeIdx> {
    let mut v = filhos.to_vec();
    v.sort_by_key(|&c| dom.computed_style_idx(c).unwrap_or_default().order.unwrap_or(0));
    v
}

/// A largura de CONTEÚDO do PRÓPRIO contentor — não a do ancestral
/// (`content_w`, o parâmetro de todo o resto deste ficheiro) — para decidir
/// onde as linhas de [`linhas_por_largura`] quebram. Só a `width`
/// DECLARADA entra; sem ela, cai no `content_w` do ancestral como
/// aproximação de "tanto quanto houver" — medir o shrink-to-fit de verdade
/// exigiria reexecutar o layout do contentor, o mesmo corte que o resto do
/// ficheiro já assume para o ascent.
fn largura_do_proprio(css: &ComputedStyle, content_w: f32, font_size: f32, ctx: &LayoutCtx) -> f32 {
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    css.width.and_then(|d| d.resolve(&resolve)).unwrap_or(content_w)
}

/// Agrupa `filhos` (já em ORDEM DE FLEX — [`em_ordem_de_flex`]) em LINHAS
/// pela largura DECLARADA — aproximação sem `grow`/`shrink`/`gap`, só para
/// saber a que LINHA cada um pertence: a baseline de fora só precisa disso,
/// nunca do tamanho FINAL (a mesma classe de aproximação estrutural que
/// [`ascent_do_item_neto`] já assume). Um item sem `width` conta 0 (nunca
/// força quebra sozinho) — corte declarado.
fn linhas_por_largura(dom: &Dom, filhos: &[NodeIdx], content_w: f32, font_size: f32, ctx: &LayoutCtx) -> Vec<Vec<NodeIdx>> {
    let mut linhas: Vec<Vec<NodeIdx>> = vec![Vec::new()];
    let mut usado = 0.0f32;
    for &c in filhos {
        let ccss = dom.computed_style_idx(c).unwrap_or_default();
        let resolve = ResolveCtx {
            parent_content_w: content_w,
            node_font_size: font_size,
            root_font_size: crate::style::root_font_size(),
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        let w = ccss.width.and_then(|d| d.resolve(&resolve)).unwrap_or(0.0);
        if !linhas.last().unwrap().is_empty() && usado + w > content_w + 0.01 {
            linhas.push(Vec::new());
            usado = 0.0;
        }
        linhas.last_mut().unwrap().push(c);
        usado += w;
    }
    linhas
}

/// A baseline do CONTENTOR flex, vista de FORA (Flexbox §8.5) — o que
/// [`super::linha_ib::ascent_do_item`] usa quando o nó que ele mede é, ele
/// próprio, um flex/inline-flex, em vez da fórmula genérica (fonte do
/// CONTENTOR) que só está certa para um bloco comum. `h` é a altura outer já
/// resolvida do contentor (a mesma que `ascent_do_item` recebe).
///
/// A "PRIMEIRA LINHA" desta secção é a primeira na ORDEM DE LAYOUT, não a do
/// DOM: os itens contam pela ordem de `order` ([`em_ordem_de_flex`]), e sob
/// `flex-wrap: wrap-reverse` a ordem das LINHAS inverte no eixo cruzado
/// (Flexbox §8.3 — a MESMA leitura que [`reverte_linhas_se_wrap_reverse`] já
/// aplica às linhas REAIS de `flex.rs`): a linha que o documento escreve
/// DEPOIS desenha-se no INÍCIO, e é ELA a "primeira" para a baseline. Sem
/// `flex-wrap` (ou com `nowrap`), tudo cabe numa única linha e só o `order`
/// interessa — nenhuma quebra é sequer calculada.
///
/// Duas regras sobre essa primeira linha, na ordem que o comentário do WPT
/// `flexbox-baseline-multi-item-horiz-001a` cita da spec:
/// 1. Se algum item dela participa do grupo baseline — a MESMA pergunta que
///    [`calcula_linha`] já faz, e só no eixo de LINHA (`flex-direction:
///    row`; numa coluna a baseline de um item de texto normal não é
///    paralela ao eixo principal e por isso nunca participa, Flexbox §8.5 —
///    a mesma leitura que já faz `coluna.rs::align_offset` cair em
///    `FlexStart`) — a baseline do contentor é a desse GRUPO: o
///    `max_ascent` que [`calcula_linha`] devolveria PARA ESSA LINHA.
/// 2. Senão (sem participante nela, ou eixo de coluna), a baseline do
///    contentor é a do PRIMEIRO item DESSA linha — [`ascent_do_item_neto`],
///    que recorre de volta a este ficheiro quando esse item é OUTRO flex.
/// 3. Sem item NENHUM em fluxo: devolve `h` tal e qual — Flexbox §8.5
///    sintetiza a baseline do bottom margin edge quando não há itens, que é
///    exactamente o que `h` já significa para quem chama `ascent_do_item`.
///
/// CORTES declarados: o ascent de cada item calcula por ESTRUTURA (margem +
/// recursão), sem reexecutar o algoritmo de flex do filho — ver o porquê no
/// doc de [`ascent_do_item_neto`]; o AGRUPAMENTO em linhas ([`linhas_por_largura`])
/// é pela largura DECLARADA, sem `grow`/`shrink`/`gap` — a mesma aproximação,
/// generalizada de "um item" para "que linha ele ocupa". `content_w` que
/// chega aos NETOS continua a ser o do CONTENTOR ANCESTRAL, não o deste flex
/// — percentagens de margem num NETO ficam por essa aproximação.
pub(in crate::layout) fn ascent_do_contentor(dom: &Dom, id: NodeIdx, h: f32, content_w: f32, ctx: &LayoutCtx) -> f32 {
    let filhos = filhos_flex_em_fluxo(dom, id);
    if filhos.is_empty() {
        return h;
    }
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);
    let eixo_de_linha = !css.flex_direction.map(|f| f.is_column()).unwrap_or(false);
    let em_flex = em_ordem_de_flex(dom, &filhos);
    let primeira_linha = if eixo_de_linha && css.flex_wrap.map(crate::style::FlexWrap::wraps).unwrap_or(false) {
        let font = font_px(&css, DEFAULT_FONT_SIZE);
        let largura = largura_do_proprio(&css, content_w, font, ctx);
        let mut linhas = linhas_por_largura(dom, &em_flex, largura, font, ctx);
        if css.flex_wrap == Some(crate::style::FlexWrap::WrapReverse) {
            linhas.reverse();
        }
        linhas.into_iter().next().unwrap_or_default()
    } else {
        em_flex
    };
    let Some(&primeiro) = primeira_linha.first() else {
        return h;
    };
    if eixo_de_linha {
        let max_ascent = primeira_linha
            .iter()
            .filter(|&&c| dom.computed_style_idx(c).unwrap_or_default().align_self.unwrap_or(align) == crate::style::AlignItems::Baseline)
            .map(|&c| ascent_do_item_neto(dom, c, content_w, ctx))
            .fold(None, |acc: Option<f32>, a| Some(acc.map_or(a, |m| m.max(a))));
        if let Some(max_ascent) = max_ascent {
            return max_ascent.min(h);
        }
    }
    ascent_do_item_neto(dom, primeiro, content_w, ctx).min(h)
}

/// O ascent de um item de flex — do topo da sua MARGIN-BOX à baseline do seu
/// conteúdo — para o grupo baseline de uma linha. `outer_h` é a altura OUTER
/// já resolvida do item (`FlexItem::h`, pós grow/shrink).
fn ascent_do_item_flex(dom: &Dom, node: NodeIdx, outer_h: f32, content_w: f32, ctx: &LayoutCtx) -> f32 {
    let css = dom.computed_style_idx(node).unwrap_or_default();
    let font = font_px(&css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let mt = css.margin.top.resolve(&resolve).unwrap_or(0.0);
    let mb = css.margin.bottom.resolve(&resolve).unwrap_or(0.0);
    let border_h = (outer_h - mt - mb).max(0.0);
    mt + super::linha_ib::ascent_do_item(dom, node, border_h, content_w, ctx)
}

/// O cross-size e os offsets de UMA linha, à luz do grupo
/// `align-items:baseline`/`align-self:baseline` (`None` quando a linha não
/// tem participante nenhum — o caso comum — sem medir ascent algum). Os dois
/// vêm juntos porque nascem da MESMA passada sobre os ascents: separar a
/// altura do offset exigiria medir os ascents duas vezes.
pub(in crate::layout) struct LinhaBaseline {
    pub(in crate::layout) cross_size: f32,
    /// índice N é `Some(offset)` quando o item N participa do grupo; `None`
    /// cai no `align_offset` normal (`off_cross_item`).
    pub(in crate::layout) offsets: Vec<Option<f32>>,
}

pub(in crate::layout) fn calcula_linha(
    dom: &Dom,
    line: &[super::flex::FlexItem],
    align: crate::style::AlignItems,
    content_w: f32,
    ctx: &LayoutCtx,
) -> Option<LinhaBaseline> {
    let participa: Vec<bool> = line
        .iter()
        .map(|it| {
            it.pseudo.is_none()
                && !it.is_text
                && it.align_self.unwrap_or(align) == crate::style::AlignItems::Baseline
        })
        .collect();
    if !participa.iter().any(|&p| p) {
        return None;
    }
    let ascents: Vec<f32> = line
        .iter()
        .map(|it| ascent_do_item_flex(dom, it.node, it.h, content_w, ctx))
        .collect();
    let max_ascent = ascents
        .iter()
        .zip(&participa)
        .filter(|&(_, &p)| p)
        .map(|(&a, _)| a)
        .fold(0.0f32, f32::max);
    let max_descent = line
        .iter()
        .zip(&ascents)
        .zip(&participa)
        .filter(|&(_, &p)| p)
        .map(|((it, &a), _)| (it.h - a).max(0.0))
        .fold(0.0f32, f32::max);
    // `max(..)` com a altura crua (spec §9.4 passo 8, item 1): um item FORA
    // do grupo pode ainda ser mais alto do que o envelope baseline sozinho.
    let items_h = line.iter().fold(0.0f32, |a, it| a.max(it.h));
    let cross_size = (max_ascent + max_descent).max(items_h);
    let offsets = participa
        .iter()
        .zip(&ascents)
        .map(|(&p, &a)| p.then_some(max_ascent - a))
        .collect();
    Some(LinhaBaseline { cross_size, offsets })
}

/// O offset cruzado final de um item: stretch vence tudo (mesma prioridade
/// de antes); senão, um baseline PRÓPRIO vence o `align_offset` genérico.
pub(in crate::layout) fn off_cross_item(
    stretches: bool,
    align: crate::style::AlignItems,
    baseline: Option<f32>,
    line_h: f32,
    item_h: f32,
) -> f32 {
    if stretches {
        return 0.0;
    }
    match (align, baseline) {
        (crate::style::AlignItems::Baseline, Some(o)) => o,
        _ => align_offset(align, line_h, item_h),
    }
}

/// `flex-wrap: wrap-reverse` troca cross-start/cross-end (Flexbox §8.3): o
/// agrupamento em linhas é o de `wrap` (já feito por quem chama); só a ORDEM
/// em que elas se empilham no eixo cruzado inverte — a linha que o
/// documento escreve DEPOIS desenha-se no INÍCIO do eixo cruzado.
pub(in crate::layout) fn reverte_linhas_se_wrap_reverse(
    lines: &mut [Vec<super::flex::FlexItem>],
    wrap: Option<crate::style::FlexWrap>,
) {
    if wrap == Some(crate::style::FlexWrap::WrapReverse) {
        lines.reverse();
    }
}
