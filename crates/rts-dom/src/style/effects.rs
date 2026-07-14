//! Efeitos visuais de fundo/caixa que viram DisplayItems próprios: `box-shadow` e
//! `background: linear-gradient(...)`. Parse egui-free (só produz dados); o backend
//! pinta (sombra com blur real, gradiente como mesh). Cobrem o visual dominante de
//! frameworks (cards elevados, heros/botões com gradiente).

use super::values::Rgba;

/// Uma sombra de caixa (`box-shadow: <dx> <dy> <blur> <spread> <cor>`). Só a
/// PRIMEIRA sombra da lista é modelada (a v1 ignora múltiplas, corte documentado);
/// `inset` não é suportado (vira sombra externa comum). Comprimentos em px.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BoxShadow {
    pub dx: f32,
    pub dy: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Rgba,
}

impl BoxShadow {
    /// Parseia `box-shadow: <dx> <dy> [blur] [spread] [cor]` (a 1ª sombra da lista).
    /// `none` / vazio → `None`. A cor pode vir em qualquer posição (aceita rgba()/hex);
    /// os comprimentos são os tokens numéricos, na ordem dx, dy, blur, spread.
    pub fn parse(v: &str) -> Option<BoxShadow> {
        let v = v.trim();
        if v.is_empty() || v.eq_ignore_ascii_case("none") {
            return None;
        }
        // só a 1ª sombra (antes da 1ª vírgula de TOPO — respeitando parênteses).
        let first = split_top_commas(v).into_iter().next()?;
        let toks = split_top_ws(&first);
        let mut lengths: Vec<f32> = Vec::new();
        let mut color: Option<Rgba> = None;
        for t in toks {
            if t.eq_ignore_ascii_case("inset") {
                continue; // inset tratado como sombra externa (v1)
            }
            if let Some(px) = parse_len_px(&t) {
                lengths.push(px);
            } else if let Some(c) = super::parse_color(&t) {
                color = Some(c);
            }
        }
        if lengths.len() < 2 {
            return None; // box-shadow precisa ao menos dx e dy
        }
        Some(BoxShadow {
            dx: lengths[0],
            dy: lengths[1],
            blur: lengths.get(2).copied().unwrap_or(0.0),
            spread: lengths.get(3).copied().unwrap_or(0.0),
            // default do CSS = currentColor; sem cor explícita usa preto translúcido.
            color: color.unwrap_or(0x0000_0040),
        })
    }
}

/// Um gradiente linear (`linear-gradient([<angle>,] <c0>, <c1>[, ...])`). A v1 usa a
/// 1ª e a ÚLTIMA cor da lista (interpolação de 2 pontos) e um ângulo; paradas
/// intermediárias são ignoradas (corte documentado — cobre a maioria dos heros/botões).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LinearGradient {
    pub c0: Rgba,
    pub c1: Rgba,
    /// Ângulo em graus na convenção CSS: 0=para cima (bottom→top), 90=para a direita.
    pub angle_deg: f32,
}

impl LinearGradient {
    /// Extrai um `linear-gradient(...)` de um valor de `background`/`background-image`.
    /// `None` se não é um gradiente.
    pub fn parse(v: &str) -> Option<LinearGradient> {
        let v = v.trim();
        let low = v.to_ascii_lowercase();
        let inner = if let Some(i) = low.find("linear-gradient(") {
            // pega o conteúdo entre os parênteses balanceados após "linear-gradient".
            let start = i + "linear-gradient(".len();
            let rest = &v[start..];
            let mut depth = 1i32;
            let mut end = rest.len();
            for (j, c) in rest.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = j;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            &rest[..end]
        } else {
            return None;
        };
        let parts = split_top_commas(inner);
        if parts.is_empty() {
            return None;
        }
        // 1ª parte pode ser o ângulo/direção; senão o default é 180deg (para baixo).
        let mut angle_deg = 180.0_f32;
        let mut color_parts: Vec<&str> = Vec::new();
        for (i, p) in parts.iter().enumerate() {
            let pl = p.to_ascii_lowercase();
            if i == 0 && (pl.ends_with("deg") || pl.starts_with("to ")) {
                angle_deg = parse_angle(&pl).unwrap_or(180.0);
            } else {
                color_parts.push(p);
            }
        }
        // 1ª e última cor (interpolação de 2 pontos). Uma parada pode ter posição
        // ("#fff 20%") — só o token de cor importa.
        let first_col = color_parts.first().and_then(|p| color_of_stop(p))?;
        let last_col = color_parts.last().and_then(|p| color_of_stop(p)).unwrap_or(first_col);
        Some(LinearGradient { c0: first_col, c1: last_col, angle_deg })
    }
}

/// A cor de uma parada de gradiente (`#fff` ou `#fff 20%` → só o 1º token de cor).
fn color_of_stop(stop: &str) -> Option<Rgba> {
    for tok in split_top_ws(stop) {
        if let Some(c) = super::parse_color(&tok) {
            return Some(c);
        }
    }
    None
}

/// Parseia um ângulo CSS de gradiente: `<n>deg` ou `to <lado>` → graus na convenção
/// CSS (0=topo, cresce horário). `to right`=90, `to bottom`=180, `to left`=270, `to top`=0.
fn parse_angle(v: &str) -> Option<f32> {
    let v = v.trim();
    if let Some(n) = v.strip_suffix("deg") {
        return n.trim().parse::<f32>().ok();
    }
    if let Some(dir) = v.strip_prefix("to ") {
        return Some(match dir.trim() {
            "top" => 0.0,
            "right" => 90.0,
            "bottom" => 180.0,
            "left" => 270.0,
            "top right" | "right top" => 45.0,
            "bottom right" | "right bottom" => 135.0,
            "bottom left" | "left bottom" => 225.0,
            "top left" | "left top" => 315.0,
            _ => 180.0,
        });
    }
    None
}

/// Parseia um comprimento em px (`4px`, `-1px`, `0`). Só px/unitless (o box-shadow
/// do Tailwind é sempre px); outras unidades → None (ignorado como comprimento).
fn parse_len_px(v: &str) -> Option<f32> {
    let v = v.trim();
    if let Some(n) = v.strip_suffix("px") {
        return n.trim().parse::<f32>().ok();
    }
    // "0" sem unidade é válido; qualquer outro número puro também (raro em shadow).
    v.parse::<f32>().ok()
}

/// Divide por vírgulas de TOPO (fora de parênteses) — não quebra `rgba(0,0,0,.1)`.
fn split_top_commas(v: &str) -> Vec<String> {
    split_top(v, ',')
}

/// Divide por espaços de TOPO (fora de parênteses).
fn split_top_ws(v: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in v.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Divide por um separador de TOPO (respeitando parênteses), trimando cada pedaço.
fn split_top(v: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in v.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            c if c == sep && depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    let t = cur.trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_shadow_basic() {
        let s = BoxShadow::parse("0 4px 6px rgba(0,0,0,0.1)").unwrap();
        assert_eq!(s.dx, 0.0);
        assert_eq!(s.dy, 4.0);
        assert_eq!(s.blur, 6.0);
        assert_eq!(s.spread, 0.0);
        assert_eq!(s.color, 0x0000_001A); // alpha 0.1 ≈ 0x1A
    }

    #[test]
    fn box_shadow_spread_and_none() {
        let s = BoxShadow::parse("2px 2px 5px 1px #000000").unwrap();
        assert_eq!((s.dx, s.dy, s.blur, s.spread), (2.0, 2.0, 5.0, 1.0));
        assert!(BoxShadow::parse("none").is_none());
        assert!(BoxShadow::parse("").is_none());
    }

    #[test]
    fn box_shadow_first_of_list() {
        // múltiplas sombras: só a 1ª (a vírgula interna do rgba não confunde).
        let s = BoxShadow::parse("0 1px 2px rgba(0,0,0,0.1), 0 8px 16px rgba(0,0,0,0.2)").unwrap();
        assert_eq!(s.dy, 1.0);
    }

    #[test]
    fn gradient_angle_and_colors() {
        let g = LinearGradient::parse("linear-gradient(90deg, #ff0000, #0000ff)").unwrap();
        assert_eq!(g.angle_deg, 90.0);
        assert_eq!(g.c0, 0xFF0000FF);
        assert_eq!(g.c1, 0x0000FFFF);
    }

    #[test]
    fn gradient_to_direction_and_stops() {
        let g = LinearGradient::parse("linear-gradient(to right, #fff 0%, #000 100%)").unwrap();
        assert_eq!(g.angle_deg, 90.0);
        assert_eq!(g.c0, 0xFFFFFFFF);
        assert_eq!(g.c1, 0x000000FF);
    }

    #[test]
    fn gradient_default_angle_and_non_gradient() {
        let g = LinearGradient::parse("linear-gradient(#f00, #00f)").unwrap();
        assert_eq!(g.angle_deg, 180.0); // default = to bottom
        assert!(LinearGradient::parse("#ff0000").is_none());
        assert!(LinearGradient::parse("url(x.png)").is_none());
    }
}
