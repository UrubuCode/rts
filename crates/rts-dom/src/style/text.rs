//! Keywords de TEXTO, LISTA e FLUXO que faltavam ao vocabulário do motor:
//! `vertical-align`, `clear`, `word-break`, `overflow-wrap`, `direction` e
//! `list-style-type`.
//!
//! Estão juntas por serem todas o mesmo formato — um enum de keywords com
//! `parse`/`css` — e num módulo NOVO porque `values.rs` já tem 777 linhas, bem
//! acima do teto de 500 do repositório: acrescentar seis enums lá era engordar o
//! ficheiro que a regra manda dividir.
//!
//! Cada tipo diz, no seu doc, o que o motor FAZ com ele hoje. A distinção
//! importa: `clear` e `vertical-align` mudam o layout; `word-break`,
//! `overflow-wrap`, `direction` e `list-style-type` são, por agora, aceites e
//! serializados — o `getComputedStyle` da página responde certo e o efeito chega
//! quando o consumidor existir. O que NÃO se faz é fingir: nenhum deles é mapeado
//! para um comportamento aproximado que a página não pediu.

/// `vertical-align` — alinhamento vertical de uma caixa inline-level dentro da
/// linha (<https://developer.mozilla.org/en-US/docs/Web/CSS/vertical-align>).
///
/// CONSUMIDO no alinhamento da corrida de inline-blocks (`layout.rs`): `top`,
/// `middle` e `bottom` posicionam a caixa dentro da altura da linha.
///
/// CORTE declarado: `baseline` — o valor INICIAL do CSS — é tratado como `top`,
/// que é o que o motor sempre fez. Alinhar por baseline exigia guardar a baseline
/// da última linha de cada inline-block, que o layout não calcula; aproximá-la
/// pelo fundo da caixa mudaria a posição de todo o texto de toda a página por uma
/// propriedade que a maioria dos elementos nem declara. `sub`/`super`/`text-top`/
/// `text-bottom` são aceites e serializados, e alinham como `baseline`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Top,
    Middle,
    Bottom,
    Sub,
    Super,
    TextTop,
    TextBottom,
}

impl VerticalAlign {
    pub fn parse(v: &str) -> Option<VerticalAlign> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "baseline" => VerticalAlign::Baseline,
            "top" => VerticalAlign::Top,
            "middle" => VerticalAlign::Middle,
            "bottom" => VerticalAlign::Bottom,
            "sub" => VerticalAlign::Sub,
            "super" => VerticalAlign::Super,
            "text-top" => VerticalAlign::TextTop,
            "text-bottom" => VerticalAlign::TextBottom,
            // A forma de COMPRIMENTO (`vertical-align: 2px`) desloca a caixa da
            // baseline — sem baseline modelada não há de onde deslocar, então é
            // recusada em vez de ser aproximada.
            _ => return None,
        })
    }

    pub fn css(self) -> &'static str {
        match self {
            VerticalAlign::Baseline => "baseline",
            VerticalAlign::Top => "top",
            VerticalAlign::Middle => "middle",
            VerticalAlign::Bottom => "bottom",
            VerticalAlign::Sub => "sub",
            VerticalAlign::Super => "super",
            VerticalAlign::TextTop => "text-top",
            VerticalAlign::TextBottom => "text-bottom",
        }
    }
}

/// `clear` — o par do `float`: obriga a caixa a começar ABAIXO dos floats
/// anteriores (<https://developer.mozilla.org/en-US/docs/Web/CSS/clear>).
///
/// CORTE declarado: os três valores agem como `both`. O motor guarda UMA linha de
/// floats por container (um topo e uma altura, ver `layout_children_vertical`),
/// não uma lista por lado — logo não há como descer abaixo só dos floats da
/// esquerda. Distinguir os lados exige o modelo de float completo, e o efeito de
/// `clear: left` numa página que só flutua para um lado é idêntico ao de `both`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Clear {
    #[default]
    None,
    Left,
    Right,
    Both,
}

impl Clear {
    pub fn parse(v: &str) -> Option<Clear> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "none" => Clear::None,
            "left" => Clear::Left,
            "right" => Clear::Right,
            "both" => Clear::Both,
            _ => return None,
        })
    }

    pub fn css(self) -> &'static str {
        match self {
            Clear::None => "none",
            Clear::Left => "left",
            Clear::Right => "right",
            Clear::Both => "both",
        }
    }

    /// `true` se este valor desce abaixo dos floats correntes.
    pub fn clears(self) -> bool {
        self != Clear::None
    }
}

/// `word-break` — onde o motor pode quebrar DENTRO de uma palavra. Herdável.
/// Aceite e serializado; a quebra do motor é sempre por espaço (`white-space`
/// decide se quebra), então `break-all` ainda não parte uma palavra longa.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WordBreak {
    #[default]
    Normal,
    BreakAll,
    KeepAll,
    /// Legado, equivalente a `overflow-wrap: break-word` (MDN).
    BreakWord,
}

impl WordBreak {
    pub fn parse(v: &str) -> Option<WordBreak> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "normal" => WordBreak::Normal,
            "break-all" => WordBreak::BreakAll,
            "keep-all" => WordBreak::KeepAll,
            "break-word" => WordBreak::BreakWord,
            _ => return None,
        })
    }

    pub fn css(self) -> &'static str {
        match self {
            WordBreak::Normal => "normal",
            WordBreak::BreakAll => "break-all",
            WordBreak::KeepAll => "keep-all",
            WordBreak::BreakWord => "break-word",
        }
    }
}

/// `overflow-wrap` (o antigo `word-wrap`) — se uma palavra que não cabe na linha
/// pode ser partida. Herdável. Aceite e serializado, pela mesma razão do
/// `word-break`: a quebra por palavra do motor ainda não parte um token.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OverflowWrap {
    #[default]
    Normal,
    BreakWord,
    Anywhere,
}

impl OverflowWrap {
    pub fn parse(v: &str) -> Option<OverflowWrap> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "normal" => OverflowWrap::Normal,
            "break-word" => OverflowWrap::BreakWord,
            "anywhere" => OverflowWrap::Anywhere,
            _ => return None,
        })
    }

    pub fn css(self) -> &'static str {
        match self {
            OverflowWrap::Normal => "normal",
            OverflowWrap::BreakWord => "break-word",
            OverflowWrap::Anywhere => "anywhere",
        }
    }
}

/// `direction` — a direção do fluxo inline. Herdável.
///
/// CORTE declarado: `rtl` é aceite e serializado, mas o layout continua a dispor
/// da esquerda para a direita. Um `rtl` verdadeiro inverte a ordem inline, a
/// resolução de `start`/`end` e o alinhamento default de todo o subárvore — é um
/// modo do motor, não uma propriedade. Guardá-la é o primeiro passo, e é honesto
/// porque a página que a lê pelo `getComputedStyle` recebe o que declarou.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn parse(v: &str) -> Option<Direction> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "ltr" => Direction::Ltr,
            "rtl" => Direction::Rtl,
            _ => return None,
        })
    }

    pub fn css(self) -> &'static str {
        match self {
            Direction::Ltr => "ltr",
            Direction::Rtl => "rtl",
        }
    }
}

/// `list-style-type` — o marcador de um item de lista. Herdável.
///
/// Aceite e serializado. O motor NÃO desenha marcador nenhum hoje (nem o `disc`
/// default do `<ul>`), então `decimal` e `disc` mudariam o mesmo nada: o valor é
/// guardado para quando o marcador existir, e para o `getComputedStyle`
/// responder. `none` é o valor que a folha real mais declara — e é exatamente o
/// que o motor já mostra.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ListStyleType {
    #[default]
    Disc,
    Circle,
    Square,
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
    None,
}

impl ListStyleType {
    pub fn parse(v: &str) -> Option<ListStyleType> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "disc" => ListStyleType::Disc,
            "circle" => ListStyleType::Circle,
            "square" => ListStyleType::Square,
            "decimal" => ListStyleType::Decimal,
            "lower-alpha" | "lower-latin" => ListStyleType::LowerAlpha,
            "upper-alpha" | "upper-latin" => ListStyleType::UpperAlpha,
            "lower-roman" => ListStyleType::LowerRoman,
            "upper-roman" => ListStyleType::UpperRoman,
            "none" => ListStyleType::None,
            _ => return None,
        })
    }

    pub fn css(self) -> &'static str {
        match self {
            ListStyleType::Disc => "disc",
            ListStyleType::Circle => "circle",
            ListStyleType::Square => "square",
            ListStyleType::Decimal => "decimal",
            ListStyleType::LowerAlpha => "lower-alpha",
            ListStyleType::UpperAlpha => "upper-alpha",
            ListStyleType::LowerRoman => "lower-roman",
            ListStyleType::UpperRoman => "upper-roman",
            ListStyleType::None => "none",
        }
    }
}
