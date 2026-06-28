//! Estilo da SCROLLBAR via CSS — resolve a aparência da barra a partir de duas
//! sintaxes (como o Chrome real aceita ambas):
//!
//! 1. **Padrão CSS** (Firefox + Chrome novo): `scrollbar-width: thin|none|auto` e
//!    `scrollbar-color: <thumb> <track>`, declarados no elemento que rola.
//! 2. **WebKit** (Chrome clássico): os pseudo-elementos `::-webkit-scrollbar`
//!    (largura via `width`), `::-webkit-scrollbar-thumb` (cor via `background`,
//!    `border-radius`) e `::-webkit-scrollbar-track` (`background`).
//!
//! O resultado é um [`ScrollbarStyle`] neutro de backend; o egui (burro) lê isso e
//! preenche o `egui::style::ScrollStyle`. O motor NÃO conhece o egui — só produz os
//! dados. Os pseudo-elementos são lidos do CSS BRUTO (o seletor principal não modela
//! pseudo-elementos), o que mantém o matcher de seletor intacto.

use crate::style::parse_color;

/// `overflow` de um eixo — decide SE/QUANDO a barra aparece (CSS):
/// - `Visible` (default): conteúdo transborda sem barra nem clip.
/// - `Auto`: barra SÓ se o conteúdo exceder a área. (o sensato p/ uma página)
/// - `Scroll`: barra SEMPRE (forçada), mesmo cabendo.
/// - `Hidden`: corta o excesso, sem barra.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Auto,
    Scroll,
    Hidden,
}

impl Overflow {
    fn parse(v: &str) -> Option<Overflow> {
        Some(match v.trim() {
            "visible" => Overflow::Visible,
            "auto" => Overflow::Auto,
            "scroll" => Overflow::Scroll,
            "hidden" | "clip" => Overflow::Hidden,
            _ => return None,
        })
    }
    /// `true` se este eixo pode rolar (auto/scroll). `Visible`/`Hidden` não rolam.
    pub fn scrollable(self) -> bool {
        matches!(self, Overflow::Auto | Overflow::Scroll)
    }
    /// `true` se a barra é FORÇADA a aparecer (scroll), mesmo sem precisar.
    pub fn always_bar(self) -> bool {
        matches!(self, Overflow::Scroll)
    }
}

/// Largura da barra, conforme `scrollbar-width` / `::-webkit-scrollbar { width }`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BarWidth {
    /// `auto` — largura padrão do backend.
    Auto,
    /// `thin` — barra fina.
    Thin,
    /// `none` — barra escondida (rola, sem barra visível).
    None,
    /// largura explícita em px (do `::-webkit-scrollbar { width: Npx }`).
    Px(f32),
}

/// Aparência resolvida da scrollbar (neutra de backend). Todos os campos opcionais:
/// `None` = "use o default do backend".
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ScrollbarStyle {
    /// Largura da barra (`scrollbar-width` ou `::-webkit-scrollbar{width}`).
    pub width: Option<BarWidth>,
    /// Cor do "polegar" (handle), `0xRRGGBBAA` (`scrollbar-color` 1º valor /
    /// `::-webkit-scrollbar-thumb{background}`).
    pub thumb: Option<u32>,
    /// Cor do trilho, `0xRRGGBBAA` (`scrollbar-color` 2º valor /
    /// `::-webkit-scrollbar-track{background}`).
    pub track: Option<u32>,
    /// Arredondamento do handle em px (`::-webkit-scrollbar-thumb{border-radius}`).
    pub thumb_radius: Option<f32>,
    /// `overflow-x` / `overflow-y` da PÁGINA — decide se cada eixo rola e se a barra
    /// é forçada. `None` em ambos = default `visible` (mas a página rola o eixo Y por
    /// conveniência se o conteúdo exceder — ver `resolve`/o backend).
    pub overflow_x: Option<Overflow>,
    pub overflow_y: Option<Overflow>,
}

impl ScrollbarStyle {
    /// `true` se nenhuma propriedade foi definida (nada a customizar).
    pub fn is_default(&self) -> bool {
        *self == ScrollbarStyle::default()
    }

    /// Sobrepõe `other` por cima (campos `Some` de `other` vencem). Usado para a
    /// ordem de precedência (webkit por cima do padrão, p.ex.).
    pub fn merge_over(&mut self, other: &ScrollbarStyle) {
        if other.width.is_some() {
            self.width = other.width;
        }
        if other.thumb.is_some() {
            self.thumb = other.thumb;
        }
        if other.track.is_some() {
            self.track = other.track;
        }
        if other.thumb_radius.is_some() {
            self.thumb_radius = other.thumb_radius;
        }
        if other.overflow_x.is_some() {
            self.overflow_x = other.overflow_x;
        }
        if other.overflow_y.is_some() {
            self.overflow_y = other.overflow_y;
        }
    }
}

/// Resolve o estilo da scrollbar da PÁGINA a partir do CSS bruto, combinando as duas
/// sintaxes: o padrão (`scrollbar-width`/`scrollbar-color` nas regras `body`/`html`/
/// `:root`/`*`) e o WebKit (`::-webkit-scrollbar*`), com o WebKit por cima (ordem do
/// Chrome). É o ponto único que o `Dom::scrollbar_style` chama.
pub fn resolve(css: &str) -> ScrollbarStyle {
    let mut s = ScrollbarStyle::default();
    // padrão: junta as declarações de toda regra "de página" (body/html/:root/*).
    for sel in ["body", "html", ":root", "*"] {
        for body in blocks_for_plain_selector(css, sel) {
            let decls = parse_decls(&body);
            s.merge_over(&from_declarations(&decls));
        }
    }
    // WebKit vence.
    s.merge_over(&from_webkit_css(css));
    s
}

/// Corpos de regras cujo seletor é EXATAMENTE `sel` (sem pseudo-elemento) — p/ achar
/// `body { scrollbar-width: ... }`. Distinto de `blocks_for_selector` (que casa por
/// sufixo p/ os `::-webkit-*`).
fn blocks_for_plain_selector(css: &str, sel: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(pos) = css[i..].find('{') {
        let open = i + pos;
        let sel_start = css[..open].rfind('}').map(|p| p + 1).unwrap_or(0);
        let selector = css[sel_start..open].trim();
        let close = match css[open + 1..].find('}') {
            Some(p) => open + 1 + p,
            None => break,
        };
        if selector == sel {
            out.push(css[open + 1..close].to_string());
        }
        i = close + 1;
    }
    out
}

/// Lê `scrollbar-width` + `scrollbar-color` de uma lista de declarações `(prop,
/// valor)` (o `style=""` ou um bloco de regra já parseado). Sintaxe padrão CSS.
pub fn from_declarations(decls: &[(String, String)]) -> ScrollbarStyle {
    let mut s = ScrollbarStyle::default();
    for (prop, value) in decls {
        match prop.as_str() {
            "scrollbar-width" => {
                s.width = Some(match value.trim() {
                    "thin" => BarWidth::Thin,
                    "none" => BarWidth::None,
                    _ => BarWidth::Auto,
                });
            }
            "scrollbar-color" => {
                // "<thumb> <track>" — duas cores separadas por espaço.
                let mut parts = value.split_whitespace();
                if let Some(thumb) = parts.next().and_then(parse_color) {
                    s.thumb = Some(thumb);
                }
                if let Some(track) = parts.next().and_then(parse_color) {
                    s.track = Some(track);
                }
            }
            // `overflow` define os dois eixos; `overflow-x`/`-y` cada um.
            "overflow" => {
                if let Some(o) = Overflow::parse(value) {
                    s.overflow_x = Some(o);
                    s.overflow_y = Some(o);
                }
            }
            "overflow-x" => s.overflow_x = Overflow::parse(value),
            "overflow-y" => s.overflow_y = Overflow::parse(value),
            _ => {}
        }
    }
    s
}

/// Varre o CSS BRUTO procurando os blocos `::-webkit-scrollbar*` e extrai a
/// aparência (sintaxe WebKit). Retorna o estilo resolvido (vazio se nenhum bloco).
/// Procura os 3 seletores: `::-webkit-scrollbar` (width), `-thumb` (background +
/// border-radius), `-track` (background). Ignora a parte antes do `::` (qualquer
/// elemento) — no nosso modelo a scrollbar é a da página.
pub fn from_webkit_css(css: &str) -> ScrollbarStyle {
    let mut s = ScrollbarStyle::default();
    // cada seletor webkit que nos interessa → como aplicar suas declarações.
    let targets: &[(&str, fn(&mut ScrollbarStyle, &str, &str))] = &[
        ("::-webkit-scrollbar-thumb", apply_thumb),
        ("::-webkit-scrollbar-track", apply_track),
        ("::-webkit-scrollbar", apply_bar), // por último (prefixo dos outros)
    ];
    for (sel, apply) in targets {
        for body in blocks_for_selector(css, sel) {
            for (prop, value) in parse_decls(&body) {
                apply(&mut s, &prop, &value);
            }
        }
    }
    s
}

fn apply_bar(s: &mut ScrollbarStyle, prop: &str, value: &str) {
    if prop == "width" || prop == "height" {
        if let Some(px) = parse_px(value) {
            s.width = Some(BarWidth::Px(px));
        }
    }
}

fn apply_thumb(s: &mut ScrollbarStyle, prop: &str, value: &str) {
    match prop {
        "background" | "background-color" => {
            if let Some(c) = parse_color(value.trim()) {
                s.thumb = Some(c);
            }
        }
        "border-radius" => {
            if let Some(px) = parse_px(value) {
                s.thumb_radius = Some(px);
            }
        }
        _ => {}
    }
}

fn apply_track(s: &mut ScrollbarStyle, prop: &str, value: &str) {
    if prop == "background" || prop == "background-color" {
        if let Some(c) = parse_color(value.trim()) {
            s.track = Some(c);
        }
    }
}

/// Retorna os corpos `{...}` de toda regra cujo seletor termina EXATAMENTE em `sel`
/// (p.ex. `::-webkit-scrollbar-thumb`), respeitando que `-thumb`/`-track` não casem
/// com o `::-webkit-scrollbar` curto. Busca textual simples no CSS bruto.
fn blocks_for_selector(css: &str, sel: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = css.as_bytes();
    let mut i = 0;
    while let Some(pos) = css[i..].find('{') {
        let open = i + pos;
        // o seletor é o texto entre o '}' (ou início) anterior e este '{'.
        let sel_start = css[..open].rfind('}').map(|p| p + 1).unwrap_or(0);
        let selector = css[sel_start..open].trim();
        // acha o '}' que fecha este bloco.
        let close = match css[open + 1..].find('}') {
            Some(p) => open + 1 + p,
            None => break,
        };
        let body = css[open + 1..close].to_string();
        // casa se o seletor termina no `sel` exato (e o char antes não é '-', p/
        // `::-webkit-scrollbar` não casar dentro de `-thumb`).
        if selector_ends_with_exact(selector, sel) {
            out.push(body);
        }
        i = close + 1;
        let _ = bytes;
    }
    out
}

/// `true` se `selector` termina no token `sel` exato — não um prefixo dele. Ex.:
/// `body::-webkit-scrollbar` casa `::-webkit-scrollbar`, mas
/// `::-webkit-scrollbar-thumb` NÃO (o char seguinte seria '-').
fn selector_ends_with_exact(selector: &str, sel: &str) -> bool {
    match selector.find(sel) {
        Some(at) => {
            let after = &selector[at + sel.len()..];
            // nada depois (ou só espaço) → casa exato.
            after.trim().is_empty()
        }
        None => false,
    }
}

/// Parser mínimo de `prop: valor;` de um corpo de bloco.
fn parse_decls(body: &str) -> Vec<(String, String)> {
    body.split(';')
        .filter_map(|d| {
            let (p, v) = d.split_once(':')?;
            let p = p.trim();
            let v = v.trim();
            if p.is_empty() || v.is_empty() {
                None
            } else {
                Some((p.to_lowercase(), v.to_string()))
            }
        })
        .collect()
}

/// `Npx` → `N` (só px; outras unidades caem fora). Reusa a convenção do resto do CSS.
fn parse_px(value: &str) -> Option<f32> {
    let v = value.trim();
    v.strip_suffix("px").unwrap_or(v).trim().parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padrao_width_e_color() {
        let s = from_declarations(&[
            ("scrollbar-width".into(), "thin".into()),
            ("scrollbar-color".into(), "#6d5efc #1a1530".into()),
        ]);
        assert_eq!(s.width, Some(BarWidth::Thin));
        assert_eq!(s.thumb, Some(0x6d5efcff));
        assert_eq!(s.track, Some(0x1a1530ff));
    }

    #[test]
    fn webkit_thumb_track_width() {
        let css = "::-webkit-scrollbar { width: 10px } \
                   ::-webkit-scrollbar-thumb { background: #6d5efc; border-radius: 5px } \
                   ::-webkit-scrollbar-track { background: #1a1530 }";
        let s = from_webkit_css(css);
        assert_eq!(s.width, Some(BarWidth::Px(10.0)));
        assert_eq!(s.thumb, Some(0x6d5efcff));
        assert_eq!(s.thumb_radius, Some(5.0));
        assert_eq!(s.track, Some(0x1a1530ff));
    }

    #[test]
    fn webkit_thumb_nao_casa_com_bar_curto() {
        // o `::-webkit-scrollbar { width }` não deve pegar o background do -thumb.
        let css = "::-webkit-scrollbar-thumb { background: #abcdef }";
        let s = from_webkit_css(css);
        assert_eq!(s.thumb, Some(0xabcdefff));
        assert_eq!(s.width, None); // nenhum bloco `::-webkit-scrollbar` puro.
    }

    #[test]
    fn merge_webkit_vence_padrao() {
        let mut base = from_declarations(&[("scrollbar-color".into(), "#111111 #222222".into())]);
        let wk = from_webkit_css("::-webkit-scrollbar-thumb { background: #ff0000 }");
        base.merge_over(&wk);
        assert_eq!(base.thumb, Some(0xff0000ff)); // webkit venceu o thumb
        assert_eq!(base.track, Some(0x222222ff)); // track do padrão preservado
    }

    #[test]
    fn vazio_e_default() {
        assert!(from_webkit_css("body { color: red }").is_default());
        assert!(from_declarations(&[("color".into(), "red".into())]).is_default());
    }

    #[test]
    fn overflow_eixos() {
        let s = from_declarations(&[("overflow".into(), "scroll".into())]);
        assert_eq!(s.overflow_x, Some(Overflow::Scroll));
        assert_eq!(s.overflow_y, Some(Overflow::Scroll));
        assert!(Overflow::Scroll.always_bar());
        assert!(Overflow::Auto.scrollable() && !Overflow::Auto.always_bar());
        assert!(!Overflow::Hidden.scrollable() && !Overflow::Visible.scrollable());
        // overflow-x/-y separados.
        let s2 = from_declarations(&[
            ("overflow-x".into(), "hidden".into()),
            ("overflow-y".into(), "auto".into()),
        ]);
        assert_eq!(s2.overflow_x, Some(Overflow::Hidden));
        assert_eq!(s2.overflow_y, Some(Overflow::Auto));
    }

    #[test]
    fn resolve_combina_padrao_e_webkit() {
        let css = "body { scrollbar-width: thin; scrollbar-color: #111111 #222222; overflow-y: scroll } \
                   ::-webkit-scrollbar-thumb { background: #ff0000 }";
        let s = resolve(css);
        assert_eq!(s.thumb, Some(0xff0000ff)); // webkit venceu
        assert_eq!(s.track, Some(0x222222ff)); // padrão
        assert_eq!(s.overflow_y, Some(Overflow::Scroll));
    }
}
