//! A CAIXA de um pseudo-elemento (`::before`/`::after`) medida com box model
//! completo — padding, borda, margem, `box-sizing` — e pintada (fundo, quatro
//! barras de borda, texto). Antes deste lote (issue #2731, BT-5) isto vivia
//! DUAS vezes, byte a byte: `pseudo_bloco.rs::PseudoBlockBox`/`medir`/`pintar`
//! e `flex_pseudo.rs::PseudoItem`/`medir`/`pintar` resolviam o MESMO padding,
//! a MESMA borda, a MESMA margem e pintavam com o MESMO desenho — só a
//! decisão de largura/altura por omissão divergia entre os dois papéis. Uma
//! correção ao desenho (a próxima borda com canto arredondado, por exemplo)
//! só chegava a quem a tivesse pedido primeiro; agora chega às duas por ter
//! um único sítio.
//!
//! Esta é a SEGUNDA e a TERCEIRA das três implementações que a issue pedia
//! unificadas. A PRIMEIRA — `runs.rs::pseudo_run`, o caminho INLINE — fica de
//! fora por escolha e não por esquecimento: ela entrega a caixa gerada como
//! um `InlineRun` (texto + cor + peso + decoração, sem padding/borda/margem
//! nenhuma — ver o corte declarado no cabeçalho desse ficheiro), que é uma
//! representação diferente da que este módulo produz, e `runs.rs` está fora
//! da área deste lote (outro agente mexe nele na mesma árvore). Se um dia o
//! caminho inline ganhar box model, `pseudo_run` (linhas 66–103 de
//! `layout/runs.rs`) é o sítio a mudar: passaria a montar uma `CaixaGerada`
//! como aqui e a entregá-la ao fluxo inline como uma caixa atómica em vez de
//! um `InlineRun` de texto solto — uma mudança de REPRESENTAÇÃO da linha, não
//! deste ficheiro.
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
//!   qualquer filho de bloco (`pseudo_bloco.rs` reusa a máquina de
//!   `vertical.rs` — `Strut`/`junta_ao_strut`/`strut_colapsado`); um item flex
//!   NUNCA colapsa margens com o que o rodeia (Flexbox §4: "margins of
//!   adjacent flex items do not collapse"), por isso `flex_pseudo.rs` não
//!   chama nada disto e usa `ml`/`mr`/`mt`/`mb` diretamente.

use super::*;

/// A caixa OUTER (com margens) de um pseudo-elemento, já medida — comum aos
/// dois papéis. `pseudo_bloco.rs` e `flex_pseudo.rs` continuam a ter os seus
/// próprios nomes (`PseudoBlockBox`, `PseudoItem`) como `type` alias para
/// este tipo: o nome no ponto de uso ainda diz qual papel é, só a estrutura
/// deixou de estar escrita duas vezes.
pub(in crate::layout) struct CaixaGerada {
    pub(in crate::layout) caixa: crate::pseudo::PseudoBox,
    pub(in crate::layout) w: f32,
    pub(in crate::layout) h: f32,
    pub(in crate::layout) ml: f32,
    pub(in crate::layout) mr: f32,
    pub(in crate::layout) mt: f32,
    pub(in crate::layout) mb: f32,
    /// borda + padding por lado: cima, direita, baixo, esquerda.
    pub(in crate::layout) arestas: [f32; 4],
    pub(in crate::layout) texto: String,
    pub(in crate::layout) fonte: f32,
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
    caixa: crate::pseudo::PseudoBox,
    arestas: Arestas,
    conteudo_w: f32,
    conteudo_h: f32,
    texto: String,
    fonte: f32,
) -> CaixaGerada {
    let (w, h) = dimensionar(&caixa.css, &arestas, conteudo_w, conteudo_h);
    CaixaGerada {
        caixa,
        w,
        h,
        ml: arestas.ml,
        mr: arestas.mr,
        mt: arestas.mt,
        mb: arestas.mb,
        arestas: arestas.valores,
        texto,
        fonte,
    }
}

/// Pinta a caixa com o canto superior-esquerdo da margin box em (`x`,`y`):
/// fundo, as quatro barras de borda, o texto — o desenho que
/// `pseudo_bloco.rs` e `flex_pseudo.rs` tinham cada um a sua cópia dele.
pub(in crate::layout) fn pintar(list: &mut DisplayList, caixa: &CaixaGerada, x: f32, y: f32, ctx: &LayoutCtx) {
    let css = &caixa.caixa.css;
    let r = Rect::new(
        x + caixa.ml,
        y + caixa.mt,
        caixa.w - caixa.ml - caixa.mr,
        caixa.h - caixa.mt - caixa.mb,
    );
    if let Some(bg) = css.bg {
        list.items.push(DisplayItem::SolidRect { rect: r, color: bg, radius: Corners::ZERO });
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
            list.items.push(DisplayItem::SolidRect { rect, color: side.color, radius: Corners::ZERO });
        }
    }
    if !caixa.texto.is_empty() {
        let mono = css.font_family.as_deref().is_some_and(crate::style::is_mono_family);
        let is_ahem = super::fonte_metricas::usa_ahem(css.font_family.as_deref());
        let lh = crate::inline_box::altura_da_linha(css, caixa.fonte, ctx.measurer);
        let conteudo = crate::inline_box::altura_do_conteudo(caixa.fonte, css.font_family.as_deref(), ctx.measurer);
        list.items.push(DisplayItem::Text {
            x: r.x + caixa.arestas[3],
            y: r.y + caixa.arestas[0] + (lh - conteudo) / 2.0,
            text: caixa.texto.clone().into(),
            color: css.color.unwrap_or(0x000000FF),
            size: caixa.fonte,
            mono,
            is_ahem,
            bold: css.bold.unwrap_or(false),
            italic: false,
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

    /// A conta que `pseudo_bloco::medir` e `flex_pseudo::medir` faziam CADA
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
        let bloco = montar(caixa_bloco, arestas_bloco, 50.0, 20.0, "x".into(), 16.0);
        let item = montar(caixa_item, arestas_item, 50.0, 20.0, "x".into(), 16.0);
        assert_eq!((bloco.w, bloco.h), (item.w, item.h));
        assert_eq!(bloco.arestas, item.arestas);
    }
}
