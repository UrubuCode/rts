//! Dimensionamento de elementos SUBSTITUÍDOS: `<img>`, e o `<source>` que um
//! `<picture>` escolhe para ele.
//!
//! Saiu do `inline_box.rs` porque são dois assuntos e não um ficheiro grande:
//! o que fica lá é espaço em branco, criação de caixa e quebra de linha.
//! Nenhuma linha de lógica foi alterada.

use super::*;

/// O `<source>` de um `<picture>` que vale para ESTE `<img>`, se houver um.
///
/// O algoritmo do HTML é: percorrer os `<source>` na ordem do documento, saltar
/// os que declaram um `media` que não casa, e ficar com o primeiro que sobra; se
/// nenhum sobra, é o `<img>` que responde. Os atributos `width`/`height` da
/// `<source>` escolhida passam a ser os do elemento — é por isso que o rodapé da
/// Wikipédia mede 84×29 no Chrome e não os 25×25 que o `<img>` declara.
///
/// O `media` é avaliado pelo MESMO `MediaQuery` que serve os blocos `@media`, e
/// não por uma leitura própria: é a mesma condição escrita na mesma gramática, e
/// um segundo avaliador seria um segundo sítio onde `min-width` pode divergir.
/// Herda também a sua honestidade — uma feature que ele não suporta torna a
/// query sempre-falsa, portanto uma `<source>` que peça `orientation` é saltada
/// em vez de escolhida por engano.
///
/// **Fica de fora, e é dito em vez de aproximado:** os descritores `w`/`x` do
/// `srcset` (qual candidato para qual densidade) e o `type` (saltar um formato
/// que não sabemos descodificar). Nenhum dos dois muda a GEOMETRIA — que é o que
/// esta função existe para responder — porque as dimensões vêm dos atributos da
/// `<source>` e não do candidato; mudam qual ficheiro se carregaria, e este
/// motor ainda não carrega nenhum por esta via.
fn fonte_de_picture(dom: &Dom, img: NodeIdx, viewport_w: f32) -> Option<NodeIdx> {
    let pai = dom.node(img).parent?;
    let crate::dom::NodeKind::Element { tag } = &dom.node(pai).kind else {
        return None;
    };
    if tag != "picture" {
        return None;
    }
    dom.node(pai).children.iter().copied().find(|&f| {
        let crate::dom::NodeKind::Element { tag } = &dom.node(f).kind else {
            return false;
        };
        if tag != "source" {
            return false;
        }
        // Sem `media` a `<source>` casa sempre; é a forma que serve só para
        // oferecer outro formato ou outra densidade.
        dom.node(f).attr("media").is_none_or(|m| {
            crate::style::stylesheet::MediaQuery::parse(m).matches(viewport_w)
        })
    })
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
    // Dentro de um `<picture>`, quem dá as dimensões é o `<source>` ESCOLHIDO —
    // o `<img>` é só o fallback de quem não sabe escolher.
    let node = dom.node(fonte_de_picture(dom, id, ctx.viewport_w).unwrap_or(id));
    let attr_px = |name: &str| -> Option<f32> {
        node.attr(name).and_then(|v| {
            let v = v.trim().trim_end_matches("px").trim();
            v.parse::<f32>().ok().filter(|n| *n >= 0.0)
        })
    };
    // `auto` DECLARADO não é o mesmo que não declarado, e confundi-los era o que
    // punha a altura do atributo HTML de volta contra a vontade do CSS: o
    // `width`/`height` de um `<img>` é um *presentational hint* de especificidade
    // zero, logo qualquer declaração o vence — e `.mw-file-element{height:auto}`
    // é exatamente essa declaração em todas as miniaturas da Wikipédia.
    //
    // A alternativa era continuar a ler `Option<f32>`: `Dimension::Auto` resolve
    // `None`, indistinguível de "o autor não disse nada", e é essa perda de
    // informação que o `or_else` transformava em silêncio.
    let declarado = |d: Option<crate::style::Dimension>, attr: &str| match d {
        Some(crate::style::Dimension::Auto) => None,
        Some(d) => d.resolve(&resolve),
        None => attr_px(attr),
    };
    let w0 = declarado(css.width, "width");
    let h0 = declarado(css.height, "height");
    // A razão de aspecto: a dos pixels quando existem e, quando não, a dos
    // ATRIBUTOS `width`/`height` do HTML.
    //
    // A segunda é spec (HTML, "dimension attributes": os dois atributos juntos
    // dão ao elemento um `aspect-ratio: auto w / h`) e existe precisamente para o
    // caso deste harness — dimensionar antes de a imagem chegar da rede, que é o
    // que evita o salto de layout. Sem ela, `height:auto` num `<img width height>`
    // ficava sem razão nenhuma e a altura saía ZERO: a miniatura da Wikipédia
    // media 252x2, só as bordas.
    //
    // A alternativa rejeitada é a que o Chrome offline mostra — sem razão, cair
    // num quadrado (252x252). Isso não é regra de CSS nenhuma: é o que aquele
    // browser faz com uma imagem que FALHOU a carregar, e copiá-lo seria acertar
    // a régua contra o defeito de rede em vez de contra a página.
    let ratio = dom
        .image_of(id)
        .filter(|(_, _, iw, ih)| *iw > 0 && *ih > 0)
        .map(|(_, _, iw, ih)| (iw as f32, ih as f32))
        .or_else(|| match (attr_px("width"), attr_px("height")) {
            (Some(aw), Some(ah)) if aw > 0.0 && ah > 0.0 => Some((aw, ah)),
            _ => None,
        });
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
    // Quem manda encolher um replaced é `max-width`/`min-width` — NÃO a largura
    // do contentor (CSS 2.1 §10.4). O que aqui estava era um corte por `avail_w`:
    // um `<img width=100>` dentro de um `<div style='width:50px'>` saía 50x50, e
    // o Chrome dá 100x101 e deixa TRANSBORDAR. A alternativa (manter o corte
    // "para não estourar a linha") é o que fechava um ciclo dentro de tabelas —
    // a imagem encolhia porque a célula era estreita, e a célula era estreita
    // porque o mínimo da imagem tinha encolhido — e na Wikipédia levava 100px a
    // valerem 3, com 545px de deslocamento a jusante.
    //
    // A razão de aspecto só se preserva quando a outra dimensão é `auto`: com
    // `width` e `height` ambos declarados, o CSS não reescala o que o autor fixou.
    let w_auto = w0.is_none();
    let h_auto = h0.is_none();
    let dim = |d: Option<crate::style::Dimension>| d.and_then(|d| d.resolve(&resolve));
    if let Some(mx) = dim(css.max_width).filter(|mx| w > *mx) {
        if h_auto && w > 0.0 {
            h = h * mx / w;
        }
        w = mx;
    }
    if let Some(mn) = dim(css.min_width).filter(|mn| w < *mn) {
        if h_auto && w > 0.0 {
            h = h * mn / w;
        }
        w = mn;
    }
    if let Some(mx) = dim(css.max_height).filter(|mx| h > *mx) {
        if w_auto && h > 0.0 {
            w = w * mx / h;
        }
        h = mx;
    }
    if let Some(mn) = dim(css.min_height).filter(|mn| h < *mn) {
        if w_auto && h > 0.0 {
            w = w * mn / h;
        }
        h = mn;
    }
    // A caixa de um replaced é a BORDER-BOX, que é o que `getBoundingClientRect`
    // devolve — e os clamps acima são sobre a content box (`box-sizing` inicial é
    // `content-box`), por isso a borda entra só aqui, depois deles.
    //
    // Eram os 2px que sobravam em cada miniatura da Wikipédia depois de a base da
    // percentagem ser corrigida: `.mw-file-element{border:1px solid}` dá 250 de
    // conteúdo e 252 de caixa, e nós parávamos nos 250. A alternativa — somar a
    // borda no chamador — espalhava a regra por três sítios de chamada, e é este
    // o único que sabe o que é conteúdo e o que é caixa.
    let bordas = crate::style::borders::resolved_sides(css);
    let px = |b: crate::style::borders::SideBorder| if b.paints() { b.width } else { 0.0 };
    let (bt, br, bb, bl) = (px(bordas[0]), px(bordas[1]), px(bordas[2]), px(bordas[3]));
    Some((w.max(0.0) + bl + br, h.max(0.0) + bt + bb))
}
