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
mod tests;
#[cfg(test)]
mod quebra_de_linha;
#[cfg(test)]
mod espaco_que_nao_colapsa;
#[cfg(test)]
mod sonda_ul;
#[cfg(test)]
mod inline_declarado_e_dono;
