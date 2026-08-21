//! O VOCABULÁRIO do segundo lote de propriedades: os keywords novos, o parse e a
//! serialização computada de cada um, num sítio só.
//!
//! ## O que este módulo promete, e o que NÃO promete
//!
//! Promete que a declaração deixa de ser deitada fora: é parseada, guardada no
//! campo de `ComputedStyle` e devolvida por `getComputedStyle`. **Não promete
//! geometria.** `text-overflow: ellipsis`, `-webkit-line-clamp`, `object-fit`,
//! `align-content` e as restantes só mudam a caixa quando o LAYOUT as ler, e o
//! layout não as lê hoje — o fluxo inline e o de blocos estão a ser mexidos por
//! outra gente, e escrever um consumo em cima disso seria dois motores a decidir
//! a mesma caixa.
//!
//! Cada propriedade abaixo diz, no seu comentário, qual das duas coisas é. Uma
//! propriedade "reconhecida" que não faz o que o nome dela diz é pior do que uma
//! ausente — a diferença entre as duas é estar escrito.
//!
//! ## Porquê um módulo e não mais braços em `parse.rs`/`fmt.rs`
//!
//! `parse.rs` já está em 660 linhas e `fmt.rs` em 400, ambos acima do teto de 500
//! do repositório para um ficheiro que não é codegen. Um lote de quinze
//! propriedades entra por um módulo próprio, ligado por UM braço em cada um dos
//! dois — que é a regra da casa para código novo em ficheiro já grande.

use super::background::BgPosition;
use super::lengths::{parse_dimension, parse_len_pub, split_top_ws};
use super::props::ComputedStyle;
use super::values::{AlignItems, Dimension, JustifyContent};

/// `text-overflow` — o que fazer ao texto que não cabe na caixa.
///
/// GUARDADA, SEM GEOMETRIA: quem corta a linha é o fluxo inline, que ainda não
/// pergunta por isto. O valor está certo no computed; a linha continua a passar
/// por fora.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TextOverflow {
    Clip,
    Ellipsis,
}

/// `text-wrap` — a estratégia de quebra. `Balance`/`Pretty` pedem uma segunda
/// passada sobre as linhas já quebradas; nenhuma existe, e por isso as duas são
/// guardadas e tratadas como `Wrap` por quem as ler.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TextWrap {
    Wrap,
    Nowrap,
    Balance,
    Pretty,
}

/// `object-fit` — como um `<img>`/`<video>` preenche a caixa dele.
/// GUARDADA, SEM GEOMETRIA: quem escala a imagem é o render.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ObjectFit {
    Fill,
    Contain,
    Cover,
    None,
    ScaleDown,
}

/// `unicode-bidi` — o nível de isolamento bidirecional. GUARDADA, SEM EFEITO:
/// não há algoritmo bidi nenhum no motor, e `direction: rtl` também ainda não
/// inverte. Reconhecê-la serve para não a confundir com uma propriedade em falta.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum UnicodeBidi {
    Normal,
    Embed,
    Isolate,
    BidiOverride,
    IsolateOverride,
    Plaintext,
}

/// `hyphens` — hifenização automática. GUARDADA, SEM EFEITO: não há dicionário
/// de hifenização; `manual` (respeitar `&shy;`) depende do fluxo inline.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hyphens {
    None,
    Manual,
    Auto,
}

/// `scrollbar-width` — a espessura da barra de rolagem deste container.
/// GUARDADA, SEM GEOMETRIA: a largura da barra vive em `scrollbar.rs` e é uma
/// constante do backend; `thin`/`none` mudariam a largura de conteúdo disponível,
/// que é uma decisão de layout.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ScrollbarWidth {
    Auto,
    Thin,
    None,
}

/// `caption-side` — o lado em que a legenda de uma tabela é colocada.
/// GUARDADA, SEM GEOMETRIA: a colocação é do layout de tabela.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CaptionSide {
    Top,
    Bottom,
}

/// `pointer-events` — se o elemento é alvo de cliques. Só as duas formas que um
/// documento HTML usa; os valores de SVG (`visiblePainted` e companhia) não são
/// modelados e caem como não-declarado, que é o mesmo que `auto`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PointerEvents {
    Auto,
    None,
}

/// Um keyword simples: a lista de pares (texto, variante), num sítio só por tipo.
macro_rules! kw {
    ($t:ty { $( $s:literal => $v:path ),* $(,)? }) => {
        impl $t {
            /// O valor CSS → a variante. `None` para um valor que a spec não tem
            /// (ou que este motor não modela).
            pub fn parse(v: &str) -> Option<$t> {
                Some(match v.trim().to_ascii_lowercase().as_str() {
                    $( $s => $v, )*
                    _ => return None,
                })
            }
            /// A variante → o texto que `getComputedStyle` devolve.
            pub fn css(self) -> &'static str {
                match self { $( $v => $s, )* }
            }
        }
    };
}

kw!(TextOverflow { "clip" => TextOverflow::Clip, "ellipsis" => TextOverflow::Ellipsis });
kw!(TextWrap {
    "wrap" => TextWrap::Wrap,
    "nowrap" => TextWrap::Nowrap,
    "balance" => TextWrap::Balance,
    "pretty" => TextWrap::Pretty,
});
kw!(ObjectFit {
    "fill" => ObjectFit::Fill,
    "contain" => ObjectFit::Contain,
    "cover" => ObjectFit::Cover,
    "none" => ObjectFit::None,
    "scale-down" => ObjectFit::ScaleDown,
});
kw!(UnicodeBidi {
    "normal" => UnicodeBidi::Normal,
    "embed" => UnicodeBidi::Embed,
    "isolate" => UnicodeBidi::Isolate,
    "bidi-override" => UnicodeBidi::BidiOverride,
    "isolate-override" => UnicodeBidi::IsolateOverride,
    "plaintext" => UnicodeBidi::Plaintext,
});
kw!(Hyphens { "none" => Hyphens::None, "manual" => Hyphens::Manual, "auto" => Hyphens::Auto });
kw!(ScrollbarWidth {
    "auto" => ScrollbarWidth::Auto,
    "thin" => ScrollbarWidth::Thin,
    "none" => ScrollbarWidth::None,
});
kw!(CaptionSide { "top" => CaptionSide::Top, "bottom" => CaptionSide::Bottom });
kw!(PointerEvents { "auto" => PointerEvents::Auto, "none" => PointerEvents::None });

/// `font-stretch` em PERCENTAGEM (100 = `normal`), que é a forma em que a spec
/// define os keywords e a forma em que o computed responde. `None` se o valor não
/// é nem keyword nem percentagem.
fn parse_font_stretch(v: &str) -> Option<f32> {
    let low = v.trim().to_ascii_lowercase();
    let pct = match low.as_str() {
        "ultra-condensed" => 50.0,
        "extra-condensed" => 62.5,
        "condensed" => 75.0,
        "semi-condensed" => 87.5,
        "normal" => 100.0,
        "semi-expanded" => 112.5,
        "expanded" => 125.0,
        "extra-expanded" => 150.0,
        "ultra-expanded" => 200.0,
        _ => return low.strip_suffix('%')?.trim().parse::<f32>().ok(),
    };
    Some(pct)
}

/// `zoom: <número> | <percentagem> | normal` → o fator (1.0 = sem zoom).
fn parse_zoom(v: &str) -> Option<f32> {
    let low = v.trim().to_ascii_lowercase();
    if low == "normal" {
        return Some(1.0);
    }
    if let Some(p) = low.strip_suffix('%') {
        return p.trim().parse::<f32>().ok().map(|n| n / 100.0);
    }
    low.parse::<f32>().ok()
}

/// Tenta aplicar uma propriedade deste lote. `false` = o nome não é de nenhuma
/// delas, e o `parse` conta-a como ignorada.
pub fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    // O prefixo de fornecedor é um alias do mesmo nome — exceto onde o valor
    // também difere, e nenhuma deste lote é desse caso.
    let name = prop.strip_prefix("-webkit-").or_else(|| prop.strip_prefix("-moz-")).unwrap_or(prop);
    match name {
        // ── COM EFEITO REAL: caem em mecanismos que já são consumidos ──────────
        // Os dois eixos de `background-position` em separado. O campo é o mesmo
        // que o shorthand escreve, portanto o render já os pinta.
        "background-position-x" => {
            let mut p = css.bg_position.unwrap_or_default();
            let Some(x) = parse_dimension_or_keyword(val, true) else { return true };
            p.x = x;
            css.bg_position = Some(p);
        }
        "background-position-y" => {
            let mut p = css.bg_position.unwrap_or_default();
            let Some(y) = parse_dimension_or_keyword(val, false) else { return true };
            p.y = y;
            css.bg_position = Some(p);
        }
        // `place-content: <align> <justify>` e `place-self: <align> <justify>` —
        // só expandem para os campos que já existem (o mesmo que `flex-flow` faz).
        "place-content" => {
            let t = split_top_ws(val);
            if let Some(j) = t.last().and_then(|s| JustifyContent::parse(s)) {
                css.justify = Some(j);
            }
            if let Some(a) = t.first().and_then(|s| JustifyContent::parse(s)) {
                css.align_content = Some(a);
            }
        }
        "place-self" => {
            let t = split_top_ws(val);
            if let Some(a) = t.first().and_then(|s| AlignItems::parse(s)) {
                css.align_self = Some(a);
            }
            if let Some(j) = t.last().and_then(|s| AlignItems::parse(s)) {
                css.justify_self = Some(j);
            }
        }

        // ── GUARDADAS, SEM GEOMETRIA (o motivo está no tipo de cada uma) ───────
        // `align-content` reusa o vocabulário de `JustifyContent` em vez de um
        // enum próprio com as mesmas seis variantes. O corte: `stretch`/`normal`
        // chegam como `FlexStart`, porque é o que o layout faz hoje com `stretch`
        // no eixo cruzado (ver a nota em `AlignItems::Stretch`).
        "align-content" => css.align_content = JustifyContent::parse(val),
        // `justify-self` reusa `AlignItems` pelo mesmo motivo que
        // `grid_justify_items` já reusa: é o mesmo conjunto de posições.
        "justify-self" => css.justify_self = AlignItems::parse(val),
        "text-overflow" => css.text_overflow = TextOverflow::parse(val),
        "text-wrap" | "text-wrap-mode" => css.text_wrap = TextWrap::parse(val),
        "object-fit" => css.object_fit = ObjectFit::parse(val),
        // `object-position` tem a MESMA gramática de `background-position` — reusa
        // o parser dela em vez de um segundo parser de posição.
        "object-position" => css.object_position = BgPosition::parse(val),
        "unicode-bidi" => css.unicode_bidi = UnicodeBidi::parse(val),
        "hyphens" => css.hyphens = Hyphens::parse(val),
        "scrollbar-width" => css.scrollbar_width = ScrollbarWidth::parse(val),
        "caption-side" => css.caption_side = CaptionSide::parse(val),
        "pointer-events" => css.pointer_events = PointerEvents::parse(val),
        "font-stretch" => css.font_stretch = parse_font_stretch(val),
        "zoom" => css.zoom = parse_zoom(val),
        "word-spacing" => {
            // `normal` é 0 — a mesma convenção do `letter-spacing` ao lado.
            css.word_spacing = if val.trim().eq_ignore_ascii_case("normal") {
                Some(0.0)
            } else {
                parse_len_pub(val)
            }
        }
        // `-webkit-line-clamp: <n>` — corta o bloco a n linhas. `none` = sem
        // limite. Guardada; quem contaria as linhas é o fluxo inline.
        "line-clamp" => {
            css.line_clamp = if val.trim().eq_ignore_ascii_case("none") {
                None
            } else {
                val.trim().parse::<i32>().ok().filter(|n| *n > 0)
            }
        }
        "column-width" => css.column_width = parse_dimension(val),
        _ => return false,
    }
    true
}

/// Um valor de eixo de `background-position-x|y`: comprimento/percentagem ou o
/// keyword do eixo. `horizontal` escolhe qual dos dois conjuntos de keywords vale.
fn parse_dimension_or_keyword(v: &str, horizontal: bool) -> Option<Dimension> {
    let low = v.trim().to_ascii_lowercase();
    let pct = match (low.as_str(), horizontal) {
        ("left", true) | ("top", false) => 0.0,
        ("center", _) => 50.0,
        ("right", true) | ("bottom", false) => 100.0,
        _ => return parse_dimension(&low),
    };
    Some(Dimension::Percent(pct))
}

/// O valor COMPUTADO das propriedades deste lote, por nome. `None` = o nome não é
/// deste lote (o `fmt` continua a decidir o que responder).
pub fn get_property(css: &ComputedStyle, name: &str) -> Option<String> {
    let s = match name {
        "text-overflow" => css.text_overflow.map(|v| v.css().to_string()).unwrap_or_else(|| "clip".into()),
        "text-wrap" => css.text_wrap.map(|v| v.css().to_string()).unwrap_or_else(|| "wrap".into()),
        "object-fit" => css.object_fit.map(|v| v.css().to_string()).unwrap_or_else(|| "fill".into()),
        "unicode-bidi" => css.unicode_bidi.map(|v| v.css().to_string()).unwrap_or_else(|| "normal".into()),
        "hyphens" => css.hyphens.map(|v| v.css().to_string()).unwrap_or_else(|| "manual".into()),
        "scrollbar-width" => {
            css.scrollbar_width.map(|v| v.css().to_string()).unwrap_or_else(|| "auto".into())
        }
        "caption-side" => css.caption_side.map(|v| v.css().to_string()).unwrap_or_else(|| "top".into()),
        "pointer-events" => {
            css.pointer_events.map(|v| v.css().to_string()).unwrap_or_else(|| "auto".into())
        }
        // O computed de `font-stretch` é a percentagem, mesmo quando o autor
        // escreveu o keyword — é o que o Chrome responde.
        "font-stretch" => format!("{}%", css.font_stretch.unwrap_or(100.0)),
        "zoom" => format!("{}", css.zoom.unwrap_or(1.0)),
        "word-spacing" => format!("{}px", css.word_spacing.unwrap_or(0.0)),
        "-webkit-line-clamp" | "line-clamp" => {
            css.line_clamp.map(|n| n.to_string()).unwrap_or_else(|| "none".into())
        }
        "column-width" => css.column_width.map(super::fmt_values::fmt_dim).unwrap_or_else(|| "auto".into()),
        "object-position" => css
            .object_position
            .map(|p| format!("{} {}", super::fmt_values::fmt_dim(p.x), super::fmt_values::fmt_dim(p.y)))
            .unwrap_or_else(|| "50% 50%".into()),
        _ => return None,
    };
    Some(s)
}
