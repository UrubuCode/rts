//! Os KEYWORDS do lote: os enums, o `clip`, e a macro `kw!` que lhes dá parse e serialização
//!
//! Extraído de `vocab.rs` sem alterar uma linha.

use super::*;

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

/// `clip: rect(<t>, <r>, <b>, <l>) | auto` — o retângulo de recorte de uma caixa
/// posicionada. Cada lado é um comprimento ou `auto`, e é medido a partir do
/// canto SUPERIOR ESQUERDO da caixa (não é um `inset`: `bottom` cresce para
/// baixo, ao contrário de `inset-bottom`).
///
/// GUARDADA, SEM RECORTE — e a pergunta que decide isso foi verificada em vez de
/// assumida. `clip` está obsoleta na spec há anos e mesmo assim aparece em 8 das
/// 13 folhas do corpus, sempre com o mesmo papel: o `.sr-only`/`.visually-hidden`
/// que esconde texto de quem vê e o deixa para o leitor de ecrã. **Se o recorte
/// faltasse, esse texto aparecia na página** — um defeito visível é pior que uma
/// propriedade ausente, que é a razão para não a reconhecer às cegas.
///
/// Não é o caso, e isto é a evidência: em TODAS as ocorrências do corpus o
/// `clip: rect(…)` vem ao lado de `width:1px; height:1px; overflow:hidden`
/// (Bootstrap, Tailwind, Bulma, Foundation, Primer, MediaWiki, WhatsApp). A
/// caixa de 1px com `overflow:hidden` já esconde o conteúdo sozinha; o `clip` é
/// cinto-e-suspensórios do tempo em que `overflow` não bastava. Guardar sem
/// recortar não torna nada visível.
///
/// A alternativa — recortar a sério — pedia um retângulo de corte na lista de
/// display, que hoje não existe (o `overflow` é resolvido pelo container de
/// rolagem, não por um clip por item). Seria um segundo mecanismo de recorte ao
/// lado do primeiro, para uma propriedade que a spec já substituiu por
/// `clip-path`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Clip {
    Auto,
    /// Os quatro lados, na ordem da spec. `None` num lado = `auto` naquele lado.
    Rect {
        top: Option<Dimension>,
        right: Option<Dimension>,
        bottom: Option<Dimension>,
        left: Option<Dimension>,
    },
}

impl Clip {
    /// `auto` ou `rect(...)`. As DUAS sintaxes de `rect()` são aceites porque as
    /// duas estão no corpus: com vírgulas (`rect(0, 0, 0, 0)` — Bootstrap,
    /// Tailwind, Foundation) e sem (`rect(0 0 0 0)` — MediaWiki, WhatsApp). A
    /// primeira é a de CSS2 e a segunda a que os browsers também aceitam; tratar
    /// só uma delas deixava metade do corpus por reconhecer.
    pub fn parse(v: &str) -> Option<Clip> {
        let low = v.trim().to_ascii_lowercase();
        if low == "auto" {
            return Some(Clip::Auto);
        }
        let dentro = low.strip_prefix("rect(")?.strip_suffix(')')?;
        // Vírgula OU espaço como separador — `split_top_ws` não serve porque não
        // conhece a vírgula, e trocar uma pela outra antes de partir é mais
        // barato que um segundo separador no partidor comum.
        let toks = split_top_ws(&dentro.replace(',', " "));
        if toks.len() != 4 {
            return None;
        }
        // `auto` por lado vem como `None`; um comprimento negativo é legal aqui
        // (o retângulo pode começar acima da caixa), e é por isso que o parser é
        // o `parse_inset` e não o `parse_dimension`, que rejeita negativos.
        let lado = |i: usize| -> Option<Dimension> {
            if toks[i] == "auto" {
                None
            } else {
                super::lengths::parse_inset(&toks[i])
            }
        };
        Some(Clip::Rect {
            top: lado(0),
            right: lado(1),
            bottom: lado(2),
            left: lado(3),
        })
    }

    /// O que `getComputedStyle` responde. O Chrome imprime sempre a forma COM
    /// vírgulas e com a unidade explícita, mesmo quando o autor escreveu `0`.
    pub fn css(self) -> String {
        let Clip::Rect {
            top,
            right,
            bottom,
            left,
        } = self
        else {
            return "auto".to_string();
        };
        let d = |v: Option<Dimension>| {
            v.map(super::fmt_values::fmt_dim)
                .unwrap_or_else(|| "auto".to_string())
        };
        format!("rect({}, {}, {}, {})", d(top), d(right), d(bottom), d(left))
    }
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
