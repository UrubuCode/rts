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
        "background-clip" => css.background_clip = BackgroundClip::parse(val),
        "mix-blend-mode" => css.mix_blend_mode = BlendMode::parse(val),
        "background-blend-mode" => css.background_blend_mode = BlendMode::parse(val),
        "text-shadow" => css.text_shadow = parse_text_shadow(val),
        _ => return false,
    }
    true
}

/// O valor tal como o elemento o DECLAROU (`el.style.x`), ou `""`. `None` = o
/// nome não é deste módulo. A distinção entre isto e o computado está no
/// cabeçalho de `style::initial`.
pub fn get_property(css: &ComputedStyle, name: &str) -> Option<String> {
    let s = match name {
        "background-clip" => css.background_clip.map(|v| v.css()).unwrap_or_default().to_string(),
        "mix-blend-mode" => css.mix_blend_mode.map(|v| v.css()).unwrap_or_default().to_string(),
        "background-blend-mode" => {
            css.background_blend_mode.map(|v| v.css()).unwrap_or_default().to_string()
        }
        // O Chrome serializa a sombra com a COR À FRENTE, mesmo quando o autor a
        // escreveu no fim (`2px 2px red` → `rgb(255, 0, 0) 2px 2px 0px`).
        "text-shadow" => css.text_shadow.map(fmt_shadow).unwrap_or_default(),
        _ => return None,
    };
    Some(s)
}
