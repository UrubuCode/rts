//! Cor, alinhamento e os keywords de TEXTO
//!
//! Extraído de `values.rs` sem alterar uma linha.

use super::*;

/// Cor RGBA empacotada `0xRRGGBBAA` num `u32`. Tipo próprio (não `Color32`).
pub type Rgba = u32;

/// `text-align` — alinhamento horizontal do conteúdo inline dentro do bloco.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
}

impl TextAlign {
    pub fn parse(v: &str) -> Option<TextAlign> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "left" | "start" => TextAlign::Left,
            "right" | "end" => TextAlign::Right,
            "center" => TextAlign::Center,
            "justify" => TextAlign::Justify,
            _ => return None,
        })
    }
}

/// `line-height` — `normal`, um MULTIPLICADOR do font-size (número sem unidade),
/// ou um comprimento absoluto em pontos.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LineHeight {
    /// `normal` — o valor INICIAL do CSS, e **não uma constante**: no browser sai
    /// das métricas da fonte (o Chrome dá ~1,125× na fonte default, não 1,2).
    /// Por isso é uma variante própria em vez de um `Mult` fixo: quem resolve
    /// passa o valor do MEDIDOR, que é o único que fala com a fonte.
    ///
    /// Era `Mult(1.2)`, e isso era um BUG mensurável: um elemento sem declaração
    /// nenhuma usava o default do medidor (1,3 × font no `ApproxMeasurer`) e um
    /// com `line-height: normal` — que a spec diz ser o mesmo valor — usava 1,2.
    /// A mesma propriedade dava duas alturas conforme fosse escrita ou omitida.
    Normal,
    /// número sem unidade (`1.5`) → 1.5 × font-size do elemento.
    Mult(f32),
    /// comprimento absoluto em pontos (`24px`).
    Px(f32),
}

impl LineHeight {
    /// Resolve para a altura da linha em pontos. `font_size` é o do elemento e
    /// `normal` é a altura que o MEDIDOR dá para esse font-size — o valor que a
    /// fonte determina, e que só o backend conhece.
    pub fn resolve(self, font_size: f32, normal: f32) -> f32 {
        match self {
            LineHeight::Normal => normal,
            LineHeight::Mult(m) => m * font_size,
            LineHeight::Px(p) => p,
        }
    }

    pub fn parse(v: &str) -> Option<LineHeight> {
        let v = v.trim();
        if v.eq_ignore_ascii_case("normal") {
            return Some(LineHeight::Normal);
        }
        // Negativo é inválido na spec (a linha não pode ter altura negativa) —
        // recusar deixa a declaração cair, que é o que o browser faz, em vez de
        // encolher a linha para trás.
        let num = |s: &str| s.trim().parse::<f32>().ok().filter(|n| *n >= 0.0);
        // `%` → multiplicador (150% = 1.5×). O % de line-height é do FONT-SIZE do
        // próprio elemento, e não do container — por isso vira multiplicador.
        if let Some(p) = v.strip_suffix('%') {
            return num(p).map(|n| LineHeight::Mult(n / 100.0));
        }
        let low = v.to_ascii_lowercase();
        // `rem` ANTES de `em` (o sufixo curto casaria dentro do longo). `rem` é
        // relativo ao root, que é fixo em 16px aqui — logo, absoluto.
        if let Some(p) = low.strip_suffix("rem") {
            return num(p).map(|n| LineHeight::Px(n * 16.0));
        }
        // `em` é relativo ao font-size do PRÓPRIO elemento, que é o mesmo número
        // que `Mult` dá NESTE elemento. Antes daqui, `1.6em` não era reconhecido
        // de todo e a linha caía no default do medidor.
        //
        // ⚠️ CORTE, e é o caso que quase toda a gente erra: os dois divergem na
        // HERANÇA. `line-height: 1.6` herda o NÚMERO (cada filho multiplica o seu
        // próprio font-size), enquanto `1.6em` herda o COMPRIMENTO já calculado
        // no pai (todos os filhos recebem os mesmos px, mesmo com font-size
        // diferente). Aqui os dois herdam como número, então um filho com
        // font-size menor recebe uma linha menor onde o Chrome lhe daria a do
        // pai. Corrigi-lo exige resolver o `em` para px na CASCADE, onde o
        // font-size do elemento já é conhecido (é onde `font-size` em `em`/`%` já
        // é resolvido, em `dom.rs`) — fica para quem tocar nessa passada.
        if let Some(p) = low.strip_suffix("em") {
            return num(p).map(LineHeight::Mult);
        }
        // `px` → absoluto.
        if let Some(p) = low.strip_suffix("px") {
            return num(p).map(LineHeight::Px);
        }
        // número puro → multiplicador.
        num(&low).map(LineHeight::Mult)
    }
}

/// `white-space` — como espaços e quebras de linha são tratados. ⚠️ PARSEADO e
/// exposto em getComputedStyle, mas o LAYOUT inline atual é linha-única (não quebra
/// texto), então `normal` vs `nowrap` são equivalentes hoje; `pre` preserva o texto
/// cru (o `collect_text` já não colapsa). Efeito pleno chega com inline-flow rico
/// `visibility` — se o elemento é PINTADO. Diferente de `display:none` num
/// ponto que decide layouts inteiros: o elemento continua a ocupar o espaço
/// dele, só não se vê.
///
/// É a forma como uma página real esconde um menu que abre ao clicar (o
/// MediaWiki fá-lo com `visibility:hidden;opacity:0;height:0`), e sem a
/// suportar o menu aparecia aberto por cima do artigo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Visibility {
    Visible,
    Hidden,
}

impl Visibility {
    pub fn parse(v: &str) -> Option<Visibility> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "visible" => Visibility::Visible,
            // `collapse` só difere de `hidden` em tabelas, que este motor ainda
            // não trata como tais — tratá-lo como `hidden` é a aproximação certa.
            "hidden" | "collapse" => Visibility::Hidden,
            _ => return None,
        })
    }
}

/// (corte de fase, documentado em layout.rs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WhiteSpace {
    /// `normal` — colapsa espaços/quebras, quebra linha quando necessário.
    Normal,
    /// `nowrap` — colapsa espaços, NÃO quebra linha.
    Nowrap,
    /// `pre` — preserva espaços e quebras, não quebra automaticamente.
    Pre,
    /// `pre-wrap` — preserva espaços/quebras E quebra automaticamente.
    PreWrap,
    /// `pre-line` — colapsa espaços mas preserva quebras explícitas.
    PreLine,
}

impl WhiteSpace {
    pub fn parse(v: &str) -> Option<WhiteSpace> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "normal" => WhiteSpace::Normal,
            "nowrap" => WhiteSpace::Nowrap,
            "pre" => WhiteSpace::Pre,
            "pre-wrap" => WhiteSpace::PreWrap,
            "pre-line" => WhiteSpace::PreLine,
            _ => return None,
        })
    }
    /// `true` se preserva os espaços/quebras originais (pre/pre-wrap/pre-line p/ quebras).
    pub fn preserves_spaces(self) -> bool {
        matches!(self, WhiteSpace::Pre | WhiteSpace::PreWrap)
    }
}

/// `text-transform` — transformação de caixa do texto.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    /// `capitalize` — primeira letra de cada palavra em maiúscula.
    Capitalize,
}

impl TextTransform {
    pub fn parse(v: &str) -> Option<TextTransform> {
        Some(match v.trim().to_ascii_lowercase().as_str() {
            "none" => TextTransform::None,
            "uppercase" => TextTransform::Uppercase,
            "lowercase" => TextTransform::Lowercase,
            "capitalize" => TextTransform::Capitalize,
            _ => return None,
        })
    }
    /// Aplica a transformação a um texto.
    pub fn apply(self, s: &str) -> String {
        match self {
            TextTransform::None => s.to_string(),
            TextTransform::Uppercase => s.to_uppercase(),
            TextTransform::Lowercase => s.to_lowercase(),
            TextTransform::Capitalize => {
                let mut out = String::with_capacity(s.len());
                let mut at_word_start = true;
                for ch in s.chars() {
                    if ch.is_whitespace() {
                        at_word_start = true;
                        out.push(ch);
                    } else if at_word_start {
                        out.extend(ch.to_uppercase());
                        at_word_start = false;
                    } else {
                        out.push(ch);
                    }
                }
                out
            }
        }
    }
}

/// `text-decoration[-line]` — a linha decorativa do texto. `None` = sem linha.
/// Modela só a presença da linha (a cor herda do texto; estilo/espessura fixos v1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextDecoration {
    /// `none` — sem decoração.
    None,
    /// `underline` — linha sob o texto.
    Underline,
    /// `line-through` — linha cortando o texto.
    LineThrough,
    /// `overline` — linha sobre o texto.
    Overline,
}

impl TextDecoration {
    /// Parseia `text-decoration`/`text-decoration-line`: pega a 1ª keyword de LINHA
    /// (o shorthand pode ter cor/estilo junto — `underline dotted red` → Underline).
    pub fn parse(v: &str) -> Option<TextDecoration> {
        for tok in v.split_whitespace() {
            match tok.to_ascii_lowercase().as_str() {
                "none" => return Some(TextDecoration::None),
                "underline" => return Some(TextDecoration::Underline),
                "line-through" => return Some(TextDecoration::LineThrough),
                "overline" => return Some(TextDecoration::Overline),
                _ => {}
            }
        }
        None
    }
}
