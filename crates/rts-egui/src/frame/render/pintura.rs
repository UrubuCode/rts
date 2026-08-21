use super::*;


/// Percorre a [`DisplayList`] e pinta cada item via `ui.painter()`, em coordenadas
/// absolutas (conteúdo + origem do `ui`). A ordem da lista É o z-order (o que vem
/// depois pinta por cima). Reserva o espaço da altura total para o `ui` pai.
pub(in crate::frame::render) fn paint_list(ui: &mut egui::Ui, list: &DisplayList, offset_y: f32) {
    let _phase = rts_dom::metrics::phases::scope("paint");
    // origem do conteúdo + a translação de scroll da PÁGINA (offset_y negativo sobe).
    let base_origin = ui.max_rect().min + egui::vec2(0.0, offset_y);
    // PILHA para o scroll container interno (#1744): cada BeginClip empilha (painter
    // recortado, offset extra da região); EndClip desempilha. O item é pintado com o
    // painter do topo e a SOMA dos offsets extra (a região rolada). Base = ui.
    let base = ui.painter().clone();
    let mut stack: Vec<(egui::Painter, egui::Vec2)> = Vec::new();
    // `walk` anda a ÁRVORE de fragmentos: os itens de uma subárvore reusada
    // chegam aqui sem nunca terem sido copiados, com o deslocamento a somar — e
    // somar uma origem já era o que este laço fazia.
    // CULLING: a área realmente visível. Um item inteiramente fora dela não é
    // pintado — o egui pagaria a construção do galley de cada texto, e uma
    // página real tem duas ordens de grandeza mais texto do que cabe na tela.
    //
    // Sem isto a Wikipédia mandava 30 093 textos por frame para pintar os ~52
    // que se veem, e o frame demorava tanto que a janela ficava BRANCA: não é
    // que não pintasse, é que quase nunca chegava ao fim. Redimensionar, que
    // repinta a cada evento, ficava impraticável pela mesma razão.
    let visivel = ui.clip_rect().intersect(ui.max_rect());
    let diagnostico = std::env::var_os("RTS_DOM_PAINT").is_some();
    // Quantos itens pintados listar. Um limite FIXO de 16 não respondia à
    // pergunta que a tela branca faz — "quem tapou o resto?" —, porque quem tapa
    // é pintado DEPOIS: a Wikipédia saía em branco por causa do item 56, um
    // `<input>` de fundo branco opaco do tamanho da página, e ele nunca chegava à
    // amostra. `RTS_DOM_PAINT_N=0` lista todos.
    let limite_diag = std::env::var("RTS_DOM_PAINT_N")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|n| if n == 0 { usize::MAX } else { n })
        .unwrap_or(16);
    let (mut vistos, mut cortados) = (0usize, 0usize);
    let mut idx = 0usize;
    list.walk(|item, dx, dy| {
        idx += 1;
        let idx = idx - 1;
        let (painter, extra) = stack
            .last()
            .map(|(p, o)| (p.clone(), *o))
            .unwrap_or_else(|| (base.clone(), egui::Vec2::ZERO));
        // origem da página + translação da região + deslocamento do fragmento
        let origin = base_origin + extra + egui::vec2(dx, dy);
        // O que está fora da tela não é pintado — menos o `BeginClip`/`EndClip`,
        // que são ESTADO da pilha e têm de continuar a casar (saltar um abre um
        // clip que nunca fecha, e aí desaparece o que vinha depois).
        if let Some(caixa) = caixa_do_item(item, origin) {
            if !caixa.intersects(visivel) {
                cortados += 1;
                return;
            }
            vistos += 1;
            if diagnostico && vistos <= limite_diag {
                let tipo = match item {
                    DisplayItem::Text { text, color, .. } =>
                        format!("txt cor=#{color:08X} {:?}", text.chars().take(18).collect::<String>()),
                    DisplayItem::SolidRect { color, .. } => format!("rect cor=#{color:08X}"),
                    DisplayItem::Border { color, .. } => format!("borda cor=#{color:08X}"),
                    _ => "?".to_owned(),
                };
                eprintln!("  [{vistos}] {tipo} caixa={caixa:?} clip_painter={:?}", painter.clip_rect());
            }
        }
        match item {
            DisplayItem::SolidRect { rect, color, radius } => {
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                // RECORTA ao visível quando não há canto arredondado. Uma página
                // real tem retângulos de dezenas de milhares de pontos (o fundo
                // de um `<div>` que envolve o documento inteiro), e mandá-los
                // assim ao tesselador desperdiça o trabalho todo fora do ecrã —
                // e a precisão de um `f32` a 77 000 pontos já não é a de um a
                // 780. Com raio não se recorta: cortar um canto arredondado
                // mudava o desenho.
                //
                // A pergunta é sobre os QUATRO cantos e é o `any()` que a faz.
                // Respondê-la por um canto — o que a leitura de um `radius`
                // único virava naturalmente ao passar a quatro — mandaria o
                // fundo do documento inteiro ao tesselador sempre que qualquer
                // canto fosse zero, que é o caso comum.
                let r = if radius.any() { r } else { r.intersect(visivel) };
                if r.is_positive() {
                    // nw/ne/se/sw do egui são tl/tr/br/bl do CSS.
                    let cr = egui::CornerRadius {
                        nw: radius.tl as u8,
                        ne: radius.tr as u8,
                        se: radius.br as u8,
                        sw: radius.bl as u8,
                    };
                    painter.rect_filled(r, cr, rgba_to_color32(*color));
                }
            }
            DisplayItem::Border { rect, width, color, radius } => {
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                painter.rect_stroke(
                    r,
                    egui::CornerRadius::same(*radius as u8),
                    egui::Stroke::new(*width, rgba_to_color32(*color)),
                    egui::StrokeKind::Inside,
                );
            }
            DisplayItem::Text { x, y, text, color, size, mono, bold, italic, letter_spacing, decoration } => {
                // a MESMA escolha que o medidor faz — ver `EguiMeasurer::family`.
                let font = egui::FontId::new(*size, EguiMeasurer::family(*mono, *bold, *italic));
                let col = rgba_to_color32(*color);
                let base = origin + egui::vec2(*x, *y);
                let total_w = if *letter_spacing != 0.0 {
                    // letter-spacing: pinta char a char, avançando pela largura do glifo
                    // + o espaçamento. Devolve a largura total (p/ a linha de decoração).
                    let mut cx = base.x;
                    for ch in text.chars() {
                        let s = ch.to_string();
                        let gw = painter
                            .ctx()
                            .fonts_mut(|f| f.glyph_width(&font, ch))
                            .max(0.0);
                        painter.text(
                            egui::pos2(cx, base.y),
                            egui::Align2::LEFT_TOP,
                            &s,
                            font.clone(),
                            col,
                        );
                        cx += gw + *letter_spacing;
                    }
                    cx - base.x
                } else {
                    let g = painter.text(base, egui::Align2::LEFT_TOP, text, font.clone(), col);
                    g.width()
                };
                // decoração: linha sob/sobre/cortando o texto (1=under, 2=through, 3=over).
                if *decoration != 0 {
                    let ly = match decoration {
                        2 => base.y + *size * 0.5,  // line-through (meio)
                        3 => base.y + *size * 0.05, // overline (topo)
                        _ => base.y + *size * 0.92, // underline (base)
                    };
                    let thick = (*size * 0.06).max(1.0);
                    painter.line_segment(
                        [egui::pos2(base.x, ly), egui::pos2(base.x + total_w, ly)],
                        egui::Stroke::new(thick, col),
                    );
                }
            }
            DisplayItem::Shadow { rect, dx, dy, blur, spread, color, radius } => {
                // box-shadow: um retângulo deslocado (dx,dy), crescido pelo spread, com
                // borda amaciada pelo blur (feathering do egui). Pintado ANTES da caixa.
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x + *dx - *spread, rect.y + *dy - *spread),
                    egui::vec2(rect.w + 2.0 * *spread, rect.h + 2.0 * *spread),
                );
                let shadow = egui::epaint::Shadow {
                    offset: [0, 0],
                    blur: blur.max(0.0) as u8,
                    spread: 0,
                    color: rgba_to_color32(*color),
                };
                let shape = shadow.as_shape(r, egui::CornerRadius::same(*radius as u8));
                painter.add(shape);
            }
            DisplayItem::GradientRect { rect, c0, c1, angle_deg, .. } => {
                // gradiente linear: mesh de 4 vértices, cada canto com a cor interpolada
                // conforme sua projeção no eixo do ângulo (convenção CSS: 0=cima,
                // 90=direita). Aproxima o linear-gradient de 2 cores.
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                paint_linear_gradient(&painter, r, *c0, *c1, *angle_deg);
            }
            DisplayItem::Image { rect, pixels_handle, pixels_off, img_w, img_h } => {
                // lê os RGBA8 do Buffer no HandleTable, sobe como textura efêmera e
                // pinta no rect (escalando). O decode/download já aconteceram no .ts.
                let need = (*img_w as usize) * (*img_h as usize) * 4;
                let rgba: Option<Vec<u8>> =
                    crate::pixels::fetch(*pixels_handle, u64::from(*pixels_off), need);
                if let Some(bytes) = rgba {
                    let ci = egui::ColorImage::from_rgba_unmultiplied(
                        [*img_w as usize, *img_h as usize],
                        &bytes,
                    );
                    let tex = painter.ctx().load_texture(
                        format!("__rts_domimg_{}_{}", pixels_handle, idx),
                        ci,
                        egui::TextureOptions::LINEAR,
                    );
                    let r = egui::Rect::from_min_size(
                        origin + egui::vec2(rect.x, rect.y),
                        egui::vec2(rect.w, rect.h),
                    );
                    painter.image(
                        tex.id(),
                        r,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
            }
            DisplayItem::Pixels { rect, data, w, h } => {
                // Os bytes vêm DENTRO da lista (um `<canvas>` que o programa
                // pintou), então não há handle a resolver — sobe direto como
                // textura efêmera. O nome da textura inclui o índice do item
                // para que dois canvas no mesmo frame não disputem a mesma.
                let ci = egui::ColorImage::from_rgba_unmultiplied(
                    [*w as usize, *h as usize],
                    data,
                );
                let tex = painter.ctx().load_texture(
                    format!("__rts_canvas_{idx}"),
                    ci,
                    egui::TextureOptions::LINEAR,
                );
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                painter.image(
                    tex.id(),
                    r,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            DisplayItem::BeginClip { rect, offset_x, offset_y, .. } => {
                // o RECT do container é FIXO (não rola) — posiciona com `origin` (que
                // já inclui o extra do pai, mas não o desta região). Os FILHOS dentro
                // rolam: empilha o offset (-offset) somado ao extra herdado.
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                let clipped = painter.with_clip_rect(r.intersect(painter.clip_rect()));
                let new_extra = extra + egui::vec2(-*offset_x, -*offset_y);
                stack.push((clipped, new_extra));
            }
            DisplayItem::EndClip { .. } => {
                stack.pop();
            }
        }
    });
    if diagnostico {
        eprintln!("[paint] pintados={vistos} cortados={cortados} visivel={visivel:?} base_origin={base_origin:?}");
    }
}

/// O retângulo que um item ocupa na tela, ou `None` quando ele não é pintura
/// (os marcadores de clip) — nesses o culling não se aplica.
fn caixa_do_item(item: &DisplayItem, origin: egui::Pos2) -> Option<egui::Rect> {
    let de = |x: f32, y: f32, w: f32, h: f32| {
        Some(egui::Rect::from_min_size(origin + egui::vec2(x, y), egui::vec2(w.max(1.0), h.max(1.0))))
    };
    match item {
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::Border { rect, .. } => de(rect.x, rect.y, rect.w, rect.h),
        DisplayItem::Text { x, y, size, text, .. } => {
            // largura estimada por cima: o culling só precisa de não cortar o que
            // é visível, e medir cada texto aqui pagaria o custo que ele evita.
            de(*x, *y, text.chars().count() as f32 * *size, *size * 2.0)
        }
        _ => None,
    }
}

/// Pinta um GRADIENTE LINEAR de 2 cores num retângulo, como mesh de 4 vértices. A cor
/// de cada canto é a interpolação `c0`→`c1` conforme a projeção do canto no EIXO do
/// gradiente (definido por `angle_deg`, convenção CSS: 0°=de baixo p/ cima, 90°=p/ a
/// direita). Aproxima o `linear-gradient` de 2 pontos (paradas intermediárias já
/// foram descartadas no parse).
fn paint_linear_gradient(
    painter: &egui::Painter,
    rect: egui::Rect,
    c0: u32,
    c1: u32,
    angle_deg: f32,
) {
    // Vetor de direção do gradiente. CSS: 0°=para cima (0,-1); cresce no sentido
    // horário → 90°=(1,0), 180°=(0,1). rad = angle; dir = (sin, -cos).
    let rad = angle_deg.to_radians();
    let (dx, dy) = (rad.sin(), -rad.cos());
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    // projeção de cada canto no eixo; normaliza para [0,1] entre min e max.
    let proj: Vec<f32> = corners.iter().map(|p| p.x * dx + p.y * dy).collect();
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for &p in &proj {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let span = (hi - lo).max(1e-3);
    let ca = rgba_to_color32(c0);
    let cb = rgba_to_color32(c1);
    let mut mesh = egui::epaint::Mesh::default();
    for (i, corner) in corners.iter().enumerate() {
        let t = ((proj[i] - lo) / span).clamp(0.0, 1.0);
        let color = lerp_color32(ca, cb, t);
        mesh.colored_vertex(*corner, color);
    }
    // dois triângulos (0,1,2) e (0,2,3).
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Interpola dois `Color32` no parâmetro `t` ∈ [0,1] (por canal, sem premultiply).
fn lerp_color32(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}
