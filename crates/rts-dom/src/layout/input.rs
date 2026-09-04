//! Campos de FORMULÁRIO: a medida de um `<input>`, o botão, e a caixa de
//! marca de checkbox/radio.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.

use super::*;
/// Layout de um `<input type=submit/button/reset>`: BOTÃO estilo UA — caixa
/// cinza-clara com borda e o `value` como rótulo (shrink-to-fit no texto). O CSS
/// do autor (bg/cor/padding) vence os defaults. Não editável, não focável (v1).
pub(in crate::layout) fn layout_button(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    let font = font_px(css, DEFAULT_FONT_SIZE - 3.0);
    let label = dom.node(id).attr("value").unwrap_or("").to_string();
    let tw = ctx.measurer.text_width(&label, font, false, false, false);
    let lh = ctx.measurer.line_height(font);
    let (pad_h, pad_v) = (12.0, 5.0);
    let w = tw + 2.0 * pad_h;
    let h = lh + 2.0 * pad_v;
    let bg = css.bg.unwrap_or(0xF8F9FAFF); // cinza-claro UA (o do botão do google)
    let fg = css.color.unwrap_or(0x3C4043FF);
    list.items.push(DisplayItem::SolidRect {
        rect: Rect::new(x, y, w, h),
        color: bg,
        // 4.0 é o raio que a UA dá a um botão; um canto declarado vence-o, e um
        // canto NÃO declarado num botão que declarou os outros continua a levar
        // o da UA — que é o que `from_style` faz e um `unwrap_or` por canto não
        // conseguiria dizer sem repetir a regra quatro vezes.
        radius: Corners::from_style(&css, 4.0),
    });
    list.items.push(DisplayItem::Border {
        rect: Rect::new(x, y, w, h),
        width: css.border_width.unwrap_or(1.0),
        color: css.border_color.unwrap_or(0xDADCE0FF),
        radius: css.corner_radius.unwrap_or(4.0),
    });
    list.items.push(DisplayItem::Text {
        x: x + pad_h,
        y: y + pad_v,
        text: label.into(),
        color: fg,
        size: font,
        mono: false,
        bold: false,
        italic: false,
        letter_spacing: 0.0,
        decoration: 0,
    });
    record_node_rect(list, id, Rect::new(x, y, w, h));
    (w + 6.0, h + 4.0) // margenzinha UA entre botões
}

/// O lado do quadrado de um `checkbox`/`radio` sem tamanho declarado. 13px é o
/// intrínseco que os browsers dão a estes controlos; não sai de fonte nenhuma,
/// por isso é uma constante e não uma medida.
const CAIXA_DE_MARCA: f32 = 13.0;

/// A caixa de um `<input>` de texto/marca: `(outer_w, outer_h)` e o frame com
/// que ela foi construída.
///
/// Existe porque a medida estava em DOIS sítios: o `layout_input`, que pinta, e
/// o `inline_widget_size`, que reserva o espaço na linha. O segundo dizia
/// espelhar o primeiro e não espelhava — um `checkbox` reservava 190x26 (um
/// campo de texto) e pintava outra coisa. Uma pergunta, uma resposta.
pub(in crate::layout) fn medida_do_input(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    avail_w: f32,
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    ctx: &LayoutCtx,
) -> MedidaDoInput {
    let font = font_px(css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let m = &css.margin;
    let p = &css.padding;
    let margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0);
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0);
    // CHECKBOX e RADIO são REPLACED: a caixa é um quadradinho de tamanho
    // intrínseco, não um campo de texto. E não levam o padding/borda com que a
    // UA veste um campo — no browser são 13x13 e mais nada, por isso os defaults
    // do frame são ZERO para eles (o CSS do autor continua a mandar).
    let quadrado = matches!(
        dom.node(id)
            .attr("type")
            .map(|t| t.to_ascii_lowercase())
            .as_deref(),
        Some("checkbox") | Some("radio")
    );
    let (pad_ua_h, pad_ua_v, borda_ua) = if quadrado {
        (0.0, 0.0, 0.0)
    } else {
        (4.0, 3.0, 1.0)
    };
    let pad_left = p.left.resolve(&resolve).unwrap_or(pad_ua_h).max(0.0);
    let pad_right = p.right.resolve(&resolve).unwrap_or(pad_ua_h).max(0.0);
    let pad_top = p.top.resolve(&resolve).unwrap_or(pad_ua_v).max(0.0);
    let pad_bottom = p.bottom.resolve(&resolve).unwrap_or(pad_ua_v).max(0.0);
    let border = css.border_width.unwrap_or(borda_ua).max(0.0);
    let padding_h = pad_left + pad_right;
    let frame = margin_left + margin_right + 2.0 * border + padding_h;
    let border_box = css.border_box.unwrap_or(false);
    let content_w = if let Some(fw) = forced_outer_w {
        (fw - frame).max(0.0)
    } else if let Some(w) = css.width.and_then(|d| d.resolve(&resolve)) {
        if border_box {
            (w - (padding_h + 2.0 * border)).max(0.0)
        } else {
            w
        }
    } else if quadrado {
        CAIXA_DE_MARCA
    } else {
        180.0_f32.min((avail_w - frame).max(0.0))
    };
    // `resolve_height` e não `resolve`: uma percentagem no eixo VERTICAL mede-se
    // contra a altura do containing block. Com o `resolve` genérico media-se
    // contra a LARGURA — os `<input type=checkbox>` do "checkbox hack" da
    // Wikipédia declaram `height:100%` e vinham com a largura da viewport de
    // altura, oito deles, o pior rácio de erro da página inteira.
    let declarada = resolve_height(css.height, avail_h, &resolve).map(|h| {
        if border_box {
            (h - (pad_top + pad_bottom + 2.0 * border)).max(0.0)
        } else {
            h
        }
    });
    // A altura IMPOSTA pelo `align-items: stretch` de um flex vence o
    // `height`, como no `layout_block`: um `<input>` num flex-row de 80px
    // estica até aos 80 (`claude-flex-stretch-input-height`). O canal não
    // existia e o campo caía sempre na altura da linha, 21.
    let imposta = forced_outer_h
        .map(|fh| (fh - margin_top - margin_bottom - pad_top - pad_bottom - 2.0 * border).max(0.0));
    let content_h = imposta.or(declarada).unwrap_or(if quadrado {
        CAIXA_DE_MARCA
    } else {
        ctx.measurer.line_height(font)
    });
    MedidaDoInput {
        content_w,
        content_h,
        pad_left,
        pad_top,
        padding_v: pad_top + pad_bottom,
        padding_h,
        border,
        margin_left,
        margin_top,
        margin_h: margin_left + margin_right,
        margin_v: margin_top + margin_bottom,
        font,
    }
}

/// O que `medida_do_input` responde: a caixa e o frame com que foi construída.
pub(in crate::layout) struct MedidaDoInput {
    content_w: f32,
    content_h: f32,
    pad_left: f32,
    pad_top: f32,
    padding_v: f32,
    padding_h: f32,
    border: f32,
    margin_left: f32,
    margin_top: f32,
    margin_h: f32,
    margin_v: f32,
    font: f32,
}

impl MedidaDoInput {
    /// A caixa EXTERNA (com margens) — o que o fluxo reserva para o widget.
    pub(in crate::layout) fn outer(&self) -> (f32, f32) {
        (
            self.content_w + self.padding_h + 2.0 * self.border + self.margin_h,
            self.content_h + self.padding_v + 2.0 * self.border + self.margin_v,
        )
    }
}

pub(in crate::layout) fn layout_input(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    x: f32,
    y: f32,
    avail_w: f32,
    // Altura do containing block, para `height: %`. `None` = pai com altura auto,
    // e aí a percentagem vale `auto` — a mesma regra do `layout_block`.
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    let med = medida_do_input(dom, id, css, avail_w, avail_h, forced_outer_w, forced_outer_h, ctx);
    let MedidaDoInput {
        content_w,
        content_h,
        pad_left,
        pad_top,
        padding_v,
        padding_h,
        border,
        margin_left,
        margin_top,
        margin_h,
        margin_v,
        font,
    } = med;
    let pad_bottom = padding_v - pad_top;
    let margin_right = margin_h - margin_left;
    let margin_bottom = margin_v - margin_top;
    let line_h = ctx.measurer.line_height(font);
    let _ = (pad_bottom, margin_right, margin_bottom, line_h);
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let _ = &resolve;
    let box_rect = Rect::new(
        x + margin_left,
        y + margin_top,
        content_w + padding_h + 2.0 * border,
        content_h + pad_top + pad_bottom + 2.0 * border,
    );
    record_node_rect(list, id, box_rect);

    // Fundo: o `background` do CSS, senão branco (campo de texto clássico).
    let radius = css.corner_radius.unwrap_or(0.0);
    let cantos = Corners::from_style(css, 0.0);
    // A OPACIDADE também vale aqui. Este era o único sítio que emite caixa sem
    // passar por `apply_opacity`, e o preço foi uma página inteira em branco: a
    // Wikipédia usa o "checkbox hack" — `<input type=checkbox>` com
    // `opacity: 0`, dimensionado à altura da página, para abrir menus sem
    // JavaScript. Oito deles, com fundo branco opaco e borda cinzenta, pintados
    // depois de tudo o resto. O layout estava certo, a lista de pintura estava
    // certa, e o que se via era o fundo de um controlo invisível.
    //
    // `unwrap_or(0xFFFFFFFF)` é o fundo que a UA dá a um campo de texto, e um
    // campo com `opacity: 0` não o pinta.
    let opacidade = css.opacity.unwrap_or(1.0);
    let bg = apply_opacity(css.bg.unwrap_or(0xFFFFFFFF), opacidade);
    list.items.push(DisplayItem::SolidRect {
        rect: box_rect,
        color: bg,
        radius: cantos,
    });
    // Borda: sempre desenha (o input tem contorno por padrão). Cor do CSS ou cinza.
    // Se o campo tem foco, realça a borda (azul), como o browser.
    let focused = dom.focused_input() == Some(id);
    let border_color = if focused {
        0x3B82F6FF // azul de foco
    } else {
        css.border_color.unwrap_or(0x9AA0A6FF)
    };
    let border_color = apply_opacity(border_color, opacidade);
    let bw = if border > 0.0 { border } else { 1.0 };
    list.items.push(DisplayItem::Border {
        rect: box_rect,
        width: bw,
        color: border_color,
        radius,
    });

    // Texto: o valor digitado, ou o placeholder apagado. Posicionado no content-box.
    let text_x = x + margin_left + bw + pad_left;
    let text_y = y + margin_top + bw + pad_top;
    let (shown, tcolor) = if dom.input_is_empty(id) {
        let ph = dom.node(id).attr("placeholder").unwrap_or("").to_string();
        (ph, 0x9AA0A6FF) // cinza apagado
    } else {
        (dom.input_value(id), css.color.unwrap_or(0x111111FF))
    };
    if !shown.is_empty() {
        list.items.push(DisplayItem::Text {
            x: text_x,
            y: text_y,
            text: shown.as_str().into(),
            color: tcolor,
            size: font,
            mono: false,
            bold: false,
            italic: false,
            letter_spacing: 0.0,
            decoration: 0,
        });
    }
    // Cursor: barrinha vertical após o texto do VALOR (não do placeholder), só com foco.
    if focused {
        let val = dom.input_value(id);
        let caret_x = text_x + ctx.measurer.text_width(&val, font, false, false, false) + 1.0;
        let caret = Rect::new(caret_x, text_y, 1.5, line_h.min(content_h.max(line_h)));
        list.items.push(DisplayItem::SolidRect {
            rect: caret,
            color: 0x111111FF,
            radius: Corners::ZERO,
        });
    }

    (
        box_rect.w + margin_left + margin_right,
        box_rect.h + margin_top + margin_bottom,
    )
}
