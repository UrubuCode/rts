//! O SHORTHAND `background` e as suas longhands de camada (image/position/size/
//! repeat), num módulo próprio.
//!
//! Vive fora de `parse.rs`/`values.rs` porque os dois já passam do teto de 500
//! linhas do repositório e porque o shorthand é a única propriedade da folha real
//! que precisa de um TOKENIZADOR próprio: `background: #fff url(a.png) no-repeat
//! center / cover` mistura cinco tipos de valor sem ordem fixa, e classificar
//! token a token é o que a MDN descreve (<https://developer.mozilla.org/en-US/
//! docs/Web/CSS/background>). A alternativa — mais um braço gigante no `match` do
//! `parse_inline_block` — foi rejeitada por engordar o ficheiro que já é o maior
//! do módulo.
//!
//! ## O que NÃO é coberto, e porquê
//!
//! - **Múltiplas camadas** (`background: a, b`): só a PRIMEIRA camada é lida. O
//!   modelo de fundo do motor é uma cor + um gradiente por caixa (o paint emite um
//!   `SolidRect` ou um `GradientRect`), então uma segunda camada não teria onde ser
//!   pintada — guardá-la seria estado que ninguém lê.
//! - **`url(...)` não é carregado.** O nome fica guardado (`bg_image`) para o
//!   `getComputedStyle` o reportar, mas o motor de CSS não busca imagens — quem
//!   carrega bitmap é o `<img>`, pelo DOM. Uma imagem de fundo, portanto, não
//!   pinta; a cor da mesma declaração pinta.
//! - **O reset das longhands omitidas.** A spec diz que o shorthand devolve ao
//!   valor inicial toda longhand que não nomeia; aqui ele SETA só o que nomeia.
//!   Fazê-lo direito exigia distinguir "não declarado" de "declarado como
//!   inicial" em `Option<T>`, o que muda o merge de toda a cascade — e o efeito
//!   visível seria um gradiente de uma regra menos específica sobreviver a um
//!   `background: red` de uma mais específica. Fica anotado como o corte que é.

use super::color::parse_color;
use super::values::Dimension;

/// `background-repeat` — como a imagem de fundo se repete. Sem imagem de fundo
/// pintada, isto é hoje um valor ACEITE E SERIALIZADO (o `getComputedStyle` da
/// página lê-o); o efeito chega quando o fundo com imagem pintar.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BgRepeat {
    #[default]
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,
    Space,
    Round,
}

impl BgRepeat {
    pub fn parse(v: &str) -> Option<BgRepeat> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "repeat" => BgRepeat::Repeat,
            "repeat-x" => BgRepeat::RepeatX,
            "repeat-y" => BgRepeat::RepeatY,
            "no-repeat" => BgRepeat::NoRepeat,
            "space" => BgRepeat::Space,
            "round" => BgRepeat::Round,
            _ => return None,
        })
    }

    /// O keyword CSS (o que o `getComputedStyle` reporta).
    pub fn css(self) -> &'static str {
        match self {
            BgRepeat::Repeat => "repeat",
            BgRepeat::RepeatX => "repeat-x",
            BgRepeat::RepeatY => "repeat-y",
            BgRepeat::NoRepeat => "no-repeat",
            BgRepeat::Space => "space",
            BgRepeat::Round => "round",
        }
    }
}

/// `background-position` — a origem da imagem na caixa, nos dois eixos. As
/// keywords viram PERCENTAGENS (`left`=0%, `center`=50%, `right`=100%), que é
/// exatamente o que o browser computa: `getComputedStyle` reporta `0% 50%` para
/// `background-position: left`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BgPosition {
    pub x: Dimension,
    pub y: Dimension,
}

impl Default for BgPosition {
    /// O inicial da spec: `0% 0%`.
    fn default() -> Self {
        BgPosition {
            x: Dimension::Percent(0.0),
            y: Dimension::Percent(0.0),
        }
    }
}

impl BgPosition {
    /// `background-position: <x> [<y>]` — keywords e/ou comprimentos. Um valor só
    /// define o eixo X e deixa o Y em `center` (regra da MDN), o que é o que faz
    /// `background-position: right` colar a imagem à direita e ao meio.
    pub fn parse(v: &str) -> Option<BgPosition> {
        let toks: Vec<&str> = v.split_whitespace().collect();
        if toks.is_empty() || toks.len() > 2 {
            return None; // 3/4 valores (a forma com offsets) não é modelada
        }
        let mut x = None;
        let mut y = None;
        for t in &toks {
            match t.to_ascii_lowercase().as_str() {
                "left" => x = Some(Dimension::Percent(0.0)),
                "right" => x = Some(Dimension::Percent(100.0)),
                "top" => y = Some(Dimension::Percent(0.0)),
                "bottom" => y = Some(Dimension::Percent(100.0)),
                // `center` é ambíguo: preenche o eixo que ainda estiver livre.
                "center" => {
                    if x.is_none() && toks.len() == 1 {
                        x = Some(Dimension::Percent(50.0));
                        y = Some(Dimension::Percent(50.0));
                    } else if x.is_none() {
                        x = Some(Dimension::Percent(50.0));
                    } else {
                        y = Some(Dimension::Percent(50.0));
                    }
                }
                other => {
                    let d = super::lengths::parse_dimension_pub(other)?;
                    if x.is_none() {
                        x = Some(d);
                    } else {
                        y = Some(d);
                    }
                }
            }
        }
        let x = x?;
        // Um valor só ⇒ o outro eixo é `center` (MDN), exceto quando o keyword
        // dado era vertical (`top`/`bottom`), caso em que o horizontal é que fica
        // ao centro.
        Some(BgPosition {
            x,
            y: y.unwrap_or(Dimension::Percent(50.0)),
        })
    }
}

/// `background-size` — `cover`/`contain`/`auto` ou um par de comprimentos.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum BgSize {
    #[default]
    Auto,
    Cover,
    Contain,
    /// `<width> <height>` (o segundo omitido = `auto`, representado por `Auto`
    /// no par — a spec chama-lhe "auto" e o browser reporta-o assim).
    Len(Dimension, Dimension),
}

impl BgSize {
    pub fn parse(v: &str) -> Option<BgSize> {
        let t = v.trim();
        if t.eq_ignore_ascii_case("cover") {
            return Some(BgSize::Cover);
        }
        if t.eq_ignore_ascii_case("contain") {
            return Some(BgSize::Contain);
        }
        if t.eq_ignore_ascii_case("auto") {
            return Some(BgSize::Auto);
        }
        let toks: Vec<&str> = t.split_whitespace().collect();
        match toks.as_slice() {
            [w] => Some(BgSize::Len(
                super::lengths::parse_dimension_pub(w)?,
                Dimension::Auto,
            )),
            [w, h] => Some(BgSize::Len(
                super::lengths::parse_dimension_pub(w)?,
                super::lengths::parse_dimension_pub(h)?,
            )),
            _ => None,
        }
    }
}

/// O que uma declaração `background: ...` nomeou. Campos `None` = não nomeados
/// (o chamador não os toca — ver o corte do reset no topo do módulo).
#[derive(Default)]
pub struct BackgroundShorthand {
    pub color: Option<super::values::Rgba>,
    pub gradient: Option<super::effects::LinearGradient>,
    pub image: Option<String>,
    pub position: Option<BgPosition>,
    pub size: Option<BgSize>,
    pub repeat: Option<BgRepeat>,
}

/// Parseia `background: <valor>` classificando token a token, na ordem em que a
/// MDN permite (qualquer). O `/` separa `position / size` — por isso a posição só
/// é aceite ANTES dele.
///
/// A classificação é por tentativa e não por posição porque o CSS real escreve
/// `#fff url(x) no-repeat` e `no-repeat url(x) #fff` com o mesmo significado;
/// uma máquina de estados posicional teria de aceitar as duas de qualquer forma.
pub fn parse_background(val: &str) -> BackgroundShorthand {
    let mut out = BackgroundShorthand::default();
    // Só a primeira camada (ver o corte no topo). A vírgula de TOPO separa
    // camadas; as de dentro de `rgb(...)`/`linear-gradient(...)` não contam.
    let layer = super::lengths::split_top(val, ',')
        .into_iter()
        .next()
        .unwrap_or_default();
    // Um gradiente ocupa o token inteiro (tem espaços dentro dos parênteses) —
    // testá-lo primeiro evita que o tokenizador o parta.
    if let Some(g) = super::effects::LinearGradient::parse(&layer) {
        out.gradient = Some(g);
    }
    // `position / size`: parte no `/` de TOPO (o `/` dentro de `url(...)` é
    // caminho, não separador).
    let slash = super::lengths::split_top(&layer, '/');
    let (before, size_part) = match slash.as_slice() {
        [a, b] => (a.clone(), Some(b.clone())),
        _ => (layer.clone(), None),
    };
    if let Some(s) = size_part {
        out.size = BgSize::parse(&s);
    }
    // Tokens do lado esquerdo, com os parênteses preservados.
    let toks = super::lengths::split_top_ws_pub(&before);
    let mut pos_toks: Vec<String> = Vec::new();
    for t in toks {
        let low = t.to_ascii_lowercase();
        if low.starts_with("url(") {
            out.image = Some(t.clone());
            continue;
        }
        if low.starts_with("linear-gradient(") || low.starts_with("repeating-linear-gradient(") {
            continue; // já tratado acima, no valor inteiro
        }
        if let Some(r) = BgRepeat::parse(&low) {
            out.repeat = Some(r);
            continue;
        }
        // `scroll`/`fixed`/`local` (attachment) e as caixas de origem/clip são
        // aceites e DESCARTADAS: nenhuma delas muda o que o motor pinta hoje, e
        // guardá-las seria estado sem leitor.
        if matches!(
            low.as_str(),
            "scroll" | "fixed" | "local" | "border-box" | "padding-box" | "content-box" | "text"
        ) {
            continue;
        }
        if low == "none" {
            out.image = Some("none".into());
            continue;
        }
        if let Some(c) = parse_color(&t) {
            out.color = Some(c);
            continue;
        }
        // O que sobra é candidato a posição (keyword ou comprimento).
        pos_toks.push(t);
    }
    if !pos_toks.is_empty() {
        out.position = BgPosition::parse(&pos_toks.join(" "));
    }
    out
}
