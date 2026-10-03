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
            crate::style::stylesheet::MediaQuery::parse(m).matches(&dom.media_context_at_width(viewport_w))
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
///
/// `forced`: o tamanho de CONTEÚDO já decidido pelo FLEX (grow/shrink no
/// eixo principal, `align-items: stretch` no cruzado) para este item — vence
/// `width`/`height` do CSS/atributo do MESMO jeito que já vence num bloco
/// comum (`bloco.rs`, `forced_outer_w`/`forced_outer_h`), e é o que faz um
/// `<img>` sem `height` mas esticado no eixo cruzado continuar quadrado: sem
/// razão a preservar (`w0`/`h0` ambos `None` aqui), a MESMA fórmula da razão
/// do atributo HTML (`(Some(w), None) => w * nh/nw`) deriva a outra dimensão
/// a partir da que o flex já forçou, em vez do tamanho natural dos pixels.
/// `(None, None)` — nem CSS/atributo nem flex — mantém o caminho de sempre.
pub(crate) fn replaced_inline_size(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    avail_w: f32,
    // The containing block's content height when it is DEFINITE — the basis
    // of `height`/`min-height`/`max-height` percentages. `None` makes them
    // `auto`/`none`, as an indefinite basis does in the browser.
    cb_h: Option<f32>,
    forced: (Option<f32>, Option<f32>),
    ctx: &LayoutCtx,
) -> Option<(f32, f32)> {
    let crate::dom::NodeKind::Element { tag } = &dom.node(id).kind else {
        return None;
    };
    // `<svg>` não está aqui de propósito: `is_block_level` ainda o manda para o
    // caminho de bloco, que o pinta. Duplicar a decisão era criar um segundo
    // sítio onde o tamanho de um replaced se decide.
    //
    // `<canvas>` ESTÁ, e é o que o tira do caminho de bloco: ele é inline por
    // natureza como o `<img>`, e forçá-lo a bloco empilhava dois canvas
    // irmãos um sobre o outro onde o Blink os põe lado a lado, e perdia a
    // borda (15×10 onde o Blink dá 17×12). 300×150 é o default do HTML para
    // um canvas sem dimensão nenhuma — ao contrário do `<img>` sem `src`,
    // aqui não há recurso que possa chegar depois e desmenti-lo.
    // O default de CSS Images §5 quando não há intrínseco NENHUM (nem
    // dimensão, nem razão) é 300×150 para QUALQUER replaced — mas só um
    // `<img>` com `src` (mesmo que o recurso não decodifique dimensão
    // nenhuma, como um `data:image/svg+xml` sem `width`/`height`/`viewBox`:
    // `flex-svg-no-intrinsic-column-001`, WPT) o recebe; `<img>` SEM `src`
    // continua em `(0,0)` — `imagem_sem_dimensao_nenhuma_continua_sem_caixa`
    // fixa exactamente essa distinção ("a caixa vem do que se DECLARA, não
    // de o elemento ser um `<img>`") e teria uma caixa inventada sem este
    // corte. "video"/"iframe"/"embed"/"object" sempre tiveram o default,
    // com ou sem `src`/`srcdoc` — nenhum teste os condiciona a isso.
    let has_src = dom.node(id).attr("src").is_some_and(|s| !s.is_empty());
    let default_box = match tag.as_str() {
        "img" if has_src => Some((300.0, 150.0)),
        "img" => None,
        "canvas" | "video" | "iframe" | "embed" | "object" => Some((300.0, 150.0)),
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
    // Inside a rotated frame (`layout/block/rotated.rs`) the styles read
    // `width` as the inline size, the page's HEIGHT; the element's own
    // dimensions — its attributes and its pixels — are physical, so they are
    // read crosswise to agree. Outside a frame nothing is swapped.
    let rotated = crate::layout::in_rotated_frame();
    let attr_px = |name: &str| -> Option<f32> {
        let name = match (rotated, name) {
            (true, "width") => "height",
            (true, "height") => "width",
            _ => name,
        };
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
    // Uma percentagem de LARGURA contra uma base INDEFINIDA computa a `auto`,
    // e é a mesma regra que a altura logo abaixo já aplicava por não ter base
    // nenhuma. A base é indefinida sempre que se mede em max-content: um item
    // shrink-to-fit mede o conteúdo com `avail_w` infinito, e `100%` de
    // infinito é infinito — a borda do item saía com `w: inf` e o
    // rasterizador ficava 65 segundos a percorrê-la
    // (`intrinsic-percent-replaced-019`, WPT). Sem base, o tamanho vem da
    // razão de aspecto ou do intrínseco, que é o que o Blink usa ali.
    let base_de_percentagem_definida = avail_w.is_finite();
    // A `<canvas>`'s `width`/`height` attributes are its INTRINSIC size, not
    // presentational hints (HTML §4.12.5): `<canvas width=400 height=400
    // style="height:100%">` in a 200px block is 200×200 in Blink, the width
    // following the ratio, where reading the attribute as `width:400px` gave
    // 400×200 (`percentage-heights-022`, WPT). On an `<img>` they ARE hints.
    let is_canvas = tag == "canvas";
    let hint_px = |attr: &str| if is_canvas { None } else { attr_px(attr) };
    let declarado = |d: Option<crate::style::Dimension>, attr: &str| match d {
        Some(crate::style::Dimension::Auto) => None,
        Some(crate::style::Dimension::Percent(_)) if !base_de_percentagem_definida => None,
        Some(d) => d.resolve(&resolve),
        None => hint_px(attr),
    };
    // A HEIGHT percentage resolves against the containing block's HEIGHT
    // when that height is definite, and computes to `auto` otherwise (CSS 2.1
    // §10.5). `Dimension::resolve` would use `parent_content_w` — the WIDTH —
    // for any percentage, which is why the vertical axis goes through
    // `resolve_height` with `cb_h`, the same rule `layout_block` applies to a
    // non-replaced box. `cb_h` is `None` wherever the caller measures rather
    // than lays out (intrinsic widths, table columns): there the basis is
    // indefinite by definition (`height-percentage-005`, WPT: an `<img
    // height:100%>` in an auto-height `<div>` is its natural square).
    let vertical = |d: Option<crate::style::Dimension>| match d {
        Some(crate::style::Dimension::Percent(_)) => crate::layout::resolve_height(d, cb_h, &resolve),
        Some(crate::style::Dimension::Calc(c)) if c.pct != 0.0 => crate::layout::resolve_height(d, cb_h, &resolve),
        d => d.and_then(|d| d.resolve(&resolve)),
    };
    let declarado_altura = |d: Option<crate::style::Dimension>, attr: &str| match d {
        Some(crate::style::Dimension::Auto) => None,
        Some(d) => vertical(Some(d)),
        None => hint_px(attr),
    };
    // O flex vence o CSS do mesmo jeito que já vence num bloco comum — é
    // por isso que entra ANTES de `declarado`, não depois: um `<img>` com
    // `width` declarado mas encolhido pelo `flex-shrink` tem de acabar na
    // largura que o flex decidiu, não na declarada.
    let w0 = forced.0.or_else(|| declarado(css.width, "width"));
    let h0 = forced.1.or_else(|| declarado_altura(css.height, "height"));
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
    //
    // `ratio_do_replaced` (mesmo ficheiro, abaixo): a resolução dos atributos
    // pelo NÓ (`node`, já ajustado ao `<picture>`) é a mesma que essa função
    // repete no `id` cru para quem não passa por `<picture>` — a `<img>` de um
    // `<picture>` nunca precisa da razão como candidato de min-content (o
    // `<source>` já lhe deu as duas dimensões), então a diferença não importa.
    let ratio = dom
        .image_dims(id)
        .map(|(iw, ih)| if rotated { (ih as f32, iw as f32) } else { (iw as f32, ih as f32) })
        .or_else(|| match (attr_px("width"), attr_px("height")) {
            (Some(aw), Some(ah)) if aw > 0.0 && ah > 0.0 => Some((aw, ah)),
            // A canvas always has an intrinsic size: a missing attribute is
            // its default, 300 wide or 150 tall.
            (aw, ah) if is_canvas => {
                let (dw, dh) = if rotated { (150.0, 300.0) } else { (300.0, 150.0) };
                Some((aw.unwrap_or(dw), ah.unwrap_or(dh))).filter(|(w, h)| *w > 0.0 && *h > 0.0)
            }
            _ => None,
        });
    let natural = match (ratio, default_box) {
        (Some(r), _) => r,
        (None, Some(d)) => d,
        (None, None) => (0.0, 0.0),
    };
    // `min-`/`max-` bind the replaced element, not the container's width
    // (CSS 2.1 §10.4): an `<img width=100>` in a 50px `<div>` overflows it, as
    // Chrome does — cutting by `avail_w` closed a loop inside tables where the
    // image shrank because the cell was narrow and the cell was narrow because
    // the image had shrunk. The ORDER of the clamps against the ratio transfer
    // is `replaced_clamp`'s, and why it is its own module is written there.
    let dim = |d: Option<crate::style::Dimension>| d.and_then(|d| d.resolve(&resolve));
    let lw = super::replaced_clamp::AxisLimits::new(dim(css.min_width), dim(css.max_width));
    let lh = super::replaced_clamp::AxisLimits::new(vertical(css.min_height), vertical(css.max_height));
    let (w, h) = super::replaced_clamp::clamp_replaced(w0, h0, ratio, natural, lw, lh);
    // A caixa de um replaced é a BORDER-BOX, que é o que `getBoundingClientRect`
    // devolve — e os clamps acima são sobre a content box (`box-sizing` inicial é
    // `content-box`), por isso a borda (e o padding, ver abaixo) entram só aqui,
    // depois deles.
    //
    // Eram os 2px que sobravam em cada miniatura da Wikipédia depois de a base da
    // percentagem ser corrigida: `.mw-file-element{border:1px solid}` dá 250 de
    // conteúdo e 252 de caixa, e nós parávamos nos 250. A alternativa — somar a
    // borda no chamador — espalhava a regra por três sítios de chamada, e é este
    // o único que sabe o que é conteúdo e o que é caixa.
    let bordas = crate::style::borders::resolved_sides(css);
    let px = |b: crate::style::borders::SideBorder| if b.paints() { b.width } else { 0.0 };
    let (bt, br, bb, bl) = (px(bordas[0]), px(bordas[1]), px(bordas[2]), px(bordas[3]));
    // Padding: era o cut "v1" citado no comentário deste ficheiro
    // (`caixa de um replaced é a BORDER-BOX ... o padding NÃO entrava"),
    // que `flex-aspect-ratio-intrinsic-padding-001` (WPT) expõe: um `<img>`
    // com `padding:20px` numa coluna flex deve medir a ALTURA pela razão
    // do CONTENT-BOX (200×100 → 100), e só DEPOIS somar o padding (a caixa
    // final é 240×140) — não a razão do border-box. `w`/`h` acima já são
    // conteúdo (o `declarado`/`forced` que os produz nunca inclui padding:
    // `layout_image` subtrai padding do `forced_outer_*`, como já fazia
    // para margem/borda), por isso somar aqui é o mesmo padrão da borda.
    let p = &css.padding;
    let pt = p.top.resolve(&resolve).unwrap_or(0.0);
    let pr = p.right.resolve(&resolve).unwrap_or(0.0);
    let pb = p.bottom.resolve(&resolve).unwrap_or(0.0);
    let pl = p.left.resolve(&resolve).unwrap_or(0.0);
    Some((w.max(0.0) + bl + br + pl + pr, h.max(0.0) + bt + bb + pt + pb))
}

/// A razão de aspeto de um replaced (`id` cru, sem resolver `<picture>`): a
/// dos pixels decodificados, senão a dos atributos HTML `width`/`height`
/// juntos — a MESMA pergunta que [`replaced_inline_size`] já faz para
/// derivar a dimensão em falta. `pub(crate)` para o candidato (d) do
/// `min-width`/`min-height: auto` (Flexbox §4.5) em
/// `table::widths::min_content`: com razão de aspeto e a OUTRA dimensão já
/// definite, o piso automático é a largura/altura DERIVADA da razão, mesmo
/// quando a dimensão do próprio eixo TAMBÉM está declarada — caso em que
/// `replaced_inline_size` (pedido para a caixa REAL) honra a declaração e
/// nunca chega a olhar para a razão (`flexbox-min-width-auto-002`/
/// `-min-height-auto-002`, WPT: mediam a largura/altura DECLARADA, não a
/// derivada — o piso nunca capava o item ao encolher).
pub(crate) fn ratio_do_replaced(dom: &Dom, id: NodeIdx) -> Option<(f32, f32)> {
    let node = dom.node(id);
    let attr_px = |name: &str| -> Option<f32> {
        node.attr(name)
            .and_then(|v| v.trim().trim_end_matches("px").trim().parse::<f32>().ok())
            .filter(|n| *n >= 0.0)
    };
    dom.image_dims(id)
        .map(|(iw, ih)| (iw as f32, ih as f32))
        .or_else(|| match (attr_px("width"), attr_px("height")) {
            (Some(aw), Some(ah)) if aw > 0.0 && ah > 0.0 => Some((aw, ah)),
            _ => None,
        })
}

/// O candidato (d) do `min-width: auto` (Flexbox §4.5) para um replaced com
/// razão: a largura DERIVADA da razão pela altura USADA (CSS2 §10.7 —
/// `height`, senão `min-height` como aproximação, clampada por
/// `max-height`/`min-height`) — `None` sem razão nem altura para
/// constranger. Extraído para `table::widths::min_content` (no tecto de 500
/// linhas) não repetir o clamp; ver [`ratio_do_replaced`] para o porquê.
pub(crate) fn largura_min_content_por_razao(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    resolve: &ResolveCtx,
) -> Option<f32> {
    let h = css
        .height
        .and_then(|d| d.resolve(resolve))
        .or_else(|| css.min_height.and_then(|d| d.resolve(resolve)))
        .map(|h| {
            let h = css.max_height.and_then(|d| d.resolve(resolve)).map_or(h, |mx| h.min(mx));
            css.min_height.and_then(|d| d.resolve(resolve)).map_or(h, |mn| h.max(mn))
        })
        .filter(|h| *h > 0.0)?;
    let (nw, nh) = ratio_do_replaced(dom, id)?;
    Some(h * nw / nh)
}

/// O espelho de [`largura_min_content_por_razao`] para o candidato (d) do
/// `min-height: auto` no eixo de COLUNA (Flexbox §4.5): a altura DERIVADA da
/// razão pela largura USADA (`width`, senão `min-width`, clampada por
/// `max-width`/`min-width`) — `None` sem razão nem largura para constranger.
pub(crate) fn altura_min_content_por_razao(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    resolve: &ResolveCtx,
) -> Option<f32> {
    let w = css
        .width
        .and_then(|d| d.resolve(resolve))
        .or_else(|| css.min_width.and_then(|d| d.resolve(resolve)))
        .map(|w| {
            let w = css.max_width.and_then(|d| d.resolve(resolve)).map_or(w, |mx| w.min(mx));
            css.min_width.and_then(|d| d.resolve(resolve)).map_or(w, |mn| w.max(mn))
        })
        .filter(|w| *w > 0.0)?;
    let (nw, nh) = ratio_do_replaced(dom, id)?;
    Some(w * nh / nw)
}
