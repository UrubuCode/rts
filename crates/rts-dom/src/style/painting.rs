//! A CAUDA DE PINTURA: propriedades que dizem como uma caixa é pintada, e que
//! este motor guarda sem ainda pintar por elas.
//!
//! ## O que promete, e o que não promete
//!
//! Promete que a declaração é parseada, guardada e devolvida pelo
//! `getComputedStyle`. **Não promete pixel nenhum.** Cada tipo abaixo diz, no
//! seu comentário, quem teria de a ler para ela ter efeito — porque uma
//! propriedade "reconhecida" que não faz o que o nome dela diz é pior do que uma
//! ausente, e a diferença entre as duas é estar escrito.
//!
//! Nenhuma destas esconde nem revela conteúdo: a falta de uma sombra é uma
//! sombra a menos, a falta de uma mistura é uma caixa opaca. É "não faz nada", e
//! não o caso do `clip`, onde não aplicar deixava texto de leitor de ecrã à
//! vista. Essa distinção é a única que obriga a parar antes de reconhecer.
//!
//! ## Porquê um módulo próprio
//!
//! `style::vocab` — o módulo do lote anterior, com o mesmo contrato — está em
//! 484 linhas e o teto do repositório é 500. Este lote entra ao lado dele com a
//! mesma forma (`try_apply` + `get_property`, ligados por um braço `_ if` em
//! `parse.rs` e um `or_else` em `fmt.rs`) em vez de o rebentar.

use super::effects::BoxShadow;
use super::aplica::{set_if, set_ou_limpa};
use super::props::ComputedStyle;
use super::values::Rgba;

/// `background-clip` — até onde o fundo é pintado.
///
/// GUARDADA, SEM RECORTE: quem pinta o fundo desenha sempre o retângulo da borda.
/// O consumidor seria o emissor de `DisplayItem::SolidRect`, que hoje não
/// pergunta pela caixa de recorte.
///
/// **`text` é o caso a ler com atenção, e não é uma regressão.** É metade do
/// idioma do "texto com gradiente" (`background-image: linear-gradient(...)` +
/// `background-clip: text` + `-webkit-text-fill-color: transparent`), e sem ele
/// a página mostra um retângulo de gradiente cheio em vez de letras pintadas com
/// ele. Isso já acontece hoje — o gradiente já é pintado e o `background-clip`
/// já era deitado fora —, portanto guardar o valor não muda o que se vê nem
/// para melhor nem para pior. O que muda é passar a haver onde ler quando o
/// paint quiser resolver o idioma inteiro.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BackgroundClip {
    BorderBox,
    PaddingBox,
    ContentBox,
    Text,
}

/// `mix-blend-mode` e `background-blend-mode` — como as cores de duas camadas se
/// combinam. O vocabulário é o MESMO nos dois (é o `<blend-mode>` da spec de
/// composição), e por isso é um tipo só: dois enums com as mesmas dezasseis
/// variantes seriam duas respostas à mesma pergunta.
///
/// GUARDADAS, SEM COMPOSIÇÃO: este motor pinta em ordem, com alfa simples. Um
/// modo de mistura pede que o pintor leia o que já está por baixo, e a lista de
/// display não tem essa operação. Sem consumidor à vista.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

/// Um keyword simples: a lista de pares (texto, variante), num sítio só por tipo.
/// É o mesmo macro de `style::vocab` — repetido aqui em vez de exportado porque
/// exportar um `macro_rules!` entre módulos irmãos obriga a `#[macro_export]`, que
/// o publica na raiz do crate. Sete linhas locais custam menos que um macro
/// público que ninguém fora daqui devia ver.
macro_rules! kw {
    ($t:ty { $( $s:literal => $v:path ),* $(,)? }) => {
        impl $t {
            pub fn parse(v: &str) -> Option<$t> {
                Some(match v.trim().to_ascii_lowercase().as_str() {
                    $( $s => $v, )*
                    _ => return None,
                })
            }
            pub fn css(self) -> &'static str {
                match self { $( $v => $s, )* }
            }
        }
    };
}

kw!(BackgroundClip {
    "border-box" => BackgroundClip::BorderBox,
    "padding-box" => BackgroundClip::PaddingBox,
    "content-box" => BackgroundClip::ContentBox,
    "text" => BackgroundClip::Text,
});
kw!(BlendMode {
    "normal" => BlendMode::Normal,
    "multiply" => BlendMode::Multiply,
    "screen" => BlendMode::Screen,
    "overlay" => BlendMode::Overlay,
    "darken" => BlendMode::Darken,
    "lighten" => BlendMode::Lighten,
    "color-dodge" => BlendMode::ColorDodge,
    "color-burn" => BlendMode::ColorBurn,
    "hard-light" => BlendMode::HardLight,
    "soft-light" => BlendMode::SoftLight,
    "difference" => BlendMode::Difference,
    "exclusion" => BlendMode::Exclusion,
    "hue" => BlendMode::Hue,
    "saturation" => BlendMode::Saturation,
    "color" => BlendMode::Color,
    "luminosity" => BlendMode::Luminosity,
});

/// `text-decoration-style` — a forma da linha de decoração.
///
/// GUARDADA, SEM PINTURA: o `layout::decoration_code` reduz a decoração a um
/// código de 0 a 3 (nenhuma/underline/line-through/overline) e a lista de
/// display não tem onde levar a forma. Uma linha ondulada pede ao pintor um
/// traçado que o `DisplayItem::Text` não descreve.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TextDecorationStyle {
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

/// `scrollbar-color: <polegar> <calha> | auto` — as duas cores de uma barra de
/// rolagem.
///
/// GUARDADA, SEM PINTURA: a barra é desenhada pelo backend (ver
/// `crate::scrollbar`), com as cores dele. É a mesma razão pela qual
/// `scrollbar-width` já estava guardada em `style::vocab` — a largura e a cor
/// são a mesma decisão, e ficam do mesmo lado da fronteira.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ScrollbarColor {
    pub polegar: Rgba,
    pub calha: Rgba,
}

/// `background-attachment` — se o fundo rola com o conteúdo, com a viewport
/// (`fixed`) ou com o container de rolagem (`local`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BackgroundAttachment {
    Scroll,
    Fixed,
    Local,
}

/// `box-decoration-break` — a borda e o fundo de uma caixa inline partida em
/// duas linhas: repetidos em cada fragmento (`clone`) ou cortados como se a
/// caixa fosse uma só (`slice`, o inicial).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BoxDecorationBreak {
    Slice,
    Clone,
}

/// `line-break` — a rigidez das regras de quebra de linha em CJK.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LineBreak {
    Auto,
    Loose,
    Normal,
    Strict,
    Anywhere,
}

/// `text-decoration-skip-ink` — se o sublinhado se interrompe onde passa por uma
/// descendente (o `g`, o `p`). GUARDADA SEM PINTURA: o sublinhado deste motor é
/// um código de 0 a 3 no `DisplayItem::Text`, sem geometria por glifo onde a
/// interrupção pudesse ser calculada.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SkipInk {
    Auto,
    None,
    All,
}

kw!(BackgroundAttachment {
    "scroll" => BackgroundAttachment::Scroll,
    "fixed" => BackgroundAttachment::Fixed,
    "local" => BackgroundAttachment::Local,
});
kw!(BoxDecorationBreak {
    "slice" => BoxDecorationBreak::Slice,
    "clone" => BoxDecorationBreak::Clone,
});
kw!(LineBreak {
    "auto" => LineBreak::Auto,
    "loose" => LineBreak::Loose,
    "normal" => LineBreak::Normal,
    "strict" => LineBreak::Strict,
    "anywhere" => LineBreak::Anywhere,
});
kw!(SkipInk { "auto" => SkipInk::Auto, "none" => SkipInk::None, "all" => SkipInk::All });
kw!(TextDecorationStyle {
    "solid" => TextDecorationStyle::Solid,
    "double" => TextDecorationStyle::Double,
    "dotted" => TextDecorationStyle::Dotted,
    "dashed" => TextDecorationStyle::Dashed,
    "wavy" => TextDecorationStyle::Wavy,
});

/// `text-shadow: <dx> <dy> [blur] [cor]` — a primeira sombra da lista.
///
/// **Reusa [`BoxShadow`] em vez de um segundo tipo de sombra**, que é a decisão
/// que interessa aqui: a gramática é a mesma menos o `spread` e o `inset`, e a
/// de caixa já sabe partir a lista pela vírgula de topo, aceitar a cor em
/// qualquer posição e cair no preto translúcido quando não há cor. Um
/// `TextShadow` próprio seria um segundo parser de sombra a divergir do primeiro
/// à primeira correção.
///
/// O CORTE, porque o reúso tem um: `text-shadow` **não tem `spread`**. Se uma
/// folha escrever quatro comprimentos, o parser de caixa lê o quarto como
/// spread; aqui ele é zerado, porque guardar um spread que a spec desta
/// propriedade não define seria inventar um valor. Nenhuma folha do corpus
/// escreve quatro.
///
/// GUARDADA, SEM PINTURA: a `BoxShadow` da caixa vira um `DisplayItem` próprio e
/// é pintada; esta pede ao pintor de TEXTO que desenhe o glifo deslocado e
/// borrado antes do glifo, e esse não pergunta por sombra nenhuma.
fn parse_text_shadow(v: &str) -> Option<BoxShadow> {
    let mut s = BoxShadow::parse(v)?;
    s.spread = 0.0;
    Some(s)
}

/// A cor de uma sombra no formato do computado. `fmt_color` é o mesmo que
/// `getComputedStyle` usa em todas as outras cores deste motor.
fn fmt_shadow(s: BoxShadow) -> String {
    let px = |v: f32| super::fmt_values::fmt_dim(super::values::Dimension::Px(v));
    format!(
        "{} {} {} {}",
        super::fmt_values::fmt_color(s.color as Rgba),
        px(s.dx),
        px(s.dy),
        px(s.blur)
    )
}

/// `tab-size: <número> | <comprimento>`. O número conta CARACTERES de espaço e o
/// comprimento é uma largura; guardar os dois no mesmo `f32` perderia a
/// diferença, por isso só o número entra — que é a forma que as folhas do
/// corpus escrevem (`tab-size: 4`). Um comprimento devolve `None` em vez de ser
/// guardado como se fosse uma contagem.
fn parse_tab_size(v: &str) -> Option<f32> {
    let t = v.trim();
    if t.ends_with(|c: char| c.is_ascii_alphabetic()) || t.ends_with('%') {
        return None;
    }
    t.parse::<f32>().ok().filter(|n| *n >= 0.0)
}

/// `scrollbar-color: <polegar> <calha>`. `auto` (e qualquer forma com menos de
/// duas cores) devolve `None`: a spec exige as duas, e inventar a segunda a
/// partir da primeira daria uma calha que o autor não escreveu.
fn parse_scrollbar_color(v: &str) -> Option<ScrollbarColor> {
    let toks = super::lengths::split_top_ws(v);
    if toks.len() != 2 {
        return None;
    }
    Some(ScrollbarColor {
        polegar: super::color::parse_color(&toks[0])?,
        calha: super::color::parse_color(&toks[1])?,
    })
}

/// Tenta aplicar uma propriedade deste lote. `false` = o nome não é de nenhuma.
pub fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    // O prefixo de fornecedor é um alias puro em todas as deste módulo: o
    // `-webkit-background-clip` das folhas tem exatamente os mesmos valores que
    // o nome nu, incluindo `text`.
    let name = prop
        .strip_prefix("-webkit-")
        .or_else(|| prop.strip_prefix("-moz-"))
        .or_else(|| prop.strip_prefix("-ms-"))
        .unwrap_or(prop);
    match name {
        "background-clip" => set_if(&mut css.background_clip, BackgroundClip::parse(val)),
        // `background-origin` tem as MESMAS três caixas de `background-clip`
        // menos o `text` — a spec não o define aqui. Reusa o tipo em vez de um
        // enum gémeo, e rejeita o `text` explicitamente: aceitá-lo guardaria uma
        // caixa que esta propriedade não tem.
        "background-origin" => {
            css.background_origin =
                BackgroundClip::parse(val).filter(|v| *v != BackgroundClip::Text)
        }
        "text-decoration-style" => set_if(&mut css.text_decoration_style, TextDecorationStyle::parse(val)),
        // `text-decoration-thickness: auto | from-font | <comprimento>`. Como o
        // `text-underline-offset` ao lado: `auto` é o inicial e o `Option` já o
        // diz. `from-font` pede à fonte uma métrica que o medidor não expõe, e
        // guardá-la como um comprimento seria inventar um número — cai em `None`.
        "text-decoration-thickness" => {
            css.text_decoration_thickness = match val.trim().to_ascii_lowercase().as_str() {
                "auto" | "from-font" => None,
                _ => super::lengths::parse_inset(val),
            }
        }
        // `caret-color` — a cor do cursor de texto. GUARDADA SEM PINTURA, e o
        // consumidor está mais perto do que o das outras: quem desenha o cursor
        // é o campo editável do DOM, que hoje usa a cor do texto.
        "caret-color" => set_if(&mut css.caret_color, super::color::parse_color(val)),
        // `background-attachment` — se o fundo rola com o conteúdo. GUARDADA SEM
        // EFEITO: `fixed` pede um fundo preso à viewport, e o pintor de fundo
        // desenha sempre no retângulo do elemento.
        "background-attachment" => set_if(&mut css.background_attachment, BackgroundAttachment::parse(val)),
        // `box-decoration-break` — o que acontece à borda/fundo de uma caixa
        // inline PARTIDA por uma quebra de linha. GUARDADA SEM EFEITO: o fluxo
        // inline pinta cada fragmento com a caixa inteira (que é `clone`), e não
        // tem a noção de "a mesma caixa continuada" para fazer `slice`.
        "box-decoration-break" => set_if(&mut css.box_decoration_break, BoxDecorationBreak::parse(val)),
        // `line-break` — a rigidez das regras de quebra em CJK. GUARDADA SEM
        // EFEITO: o quebrador de linha deste motor parte em espaços, e as quatro
        // variantes só se distinguem umas das outras num texto CJK.
        "line-break" => set_if(&mut css.line_break, LineBreak::parse(val)),
        "text-decoration-skip-ink" => set_if(&mut css.text_decoration_skip_ink, SkipInk::parse(val)),
        // `-webkit-text-fill-color` é a cor de PREENCHIMENTO do glifo, e no
        // WebKit ganha ao `color` quando ambas estão postas. Guardada sem
        // consumidor: quem pinta texto lê `color`. É a outra metade do idioma do
        // texto com gradiente — ver a nota em `BackgroundClip`.
        "text-fill-color" => set_if(&mut css.text_fill_color, super::color::parse_color(val)),
        // `text-underline-offset: auto | <comprimento>`. `auto` = não declarado:
        // é o inicial, e um campo `Option` já o exprime sem uma variante extra.
        "text-underline-offset" => {
            css.text_underline_offset = if val.trim().eq_ignore_ascii_case("auto") {
                None
            } else {
                super::lengths::parse_inset(val)
            }
        }
        // `tab-size: <número> | <comprimento>` — a largura de um TAB. Guardada
        // sem consumidor: o medidor de texto trata `\t` como um espaço.
        "tab-size" => set_if(&mut css.tab_size, parse_tab_size(val)),
        "scrollbar-color" => set_if(&mut css.scrollbar_color, parse_scrollbar_color(val)),
        // As três da MÁSCARA que faltavam ao lado do `mask-image`. Reusam os
        // parsers de `background-*`: a spec define-lhes a MESMA gramática, e um
        // segundo parser de posição/tamanho/repetição divergiria do primeiro à
        // primeira correção.
        //
        // GUARDADAS, SEM MÁSCARA: o que o `mask-image` faz hoje é suprimir o
        // fundo da caixa (`layout::deve_suprimir_fundo`), para um ícone não sair
        // como quadrado cheio. Essa função lê APENAS `mask_image` — verificado,
        // não assumido —, portanto guardar estas três não muda o que se pinta.
        "mask-size" => set_if(&mut css.mask_size, super::BgSize::parse(val)),
        "mask-position" => set_if(&mut css.mask_position, super::BgPosition::parse(val)),
        "mask-repeat" => set_if(&mut css.mask_repeat, super::BgRepeat::parse(val)),
        // O shorthand `mask: <image> …`. Só a imagem é lida, que é a única parte
        // com consumidor; as outras camadas caem nos campos acima quando vierem
        // escritas à parte. Reusar `parse_background` seria tentador e está
        // errado: aquele resolve `background-color`, que a máscara não tem.
        "mask" => {
            if let Some(tok) = val.split_whitespace().find(|t| t.starts_with("url(")) {
                set_if(&mut css.mask_image, Some(tok.to_string()));
            }
        }
        "mix-blend-mode" => set_if(&mut css.mix_blend_mode, BlendMode::parse(val)),
        "background-blend-mode" => set_if(&mut css.background_blend_mode, BlendMode::parse(val)),
        "text-shadow" => set_ou_limpa(&mut css.text_shadow, val, parse_text_shadow(val)),
        _ => return false,
    }
    true
}

/// O valor tal como o elemento o DECLAROU (`el.style.x`), ou `""`. `None` = o
/// nome não é deste módulo. A distinção entre isto e o computado está no
/// cabeçalho de `style::initial`.
pub fn get_property(css: &ComputedStyle, name: &str) -> Option<String> {
    let s = match name {
        "background-clip" => css
            .background_clip
            .map(|v| v.css())
            .unwrap_or_default()
            .to_string(),
        "mix-blend-mode" => css
            .mix_blend_mode
            .map(|v| v.css())
            .unwrap_or_default()
            .to_string(),
        "background-blend-mode" => css
            .background_blend_mode
            .map(|v| v.css())
            .unwrap_or_default()
            .to_string(),
        // O Chrome serializa a sombra com a COR À FRENTE, mesmo quando o autor a
        // escreveu no fim (`2px 2px red` → `rgb(255, 0, 0) 2px 2px 0px`).
        "text-shadow" => css.text_shadow.map(fmt_shadow).unwrap_or_default(),
        "background-origin" => css
            .background_origin
            .map(|v| v.css())
            .unwrap_or_default()
            .to_string(),
        "text-decoration-style" => css
            .text_decoration_style
            .map(|v| v.css())
            .unwrap_or_default()
            .to_string(),
        "-webkit-text-fill-color" | "text-fill-color" => css
            .text_fill_color
            .map(super::fmt_values::fmt_color)
            .unwrap_or_default(),
        "text-underline-offset" => css
            .text_underline_offset
            .map(super::fmt_values::fmt_dim)
            .unwrap_or_default(),
        "tab-size" => css.tab_size.map(|n| n.to_string()).unwrap_or_default(),
        "background-attachment" => {
            css.background_attachment.map(|v| v.css()).unwrap_or_default().to_string()
        }
        "box-decoration-break" => {
            css.box_decoration_break.map(|v| v.css()).unwrap_or_default().to_string()
        }
        "line-break" => css.line_break.map(|v| v.css()).unwrap_or_default().to_string(),
        "text-decoration-skip-ink" => {
            css.text_decoration_skip_ink.map(|v| v.css()).unwrap_or_default().to_string()
        }
        "caret-color" => css.caret_color.map(super::fmt_values::fmt_color).unwrap_or_default(),
        "text-decoration-thickness" => {
            css.text_decoration_thickness.map(super::fmt_values::fmt_dim).unwrap_or_default()
        }
        "scrollbar-color" => css
            .scrollbar_color
            .map(|c| {
                format!(
                    "{} {}",
                    super::fmt_values::fmt_color(c.polegar),
                    super::fmt_values::fmt_color(c.calha)
                )
            })
            .unwrap_or_default(),
        // As da máscara respondem pelos mesmos formatadores das de fundo, que é
        // a outra metade do reúso da gramática.
        "mask-position" => css
            .mask_position
            .map(|p| {
                format!(
                    "{} {}",
                    super::fmt_values::fmt_dim(p.x),
                    super::fmt_values::fmt_dim(p.y)
                )
            })
            .unwrap_or_default(),
        "mask-repeat" => css
            .mask_repeat
            .map(|r| r.css().to_string())
            .unwrap_or_default(),
        "mask-size" => match css.mask_size {
            None => String::new(),
            Some(super::BgSize::Auto) => "auto".into(),
            Some(super::BgSize::Cover) => "cover".into(),
            Some(super::BgSize::Contain) => "contain".into(),
            Some(super::BgSize::Len(w, h)) => format!(
                "{} {}",
                super::fmt_values::fmt_dim(w),
                super::fmt_values::fmt_dim(h)
            ),
        },
        _ => return None,
    };
    Some(s)
}
