//! RUNS: os pedaços de texto e de conteúdo atómico que uma linha vai conter,
//! recolhidos da árvore antes de se saber onde ela quebra.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução deste ficheiro é byte a byte a do original.

use super::*;
/// Um pedaço de texto inline com seu estilo resolvido (cor/peso herdados do span pai).
/// `atomic: Some((idx, kind))` = uma CAIXA em vez de texto — um widget de
/// formulário, um replaced element (`<img>`), ou o marcador de um inline vazio.
/// As duas primeiras fluem como uma "palavra" inquebrável de `ww × wh` pontos
/// (item 8 do handoff #1793; os botões 'Pesquisa Google' do google legado vivem
/// em span>span>input); o marcador não ocupa nada.
pub(in crate::layout) struct InlineRun {
    pub(in crate::layout) text: String,
    pub(in crate::layout) color: u32,
    pub(in crate::layout) bold: bool,
    /// `font-style: italic` do span que contém este texto. Eixo INDEPENDENTE do
    /// `bold` — `<em><strong>` é bold-italic e um único bit não o exprimiria.
    pub(in crate::layout) italic: bool,
    /// decoração do RUN (0=none 1=underline 2=line-through) — vem do <a>/<span>
    /// que contém o texto, não do bloco pai (um <a> sublinha só o seu texto).
    pub(in crate::layout) deco: u8,
    /// Elementos inline ancestrais deste run. Cada um recebe a união dos fragmentos.
    pub(in crate::layout) owners: Vec<NodeIdx>,
    pub(in crate::layout) atomic: Option<(NodeIdx, AtomicKind)>,
    pub(in crate::layout) ww: f32,
    pub(in crate::layout) wh: f32,
}

/// O run de texto de uma caixa gerada (`::before`/`::after`) de `id`, ou vazio
/// se a cascata não manda gerar nenhuma.
///
/// Entregar conteúdo gerado como um `InlineRun` é o que faz esta funcionalidade
/// caber sem reescrever o fluxo: um run é "texto com um estilo, pertencente a
/// estes elementos inline", e é exatamente o que um `::before` de texto é. Em
/// particular ele quebra linha, herda e é medido pelo mesmo caminho do resto —
/// nada disto precisou de um segundo caminho.
///
/// `donos` é a CADEIA inline inteira terminada no elemento originante, e não só
/// ele. No browser a caixa gerada está dentro da caixa do elemento e um clique
/// nela atinge o elemento — mas também está dentro de cada inline que o
/// envolve, exatamente como o texto normal está.
///
/// Isto já esteve errado, e o sintoma era invisível até o resto ficar certo:
/// com `owners: vec![id]` um `<span><a></a></span>` em que todo o conteúdo do
/// `<a>` vem de `a::before` deixava o `<span>` sem geometria NENHUMA, porque
/// nada lhe chamava `union_rect`. Na Wikipédia eram os 397 retrolinks da lista
/// de referências. Um fragmento gerado é um fragmento: conta para a união dos
/// ancestrais como qualquer outro, e é `uniontests.rs` que o fixa.
///
/// CORTE DECLARADO: só o texto e as propriedades que um run carrega (cor, peso,
/// decoração) chegam à pintura. `background`, `padding`, `border` e `width` do
/// pseudo são ignorados, e `inline-block`/`position:absolute` nele são
/// tratados como o inline que a maioria é. Medido na folha da Wikipédia: 88
/// das 100 regras com pseudo-elemento são inline por omissão.
///
/// `display:block`/`flex`/`grid` SAIU deste corte (lote `pintura-e-caixas`):
/// esse pseudo agora gera uma caixa de BLOCO própria — `pseudo_bloco.rs`, só
/// para o DONO de um fluxo vertical (`<p>`, `<div>`, …) — e não pode ser
/// entregue aqui também, ou o conteúdo pinta DUAS vezes (uma por caminho). Um
/// pseudo de bloco de um elemento que NÃO é dono de fluxo vertical (um
/// `<span>` a meio de uma linha, por exemplo) não tem hoje onde a caixa de
/// bloco se prenda — fica sem nenhuma das duas, o que é mais estreito do que
/// "sempre inline" mas nunca duplicado.
pub(in crate::layout) fn pseudo_run(
    dom: &Dom,
    id: NodeIdx,
    // A cadeia inline que envolve o originante, ele incluído e por último.
    donos: &[NodeIdx],
    pe: crate::style::PseudoElement,
    // A cor já resolvida do contexto — a caixa gerada herda-a quando não
    // declara `color`.
    cor_herdada: u32,
    // idem para o itálico: a caixa gerada herda o estilo do elemento.
    herdado_italico: bool,
) -> Option<InlineRun> {
    let caixa = dom.pseudo_box(id, pe)?;
    if matches!(
        caixa.css.effective_display(),
        Some(
            crate::style::DisplayKind::Block
                | crate::style::DisplayKind::Flex
                | crate::style::DisplayKind::Grid
        )
    ) {
        return None;
    }
    crate::bump!(inline_runs);
    Some(InlineRun {
        text: caixa.texto,
        color: cor_visivel(&caixa.css, caixa.css.color.unwrap_or(cor_herdada)),
        bold: caixa.css.bold.unwrap_or(false),
        // a caixa gerada é do PRÓPRIO elemento: nenhuma tag nova entra, por isso
        // a UA não tem aqui nada a dizer — só o CSS do pseudo e o que herdou.
        italic: caixa.css.italic.unwrap_or(herdado_italico),
        deco: decoration_code(&caixa.css),
        owners: donos.to_vec(),
        atomic: None,
        ww: 0.0,
        wh: 0.0,
    })
}

/// Coleta os RUNS de texto de `id` em ordem de documento, cada um com a COR efetiva
/// do elemento inline que o contém (um `<span style=color:x>` muda a cor do seu
/// texto). Aplica text-transform por run. A cor vem do `computed_style_idx` do nó
/// inline (que já herda do pai via a cascade) — é por isso que o style do span passa
/// a valer no texto.
pub(in crate::layout) fn collect_runs(
    dom: &Dom,
    id: NodeIdx,
    parent_css: &ComputedStyle,
    avail_w: f32,
    ctx: &LayoutCtx,
) -> Vec<InlineRun> {
    let _phase = crate::metrics::phases::scope("collect-runs");
    let mut runs = Vec::new();
    walk(
        dom,
        ctx,
        avail_w,
        id,
        cor_visivel(parent_css, parent_css.color.unwrap_or(0x000000FF)),
        decoration_code(parent_css),
        parent_css.text_transform,
        parent_css.bold.unwrap_or(false),
        parent_css.italic.unwrap_or(false),
        &[],
        &mut runs,
    );
    return runs;

    fn walk(
        dom: &Dom,
        ctx: &LayoutCtx,
        avail_w: f32,
        id: NodeIdx,
        inherited_color: u32,
        inherited_deco: u8,
        inherited_tt: Option<crate::style::TextTransform>,
        inherited_bold: bool,
        inherited_italic: bool,
        inherited_owners: &[NodeIdx],
        out: &mut Vec<InlineRun>,
    ) {
        match &dom.node(id).kind {
            NodeKind::Text(t) => {
                let text = match inherited_tt {
                    Some(tt) => tt.apply(t),
                    None => t.clone(),
                };
                crate::bump!(inline_runs);
                out.push(InlineRun {
                    text,
                    color: inherited_color,
                    bold: inherited_bold,
                    italic: inherited_italic,
                    deco: inherited_deco,
                    owners: inherited_owners.to_vec(),
                    atomic: None,
                    ww: 0.0,
                    wh: 0.0,
                });
            }
            NodeKind::Element { tag } => {
                // `<script>`/`<style>`/head-etc DENTRO de um contexto inline (um
                // script dentro de <td>/<center> — google.com faz isso): o texto
                // cru NÃO é conteúdo renderável — sem este skip, o código JS era
                // PINTADO na página.
                if is_non_rendered_tag(tag) {
                    return;
                }
                // `display:none` DENTRO de uma linha. O comentário de
                // `e_display_none` diz que a herança vem de "quem varre já não
                // desce nele" — e este varredor descia: um
                // `<span><span style=display:none>Z39.88…</span></span>` (o
                // COinS de cada citação da Wikipédia, ~280 na página) era
                // medido e PINTADO na linha, dando ao pai a largura do texto
                // oculto em vez da caixa de largura zero que o Chrome lhe dá.
                //
                // Saltar aqui é também o que devolve a caixa ao pai: sem filho
                // que gere run, ele cai no `Marker` lá abaixo, que é a resposta
                // que já existia para o inline vazio. A alternativa — um caminho
                // novo para "inline cujo conteúdo todo é invisível" — era pôr a
                // mesma resposta num segundo sítio.
                if e_display_none(dom, id) {
                    return;
                }
                // WIDGET inline: um `<input>` no meio do fluxo (botão/campo) vira
                // um run-widget com o tamanho pré-medido — o wrap o trata como
                // palavra inquebrável e a emissão pinta a caixa no lugar.
                if is_text_input_tag(tag) {
                    let itype = dom
                        .node(id)
                        .attr("type")
                        .map(|t| t.to_ascii_lowercase())
                        .unwrap_or_default();
                    if itype == "hidden" {
                        return;
                    }
                    let (ww, wh) = inline_widget_size(dom, id, &itype, avail_w, ctx);
                    // Os ANCESTRAIS inline não engolem a caixa deste widget: no
                    // browser a caixa de um inline tem a largura do que ele
                    // contém e a altura da FONTE. Quem recebe `ww × wh` é só o
                    // próprio elemento, na emissão.
                    let owners = inherited_owners.to_vec();
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Widget)),
                        ww,
                        wh,
                    });
                    return;
                }
                // `<br>`: uma QUEBRA no meio do fluxo. Não é texto nem caixa — é o
                // fim da linha corrente, e o browser dá-lhe na mesma posição e
                // altura de linha. Sem isto as duas linhas que ele separa saíam
                // como uma só, e tudo o que vinha abaixo subia uma linha.
                if tag == "br" {
                    let mut owners = inherited_owners.to_vec();
                    owners.push(id);
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Break)),
                        ww: 0.0,
                        wh: 0.0,
                    });
                    return;
                }
                // REPLACED inline (`<img>` dentro de um `<a>`, `<video>`, …): não é
                // texto e não tem filhos que o descrevam, por isso não produzia run
                // nenhum e ficava sem caixa. Flui como palavra inquebrável.
                let rcss = dom.computed_style_idx(id).unwrap_or_default();
                if let Some((ww, wh)) =
                    crate::inline_box::replaced_inline_size(dom, id, &rcss, avail_w, ctx)
                {
                    // Como no widget: a caixa do replaced é dele; os ancestrais
                    // inline recebem só a linha que ele ocupa.
                    let owners = inherited_owners.to_vec();
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Replaced)),
                        ww,
                        wh,
                    });
                    return;
                }
                // INLINE COM CAIXA: mede-se como bloco shrink-to-fit e entra na
                // linha como palavra inquebrável. Antes fechava o fluxo inline e
                // abria linha própria — um `<p>texto <span com fundo>x</span>
                // texto</p>` saía em TRÊS linhas em vez de uma, e numa página
                // real isso multiplicava a altura do documento por ~2,7.
                if is_inline_block(dom, id) {
                    let (bw, bh) = measure_block(dom, id, avail_w, None, None, None, true, ctx);
                    let mut owners = inherited_owners.to_vec();
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners: std::mem::take(&mut owners),
                        atomic: Some((id, AtomicKind::Block)),
                        ww: bw,
                        wh: bh,
                    });
                    return;
                }
                // a cor/text-transform/peso/decoração DESTE inline (se declarar)
                // vence p/ os filhos (o <a> sublinha só o próprio texto).
                let css = dom.computed_style_idx(id);
                let color = css
                    .as_ref()
                    .and_then(|c| c.color)
                    .unwrap_or(inherited_color);
                let tt = css.as_ref().and_then(|c| c.text_transform).or(inherited_tt);
                let bold = css.as_ref().and_then(|c| c.bold).unwrap_or(inherited_bold);
                let italic = italico(css.as_deref(), Some(tag), inherited_italic);
                let deco = match css.as_deref().map(decoration_code) {
                    Some(d) if d != 0 => d,
                    _ => inherited_deco,
                };
                let mut owners = inherited_owners.to_vec();
                // Um `display:inline` DECLARADO é dono dos seus fragmentos,
                // mesmo quando `is_block_level` o marcou para pintura de caixa.
                //
                // `is_inline_text_container` pergunta `!is_block_level`, e essa
                // responde `true` a um inline que declare padding — porque
                // alguém tem de pintar esse padding. Só que "precisa de ser
                // pintado como caixa" não é "não é conteúdo de linha": o
                // elemento continua a fluir, os filhos continuam a receber as
                // suas caixas (é o que se mede: 223 descendentes certos), e o
                // único que ficava de fora era ele.
                //
                // É a hlist do MediaWiki, e bastava `padding:0` para a disparar:
                // `.hlist ul{padding:0}` faz `padding.any_set()` responder
                // "declarado" — que não é "cria caixa". 28 `<ul>` da página
                // ficavam sem retângulo à volta de conteúdo já desenhado.
                //
                // A alternativa era ensinar `any_set()` a ignorar o zero. Está
                // errada aqui por duas razões: o segundo seletor que atinge
                // estes mesmos `<ul>` declara `padding:0.125em 0`, que não é
                // zero e continuaria a perdê-los; e `any_set()` é lida por quem
                // decide pintura, onde "declarado" é a pergunta certa.
                let is_container = is_inline_text_container(dom, id)
                    || css.as_ref().and_then(|c| c.effective_display())
                        == Some(crate::style::DisplayKind::Inline);
                if is_container {
                    owners.push(id);
                }
                // As caixas geradas de um elemento INLINE (`a::after`) entram
                // aqui, à volta do conteúdo próprio dele. O dono de um fluxo
                // inteiro é tratado em `layout_inline_flow`, que é onde ele se
                // sabe dono; os dois casos não se sobrepõem.
                let before = out.len();
                // As ARESTAS de um inline por fragmentos (`AtomicKind::Aresta*`):
                // padding+borda esquerdo antes do conteúdo, direito depois —
                // largura na linha, sem altura, colados ao texto vizinho.
                let arestas = css
                    .as_deref()
                    .filter(|c| is_container && crate::inline_box::inline_por_fragmentos(c))
                    .filter(|_| super::caixa::tem_conteudo_para_fragmento(dom, id))
                    .map(|c| {
                        let fonte = font_px(c, DEFAULT_FONT_SIZE);
                        crate::inline_box::arestas_do_inline(c, fonte, avail_w, ctx)
                    });
                let aresta = |kind: AtomicKind, ww: f32, owners: &[NodeIdx]| InlineRun {
                    text: String::new(),
                    color,
                    bold: false,
                    italic: false,
                    deco: 0,
                    owners: owners.to_vec(),
                    atomic: Some((id, kind)),
                    ww,
                    wh: 0.0,
                };
                if let Some([esq, ..]) = arestas {
                    crate::bump!(inline_runs);
                    out.push(aresta(AtomicKind::ArestaInicio, esq, &owners));
                }
                // A cadeia que o fragmento gerado herda. `owners` só contém
                // `id` quando ele é container inline; um `inline-block` com
                // `::before` continua a ser dono da sua própria caixa gerada.
                let donos_do_pseudo = if owners.last() == Some(&id) {
                    owners.clone()
                } else {
                    let mut v = owners.clone();
                    v.push(id);
                    v
                };
                out.extend(pseudo_run(
                    dom,
                    id,
                    &donos_do_pseudo,
                    crate::style::PseudoElement::Before,
                    color,
                    italic,
                ));
                for &c in &dom.node(id).children {
                    walk(dom, ctx, avail_w, c, color, deco, tt, bold, italic, &owners, out);
                }
                out.extend(pseudo_run(
                    dom,
                    id,
                    &donos_do_pseudo,
                    crate::style::PseudoElement::After,
                    color,
                    italic,
                ));
                if let Some([_, dir, ..]) = arestas {
                    crate::bump!(inline_runs);
                    out.push(aresta(AtomicKind::ArestaFim, dir, &owners));
                }
                // Um inline VAZIO (`<source>`, `<br>`, `<span></span>`) não gerou run
                // e ficaria sem caixa. O marker dá-lhe a posição na linha sem lhe dar
                // largura nem altura próprias — que é a caixa que o browser reporta.
                if is_container && out.len() == before {
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Marker)),
                        ww: 0.0,
                        wh: 0.0,
                    });
                }
            }
            _ => {}
        }
    }
}

/// Tamanho OUTER de um widget inline (`<input>`): o MESMO cálculo que a emissão
/// usa (layout_button / layout_input), para o wrap reservar a largura exata.
pub(in crate::layout) fn inline_widget_size(
    dom: &Dom,
    id: NodeIdx,
    itype: &str,
    avail_w: f32,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    if matches!(itype, "submit" | "button" | "reset") {
        let font = font_px(&css, DEFAULT_FONT_SIZE - 3.0);
        let label = dom.node(id).attr("value").unwrap_or("").to_string();
        let tw = ctx.measurer.text_width(&label, font, false, false, false);
        let lh = ctx.measurer.line_height(font);
        return (tw + 24.0 + 6.0, lh + 10.0 + 4.0); // espelha layout_button
    }
    // Campo de texto ou marca: a MESMA medida que a emissão vai usar, pedida à
    // mesma função. Estava aqui uma cópia com números à mão (190 x lh+8) que
    // dizia espelhar o `layout_input` e não espelhava — um `checkbox` reservava
    // um campo de texto e pintava um quadrado.
    //
    // `None` de altura disponível: uma caixa numa linha não tem containing block
    // de altura definida, logo `height:%` vale `auto`, como no browser.
    medida_do_input(dom, id, &css, avail_w, None, None, None, ctx).outer()
}
