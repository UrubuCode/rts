//! A CAIXA de um pseudo-elemento (`::before`/`::after`) medida com box model
//! completo — padding, borda, margem, `box-sizing` — e pintada (fundo, quatro
//! barras de borda, texto). Antes deste lote (issue #2731, BT-5) isto vivia
//! DUAS vezes, byte a byte: `pseudo_block.rs::PseudoBlockBox`/`medir`/`pintar`
//! e `flex/pseudo.rs::PseudoItem`/`medir`/`pintar` resolviam o MESMO padding,
//! a MESMA borda, a MESMA margem e pintavam com o MESMO desenho — só a
//! decisão de largura/altura por omissão divergia entre os dois papéis. Uma
//! correção ao desenho (a próxima borda com canto arredondado, por exemplo)
//! só chegava a quem a tivesse pedido primeiro; agora chega às duas por ter
//! um único sítio.
//!
//! The THIRD role, the inline path, now measures and paints through here too
//! (`pseudo_inline.rs`): an `inline-block` pseudo is a `CaixaGerada` that the
//! line carries as an atom, sized by the same [`montar`] and painted by the
//! same [`pintar`]. An `inline` pseudo does not become a `CaixaGerada` — it
//! is text that breaks with the line, so its surface is painted per line
//! fragment by `inline_fragments.rs`, the way a real inline's is.
//!
//! The text of the box WRAPS at its content width ([`linhas_do_texto`]), by
//! the same `wrap_runs` the line flow uses. It used to be measured as one
//! word: a `display:block; width:40px` pseudo with four words stayed one line
//! tall and overflowed sideways (`claude-pseudo-caixa-gerada`, `#p4`).
//!
//! O que fica de fora de propósito, e é divergência de SPEC e não descuido:
//! - **a largura/altura por omissão** (`width`/`height` ausentes do pseudo):
//!   um pseudo de BLOCO enche o content-box do pai (CSS 2.1 §10.3.3, como
//!   qualquer bloco em fluxo normal); um pseudo-ITEM de flex encolhe ao seu
//!   conteúdo (Flexbox §9.2, shrink-to-fit — este motor não corre o algoritmo
//!   de flex sobre o conteúdo do próprio pseudo, e a aproximação prática é a
//!   largura do texto medido). Cada chamador decide o SEU par e entrega-o já
//!   resolvido a [`montar`]; nenhum dos dois é "o certo" para o outro papel.
//! - **o colapso de margem**: um pseudo de bloco participa dele como
//!   qualquer filho de bloco (`pseudo_block.rs` reusa a máquina de
//!   `vertical_flow.rs` — `Strut`/`junta_ao_strut`/`strut_colapsado`); um item flex
//!   NUNCA colapsa margens com o que o rodeia (Flexbox §4: "margins of
//!   adjacent flex items do not collapse"), por isso `flex/pseudo.rs` não
//!   chama nada disto e usa `ml`/`mr`/`mt`/`mb` diretamente.

use super::*;

/// A caixa OUTER (com margens) de um pseudo-elemento, já medida — comum aos
/// dois papéis. `pseudo_block.rs` e `flex/pseudo.rs` continuam a ter os seus
/// próprios nomes (`PseudoBlockBox`, `PseudoItem`) como `type` alias para
/// este tipo: o nome no ponto de uso ainda diz qual papel é, só a estrutura
/// deixou de estar escrita duas vezes.
pub(in crate::layout) struct CaixaGerada {
    pub(in crate::layout) caixa: crate::pseudo::PseudoBox,
    /// Its box in the tree (`BoxKind::Generated`), under which [`pintar`]
    /// records the geometry.
    pub(in crate::layout) gerada: crate::boxes::BoxId,
    pub(in crate::layout) w: f32,
    pub(in crate::layout) h: f32,
    pub(in crate::layout) ml: f32,
    pub(in crate::layout) mr: f32,
    pub(in crate::layout) mt: f32,
    pub(in crate::layout) mb: f32,
    /// borda + padding por lado: cima, direita, baixo, esquerda.
    pub(in crate::layout) arestas: [f32; 4],
    /// The text already broken into lines at the content width it was
    /// measured with. Kept broken rather than re-broken when painting: the
    /// box's height was decided by this count, and breaking twice could give
    /// two counts for one box.
    pub(in crate::layout) linhas: Vec<String>,
    pub(in crate::layout) fonte: f32,
}

/// The text of a generated box broken into lines at `largura` (its content
/// width), by the line flow's own `wrap_runs`, under the pseudo's own
/// `white-space`, `word-spacing` and `hyphens`. Empty text gives no line.
pub(in crate::layout) fn linhas_do_texto(css: &ComputedStyle, texto: &str, largura: f32, fonte: f32, ctx: &LayoutCtx) -> Vec<String> {
    if texto.is_empty() {
        return Vec::new();
    }
    let nowrap = matches!(css.white_space, Some(crate::style::WhiteSpace::Nowrap | crate::style::WhiteSpace::Pre));
    let run = InlineRun {
        text: texto.to_string(),
        color: 0,
        bold: css.bold.unwrap_or(false),
        italic: css.italic.unwrap_or(false),
        deco: 0,
        owners: Vec::new(),
        atomic: None,
        ww: 0.0,
        wh: 0.0,
    };
    let familia = css.font_family.as_deref();
    let linhas = wrap_runs(
        std::slice::from_ref(&run),
        &mut |_| if nowrap { f32::INFINITY } else { largura },
        &mut |_| 0.0,
        fonte,
        familia.is_some_and(crate::style::is_mono_family),
        crate::inline_box::quebra_dentro(css),
        crate::layout::inline::preserved_spaces::Spaces::from_css(css),
        css.word_spacing.unwrap_or(0.0),
        css.hyphens != Some(crate::style::vocab::Hyphens::None),
        &crate::layout::inline::run_font::Fontes::uniforme(familia, fonte, familia.is_some_and(crate::style::is_mono_family)),
        ctx.measurer,
    );
    linhas
        .into_iter()
        .map(|l| l.into_iter().map(|s| s.text).collect::<String>())
        .filter(|l| !l.is_empty())
        .collect()
}

/// The generated box `pe` of `id` as the TREE has it: its `BoxId` — the child
/// of `dono`, the box of `id` being laid out — and its content, asked of the
/// cascade now (`BoxTree::pseudo_box`), never a copy.
///
/// This is where the block and flex roles stopped re-deriving the pseudo from
/// the node: existence is the tree's answer, and the build asked the same
/// `Dom::pseudo_box` under the same memo key, so the two cannot disagree
/// within a pass.
pub(in crate::layout) fn da_arvore(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    dono: crate::boxes::BoxId,
    pe: crate::style::PseudoElement,
) -> Option<(crate::boxes::BoxId, crate::pseudo::PseudoBox)> {
    let gerada = tree.generated_child(dono, pe)?;
    Some((gerada, tree.pseudo_box(dom, gerada)?))
}

/// The height of `linhas` lines of the pseudo's text: one line box each.
pub(in crate::layout) fn altura_das_linhas(css: &ComputedStyle, linhas: &[String], fonte: f32, ctx: &LayoutCtx) -> f32 {
    linhas.len() as f32 * crate::inline_box::altura_da_linha(css, fonte, ctx.measurer)
}

/// Borda + padding (combinados em `valores`, por lado: cima, direita, baixo,
/// esquerda) e margem, resolvidos contra `css` — a parte que os dois papéis
/// resolvem de forma IDÊNTICA (CSS 2.1 §8 não distingue bloco de item flex
/// para padding/borda/margem em si, só para o que acontece à volta deles).
pub(in crate::layout) struct Arestas {
    pub(in crate::layout) valores: [f32; 4],
    pub(in crate::layout) ml: f32,
    pub(in crate::layout) mr: f32,
    pub(in crate::layout) mt: f32,
    pub(in crate::layout) mb: f32,
}

pub(in crate::layout) fn resolve_arestas(css: &ComputedStyle, r: &ResolveCtx) -> Arestas {
    let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
    let p = &css.padding;
    let (pl, pr) = (p.left.resolve(r).unwrap_or(0.0), p.right.resolve(r).unwrap_or(0.0));
    let (pt, pb) = (p.top.resolve(r).unwrap_or(0.0), p.bottom.resolve(r).unwrap_or(0.0));
    let m = &css.margin;
    let (ml, mr) = (m.left.resolve(r).unwrap_or(0.0), m.right.resolve(r).unwrap_or(0.0));
    let (mt, mb) = (m.top.resolve(r).unwrap_or(0.0), m.bottom.resolve(r).unwrap_or(0.0));
    Arestas { valores: [bt + pt, br + pr, bb + pb, bl + pl], ml, mr, mt, mb }
}

/// `box-sizing` aplicado ao par (conteúdo-w, conteúdo-h): devolve a caixa
/// OUTER (com margens). Igual nos dois papéis — `border-box` só muda o que
/// `width`/`height` mede, e só quando a propriedade foi DECLARADA; sem
/// declaração o resultado já é content-box nos dois.
fn dimensionar(css: &ComputedStyle, arestas: &Arestas, conteudo_w: f32, conteudo_h: f32) -> (f32, f32) {
    let [at, ar, ab, al] = arestas.valores;
    let (w, h) = if css.border_box.unwrap_or(false) && (css.width.is_some() || css.height.is_some()) {
        (
            css.width.map_or(conteudo_w + al + ar, |_| conteudo_w),
            css.height.map_or(conteudo_h + at + ab, |_| conteudo_h),
        )
    } else {
        (conteudo_w + al + ar, conteudo_h + at + ab)
    };
    (w + arestas.ml + arestas.mr, h + arestas.mt + arestas.mb)
}

/// Monta a [`CaixaGerada`] a partir da caixa já resolvida por
/// `dom.pseudo_box`, das arestas já resolvidas e do par de conteúdo que o
/// chamador decidiu (ver o cabeçalho do ficheiro: é o único pedaço que os
/// dois papéis não partilham).
pub(in crate::layout) fn montar(
    (gerada, caixa): (crate::boxes::BoxId, crate::pseudo::PseudoBox),
    arestas: Arestas,
    conteudo_w: f32,
    conteudo_h: f32,
    linhas: Vec<String>,
    fonte: f32,
) -> CaixaGerada {
    let (w, h) = dimensionar(&caixa.css, &arestas, conteudo_w, conteudo_h);
    CaixaGerada {
        caixa,
        gerada,
        w,
        h,
        ml: arestas.ml,
        mr: arestas.mr,
        mt: arestas.mt,
        mb: arestas.mb,
        arestas: arestas.valores,
        linhas,
        fonte,
    }
}

/// Pinta a caixa com o canto superior-esquerdo da margin box em (`x`,`y`):
/// fundo, as quatro barras de borda, o texto — o desenho que
/// `pseudo_block.rs` e `flex/pseudo.rs` tinham cada um a sua cópia dele.
///
/// And records its BORDER box under its `BoxId`, the rect every other box
/// records (`layout_block`'s `box_rect`), so `DisplayList::rect_of_box`
/// answers for it. Here and not in each role: this is the one place all three
/// (block, flex item, `inline-block` atom) pass with their final position.
/// It reaches no DOM-facing geometry — the box has no node — and it is
/// shifted with its element by `relative.rs`/`transform_rects.rs`, which walk
/// the tree's full `children`.
pub(in crate::layout) fn pintar(list: &mut DisplayList, caixa: &CaixaGerada, x: f32, y: f32, ctx: &LayoutCtx) {
    let css = &caixa.caixa.css;
    let r = Rect::new(
        x + caixa.ml,
        y + caixa.mt,
        caixa.w - caixa.ml - caixa.mr,
        caixa.h - caixa.mt - caixa.mb,
    );
    super::record_box_rect(list, caixa.gerada, r);
    if let Some(bg) = css.bg {
        list.push_item(DisplayItem::SolidRect { rect: r, color: bg, radius: Corners::ZERO });
    }
    let sides = crate::style::borders::resolved_sides(css);
    let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
    let barras = [
        (Rect::new(r.x, r.y, r.w, bt), sides[0]),
        (Rect::new(r.x + r.w - br, r.y, br, r.h), sides[1]),
        (Rect::new(r.x, r.y + r.h - bb, r.w, bb), sides[2]),
        (Rect::new(r.x, r.y, bl, r.h), sides[3]),
    ];
    for (rect, side) in barras {
        if side.paints() && side.color & 0xFF != 0 {
            list.push_item(DisplayItem::SolidRect { rect, color: side.color, radius: Corners::ZERO });
        }
    }
    let mono = css.font_family.as_deref().is_some_and(crate::style::is_mono_family);
    let is_ahem = crate::layout::measure::font_metrics::usa_ahem(css.font_family.as_deref());
    let lh = crate::inline_box::altura_da_linha(css, caixa.fonte, ctx.measurer);
    let conteudo = crate::inline_box::altura_do_conteudo(caixa.fonte, css.font_family.as_deref(), ctx.measurer);
    for (i, linha) in caixa.linhas.iter().enumerate() {
        list.push_item(DisplayItem::Text {
            x: r.x + caixa.arestas[3],
            y: r.y + caixa.arestas[0] + i as f32 * lh + crate::inline_box::meia_entrelinha(lh, conteudo),
            text: linha.clone().into(),
            color: css.color.unwrap_or(0x000000FF),
            size: caixa.fonte,
            mono,
            is_ahem,
            bold: css.bold.unwrap_or(false),
            // The same `italic` the lines were broken with (`linhas_do_texto`).
            italic: css.italic.unwrap_or(false),
            letter_spacing: css.letter_spacing.unwrap_or(0.0),
            decoration: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::values::{Dimension, Side};

    fn ctx() -> ResolveCtx {
        ResolveCtx {
            parent_content_w: 300.0,
            node_font_size: 16.0,
            root_font_size: 16.0,
            viewport_w: 1280.0,
            viewport_h: 720.0,
        }
    }

    /// A conta que `pseudo_block::medir` e `crate::layout::flex::pseudo::medir` faziam CADA
    /// UMA a sua cópia até este lote: padding+margem resolvidos e `box-sizing`
    /// aplicado ao par (largura, altura) DECLARADO. Com `width`/`height`
    /// explícitos os dois papéis não têm decisão nenhuma a tomar — divergem
    /// só quando o valor é `auto` (ver `pseudo_bt5_corpus.rs`, que fixa isso
    /// pelos DOIS caminhos de layout) — por isso este teste pina a aritmética
    /// PARTILHADA sozinha, sem precisar de uma árvore DOM.
    #[test]
    fn dimensionar_com_border_box_usa_o_valor_declarado_como_caixa_final() {
        let mut css = ComputedStyle::default();
        css.padding = crate::style::values::Edges::all(Side::Len(Dimension::Px(5.0)));
        css.margin = crate::style::values::Edges::all(Side::Len(Dimension::Px(10.0)));
        css.width = Some(Dimension::Px(100.0));
        css.height = Some(Dimension::Px(40.0));
        css.border_box = Some(true);
        let r = ctx();
        let arestas = resolve_arestas(&css, &r);
        // Sem borda declarada, "arestas" é só o padding: 5 em cada lado.
        assert_eq!(arestas.valores, [5.0, 5.0, 5.0, 5.0]);
        let (w, h) = dimensionar(&css, &arestas, 100.0, 40.0);
        // border-box: os 100/40 declarados JÁ são a caixa border+padding+
        // conteúdo; só a margem soma por cima.
        assert_eq!((w, h), (120.0, 60.0), "w=100+10+10, h=40+10+10");
    }

    /// O mesmo par, mas `content-box` (o default): agora `width`/`height`
    /// medem só o CONTEÚDO, e o padding entra na caixa por cima.
    #[test]
    fn dimensionar_sem_border_box_soma_o_padding_por_cima() {
        let mut css = ComputedStyle::default();
        css.padding = crate::style::values::Edges::all(Side::Len(Dimension::Px(5.0)));
        css.margin = crate::style::values::Edges::all(Side::Len(Dimension::Px(0.0)));
        css.width = Some(Dimension::Px(100.0));
        css.height = Some(Dimension::Px(40.0));
        let r = ctx();
        let arestas = resolve_arestas(&css, &r);
        let (w, h) = dimensionar(&css, &arestas, 100.0, 40.0);
        assert_eq!((w, h), (110.0, 50.0), "100+5+5, 40+5+5 — padding somado");
    }

    /// `montar` é o ponto que os dois papéis chamam depois de decidirem o SEU
    /// par (conteúdo-w, conteúdo-h) — este teste prova que, dado o MESMO par,
    /// `montar` responde a MESMA `CaixaGerada` independentemente de quem
    /// chamou, que é a garantia que esta unificação existe para dar.
    #[test]
    fn montar_da_a_mesma_caixa_para_o_mesmo_par_de_conteudo() {
        let mut css = ComputedStyle::default();
        css.margin = crate::style::values::Edges::all(Side::Len(Dimension::Px(10.0)));
        let r = ctx();
        let arestas_bloco = resolve_arestas(&css, &r);
        let arestas_item = resolve_arestas(&css, &r);
        let caixa_bloco = crate::pseudo::PseudoBox { texto: "x".into(), css: css.clone() };
        let caixa_item = crate::pseudo::PseudoBox { texto: "x".into(), css: css.clone() };
        // Any box of any tree: `montar` only carries it, it never reads it.
        let dom = crate::parse_html_to_dom("<p></p>");
        let gerada = dom.box_tree().roots().next().expect("the document has a root box");
        let bloco = montar((gerada, caixa_bloco), arestas_bloco, 50.0, 20.0, vec!["x".into()], 16.0);
        let item = montar((gerada, caixa_item), arestas_item, 50.0, 20.0, vec!["x".into()], 16.0);
        assert_eq!((bloco.w, bloco.h), (item.w, item.h));
        assert_eq!(bloco.arestas, item.arestas);
    }
}
