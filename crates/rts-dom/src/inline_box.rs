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

/// Este carácter é WHITESPACE para o CSS?
///
/// São cinco e só cinco (CSS Text §3, "white space characters"): espaço, tab,
/// LF, CR e FF. `char::is_whitespace` — que é o que o fluxo inline usava em
/// todos os sítios — responde pela propriedade Unicode `White_Space`, e essa
/// inclui o NBSP (U+00A0). O NBSP é exatamente o carácter que a spec manda NÃO
/// colapsar e NÃO oferecer como oportunidade de quebra: colapsá-lo apaga o seu
/// avanço (4,45px a 14,4px de fonte, medido no Chrome) e desloca para a
/// esquerda tudo o que vem depois dele na linha.
///
/// A alternativa rejeitada era perguntar `!c.is_whitespace() || c == NBSP` em
/// cada chamada. Isso é a mesma regra escrita nove vezes dentro de um ficheiro
/// — e foi tê-la escrita uma vez por sítio (como `char::is_whitespace`) que
/// produziu o defeito. Uma pergunta, uma resposta.
pub(crate) fn e_espaco_css(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{000C}')
}

/// O texto sem whitespace CSS nas duas pontas — o `trim` desta regra.
pub(crate) fn apara_css(s: &str) -> &str {
    s.trim_matches(e_espaco_css)
}

/// Este run é SÓ separador? Um run de um NBSP não é, e é essa a diferença que
/// lhe devolve a caixa: quem responde `true` aqui não abre peça nenhuma no
/// aglomerado, logo não fica dono de nada, logo não recebe retângulo.
pub(crate) fn so_espaco_css(s: &str) -> bool {
    s.chars().all(e_espaco_css)
}

/// As palavras separadas por whitespace CSS — o `split_whitespace` desta regra.
pub(crate) fn palavras_css(s: &str) -> impl Iterator<Item = &str> {
    s.split(e_espaco_css).filter(|w| !w.is_empty())
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
    css.line_height
        .map(|l| l.resolve(font_size, normal))
        .unwrap_or(normal)
}

/// Este `Edges` OCUPA ESPAÇO — algum lado resolve a um valor diferente de zero?
///
/// Não é a mesma pergunta que `any_set`, e a diferença é o que dava 752px de
/// largura a cada um dos 51 cabeçalhos da Wikipédia: `.mw-heading h3` declara
/// `display:inline;border:0;margin:0;padding:0`, e o `padding:0` contava como
/// "há padding" — devolvendo ao caminho de bloco o elemento que o `display`
/// acabou de tirar de lá. Um zero declarado não ocupa nem pinta; declará-lo é a
/// forma normal de um reset dizer que NÃO quer padding.
///
/// Uma percentagem responde `true` sem se resolver: aqui não há largura de
/// contentor, e `padding:5%` ocupa espaço em qualquer contentor que não seja
/// vazio. O erro conservador é o certo — tratá-la como zero tirava a caixa a
/// quem tem padding a sério.
fn ocupa_espaco(e: &crate::style::Edges) -> bool {
    use crate::style::{Dimension, Side};
    [e.top, e.right, e.bottom, e.left].iter().any(|s| match s {
        // `auto` não ocupa por si: num padding não existe, e numa margem é o
        // espaço livre que sobra — não um comprimento que crie caixa.
        Side::Unset | Side::Auto => false,
        Side::Len(d) => !matches!(
            d,
            Dimension::Px(v) | Dimension::Percent(v) | Dimension::Em(v)
                | Dimension::Rem(v) | Dimension::Vw(v) | Dimension::Vh(v)
            if *v == 0.0
        ),
    })
}

/// A mesma pergunta para a BORDA, que é `f32` uniforme mais os quatro lados.
///
/// O estilo (`border-style:none` não pinta, mesmo com largura) deliberadamente
/// NÃO entra aqui: é outra pergunta, com outro consumidor
/// (`SideBorder::paints`), e respondê-la neste sítio alargava um lote que existe
/// para corrigir a confusão entre "declarado" e "ocupa espaço".
fn borda_ocupa_espaco(css: &ComputedStyle) -> bool {
    css.border_width.is_some_and(|w| w > 0.0) || ocupa_espaco(&css.border_widths)
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
        || ocupa_espaco(&css.padding)
        || ocupa_espaco(&css.margin)
        || borda_ocupa_espaco(css)
        || css.width.is_some()
        || css.height.is_some()
}

/// A mesma pergunta que [`cria_caixa_de_bloco`], mas para um elemento cujo
/// `display:inline` é DECLARADO — onde a resposta é diferente por causa do CSS
/// e não por conveniência.
///
/// Numa caixa inline a margem vertical, a `width` e a `height` NÃO SE APLICAM
/// (CSS 2.1 §10.3.1/§10.6.1). Perguntar `cria_caixa_de_bloco` a um
/// `<h3 style="display:inline">` respondia `true` pela margem que a
/// UA-stylesheet do `<h3>` lhe põe — uma propriedade que o próprio `display`
/// acabou de tornar inoperante — e devolvia-o ao caminho de bloco, que é
/// exatamente o que a declaração pedia para evitar.
///
/// A alternativa rejeitada era tirar a margem de `cria_caixa_de_bloco`: lá ela
/// está certa, porque o chamador é um elemento que ainda pode ser de bloco.
/// São duas perguntas, e o que as separa é o `display`.
///
/// O que fica são as propriedades que PINTAM uma superfície ou ocupam espaço na
/// horizontal, e essas continuam a exigir `layout_block` para serem pintadas.
pub(crate) fn cria_caixa_apesar_de_inline(css: &ComputedStyle) -> bool {
    css.bg.is_some()
        || css.gradient.is_some()
        || css.box_shadow.is_some()
        || ocupa_espaco(&css.padding)
        || borda_ocupa_espaco(css)
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

/// Pode a linha ser partida DENTRO de um aglomerado — isto é, no meio de uma
/// palavra, onde o texto não oferece oportunidade de quebra?
///
/// É a resolução conjunta de `word-break` e `overflow-wrap` (o antigo
/// `word-wrap`, que MDN dá como alias e o parser já mapeia para o mesmo campo).
/// Resolvidas JUNTAS e num só valor porque é uma só pergunta para quem quebra:
/// a alternativa — o `wrap_runs` receber as duas propriedades e voltar a
/// combiná-las em cada aglomerado — punha a mesma regra em dois sítios, que é a
/// duplicação que este motor já pagou várias vezes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum QuebraDentro {
    /// `normal` — nunca. Uma palavra que não cabe transborda.
    Nao,
    /// `overflow-wrap: break-word|anywhere`, `word-break: break-word` — parte-se
    /// só quando a palavra não cabe NEM numa linha inteira e vazia. Descer
    /// primeiro e partir depois é o que o Chrome faz.
    SePreciso,
    /// `word-break: break-all` — parte-se assim que não cabe no que resta da
    /// linha, sem esperar por oportunidade nenhuma.
    Sempre,
}

/// A resolução, e a razão de `word-break` ganhar a `overflow-wrap`: `break-all`
/// é estritamente mais agressivo, e a spec dá-lhe precedência sobre o
/// `overflow-wrap` do mesmo elemento.
///
/// `keep-all` e `auto-phrase` respondem `Nao` — as duas são sobre onde partir
/// texto CJK, e este motor mede por carácter sem análise de escrita. Mapeá-las
/// para `Nao` é o comportamento certo em texto latino (que é todo o corpus) e é
/// honesto no resto: não partir é o que `keep-all` pede.
pub(crate) fn quebra_dentro(css: &ComputedStyle) -> QuebraDentro {
    use crate::style::{OverflowWrap, WordBreak};
    match css.word_break {
        Some(WordBreak::BreakAll) => return QuebraDentro::Sempre,
        // Legado: `word-break: break-word` é, por MDN, o mesmo que
        // `overflow-wrap: break-word`. Aparece 15 vezes no corpus de 13 folhas —
        // mais do que `break-all` — por isso não é um caso de canto.
        Some(WordBreak::BreakWord) => return QuebraDentro::SePreciso,
        _ => {}
    }
    match css.overflow_wrap {
        // `anywhere` difere de `break-word` só no cálculo da largura MÍNIMA
        // intrínseca (`min-content`), que este motor não distingue; na quebra da
        // linha as duas fazem o mesmo, e é isso que aqui se decide.
        Some(OverflowWrap::BreakWord | OverflowWrap::Anywhere) => QuebraDentro::SePreciso,
        _ => QuebraDentro::Nao,
    }
}

/// O maior prefixo de `texto` cuja largura cabe em `disp`, em bytes, com a sua
/// largura medida. `(0, 0.0)` quando nem o primeiro carácter cabe.
///
/// Busca BINÁRIA sobre as fronteiras de carácter: uma varredura acumulativa
/// custava uma medição por carácter e este caminho corre por palavra partida,
/// não por página. Medir cada prefixo do início (em vez de somar larguras de
/// glifos) é o que respeita kerning e ligaduras — a soma dos caracteres não é a
/// largura da palavra em fonte proporcional.
pub(crate) fn prefixo_que_cabe(
    texto: &str,
    disp: f32,
    font_size: f32,
    mono: bool,
    bold: bool,
    italic: bool,
    m: &dyn TextMeasurer,
) -> (usize, f32) {
    if disp <= 0.0 {
        return (0, 0.0);
    }
    // As fronteiras candidatas, excluindo o zero (prefixo vazio nunca é resposta
    // útil) e incluindo o fim (o texto inteiro pode caber).
    let cortes: Vec<usize> = texto
        .char_indices()
        .skip(1)
        .map(|(i, _)| i)
        .chain(std::iter::once(texto.len()))
        .collect();
    let (mut lo, mut hi) = (0usize, cortes.len());
    let mut melhor = (0usize, 0.0f32);
    while lo < hi {
        let meio = (lo + hi) / 2;
        let corte = cortes[meio];
        let w = m.text_width(&texto[..corte], font_size, mono, bold, italic);
        if w <= disp {
            melhor = (corte, w);
            lo = meio + 1;
        } else {
            hi = meio;
        }
    }
    melhor
}

#[cfg(test)]
mod tests {
    use crate::table::tests::{geometria, rect, textos};

    /// A palavra que não cabe TRANSBORDA — `overflow-wrap: normal` é o inicial, e
    /// esta é a metade da prova que fixa o antes.
    #[test]
    fn palavra_longa_sem_overflow_wrap_transborda_o_container() {
        let (dom, list) = geometria(
            "<div style='width:40px'><span>abcdefghijklmnopqrst</span></div>",
            800.0,
        );
        let s = rect(&dom, &list, "span", 0);
        assert!(
            s.w > 40.0,
            "sem overflow-wrap a palavra tem de sair da caixa: {s:?}"
        );
    }

    /// E a MESMA com `overflow-wrap: break-word` fica dentro, em várias linhas.
    #[test]
    fn palavra_longa_com_break_word_parte_e_cabe_no_container() {
        let (dom, list) = geometria(
            "<div style='width:40px;overflow-wrap:break-word'>\
             <span>abcdefghijklmnopqrst</span></div>",
            800.0,
        );
        let s = rect(&dom, &list, "span", 0);
        assert!(s.w <= 41.0, "a palavra partida cabe na caixa: {s:?}");
        assert!(s.h > 20.0, "e ocupa mais do que uma linha: {s:?}");
    }

    /// O nome LEGADO faz o mesmo: `word-wrap` é alias de `overflow-wrap` (MDN), e
    /// é a grafia que 8 das 13 folhas do corpus escrevem.
    #[test]
    fn word_wrap_legado_quebra_como_overflow_wrap() {
        let (dom, list) = geometria(
            "<div style='width:40px;word-wrap:break-word'>\
             <span>abcdefghijklmnopqrst</span></div>",
            800.0,
        );
        let s = rect(&dom, &list, "span", 0);
        assert!(s.w <= 41.0, "o alias legado tem de quebrar igual: {s:?}");
    }

    /// `break-all` parte no meio de uma palavra CURTA — a que caberia sozinha na
    /// linha seguinte e que por isso `break-word` deixaria descer inteira. É a
    /// diferença entre os dois valores, e é o que este teste fixa.
    #[test]
    fn break_all_parte_uma_palavra_que_break_word_deixaria_descer() {
        let estreito = "width:60px;font-size:16px";
        let (d1, l1) = geometria(
            &format!("<div style='{estreito};overflow-wrap:break-word'>aaaa <span>bbbb</span></div>"),
            800.0,
        );
        let (d2, l2) = geometria(
            &format!("<div style='{estreito};word-break:break-all'>aaaa <span>bbbb</span></div>"),
            800.0,
        );
        let com_word = rect(&d1, &l1, "span", 0);
        let com_all = rect(&d2, &l2, "span", 0);
        assert!(
            com_all.h > com_word.h,
            "break-all reparte a palavra em duas linhas onde break-word a desce \
             inteira: all={com_all:?} word={com_word:?}"
        );
    }

    /// `keep-all` NÃO parte: é sobre texto CJK e, em texto latino, o que pede é
    /// exatamente o comportamento inicial.
    #[test]
    fn keep_all_nao_parte_a_palavra() {
        let (dom, list) = geometria(
            "<div style='width:40px;word-break:keep-all'>\
             <span>abcdefghijklmnopqrst</span></div>",
            800.0,
        );
        let s = rect(&dom, &list, "span", 0);
        assert!(s.w > 40.0, "keep-all tem de deixar transbordar: {s:?}");
    }

    /// A elipse só aparece quando as TRÊS condições se juntam: `ellipsis`,
    /// transbordo escondido e linha que não quebra.
    #[test]
    fn elipse_aparece_so_com_ellipsis_overflow_hidden_e_nowrap() {
        let conteudo = "uma frase bastante comprida para nao caber";
        let completo = "width:80px;text-overflow:ellipsis;overflow:hidden;white-space:nowrap";
        let (_d, l) = geometria(&format!("<div style='{completo}'>{conteudo}</div>"), 800.0);
        assert!(
            textos(&l).iter().any(|t| t.ends_with('…')),
            "com as três condições a linha acaba em reticências: {:?}",
            textos(&l)
        );

        // e cada uma em falta desliga-a.
        for faltando in [
            "width:80px;overflow:hidden;white-space:nowrap",
            "width:80px;text-overflow:ellipsis;white-space:nowrap",
            "width:80px;text-overflow:ellipsis;overflow:hidden",
        ] {
            let (_d, l) = geometria(&format!("<div style='{faltando}'>{conteudo}</div>"), 800.0);
            assert!(
                !textos(&l).iter().any(|t| t.contains('…')),
                "sem uma das condições não há elipse ({faltando}): {:?}",
                textos(&l)
            );
        }
    }

    /// E a linha com elipse CABE na caixa: o orçamento tira a largura das
    /// próprias reticências antes de cortar, senão elas ficavam de fora.
    #[test]
    fn a_linha_com_elipse_nao_transborda_a_caixa() {
        let (dom, list) = geometria(
            "<div style='width:80px;text-overflow:ellipsis;overflow:hidden;\
             white-space:nowrap'><span>uma frase bastante comprida para nao caber</span></div>",
            800.0,
        );
        let s = rect(&dom, &list, "span", 0);
        assert!(s.w <= 81.0, "a linha cortada cabe na caixa: {s:?}");
    }

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
        assert!(
            fig.w >= 252.0,
            "a figura encolheu a {} em volta de uma imagem de 252",
            fig.w
        );
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

    /// Uma imagem larga num contentor estreito TRANSBORDA — não encolhe.
    ///
    /// Medido no Chrome: `<div style='width:50px'><img width='100' height='101'>`
    /// dá 100x101, e assim a 500, 100, 50 e 10px de contentor. Encolher era o que
    /// aqui se fazia (50x50 aos 50px, 10x10 aos 10px), e dentro de uma tabela
    /// fechava um ciclo: a célula estreitava porque a imagem encolhia, e a imagem
    /// encolhia porque a célula estreitara.
    #[test]
    fn imagem_larga_em_container_estreito_transborda_como_no_chrome() {
        for largura in [500, 100, 50, 10] {
            let html = format!(
                "<div style='width:{largura}px'><img width='100' height='101'></div>"
            );
            let (dom, list) = geometria(&html, 800.0);
            let img = rect(&dom, &list, "img", 0);
            assert!(
                (img.w - 100.0).abs() < 0.5 && (img.h - 101.0).abs() < 0.5,
                "contentor de {largura}px encolheu a imagem: {img:?}"
            );
        }
    }

    /// E um `max-width` DECLARADO continua a encolher: é ele quem manda encolher
    /// no CSS, e esta é a metade que prova que o corte mudou de sítio em vez de
    /// desaparecer.
    ///
    /// A altura NÃO acompanha aqui, e é o comportamento certo: a razão só se
    /// preserva quando a outra dimensão é `auto`, e este `<img>` declara as duas.
    /// (Sem rede não há pixels, logo não há razão intrínseca para exercitar o
    /// outro ramo — é o que todo este harness mede.)
    #[test]
    fn max_width_declarado_encolhe_a_largura() {
        let (dom, list) = geometria(
            "<div style='width:500px'><img style='max-width:50px' width='100' height='200'></div>",
            800.0,
        );
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 50.0).abs() < 0.5, "largura = {}", img.w);
        assert!((img.h - 200.0).abs() < 0.5, "altura declarada = {}", img.h);
    }

    /// A base de uma PERCENTAGEM é a largura do bloco contentor — a margem do
    /// próprio elemento NÃO entra nela.
    ///
    /// É a receita do `.mw-file-element` da Wikipédia: `margin:3px` e
    /// `max-width:calc(100% - (2 * 3px) - (2 * 1px))` num contentor de 258px. O
    /// `100%` vale 258, o `calc` dá 250, e a imagem de 250 fica intacta. Descontar
    /// as margens antes de resolver — o que `layout_image` fazia — punha o `100%`
    /// a valer 252 e cortava a imagem para 244: 6px exatos, em 30 imagens da
    /// página.
    #[test]
    fn a_percentagem_de_max_width_resolve_contra_o_contentor_e_nao_menos_a_margem() {
        let html = "<div style='width:258px'>            <img style='margin:3px;max-width:calc(100% - (2 * 3px) - (2 * 1px))'              width='250' height='167'></div>";
        let (dom, list) = geometria(html, 800.0);
        let img = rect(&dom, &list, "img", 0);
        assert!(
            (img.w - 250.0).abs() < 0.5,
            "o 100% resolveu contra {} em vez de 258: {img:?}",
            img.w + 8.0
        );
    }

    /// Um `height:auto` DECLARADO vence o atributo `height` do HTML — e a altura
    /// sai da RAZÃO, não do zero.
    ///
    /// Duas regras num teste porque é o par que as torna verdadeiras: o atributo
    /// é um presentational hint e perde para qualquer declaração, mas os dois
    /// atributos juntos continuam a dar a razão de aspecto (HTML, "dimension
    /// attributes"). Tirar o atributo e não pôr a razão dava altura ZERO — a
    /// miniatura da Wikipédia media 252x2, só as bordas.
    #[test]
    fn height_auto_declarado_vence_o_atributo_mas_a_razao_sobrevive() {
        let (dom, list) = geometria(
            "<div style='width:400px'><img style='height:auto' width='250' height='167'></div>",
            800.0,
        );
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 250.0).abs() < 0.5, "largura = {}", img.w);
        assert!(
            (img.h - 167.0).abs() < 0.5,
            "a altura tem de vir da razão 250:167, não do zero: {}",
            img.h
        );
    }

    /// E com `width` declarado MENOR, a razão dos atributos escala a altura: é a
    /// prova de que o 167 acima veio da razão e não de o atributo ter sobrevivido.
    #[test]
    fn a_razao_dos_atributos_escala_a_altura_quando_a_largura_muda() {
        let (dom, list) = geometria(
            "<div style='width:400px'><img style='width:125px;height:auto'              width='250' height='167'></div>",
            800.0,
        );
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 125.0).abs() < 0.5, "largura = {}", img.w);
        assert!(
            (img.h - 83.5).abs() < 0.5,
            "metade da largura é metade da altura: {}",
            img.h
        );
    }

    /// A caixa de um replaced é a BORDER-BOX, que é o que `getBoundingClientRect`
    /// devolve. `.mw-file-element{border:1px solid}` dá 250 de conteúdo e 252 de
    /// caixa — os 2px que sobravam depois de a base da percentagem ser corrigida.
    #[test]
    fn a_borda_entra_na_caixa_do_replaced() {
        let (dom, list) = geometria(
            "<div style='width:400px'><img style='border:1px solid #000'              width='250' height='167'></div>",
            800.0,
        );
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 252.0).abs() < 0.5, "250 + 1 + 1 = {}", img.w);
        assert!((img.h - 169.0).abs() < 0.5, "167 + 1 + 1 = {}", img.h);
    }

    /// Uma largura SEM estilo de borda não ocupa nada: o inicial de
    /// `border-style` é `none`, e sem estilo o Chrome não desenha nem reserva.
    #[test]
    fn largura_de_borda_sem_estilo_nao_entra_na_caixa() {
        let (dom, list) = geometria(
            "<div style='width:400px'><img style='border-width:10px'              width='250' height='167'></div>",
            800.0,
        );
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 250.0).abs() < 0.5, "largura = {}", img.w);
    }

    /// A receita COMPLETA da Wikipédia, medida contra o Chrome: contentor de
    /// 258px, `margin:3px`, `border:1px`, `height:auto` e
    /// `max-width:calc(100% - (2 * 3px) - (2 * 1px))` sobre um `<img>` de 250x167.
    ///
    /// A largura é a do Chrome ao pixel (252). A ALTURA diverge — o Chrome dá 252,
    /// porque offline a imagem nunca carrega e ele cai num quadrado; nós damos 169
    /// pela razão dos atributos, que é o que a spec manda e o que continua certo
    /// no dia em que a imagem chegar. Divergência conhecida, fixada aqui para que
    /// mude por decisão e não por acidente.
    #[test]
    fn a_miniatura_da_wikipedia_mede_a_largura_do_chrome() {
        let html = "<div style='width:258px'><a>            <img style='margin:3px;border:1px solid #000;height:auto;             max-width:calc(100% - (2 * 3px) - (2 * 1px))' width='250' height='167'></a></div>";
        let (dom, list) = geometria(html, 800.0);
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 252.0).abs() < 0.5, "o Chrome dá 252: {}", img.w);
        assert!((img.h - 169.0).abs() < 0.5, "167 + as duas bordas: {}", img.h);
    }

    /// Dentro de um `<picture>`, é o `<source>` escolhido que dá as dimensões — e
    /// a escolha depende do VIEWPORT.
    ///
    /// É o rodapé da Wikipédia, ao pixel: com o viewport de 1280 o
    /// `media="(min-width: 500px)"` casa e a caixa é a do `<source>` (84×29, que
    /// é o que o Chrome mede); com 400 não casa, e responde o `<img>` de
    /// fallback. O mesmo documento, duas respostas, que é o que `<picture>` é.
    #[test]
    fn o_source_escolhido_do_picture_da_as_dimensoes_e_o_viewport_decide() {
        let html = "<picture>            <source media='(min-width: 500px)' srcset='/b.svg' width='84' height='29'>            <img src='/a.svg' width='25' height='25'></picture>";
        let (d1, l1) = geometria(html, 1280.0);
        let largo = rect(&d1, &l1, "img", 0);
        assert!(
            (largo.w - 84.0).abs() < 0.5 && (largo.h - 29.0).abs() < 0.5,
            "o source que casa tem de mandar: {largo:?}"
        );
        let (d2, l2) = geometria(html, 400.0);
        let estreito = rect(&d2, &l2, "img", 0);
        assert!(
            (estreito.w - 25.0).abs() < 0.5 && (estreito.h - 25.0).abs() < 0.5,
            "sem source que case, responde o <img>: {estreito:?}"
        );
    }

    /// Um `media` que o avaliador NÃO SABE ler salta a `<source>` em vez de a
    /// escolher por engano.
    ///
    /// É a honestidade que o `MediaQuery` já tem — uma feature desconhecida torna
    /// a query sempre-falsa — e vale a pena fixá-la aqui: a alternativa (ignorar
    /// o `media` que não se entende) escolhia uma caixa que o browser não teria
    /// escolhido, e o erro ficava silencioso.
    #[test]
    fn um_media_que_nao_sabemos_avaliar_salta_a_source() {
        let html = "<picture>            <source media='(orientation: landscape)' srcset='/b.svg' width='84' height='29'>            <img src='/a.svg' width='25' height='25'></picture>";
        let (dom, list) = geometria(html, 1280.0);
        let img = rect(&dom, &list, "img", 0);
        assert!(
            (img.w - 25.0).abs() < 0.5,
            "uma condição que não entendemos não pode ganhar: {img:?}"
        );
    }

    /// Uma `<source>` SEM `media` casa sempre — é a forma que só oferece outro
    /// formato — e a PRIMEIRA que casa é a que ganha, na ordem do documento.
    #[test]
    fn a_primeira_source_que_casa_ganha_e_sem_media_casa_sempre() {
        let html = "<picture>            <source media='(min-width: 5000px)' srcset='/x.svg' width='10' height='10'>            <source srcset='/b.svg' width='84' height='29'>            <source srcset='/c.svg' width='99' height='99'>            <img src='/a.svg' width='25' height='25'></picture>";
        let (dom, list) = geometria(html, 1280.0);
        let img = rect(&dom, &list, "img", 0);
        assert!(
            (img.w - 84.0).abs() < 0.5,
            "a primeira que casa é a segunda source: {img:?}"
        );
    }

    /// Um `<img>` FORA de um `<picture>` não é afetado: um `<source>` que não é
    /// irmão dele não lhe diz respeito.
    #[test]
    fn uma_source_fora_do_picture_nao_dimensiona_a_imagem() {
        let html = "<div>            <source srcset='/b.svg' width='84' height='29'>            <img src='/a.svg' width='25' height='25'></div>";
        let (dom, list) = geometria(html, 1280.0);
        let img = rect(&dom, &list, "img", 0);
        assert!((img.w - 25.0).abs() < 0.5, "{img:?}");
    }

    /// Um `display:inline` com `padding:0` mede o TEXTO, não o contentor.
    ///
    /// É a receita do `.mw-heading` da Wikipédia, e o `padding:0` era sozinho o
    /// que a partia: um zero declarado contava como "há padding", devolvia o
    /// `<h3>` ao caminho de bloco e ele saía com os 752px do contentor em vez dos
    /// ~185 do seu texto. São 51 cabeçalhos na página, 100% errados, todos assim.
    #[test]
    fn cabecalho_inline_com_padding_zero_mede_o_texto_e_nao_o_contentor() {
        let regra = "<style>.mw-heading h3{display:inline;border:0;margin:0;                     padding:0;color:inherit;font:inherit}</style>";
        let html = format!(
            "{regra}<div style='width:600px' class='mw-heading'><h3>abc</h3></div>"
        );
        let (dom, list) = geometria(&html, 800.0);
        let h3 = rect(&dom, &list, "h3", 0);
        assert!(
            h3.w < 100.0,
            "o cabeçalho tomou a largura do contentor: {h3:?}"
        );
    }

    /// E um padding REAL continua a criar caixa: é o par que prova que a pergunta
    /// mudou de "declarou?" para "ocupa espaço?" em vez de desaparecer.
    #[test]
    fn padding_declarado_com_valor_continua_a_criar_caixa() {
        let zero = "<div style='width:400px'><span style='padding:0'>abc</span></div>";
        let real = "<div style='width:400px'><span style='padding:4px'>abc</span></div>";
        let (d1, l1) = geometria(zero, 800.0);
        let (d2, l2) = geometria(real, 800.0);
        let a = rect(&d1, &l1, "span", 0);
        let b = rect(&d2, &l2, "span", 0);
        assert!(
            (b.w - a.w - 8.0).abs() < 0.5,
            "os 4px de cada lado têm de aparecer: zero={a:?} real={b:?}"
        );
    }

    /// A mesma pergunta para a BORDA, que hoje não disparava por acidente e não
    /// por desenho — os dois lados respondem agora à mesma regra.
    #[test]
    fn borda_zero_nao_cria_caixa_e_borda_real_cria() {
        let zero = "<div style='width:400px'><span style='border:0'>abc</span></div>";
        let real = "<div style='width:400px'><span style='border:2px solid #000'>abc</span></div>";
        let (d1, l1) = geometria(zero, 800.0);
        let (d2, l2) = geometria(real, 800.0);
        let a = rect(&d1, &l1, "span", 0);
        let b = rect(&d2, &l2, "span", 0);
        assert!((a.w - 22.08).abs() < 1.0, "borda zero não ocupa: {a:?}");
        assert!(b.w > a.w, "borda real ocupa: zero={a:?} real={b:?}");
    }

    /// A `hlist` do MediaWiki NÃO regride, e o número é o do Chrome.
    ///
    /// Medido em `chrome_extract.mjs` sobre o CSS verdadeiro: o `<ul>` interior
    /// mede **89,3** — a largura do seu conteúdo — e não os 600 do contentor. O
    /// teste que já existia fixava só `w > 0`, o que passa nos dois mundos; este
    /// fixa o valor, que é o que separa "tem caixa" de "tem a caixa certa".
    /// (88,3 aqui contra 89,3 lá é a métrica aproximada do medidor de teste.)
    #[test]
    fn a_hlist_do_mediawiki_mede_o_conteudo_e_nao_o_contentor() {
        let html = "<style>.hlist ul{margin:0;padding:0}            .hlist li{margin:0;display:inline}.hlist ul ul{display:inline}</style>            <div style='width:600px'><div class='hlist'><ul><li>alfa            <ul><li>bravo</li><li>charlie</li></ul></li><li>delta</li></ul></div></div>";
        let (dom, list) = geometria(html, 800.0);
        let ul = rect(&dom, &list, "ul", 1);
        assert!(
            ul.w > 50.0 && ul.w < 150.0,
            "o ul interior é a largura do seu conteúdo (~89 no Chrome): {ul:?}"
        );
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
        let (dom, list) = geometria(
            "<p style='width:60px'>aaaaa <sup><i>y</i><b>z</b></sup></p>",
            800.0,
        );
        let i = rect(&dom, &list, "i", 0);
        let b = rect(&dom, &list, "b", 0);
        let sup = rect(&dom, &list, "sup", 0);
        assert_eq!(i.y, b.y, "o aglomerado não é partido: i={i:?} b={b:?}");
        assert!(
            i.x < b.x,
            "e mantém a ordem na mesma linha: i={i:?} b={b:?}"
        );
        assert!(
            sup.w < 30.0,
            "a caixa do pai é a do aglomerado, não a da linha: {sup:?}"
        );
    }
}



#[cfg(test)]
mod espaco_que_nao_colapsa {
    //! O NBSP e o inline sem texto visivel: dois defeitos com o mesmo sintoma
    //! (o elemento fica sem caixa) e causas diferentes, por isso testados
    //! separados.
    //!
    //! Todos medem com o `ApproxMeasurer` — cada caractere vale
    //! `tamanho * 0.46` — para que as larguras sejam previsiveis sem falar
    //! sobre uma fonte real.

    use crate::table::tests::{geometria, rect, textos};

    /// Uma caixa que pode nao existir: e a diferenca entre "sem geometria" e
    /// "geometria de largura zero", e os dois casos aparecem aqui.
    fn caixa(html: &str, sel: &str, n: usize) -> Option<crate::layout::Rect> {
        let (dom, list) = geometria(html, 800.0);
        let id = *dom.query_all(sel).get(n)?;
        let idx = dom.resolve(id)?;
        list.geometry_now().rects.get(&idx).copied()
    }

    /// O NBSP OCUPA largura; o espaco normal no mesmo sitio nao ocupa nenhuma.
    ///
    /// E o par que separa os dois caracteres: um `<span>&nbsp;</span>` e
    /// conteudo — o Chrome da-lhe 4,45px a 14,4px de fonte — e um
    /// `<span> </span>` entre duas palavras e o separador que elas ja tinham,
    /// portanto colapsa e o span fica com largura zero.
    #[test]
    fn nbsp_ocupa_largura_e_o_espaco_normal_colapsa() {
        let com_nbsp = caixa("<p>aa <span>&nbsp;</span> bb</p>", "span", 0);
        let com_espaco = caixa("<p>aa <span> </span> bb</p>", "span", 0);
        assert!(
            com_nbsp.is_some_and(|r| r.w > 0.0),
            "o NBSP tem avanco proprio: {com_nbsp:?}"
        );
        assert!(
            com_espaco.is_none_or(|r| r.w == 0.0),
            "o espaco normal entre duas palavras colapsa: {com_espaco:?}"
        );
    }

    /// E o NBSP e PINTADO, em vez de desaparecer no colapso.
    ///
    /// A prova de que nao e so a caixa que voltou: o caractere continua no
    /// texto que vai para o ecra, que e o que faz a linha ter a largura certa.
    #[test]
    fn o_nbsp_sobrevive_ao_texto_pintado() {
        let (_d, list) = geometria("<p>aa<span>&nbsp;</span>bb</p>", 800.0);
        assert!(
            textos(&list).iter().any(|t| t.contains('\u{00A0}')),
            "o NBSP foi engolido pela normalizacao: {:?}",
            textos(&list)
        );
    }

    /// O NBSP NAO e oportunidade de quebra — e a razao de existir.
    ///
    /// Com espaco normal as duas palavras separam-se e a linha parte em duas;
    /// com NBSP nao ha onde partir e a linha transborda inteira, que e o que o
    /// browser faz. A caixa de 60px cabe "aaaa" (29,4) mas nao "aaaa bbbb"
    /// (66,2).
    #[test]
    fn nbsp_nao_oferece_oportunidade_de_quebra() {
        let estreito = "width:60px;font-size:16px";
        let colado = caixa(
            &format!("<p style='{estreito}'><span>aaaa&nbsp;bbbb</span></p>"),
            "span",
            0,
        )
        .expect("o span colado tem caixa");
        let separado = caixa(
            &format!("<p style='{estreito}'><span>aaaa bbbb</span></p>"),
            "span",
            0,
        )
        .expect("o span separado tem caixa");
        assert!(
            colado.h < separado.h,
            "o NBSP tem de manter uma linha so: colado={colado:?} separado={separado:?}"
        );
        assert!(
            colado.w > 60.0,
            "e transborda em vez de partir: {colado:?}"
        );
    }

    /// O avanco do NBSP DESLOCA o que vem a seguir na linha.
    ///
    /// E o erro visivel do colapso, e o que o distingue de uma caixa em falta:
    /// perder o NBSP nao apaga so o span dele, encosta a esquerda tudo o que
    /// vem depois na mesma linha.
    #[test]
    fn o_avanco_do_nbsp_empurra_o_vizinho_da_linha() {
        let com = caixa("<p>a<span>&nbsp;</span><b>b</b></p>", "b", 0)
            .expect("o vizinho tem caixa");
        let sem = caixa("<p>a<span></span><b>b</b></p>", "b", 0)
            .expect("o vizinho tem caixa");
        assert!(
            com.x > sem.x,
            "o NBSP tem de empurrar o vizinho: com={com:?} sem={sem:?}"
        );
    }

    /// Um inline cujo conteudo TODO e `display:none` tem caixa de largura zero.
    ///
    /// E a forma do COinS da Wikipedia — `<span class="Z3988">` com um unico
    /// filho escondido, ~280 por pagina. Pela spec o pai continua a gerar uma
    /// caixa inline: existe, esta na posicao corrente da linha, e nao tem
    /// largura porque nao tem conteudo que a de.
    #[test]
    fn inline_com_todo_o_conteudo_escondido_tem_caixa_de_largura_zero() {
        let r = caixa(
            "<p>aa <span class='z'><span style='display:none'>Z39.88</span></span> bb</p>",
            "span",
            0,
        );
        let r = r.expect("o inline escondido continua a ter caixa");
        assert_eq!(r.w, 0.0, "o conteudo escondido nao da largura: {r:?}");
        assert!(r.h > 0.0, "mas a caixa tem a altura da linha: {r:?}");
    }

    /// E o texto escondido NAO e pintado.
    ///
    /// A metade que prova que a largura zero veio de o conteudo ser saltado, e
    /// nao de ele ter sido medido a zero: antes deste par, os metadados de
    /// citacao apareciam escritos no meio do paragrafo.
    #[test]
    fn o_texto_de_um_display_none_nao_entra_na_linha() {
        let (_d, list) = geometria(
            "<p>aa <span><span style='display:none'>Z39.88</span></span> bb</p>",
            800.0,
        );
        assert!(
            !textos(&list).iter().any(|t| t.contains("Z39.88")),
            "texto escondido pintado na linha: {:?}",
            textos(&list)
        );
    }

    /// Um inline VAZIO ja tinha caixa, e continua a ter: o `Marker` e a
    /// resposta que os dois casos acima passaram a partilhar em vez de
    /// duplicar.
    #[test]
    fn inline_vazio_continua_a_ter_caixa_de_largura_zero() {
        let (dom, list) = geometria("<p>aa <span></span> bb</p>", 800.0);
        let r = rect(&dom, &list, "span", 0);
        assert_eq!(r.w, 0.0, "{r:?}");
        assert!(r.h > 0.0, "{r:?}");
    }
}

#[cfg(test)]
mod sonda_ul {
    use crate::table::tests::geometria;
    fn r(html: &str, sel: &str, n: usize) -> Option<crate::layout::Rect> {
        let (dom, list) = geometria(html, 800.0);
        let id = *dom.query_all(sel).get(n)?;
        let idx = dom.resolve(id)?;
        list.geometry_now().rects.get(&idx).copied()
    }
    #[test]
    fn sonda() {
        println!("ul aninhada     = {:?}", r("<ul><li>a<ul><li>b</li></ul></li></ul>", "ul", 1));
        println!("ul vazia        = {:?}", r("<ul><li>a<ul></ul></li></ul>", "ul", 1));
        println!("ul em li inline = {:?}", r("<li style='display:inline'>a<ul><li>b</li></ul></li>", "ul", 0));
        println!("navbox          = {:?}", r("<div><ul><li><ul><li><a>x</a></li></ul></li></ul></div>", "ul", 1));
        println!("ul so espacos   = {:?}", r("<ul><li>a<ul>   </ul></li></ul>", "ul", 1));
    }
}






#[cfg(test)]
mod inline_declarado_e_dono {
    //! Um elemento cujo `display:inline` e DECLARADO continua a ser dono dos
    //! seus fragmentos mesmo quando declara padding.
    //!
    //! A forma e a `hlist` do MediaWiki — `.hlist ul{padding:0}` sobre
    //! `.hlist ul ul{display:inline}` — e o que a disparava era o `padding`
    //! valer como "declarado" independentemente de ser zero.

    use crate::table::tests::geometria;

    fn caixa(html: &str, sel: &str, n: usize) -> Option<crate::layout::Rect> {
        let (dom, list) = geometria(html, 800.0);
        let id = *dom.query_all(sel).get(n)?;
        let idx = dom.resolve(id)?;
        list.geometry_now().rects.get(&idx).copied()
    }

    /// O `padding` declarado num inline NAO lhe tira a caixa — nem quando e
    /// zero, que era a forma que a pagina real usava.
    #[test]
    fn ul_inline_com_padding_declarado_continua_a_ter_caixa() {
        let regras = "<style>.h li{display:inline}.h ul ul{display:inline}</style>";
        let lista = "<div class='h'><ul><li>a<ul><li>b</li><li>c</li></ul></li></ul></div>";
        let sem_padding = caixa(&format!("{regras}{lista}"), "ul", 1);
        assert!(
            sem_padding.is_some_and(|r| r.w > 0.0),
            "sem padding a caixa ja existia: {sem_padding:?}"
        );
        for padding in ["padding:0", "padding:2px 0"] {
            let com = caixa(
                &format!("{regras}<style>.h ul{{{padding}}}</style>{lista}"),
                "ul",
                1,
            );
            assert!(
                com.is_some_and(|r| r.w > 0.0),
                "`{padding}` tirou a caixa ao inline: {com:?}"
            );
        }
    }

    /// E a caixa que ele ganha e a UNIAO dos filhos, nao uma caixa inventada:
    /// contem os `li` que ja estavam certos.
    #[test]
    fn a_caixa_do_inline_contem_os_fragmentos_dos_filhos() {
        let html = "<style>.h li{display:inline}.h ul ul{display:inline}\
                    .h ul{padding:0}</style>\
                    <div class='h'><ul><li>a<ul><li>bbbb</li><li>cccc</li></ul></li></ul></div>";
        let ul = caixa(html, "ul", 1).expect("o ul interior tem caixa");
        let li = caixa(html, "li", 1).expect("o li neto tem caixa");
        assert!(
            ul.x <= li.x + 0.01 && ul.x + ul.w >= li.x + li.w - 0.01,
            "a caixa do ul tem de conter o li: ul={ul:?} li={li:?}"
        );
    }
}





