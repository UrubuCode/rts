//! `background-image: url(...)` PINTADA — o achado do lote
//! `background-image-pintado`: 173 das 294 falhas de `css/CSS2/backgrounds`
//! (WPT) erram por menos de 1% de pixels, muitas por exatamente 10 000 px
//! (100×100) — um quadrado de `background-color: red` coberto por
//! `background-image: url(...)` que nunca era desenhado. `DisplayItem::Pixels`
//! só existia para `<img>`/`<canvas>` (`replaced.rs`); este módulo é o
//! terceiro emissor, para a caixa de QUALQUER elemento.
//!
//! ## Onde os pixels vêm de
//!
//! Os bytes são carregados de FORA (o rasterizador da régua, em
//! `carregar_imagens`; numa página a correr, `dom.ts::loadResources`) e
//! guardados no PRÓPRIO nó por `Dom::set_pixel_data` — a MESMA API que
//! `<img>` já usa (`dom.pixel_data_of`), porque um nó com fundo é um nó como
//! outro qualquer para essa tabela: não há necessidade de uma segunda.
//! Sem pixels carregados, nada é emitido — a cor/gradiente do fundo (já
//! pintados por `bloco.rs`) continuam a mostrar-se por baixo, como um
//! `<img>` sem `src` decodificado mostra só a caixa.
//!
//! ## O corte
//!
//! `repeat`/`repeat-x`/`repeat-y`/`no-repeat` com `background-position` (a
//! forma que fecha a maioria dos 173) — TILING por ladrilhos de tamanho
//! NATURAL da imagem, recortados à área de posicionamento (padding-box, o
//! `background-origin` default) por um `BeginClip`/`EndClip`. NÃO
//! implementado, dito: `background-size` (sempre o natural — `cover`/
//! `contain`/comprimentos são lidos e ignorados), `background-attachment:
//! fixed`, `background-clip`/`-origin` não-default, `background-blend-mode`,
//! e mais de uma camada (o mesmo corte que `style::background` já assume —
//! `bg_image` só guarda a PRIMEIRA).

use super::*;
use crate::style::BgRepeat;

/// Os itens de `Pixels` (mais o `BeginClip`/`EndClip` que os recorta) para o
/// `background-image` deste nó, na ordem em que `bloco.rs` os insere: depois
/// da cor/gradiente, antes da borda. Vazio quando não há `background-image`
/// declarado, quando é `none`, ou quando os pixels ainda não carregaram.
///
/// `area` é a ÁREA DE POSICIONAMENTO — o padding-box (border-box menos as
/// quatro larguras de borda USADAS), que é o `background-origin` inicial da
/// spec (CSS Backgrounds 3 §3.4) — nunca o border-box inteiro, ou uma borda
/// arredondada pintaria por baixo do canto reto do fundo.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn background_pixels_items(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    box_rect: Rect,
    border_top: f32,
    border_right: f32,
    border_bottom: f32,
    border_left: f32,
    // Bookkeeping do CLIP — a mesma dupla que o clip de scroll (`bloco.rs`,
    // logo abaixo neste ficheiro) já usa e pela mesma razão (ver
    // `DisplayItem::BeginClip`/`EndClip`): quantas subárvores-filhas já
    // existiam quando o clip abriu/fechou. Um clip de fundo não recorta
    // nenhuma diferente das do scroll — os FILHOS deste nó já foram
    // layoutados antes deste ponto — por isso os dois valores coincidem com
    // os que o chamador já tem à mão.
    filhos_antes: usize,
    filhos_dentro: usize,
) -> Vec<DisplayItem> {
    let has_image = css
        .bg_image
        .as_deref()
        .is_some_and(|v| v.trim().to_ascii_lowercase().starts_with("url("));
    if !has_image {
        return Vec::new();
    }
    let Some((data, img_w, img_h)) = dom.pixel_data_of(id).filter(|(_, w, h)| *w > 0 && *h > 0) else {
        return Vec::new();
    };
    let area = Rect::new(
        box_rect.x + border_left,
        box_rect.y + border_top,
        (box_rect.w - border_left - border_right).max(0.0),
        (box_rect.h - border_top - border_bottom).max(0.0),
    );
    if area.w <= 0.0 || area.h <= 0.0 {
        return Vec::new();
    }
    let repeat = css.bg_repeat.unwrap_or_default();
    let pos = css.bg_position.unwrap_or_default();
    let resolve = crate::style::ResolveCtx {
        parent_content_w: area.w,
        node_font_size: super::DEFAULT_FONT_SIZE,
        root_font_size: crate::style::root_font_size(),
        viewport_w: area.w,
        viewport_h: area.h,
    };
    // A ORIGEM: uma percentagem escala pelo espaço LIVRE (área menos a imagem),
    // não pela área inteira — é assim que `center`/`right`/`bottom` colam a
    // imagem à borda oposta em vez de a deixarem a meio caminho.
    let pos_x = area.x + resolve_bg_offset(pos.x, area.w, img_w as f32, &resolve);
    let pos_y = area.y + resolve_bg_offset(pos.y, area.h, img_h as f32, &resolve);
    let repeats_x = matches!(repeat, BgRepeat::Repeat | BgRepeat::RepeatX | BgRepeat::Space | BgRepeat::Round);
    let repeats_y = matches!(repeat, BgRepeat::Repeat | BgRepeat::RepeatY | BgRepeat::Space | BgRepeat::Round);
    let xs = tile_starts(area.x, area.w, pos_x, img_w as f32, repeats_x);
    let ys = tile_starts(area.y, area.h, pos_y, img_h as f32, repeats_y);
    if xs.is_empty() || ys.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(xs.len() * ys.len() + 2);
    out.push(DisplayItem::BeginClip {
        rect: area,
        node: id,
        offset_x: 0.0,
        offset_y: 0.0,
        filhos_antes,
    });
    for &ty in &ys {
        for &tx in &xs {
            out.push(DisplayItem::Pixels {
                rect: Rect::new(tx, ty, img_w as f32, img_h as f32),
                data: std::rc::Rc::clone(&data),
                w: img_w,
                h: img_h,
            });
        }
    }
    out.push(DisplayItem::EndClip { filhos_dentro });
    out
}

/// A percentagem/comprimento de `background-position` num eixo, resolvida
/// contra o espaço LIVRE (CSS Backgrounds 3 §3.8): `0%` cola ao início, `100%`
/// ao fim, um comprimento é um deslocamento direto a partir do início.
fn resolve_bg_offset(d: crate::style::Dimension, area_len: f32, img_len: f32, resolve: &crate::style::ResolveCtx) -> f32 {
    match d {
        crate::style::Dimension::Percent(p) => (area_len - img_len) * (p / 100.0),
        other => other.resolve(resolve).unwrap_or(0.0),
    }
}

/// Os pontos de início de cada ladrilho num eixo, cobrindo `[area_start,
/// area_start+area_len)`. Sem repetição, um único ladrilho em `pos`. Um teto
/// (`4096`) contra um `size` ínfimo/zero gerar um laço sem fim — mesma
/// doutrina de qualquer laço orientado a geometria neste motor.
fn tile_starts(area_start: f32, area_len: f32, pos: f32, size: f32, repeats: bool) -> Vec<f32> {
    if size <= 0.0 {
        return Vec::new();
    }
    if !repeats {
        return vec![pos];
    }
    let mut first = pos;
    if first > area_start {
        let k = ((first - area_start) / size).ceil();
        first -= k * size;
    } else if first < area_start {
        let k = ((area_start - first) / size).floor();
        first += k * size;
    }
    let mut out = Vec::new();
    let mut x = first;
    let end = area_start + area_len;
    let mut n = 0;
    while x < end && n < 4096 {
        out.push(x);
        x += size;
        n += 1;
    }
    out
}
