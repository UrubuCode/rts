//! `border-*-radius`: parsing e serialização dos quatro cantos elípticos.
//!
//! Um canto CSS tem dois raios, horizontal e vertical. O layout/painter actual
//! continua a consumir o raio horizontal legado, mas o estilo computado preserva
//! ambos para que `getComputedStyle` não reduza uma elipse a um círculo.

use super::lengths::{parse_len_pub, split_top_ws};
use super::props::ComputedStyle;

/// Os quatro cantos, na ordem em que o shorthand os escreve.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

impl Corner {
    /// O canto nomeado pelo sufixo de uma longhand, física ou lógica.
    ///
    /// As lógicas assumem LTR horizontal, que é o mesmo corte de
    /// `padding-inline-start` e de `style::logical`.
    fn parse(suffix: &str) -> Option<Corner> {
        Some(match suffix {
            "top-left" | "start-start" => Corner::TopLeft,
            "top-right" | "start-end" => Corner::TopRight,
            "bottom-right" | "end-end" => Corner::BottomRight,
            "bottom-left" | "end-start" => Corner::BottomLeft,
            _ => return None,
        })
    }
}

fn set_horizontal(css: &mut ComputedStyle, corner: Corner, value: Option<f32>) {
    match corner {
        Corner::TopLeft => css.corner_tl = value,
        Corner::TopRight => css.corner_tr = value,
        Corner::BottomRight => css.corner_br = value,
        Corner::BottomLeft => css.corner_bl = value,
    }
}

fn set_vertical(css: &mut ComputedStyle, corner: Corner, value: Option<f32>) {
    match corner {
        Corner::TopLeft => css.corner_tl_y = value,
        Corner::TopRight => css.corner_tr_y = value,
        Corner::BottomRight => css.corner_br_y = value,
        Corner::BottomLeft => css.corner_bl_y = value,
    }
}

fn horizontal(css: &ComputedStyle, corner: Corner) -> Option<f32> {
    match corner {
        Corner::TopLeft => css.corner_tl,
        Corner::TopRight => css.corner_tr,
        Corner::BottomRight => css.corner_br,
        Corner::BottomLeft => css.corner_bl,
    }
}

fn vertical(css: &ComputedStyle, corner: Corner) -> Option<f32> {
    match corner {
        Corner::TopLeft => css.corner_tl_y,
        Corner::TopRight => css.corner_tr_y,
        Corner::BottomRight => css.corner_br_y,
        Corner::BottomLeft => css.corner_bl_y,
    }
}

fn parse_radius_length(value: &str) -> Option<f32> {
    parse_len_pub(value).or_else(|| (value.trim() == "0").then_some(0.0))
}

/// Expande a lista CSS de 1 a 4 valores na ordem TL, TR, BR, BL.
fn expand_values(values: &[String]) -> Option<[f32; 4]> {
    let value = |index: usize| values.get(index).and_then(|token| parse_radius_length(token));
    match values.len() {
        1 => Some([value(0)?, value(0)?, value(0)?, value(0)?]),
        2 => Some([value(0)?, value(1)?, value(0)?, value(1)?]),
        3 => Some([value(0)?, value(1)?, value(2)?, value(1)?]),
        4 => Some([value(0)?, value(1)?, value(2)?, value(3)?]),
        _ => None,
    }
}

/// Lê a componente de um canto: `10px` ou `10px 20px`.
fn parse_corner_pair(value: &str) -> Option<(f32, f32)> {
    let values = split_top_ws(value);
    if values.is_empty() || values.len() > 2 {
        return None;
    }
    let horizontal = parse_radius_length(values.first()?)?;
    let vertical = match values.get(1) {
        Some(token) => parse_radius_length(token)?,
        None => horizontal,
    };
    Some((horizontal, vertical))
}

/// O shorthand `border-radius`, incluindo a forma elíptica
/// `<horizontal> / <vertical>`.
///
/// A expansão segue a regra CSS: 1–4 valores em cada lado, na ordem TL, TR, BR,
/// BL, com o segundo e o quarto a repetirem os valores correspondentes.
pub fn apply_shorthand(css: &mut ComputedStyle, value: &str) {
    let parts: Vec<&str> = value.split('/').collect();
    if parts.len() > 2 {
        return;
    }
    let horizontal = match expand_values(&split_top_ws(parts[0])) {
        Some(values) => values,
        None => return,
    };
    let vertical = if let Some(vertical_text) = parts.get(1) {
        match expand_values(&split_top_ws(vertical_text)) {
            Some(values) => values,
            None => return,
        }
    } else {
        horizontal
    };
    let corners = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomRight,
        Corner::BottomLeft,
    ];
    for (index, corner) in corners.into_iter().enumerate() {
        set_horizontal(css, corner, Some(horizontal[index]));
        set_vertical(css, corner, Some(vertical[index]));
    }
}

/// Tenta aplicar uma longhand de canto. `false` = o nome não é de uma delas.
pub fn try_apply(css: &mut ComputedStyle, property: &str, value: &str) -> bool {
    let Some(suffix) = property
        .strip_prefix("border-")
        .and_then(|rest| rest.strip_suffix("-radius"))
    else {
        return false;
    };
    let Some(corner) = Corner::parse(suffix) else {
        return false;
    };
    let pair = parse_corner_pair(value);
    set_horizontal(css, corner, pair.map(|values| values.0));
    set_vertical(css, corner, pair.map(|values| values.1));
    true
}

/// O valor computado de uma longhand de canto.
pub fn get_property(css: &ComputedStyle, property: &str) -> Option<String> {
    let suffix = property
        .strip_prefix("border-")
        .and_then(|rest| rest.strip_suffix("-radius"))?;
    let corner = Corner::parse(suffix)?;
    let horizontal = horizontal(css, corner)?;
    let vertical = vertical(css, corner).unwrap_or(horizontal);
    Some(if horizontal == vertical {
        super::fmt_values::fmt_px(horizontal)
    } else {
        format!(
            "{} {}",
            super::fmt_values::fmt_px(horizontal),
            super::fmt_values::fmt_px(vertical)
        )
    })
}
