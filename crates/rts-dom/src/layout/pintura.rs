//! O que se PINTA à volta de uma caixa: fundo do documento, barras de scroll,
//! bordas, decoração de texto, cor e opacidade.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.

use super::*;
/// Emite os retângulos da SCROLLBAR (track + thumb) na DisplayList — a BARRA é
/// preparada pelo DOM, não pelo backend (o egui só pinta `SolidRect`, mantendo-se
/// burro e substituível). Dados de geometria: `viewport_w/h` (área visível),
/// `content_h` (altura total do conteúdo), `offset_y` (quanto já rolou). Estilo:
/// `sb` (cor/largura/radius do CSS). Só emite a barra VERTICAL (a horizontal segue
/// o mesmo molde quando precisar). Coordenadas em espaço de CONTEÚDO já rolado: a
/// barra é desenhada FIXA na viewport, então some o `offset_y` (o backend translada
/// o conteúdo por -offset; somar offset à barra a mantém na tela).
///
/// Não faz nada se o conteúdo cabe (sem overflow) e a barra não é forçada.
pub fn emit_scrollbar(
    list: &mut DisplayList,
    viewport_w: f32,
    viewport_h: f32,
    content_h: f32,
    offset_y: f32,
    sb: &crate::scrollbar::ScrollbarStyle,
    force: bool,
) {
    use crate::scrollbar::BarWidth;
    // precisa rolar? (conteúdo maior que a viewport) ou barra forçada (overflow:scroll).
    let overflow = content_h > viewport_h + 0.5;
    if !overflow && !force {
        return;
    }
    // largura da barra (px): thin=8, none=0 (não desenha), px direto, senão 12.
    let bar_w = match sb.width {
        Some(BarWidth::None) => return,
        Some(BarWidth::Thin) => 8.0,
        Some(BarWidth::Px(px)) => px,
        _ => 12.0,
    };
    // cores default fiéis a um browser escuro (sobrescritas pelo CSS).
    let track_color = sb.track.unwrap_or(0x1e1e1eff);
    let thumb_color = sb.thumb.unwrap_or(0x6b6b6bff);
    let radius = sb.thumb_radius.unwrap_or(bar_w / 2.0);
    let bar_x = viewport_w - bar_w;
    // o thumb: tamanho proporcional à fração visível; posição proporcional ao offset.
    let frac = (viewport_h / content_h).clamp(0.0, 1.0);
    let thumb_h = (viewport_h * frac).max(24.0); // mínimo p/ pegar com o mouse
    let max_off = (content_h - viewport_h).max(1.0);
    let scroll_frac = (offset_y / max_off).clamp(0.0, 1.0);
    let thumb_y = scroll_frac * (viewport_h - thumb_h);
    // FIXA na viewport: soma offset_y (o backend translada tudo por -offset).
    let vy = offset_y;
    // track (faixa direita inteira) — atrás do thumb.
    list.items.push(DisplayItem::SolidRect {
        rect: Rect::new(bar_x, vy, bar_w, viewport_h),
        color: track_color,
        radius: Corners::ZERO,
    });
    // thumb (handle).
    list.items.push(DisplayItem::SolidRect {
        rect: Rect::new(bar_x, vy + thumb_y, bar_w, thumb_h),
        color: thumb_color,
        radius: Corners::same(radius),
    });
}

/// Emite as barras (x e/ou y) DENTRO de um scroll container interno (#1744), no rect
/// visível dele (coords de conteúdo da página). Diferente de `emit_scrollbar` (que é
/// a viewport): aqui as barras ficam nas bordas da DIV. Emitidas APÓS o `EndClip`
/// (fora do recorte), então não rolam — ficam fixas na div. `offset_*` é o quanto a
/// região rolou (posiciona o thumb).
pub fn emit_scrollbar_in(
    list: &mut DisplayList,
    region: &ScrollRegion,
    offset_x: f32,
    offset_y: f32,
    sb: &crate::scrollbar::ScrollbarStyle,
) {
    use crate::scrollbar::BarWidth;
    let bar_w = match sb.width {
        Some(BarWidth::None) => return,
        Some(BarWidth::Thin) => 8.0,
        Some(BarWidth::Px(px)) => px,
        _ => 12.0,
    };
    let track_color = sb.track.unwrap_or(0x1e1e1eff);
    let thumb_color = sb.thumb.unwrap_or(0x6b6b6bff);
    let radius = sb.thumb_radius.unwrap_or(bar_w / 2.0);
    let v = region.visible;
    let need_y = region.overflow_y.scrollable() && region.content_h > v.h + 0.5;
    let need_x = region.overflow_x.scrollable() && region.content_w > v.w + 0.5;
    // barra VERTICAL (borda direita da div).
    if need_y {
        let track_h = if need_x { v.h - bar_w } else { v.h };
        let frac = (track_h / region.content_h).clamp(0.0, 1.0);
        let thumb_h = (track_h * frac).max(24.0);
        let max_off = (region.content_h - v.h).max(1.0);
        let thumb_y = (offset_y / max_off).clamp(0.0, 1.0) * (track_h - thumb_h);
        let bx = v.x + v.w - bar_w;
        list.items.push(DisplayItem::SolidRect {
            rect: Rect::new(bx, v.y, bar_w, track_h),
            color: track_color,
            radius: Corners::ZERO,
        });
        list.items.push(DisplayItem::SolidRect {
            rect: Rect::new(bx, v.y + thumb_y, bar_w, thumb_h),
            color: thumb_color,
            radius: Corners::same(radius),
        });
    }
    // barra HORIZONTAL (borda inferior da div).
    if need_x {
        let track_w = if need_y { v.w - bar_w } else { v.w };
        let frac = (track_w / region.content_w).clamp(0.0, 1.0);
        let thumb_w = (track_w * frac).max(24.0);
        let max_off = (region.content_w - v.w).max(1.0);
        let thumb_x = (offset_x / max_off).clamp(0.0, 1.0) * (track_w - thumb_w);
        let by = v.y + v.h - bar_w;
        list.items.push(DisplayItem::SolidRect {
            rect: Rect::new(v.x, by, track_w, bar_w),
            color: track_color,
            radius: Corners::ZERO,
        });
        list.items.push(DisplayItem::SolidRect {
            rect: Rect::new(v.x + thumb_x, by, thumb_w, bar_w),
            color: thumb_color,
            radius: Corners::same(radius),
        });
    }
}

/// O `background` do `<html>` (ou, se ausente/transparente e sem imagem, do
/// `<body>` dentro dele) — a cor que o CSS PROPAGA para o viewport inteiro
/// (CSS 2.1 §14.2). `None` se nenhum dos dois tem fundo (visível) algum.
///
/// **Precedência corrigida**: o `<html>` vence SEMPRE que declara fundo
/// próprio (cor visível ou imagem) — `c45-bg-canvas-000` (`html:purple` +
/// `body:navy`) espera a tela PURPLE, com o `<body>` a pintar a SUA navy por
/// cima. A versão anterior tentava o `<body>` PRIMEIRO e só recorria ao
/// `<html>` quando o body não tinha fundo — invertido face à cascata, e dava
/// a tela errada sempre que os dois declaravam cores diferentes.
///
/// **E a falta que zerava o caso comum**: quando `<html>` não declara fundo
/// nenhum, `bg_of_tag(html)` é `None` e o ramo nunca chegava a olhar para o
/// `<body>` lá dentro — `background-body-001` (só `body{background:green}`)
/// media 97,86% de pixels errados por isto: a tela ficava no branco por
/// omissão em vez do verde do body.
pub(in crate::layout) fn body_background(dom: &Dom) -> Option<u32> {
    for &child in &dom.node(dom.root).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if tag == "html" {
                let css = dom.computed_style_idx(child);
                // Alpha zero (`background: transparent`, explícito ou o
                // inicial) NÃO conta como fundo próprio para esta decisão —
                // só bloqueia a propagação uma cor de facto visível.
                let color = css.as_ref().and_then(|c| c.bg).filter(|c| c & 0xFF != 0);
                let has_image = css.as_ref().is_some_and(|c| c.bg_image.is_some());
                if color.is_some() || has_image {
                    // Tem imagem mas não cor: devolve `None` (tela branca por
                    // omissão) em vez de "roubar" a cor do `<body>`, que
                    // perdeu a decisão — a imagem em si na tela é um corte à
                    // parte (`fundo_imagem.rs` só desenha por caixa de
                    // elemento, ainda não pela tela).
                    return color;
                }
                return find_body_bg(dom, child);
            }
        }
        if let Some(bg) = bg_of_tag(dom, child, "body") {
            return Some(bg);
        }
    }
    None
}

/// O bg de `idx` se sua tag é `tag` e tem background computado.
fn bg_of_tag(dom: &Dom, idx: NodeIdx, tag: &str) -> Option<u32> {
    match &dom.node(idx).kind {
        NodeKind::Element { tag: t } if t == tag => dom.computed_style_idx(idx).and_then(|c| c.bg),
        _ => None,
    }
}

/// Procura um `<body>` com bg na subárvore de `idx` (ex: html>body).
fn find_body_bg(dom: &Dom, idx: NodeIdx) -> Option<u32> {
    for &child in &dom.node(idx).children {
        if let Some(bg) = bg_of_tag(dom, child, "body") {
            return Some(bg);
        }
    }
    None
}

/// `true` quando o fundo do elemento NÃO deve ser pintado por a forma dele vir de
/// uma `mask-image` que não sabemos carregar.
///
/// Em CSS a máscara RECORTA o fundo: `background-color` mais `mask-image` é o modo
/// canónico de desenhar um ícone monocromático (o MediaWiki fá-lo em
/// `.cdx-button__icon`, e a Wikipédia traz 24 deles). Pintar o fundo sem a máscara
/// não é uma aproximação da forma — é o retângulo inteiro, um bloco cinzento onde
/// o browser mostra um glifo. Não pintar nada erra por omissão, que é o erro
/// menor, e é a mesma regra do CLAUDE.md sobre superfícies que não fazem o que o
/// nome diz: a ausência falha à vista, o oco engana.
///
/// SUBSTITUTO TEMPORÁRIO. Quando carregarmos e aplicarmos máscaras a sério, o
/// fundo volta a ser pintado e passa a ser recortado pela máscara — esta função
/// desaparece em vez de mudar de resposta.
pub(in crate::layout) fn deve_suprimir_fundo(css: &ComputedStyle) -> bool {
    css.mask_image.is_some()
}

/// Código de decoração de texto p/ o `DisplayItem::Text` a partir do estilo:
/// 0=nenhuma, 1=underline, 2=line-through, 3=overline.
pub(in crate::layout) fn decoration_code(css: &ComputedStyle) -> u8 {
    match css.text_decoration {
        Some(crate::style::values::TextDecoration::Underline) => 1,
        Some(crate::style::values::TextDecoration::LineThrough) => 2,
        Some(crate::style::values::TextDecoration::Overline) => 3,
        _ => 0,
    }
}

/// Multiplica o ALPHA de uma cor `0xRRGGBBAA` por `opacity` ∈ [0,1] (o RGB fica
/// A cor com que um elemento pinta, dado o seu `visibility`.
///
/// `visibility:hidden` não salta o layout — o elemento ocupa o espaço na mesma —,
/// só não é pintado. Zerar o alpha é como isso se exprime numa display list que
/// não tem grupos de compositing, e a propriedade ser HERDADA faz o resto: os
/// descendentes chegam ao seu próprio layout já com ela posta.
/// `font-style: italic` resolvido para uma tag: o CSS computado vence, e a
/// UA-stylesheet responde quando ninguém declarou; `herdado` é o último recurso.
///
/// A consulta à UA passa por [`crate::block::lookup_inline`] — a TABELA que
/// regista `<i>` e `<em>` como `FLAG_ITALIC` — e não por um `match` sobre o
/// nome da tag. A alternativa rejeitada era exatamente esse `match`: o motor de
/// layout não nomeia tags HTML, a UA-stylesheet é que as nomeia, e é ela quem
/// muda quando o default de uma tag muda.
///
/// Sem este ramo, um `<em>` sem regra de autor não fica itálico nenhum — o mapa
/// da UA não tinha, até aqui, UM ÚNICO leitor em todo o motor.
/// O nome da tag de um nó, ou `None` se for texto. Existe para o [`italico`]
/// poder consultar a UA-stylesheet sem que quem chama tenha de desmontar o nó.
pub(in crate::layout) fn tag_de(dom: &Dom, id: NodeIdx) -> Option<&str> {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => Some(tag.as_str()),
        _ => None,
    }
}

pub(in crate::layout) fn italico(css: Option<&crate::style::ComputedStyle>, tag: Option<&str>, herdado: bool) -> bool {
    if let Some(v) = css.and_then(|c| c.italic) {
        return v;
    }
    let ua = tag.is_some_and(|t| crate::block::lookup_inline(t) & crate::block::FLAG_ITALIC != 0);
    ua || herdado
}

pub(in crate::layout) fn cor_visivel(css: &crate::style::ComputedStyle, cor: u32) -> u32 {
    if css.visibility.is_some_and(|v| v.suppresses_paint()) {
        cor & 0xFFFF_FF00
    } else {
        cor
    }
}

/// intacto; só o canal alpha escala). `opacity >= 1` devolve a cor inalterada.
pub(in crate::layout) fn apply_opacity(color: u32, opacity: f32) -> u32 {
    if opacity >= 1.0 {
        return color;
    }
    let op = opacity.clamp(0.0, 1.0);
    let a = (color & 0xFF) as f32;
    let new_a = (a * op).round().clamp(0.0, 255.0) as u32;
    (color & 0xFFFF_FF00) | new_a
}

/// `true` se a tag é um campo de TEXTO editável (mini-browser): `<input>` (tipos
/// textuais) ou `<textarea>`. Um `<input type=checkbox/radio/...>` não conta (v1
/// só faz texto). Sem `type` → texto (o default do HTML).
pub(in crate::layout) fn is_text_input_tag(tag: &str) -> bool {
    matches!(tag, "input" | "textarea")
}

/// Os itens de BORDA de uma caixa: a moldura uniforme, as barras por lado, e o
/// `outline`. Uma função só porque a lista é emitida num sítio e CONTADA noutro
/// (o índice onde o clip de scroll começa) — duas regras para a mesma lista foi o
/// que já dessincronizou o clip antes.
///
/// Duas formas, e a escolha é por fidelidade:
/// - Sem nada declarado por lado, sai UM `DisplayItem::Border` — o caminho que já
///   existia, e o único que respeita o `border-radius` (o backend desenha a
///   moldura arredondada).
/// - Com um lado declarado (`border-bottom: 1px solid #ccc`, o separador de 17
///   ocorrências na folha da Wikipédia), sai uma BARRA por lado visível, como
///   `SolidRect`. Emitir a moldura uniforme neste caso desenharia os quatro lados
///   onde a página pediu um: errado de forma mais visível do que ignorar.
///
/// CORTE FECHADO (estava documentado aqui como aberto): a largura por lado JÁ
/// entra na geometria da caixa. `bloco.rs` lê `style::borders::used_widths` —
/// as quatro larguras por lado, não o escalar — e alimenta `border_h`/`border_v`,
/// `content_w`, `content_x` e o `box_rect` com elas; esta função já recebe os
/// quatro valores separados (`border_top`/`right`/`bottom`/`left`) de quem a
/// chama. Uma `border-bottom: 1px` empurra o conteúdo 1px, como o Chrome. A nota
/// antiga ficou a descrever um estado que outro lote já tinha corrigido.
///
/// O `outline` sai por último (por cima) e por FORA do border-box, inflado pelo
/// `outline-offset` — é o que o distingue da borda: não ocupa espaço nenhum.
/// Os quatro lados de uma borda como trapézios (topo, direita, fundo,
/// esquerda), do canto EXTERIOR ao canto INTERIOR — a junção que o Blink
/// desenha. Um lado de largura 0 degenera num trapézio sem área, e é assim que
/// dois lados a zero e um enorme dão um triângulo.
fn trapezios_dos_lados(
    r: Rect,
    sides: &[crate::style::borders::SideBorder; 4],
) -> [([(f32, f32); 4], crate::style::borders::SideBorder); 4] {
    let (t, rt, b, l) = (sides[0].width, sides[1].width, sides[2].width, sides[3].width);
    let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
    let (o_tl, o_tr, o_br, o_bl) = ((x0, y0), (x1, y0), (x1, y1), (x0, y1));
    let (i_tl, i_tr, i_br, i_bl) =
        ((x0 + l, y0 + t), (x1 - rt, y0 + t), (x1 - rt, y1 - b), (x0 + l, y1 - b));
    [
        ([o_tl, o_tr, i_tr, i_tl], sides[0]),
        ([o_tr, o_br, i_br, i_tr], sides[1]),
        ([o_br, o_bl, i_bl, i_br], sides[2]),
        ([o_bl, o_tl, i_tl, i_bl], sides[3]),
    ]
}

pub(crate) fn border_items(
    css: &ComputedStyle,
    box_rect: Rect,
    radius: f32,
    op: f32,
    // O `filter` do elemento. Parâmetro e não leitura do `css` aqui dentro porque
    // esta função é chamada TAMBÉM só para CONTAR quantos itens de borda existem
    // (o índice do clip), e nessa chamada a cor não interessa — passar a
    // identidade diz isso explicitamente em vez de calcular uma matriz para a
    // deitar fora.
    fx: crate::painteffects::FilterMatriz,
) -> Vec<DisplayItem> {
    let mut out = Vec::new();
    let sides = crate::style::borders::resolved_sides(css);
    if crate::style::borders::has_per_side(css) {
        let (x, y, w, h) = (box_rect.x, box_rect.y, box_rect.w, box_rect.h);
        // JUNÇÃO DIAGONAL: quando dois lados adjacentes pintam com cores
        // diferentes, cada lado é o trapézio do canto exterior ao interior
        // (`claude-border-juncao`, `claude-triangulo-de-borda`). Com cores
        // iguais — o caso de `border: 1px solid` escrito por lado — as barras
        // abaixo pintam o mesmo desenho com menos vértices, e ficam.
        let adjacentes_diferem = (0..4).any(|i| {
            let (a, b) = (sides[i], sides[(i + 1) % 4]);
            a.paints() && b.paints() && a.color != b.color
        });
        if adjacentes_diferem {
            for (pts, side) in trapezios_dos_lados(box_rect, &sides) {
                // um lado `transparent` (o truque do triângulo) pinta nada:
                // não vale um item.
                if side.paints() && side.color & 0xFF != 0 {
                    out.push(DisplayItem::Quad {
                        pts,
                        color: fx.aplicar_com_opacidade(side.color, op),
                    });
                }
            }
        }
        // top, right, bottom, left — a ordem de `resolved_sides`. Cada barra ocupa
        // a aresta INTEIRA; os cantos ficam sobrepostos em vez de mitrados, que é
        // invisível enquanto as cores dos lados adjacentes coincidem e é o que um
        // separador (um lado só) precisa.
        let bars = if adjacentes_diferem { vec![] } else { vec![
            (Rect::new(x, y, w, sides[0].width), sides[0]),
            (
                Rect::new(x + w - sides[1].width, y, sides[1].width, h),
                sides[1],
            ),
            (
                Rect::new(x, y + h - sides[2].width, w, sides[2].width),
                sides[2],
            ),
            (Rect::new(x, y, sides[3].width, h), sides[3]),
        ] };
        for (rect, side) in bars {
            if side.paints() {
                out.push(DisplayItem::SolidRect {
                    rect,
                    color: fx.aplicar_com_opacidade(side.color, op),
                    radius: Corners::ZERO,
                });
            }
        }
    } else {
        // A borda uniforme só pinta se tem largura E um `border-style` VISÍVEL. O
        // default CSS de border-style é `none` → sem `border-style` declarado, NÃO
        // pinta (fiel ao Chrome: `border-width:2px` sozinho dá borda invisível).
        if sides[0].paints() {
            out.push(DisplayItem::Border {
                rect: box_rect,
                width: sides[0].width,
                color: fx.aplicar_com_opacidade(sides[0].color, op),
                radius,
            });
        }
    }
    let ow = css.outline_width.unwrap_or(0.0);
    let visible = css.outline_style.map(|s| s.is_visible()).unwrap_or(false);
    if ow > 0.0 && visible {
        let off = css.outline_offset.unwrap_or(0.0) + ow / 2.0;
        out.push(DisplayItem::Border {
            rect: Rect::new(
                box_rect.x - off,
                box_rect.y - off,
                box_rect.w + 2.0 * off,
                box_rect.h + 2.0 * off,
            ),
            width: ow,
            // `outline-color` ausente = `currentColor` (a cor do texto).
            color: fx
                .aplicar_com_opacidade(css.outline_color.or(css.color).unwrap_or(0x000000FF), op),
            // O outline é sempre RETANGULAR aqui (o Chrome moderno segue o
            // border-radius) — ver `style::borders`.
            radius: 0.0,
        });
    }
    out
}
