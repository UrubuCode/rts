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
/// **Os três valores já não agem como `both`.** Agiam enquanto o motor
/// guardava UMA lista de floats por container sem distinguir o lado — o corte
/// que este comentário registava. `layout::bfc::BlockFormattingContext`
/// guarda a lista com o `side` de cada float preservado, e [`Clear::sides`] é
/// a pergunta "em quais lados" que faltava para a ler por lado.
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
            // As formas LÓGICAS: em modo horizontal LTR, `inline-start` é a
            // esquerda e `inline-end` a direita — o único modo que este motor
            // suporta (sem `direction:rtl`/bidi), então o mapeamento LTR fixo
            // é correto para todo o corpus atual.
            "inline-start" => Clear::Left,
            "inline-end" => Clear::Right,
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

    /// Em quais lados este valor desce — `(esquerda, direita)`. É a resposta
    /// por lado que `layout::bfc::BlockFormattingContext::fundo_lado` lê para
    /// `clear:left` só descer abaixo dos floats ESQUERDOS, `right` só dos
    /// direitos e `both` dos dois (CSS 2.1 §9.5.2).
    pub fn sides(self) -> (bool, bool) {
        match self {
            Clear::None => (false, false),
            Clear::Left => (true, false),
            Clear::Right => (false, true),
            Clear::Both => (true, true),
        }
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
    /// `auto-phrase` — quebra por análise de frase (CJK). Aceite para não ser
    /// descartada; a quebra deste motor é por espaço, então não muda nada.
    AutoPhrase,
}

impl WordBreak {
    pub fn parse(v: &str) -> Option<WordBreak> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "normal" => WordBreak::Normal,
            "break-all" => WordBreak::BreakAll,
            "keep-all" => WordBreak::KeepAll,
            "break-word" => WordBreak::BreakWord,
            "auto-phrase" => WordBreak::AutoPhrase,
            _ => return None,
        })
    }

    pub fn css(self) -> &'static str {
        match self {
            WordBreak::Normal => "normal",
            WordBreak::BreakAll => "break-all",
            WordBreak::KeepAll => "keep-all",
            WordBreak::BreakWord => "break-word",
            WordBreak::AutoPhrase => "auto-phrase",
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

/// `writing-mode` — a direção do eixo de BLOCO. Herdável.
///
/// CORTE declarado (lote `flex-justify-logico`, retrabalho): só
/// `horizontal-tb` (o default) tem layout de verdade — os quatro valores
/// verticais são aceites e serializados, mas o motor não troca os eixos de
/// bloco/inline (a mesma limitação de `Direction`, que também não faz bidi).
/// O que ISTO desbloqueia é [`is_horizontal`](WritingMode::is_horizontal):
/// um efeito físico que só faz sentido quando o eixo de bloco é vertical
/// (como o espelho de `direction:rtl` no eixo CRUZADO de uma flex-column,
/// `coluna_rtl::cross_x`) pode perguntar por ele ANTES de se aplicar — sem
/// isto, um `.row-wrapper{writing-mode:vertical-rl;direction:rtl}` do WPT
/// `overflow-top-left` espelhava um contentor que o motor já desenha
/// horizontal, divergindo da referência (também tratada como bloco/
/// horizontal) só porque o espelho não perguntou.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WritingMode {
    #[default]
    HorizontalTb,
    VerticalRl,
    VerticalLr,
    SidewaysRl,
    SidewaysLr,
}

impl WritingMode {
    pub fn parse(v: &str) -> Option<WritingMode> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "horizontal-tb" => WritingMode::HorizontalTb,
            "vertical-rl" => WritingMode::VerticalRl,
            "vertical-lr" => WritingMode::VerticalLr,
            "sideways-rl" => WritingMode::SidewaysRl,
            "sideways-lr" => WritingMode::SidewaysLr,
            _ => return None,
        })
    }

    pub fn css(self) -> &'static str {
        match self {
            WritingMode::HorizontalTb => "horizontal-tb",
            WritingMode::VerticalRl => "vertical-rl",
            WritingMode::VerticalLr => "vertical-lr",
            WritingMode::SidewaysRl => "sideways-rl",
            WritingMode::SidewaysLr => "sideways-lr",
        }
    }

    /// `true` só para `horizontal-tb` (o default) — a única forma que o
    /// motor sabe dispor. Ver o corte no comentário do tipo.
    pub fn is_horizontal(self) -> bool {
        matches!(self, WritingMode::HorizontalTb)
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
