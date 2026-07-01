//! Parse de COR CSS → [`Rgba`] (`0xRRGGBBAA`). Hex, `rgb()`/`rgba()` (legado por
//! vírgula E moderno por espaço com `/ alpha`), `hsl()`/`hsla()` e nomes básicos.
//! A serialização inversa (formato do browser) vive em `fmt.rs`.

use super::values::Rgba;

/// Parseia uma cor CSS para `u32` RGBA (`0xRRGGBBAA`). Suporta:
/// - hex: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` (com alpha)
/// - `rgb()`/`rgba()`: legado por vírgula OU moderno por espaço, com `/ alpha`;
///   canais 0-255 ou `%`; alpha 0-1 ou `%`
/// - `hsl()`/`hsla()`: idem, convertido para RGB
/// - nomes (tabela básica) + `transparent`. Alpha implícito = opaco.
pub fn parse_color(v: &str) -> Option<Rgba> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }
    // rgb()/rgba() — o nome da função não importa (são aliases na spec moderna).
    if let Some(inner) = func_args(v, "rgb").or_else(|| func_args(v, "rgba")) {
        return parse_rgb_fn(inner);
    }
    // hsl()/hsla() — converte para RGB.
    if let Some(inner) = func_args(v, "hsl").or_else(|| func_args(v, "hsla")) {
        return parse_hsl_fn(inner);
    }
    named_color(v)
}

/// Extrai o miolo de uma chamada `name(...)` (case-insensitive), ou `None`.
fn func_args<'a>(v: &'a str, name: &str) -> Option<&'a str> {
    let low = v.to_ascii_lowercase();
    if low.starts_with(name) && low[name.len()..].trim_start().starts_with('(') && v.ends_with(')') {
        let open = v.find('(')?;
        Some(v[open + 1..v.len() - 1].trim())
    } else {
        None
    }
}

/// Compõe `0xRRGGBBAA` opaco a partir de componentes (alpha = 0xFF).
fn rgba(r: u8, g: u8, b: u8) -> Rgba {
    rgba_a(r, g, b, 0xFF)
}

/// Compõe `0xRRGGBBAA` com alpha explícito.
fn rgba_a(r: u8, g: u8, b: u8, a: u8) -> Rgba {
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | (a as u32)
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    // expande um nibble (f → ff) ou lê um byte.
    let nib = |c: char| c.to_digit(16).map(|d| (d * 17) as u8);
    let chars: Vec<char> = hex.chars().collect();
    match chars.len() {
        // #rgb / #rgba — cada nibble expandido.
        3 | 4 => {
            let r = nib(chars[0])?;
            let g = nib(chars[1])?;
            let b = nib(chars[2])?;
            let a = if chars.len() == 4 { nib(chars[3])? } else { 0xFF };
            Some(rgba_a(r, g, b, a))
        }
        // #rrggbb / #rrggbbaa — bytes.
        6 | 8 => {
            let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
            let r = byte(0)?;
            let g = byte(2)?;
            let b = byte(4)?;
            let a = if chars.len() == 8 { byte(6)? } else { 0xFF };
            Some(rgba_a(r, g, b, a))
        }
        _ => None,
    }
}

/// Parseia os args de `rgb(...)`/`rgba(...)`: 3-4 componentes separados por VÍRGULA
/// (legado) ou ESPAÇO (moderno, com `/ alpha`). Cada R/G/B é 0-255 ou `%`; alpha
/// 0-1 ou `%`. Tolerante a mistura (a spec permite).
fn parse_rgb_fn(inner: &str) -> Option<Rgba> {
    let (main_part, slash_alpha) = split_alpha(inner);
    let comps: Vec<&str> = split_components(main_part);
    // 3 componentes (alpha via `/` opcional) OU 4 (legado: alpha é a 4ª vírgula).
    if comps.len() < 3 || comps.len() > 4 {
        return None;
    }
    let r = parse_channel_255(comps[0])?;
    let g = parse_channel_255(comps[1])?;
    let b = parse_channel_255(comps[2])?;
    // alpha: o 4º componente (legado `rgba(r,g,b,a)`) tem prioridade; senão o `/`.
    let a = if comps.len() == 4 {
        parse_alpha(comps[3])?
    } else {
        slash_alpha.and_then(parse_alpha).unwrap_or(0xFF)
    };
    Some(rgba_a(r, g, b, a))
}

/// Parseia `hsl(h, s%, l% [/ a])` para RGB. `h` em graus (0-360, wrap), `s`/`l` em
/// `%` (0-100). Conversão padrão HSL→RGB.
fn parse_hsl_fn(inner: &str) -> Option<Rgba> {
    let (main_part, slash_alpha) = split_alpha(inner);
    let comps: Vec<&str> = split_components(main_part);
    if comps.len() < 3 || comps.len() > 4 {
        return None;
    }
    let h = comps[0].trim().trim_end_matches("deg").trim().parse::<f32>().ok()?;
    let s = comps[1].trim().trim_end_matches('%').trim().parse::<f32>().ok()? / 100.0;
    let l = comps[2].trim().trim_end_matches('%').trim().parse::<f32>().ok()? / 100.0;
    let (r, g, b) = hsl_to_rgb(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    let a = if comps.len() == 4 {
        parse_alpha(comps[3])?
    } else {
        slash_alpha.and_then(parse_alpha).unwrap_or(0xFF)
    };
    Some(rgba_a(r, g, b, a))
}

/// Separa um valor de função no `/` (alpha moderno): `(antes, depois?)`.
fn split_alpha(inner: &str) -> (&str, Option<&str>) {
    match inner.split_once('/') {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => (inner, None),
    }
}

/// Divide os componentes por vírgula (legado) ou whitespace (moderno).
fn split_components(s: &str) -> Vec<&str> {
    if s.contains(',') {
        s.split(',').map(str::trim).collect()
    } else {
        s.split_whitespace().collect()
    }
}

/// Um canal R/G/B: número 0-255 OU `%` (×2.55). `none` = 0.
fn parse_channel_255(s: &str) -> Option<u8> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") {
        return Some(0);
    }
    if let Some(p) = s.strip_suffix('%') {
        let pct = p.trim().parse::<f32>().ok()?;
        return Some((pct.clamp(0.0, 100.0) * 2.55).round() as u8);
    }
    s.parse::<f32>().ok().map(|n| n.clamp(0.0, 255.0).round() as u8)
}

/// Alpha: número 0-1 OU `%` (0-100). Vira 0-255.
fn parse_alpha(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        let pct = p.trim().parse::<f32>().ok()?;
        return Some((pct.clamp(0.0, 100.0) * 2.55).round() as u8);
    }
    s.parse::<f32>().ok().map(|n| (n.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Conversão HSL→RGB (algoritmo padrão CSS). `h` graus, `s`/`l` em 0..=1.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = ((h % 360.0) + 360.0) % 360.0; // wrap para 0..360
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to(r1), to(g1), to(b1))
}

fn named_color(v: &str) -> Option<Rgba> {
    Some(match v.to_ascii_lowercase().as_str() {
        "black" => rgba(0, 0, 0),
        "white" => rgba(255, 255, 255),
        "red" => rgba(255, 0, 0),
        // CSS `green` é #008000 (0,128,0), NÃO verde puro — esse é `lime`.
        "green" => rgba(0, 128, 0),
        "lime" => rgba(0, 255, 0),
        "blue" => rgba(0, 0, 255),
        "yellow" => rgba(255, 255, 0),
        "gray" | "grey" => rgba(128, 128, 128),
        "silver" => rgba(192, 192, 192),
        "lightgray" | "lightgrey" => rgba(211, 211, 211),
        "darkgray" | "darkgrey" => rgba(169, 169, 169),
        "orange" => rgba(255, 165, 0),
        "purple" => rgba(128, 0, 128),
        "cyan" | "aqua" => rgba(0, 255, 255),
        "magenta" | "fuchsia" => rgba(255, 0, 255),
        "maroon" => rgba(128, 0, 0),
        "navy" => rgba(0, 0, 128),
        "olive" => rgba(128, 128, 0),
        "teal" => rgba(0, 128, 128),
        "pink" => rgba(255, 192, 203),
        "brown" => rgba(165, 42, 42),
        "gold" => rgba(255, 215, 0),
        "transparent" => 0x0000_0000,
        _ => return None,
    })
}
