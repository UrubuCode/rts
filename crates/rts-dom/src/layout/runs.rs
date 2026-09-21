//! RUNS: os pedaços de texto e de conteúdo atómico que uma linha vai conter,
//! recolhidos da árvore antes de se saber onde ela quebra.

use super::*;
/// Um pedaço de texto inline com seu estilo resolvido (cor/peso herdados do span pai).
/// `atomic: Some((idx, caixa, kind))` = uma CAIXA em vez de texto — um widget de
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
    pub(in crate::layout) atomic: Option<(NodeIdx, Option<crate::boxes::BoxId>, AtomicKind)>,
    pub(in crate::layout) ww: f32,
    pub(in crate::layout) wh: f32,
}

/// Coleta os RUNS de texto de `id` em ordem de documento, cada um com a COR efetiva
/// do elemento inline que o contém (um `<span style=color:x>` muda a cor do seu
/// texto). Aplica text-transform por run. A cor vem do `computed_style_idx` do nó
/// inline (que já herda do pai via a cascade) — é por isso que o style do span passa
/// a valer no texto.
pub(in crate::layout) fn collect_runs(
    dom: &Dom,
    id: NodeIdx,
    // A CAIXA de `id`: por onde se desce, e o que cada ÁTOMO leva consigo.
    //
    // **É o que faz o fluxo inline VER a partição do CSS 2.1 §9.2.1.1.** Um
    // `<span>` com um `<div>` dentro nunca chega a `layout_block` — quem o
    // dispõe é este varredor, e ele lia `dom.node(span).children`, onde o
    // `<div>` continua a estar: descia nele como se o `<span>` fosse
    // transparente, e `width`, `height`, `background`, `padding` e `margin` do
    // bloco eram deitados fora. Com a caixa, os filhos vêm de
    // `tree.children(caixa)` — e a caixa de um FRAGMENTO do inline partido só
    // tem a corrida dela, sem o bloco, que a árvore já pôs como irmão.
    //
    // E dá a um ÁTOMO (inline-flex, inline-block, widget) a sua caixa, pela
    // qual `layout_block` o dispõe e lhe reserva a ordem de hit-test. `None`
    // sem árvore (`DisplayList::default()`): os filhos vêm do DOM.
    caixa: Option<crate::boxes::BoxId>,
    tree: &crate::boxes::BoxTree,
    parent_css: &ComputedStyle,
    avail_w: f32,
    ctx: &LayoutCtx,
) -> Vec<InlineRun> {
    let _phase = crate::metrics::phases::scope("collect-runs");
    let mut runs = Vec::new();
    walk(
        dom,
        tree,
        ctx,
        avail_w,
        id,
        caixa,
        cor_visivel(parent_css, parent_css.color.unwrap_or(0x000000FF)),
        decoration_code(parent_css),
        parent_css.text_transform,
        parent_css.bold.unwrap_or(false),
        parent_css.italic.unwrap_or(false),
        &[],
        &mut runs,
    );
    return runs;

    /// Os filhos por onde este varredor desce, e a caixa de cada um.
    ///
    /// Com caixa, a ÁRVORE decide: é o que exclui o filho de bloco que partiu
    /// este inline, porque ele já não é filho do fragmento. Sem caixa (sem
    /// árvore), o DOM. São os mesmos NÓS fora da partição — um comentário não
    /// gera caixa e este varredor já o ignorava, e um `display:none` gera
    /// caixa e continua a ser recusado por `e_display_none`.
    fn filhos_do_varrimento(
        dom: &Dom,
        tree: &crate::boxes::BoxTree,
        id: NodeIdx,
        caixa: Option<crate::boxes::BoxId>,
    ) -> Vec<(NodeIdx, Option<crate::boxes::BoxId>)> {
        let Some(b) = caixa else {
            return dom.node(id).children.iter().map(|&c| (c, None)).collect();
        };
        // Without `::before`/`::after`: their runs come from the two
        // `pseudo_run` calls around this walk, not from a child box.
        tree.children_without_generated(b)
            .iter()
            .map(|&cb| {
                // Uma caixa ANÓNIMA aqui seria a partição a criar uma onde não
                // cria — ela só aparece no CONTENTOR. Deixá-la cair em silêncio
                // perdia a corrida inteira que ela envolve.
                let no = tree
                    .node_of(cb)
                    .expect("uma caixa anonima dentro de um fragmento inline");
                (no, Some(cb))
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        dom: &Dom,
        tree: &crate::boxes::BoxTree,
        ctx: &LayoutCtx,
        avail_w: f32,
        id: NodeIdx,
        caixa: Option<crate::boxes::BoxId>,
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
                // Sem caixa COM árvore = inline partido só de espaço: os blocos
                // dele já são irmãos na árvore (ver `sequencia`); descer aqui
                // pelo DOM dispunha-os DUAS vezes, e os átomos sem caixa.
                if caixa.is_none() && !tree.is_empty() {
                    return;
                }
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
                // A FLOAT in the middle of the flow: only an anchor (`float_in_line.rs`).
                // An absolutely positioned box: the anchor of its static position.
                let ancora = super::float_in_line::anchor(dom, id, caixa, inherited_color)
                    .or_else(|| super::ancora_estatica::anchor(dom, id, caixa, inherited_color));
                if let Some(ancora) = ancora {
                    out.push(ancora);
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
                    let (ww, wh) = super::input::inline_widget_size(dom, id, &itype, avail_w, ctx);
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
                        atomic: Some((id, caixa, AtomicKind::Widget)),
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
                        atomic: Some((id, caixa, AtomicKind::Break)),
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
                    crate::inline_box::replaced_inline_size(dom, id, &rcss, avail_w, (None, None), ctx)
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
                        atomic: Some((id, caixa, AtomicKind::Replaced)),
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
                    let (bw, bh) = measure_block(dom, id, caixa.expect("sem caixa saiu acima"), avail_w, None, None, None, true, ctx);
                    let mut owners = inherited_owners.to_vec();
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners: std::mem::take(&mut owners),
                        atomic: Some((id, caixa, AtomicKind::Block)),
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
                    atomic: Some((id, caixa, kind)),
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
                out.extend(super::pseudo_inline::pseudo_run_da_caixa(
                    dom,
                    id,
                    caixa.and_then(|b| tree.generated_child(b, crate::style::PseudoElement::Before)),
                    &donos_do_pseudo,
                    crate::style::PseudoElement::Before,
                    color,
                    italic,
                    avail_w,
                    ctx,
                ));
                for (c, cb) in filhos_do_varrimento(dom, tree, id, caixa) {
                    walk(
                        dom, tree, ctx, avail_w, c, cb, color, deco, tt, bold, italic, &owners,
                        out,
                    );
                }
                out.extend(super::pseudo_inline::pseudo_run_da_caixa(
                    dom,
                    id,
                    caixa.and_then(|b| tree.generated_child(b, crate::style::PseudoElement::After)),
                    &donos_do_pseudo,
                    crate::style::PseudoElement::After,
                    color,
                    italic,
                    avail_w,
                    ctx,
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
                        atomic: Some((id, caixa, AtomicKind::Marker)),
                        ww: 0.0,
                        wh: 0.0,
                    });
                }
            }
            _ => {}
        }
    }
}

