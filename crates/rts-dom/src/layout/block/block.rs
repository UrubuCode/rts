//! `layout_block` — a colocação de UM bloco: caixa, margens, padding, borda, e
//! a escolha de como dispor os filhos.
//!
//! **Este módulo é uma função.** São 1 101 linhas (a função, 1 056), e ficam acima do teto de 500 de
//! propósito: partir uma função por dentro deixa de ser um movimento de código e
//! passa a ser uma alteração de comportamento que nenhuma régua desta arrumação
//! consegue verificar. O teto vale para ficheiros que juntam assuntos; aqui o
//! assunto é um só e tem 1 056 linhas. Reduzi-lo é trabalho medido, à parte:
//! extrair funções do corpo carrega comportamento e pede a régua de layout
//! (2026-09-26: `margin_collapse.rs` foi o único move inteiro possível).
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

pub(in crate::layout) use super::margin_collapse::{
    collapse_margin, edge_margin_from_children, escaped_child_margins, escaped_margins_for_box,
};

/// `true` se `id` estabelece o seu PRÓPRIO bloco de formatação (CSS 2.1
/// §9.4.1) — hoje usado para barrar o escape de margens
/// ([`escaped_child_margins`]) E para decidir o `BlockFormattingContext` de
/// `layout_block`; ver `block/bfc.rs` para o porquê da entidade. A raiz do
/// documento entra por `id` ser filho direto de `dom.root` — o único gatilho
/// que não está no `ComputedStyle`.
/// Raised to `pub(crate)` for `crate::boxes::context`: a box has to be able to
/// say whether it establishes its own formatting context, and re-deriving the
/// triggers there would be a second answer to a question this function already
/// carries with the fixtures that pinned each one. The rule it encodes is a
/// STYLE question and will move to `style/` with `is_block_level`.
pub(crate) fn establishes_block_formatting_context(dom: &Dom, id: NodeIdx, css: &ComputedStyle) -> bool {
    // The style half lives in `bfc_style.rs` since BT-5, so a box with no
    // node — a generated one — can ask it too. What stays is what only a node
    // answers: being the root, and `overflow` propagating to the viewport.
    let parent_css = dom.node(id).parent.and_then(|p| dom.computed_style_idx(p));
    super::bfc_style::by_style(css, parent_css.as_deref())
        || (super::bfc_style::overflow_establishes(css) && !super::overflow_viewport::propagated_to_viewport(dom, id))
        || dom.node(id).parent == Some(dom.root)
}


/// Faz o layout de UM nó-bloco a partir de `(x, y)`, com `avail_w` de largura
/// disponível (a do container). Emite os itens (fundo/borda/texto/filhos) na
/// `list` e devolve o TAMANHO EXTERNO `(outer_w, outer_h)` da caixa (incluindo
/// padding/border/margin) — o pai usa a altura (empilhamento vertical) ou a
/// largura (horizontal) para posicionar o irmão seguinte. Texto solto e nós inline
/// são desenhados como linhas dentro do content-box.
pub(crate) fn layout_block(
    dom: &Dom,
    id: NodeIdx,
    box_id: crate::boxes::BoxId,
    x: f32,
    y: f32,
    avail_w: f32,
    // Altura do CONTENT do containing block, quando DEFINIDA (height explícito no
    // pai / viewport na raiz): a base de `height: %` — que resolve contra a ALTURA
    // do pai (antes resolvia errado contra a largura; `h-100` não funcionava).
    // `None` = pai com altura auto → `height: %` vira auto (fiel ao browser).
    avail_h: Option<f32>,
    // Largura OUTER IMPOSTA (com margem) — o flex resolveu grow/shrink e DITA o
    // main size do item; vence width/min-max/shrink-to-fit (v1: clamp min/max no
    // resolve flex fica como corte documentado). `None` = fluxo normal.
    forced_outer_w: Option<f32>,
    // Altura OUTER IMPOSTA (com margem) — o `align-items/self: stretch` do flex.
    // O caller só passa para item SEM height explícito. `None` = altura natural.
    forced_outer_h: Option<f32>,
    // `true` quando `forced_outer_h` é o MAIN SIZE de um item de flex-COLUMN
    // (grow/shrink já resolvidos) e não o stretch do eixo cruzado: vence
    // `height`/`aspect-ratio` do próprio nó e pode ENCOLHER abaixo do
    // conteúdo — o oposto do `forced_outer_h` "mole" de baixo, que só
    // cresce (nunca corta um item mais alto que a linha). `false` em todo
    // caller que não seja `layout_children_column` — o eixo horizontal já
    // tem este comportamento em `content_w`/`forced_outer_w` (linha 468),
    // sem precisar de uma segunda flag: lá não há um "stretch mole" a
    // proteger, então o override é sempre incondicional.
    forced_outer_h_hard: bool,
    // `shrink_to_fit`: quando true, um bloco SEM `width` explícito dimensiona pela
    // largura do CONTEÚDO (como `inline-block`/item flex), não ocupa a largura
    // disponível. É o que faz badges num container horizontal não esticarem para a
    // linha toda. No fluxo vertical normal é false (block ocupa a largura — MDN).
    shrink_to_fit: bool,
    // O bloco de formatação AMBIENTE, do antepassado que o estabeleceu — não
    // necessariamente o pai imediato. Ignorado se `id` estabelece o SEU
    // PRÓPRIO BFC (um novo é criado abaixo, em `children_bfc`). Ver `block/bfc.rs`.
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    crate::bump!(block_calls);
    // Nós não-elemento no nível de bloco (texto solto, comentário): trata o texto
    // como uma linha; comentário não pinta.
    let css = match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            // Metadata não-renderável (`<head>` e seu conteúdo, `<style>`,
            // `<script>`): pula a subárvore inteira — não pinta nada. Permite
            // carregar um HTML COMPLETO (com <head><title><meta>) e renderizar só
            // o que é visível (o <body> e seus filhos).
            if is_non_rendered_tag(tag) {
                return (0.0, 0.0);
            }
            let css = dom.computed_style_idx(id).unwrap_or_default();
            // `display:none` — não renderiza nem ocupa espaço (some da árvore visual).
            if is_display_none(dom, id) {
                return (0.0, 0.0);
            }
            if tag == "select" {
                return layout_select(
                    box_id, &css, x, y, avail_w, avail_h, forced_outer_w,
                    forced_outer_h, ctx, list,
                );
            }
            // `<input>`/`<textarea>` editável (mini-browser): void, sem filhos — o
            // "conteúdo" é o texto do value/placeholder + cursor. Caminho próprio,
            // fora do fluxo de bloco genérico (que desceria em filhos inexistentes).
            if is_text_input_tag(tag) {
                let itype = dom
                    .node(id)
                    .attr("type")
                    .map(|t| t.to_ascii_lowercase())
                    .unwrap_or_default();
                // `type=hidden`: invisível e sem espaço (o form legado do google
                // tem 5 — viravam caixas de texto fantasmas).
                if itype == "hidden" {
                    return (0.0, 0.0);
                }
                // `type=submit/button/reset`: BOTÃO — caixa cinza UA com o value
                // como rótulo (não editável). O suficiente p/ o "Pesquisa Google".
                if matches!(itype.as_str(), "submit" | "button" | "reset") {
                    return layout_button(dom, id, box_id, &css, x, y, forced_outer_h, ctx, list);
                }
                return layout_input(
                    dom,
                    id,
                    box_id,
                    &css,
                    x,
                    y,
                    avail_w,
                    avail_h,
                    forced_outer_w,
                    forced_outer_h,
                    ctx,
                    list,
                );
            }
            // `<img>` com pixels decodificados: emite a imagem no rect (tamanho do CSS
            // width/height, senão o natural da imagem). Void — sem filhos.
            // `<canvas>`: elemento REPLACED cujo conteúdo é uma superfície de
            // pixels. A caixa vem dos atributos `width`/`height` (ou do CSS), e
            // o desenho aparece quando o programa pinta — antes disso a caixa
            // existe e fica vazia, que é o que o browser também faz.
            // The replaced element's `height: %` basis: its parent's height, kept
            // only when the parent declares one (`definite_cb_height`).
            let replaced_cb_h = dom
                .node(id)
                .parent
                .and_then(|p| dom.computed_style_idx(p))
                .and_then(|pcss| crate::inline_box::replaced_clamp::definite_cb_height(&pcss, avail_h));
            if tag == "canvas" {
                if let Some(r) = layout_canvas(dom, id, box_id, &css, x, y, avail_w, replaced_cb_h, ctx, list) {
                    return r;
                }
            }
            if tag == "img" {
                if let Some(img) =
                    layout_image(dom, id, box_id, &css, x, y, avail_w, replaced_cb_h, forced_outer_w, forced_outer_h, ctx, list)
                {
                    return img;
                }
                // sem pixels ainda (não baixou/decodificou): ocupa 0 (não pinta nada).
            }
            // `<svg>` é um REPLACED element: não desenhamos o vetor, mas RESERVAMOS
            // a caixa (dimensões do CSS width/height, dos atributos, ou da razão do
            // `viewBox`) e pintamos um placeholder cinza — assim a estrutura da
            // página fica correta mesmo sem o SVG (logo/ícones do google ocupam o
            // espaço certo em vez de colapsar pra 0×0).
            if tag == "svg" {
                if let Some(r) = layout_svg_placeholder(dom, id, box_id, &css, x, y, avail_w, ctx, list) {
                    return r;
                }
            }
            css
        }
        // Texto solto ao nível de bloco: uma linha com a fonte do PAI
        // (`bare_text.rs`); whitespace estrutural não cria linha nenhuma.
        NodeKind::Text(t) => return super::bare_text::layout_bare_text(dom, id, t, x, y, ctx, list),
        _ => return (0.0, 0.0), // Comment / Document aninhado: não pinta.
    };

    // ── Box model (content-box): resolve as bordas/espaços absolutos ─────────────
    // O contexto de RESOLUÇÃO tardia primeiro (margens/paddings agora aceitam
    // unidades relativas — `p-3` = 1rem do Bootstrap — e resolvem AQUI, como width).
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font_px(&css, DEFAULT_FONT_SIZE),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // Margin/padding POR LADO (Edges). O `margin_v` (UA-stylesheet, só vertical) é
    // somado ao top/bottom. Margens são SIGNED (negativa puxa — gutters `.row`);
    // padding é clampado ≥ 0 (padding negativo não existe no CSS).
    let m = &css.margin;
    let p = &css.padding;
    let mut margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let mut margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    // margin_v (UA-stylesheet) só vale no lado que o AUTOR NÃO declarou — um
    // `margin-top: 0` explícito ANULA o default da UA naquele lado (era o brand
    // do cover descendo 16px apesar do `h3 { margin-top: 0 }` do Bootstrap).
    let margin_v_extra = css.margin_v.unwrap_or(0.0);
    let mv_top = if m.top == crate::style::Side::Unset {
        margin_v_extra
    } else {
        0.0
    };
    let mv_bottom = if m.bottom == crate::style::Side::Unset {
        margin_v_extra
    } else {
        0.0
    };
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0) + mv_top;
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0) + mv_bottom;
    // RECUO DA LISTA: `<ul>`/`<ol>` (e `<menu>`/`<dir>`) trazem
    // `padding-inline-start: 40px` de `style/ua.css` (lote I) — uma regra CSS
    // normal na origem UA, como qualquer outra, e não mais uma função à parte
    // (`ua_list_indent`, apagada) chamada depois do `padding` resolvido. Um
    // `padding-left` do autor já a vence pela CASCADE, antes de chegar aqui.
    let pad_left = p.left.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_right = p.right.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_top = p.top.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_bottom = p.bottom.resolve(&resolve).unwrap_or(0.0).max(0.0);
    // BORDA POR LADO: as larguras USADAS (um lado com `border-style: none` vale
    // zero, por mais que declare largura — ver `style::borders::used_widths`).
    // Era um ESCALAR `css.border_width` aplicado aos quatro lados, e isso não é
    // uma simplificação: `border-bottom: 5px` alargava a caixa nos quatro lados
    // ou em nenhum. Medido no corpus, era o maior desvio de uma fixture só —
    // `claude-border-lados`, 15 de 82.
    let [border_top, border_right, border_bottom, border_left] =
        crate::style::borders::used_widths(&css);
    // Atalhos para o eixo (horizontal = left+right): a maioria do box model usa o
    // total por eixo. (`margin_h`/`padding_h` = soma do eixo horizontal.)
    let border_h = border_left + border_right;
    let border_v = border_top + border_bottom;
    let margin_h = margin_left + margin_right;
    let padding_h = pad_left + pad_right;
    // `frame` horizontal = o que cerca o content no eixo X (margin+border+padding
    // dos DOIS lados); cada termo já é a soma do seu eixo.
    let frame = margin_h + border_h + padding_h;
    let font_for_content = font_px(&css, DEFAULT_FONT_SIZE);
    let border_box = css.border_box.unwrap_or(false);
    // O PAPEL da caixa (item de lista, parte de tabela) — decidido já aqui e não
    // junto do eixo dos filhos porque a `<table>` muda a resolução da SUA PRÓPRIA
    // largura, três linhas abaixo.
    let used = used_display(dom, id);
    // Uma `<table>` sem `width` é SHRINK-TO-FIT: encolhe ao conteúdo em vez de
    // ocupar o pai. É a diferença mais visível entre uma tabela e um `<div>`, e
    // sem ela cada tabela da página nasce com a largura da coluna inteira.
    let shrink_to_fit = shrink_to_fit || used.is_some_and(crate::style::DisplayKind::is_table_box);
    let content_w = if let Some(fw) = forced_outer_w {
        // main size do FLEX (grow/shrink já resolvidos): outer imposto → content =
        // outer - frame (o frame já soma margem+borda+padding dos dois lados).
        // Vence width/min-max (o clamp no resolve flex é corte documentado).
        (fw - frame).max(0.0)
    } else {
        // `width: max-content` — a largura que o conteúdo PEDE, e nada a limita.
        //
        // É essa a diferença face ao shrink-to-fit logo abaixo, que é a mesma
        // medição com `.min(disponível)` por cima: `max-content` transborda de
        // propósito, o shrink-to-fit cede. Usar o ramo do shrink-to-fit seria
        // dar a resposta certa por acaso sempre que coubesse, e a errada quando
        // é precisamente o caso que a palavra-chave existe para exprimir.
        //
        // Sem esta linha, `width:max-content` chegava aqui como `None` (o parse
        // descartava-o) e o elemento tomava a largura DO PAI: o painel do menu da
        // Wikipédia media 56,2 onde o Chrome dá 198,6, e tudo lá dentro herdava
        // o estrangulamento — 135 `<li>` a quebrar em linhas a mais.
        //
        // O `box-sizing` não entra: o valor medido JÁ é o conteúdo, ao contrário
        // de um `width` declarado, onde `border-box` manda descontar o frame.
        // Descontá-lo aqui tirava padding a um número que nunca o incluiu.
        //
        // O clamp de `min`/`max-width` abaixo continua a morder por cima, como
        // manda a spec — e neste elemento o `max-width:200px` da mesma regra NÃO
        // morde: o conteúdo pede 166,6. Se um dia bater nos 200, é sinal de que
        // esta medição passou a calcular a mais.
        let base = if css.width == Some(crate::style::Dimension::MaxContent) {
            crate::layout::flex::column_wrap_width::max_content_width(dom, id, box_id, font_for_content, avail_h, &css, ctx)
        } else {
            match css.width.and_then(|d| d.resolve_family(&resolve, css.font_family.as_deref())) {
                // `width` explícito. Em `border-box`, o `width` INCLUI padding+border —
                // então o content é `width - (padding_h + 2*border)`. Em content-box
                // (default), o `width` JÁ é o content.
                Some(w) if border_box => (w - (padding_h + border_h)).max(0.0),
                Some(w) => w,
                // Sem width: shrink-to-fit → largura do conteúdo (com o piso
                // de min-content e o tecto do disponível, CSS2 §10.3.5 —
                // `crate::layout::flex::limits::shrink_to_fit_width`, extraída para não
                // crescer este ficheiro); senão (fluxo block normal) →
                // ocupa a largura disponível.
                //
                // `transferred_intrinsic_width` decide PRIMEIRO quando um
                // `<img>` sem tamanho lá dentro pesa pela razão×altura
                // esticada em vez do natural (`transferred_size.rs`) —
                // `None` em qualquer outro caso, e cai no shrink-to-fit de
                // sempre.
                None if shrink_to_fit => {
                    let h = forced_outer_h
                        .map(|h| (h - pad_top - pad_bottom - border_top - border_bottom).max(0.0));
                    crate::layout::replaced::transferred_size::transferred_intrinsic_width(dom, id, font_for_content, h, ctx)
                        .unwrap_or_else(|| {
                            crate::layout::flex::limits::shrink_to_fit_width(
                                dom, id, (avail_w - frame).max(0.0), frame, font_for_content, ctx,
                            )
                        })
                }
                None => (avail_w - frame).max(0.0),
            }
        };
        // CLAMP min/max-width (#1751): `used = clamp(min, width, max)`. min/max são
        // sobre a CAIXA (border-box) na spec — descontamos o frame p/ aplicar ao
        // content quando border-box; em content-box já são do content.
        let mnw = crate::layout::measure::intrinsic_min_max::resolve(css.min_width, dom, id, font_for_content, ctx, &resolve).map(|v| {
            if border_box {
                (v - (padding_h + border_h)).max(0.0)
            } else {
                v
            }
        });
        let mxw = crate::layout::measure::intrinsic_min_max::resolve(css.max_width, dom, id, font_for_content, ctx, &resolve).map(|v| {
            if border_box {
                (v - (padding_h + border_h)).max(0.0)
            } else {
                v
            }
        });
        crate::style::clamp_size(base, mnw, mxw)
    };

    // `margin: 0 auto` (#1745): se o margin-left/right é `auto` E o bloco tem largura
    // definida (não ocupa o pai inteiro), o espaço livre se distribui pelos lados
    // auto — centralizando (ambos auto) ou empurrando (um só auto). Resolvido AQUI,
    // depois de saber o content_w. Só quando há largura explícita (senão o bloco já
    // ocupa avail_w e não há espaço a distribuir).
    //
    // UM FLOAT NUNCA CENTRALIZA (CSS2.1 §10.3.5): o valor usado de
    // `margin-left`/`margin-right` quando `auto` é ZERO, não uma fração do
    // espaço livre — a regra de distribuição acima é para blocos não-flutuantes
    // no fluxo normal. Sem esta guarda, `float:left; margin-left:auto` central-
    // izava o quadrado em vez de o colar no canto superior-esquerdo do
    // container (WPT `floats-clear/float-non-replaced-width-001` e
    // `float-replaced-width-001`, ambos com `n` idêntico — a mesma causa).
    let is_float = crate::layout::float::float::float_of(dom, id) != crate::style::FloatSide::None;
    let has_width = css.width.is_some() || css.max_width.is_some();
    if is_float {
        if m.left.is_auto() {
            margin_left = 0.0;
        }
        if m.right.is_auto() {
            margin_right = 0.0;
        }
    } else if has_width {
        let box_outer = content_w + padding_h + border_h; // sem a margin
        // COM SINAL (não `.max(0.0)`): o ramo `direction:rtl` de
        // `rtl::used_margin_left` precisa do valor negativo quando o
        // filho é mais largo do que o disponível — ver o módulo.
        let signed_free = avail_w - box_outer;
        let free = signed_free.max(0.0);
        match (m.left.is_auto(), m.right.is_auto()) {
            (true, true) => {
                margin_left = free / 2.0;
                margin_right = free / 2.0;
            }
            (true, false) => margin_left = (free - margin_right).max(0.0),
            (false, true) => margin_right = (free - margin_left).max(0.0),
            (false, false) => {
                margin_left =
                    super::rtl::used_margin_left(dom, id, margin_left, margin_right, signed_free);
            }
        }
    }

    // Posição do content-box (canto sup-esq): deslocado pelo lado ESQUERDO/TOPO
    // (margin+border+padding daquele lado), não a soma do eixo.
    let content_x = x + margin_left + border_left + pad_left;
    // MARGIN-COLLAPSE PAI→PRIMEIRO-FILHO — porquê em `escaped_margin.rs`.
    let escaped_top_pre = crate::layout::block::escaped_margin::escaped_at_top(
        dom, id, &css, content_w, font_for_content, pad_top, border_top, ctx,
    );
    let content_y =
        y + (collapse_margin(margin_top, escaped_top_pre) - escaped_top_pre) + border_top + pad_top;

    // Z-ORDER: o fundo/borda da caixa precisam ficar ATRÁS dos filhos. Como a
    // display list é pintada em ordem, lembramos AGORA a posição onde a caixa será
    // inserida (antes de qualquer filho), descemos nos filhos (que dão append no
    // fim), e só DEPOIS — conhecendo a altura — inserimos o fundo nessa posição.
    // Nothing emitted before it can move (descendants only append after it), so
    // the position stays true and an insert corrects nothing (`pieces.rs`).
    let box_start = list.pieces.len();
    // Reserva a posição do pai antes dos filhos; a geometria final é preenchida
    // depois que a altura natural do conteúdo for conhecida.
    reserve_box_order(list, box_id);

    // ── Filhos: o EIXO depende do `display` do bloco ─────────────────────────────
    // vertical (default): cada filho ABAIXO do anterior, ocupando a largura.
    // horizontal (`display:horizontal`/flex-row): cada filho À DIREITA do anterior,
    // a altura do content = a do filho mais alto (MDN flow: inline-axis stacking).
    let display = css_display(dom, id);
    let font_size = font_px(&css, DEFAULT_FONT_SIZE);

    // SCROLL CONTAINER (#1744): uma div com `overflow-x:auto/scroll` NÃO comprime os
    // filhos — eles transbordam e a div rola. Nesse caso layoutamos os filhos com a
    // largura NATURAL do conteúdo (intrinsic), não a do container. (overflow-y já não
    // comprime: o vertical empilha e a altura é a soma — só precisamos do clip+barra.)
    let ov_x_declared = css
        .overflow_x
        .unwrap_or(crate::scrollbar::Overflow::Visible);
    let ov_y_declared = css
        .overflow_y
        .unwrap_or(crate::scrollbar::Overflow::Visible);
    // CSS Overflow 1 §3: se só UM eixo é `visible` e o outro não, o `visible`
    // COMPUTA como `auto` — não fica um eixo aberto. `#so-x` de
    // `claude-overflow.html` (`overflow-x:hidden;overflow-y:visible`) é
    // exatamente este caso: o Chrome recorta os DOIS eixos (a régua de
    // pintura mediu: sem esta regra o nosso lado deixava a coluna Y aberta e
    // divergia 1,95% onde deveria bater). Sem a marca `visible` PURA (os
    // dois iguais) a exceção não se aplica — só quando os eixos DIVERGEM.
    let mixed = ov_x_declared != ov_y_declared;
    let visible_becomes_auto = |o: crate::scrollbar::Overflow| {
        if mixed && o == crate::scrollbar::Overflow::Visible {
            crate::scrollbar::Overflow::Auto
        } else {
            o
        }
    };
    let ov_x = visible_becomes_auto(ov_x_declared);
    let ov_y = visible_becomes_auto(ov_y_declared);
    // `scroll_children_width` (overflow_viewport.rs, tecto): decide se os
    // filhos recebem a largura NATURAL (sem comprimir — #1744) e se isso
    // ainda vale quando `flex-wrap:wrap` precisa de saber ONDE quebrar linha
    // (`flexbox-overflow-horiz-004`/`-005` do WPT — `overflow:hidden` não
    // rola, então não há "deixar transbordar" para ele: a linha quebra na
    // largura declarada e o excesso é só recortado na pintura).
    let scroll_children_w =
        overflow_viewport::scroll_children_width(dom, id, font_size, ctx, display, ov_x, content_w);
    let children_w = content_w;

    // `height` EXPLÍCITO resolve ANTES dos filhos (não depende deles): eles o
    // recebem como containing-block height (base do `height:%` deles), e o flex
    // COLUMN o usa como referência do eixo principal (justify/margin-auto).
    let frame_v = pad_top + pad_bottom + border_v;
    // Fecha o par com o `forced_outer_w` de `content_w` (linha 468): lá o
    // override é sempre incondicional porque não existe "stretch mole" no
    // eixo horizontal a proteger. Aqui existe (o align-items:stretch do
    // flex-row/grid, que nunca corta um item mais alto que a linha), então o
    // MAIN SIZE de coluna (`forced_outer_h_hard`) precisa de entrar ANTES —
    // e sem passar pelo `.max(content_h)` mais abaixo, que é a parte que
    // protege o stretch e que o encolhimento precisa de ignorar.
    let explicit_content_h = if forced_outer_h_hard {
        forced_outer_h.map(|oh| {
            let mv = margin_top + margin_bottom;
            (oh - mv - frame_v).max(0.0)
        })
    } else {
        resolve_height(css.height, avail_h, &resolve)
            .map(|h| {
                if border_box {
                    (h - frame_v).max(0.0)
                } else {
                    h
                }
            })
            // `aspect-ratio`: sem height explícito, a altura vem da largura / razão. Só
            // quando há largura resolvida (content_w) e uma razão > 0.
            .or_else(|| {
                css.aspect_ratio
                    .filter(|r| *r > 0.0)
                    .map(|r| (content_w / r).max(0.0))
            })
            // ALTURA IMPOSTA pelo flex (grow/stretch): o `forced_outer_h` é a altura
            // OUTER do item — o content-box é ela menos margem-v/frame. Vira o
            // containing block dos filhos (um filho `height:100%` resolve contra ela),
            // resolvendo o logo/caixa do google que crescem via flex-grow vertical.
            .or_else(|| {
                forced_outer_h.map(|oh| {
                    let mv = margin_top + margin_bottom;
                    (oh - mv - frame_v).max(0.0)
                })
            })
    };

    // Altura que serve de CONTAINING BLOCK aos filhos (`height:%`): o height
    // explícito, senão um `max-height` conhecido (o Google dá ao container do
    // logo `height:calc(100% - 560px); max-height:290px` — o max é a altura
    // efetiva; sem isso o filho `height:100%` resolvia contra o conteúdo e
    // inflava). Calculado ANTES de layoutar os filhos (a resolução do `%` do
    // filho é top-down; a spec exige o CB conhecido).
    let mnh_pre = resolve_height(css.min_height, avail_h, &resolve).map(|v| {
        if border_box {
            (v - frame_v).max(0.0)
        } else {
            v
        }
    });
    let mxh_pre = resolve_height(css.max_height, avail_h, &resolve).map(|v| {
        if border_box {
            (v - frame_v).max(0.0)
        } else {
            v
        }
    });
    // `min-height` conta como altura DEFINIDA para o `align-items:stretch` dos
    // filhos flex e para o `height:%` dos netos (CSS Flexbox §4.1, "Definite
    // and Indefinite Sizes") quando o conteúdo não a alarga — o caso comum de
    // um contentor `min-height`-only, sem `height`. Não distinguimos aqui se o
    // conteúdo VAI exceder o mínimo (isso só se sabe DEPOIS de layoutar os
    // filhos, e é exatamente a circularidade que a nota da spec existe para
    // evitar): tratamos `min-height` como definida sempre que `height` não o
    // é, o mesmo corte que `mxh_pre` já fazia ao lado. Sem isto, `#item h=0`
    // e `#neto h=0` (`claude-flex-definite-min-height`) — o `avail_children`
    // nunca incluía `mnh_pre`, calculado três linhas acima e descartado.
    let avail_children = explicit_content_h.or(mxh_pre).or(mnh_pre);
    // O limiar de QUEBRA do `flex-wrap` numa coluna é uma pergunta MAIS
    // ESTREITA do que `avail_children`: precisa de um main size que o
    // CONTEÚDO não vai alargar — `height`/`max-height` são isso (um deles
    // sendo o "genuinamente definido" que `avail_children` já mistura para
    // %/stretch dos filhos); `min-height` NÃO é — é só um PISO, o container
    // cresce à vontade acima dele, e é exatamente o que o "conteúdo" de uma
    // coluna com wrap está livre para fazer. Achado do lote
    // `flex-column-wrap` (merge com `flex-justify-logico`, régua central):
    // `.item{min-height:0}` sendo ele próprio `display:flex;flex-direction:
    // column;flex-wrap:wrap` (WPT `flexbox-flex-basis-content-004a/b`, o
    // `innerFlex` com `flex-wrap:wrap` inline) fazia `avail_children` valer
    // `Some(0.0)` — um limiar de wrap DEGENERADO onde o 2.º item de
    // QUALQUER coluna já não cabe, abrindo uma coluna nova por item (3 itens
    // ficavam 3 colunas de 1, lado a lado, em vez de uma pilha vertical de
    // 3). `layout_children_column`/`column_wrap.rs` continuam a receber
    // `avail_children` para tudo o resto (gap%, `height:%` dos netos,
    // grow/shrink) — só o DESPACHO do wrap lê este valor mais estreito.
    let wrap_definite_h = explicit_content_h.or(mxh_pre);

    // Novo BFC (fresco, vazio) só se `id` o estabelece — senão os filhos
    // recebem a mesma referência ambiente, e um float lá dentro alcança os
    // IRMÃOS do antepassado que a possui. Ver `block/bfc.rs`.
    let establishes_bfc = establishes_block_formatting_context(dom, id, &css);
    let own_bfc = establishes_bfc.then(BlockFormattingContext::new);
    let children_bfc = own_bfc.as_ref().unwrap_or(bfc);

    // `flex-direction: column` — o eixo PRINCIPAL do flex vira o vertical: os itens
    // empilham (sem margin-collapse, que flex não tem), gap/justify/margin-auto
    // atuam no Y e align-items no X (stretch = ocupar a largura, o default).
    // `is_column` é o eixo FÍSICO e não a keyword crua — `writing-mode` troca
    // qual eixo lógico é X e qual é Y (`crate::layout::flex::axes::main_on_y_axis`): um
    // `row` VERTICAL é o eixo inline, que aí é o Y, e desce por
    // `layout_children_column` como se fosse `column` (e vice-versa).
    let wm = css.writing_mode.unwrap_or_default();
    let dir = css.direction.unwrap_or_default();
    let is_column_kw = css.flex_direction.map(|f| f.is_column()).unwrap_or(false);
    let is_column = crate::layout::flex::axes::main_on_y_axis(wm, is_column_kw);
    // `row-reverse`/`column-reverse` × o sentido FÍSICO do eixo que ficou
    // principal (`crate::layout::flex::axes::effective_reverse`), nunca o do eixo original da
    // keyword — que já pode não ser mais o principal. Aplicado DEPOIS do
    // `order` (spec §5.1) — cada função inverte a lista já ordenada por
    // `order`, o que é equivalente a inverter a atribuição de posições no
    // eixo principal.
    let is_reverse_kw = css
        .flex_direction
        .map(|f| {
            matches!(
                f,
                crate::style::FlexDirection::RowReverse | crate::style::FlexDirection::ColumnReverse
            )
        })
        .unwrap_or(false);
    let is_reverse = crate::layout::flex::axes::effective_reverse(wm, dir, is_column, is_reverse_kw);
    let is_flex =
        display == crate::block::DISPLAY_HORIZONTAL || display == crate::block::DISPLAY_WRAP;
    // `gap`/`row-gap` seguem a KEYWORD, nunca o eixo físico que `writing-mode`
    // troca (CSS Box Alignment §12.2 + Flexbox §8.1: "row-gap" é o espaço
    // entre as LINHAS do flex e "column-gap" entre os itens de uma linha —
    // "linha"/"coluna" aqui são o que `flex-direction:row`/`column` definem,
    // não X/Y físicos). `row.rs` (roda o eixo físico X) e `column.rs` (roda
    // o Y) continuam a ler `gap`=principal/`row_gap`=cruzado e
    // `row_gap`=principal/`gap`=cruzado, respetivamente — os papéis que já
    // tinham antes deste lote. Quando o DESPACHO físico diverge da keyword
    // (`is_column != is_column_kw`, que só acontece em `writing-mode`
    // vertical — `main_on_y_axis` troca-o), o algoritmo que corre é o do
    // eixo físico ERRADO para os nomes que já lê; troca-se os dois campos
    // aqui, uma vez, para o algoritmo continuar a ler o nome que já lia e
    // acertar mesmo assim (achado pelo WPT `gap-*-lr/rl/rtl` e
    // `flexbox-column-row-gap-002/004`: sem isto, um `flex-direction:column`
    // vertical passava o `gap` do documento — pensado para as SUAS colunas —
    // a `row.rs`, que o lê como o espaço entre ITENS de uma linha).
    let css = if is_flex && is_column != is_column_kw {
        let mut c = (*css).clone();
        std::mem::swap(&mut c.gap, &mut c.row_gap);
        std::rc::Rc::new(c)
    } else {
        css
    };
    let content_h = match display {
        // flex column: sem wrap empilha numa coluna; COM wrap (e altura
        // definida) `layout_children_column` delega para `column_wrap.rs` —
        // ver o comentário no parâmetro `wrap` lá.
        _ if is_flex && is_column => layout_children_column(
            dom,
            box_id,
            content_x,
            content_y,
            children_w,
            avail_children,
            &css,
            font_size,
            is_reverse,
            display == crate::block::DISPLAY_WRAP,
            wrap_definite_h,
            ctx,
            list,
        ),
        // horizontal (flex-row sem wrap): lado a lado, encolhe pra caber, não quebra.
        d if d == crate::block::DISPLAY_HORIZONTAL => layout_children_horizontal(
            dom,
            id,
            box_id,
            content_x,
            content_y,
            scroll_children_w,
            avail_children,
            &css,
            font_size,
            false,
            None,
            is_reverse,
            ctx,
            list,
        ),
        // GRID REAL: track-sizing (px/fr/auto/%) + auto-placement row-by-row +
        // alinhamento de célula (align-items/justify-items). Só quando é
        // `display:grid` de fato; senão o wrap horizontal (inline-block flow).
        // TABELA: a grade inteira é construída antes de posicionar o que quer que
        // seja (a largura de uma célula vem da COLUNA, não dela). Fica antes do
        // grid porque uma `<table>` que o autor não tocou tem eixo vertical e
        // cairia no empilhamento de blocos, descendo por `<tr>` como se fossem
        // `<div>` — que é exatamente o que a página real mostrava.
        _ if used.is_some_and(crate::style::DisplayKind::is_table_box) => crate::table::layout_table(
            dom,
            id,
            box_id,
            content_x,
            content_y,
            children_w,
            // Same definiteness the table's OWN children (`avail_children`,
            // three lines above) already resolve `height:%` against: only an
            // explicitly specified height (or its `aspect-ratio`/flex
            // equivalents), never the auto result of stacking the rows. A
            // `<tr>`/`<tbody>`'s `top:100%` (CSS 2.1 §9.3.2) reuses this same
            // value rather than the laid-out row height — see
            // `table::relative`.
            explicit_content_h,
            &css,
            font_size,
            ctx,
            list,
        ),
        _ if css.effective_display().is_some_and(crate::style::DisplayKind::is_grid_container) => {
            layout_children_grid(
                dom,
                id,
                box_id,
                content_x,
                content_y,
                children_w,
                avail_children,
                &css,
                font_size,
                ctx,
                list,
            )
        }
        // wrap (inline-block flow): lado a lado E QUEBRA linha quando enche.
        d if d == crate::block::DISPLAY_WRAP => layout_children_horizontal(
            dom,
            id,
            box_id,
            content_x,
            content_y,
            scroll_children_w,
            avail_children,
            &css,
            font_size,
            true,
            None,
            is_reverse,
            ctx,
            list,
        ),
        // vertical (block): empilha.
        _ => {
            // A caixa chegou do chamador. O fluxo vertical precisa dela para
            // cortar a sequência da árvore quando o nó gerou vários fragmentos.
            layout_children_vertical(
                dom,
                id,
                box_id,
                content_x,
                content_y,
                children_w,
                avail_children,
                &css,
                font_size,
                children_bfc,
                ctx,
                list,
            )
        }
    };
    // CSS 2.1 §10.6.7: só o BFC responsável cresce para conter os SEUS
    // floats — `own_bfc` só existe quando `id` é ele (senão é `None` e
    // este `match` não mexe em nada). `flex/grid/tabela` acima nunca
    // acrescentam floats a `own_bfc` (floats não se aplicam lá dentro).
    let content_h = match &own_bfc {
        Some(own) => match own.side_bottom(true, true) {
            Some(floats_bottom) => content_h.max((floats_bottom - content_y).max(0.0)),
            None => content_h,
        },
        None => content_h,
    };
    // MARCADOR do item de lista. Emitido DEPOIS dos filhos e com o content-box já
    // conhecido, e não desloca coisa nenhuma: `list-style-position: outside` (o
    // default, e o único que este motor desenha) põe o marcador FORA da caixa de
    // conteúdo, dentro do recuo que o `<ul>` já reservou.
    if used == Some(crate::style::DisplayKind::ListItem) {
        crate::listitem::emit_marker(dom, id, &css, content_x, content_y, font_size, ctx, list);
    }

    // a altura REAL do conteúdo (antes de `height` explícito a cortar) — p/ o scroll-Y.
    let content_h_natural = content_h;

    // CAIXA INLINE: um elemento cujo display USADO é `inline` mas que tem caixa
    // (fundo, padding, borda) NÃO tira a altura do fluxo dos filhos — tira-a da
    // FONTE, como qualquer inline. É a mesma regra da content area que o fluxo
    // inline já aplica, e faltava aqui: um `<a>` com padding num parágrafo de
    // `line-height:1.6` respondia 22,4 (a altura da LINHA) onde o browser
    // responde 16, e são milhares deles numa página real.
    //
    // Só quando o autor não declara `display` nem `height`: um `display:inline-block`
    // de facto é um contentor de blocos e a altura dele vem mesmo do conteúdo.
    // `used.is_none()`: um papel USADO (célula, linha, item de lista, tabela) já
    // não é uma caixa inline — foi o `<td>` que o mostrou, porque tem padding da
    // UA e por isso passava no teste de "inline com caixa".
    // A float or an absolutely positioned box is BLOCKIFIED (CSS 2.1 §9.7)
    // even when its `display` is only the tag's default — `effective_display`
    // cannot say so, having no tag — and a block's height is its LINES, not the
    // font's content area: a floated `<span>` is 18px tall, not 17.
    let blockified = css.float_side.is_some_and(|f| f != crate::style::FloatSide::None)
        || css.position.is_some_and(|p| p.out_of_flow());
    let inline_box_id = used.is_none()
        && css.effective_display().is_none()
        && css.height.is_none()
        && !blockified
        && is_inline_block(dom, id);
    let content_h = if inline_box_id {
        crate::inline_box::altura_do_conteudo(font_size, css.font_family.as_deref(), ctx.measurer)
    } else {
        content_h
    };
    // Botão nativo sem `height`: a linha do texto-filho não herda a altura do
    // pai (20px), usa a métrica interna do widget (~15px). O frame da UA já
    // está em `frame_v`; só substituímos o conteúdo natural, deixando o
    // `forced_outer_h` do flex atuar mais abaixo.
    let is_button = matches!(&dom.node(id).kind, crate::NodeKind::Element { tag } if tag == "button");
    let content_h = if is_button && css.height.is_none() {
        content_h.min(15.0)
    } else {
        content_h
    };
    // `height` explícito SOBRESCREVE a altura do conteúdo (a caixa tem essa altura,
    // mesmo que o conteúdo seja menor) — já resolvido antes dos filhos.
    let content_h = explicit_content_h.unwrap_or(content_h);
    // CLAMP min/max-height (#1751): used = clamp(min, height, max) — eixo vertical
    // (`%` contra a ALTURA do containing block, como o height).
    let content_h = crate::style::clamp_size(content_h, mnh_pre, mxh_pre);
    // STRETCH do flex: altura OUTER imposta pelo container (align-items/self:
    // stretch) → content = outer - margens - frame_v; nunca ENCOLHE o conteúdo
    // (max com o natural — um item mais alto que a linha não é cortado).
    // `forced_outer_h_hard` já decidiu `content_h` acima (via
    // `explicit_content_h`) e é exatamente isso que o MAIN SIZE de coluna
    // precisa: o `.max` aqui é a parte "mole" que este caller pediu para não
    // ter.
    let content_h = if forced_outer_h_hard {
        content_h
    } else {
        match forced_outer_h {
            Some(fh) => (fh - margin_top - margin_bottom - frame_v).max(content_h),
            None => content_h,
        }
    };

    // MARGIN-COLLAPSE pai/filho: sem BFC, borda ou padding, a margem da primeira
    // caixa de bloco pode escapar por cima e a da última pode escapar por baixo.
    // O cursor que o pai devolve continua a incluir o espaço colapsado, mas o
    // border-box próprio não pinta nem mede essa margem externa.
    let (escaped_top, escaped_bottom) = escaped_child_margins(
        dom,
        id,
        &css,
        content_w,
        font_size,
        ctx,
        pad_top,
        border_top,
        pad_bottom,
        border_bottom,
        explicit_content_h.is_none() && mnh_pre.is_none(),
    );
    let box_content_h = if explicit_content_h.is_none() && mnh_pre.is_none() {
        (content_h - escaped_top - escaped_bottom).max(0.0)
    } else {
        content_h
    };
    let box_top_margin = collapse_margin(margin_top, escaped_top);
    let box_bottom_margin = collapse_margin(margin_bottom, escaped_bottom);
    // ── Insere a CAIXA (fundo + borda) no índice reservado, ATRÁS dos filhos ─────
    // O BORDER-BOX do nó: content + padding + border (NÃO a margin — esta é espaço
    // externo). É o retângulo que `getBoundingClientRect()` reporta.
    let box_rect = Rect::new(
        x + margin_left,
        y + box_top_margin,
        content_w + padding_h + border_h,
        box_content_h + pad_top + pad_bottom + border_v,
    );
    // A fronteira pública agrega por nó, mas este bloco conhece a caixa exata:
    // não pode preencher com o mesmo rect os demais fragmentos do inline.
    record_box_rect(list, box_id, box_rect);

    // Pinta a CAIXA (fundo/borda) ATRÁS dos filhos. `insert` no `box_start` põe o
    // fundo antes dos itens dos filhos (z-order); `at` ends past the box items,
    // where the overflow clip below opens.
    let mut at = box_start;
    if css.has_box() {
        let radius = css.corner_radius.unwrap_or(0.0);
        // O FUNDO pinta por canto; a borda e a sombra continuam a ler o campo
        // único, e é isso que as deixa responder hoje o que respondiam ontem.
        let corners = Corners::from_style(&css, 0.0);
        // `opacity` do elemento: multiplica o ALPHA das cores próprias (fundo/borda).
        // Cobre o caso comum (card/botão/overlay com fade) sem grupo de compositing.
        // `visibility:hidden`/`collapse` zera o alpha de tudo o que ESTE
        // elemento pinta (`suppresses_paint`, style/values/texto.rs). Não
        // salta o layout: o elemento continua a ocupar o espaço dele, que é
        // exatamente o que o distingue de `display:none` — e como a propriedade
        // é herdada, os descendentes chegam aqui já com ela.
        let op = if css.visibility.is_some_and(|v| v.suppresses_paint()) {
            0.0
        } else {
            css.opacity.unwrap_or(1.0)
        };
        // `filter`: a cadeia inteira reduzida a UMA matriz de cor, uma vez por
        // elemento. Ver `painteffects` para o que é exprimível — em resumo, as
        // funções que são aritmética de canal são exatas e o `blur`/`drop-shadow`
        // recusam a cadeia toda, deixando esta matriz na identidade.
        //
        // Aplicada ANTES do `opacity` porque é essa a ordem do CSS: o filtro
        // atua sobre o elemento renderizado e a opacidade compõe o resultado.
        //
        // LIMITE, e é o mesmo que o `opacity` acima já tem: alcança as cores
        // PRÓPRIAS desta caixa (sombra, fundo, gradiente, borda) e não os
        // descendentes, que são pintados pelos seus próprios layouts. Um
        // `filter: invert(1)` numa div com texto inverte aqui o fundo e não o
        // texto. Não há grupo de compositing nesta display list onde a subárvore
        // pudesse ser filtrada como uma unidade — quando houver, é ele que passa
        // a carregar isto, e não este sítio.
        let fx = crate::painteffects::filtro(css.filter.as_deref().unwrap_or(""));
        // Compõe as duas numa função só, para que nenhum dos pontos de emissão
        // abaixo possa aplicar uma e esquecer a outra.
        let paint_color = |c: u32| apply_opacity(fx.aplicar(c), op);
        // Insere na ordem: primeiro o fundo, depois a borda por cima dele (ambos
        // atrás dos filhos).
        // SOMBRA primeiro (atrás de tudo): box-shadow.
        if let Some(sh) = css.box_shadow {
            list.pieces.insert(
                at,
                Piece::Item(DisplayItem::Shadow {
                    rect: box_rect,
                    dx: sh.dx,
                    dy: sh.dy,
                    blur: sh.blur,
                    spread: sh.spread,
                    color: paint_color(sh.color),
                    radius,
                }),
            );
            at += 1;
        }
        // FUNDO: gradiente (se houver) OU cor sólida — a menos que uma MÁSCARA
        // dê a forma da caixa (ver `deve_suprimir_fundo`).
        let paints_background = !deve_suprimir_fundo(&css);
        // A cor de fundo começa no border-box por omissão, mas `content-box`
        // exclui padding e borda. O rect já calculado usa a caixa final (após
        // min/max e flex), portanto não volta a resolver percentagens aqui.
        let background_rect = match css.background_clip {
            Some(crate::style::painting::BackgroundClip::ContentBox) => Rect::new(
                box_rect.x + border_left + pad_left,
                box_rect.y + border_top + pad_top,
                content_w,
                box_content_h,
            ),
            Some(crate::style::painting::BackgroundClip::PaddingBox) => Rect::new(
                box_rect.x + border_left,
                box_rect.y + border_top,
                content_w + padding_h,
                box_content_h + pad_top + pad_bottom,
            ),
            _ => box_rect,
        };
        if let Some(g) = css.gradient.filter(|_| paints_background) {
            list.pieces.insert(
                at,
                Piece::Item(DisplayItem::GradientRect {
                    rect: background_rect,
                    c0: paint_color(g.c0),
                    c1: paint_color(g.c1),
                    angle_deg: g.angle_deg,
                    radius,
                }),
            );
            at += 1;
        } else if let Some(color) = css.bg.filter(|_| paints_background) {
            let color = paint_color(color);
            list.pieces.insert(
                at,
                Piece::Item(DisplayItem::SolidRect {
                    rect: background_rect,
                    color,
                    radius: corners,
                }),
            );
            at += 1;
        }
        // IMAGEM DE FUNDO: sobre a cor/gradiente, atrás da borda — os pixels
        // (quando já carregados; ver `fundo_imagem`) tapam o que a cor pintou,
        // exatamente como o Blink compõe as camadas de `background`.
        for item in crate::paint::background_image::background_pixels_items(
            dom, id, &css, box_rect, border_top, border_right, border_bottom, border_left,
        ) {
            list.pieces.insert(at, Piece::Item(item));
            at += 1;
        }
        for item in border_items(&css, box_rect, radius, op, fx) {
            list.pieces.insert(at, Piece::Item(item));
            at += 1;
        }
    }

    // ── SCROLL CONTAINER interno (#1744): se a div rola (overflow-x/y) e o conteúdo
    // excede a caixa, (1) RECORTA os itens dos filhos ao content-box (BeginClip já
    // emitido depois da caixa, EndClip no fim), (2) registra a ScrollRegion p/ o
    // backend gerenciar o offset + pintar as barras. `hidden` também recorta (corta o
    // excesso, sem barra). `visible` não faz nada (transborda, como hoje).
    let clips =
        ov_x != crate::scrollbar::Overflow::Visible || ov_y != crate::scrollbar::Overflow::Visible;
    if clips {
        // Os dois eixos recortam sempre que `clips` é verdade: a regra
        // `visible_becomes_auto` acima já garante que um `visible` sozinho
        // (`overflow-x:hidden;overflow-y:visible`, o caso `so-x` de
        // `claude-overflow.html`) nunca chega aqui — computou como `auto`, e
        // `ov_x`/`ov_y` já refletem isso. Um único `BeginClip` retangular
        // basta, sem eixo aberto.
        let content_rect = Rect::new(
            content_x,
            box_rect.y + border_top + pad_top,
            content_w,
            box_content_h,
        );
        // BeginClip onde os FILHOS começam — `at`, just past the box items
        // inserted above; EndClip no fim. It used to RECOUNT the box items by a
        // second rule, and the two disagreed for a `mask-image` box (the
        // background counted, not emitted): the clip opened one item late, or
        // past the end of the list and panicked.
        // O offset vem do `Dom` (`dom/scroll.rs`) — não é escrito aqui de
        // propósito, só LIDO: quem rola é o backend, respondendo a input, e o
        // layout nunca recebe `&mut Dom` (ver a auditoria estrutural). Este
        // valor é só o "como estava quando o fragmento foi montado" — nem a
        // pintura nem uma consulta de geometria confiam nele (as duas voltam
        // a perguntar ao `Dom` o valor VIVO); ver a nota de topo de
        // `dom/scroll.rs` sobre por que scroll nunca invalida este cache.
        let (offset_x, offset_y) = dom.scroll_of_idx(id);
        list.pieces.insert(
            at,
            Piece::Item(DisplayItem::BeginClip {
                rect: content_rect,
                node: id,
                offset_x,
                offset_y,
            }),
        );
        list.pieces.push(Piece::Item(DisplayItem::EndClip));
        // só registra como rolável (com barra) se de fato rola (auto/scroll), não hidden.
        if ov_x.scrollable() || ov_y.scrollable() {
            list.scroll_regions.push(ScrollRegion {
                node_idx: id,
                visible: content_rect,
                content_w: scroll_children_w.max(content_w),
                content_h: content_h_natural,
                overflow_x: ov_x,
                overflow_y: ov_y,
            });
        }
    }

    // `clip-path` — SÓ `inset()` sem `round` chega a devolver um rect (ver
    // `painteffects`) — e o `clip` legado (CSS2.1 §11.1.2), que só se aplica a
    // posicionados (`absolute`/`fixed`, a condição da spec) e reusa o MESMO
    // par BeginClip/EndClip: dois retângulos possíveis, o mesmo emissor.
    let clip_path_rect = css
        .clip_path
        .as_deref()
        .and_then(|cp| crate::painteffects::clip_retangulo(cp, box_rect));
    let legacy_clip_rect = css
        .position
        .is_some_and(|p| p.out_of_flow())
        .then(|| css.clip)
        .flatten()
        .and_then(|clip| crate::painteffects::clip_legacy_retangulo(clip, box_rect));
    for rect in [clip_path_rect, legacy_clip_rect].into_iter().flatten() {
        // Emitido DEPOIS do bloco de overflow acima, e de propósito: inserir em
        // `box_start` empurra tudo o que vem a partir dali, e fazê-lo antes
        // desalinharia por um o `at` que aquele bloco usa. Como
        // o `EndClip` deste é empilhado no fim, o aninhamento sai certo — este
        // abre primeiro e fecha por último, portanto envolve o clip de scroll.
        //
        // A diferença para o clip de overflow é onde ABRE: aquele recorta só
        // os FILHOS (abre depois dos itens de caixa), este recorta o elemento
        // INTEIRO, fundo e borda incluídos.
        list.pieces.insert(
            box_start,
            Piece::Item(DisplayItem::BeginClip { rect, node: id, offset_x: 0.0, offset_y: 0.0 }),
        );
        list.pieces.push(Piece::Item(DisplayItem::EndClip));
    }

    // POSITION:RELATIVE — porquê e o que desloca em `relative.rs`. ANTES do
    // `transform`: a caixa de referência dele é a posição já deslocada.
    apply_relative_offset(box_id, &css, avail_w, avail_h, font_size, box_start, ctx, list);

    // ── TRANSFORM (matriz 2D completa: matrix/translate/scale/rotate/skew,
    // compostas por `TransformList::resolve`): pós-processa os itens DESTE
    // elemento e seus descendentes (o range `[box_start..]`), em torno de
    // `transform-origin` (default `50% 50%` — CSS Transforms 1 §6). Aplicado
    // por último (não afeta o fluxo/tamanho — como no CSS, transform é visual).
    if let Some(tf) = css.transform {
        if !tf.is_identity() {
            let mat = crate::paint::transform::matriz_transform(
                tf,
                css.transform_origin,
                box_rect,
                font_size,
                ctx.viewport_w,
                ctx.viewport_h,
            );

            // `getBoundingClientRect` (`node_rects`) SEMPRE reflete a matriz —
            // a bounding box dos 4 cantos, para este nó E para cada
            // descendente (herdam a transformação do pai). Corre ANTES do
            // atalho abaixo e para os dois ramos: a bbox de um rect só
            // transladado é só transladada, a mesma chamada serve os dois.
            let from_tree = std::rc::Rc::clone(&list.tree);
            crate::layout::fragment::transform_rects::transform_box_rects(&from_tree, box_id, &mat, list);

            // Um transform MUTA itens, e um item de subárvore reusada é
            // COMPARTILHADO — mutá-lo no lugar mudaria o desenho de todo mundo
            // que aponta para ele.
            //
            // Uma matriz de TRANSLAÇÃO PURA não precisa de achatar nada: a
            // subárvore é desenhada com um deslocamento que já existe no
            // `ChildRef`, e somar ao `dx`/`dy` dele é a mesma conta sem tocar
            // no que é partilhado.
            //
            // Achatar aqui era um defeito com alcance muito além do elemento:
            // `materialize` reescrevia a lista INTEIRA, e todos os índices que os
            // ancestrais reservaram para as caixas deles passavam a apontar para
            // outro item. Um `position:absolute` com `transform:translateY(-50%)`
            // — uma regra de ícone, na folha do MediaWiki — punha a página
            // inteira da Wikipédia a zero: 16 813 elementos sem geometria porque
            // uma regra de 40 bytes casou com um `<span>`.
            let is_pure_translate = mat.a == 1.0 && mat.b == 0.0 && mat.c == 0.0 && mat.d == 1.0;
            if is_pure_translate {
                crate::paint::pieces::shift_from(&mut list.pieces, box_start, mat.e, mat.f);
            } else {
                // Escala/rotação/skew/matriz: em vez de mutar cada item por
                // aproximação (norma das colunas — a caixa continuava
                // axis-aligned, só do tamanho errado), a matriz VIAJA na
                // lista como `PushTransform`/`PopTransform` em torno de
                // `[box_start..]`. The reused subtrees in that range are
                // flattened first — and ONLY those: flattening the whole list,
                // as this did, also flattened the preceding siblings, and the
                // `PushTransform` then landed inside them (`pieces::flatten_from`).
                crate::paint::pieces::flatten_from(&mut list.pieces, box_start);
                list.pieces.insert(box_start, Piece::Item(DisplayItem::PushTransform { mat }));
                list.pieces.push(Piece::Item(DisplayItem::PopTransform));
            }
        }
    }

    // Tamanho EXTERNO da caixa (outer = content + padding + border + margin) — cada
    // componente já é a SOMA do seu eixo (padding_h = left+right; margin_h idem;
    // border conta 2× pelos dois lados). Não multiplicar margin/padding por 2.
    let outer_w = content_w + padding_h + border_h + margin_h;
    let outer_h =
        box_content_h + pad_top + pad_bottom + border_v + box_top_margin + box_bottom_margin;
    (outer_w, outer_h)
}
