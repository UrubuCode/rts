//! Interpolação de VALORES de estilo para animação (`transition`/`@keyframes`).
//!
//! O trait [`AnimValue`] dá a cada TIPO de valor a sua regra de interpolação; a
//! macro `css_props!` (em `props.rs`) gera o `ComputedStyle::interpolate_animated`
//! chamando `lerp_anim` só nos campos marcados `anim` — assim um campo animável
//! novo NUNCA fica de fora da interpolação (a lista fixa que dessincronizava do
//! `styles_differ_animated` morreu aqui). Movido do `anim.rs`, semântica idêntica.

use super::values::{Dimension, Edges, Rgba, Side};

/// Interpola um `f32` linearmente.
pub fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Interpola uma cor RGBA `0xRRGGBBAA` componente a componente (em sRGB, como os
/// browsers fazem por padrão para transições simples).
pub fn lerp_color(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let ch = |shift: u32| {
        let ca = ((a >> shift) & 0xFF) as f32;
        let cb = ((b >> shift) & 0xFF) as f32;
        (lerp_f32(ca, cb, t).round().clamp(0.0, 255.0) as u32) << shift
    };
    ch(24) | ch(16) | ch(8) | ch(0)
}

/// Interpola uma `Dimension` — só quando ambas têm a MESMA unidade (px↔px, %↔%);
/// senão salta para o destino em t>=0.5 (fiel ao "discrete" do CSS p/ não-animável).
pub fn lerp_dimension(a: Dimension, b: Dimension, t: f32) -> Dimension {
    match (a, b) {
        (Dimension::Px(x), Dimension::Px(y)) => Dimension::Px(lerp_f32(x, y, t)),
        (Dimension::Percent(x), Dimension::Percent(y)) => Dimension::Percent(lerp_f32(x, y, t)),
        (Dimension::Em(x), Dimension::Em(y)) => Dimension::Em(lerp_f32(x, y, t)),
        (Dimension::Rem(x), Dimension::Rem(y)) => Dimension::Rem(lerp_f32(x, y, t)),
        // calc interpola por COMPONENTE (a combinação linear é fechada sob lerp).
        (Dimension::Calc(x), Dimension::Calc(y)) => Dimension::Calc(crate::style::CalcLen {
            px: lerp_f32(x.px, y.px, t),
            pct: lerp_f32(x.pct, y.pct, t),
            em: lerp_f32(x.em, y.em, t),
            rem: lerp_f32(x.rem, y.rem, t),
            vw: lerp_f32(x.vw, y.vw, t),
            vh: lerp_f32(x.vh, y.vh, t),
        }),
        _ => if t < 0.5 { a } else { b }, // unidades diferentes → discreto
    }
}

/// Interpola um lado de [`Edges`]: comprimentos interpolam pela regra de
/// [`lerp_dimension`] (mesma unidade — px↔px, rem↔rem; misto salta); Auto/Unset
/// saltam discretos.
fn lerp_side(a: Side, b: Side, t: f32) -> Side {
    match (a, b) {
        (Side::Len(x), Side::Len(y)) => Side::Len(lerp_dimension(x, y, t)),
        _ => if t < 0.5 { a } else { b },
    }
}

/// A regra de interpolação de UM tipo de valor animável. Implementado por tipo;
/// a semântica de `Option` (lado ausente) é a de cada tipo (documentada no impl).
pub trait AnimValue {
    fn lerp_anim(a: &Self, b: &Self, t: f32) -> Self;
}

/// Cores (`Option<Rgba>` = `Option<u32>`): interpola sRGB quando ambos presentes;
/// um lado ausente → o presente salta discreto (não há de/para completo).
impl AnimValue for Option<Rgba> {
    fn lerp_anim(a: &Self, b: &Self, t: f32) -> Self {
        match (*a, *b) {
            (None, None) => None,
            (Some(x), Some(y)) => Some(lerp_color(x, y, t)),
            (x, y) => if t < 0.5 { x } else { y },
        }
    }
}

/// Comprimentos em pontos (`Option<f32>`, ex: border_width/corner_radius): lado
/// ausente vale 0 e interpola; ambos ausentes → `None`.
impl AnimValue for Option<f32> {
    fn lerp_anim(a: &Self, b: &Self, t: f32) -> Self {
        match (*a, *b) {
            (None, None) => None,
            (x, y) => Some(lerp_f32(x.unwrap_or(0.0), y.unwrap_or(0.0), t)),
        }
    }
}

/// Dimensões (`Option<Dimension>`, ex: width/height): interpola mesma-unidade;
/// um lado ausente salta discreto.
impl AnimValue for Option<Dimension> {
    fn lerp_anim(a: &Self, b: &Self, t: f32) -> Self {
        match (*a, *b) {
            (None, None) => None,
            (Some(x), Some(y)) => Some(lerp_dimension(x, y, t)),
            (x, y) => if t < 0.5 { x } else { y },
        }
    }
}

/// Caixa por lado ([`Edges`], padding/margin): interpola cada lado (Px↔Px; o
/// resto discreto).
impl AnimValue for Edges {
    fn lerp_anim(a: &Self, b: &Self, t: f32) -> Self {
        Edges {
            top: lerp_side(a.top, b.top, t),
            right: lerp_side(a.right, b.right, t),
            bottom: lerp_side(a.bottom, b.bottom, t),
            left: lerp_side(a.left, b.left, t),
        }
    }
}
