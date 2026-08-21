//! A GEOMETRIA das caixas que vivem dentro de uma linha.
//!
//! O fluxo inline de `layout.rs` produz RUNS de texto e pinta-os; o que faltava
//! era dizer *que retângulo* cada elemento inline ocupa. Um `<a>`, um `<span>`,
//! um `<img>` no meio de um parágrafo têm caixa no browser
//! (`getBoundingClientRect`) e respondiam `0,0,0,0` aqui, o que os torna
//! inexistentes para hit-test, para scroll-into-view e para qualquer medição.
//!
//! Este módulo é a parte dessa resposta que NÃO é o fluxo: como se dimensiona
//! uma caixa atómica (um replaced element) e como se acumula a união dos
//! fragmentos de um elemento que quebrou em várias linhas. O fluxo em si — a
//! quebra e a colocação — fica em `layout.rs`, porque é lá que ele já estava; a
//! alternativa (trazer o wrap para aqui) partia o único sítio onde a ordem de
//! pintura é decidida, e este trabalho acrescenta geometria sem tocar na pintura.

use crate::dom::{Dom, NodeIdx};
use crate::layout::{DisplayList, LayoutCtx, Rect, TextMeasurer};
use crate::style::{ComputedStyle, ResolveCtx};

/// O que é uma caixa ATÓMICA no fluxo inline — flui como uma palavra
/// inquebrável, em vez de como texto que se pode partir ao meio.
///
/// São três e não duas porque o `Marker` não é uma caixa: é um elemento inline
/// VAZIO (`<source>`, um `<span></span>`) que no browser tem posição e altura
/// de linha mas largura zero. Tratá-lo como `Replaced` de tamanho zero era a
/// alternativa e está errada por duas razões: entraria na conta da altura da
/// linha (um `<source>` sozinho passaria a criar linha, mudando o layout) e
/// receberia altura zero, quando o Chrome lhe dá a altura da linha em que está.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum AtomicKind {
    /// Um controlo de formulário (`<input>`/`<button>`) pintado pela emissão.
    Widget,
    /// Um replaced element (`<img>`, `<video>`, …): caixa própria, não é texto.
    Replaced,
    /// Um elemento inline vazio: posição e altura de linha, largura zero.
    Marker,
    /// Um `<br>`: fecha a linha corrente. Tem caixa (posição e altura de linha,
    /// largura zero) como o `Marker`, e além disso QUEBRA — que é o que o
    /// distingue dele e a razão de não ser o mesmo caso.
    Break,
    /// Um inline com CAIXA (`<span style=background>`, `<a>` com padding): tem
    /// fundo e borda próprios, logo precisa de `layout_block` para os pintar,
    /// mas continua a ser conteúdo de linha e não deve parti-la.
    Block,
}

/// Tamanho de um replaced element inline, ou `None` se a tag não é replaced.
///
/// A ordem — CSS, depois atributo HTML, depois o natural da imagem — é a da
/// spec e é o que faz a Wikipédia medir: as suas imagens trazem `width`/
/// `height` no HTML e, sem rede, nunca chegam a ter pixels. Devolver `None`
/// nesse caso (a alternativa: "sem pixels não há caixa") é exatamente o que
/// deixava 4 dos 20 piores desvios do harness de paridade a zero.
///
/// Um `<img>` sem nenhuma dimensão devolve `Some((0,0))` e não `None`: no
/// browser é uma caixa de área nula, mas COM posição, e é a posição que o
/// chamador precisa de registar.
pub(crate) fn replaced_inline_size(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    avail_w: f32,
    ctx: &LayoutCtx,
) -> Option<(f32, f32)> {
    let crate::dom::NodeKind::Element { tag } = &dom.node(id).kind else {
        return None;
    };
    // `<svg>` e `<canvas>` não estão aqui de propósito: `is_block_level` já os
    // manda para o caminho de bloco, que os pinta. Duplicar a decisão aqui era
    // criar um segundo sítio onde o tamanho de um replaced se decide.
    let default_box = match tag.as_str() {
        "img" => None,
        // O default do HTML para estes é 300x150 (a mesma caixa do canvas).
        "video" | "iframe" | "embed" | "object" => Some((300.0, 150.0)),
        _ => return None,
    };
    let font = crate::layout::font_px(css, crate::layout::DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: crate::layout::DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let node = dom.node(id);
    let attr_px = |name: &str| -> Option<f32> {
        node.attr(name).and_then(|v| {
            let v = v.trim().trim_end_matches("px").trim();
            v.parse::<f32>().ok().filter(|n| *n >= 0.0)
        })
    };
    let w0 = css
        .width
        .and_then(|d| d.resolve(&resolve))
        .or_else(|| attr_px("width"));
    let h0 = css
        .height
        .and_then(|d| d.resolve(&resolve))
        .or_else(|| attr_px("height"));
    // razão de aspecto: a dos pixels quando existem, senão nenhuma (uma
    // dimensão só fica sozinha em vez de inventar a outra).
    let ratio = dom
        .image_of(id)
        .filter(|(_, _, iw, ih)| *iw > 0 && *ih > 0)
        .map(|(_, _, iw, ih)| (iw as f32, ih as f32));
    let (mut w, mut h) = match (w0, h0) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, ratio.map(|(nw, nh)| w * nh / nw).unwrap_or(0.0)),
        (None, Some(h)) => (ratio.map(|(nw, nh)| h * nw / nh).unwrap_or(0.0), h),
        (None, None) => match (ratio, default_box) {
            (Some((nw, nh)), _) => (nw, nh),
            (None, Some(d)) => d,
            (None, None) => (0.0, 0.0),
        },
    };
    // não estoura a linha: encolhe mantendo a razão, como `layout_image`.
    let max_w = avail_w.max(0.0);
    if w > max_w && w > 0.0 {
        h = h * max_w / w;
        w = max_w;
    }
    Some((w.max(0.0), h.max(0.0)))
}

/// A ALTURA DE UMA LINHA de texto sob este estilo.
///
/// Existe como função porque a resposta estava em dois estados no motor: o fluxo
/// inline lia `line-height` do CSS, e os caminhos de flex/wrap que recebem texto
/// solto perguntavam direto ao medidor. O mesmo `<p>` respondia 26px ou 20,8px
/// conforme o `display` do pai, e um `line-height` declarado desaparecia sem
/// aviso no segundo caso. Uma pergunta, uma resposta.
///
/// O medidor é quem responde por `normal` e pela ausência de declaração, porque
/// esse valor sai das MÉTRICAS DA FONTE e não de uma constante.
pub(crate) fn altura_da_linha(css: &ComputedStyle, font_size: f32, m: &dyn TextMeasurer) -> f32 {
    let normal = m.line_height(font_size);
    css.line_height.map(|l| l.resolve(font_size, normal)).unwrap_or(normal)
}

/// Este estilo cria uma caixa que precisa de LAYOUT DE BLOCO?
///
/// Não é a mesma pergunta que `ComputedStyle::has_box`, e a diferença é o que
/// tirava 5 262 dos 5 263 `<a>` da Wikipédia do fluxo inline: `has_box` responde
/// "há algo para pintar por este caminho" e por isso conta o `border-radius` e o
/// `outline`. Nenhum dos dois CRIA caixa — um raio sem fundo nem borda não pinta
/// nada, e o outline não ocupa espaço por definição. Um elemento inline que só
/// os declara continua a ser texto a fluir, e é isso que o browser faz.
///
/// O que blockifica é o que ocupa espaço ou pinta uma superfície: fundo,
/// gradiente, sombra, padding, margem, borda, largura ou altura.
pub(crate) fn cria_caixa_de_bloco(css: &ComputedStyle) -> bool {
    css.bg.is_some()
        || css.gradient.is_some()
        || css.box_shadow.is_some()
        || css.padding.any_set()
        || css.margin.any_set()
        || css.border_width.is_some()
        || css.border_widths.any_set()
        || css.width.is_some()
        || css.height.is_some()
}

/// A altura da CAIXA de um elemento inline — que NÃO é a altura da linha.
///
/// A distinção é a do CSS entre a caixa de linha e a caixa do inline: o
/// `getBoundingClientRect` de um `<a>` devolve a *content area* — ascent +
/// descent da fonte —, e o `line-height` decide só onde a linha SEGUINTE começa.
/// Com `line-height: 26px` e uma fonte de 16px são ~8px de diferença por
/// elemento, e a Wikipédia tem 3 032 `<a>`: dar a altura da linha em vez desta
/// somava ~24 500px à página.
///
/// A content area é pedida ao medidor como o `normal` dele, que é o valor que
/// sai das métricas da fonte: um backend com fonte real responde ascent+descent,
/// e o medidor aproximado responde a sua constante calibrada. Não há aqui um
/// segundo número — é o mesmo `line_height` que responde por `normal`.
pub(crate) fn altura_do_conteudo(font_size: f32, m: &dyn TextMeasurer) -> f32 {
    m.line_height(font_size)
}

/// Meia-entrelinha: o espaço que sobra da caixa de linha depois da content area
/// é repartido IGUALMENTE acima e abaixo (CSS 2.1 §10.8, "half-leading"). É o
/// que põe o texto no meio da linha quando o `line-height` é maior do que a
/// fonte — e o que faz a caixa do inline começar abaixo do topo da linha.
///
/// Pode ser NEGATIVA (`line-height` menor que a fonte): aí a content area
/// transborda a linha, que é o que o browser também faz.
pub(crate) fn meia_entrelinha(altura_da_linha: f32, conteudo: f32) -> f32 {
    (altura_da_linha - conteudo) / 2.0
}

/// Une um fragmento de linha ao retângulo acumulado de um elemento inline.
///
/// É a definição da spec para `getBoundingClientRect` de um inline: a bounding
/// box dos border boxes dos seus fragmentos. Um `<a>` que quebra em duas linhas
/// tem dois fragmentos e um retângulo que os contém aos dois — deliberadamente
/// mais largo do que qualquer um deles, que é o que o browser também devolve.
pub(crate) fn union_rect(list: &mut DisplayList, idx: NodeIdx, fragment: Rect) {
    if let Some(old) = list.node_rects.get_mut(&idx) {
        // Um placeholder reservado (`reserve_node_order`) é 0,0,0,0 e não é um
        // fragmento: uni-lo puxaria a caixa até à origem do documento.
        if old.w == 0.0 && old.h == 0.0 && old.x == 0.0 && old.y == 0.0 {
            *old = fragment;
            return;
        }
        let right = (old.x + old.w).max(fragment.x + fragment.w);
        let bottom = (old.y + old.h).max(fragment.y + fragment.h);
        let x = old.x.min(fragment.x);
        let y = old.y.min(fragment.y);
        *old = Rect::new(x, y, right - x, bottom - y);
    } else {
        list.node_rects.insert(idx, fragment);
        list.hit_order.push(idx);
    }
}

#[cfg(test)]
mod tests {
    use crate::table::tests::{geometria, rect};

    /// Um `<img width height>` sem pixels carregados OCUPA a sua caixa. É o que o
    /// browser faz enquanto a imagem não chegou da rede — e sem rede nunca chega,
    /// que é a situação de todo o harness de paridade.
    #[test]
    fn imagem_sem_pixels_ocupa_a_caixa_que_declara() {
        let (dom, list) = geometria("<div><img width='252' height='252'></div>", 800.0);
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 252.0).abs() < 0.5, "largura = {}", img.w);
        assert!((img.h - 252.0).abs() < 0.5, "altura = {}", img.h);
    }

    /// E a caixa CONTA para a largura intrínseca de quem encolhe ao conteúdo.
    ///
    /// É a cadeia inteira do defeito medido contra o Chrome: a `<figure>` do
    /// MediaWiki é `display:table`, encolhe ao conteúdo, e com a imagem a medir
    /// zero ficava com 10px em vez de 260 — a `<figcaption>` ao lado passava a
    /// quebrar a um carácter por linha, 700px de altura onde o Chrome tem 107.
    /// A imagem é que estava errada; a legenda só pagava.
    #[test]
    fn a_figura_que_encolhe_ao_conteudo_mede_a_imagem_sem_pixels() {
        let html = "<figure style='display:table'>\
            <img width='252' height='252'>\
            <figcaption style='display:table-caption'>aa bb cc dd</figcaption>\
            </figure>";
        let (dom, list) = geometria(html, 800.0);
        let fig = rect(&dom, &list, "figure", 0);
        assert!(fig.w >= 252.0, "a figura encolheu a {} em volta de uma imagem de 252", fig.w);
    }

    /// Sem pixels a caixa existe mas NADA é pintado: uma reserva vazia é o que o
    /// browser mostra, e pintar um retângulo ali seria inventar conteúdo.
    #[test]
    fn imagem_sem_pixels_reserva_a_caixa_mas_nao_pinta_nada() {
        let (_dom, list) = geometria("<div><img width='40' height='40'></div>", 800.0);
        let pintadas = list
            .materialized()
            .iter()
            .filter(|i| matches!(i, crate::layout::DisplayItem::Image { .. }))
            .count();
        assert_eq!(pintadas, 0, "pintou {pintadas} imagem(ns) sem pixels");
    }

    /// Um `<img>` que não declara dimensão nenhuma e não tem pixels continua sem
    /// caixa. O par com os testes acima é o que prova que a caixa vem do que se
    /// DECLARA, e não de o elemento ser um `<img>`.
    #[test]
    fn imagem_sem_dimensao_nenhuma_continua_sem_caixa() {
        let (dom, list) = geometria("<div><img></div>", 800.0);
        let ids = dom.query_all("img");
        let idx = dom.resolve(ids[0]).unwrap();
        let r = list.geometry_now().rects.get(&idx).copied();
        assert!(r.is_none_or(|r| r.w == 0.0), "caixa inventada: {r:?}");
    }
}

#[cfg(test)]
mod quebra_de_linha {
    use crate::table::tests::{geometria, rect};

    /// Um aglomerado SEM whitespace desce inteiro para a linha seguinte.
    ///
    /// `<i>y</i><b>z</b>` não tem espaço entre os dois: em CSS não há ali
    /// oportunidade de quebra nenhuma, e o browser move os dois juntos. Partir
    /// o aglomerado punha o `y` no fim de uma linha e o `z` no início da outra,
    /// e a caixa do pai — que é a UNIÃO dos fragmentos, e essa estava certa —
    /// passava a ser um retângulo com a largura da linha inteira.
    ///
    /// É a forma exata do pior desvio de largura da Wikipédia: as referências
    /// (`<sup class="mw-ref"><a><span>[1]</span></a></sup>`) saíam com 752x41
    /// onde o Chrome dá 21x15. O que estava errado era o sítio do corte, não a
    /// união.
    ///
    /// Medida com o `ApproxMeasurer`: 8px por carácter a 16px. "aaaaa" mede 40,
    /// o espaço 8, e cada letra 8 — o aglomerado `yz` pede 8+16=24 sobre os 40
    /// já postos, logo 64 numa caixa de 60: desce, e desce inteiro.
    #[test]
    fn aglomerado_sem_espacos_desce_inteiro_para_a_linha_seguinte() {
        let (dom, list) =
            geometria("<p style='width:60px'>aaaaa <sup><i>y</i><b>z</b></sup></p>", 800.0);
        let i = rect(&dom, &list, "i", 0);
        let b = rect(&dom, &list, "b", 0);
        let sup = rect(&dom, &list, "sup", 0);
        assert_eq!(i.y, b.y, "o aglomerado não é partido: i={i:?} b={b:?}");
        assert!(i.x < b.x, "e mantém a ordem na mesma linha: i={i:?} b={b:?}");
        assert!(
            sup.w < 30.0,
            "a caixa do pai é a do aglomerado, não a da linha: {sup:?}"
        );
    }
}
