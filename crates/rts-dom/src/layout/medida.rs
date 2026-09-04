//! MEDIÇÃO: o `TextMeasurer` e o medidor aproximado do headless, mais as
//! larguras intrínsecas e as alturas externas que os pré-passos pedem.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.

use super::*;
/// Abstração de MEDIÇÃO de texto (largura/altura de uma string num tamanho/peso).
/// Vive aqui (no `rts-dom`) e é IMPLEMENTADA pelo backend (o egui mede via galley);
/// reimplementar largura de glifo no `rts-dom` é a armadilha que o roadmap alertou.
/// O layout depende SÓ deste trait — continua egui-free e testável com um mock.
pub trait TextMeasurer {
    /// Largura em pontos de `text` renderizado em `size` (mono ou proporcional,
    /// regular ou `bold`). O peso importa: a fonte bold é mais larga — medir regular
    /// e pintar bold faz o texto estourar a linha (quebra a mais).
    ///
    /// `italic` entra pelo mesmo argumento e NÃO porque tenhamos um fator para
    /// ele: um medidor com fonte de verdade (o do egui, que mede a galley)
    /// responde a largura real da família itálica, e recusar-lhe o bit seria
    /// pedir-lhe a largura do texto errado. O `ApproxMeasurer`, que não tem
    /// fonte, ignora-o e diz porquê na sua implementação.
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool, italic: bool) -> f32;
    /// Altura de UMA linha em `size` (line-height). Aproximação aceitável: `size *
    /// fator`; o backend pode dar o valor exato da fonte.
    fn line_height(&self, size: f32) -> f32;

    /// Ascent da fonte usado para alinhar texto com a baseline de atoms altos, e
    /// para posicionar `vertical-align: text-top`/`middle` no modelo de baseline
    /// (`layout::alinhamento_vertical`). `0.90×size`, calibrado contra o Chrome
    /// — ver `style::ASCENT_RATIO` para a derivação. Backends com métricas
    /// próprias (o `EguiMeasurer` do `rts-egui`) substituem pela ascent REAL da
    /// fonte carregada.
    fn font_ascent(&self, size: f32) -> f32 {
        size * crate::style::ASCENT_RATIO
    }

    /// Descent da fonte usado para fechar a line box depois de um inline-block, e
    /// para `vertical-align: text-bottom`. `0.3125×size` — ver
    /// `style::DESCENT_RATIO`.
    fn font_descent(&self, size: f32) -> f32 {
        size * crate::style::DESCENT_RATIO
    }

    /// IDENTIDADE deste medidor: dois medidores com a mesma identidade têm de
    /// dar a mesma largura para o mesmo texto.
    ///
    /// Entra na chave de todo cache de layout, porque a mesma árvore no mesmo
    /// viewport se dispõe diferente com outra fonte. Era o ENDEREÇO do `dyn`
    /// que servia de identidade — e um medidor construído na pilha por frame
    /// (o do egui é) pode mudar de endereço sem mudar de comportamento, ou
    /// reusar o endereço de outro que mudou: as duas falhas em direções
    /// opostas. O default `0` serve a um medidor sem estado; um backend cujo
    /// resultado dependa de fonte/escala DEVE derivar disto o que muda.
    fn identity(&self) -> u64 {
        0
    }
}

/// Medidor APROXIMADO, sem backend — para teste e para o caminho headless puro
/// (gerar layout sem janela). Largura ≈ `n_chars * size * 0.5` (média de fonte
/// proporcional latina); altura ≈ `size * 1.3`. Não é exato (o egui dá o real),
/// mas é determinístico e suficiente para block-flow (onde a largura do texto não
/// decide a da caixa — a caixa ocupa o container).
pub struct ApproxMeasurer;

impl TextMeasurer for ApproxMeasurer {
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool, _italic: bool) -> f32 {
        // `_italic` é IGNORADO de propósito, e a alternativa rejeitada foi
        // multiplicar por um fator: um itálico real é mais estreito ou mais
        // largo conforme a fonte, e não há aqui uma única medição contra o
        // Chrome que diga qual — ao contrário do 1,06 do bold e do 0,5498 do
        // mono, que o corpus calibrou. Um fator inventado seria um erro com
        // aparência de precisão. Quando houver medição, o número vai para
        // `style::text_metrics` ao lado dos outros.
        // Os avanços vivem em `style::text_metrics`, com a medição contra o
        // Chrome que os calibrou — o mono era 0.6 e o Chrome mede 0.5498.
        let mut per = if mono {
            crate::style::MONO_ADVANCE
        } else {
            crate::style::PROP_ADVANCE
        };
        if bold {
            per *= 1.06; // bold ~6% mais largo.
        }
        text.chars().count() as f32 * size * per
    }
    fn line_height(&self, size: f32) -> f32 {
        // 1.125 e não 1.3, e o número é uma APROXIMAÇÃO calibrada, não uma lei:
        // `line-height: normal` sai das métricas da fonte (ascent + descent +
        // line gap) e este medidor não tem fonte nenhuma. 1.125 é o que o Chrome
        // computa para a fonte padrão a 16px (18px), medido pelo corpus de
        // fixtures — o 1.3 anterior dava 20.8 e aparecia como o desvio mais
        // repetido do corpus, 43 vezes.
        //
        // Um backend COM métricas não usa isto: o `rts-egui` responde
        // `row_height` da fonte real. Este valor serve o layout headless, onde a
        // alternativa era não ter resposta nenhuma.
        //
        // A constante e a medição que a calibrou vivem em `style::text_metrics`,
        // porque `normal` é o valor INICIAL de uma propriedade CSS e não uma
        // preferência do medidor — e porque lá está o arredondamento para cima
        // que faz 20px dar 23 e 30px dar 34, os inteiros que o Chrome reporta
        // (sem ele saíam 22,5 e 33,75).
        crate::style::normal_line_height(size)
    }
}

/// Largura NATURAL do conteúdo de um nó (sem `width` explícito): a maior largura
/// de uma linha de texto entre os descendentes. É o "preferred width" do
/// shrink-to-fit (item flex / inline-block). Para um filho-bloco com `width`, usa
/// esse width (+ frame); para texto, a largura medida. Aproximação do max-content
/// (o inline-flow exato — palavras quebrando — vem na fatia de inline).
pub(in crate::layout) fn content_natural_width(
    dom: &Dom,
    id: NodeIdx,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    intrinsic_content_width(dom, id, font, ctx)
}

/// LARGURA INTRÍNSECA do CONTEÚDO de um elemento (max-content): quanto o conteúdo
/// QUER de largura sem quebrar. É a BASE de toda medição (shrink-to-fit, item flex,
/// inline-block, container flex). CONSCIENTE DO DISPLAY dos filhos:
/// - flex-ROW (horizontal/wrap): SOMA as larguras outer dos filhos + os gaps (eles
///   ficam lado a lado). Era o bug do navbar: `.logo`/`.links` (flex) mediam pelo
///   MAX, dando ~0.
/// - block (vertical): MAX das larguras dos filhos (empilham).
/// - texto: a largura do texto concatenado.
/// Recursivo: a largura de um filho é a SUA intrínseca + frame (ou seu `width` fixo).
pub(in crate::layout) fn intrinsic_content_width(
    dom: &Dom,
    id: NodeIdx,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let key = IntrinsicWidthKey {
        tree: dom.cache_identity(),
        node_epoch: dom.layout_epoch(id),
        style_epoch: crate::style::props::style_epoch(),
        node: id,
        font_size: font.to_bits(),
        viewport_w: ctx.viewport_w.to_bits(),
        viewport_h: ctx.viewport_h.to_bits(),
        measurer: ctx.measurer.identity(),
    };
    crate::bump!(intrinsic_calls);
    if let Some(hit) = dom.intrinsic_width_get(key) {
        crate::bump!(intrinsic_hits);
        return hit;
    }

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
            dom.intrinsic_width_put(key, w);
            return w;
        }
    }

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
        let width = ctx.measurer.text_width(&own_text, font, mono, bold, italic)
            + crate::style::spacing_width(own_text.chars().count(), ls)
            + ws_extra;
        dom.intrinsic_width_put(key, width);
        return width;
    }

    // TABELA: a largura que o conteúdo quer é a SOMA das colunas, e nenhuma das
    // duas regras abaixo a dá — o MAX (bloco) devolveria a linha mais larga e a
    // SOMA (flex) somaria linhas inteiras. Quem sabe é o algoritmo de colunas.
    if used_display(dom, id) == Some(crate::style::DisplayKind::Table) {
        let width = crate::table::max_content_width(dom, id, font, ctx);
        dom.intrinsic_width_put(key, width);
        return width;
    }

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
    for &child in &dom.node(id).children {
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
        let w = intrinsic_outer_width(dom, child, font, ctx);
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
    let width = if is_row {
        // soma + gaps entre os itens.
        sum + (count.saturating_sub(1)) as f32 * gap
    } else {
        maior
    };
    dom.intrinsic_width_put(key, width);
    width
}

/// `true` se este filho FECHA a corrida inline a que pertenceria — ou seja, se a
/// linha do max-content acaba nele.
///
/// `is_block_level` sozinho não serve: ele responde `true` a um `inline-block`,
/// porque um `inline-block` cria caixa e precisa do caminho de bloco para a
/// pintar. Mas criar caixa e quebrar a linha são perguntas diferentes, e o
/// `InlineBlock` é exatamente o valor que as separa — está escrito assim no
/// cabeçalho do `is_inline_block`, e é a mesma família do "não é inline?" que
/// esta árvore já pagou cinco vezes.
///
/// O `<br>` é o outro lado: não é de bloco, não cria caixa, e quebra na mesma.
fn fecha_a_corrida(dom: &Dom, id: NodeIdx) -> bool {
    if let NodeKind::Element { tag } = &dom.node(id).kind {
        if tag == "br" {
            return true;
        }
    }
    is_block_level(dom, id) && !is_inline_block(dom, id)
}

/// A largura OUTER intrínseca de UM filho (max-content): seu `width` fixo (+ frame),
/// senão a intrínseca do seu conteúdo (+ frame). Texto → largura do texto.
pub(crate) fn intrinsic_outer_width(
    dom: &Dom,
    id: NodeIdx,
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
            if let Some(w) = crate::style::dimensao_absoluta(
                css.width.unwrap_or(crate::style::Dimension::Auto),
                &resolve,
            ) {
                return if border_box {
                    w + css.margin.resolve_h_intrinseco(&resolve)
                } else {
                    w + frame
                };
            }
            // senão: a intrínseca do conteúdo + frame.
            intrinsic_content_width(dom, id, f, ctx) + frame
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

/// Altura OUTER que um filho QUER, para o align-items/cross-axis. Para nós-bloco,
/// MEDE chamando o `layout_block` real numa `DisplayList` DESCARTÁVEL — assim a
/// altura medida é EXATAMENTE a que será pintada (inclui height explícito, frame,
/// recursão nos filhos, %). Sem aproximação: a verificação adversarial pegou que a
/// estimativa por "nº de linhas × line-height" divergia da pintura quando o filho
/// tinha frame próprio ou múltiplas linhas, errando a centralização cross-axis.
pub(in crate::layout) fn child_outer_height(
    dom: &Dom,
    id: NodeIdx,
    container_w: f32,
    container_h: Option<f32>,
    parent_css: &ComputedStyle,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    match &dom.node(id).kind {
        // Como no eixo horizontal: qualquer elemento renderável mede-se pela sua
        // caixa real, porque um inline blockificado (item de flex) tem uma.
        NodeKind::Element { tag } if !is_non_rendered_tag(tag) => {
            // layout de teste numa lista descartável: o (_, outer_h) é a altura real.
            let (_, outer_h) =
                measure_block(dom, id, container_w, container_h, None, None, true, ctx);
            outer_h
        }
        // A MESMA altura que o fluxo dará a esta linha — medir com o default do
        // medidor enquanto o pai declara `line-height` fazia a medida do
        // cross-axis discordar da pintura, que é o erro que este comentário
        // acima diz que a verificação adversarial já apanhou uma vez.
        NodeKind::Text(_) => {
            crate::inline_box::altura_da_linha(parent_css, parent_font, ctx.measurer)
        }
        _ => 0.0,
    }
}

/// Largura OUTER que um filho QUER (sem pintar), para decidir a quebra de linha no
/// modo wrap. Bloco com `width`: esse width (+ frame); sem width: largura natural
/// do conteúdo (+ frame); texto solto: a largura do texto.
pub(in crate::layout) fn child_outer_width(
    dom: &Dom,
    id: NodeIdx,
    container_w: f32,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    match &dom.node(id).kind {
        // QUALQUER elemento renderável, e não só os de nível bloco: um `<span>`
        // BLOCKIFICADO (item de flex, float) tem largura natural como qualquer
        // outra caixa. Com o guard antigo caía no `_ => 0.0` e era medido como
        // tendo largura ZERO — a caixa existia e não tinha tamanho.
        NodeKind::Element { tag } if !is_non_rendered_tag(tag) => {
            let css = dom.computed_style_idx(id).unwrap_or_default();
            let font = font_px(&css, parent_font);
            let resolve = ResolveCtx {
                parent_content_w: container_w,
                node_font_size: font,
                root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            // frame horizontal = margin_h + 2*border + padding_h (cada já é o eixo;
            // unidades relativas resolvidas contra o container).
            let frame = css.margin.resolve_h(&resolve)
                + { let [_, r, _, l] = crate::style::borders::used_widths(&css); l + r }
                + css.padding.resolve_h(&resolve);
            // Em border-box, o `width` declarado JÁ é a caixa (outer sem margin) —
            // não soma pad/border de novo; só a margin. Em content-box, soma o frame.
            match css.width.and_then(|d| d.resolve(&resolve)) {
                Some(w) if css.border_box.unwrap_or(false) => w + css.margin.resolve_h(&resolve),
                Some(w) => w + frame,
                None => content_natural_width(dom, id, font, ctx) + frame,
            }
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

/// Concatena o texto de todos os descendentes de `id` (ordem de documento).
pub(in crate::layout) fn collect_text(dom: &Dom, id: NodeIdx) -> String {
    let _phase = crate::metrics::phases::scope("collect-text");
    let mut out = String::new();
    collect_into(dom, id, &mut out);
    return out;

    fn collect_into(dom: &Dom, id: NodeIdx, out: &mut String) {
        match &dom.node(id).kind {
            NodeKind::Text(t) => out.push_str(t),
            // `<script>`/`<style>` não são conteúdo renderável — o texto cru
            // deles não entra no texto pintado (mesmo skip do collect_runs).
            NodeKind::Element { tag } if is_non_rendered_tag(tag) => {}
            _ => {
                for &c in &dom.node(id).children {
                    collect_into(dom, c, out);
                }
            }
        }
    }
}
