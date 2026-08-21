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
        || css.padding.any_set()
        || css.border_width.is_some()
        || css.border_widths.any_set()
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
