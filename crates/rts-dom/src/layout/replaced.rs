//! Elementos SUBSTITUÍDOS que o layout coloca: `<svg>`, `<img>` e `<canvas>`.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.

use super::*;
/// Layout de um `<input>`/`<textarea>` editável: emite a CAIXA (fundo+borda), o
/// TEXTO (o valor digitado, ou o `placeholder` apagado se vazio) e, se o campo tem
/// o FOCO, um CURSOR (barrinha) após o texto. Void (sem filhos) — o egui só recebe
/// SolidRect+Text+SolidRect e pinta burramente. Retorna `(outer_w, outer_h)`.
#[allow(clippy::too_many_arguments)]
/// Layout de um `<img>` com pixels já decodificados: emite `DisplayItem::Image` no
/// rect. Tamanho: `width`/`height` do CSS se houver; senão o natural da imagem (mas
/// limitado à largura disponível, preservando a proporção). `None` se o `<img>` ainda
/// não tem imagem setada (nada a pintar). Retorna `(outer_w, outer_h)`.
#[allow(clippy::too_many_arguments)]
/// Reserva a CAIXA de um `<svg>` (replaced element) sem desenhar o vetor: usa
/// `width`/`height` do CSS ou dos atributos; se só um lado é dado e há `viewBox`,
/// deriva o outro pela razão de aspecto; se nada, cai numa proporção do viewBox
/// ou num tamanho default. Pinta um placeholder cinza-claro (a "caixa" do ícone/
/// logo) no rect. `None` se não dá pra dimensionar (colapsa como antes).
pub(in crate::layout) fn layout_svg_placeholder(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> Option<(f32, f32)> {
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let node = dom.node(id);
    // Os atributos são COMPRIMENTOS CSS (presentation attributes, SVG 2 §7):
    // `1em` é o font-size, `50%` é do contentor, `24` é px. Lidos só como
    // número, o `<svg width="1em">` do botão de tema do Bootstrap caía no
    // viewBox (`claude-svg-atributo-em`).
    let attr_px = |name: &str| -> Option<f32> {
        node.attr(name).and_then(|v| {
            let v = v.trim();
            let sem_unidade = v.parse::<f32>().ok();
            sem_unidade
                .or_else(|| crate::style::lengths::parse_dimension_pub(v).and_then(|d| d.resolve(&resolve)))
                .filter(|n| *n > 0.0)
        })
    };
    // razão de aspecto do viewBox ("0 0 W H" → W/H).
    let vb_ratio = node.attr("viewBox").and_then(|vb| {
        let n: Vec<f32> = vb
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        if n.len() == 4 && n[3] > 0.0 {
            Some(n[2] / n[3])
        } else {
            None
        }
    });
    let css_w = css
        .width
        .and_then(|d| d.resolve(&resolve))
        .filter(|w| *w > 0.0);
    let css_h = css
        .height
        .and_then(|d| resolve_height(Some(d), None, &resolve))
        .filter(|h| *h > 0.0);
    let w0 = css_w.or_else(|| attr_px("width"));
    let h0 = css_h.or_else(|| attr_px("height"));
    // resolve (w, h): ambos dados usa-os; só um + viewBox deriva o outro; nada →
    // um ícone default (24×24) ou o viewBox escalado a 24 de altura.
    let (w, h) = match (w0, h0) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, vb_ratio.map(|r| w / r).unwrap_or(w)),
        (None, Some(h)) => (vb_ratio.map(|r| h * r).unwrap_or(h), h),
        (None, None) => {
            let h = 24.0;
            (vb_ratio.map(|r| h * r).unwrap_or(h), h)
        }
    };
    let w = w.min(avail_w.max(1.0));
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    // As margens, como o `<img>` (`layout_image`): a caixa fica dentro delas e
    // o outer devolvido conta-as — sem isto um `svg{margin-bottom:4px}` não
    // empurrava o irmão seguinte (`claude-svg-atributo-em`, y=16 vs 20).
    let m = &css.margin;
    let (ml, mr) = (m.left.resolve(&resolve).unwrap_or(0.0), m.right.resolve(&resolve).unwrap_or(0.0));
    let (mt, mb) = (m.top.resolve(&resolve).unwrap_or(0.0), m.bottom.resolve(&resolve).unwrap_or(0.0));
    let rect = Rect::new(x + ml, y + mt, w, h);
    // placeholder cinza-claro (a caixa do ícone) — só quando não é minúsculo demais.
    list.items.push(DisplayItem::SolidRect {
        rect,
        color: 0xE8EAEDFF,
        radius: Corners::same(2.0),
    });
    record_node_rect(list, id, rect);
    Some((w + ml + mr, h + mt + mb))
}

pub(in crate::layout) fn layout_image(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    // Tamanho OUTER que o FLEX já decidiu (grow/shrink no eixo principal,
    // `align-items: stretch` no cruzado) — `None` fora de um item flex, ou
    // quando o eixo não é imposto (o `<img>` decide sozinho pela CSS/atributo/
    // natural). Vence `width`/`height` do mesmo jeito que já vence num bloco
    // comum (`bloco.rs`): sem isto um `<img>` esticado no eixo cruzado nunca
    // via a altura que o flex lhe deu (`claude-flex-abspos-img-aspect-ratio`,
    // `claude-img-sem-tamanho-natural-em-flex`) — o despacho de `<img>` em
    // `bloco.rs` ignorava os dois parâmetros por inteiro.
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> Option<(f32, f32)> {
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // margens (respeita o CSS); a imagem em si é o content (sem padding/borda v1).
    let m = &css.margin;
    let margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0);
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0);
    // `forced_outer_*` é OUTER (margem+borda+conteúdo, como `FlexItem::main`);
    // `replaced_inline_size` decide em CONTEÚDO e soma a borda por conta
    // própria no fim — descontam-se aqui as duas para não a somar em dobro.
    let [border_top, border_right, border_bottom, border_left] = crate::style::borders::used_widths(css);
    let forced_w = forced_outer_w
        .map(|w| (w - margin_left - margin_right - border_left - border_right).max(0.0));
    let forced_h = forced_outer_h
        .map(|h| (h - margin_top - margin_bottom - border_top - border_bottom).max(0.0));
    // A CAIXA não depende de haver pixels, e é por isso que ela vem de
    // `replaced_inline_size` em vez de uma segunda cópia das mesmas regras: o
    // `width`/`height` do CSS ou do atributo HTML já decide, que é o que o
    // browser faz e o que as miniaturas da Wikipédia trazem (109 dos 110 `<img>`
    // da página têm os dois atributos).
    //
    // Sair aqui quando os pixels faltam — o que este caminho fazia — não deixava
    // a imagem sem caixa apenas a ela: a `<figure>` que a contém é
    // `display:table`, encolhia ao conteúdo e ficava com 10px, e a `<figcaption>`
    // ao lado passava a quebrar a um carácter por linha. 25 figuras nessa forma
    // valem +6 629px de legenda na página.
    // A base de uma PERCENTAGEM é a largura do bloco CONTENTOR, e a margem do
    // próprio elemento não entra nela (CSS 2.1 §10.2). Descontá-la aqui — o que
    // este sítio fazia — custava 6px exatos em cada miniatura da Wikipédia:
    // `.mw-file-element` declara `margin:3px` e `max-width:calc(100% - 8px)`, o
    // `100%` valia 252 em vez dos 258 do `<a>`, e a imagem saía 244 onde o Chrome
    // dá 250 de conteúdo. Enquanto o tamanho era cortado por `avail_w` o erro
    // estava lá e ninguém o lia: nada consultava esta base.
    //
    // A alternativa — manter a subtração e corrigi-la só para o `calc` — punha a
    // regra de resolução em dois sítios, que é o que este ficheiro já pagou.
    let (w, h) = crate::inline_box::replaced_inline_size(dom, id, css, avail_w, (forced_w, forced_h), ctx)?;
    let rect = Rect::new(x + margin_left, y + margin_top, w, h);
    record_node_rect(list, id, rect);
    // O FUNDO da caixa pinta-se com ou sem pixels — um `<img>` com
    // `background` é uma caixa como as outras enquanto a imagem não chega
    // (`claude-object-fit`: o Blink mostra o `#eee` por baixo, e aqui a régua
    // de pintura via 0 itens). CORTE dito: a cor sai crua — sem `filter` nem
    // `opacity` do elemento, que o caminho do bloco aplica por `cor()`; e sem
    // borda/padding, que este layout ainda não dá à imagem (v1 acima).
    if let Some(color) = css.bg.filter(|_| !super::pintura::deve_suprimir_fundo(css)) {
        list.items.push(DisplayItem::SolidRect { rect, color, radius: Corners::ZERO });
    }
    // O item de pintura, esse, PRECISA de pixels: uma caixa reservada com nada
    // dentro é o que o browser mostra enquanto a imagem não chega, e é a mesma
    // doutrina do `<canvas>` logo abaixo. Pixels guardados NO documento (uma
    // `data:` URL descodificada pela ponte) saem como `Pixels`, o item do
    // canvas — o rasterizador da régua pinta-o sem handle table. CORTE dito:
    // a imagem estica à caixa (`object-fit: fill`); `contain`/`cover`/`none`
    // ainda não recortam nem centram.
    if let Some((data, pw, ph)) = dom.pixel_data_of(id).filter(|(_, pw, ph)| *pw > 0 && *ph > 0) {
        list.items.push(DisplayItem::Pixels { rect, data, w: pw, h: ph });
    } else if let Some((handle, off, iw, ih)) = dom
        .image_of(id)
        .filter(|(h, _, iw, ih)| *h != 0 && *iw != 0 && *ih != 0)
    {
        list.items.push(DisplayItem::Image {
            rect,
            pixels_handle: handle,
            pixels_off: off,
            img_w: iw,
            img_h: ih,
        });
    }
    Some((
        w + margin_left + margin_right,
        h + margin_top + margin_bottom,
    ))
}

/// Layout de um `<canvas>`: a caixa dos atributos `width`/`height` (o padrão do
/// HTML é 300×150) ou do CSS, e o `DisplayItem::Pixels` quando há desenho.
///
/// Sem pixels a caixa é reservada e nada é pintado — um canvas em branco é um
/// canvas em branco, não um buraco no layout. É essa reserva que faz o resto da
/// página se dispor no lugar certo antes de o programa desenhar.
pub(in crate::layout) fn layout_canvas(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> Option<(f32, f32)> {
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let attr_px = |name: &str| -> Option<f32> {
        dom.node(id)
            .attr(name)
            .and_then(|v| v.trim().trim_end_matches("px").trim().parse::<f32>().ok())
            .filter(|v| *v >= 0.0)
    };
    // 300×150 é o default do HTML para um canvas sem dimensões.
    let w = css
        .width
        .and_then(|d| d.resolve(&resolve))
        .or_else(|| attr_px("width"))
        .unwrap_or(300.0);
    let h = css
        .height
        .and_then(|d| d.resolve(&resolve))
        .or_else(|| attr_px("height"))
        .unwrap_or(150.0);
    let m = &css.margin;
    let margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0);
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0);
    let rect = Rect::new(x + margin_left, y + margin_top, w, h);
    record_node_rect(list, id, rect);
    if let Some(color) = css.bg {
        list.items.push(DisplayItem::SolidRect {
            rect,
            color,
            radius: Corners::ZERO,
        });
    }
    if let Some((data, pw, ph)) = dom.pixel_data_of(id) {
        if pw > 0 && ph > 0 {
            list.items.push(DisplayItem::Pixels {
                rect,
                data,
                w: pw,
                h: ph,
            });
        }
    }
    Some((
        w + margin_left + margin_right,
        h + margin_top + margin_bottom,
    ))
}
