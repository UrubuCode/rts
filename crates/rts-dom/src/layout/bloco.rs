//! `layout_block` — a colocação de UM bloco: caixa, margens, padding, borda, e
//! a escolha de como dispor os filhos.
//!
//! **Este módulo é uma função.** São 807 linhas, e ficam acima do teto de 500 de
//! propósito: partir uma função por dentro deixa de ser um movimento de código e
//! passa a ser uma alteração de comportamento que nenhuma régua desta arrumação
//! consegue verificar. O teto vale para ficheiros que juntam assuntos; aqui o
//! assunto é um só e tem 807 linhas. Reduzi-lo é trabalho medido, à parte.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum MarginChildRole {
    Ignore,
    Barrier,
    Block { top: f32, bottom: f32 },
}

/// `true` se `id` estabelece o seu PRÓPRIO bloco de formatação (CSS 2.1
/// §9.4.1) — hoje usado para barrar o escape de margens
/// ([`escaped_child_margins`]) E para decidir o `BlockFormattingContext` de
/// `layout_block`; ver `layout/bfc.rs` para o porquê da entidade. A raiz do
/// documento entra por `id` ser filho direto de `dom.root` — o único gatilho
/// que não está no `ComputedStyle`.
pub(in crate::layout) fn establishes_block_formatting_context(dom: &Dom, id: NodeIdx, css: &ComputedStyle) -> bool {
    let is_root = dom.node(id).parent == Some(dom.root);
    let display_bfc = matches!(
        css.effective_display(),
        Some(
            crate::style::DisplayKind::Flex
                | crate::style::DisplayKind::FlexWrap
                | crate::style::DisplayKind::InlineFlex // flex por dentro (Flexbox §4): mesmo contexto
                | crate::style::DisplayKind::InlineFlexWrap
                | crate::style::DisplayKind::Grid
                | crate::style::DisplayKind::InlineBlock
                | crate::style::DisplayKind::Table
                | crate::style::DisplayKind::TableRowGroup
                | crate::style::DisplayKind::TableRow
                | crate::style::DisplayKind::TableCell
                | crate::style::DisplayKind::TableCaption
        )
    );
    // CSS2.1 §9.4.1: "overflow" outro que não `visible` estabelece um BFC —
    // `scrollable()` (auto/scroll) OU `clips()` (hidden/clip, CSS Overflow 3;
    // `clip` entrou no lote `flex-min-auto-content`, retrabalho: antes de
    // `Overflow::Clip` existir como variante própria, `hidden`/`clip` eram a
    // MESMA e este `any` já os cobria os dois sem saber).
    let overflow_bfc = [css.overflow_x, css.overflow_y]
        .into_iter()
        .any(|value| value.is_some_and(|o| o.scrollable() || o.clips()));
    let float_bfc = css
        .float_side
        .is_some_and(|side| side != crate::style::FloatSide::None);
    let positioned_bfc = css
        .position
        .map(|position| position.out_of_flow())
        .unwrap_or(false);
    // Um ITEM de flex ou de grid estabelece o seu próprio contexto (Flexbox
    // §4, Grid §6): contém os seus floats como um `flow-root`. Sem isto o
    // `<header class="mb-auto">` do Bootstrap cover — um `float-md-start` e um
    // `float-md-end` lá dentro — media 0px onde o Blink dá 36
    // (`claude-flex-item-contem-floats`).
    let item_bfc = dom
        .node(id)
        .parent
        .and_then(|p| dom.computed_style_idx(p))
        .is_some_and(|pc| {
            matches!(
                pc.effective_display(),
                Some(
                    crate::style::DisplayKind::Flex
                        | crate::style::DisplayKind::FlexWrap
                        | crate::style::DisplayKind::InlineFlex // idem: filho de flex
                        | crate::style::DisplayKind::InlineFlexWrap
                        | crate::style::DisplayKind::Grid
                )
            )
        });
    css.flow_root.unwrap_or(false)
        || display_bfc
        || item_bfc
        || (overflow_bfc && !super::overflow_viewport::propagado_para_viewport(dom, id))
        || float_bfc
        || positioned_bfc
        || is_root
}

pub(in crate::layout) fn collapse_margin(first: f32, second: f32) -> f32 {
    if first >= 0.0 && second >= 0.0 {
        first.max(second)
    } else if first <= 0.0 && second <= 0.0 {
        first.min(second)
    } else {
        first + second
    }
}

fn margin_child_role(
    dom: &Dom,
    child: NodeIdx,
    content_w: f32,
    parent_font_size: f32,
    ctx: &LayoutCtx,
) -> MarginChildRole {
    match &dom.node(child).kind {
        NodeKind::Comment(_) => MarginChildRole::Ignore,
        NodeKind::Text(text) if text.trim().is_empty() => MarginChildRole::Ignore,
        NodeKind::Text(_) | NodeKind::Document => MarginChildRole::Barrier,
        NodeKind::Element { tag } if is_non_rendered_tag(tag) => MarginChildRole::Ignore,
        NodeKind::Element { .. } => {
            let css = dom.computed_style_idx(child).unwrap_or_default();
            if e_display_none(dom, child)
                || css
                    .position
                    .map(|position| position.out_of_flow())
                    .unwrap_or(false)
                || css
                    .float_side
                    .map(|side| side != crate::style::FloatSide::None)
                    .unwrap_or(false)
            {
                return MarginChildRole::Ignore;
            }
            let effective = css.effective_display();
            let block_candidate = match effective {
                Some(
                    crate::style::DisplayKind::Inline
                    | crate::style::DisplayKind::InlineBlock
                    | crate::style::DisplayKind::InlineFlex // inline-level por fora, idem
                    | crate::style::DisplayKind::InlineFlexWrap
                    | crate::style::DisplayKind::TableRowGroup
                    | crate::style::DisplayKind::TableRow
                    | crate::style::DisplayKind::TableCell
                    | crate::style::DisplayKind::TableCaption
                    | crate::style::DisplayKind::None,
                ) => false,
                Some(_) => true,
                None => is_block_level(dom, child) && !is_inline_block(dom, child),
            };
            if !block_candidate {
                return MarginChildRole::Barrier;
            }
            let resolve = ResolveCtx {
                parent_content_w: content_w,
                node_font_size: font_px(&css, parent_font_size),
                root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            let margin_v = css.margin_v.unwrap_or(0.0);
            let margin_top_extra = if css.margin.top == crate::style::Side::Unset {
                margin_v
            } else {
                0.0
            };
            let margin_bottom_extra = if css.margin.bottom == crate::style::Side::Unset {
                margin_v
            } else {
                0.0
            };
            MarginChildRole::Block {
                top: css.margin.top.resolve(&resolve).unwrap_or(0.0) + margin_top_extra,
                bottom: css.margin.bottom.resolve(&resolve).unwrap_or(0.0) + margin_bottom_extra,
            }
        }
    }
}

pub(in crate::layout) fn edge_margin_from_children(
    dom: &Dom,
    id: NodeIdx,
    content_w: f32,
    parent_font_size: f32,
    ctx: &LayoutCtx,
    from_end: bool,
) -> Option<f32> {
    let children = &dom.node(id).children;
    if from_end {
        for &child in children.iter().rev() {
            match margin_child_role(dom, child, content_w, parent_font_size, ctx) {
                MarginChildRole::Ignore => continue,
                MarginChildRole::Barrier => return None,
                MarginChildRole::Block { bottom, .. } => return Some(bottom),
            }
        }
    } else {
        for &child in children {
            match margin_child_role(dom, child, content_w, parent_font_size, ctx) {
                MarginChildRole::Ignore => continue,
                MarginChildRole::Barrier => return None,
                MarginChildRole::Block { top, .. } => return Some(top),
            }
        }
    }
    None
}

pub(in crate::layout) fn escaped_child_margins(
    dom: &Dom,
    id: NodeIdx,
    parent_css: &ComputedStyle,
    content_w: f32,
    parent_font_size: f32,
    ctx: &LayoutCtx,
    pad_top: f32,
    border_top: f32,
    pad_bottom: f32,
    border_bottom: f32,
    bottom_auto_height: bool,
) -> (f32, f32) {
    if establishes_block_formatting_context(dom, id, parent_css) {
        return (0.0, 0.0);
    }
    let top = if pad_top == 0.0 && border_top == 0.0 {
        edge_margin_from_children(dom, id, content_w, parent_font_size, ctx, false).unwrap_or(0.0)
    } else {
        0.0
    };
    let bottom = if bottom_auto_height && pad_bottom == 0.0 && border_bottom == 0.0 {
        edge_margin_from_children(dom, id, content_w, parent_font_size, ctx, true).unwrap_or(0.0)
    } else {
        0.0
    };
    (top, bottom)
}

pub(in crate::layout) fn escaped_margins_for_box(
    dom: &Dom,
    id: NodeIdx,
    content_w: f32,
    parent_font_size: f32,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_px(&css, parent_font_size),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let pad_top = css.padding.top.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_bottom = css.padding.bottom.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let [border_top, _, border_bottom, _] = crate::style::borders::used_widths(&css);
    escaped_child_margins(
        dom,
        id,
        &css,
        content_w,
        parent_font_size,
        ctx,
        pad_top,
        border_top,
        pad_bottom,
        border_bottom,
        css.height.is_none() && css.min_height.is_none(),
    )
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
    // PRÓPRIO BFC (um novo é criado abaixo, em `bfc_filhos`). Ver `layout/bfc.rs`.
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
            if e_display_none(dom, id) {
                return (0.0, 0.0);
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
                    return layout_button(dom, id, &css, x, y, ctx, list);
                }
                return layout_input(
                    dom,
                    id,
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
            if tag == "canvas" {
                if let Some(r) = layout_canvas(dom, id, &css, x, y, avail_w, ctx, list) {
                    return r;
                }
            }
            if tag == "img" {
                if let Some(img) =
                    layout_image(dom, id, &css, x, y, avail_w, forced_outer_w, forced_outer_h, ctx, list)
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
                if let Some(r) = layout_svg_placeholder(dom, id, &css, x, y, avail_w, ctx, list) {
                    return r;
                }
            }
            css
        }
        // Texto solto ao nível de bloco: uma linha com a fonte do PAI
        // (`texto_solto.rs`); whitespace estrutural não cria linha nenhuma.
        NodeKind::Text(t) => return super::texto_solto::layout_texto_solto(dom, id, t, x, y, ctx, list),
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
    let shrink_to_fit = shrink_to_fit || used == Some(crate::style::DisplayKind::Table);
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
            content_natural_width(dom, id, font_for_content, ctx)
        } else {
            match css.width.and_then(|d| d.resolve(&resolve)) {
                // `width` explícito. Em `border-box`, o `width` INCLUI padding+border —
                // então o content é `width - (padding_h + 2*border)`. Em content-box
                // (default), o `width` JÁ é o content.
                Some(w) if border_box => (w - (padding_h + border_h)).max(0.0),
                Some(w) => w,
                // Sem width: shrink-to-fit → largura do conteúdo (com o piso
                // de min-content e o tecto do disponível, CSS2 §10.3.5 —
                // `flex_limites::largura_shrink_to_fit`, extraída para não
                // crescer este ficheiro); senão (fluxo block normal) →
                // ocupa a largura disponível.
                //
                // `largura_intrinseca_transferida` decide PRIMEIRO quando um
                // `<img>` sem tamanho lá dentro pesa pela razão×altura
                // esticada em vez do natural (`replaced_transferido.rs`) —
                // `None` em qualquer outro caso, e cai no shrink-to-fit de
                // sempre.
                None if shrink_to_fit => {
                    let h = forced_outer_h
                        .map(|h| (h - pad_top - pad_bottom - border_top - border_bottom).max(0.0));
                    super::replaced_transferido::largura_intrinseca_transferida(dom, id, font_for_content, h, ctx)
                        .unwrap_or_else(|| {
                            super::flex_limites::largura_shrink_to_fit(
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
        let mnw = super::intrinseco_min_max::resolve(css.min_width, dom, id, font_for_content, ctx, &resolve).map(|v| {
            if border_box {
                (v - (padding_h + border_h)).max(0.0)
            } else {
                v
            }
        });
        let mxw = super::intrinseco_min_max::resolve(css.max_width, dom, id, font_for_content, ctx, &resolve).map(|v| {
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
    let has_width = css.width.is_some() || css.max_width.is_some();
    if has_width {
        let box_outer = content_w + padding_h + border_h; // sem a margin
        // COM SINAL (não `.max(0.0)`): o ramo `direction:rtl` de
        // `rtl_bloco::margin_left_usado` precisa do valor negativo quando o
        // filho é mais largo do que o disponível — ver o módulo.
        let free_com_sinal = avail_w - box_outer;
        let free = free_com_sinal.max(0.0);
        match (m.left.is_auto(), m.right.is_auto()) {
            (true, true) => {
                margin_left = free / 2.0;
                margin_right = free / 2.0;
            }
            (true, false) => margin_left = (free - margin_right).max(0.0),
            (false, true) => margin_right = (free - margin_left).max(0.0),
            (false, false) => {
                margin_left =
                    super::rtl_bloco::margin_left_usado(dom, id, margin_left, margin_right, free_com_sinal);
            }
        }
    }

    // Posição do content-box (canto sup-esq): deslocado pelo lado ESQUERDO/TOPO
    // (margin+border+padding daquele lado), não a soma do eixo.
    let content_x = x + margin_left + border_left + pad_left;
    // MARGIN-COLLAPSE PAI→PRIMEIRO-FILHO — porquê em `margem_escapada.rs`.
    let escaped_top_pre = crate::layout::margem_escapada::escapada_no_topo(
        dom, id, &css, content_w, font_for_content, pad_top, border_top, ctx,
    );
    let content_y =
        y + (collapse_margin(margin_top, escaped_top_pre) - escaped_top_pre) + border_top + pad_top;

    // Z-ORDER: o fundo/borda da caixa precisam ficar ATRÁS dos filhos. Como a
    // display list é pintada em ordem, reservamos AGORA o índice onde a caixa será
    // inserida (antes de qualquer filho), descemos nos filhos (que dão append no
    // fim), e só DEPOIS — conhecendo a altura — inserimos o fundo nesse índice.
    let box_index = list.items.len();
    // Quantas subárvores já existiam ANTES desta caixa começar. Só as que
    // vierem a seguir é que são empurradas quando o fundo for inserido — ver
    // [`insert_item`].
    let filhos_antes_da_caixa = list.children.len();
    // Reserva a posição do pai antes dos filhos; a geometria final é preenchida
    // depois que a altura natural do conteúdo for conhecida.
    reserve_node_order(list, id);

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
    let ov_x_declarado = css
        .overflow_x
        .unwrap_or(crate::scrollbar::Overflow::Visible);
    let ov_y_declarado = css
        .overflow_y
        .unwrap_or(crate::scrollbar::Overflow::Visible);
    // CSS Overflow 1 §3: se só UM eixo é `visible` e o outro não, o `visible`
    // COMPUTA como `auto` — não fica um eixo aberto. `#so-x` de
    // `claude-overflow.html` (`overflow-x:hidden;overflow-y:visible`) é
    // exatamente este caso: o Chrome recorta os DOIS eixos (a régua de
    // pintura mediu: sem esta regra o nosso lado deixava a coluna Y aberta e
    // divergia 1,95% onde deveria bater). Sem a marca `visible` PURA (os
    // dois iguais) a exceção não se aplica — só quando os eixos DIVERGEM.
    let mistos = ov_x_declarado != ov_y_declarado;
    let visible_vira_auto = |o: crate::scrollbar::Overflow| {
        if mistos && o == crate::scrollbar::Overflow::Visible {
            crate::scrollbar::Overflow::Auto
        } else {
            o
        }
    };
    let ov_x = visible_vira_auto(ov_x_declarado);
    let ov_y = visible_vira_auto(ov_y_declarado);
    let scrolls_x = ov_x.scrollable() || ov_x.clips();
    // A inflação vale para o eixo do FLUXO HORIZONTAL, que é onde a compressão
    // aconteceria (o flex encolhe os itens até caberem). Nos demais layouts ela
    // vira base de PORCENTAGEM dos filhos, e aí está errada: `width:100%` dentro
    // de um container que rola é 100% da CAIXA, não do conteúdo transbordado.
    //
    // Medido na página real do WhatsApp Web, que aninha vários containers com
    // `overflow-y:auto`: cada nível multiplicava a largura do seguinte, e o
    // conteúdo terminava em x = 2300 numa janela de 1100 — a tela abria vazia
    // com tudo desenhado fora dela.
    let scroll_children_w = if scrolls_x {
        // largura que o conteúdo QUER (sem comprimir) — pode exceder content_w.
        intrinsic_content_width(dom, id, font_size, ctx).max(content_w)
    } else {
        content_w
    };
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
    // 3). `layout_children_column`/`coluna_wrap.rs` continuam a receber
    // `avail_children` para tudo o resto (gap%, `height:%` dos netos,
    // grow/shrink) — só o DESPACHO do wrap lê este valor mais estreito.
    let wrap_definite_h = explicit_content_h.or(mxh_pre);

    // Novo BFC (fresco, vazio) só se `id` o estabelece — senão os filhos
    // recebem a mesma referência ambiente, e um float lá dentro alcança os
    // IRMÃOS do antepassado que a possui. Ver `layout/bfc.rs`.
    let estabelece_bfc = establishes_block_formatting_context(dom, id, &css);
    let bfc_proprio = estabelece_bfc.then(BlockFormattingContext::new);
    let bfc_filhos = bfc_proprio.as_ref().unwrap_or(bfc);

    // `flex-direction: column` — o eixo PRINCIPAL do flex vira o vertical: os itens
    // empilham (sem margin-collapse, que flex não tem), gap/justify/margin-auto
    // atuam no Y e align-items no X (stretch = ocupar a largura, o default).
    // `is_column` é o eixo FÍSICO e não a keyword crua — `writing-mode` troca
    // qual eixo lógico é X e qual é Y (`eixos_flex::main_no_eixo_y`): um
    // `row` VERTICAL é o eixo inline, que aí é o Y, e desce por
    // `layout_children_column` como se fosse `column` (e vice-versa).
    let wm = css.writing_mode.unwrap_or_default();
    let dir = css.direction.unwrap_or_default();
    let is_column_kw = css.flex_direction.map(|f| f.is_column()).unwrap_or(false);
    let is_column = super::eixos_flex::main_no_eixo_y(wm, is_column_kw);
    // `row-reverse`/`column-reverse` × o sentido FÍSICO do eixo que ficou
    // principal (`eixos_flex::reverse_efetivo`), nunca o do eixo original da
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
    let is_reverse = super::eixos_flex::reverse_efetivo(wm, dir, is_column, is_reverse_kw);
    let is_flex =
        display == crate::block::DISPLAY_HORIZONTAL || display == crate::block::DISPLAY_WRAP;
    // `gap`/`row-gap` seguem a KEYWORD, nunca o eixo físico que `writing-mode`
    // troca (CSS Box Alignment §12.2 + Flexbox §8.1: "row-gap" é o espaço
    // entre as LINHAS do flex e "column-gap" entre os itens de uma linha —
    // "linha"/"coluna" aqui são o que `flex-direction:row`/`column` definem,
    // não X/Y físicos). `flex.rs` (roda o eixo físico X) e `coluna.rs` (roda
    // o Y) continuam a ler `gap`=principal/`row_gap`=cruzado e
    // `row_gap`=principal/`gap`=cruzado, respetivamente — os papéis que já
    // tinham antes deste lote. Quando o DESPACHO físico diverge da keyword
    // (`is_column != is_column_kw`, que só acontece em `writing-mode`
    // vertical — `main_no_eixo_y` troca-o), o algoritmo que corre é o do
    // eixo físico ERRADO para os nomes que já lê; troca-se os dois campos
    // aqui, uma vez, para o algoritmo continuar a ler o nome que já lia e
    // acertar mesmo assim (achado pelo WPT `gap-*-lr/rl/rtl` e
    // `flexbox-column-row-gap-002/004`: sem isto, um `flex-direction:column`
    // vertical passava o `gap` do documento — pensado para as SUAS colunas —
    // a `flex.rs`, que o lê como o espaço entre ITENS de uma linha).
    let css = if is_flex && is_column != is_column_kw {
        let mut c = (*css).clone();
        std::mem::swap(&mut c.gap, &mut c.row_gap);
        std::rc::Rc::new(c)
    } else {
        css
    };
    let content_h = match display {
        // flex column: sem wrap empilha numa coluna; COM wrap (e altura
        // definida) `layout_children_column` delega para `coluna_wrap.rs` —
        // ver o comentário no parâmetro `wrap` lá.
        _ if is_flex && is_column => layout_children_column(
            dom,
            id,
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
        _ if used == Some(crate::style::DisplayKind::Table) => crate::table::layout_table(
            dom, id, content_x, content_y, children_w, &css, font_size, ctx, list,
        ),
        _ if css.effective_display() == Some(crate::style::DisplayKind::Grid) => {
            layout_children_grid(
                dom,
                id,
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
        _ => layout_children_vertical(
            dom,
            id,
            content_x,
            content_y,
            children_w,
            avail_children,
            &css,
            font_size,
            bfc_filhos,
            ctx,
            list,
        ),
    };
    // CSS 2.1 §10.6.7: só o BFC responsável cresce para conter os SEUS
    // floats — `bfc_proprio` só existe quando `id` é ele (senão é `None` e
    // este `match` não mexe em nada). `flex/grid/tabela` acima nunca
    // acrescentam floats a `bfc_proprio` (floats não se aplicam lá dentro).
    let content_h = match &bfc_proprio {
        Some(proprio) => match proprio.fundo_lado(true, true) {
            Some(fundo) => content_h.max((fundo - content_y).max(0.0)),
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
    let caixa_inline = used.is_none()
        && css.effective_display().is_none()
        && css.height.is_none()
        && is_inline_block(dom, id);
    let content_h = if caixa_inline {
        crate::inline_box::altura_do_conteudo(font_size, ctx.measurer)
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
    // Registra a geometria deste nó (base do getBoundingClientRect/offsetWidth).
    record_node_rect(list, id, box_rect);

    // Pinta a CAIXA (fundo/borda) ATRÁS dos filhos. `insert` no `box_index` põe o
    // fundo antes dos itens dos filhos (z-order).
    if css.has_box() {
        let radius = css.corner_radius.unwrap_or(0.0);
        // O FUNDO pinta por canto; a borda e a sombra continuam a ler o campo
        // único, e é isso que as deixa responder hoje o que respondiam ontem.
        let cantos = Corners::from_style(&css, 0.0);
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
        let cor = |c: u32| apply_opacity(fx.aplicar(c), op);
        // Insere na ordem: primeiro o fundo, depois a borda por cima dele (ambos
        // atrás dos filhos). `insert` desloca os filhos para a frente.
        let mut at = box_index;
        // SOMBRA primeiro (atrás de tudo): box-shadow.
        if let Some(sh) = css.box_shadow {
            insert_item(
                list,
                at,
                filhos_antes_da_caixa,
                DisplayItem::Shadow {
                    rect: box_rect,
                    dx: sh.dx,
                    dy: sh.dy,
                    blur: sh.blur,
                    spread: sh.spread,
                    color: cor(sh.color),
                    radius,
                },
            );
            at += 1;
        }
        // FUNDO: gradiente (se houver) OU cor sólida — a menos que uma MÁSCARA
        // dê a forma da caixa (ver `deve_suprimir_fundo`).
        let fundo = !deve_suprimir_fundo(&css);
        if let Some(g) = css.gradient.filter(|_| fundo) {
            insert_item(
                list,
                at,
                filhos_antes_da_caixa,
                DisplayItem::GradientRect {
                    rect: box_rect,
                    c0: cor(g.c0),
                    c1: cor(g.c1),
                    angle_deg: g.angle_deg,
                    radius,
                },
            );
            at += 1;
        } else if let Some(color) = css.bg.filter(|_| fundo) {
            let color = cor(color);
            insert_item(
                list,
                at,
                filhos_antes_da_caixa,
                DisplayItem::SolidRect {
                    rect: box_rect,
                    color,
                    radius: cantos,
                },
            );
            at += 1;
        }
        for item in border_items(&css, box_rect, radius, op, fx) {
            insert_item(list, at, filhos_antes_da_caixa, item);
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
        // `visible_vira_auto` acima já garante que um `visible` sozinho
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
        // BeginClip no índice onde os FILHOS começam (logo após os itens de caixa que
        // foram inseridos em `box_index`); EndClip no fim. Quantos itens de caixa:
        // fundo (se bg) + borda (se visível).
        let box_items = if css.has_box() {
            // MESMA contagem da emissão acima: sombra + (gradiente OU bg) + as
            // barras de borda/outline. Estas últimas vêm de `border_items`, a mesma
            // função que as emitiu — contar por outra regra é o que dessincroniza
            // o índice do clip quando uma borda por lado entra em jogo.
            css.box_shadow.is_some() as usize
                + (css.gradient.is_some() || css.bg.is_some()) as usize
                + border_items(
                    &css,
                    box_rect,
                    css.corner_radius.unwrap_or(0.0),
                    1.0,
                    crate::painteffects::FilterMatriz::IDENTIDADE,
                )
                .len()
        } else {
            0
        };
        let children_start = box_index + box_items;
        // O offset vem do `Dom` (`dom/scroll.rs`) — não é escrito aqui de
        // propósito, só LIDO: quem rola é o backend, respondendo a input, e o
        // layout nunca recebe `&mut Dom` (ver a auditoria estrutural). Este
        // valor é só o "como estava quando o fragmento foi montado" — nem a
        // pintura nem uma consulta de geometria confiam nele (as duas voltam
        // a perguntar ao `Dom` o valor VIVO); ver a nota de topo de
        // `dom/scroll.rs` sobre por que scroll nunca invalida este cache.
        let (offset_x, offset_y) = dom.scroll_of_idx(id);
        insert_item(
            list,
            children_start,
            filhos_antes_da_caixa,
            DisplayItem::BeginClip {
                rect: content_rect,
                node: id,
                offset_x,
                offset_y,
                // O valor capturado ANTES de os filhos serem layoutados
                // (linha ~549), não `list.children.len()` de AGORA: os
                // filhos deste elemento já foram anexados a `list.children`
                // pela recursão que os layoutou, e usar a contagem atual
                // marcava TODOS eles como "já existiam quando o clip abriu"
                // — o `walk_items` de `itens.rs` então desenhava-os ANTES de
                // entrar no clip, e o recorte nunca continha nada. Era por
                // isso que `overflow:hidden`/`auto` nunca recortava (medido
                // pela régua de pintura: `claude-overflow.html` a 5,57%).
                filhos_antes: filhos_antes_da_caixa,
            },
        );
        list.items.push(DisplayItem::EndClip {
            filhos_dentro: list.children.len(),
        });
        if std::env::var_os("RTS_CLIP_DEBUG").is_some() && content_w <= 2.0 {
            let filhos: Vec<(usize, f32)> = list.children.iter().map(|c| (c.at, c.dy)).collect();
            eprintln!(
                "[clip] no={id:?} box_index={box_index} children_start={children_start} end_at={} children={:?}",
                list.items.len() - 1,
                &filhos[filhos.len().saturating_sub(6)..]
            );
        }
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

    // ── CLIP-PATH: recorta o elemento a um retângulo. SÓ `inset()` sem `round`
    // chega aqui com um rect — as outras formas devolvem `None` e não recortam
    // nada, porque recortar um `polygon()` pela caixa envolvente desenharia um
    // quadrado onde devia estar um losango (ver `painteffects`).
    //
    // Emitido DEPOIS do bloco de overflow acima, e de propósito: inserir em
    // `box_index` empurra tudo o que vem a partir dali, e fazê-lo antes
    // desalinharia por um o `children_start` que aquele bloco calcula. Como o
    // `EndClip` deste é empilhado no fim, o aninhamento sai certo — este abre
    // primeiro e fecha por último, portanto envolve o clip de scroll.
    //
    // A diferença para o clip de overflow é onde ABRE: aquele recorta só os
    // FILHOS (abre depois dos itens de caixa), este recorta o elemento INTEIRO,
    // fundo e borda incluídos, que é o que o `clip-path` do CSS faz. Daí abrir
    // em `box_index`.
    if let Some(cp) = css.clip_path.as_deref() {
        if let Some(rect) = crate::painteffects::clip_retangulo(cp, box_rect) {
            insert_item(
                list,
                box_index,
                filhos_antes_da_caixa,
                DisplayItem::BeginClip {
                    rect,
                    node: id,
                    offset_x: 0.0,
                    offset_y: 0.0,
                    // Os fragmentos-filhos que existiam quando a CAIXA foi
                    // reservada — não `list.children.len()` de agora, que já
                    // conta os desta subárvore. O clip abre conceptualmente
                    // antes deles, mesmo sendo inserido depois de existirem.
                    filhos_antes: filhos_antes_da_caixa,
                },
            );
            list.items.push(DisplayItem::EndClip {
                filhos_dentro: list.children.len(),
            });
        }
    }

    // POSITION:RELATIVE — porquê e o que desloca em `relativo.rs`. ANTES do
    // `transform`: a caixa de referência dele é a posição já deslocada.
    aplica_offset_relativo(dom, id, &css, avail_w, avail_h, font_size, box_index, ctx, list);

    // ── TRANSFORM (matriz 2D completa: matrix/translate/scale/rotate/skew,
    // compostas por `TransformList::resolve`): pós-processa os itens DESTE
    // elemento e seus descendentes (o range `[box_index..]`), em torno de
    // `transform-origin` (default `50% 50%` — CSS Transforms 1 §6). Aplicado
    // por último (não afeta o fluxo/tamanho — como no CSS, transform é visual).
    if let Some(tf) = css.transform {
        if !tf.is_identity() {
            let mat = super::transformacao::matriz_transform(
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
            super::transformacao::transforma_node_rects(dom, id, &mat, list);

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
            // `materialize` reescreve `items` INTEIRO, e todos os índices que os
            // ancestrais reservaram para as caixas deles passam a apontar para
            // outro item. Um `position:absolute` com `transform:translateY(-50%)`
            // — uma regra de ícone, na folha do MediaWiki — punha a página
            // inteira da Wikipédia a zero: 16 813 elementos sem geometria porque
            // uma regra de 40 bytes casou com um `<span>`.
            let is_pure_translate = mat.a == 1.0 && mat.b == 0.0 && mat.c == 0.0 && mat.d == 1.0;
            if is_pure_translate {
                for it in list.items[box_index..].iter_mut() {
                    translate_item(it, mat.e, mat.f);
                }
                for child in list.children.iter_mut().filter(|c| c.at >= box_index) {
                    child.dx += mat.e;
                    child.dy += mat.f;
                }
            } else {
                // Escala/rotação/skew/matriz: em vez de mutar cada item por
                // aproximação (norma das colunas — a caixa continuava
                // axis-aligned, só do tamanho errado), a matriz VIAJA na
                // lista como `PushTransform`/`PopTransform` em torno de
                // `[box_index..]`. `materialize()` primeiro pela mesma razão
                // de índices que já valia aqui: a subárvore precisa estar
                // achatada para o range `[box_index..]` corresponder
                // exatamente a este elemento e seus descendentes — nada
                // depois pertence a outro irmão ainda por vir.
                list.materialize();
                list.items.insert(box_index, DisplayItem::PushTransform { mat });
                list.items.push(DisplayItem::PopTransform);
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
