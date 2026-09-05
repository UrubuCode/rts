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

mod substituido;
pub(crate) use self::substituido::{
    altura_min_content_por_razao, largura_min_content_por_razao, replaced_inline_size,
};

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
    /// A ARESTA de um inline que flui por fragmentos (`inline_por_fragmentos`):
    /// o padding+borda esquerdo antes do primeiro filho, o direito depois do
    /// último. Ocupa largura na linha (`ww`), não tem altura própria, e cola-se
    /// à palavra vizinha (um átomo nunca abre oportunidade de quebra) — é o que
    /// faz `<span style="padding:0 4px">aaa` medir 4px a mais na primeira linha
    /// e nada nas seguintes, como o Blink.
    ArestaInicio,
    ArestaFim,
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

/// Este inline COM conteúdo flui por FRAGMENTOS de linha (CSS 2.1 §9.2.2) em
/// vez de ser promovido a `inline-block`: tem uma superfície (fundo, borda,
/// padding, sombra, gradiente) e nada que o tornasse uma caixa atómica —
/// `width`/`height` não se aplicam a um inline, e a margem fica no caminho
/// antigo por ser o único destes que a promoção respeitava. Sob esta pergunta
/// o texto dele quebra com a linha e a caixa é a UNIÃO dos fragmentos
/// (`claude-inline-fragmentos`); antes, o span era um átomo que nem quebrava.
pub(crate) fn inline_por_fragmentos(css: &ComputedStyle) -> bool {
    cria_caixa_apesar_de_inline(css)
        && css.width.is_none()
        && css.height.is_none()
        && !ocupa_espaco(&css.margin)
}

/// As quatro arestas (padding + borda pintada) de um inline por fragmentos, em
/// px: `[esquerda, direita, cima, baixo]`. `base_w` é a base das percentagens.
pub(crate) fn arestas_do_inline(
    css: &ComputedStyle,
    fonte: f32,
    base_w: f32,
    ctx: &crate::layout::LayoutCtx,
) -> [f32; 4] {
    let rc = crate::style::ResolveCtx {
        parent_content_w: base_w,
        node_font_size: fonte,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let p = &css.padding;
    let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
    [
        p.left.resolve(&rc).unwrap_or(0.0) + bl,
        p.right.resolve(&rc).unwrap_or(0.0) + br,
        p.top.resolve(&rc).unwrap_or(0.0) + bt,
        p.bottom.resolve(&rc).unwrap_or(0.0) + bb,
    ]
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

/// Se a corrida de whitespace no INÍCIO de `rest` contém um `\n`, o índice (em
/// bytes) logo depois dele — o ponto onde `white-space: pre/pre-wrap/pre-line`
/// reinicia a busca por palavra depois de fechar uma linha. `None` quando a
/// corrida não tem `\n` (só colapsa) ou `rest` não começa por whitespace.
///
/// Só o PRIMEIRO `\n` da corrida é respondido: uma corrida "\n\n" produz duas
/// linhas em duas chamadas, não uma — o chamador (`wrap_runs`) volta a testar
/// o resto da corrida na iteração seguinte, exatamente como já fazia para o
/// colapso de espaços comuns.
pub(crate) fn quebra_forcada_em(rest: &str) -> Option<usize> {
    let fim = rest.find(|c: char| !e_espaco_css(c)).unwrap_or(rest.len());
    rest[..fim].find('\n').map(|nl| nl + 1)
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
#[cfg(test)]
mod quebra_forcada;
