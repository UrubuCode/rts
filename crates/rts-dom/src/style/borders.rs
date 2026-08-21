//! BORDAS POR LADO (`border-top`, `border-left-color`, …) e `outline`.
//!
//! O modelo tinha UMA borda uniforme (`border_width`/`border_style`/
//! `border_color`), e a folha real da Wikipédia usa `border-bottom` 17 vezes e
//! `border-top` 16 — uma barra separadora é quase sempre um lado só. Pintar essa
//! declaração com a borda uniforme daria uma moldura fechada onde a página pede
//! uma linha: ERRADO de forma mais visível do que ignorá-la, que era o estado
//! anterior.
//!
//! ## Como o valor por lado convive com o uniforme
//!
//! Os campos por lado são um SEGUNDO nível, não uma substituição: `border_widths`
//! (um [`Edges`], que já mescla lado a lado na cascade) e `border_<lado>_style` /
//! `border_<lado>_color`. Quem lê pede [`resolved_sides`], que faz o fallback para
//! a borda uniforme lado a lado — assim `border: 1px solid red; border-top: none`
//! dá o que o browser dá, e `border: 1px solid red` sozinho continua a passar pelo
//! caminho uniforme que já existia.
//!
//! Rejeitado: dar quatro campos completos a cada lado e apagar o uniforme. Isso
//! obrigava a reescrever o paint, o `has_box`, o `measure` e os 232 testes por uma
//! propriedade que 90% das páginas declara uniforme.
//!
//! ## `outline` (<https://developer.mozilla.org/en-US/docs/Web/CSS/outline>)
//!
//! Uma borda que NÃO ocupa espaço: desenha-se por fora da caixa e não entra no box
//! model. É por isso que vive aqui e não no `Edges` — nada no layout a soma.
//! `outline-offset` é aceite e guardado. Cortes: o outline é sempre RETANGULAR
//! (não segue o `border-radius`, ao contrário do Chrome moderno) e `auto` vale
//! `solid`, porque não temos o anel de foco próprio da plataforma.

use super::props::ComputedStyle;
use super::values::{BorderStyle, Dimension, Rgba, Side};

/// Um dos quatro lados, na ordem do CSS (top/right/bottom/left).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SideName {
    Top,
    Right,
    Bottom,
    Left,
}

impl SideName {
    /// O lado nomeado por um sufixo de propriedade (`"top"` → `Top`).
    pub fn parse(v: &str) -> Option<SideName> {
        Some(match v {
            "top" => SideName::Top,
            "right" => SideName::Right,
            "bottom" => SideName::Bottom,
            "left" => SideName::Left,
            _ => return None,
        })
    }
}

/// A borda EFETIVA de um lado, já resolvida: largura em pontos, estilo e cor.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SideBorder {
    pub width: f32,
    pub style: BorderStyle,
    pub color: Rgba,
}

impl SideBorder {
    /// `true` se este lado pinta alguma coisa (largura > 0 E estilo visível). O
    /// default do CSS para `border-style` é `none` — sem estilo declarado, uma
    /// largura sozinha não desenha nada (fiel ao Chrome).
    pub fn paints(self) -> bool {
        self.width > 0.0 && self.style.is_visible()
    }
}

/// Escreve a largura de UM lado (em `border_widths`).
pub fn set_side_width(css: &mut ComputedStyle, side: SideName, w: Option<f32>) {
    let v = match w {
        Some(w) => Side::Len(Dimension::Px(w)),
        None => Side::Unset,
    };
    match side {
        SideName::Top => css.border_widths.top = v,
        SideName::Right => css.border_widths.right = v,
        SideName::Bottom => css.border_widths.bottom = v,
        SideName::Left => css.border_widths.left = v,
    }
}

/// Escreve o estilo de UM lado.
pub fn set_side_style(css: &mut ComputedStyle, side: SideName, s: Option<BorderStyle>) {
    match side {
        SideName::Top => css.border_top_style = s,
        SideName::Right => css.border_right_style = s,
        SideName::Bottom => css.border_bottom_style = s,
        SideName::Left => css.border_left_style = s,
    }
}

/// Escreve a cor de UM lado.
pub fn set_side_color(css: &mut ComputedStyle, side: SideName, c: Option<Rgba>) {
    match side {
        SideName::Top => css.border_top_color = c,
        SideName::Right => css.border_right_color = c,
        SideName::Bottom => css.border_bottom_color = c,
        SideName::Left => css.border_left_color = c,
    }
}

/// O shorthand `border-<lado>: <width> || <style> || <color>` — os três em
/// qualquer ordem, qualquer um omitível (MDN). Igual ao `border` uniforme, mas
/// escrevendo só no lado nomeado.
///
/// `border-top: none` (só o estilo) tem de APAGAR a borda daquele lado, e é por
/// isso que o estilo omitido cai em `None` explicitamente: sem isso, um
/// `border: 1px solid` anterior sobreviveria no lado que a página desligou.
pub fn apply_side_shorthand(css: &mut ComputedStyle, side: SideName, val: &str) {
    let mut width = None;
    let mut style = None;
    let mut color = None;
    for tok in val.split_whitespace() {
        if let Some(s) = BorderStyle::parse(tok) {
            style = Some(s);
        } else if let Some(w) = parse_width_token(tok) {
            width = Some(w);
        } else if let Some(c) = super::color::parse_color(tok) {
            color = Some(c);
        }
    }
    // A spec: o shorthand SETA as três longhands, e o que foi omitido vai ao
    // valor inicial (width=medium, style=none, color=currentColor). Seguimos isso
    // para o ESTILO — que é o que decide se a linha aparece — e para a largura;
    // a cor omitida fica a herdar (`currentColor` sem um campo próprio para ela).
    set_side_style(css, side, Some(style.unwrap_or(BorderStyle::None)));
    set_side_width(css, side, Some(width.unwrap_or(3.0)));
    if color.is_some() {
        set_side_color(css, side, color);
    }
}

/// Largura de borda de um token: `thin`/`medium`/`thick` (os valores do Chrome:
/// 1/3/5 px) ou um comprimento absoluto.
pub fn parse_width_token(tok: &str) -> Option<f32> {
    match tok.to_ascii_lowercase().as_str() {
        "thin" => Some(1.0),
        "medium" => Some(3.0),
        "thick" => Some(5.0),
        _ => super::lengths::parse_len_pub(tok),
    }
}

/// `true` se o nome é uma longhand de borda POR LADO
/// (`border-<lado>-<width|style|color>`). Reconhecer pela FORMA — e não com doze
/// braços literais no `match` do parse — mantém o dispatch por nome num sítio só
/// e não deixa esquecer uma combinação.
pub fn is_longhand(prop: &str) -> bool {
    split_longhand(prop).is_some()
}

/// Parte `border-top-width` em `(Top, "width")`. `None` se não é uma longhand
/// por lado (inclui `border-width`, que é a uniforme e tem braço próprio).
fn split_longhand(prop: &str) -> Option<(SideName, &str)> {
    let rest = prop.strip_prefix("border-")?;
    let (side, what) = rest.split_once('-')?;
    // Só as três longhands reais. `border-top-left-radius` casaria a forma
    // (`top` + resto) e ficaria engolido em silêncio — escrito no raio ÚNICO,
    // arredondaria também os outros três cantos. A recusa continua certa, e agora
    // o canto tem para onde ir: `style::radius` tem um campo por canto.
    matches!(what, "width" | "style" | "color").then_some(())?;
    Some((SideName::parse(side)?, what))
}

/// Aplica uma longhand por lado já reconhecida por [`is_longhand`].
pub fn apply_longhand(css: &mut ComputedStyle, prop: &str, val: &str) {
    let Some((side, what)) = split_longhand(prop) else { return };
    match what {
        "width" => set_side_width(css, side, parse_width_token(val)),
        "style" => set_side_style(css, side, BorderStyle::parse(val)),
        "color" => set_side_color(css, side, super::color::parse_color(val)),
        // Inalcançável: o `split_longhand` acima já recusou tudo o que não é uma
        // das três. Os cantos são de `style::radius`, que tem um campo por canto.
        _ => {}
    }
}

/// `outline: <width> || <style> || <color>` — a mesma classificação por token do
/// `border`, para os campos de outline. `auto` conta como `solid` (não há anel de
/// foco de plataforma para imitar).
pub fn apply_outline_shorthand(css: &mut ComputedStyle, val: &str) {
    let mut style = None;
    for tok in val.split_whitespace() {
        if tok.eq_ignore_ascii_case("auto") {
            style = Some(BorderStyle::Solid);
        } else if let Some(s) = BorderStyle::parse(tok) {
            style = Some(s);
        } else if let Some(w) = parse_width_token(tok) {
            css.outline_width = Some(w);
        } else if let Some(c) = super::color::parse_color(tok) {
            css.outline_color = Some(c);
        }
    }
    css.outline_style = Some(style.unwrap_or(BorderStyle::None));
    if css.outline_width.is_none() {
        css.outline_width = Some(3.0); // `medium`, o inicial da spec
    }
}

/// As quatro bordas EFETIVAS (top, right, bottom, left), com o fallback para a
/// borda uniforme por lado. É o que o paint consome.
pub fn resolved_sides(css: &ComputedStyle) -> [SideBorder; 4] {
    let uw = css.border_width.unwrap_or(0.0);
    let us = css.border_style.unwrap_or(BorderStyle::None);
    let uc = css.border_color.unwrap_or(0x808080FF);
    let one = |w: Side, s: Option<BorderStyle>, c: Option<Rgba>| SideBorder {
        width: w.px().unwrap_or(uw).max(0.0),
        style: s.unwrap_or(us),
        color: c.unwrap_or(uc),
    };
    [
        one(css.border_widths.top, css.border_top_style, css.border_top_color),
        one(css.border_widths.right, css.border_right_style, css.border_right_color),
        one(css.border_widths.bottom, css.border_bottom_style, css.border_bottom_color),
        one(css.border_widths.left, css.border_left_style, css.border_left_color),
    ]
}

/// Repõe os campos POR LADO — o que o shorthand `border` tem de fazer antes de
/// escrever os seus três valores.
///
/// `border-left: 20px solid; border: 6px solid` dá 6px nos quatro lados, porque
/// o shorthand curto escreve as DOZE longhands e não só as três uniformes. Sem
/// esta limpeza, o lado declarado antes sobrevivia e a caixa saía 14px mais
/// larga do que no Chrome (medido em `claude-border-lados`).
///
/// ⚠️ CORTE: a reposição vale dentro do MESMO bloco de declarações, que é onde a
/// ordem é conhecida. Entre regras diferentes, a cascade mescla campo a campo e
/// um `border-left` de uma regra menos específica sobrevive a um `border` de uma
/// mais específica. Distingui-los exige guardar "declarado como inicial" em vez
/// de `None` em toda a struct — a mesma dívida que o shorthand `background` tem,
/// e pela mesma razão.
pub fn clear_sides(css: &mut ComputedStyle) {
    css.border_widths = super::values::Edges::default();
    css.border_top_style = None;
    css.border_right_style = None;
    css.border_bottom_style = None;
    css.border_left_style = None;
    css.border_top_color = None;
    css.border_right_color = None;
    css.border_bottom_color = None;
    css.border_left_color = None;
}

/// As larguras USADAS das quatro bordas, na ordem (top, right, bottom, left) —
/// o que o BOX MODEL soma à caixa.
///
/// Não é o mesmo que a largura declarada, e a diferença é uma regra da spec:
/// **`border-style: none` faz a largura usada ser ZERO**, por mais que o autor
/// declare `border-right-width: 30px`. Sem isto, um lado que não pinta ocupava
/// espaço na mesma — medido no corpus (`claude-border-lados`), é a diferença
/// entre a caixa de 200px que o Chrome dá e uma de 230px.
///
/// É a mesma função que decide a PINTURA ([`SideBorder::paints`]), e de
/// propósito: uma borda que ocupa espaço mas não pinta, ou o contrário, é a
/// forma mais barata de o layout e o render discordarem sobre a mesma caixa.
pub fn used_widths(css: &ComputedStyle) -> [f32; 4] {
    let s = resolved_sides(css);
    [
        if s[0].paints() { s[0].width } else { 0.0 },
        if s[1].paints() { s[1].width } else { 0.0 },
        if s[2].paints() { s[2].width } else { 0.0 },
        if s[3].paints() { s[3].width } else { 0.0 },
    ]
}

/// `true` se algum lado foi declarado à parte — o gatilho para o paint sair do
/// caminho uniforme (um `DisplayItem::Border`) e emitir uma barra por lado.
pub fn has_per_side(css: &ComputedStyle) -> bool {
    css.border_widths.any_set()
        || css.border_top_style.is_some()
        || css.border_right_style.is_some()
        || css.border_bottom_style.is_some()
        || css.border_left_style.is_some()
        || css.border_top_color.is_some()
        || css.border_right_color.is_some()
        || css.border_bottom_color.is_some()
        || css.border_left_color.is_some()
}
