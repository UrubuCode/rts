//! Os FORMATADORES de valor — cor, comprimento, keyword — que o
//! `getComputedStyle` usa para imprimir um valor computado.
//!
//! Separados de [`super::fmt`] quando aquele passou o teto de 500 linhas do
//! repositório. O corte é por responsabilidade e não por tamanho: `fmt.rs`
//! responde "que valor tem esta PROPRIEDADE" (um match por nome CSS), e este
//! módulo responde "como se escreve este VALOR" (uma função por tipo). Quem
//! acrescenta uma propriedade toca no primeiro; quem acrescenta um tipo de
//! valor, neste.

use super::props::ComputedStyle;
use super::values::{AlignItems, Dimension, DisplayKind, JustifyContent, Rgba};

/// A borda EFETIVA do lado nomeado por uma longhand (`border-top-width` → o lado
/// top), já com o fallback para a borda uniforme. O nome chega inteiro porque é
/// o que o `match` do `get_property` tem em mão.
pub(crate) fn side_of(css: &ComputedStyle, prop: &str) -> crate::style::SideBorder {
    let sides = crate::style::borders::resolved_sides(css);
    let idx = match prop.split('-').nth(1) {
        Some("right") => 1,
        Some("bottom") => 2,
        Some("left") => 3,
        _ => 0,
    };
    sides[idx]
}

/// Um lado de margin/padding → string CSS: comprimento cru (`fmt_dim` — px sai
/// `Npx` como antes; relativo sai `Nrem` etc, corte documentado), `auto`, ou `""`.
pub(crate) fn side_css(s: crate::style::Side) -> String {
    match s {
        crate::style::Side::Len(d) => fmt_dim(d),
        crate::style::Side::Auto => "auto".into(),
        crate::style::Side::Unset => String::new(),
    }
}

/// Como [`side_css`], mas resolve `em`/`rem` para px antes de imprimir — o
/// Chrome NUNCA devolve `getComputedStyle().marginTop` em unidade relativa.
/// `%`/`vw`/`vh` continuam crus (o corte documentado de `side_css`): esses
/// precisam da largura do containing block, que só o LAYOUT tem; `em`/`rem`
/// não — o `em` de margin/padding resolve contra o `font-size` do PRÓPRIO
/// nó, já resolvido a `Px` na cascade (`dom/cascade.rs`), e o `rem` contra a
/// base thread-local que a raiz já escreveu. Descoberto pela folha de UA
/// (lote I): `h2 { margin: 0.83em 0 }` devolvia `"0.83em"` cru.
fn arredondar_6(v: f64) -> f32 {
    (v * 1e6).round() as f32 / 1e6
}

pub(crate) fn side_css_resolved(css: &ComputedStyle, s: crate::style::Side) -> String {
    match s {
        crate::style::Side::Len(Dimension::Em(v)) => {
            let font = match css.font_size {
                Some(Dimension::Px(f)) => f,
                _ => crate::layout::DEFAULT_FONT_SIZE,
            };
            // f64: `0.83f32 × 24f32` como f32 já carrega o ruído de duas
            // conversões; o Chrome multiplica em double. Arredondado a 6
            // casas para não expor o resto do ruído de round-trip do `f32`
            // (`22.1776` saía `22.177599` sem isto).
            fmt_px(arredondar_6(v as f64 * font as f64))
        }
        crate::style::Side::Len(Dimension::Rem(v)) => {
            fmt_px(arredondar_6(v as f64 * crate::style::root_font_size() as f64))
        }
        other => side_css(other),
    }
}

/// Serializa uma cor `0xRRGGBBAA` no formato do browser: `rgb(r, g, b)` se opaco
/// (alpha 255), senão `rgba(r, g, b, a)` com alpha 0-1 (até 2 casas, sem zeros à
/// direita). É o que o `getComputedStyle().color` reporta.
pub(crate) fn fmt_color(c: Rgba) -> String {
    let r = (c >> 24) & 0xFF;
    let g = (c >> 16) & 0xFF;
    let b = (c >> 8) & 0xFF;
    let a = c & 0xFF;
    if a == 0xFF {
        format!("rgb({r}, {g}, {b})")
    } else {
        // alpha 0-1 = a/255, arredondado a 2 casas — é o que o Chrome real reporta
        // (VALIDADO no browser: #0000ff80 → "rgba(0, 0, 255, 0.5)", não 0.501961; a
        // verificação adversarial sugeriu precisão cheia mas a medição desempatou).
        let af = (a as f32 / 255.0 * 100.0).round() / 100.0;
        let mut s = format!("{af}");
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        format!("rgba({r}, {g}, {b}, {s})")
    }
}

/// Comprimento em pontos → `Npx` (sem casas se inteiro: `14px`, não `14.0px`).
pub(crate) fn fmt_px(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}px", v as i64)
    } else {
        format!("{v}px")
    }
}

/// `url(...)` computado, com o miolo SEMPRE entre aspas duplas — é o que o
/// CSSOM manda (a serialização de um `<url>` é `url(` + string serializada +
/// `)`, e uma string serializada tem aspas) e o que o Blink devolve, aspas ou
/// não no autor: `url(x.png)` e `url('x.png')` respondem os dois
/// `url("x.png")`. Guardamos o valor CRU (o token da folha, sem normalizar —
/// ver `list_style_image`/`bg_image`/`cursor` na tabela), então quem SERIALIZA
/// é quem tem de normalizar, não quem guarda.
///
/// `raw` que não é um `url(...)` (uma keyword de `cursor`, por exemplo) volta
/// tal e qual — esta função só reconhece a forma funcional.
pub(crate) fn fmt_url(raw: &str) -> String {
    let raw = raw.trim();
    let Some(miolo) = raw
        .strip_prefix("url(")
        .or_else(|| raw.strip_prefix("URL("))
        .and_then(|s| s.strip_suffix(')'))
    else {
        return raw.to_string();
    };
    let miolo = miolo.trim();
    let sem_aspas = miolo
        .strip_prefix('"')
        .or_else(|| miolo.strip_prefix('\''))
        .map(|s| s.trim_end_matches(['"', '\'']))
        .unwrap_or(miolo);
    format!("url(\"{sem_aspas}\")")
}

/// Uma `Dimension` computada → string CSS (px/%/auto…).
pub(crate) fn fmt_dim(d: Dimension) -> String {
    match d {
        Dimension::Px(v) => fmt_px(v),
        Dimension::Percent(p) => format!("{p}%"),
        Dimension::Em(v) => format!("{v}em"),
        Dimension::Rem(v) => format!("{v}rem"),
        Dimension::Vw(v) => format!("{v}vw"),
        Dimension::Vh(v) => format!("{v}vh"),
        Dimension::Ex(v) => format!("{v}ex"),
        Dimension::Ch(v) => format!("{v}ch"),
        Dimension::Auto => "auto".into(),
        Dimension::MaxContent => "max-content".into(),
        // calc: reconstrói a forma canônica com os termos não-zero.
        Dimension::Calc(c) => {
            let mut parts: Vec<String> = Vec::new();
            for (v, u) in [
                (c.px, "px"),
                (c.pct, "%"),
                (c.em, "em"),
                (c.rem, "rem"),
                (c.vw, "vw"),
                (c.vh, "vh"),
            ] {
                if v != 0.0 {
                    parts.push(format!("{v}{u}"));
                }
            }
            if parts.is_empty() {
                "0px".into()
            } else {
                format!("calc({})", parts.join(" + "))
            }
        }
    }
}

pub(crate) fn fmt_justify(j: JustifyContent) -> String {
    match j {
        JustifyContent::FlexStart => "flex-start",
        JustifyContent::FlexEnd => "flex-end",
        JustifyContent::Center => "center",
        JustifyContent::SpaceBetween => "space-between",
        JustifyContent::SpaceAround => "space-around",
        JustifyContent::SpaceEvenly => "space-evenly",
        JustifyContent::Left => "left",
        JustifyContent::Right => "right",
    }
    .into()
}

pub(crate) fn fmt_align(a: AlignItems) -> String {
    match a {
        AlignItems::Stretch => "stretch",
        AlignItems::FlexStart => "flex-start",
        AlignItems::FlexEnd => "flex-end",
        AlignItems::Center => "center",
    }
    .into()
}

/// `DisplayKind` → keyword CSS VÁLIDO para `getComputedStyle('display')`. NB:
/// `FlexWrap` é só um estado interno (flex + flex-wrap) — para a propriedade
/// `display` o keyword é `flex` (flex-wrap é uma propriedade separada). Não usar
/// `{:?}` (geraria `flexwrap`, inválido).
/// O keyword CSS de um `Overflow`. Vive aqui (e não em `scrollbar.rs`) porque
/// serializar um valor computado é o trabalho DESTE módulo — o `scrollbar` sabe
/// rolar, não sabe imprimir.
/// Uma lista de trilhas de grid → a forma CSS (`100px 1fr auto`). `None` = a
/// propriedade não foi declarada.
pub(crate) fn fmt_tracks(t: Option<&Vec<crate::style::GridTrack>>) -> String {
    match t {
        None => String::new(),
        Some(v) => v
            .iter()
            .map(|track| match track {
                crate::style::GridTrack::Fixed(d) => fmt_dim(*d),
                crate::style::GridTrack::Fr(f) => format!("{f}fr"),
                crate::style::GridTrack::Auto => "auto".to_string(),
                crate::style::GridTrack::Bounded { min, max } => {
                    format!("minmax({}, {})", fmt_dim(*min), fmt_dim(*max))
                }
                crate::style::GridTrack::Intrinsic { min, max } => {
                    format!("minmax({}, {})", fmt_track_bound(min), fmt_track_bound(max))
                }
                // Nunca chega aqui em uso normal: `dom/estilo.rs` serializa
                // `grid-template-columns`/`-rows` a partir dos tamanhos JÁ
                // RESOLVIDOS em `list.grid_column_tracks` (ver o comentário
                // lá), nunca chamando `fmt_tracks` sobre a declaração crua
                // quando ela contém um `repeat(auto-fill|auto-fit, …)` por
                // resolver. A forma crua fica aqui só para não deixar o
                // `match` incompleto.
                crate::style::GridTrack::AutoRepeat { fit, .. } => {
                    format!("repeat({}, …)", if *fit { "auto-fit" } else { "auto-fill" })
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Um lado de `minmax()` intrínseco, na forma CSS — só usado pela forma crua
/// de `fmt_tracks` (ver o comentário no `AutoRepeat` acima).
fn fmt_track_bound(b: &crate::style::TrackBound) -> String {
    match b {
        crate::style::TrackBound::Fixed(d) => fmt_dim(*d),
        crate::style::TrackBound::MinContent => "min-content".to_string(),
        crate::style::TrackBound::MaxContent => "max-content".to_string(),
        crate::style::TrackBound::FitContent(d) => format!("fit-content({})", fmt_dim(*d)),
    }
}

pub(crate) fn overflow_css(o: crate::scrollbar::Overflow) -> &'static str {
    match o {
        crate::scrollbar::Overflow::Visible => "visible",
        crate::scrollbar::Overflow::Auto => "auto",
        crate::scrollbar::Overflow::Scroll => "scroll",
        crate::scrollbar::Overflow::Hidden => "hidden",
    }
}

/// `DisplayKind` → o keyword CSS. `pub(crate)` porque o módulo do valor
/// INICIAL precisa do mesmo mapeamento para a resposta que vem da tag: duas
/// tabelas de keywords divergiriam na primeira variante nova (já divergiram uma
/// vez, quando as caixas de tabela entraram).
pub(crate) fn display_css(d: DisplayKind) -> &'static str {
    match d {
        DisplayKind::Block => "block",
        DisplayKind::Flex | DisplayKind::FlexWrap => "flex",
        DisplayKind::InlineFlex => "inline-flex",
        DisplayKind::Inline => "inline",
        DisplayKind::InlineBlock => "inline-block",
        DisplayKind::Grid => "grid",
        // As caixas de tabela e o `list-item` respondem o keyword que lhes deu
        // origem: `getComputedStyle` devolve o `display` USADO, e um `<li>` que
        // gera marcador é `list-item`, não `block`.
        DisplayKind::ListItem => "list-item",
        DisplayKind::Table => "table",
        DisplayKind::TableRow => "table-row",
        DisplayKind::TableCell => "table-cell",
        DisplayKind::TableCaption => "table-caption",
        DisplayKind::TableRowGroup => "table-row-group",
        DisplayKind::None => "none",
    }
}
