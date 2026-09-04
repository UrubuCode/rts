//! Serialização de propriedades DE VÁRIAS CAMADAS: `text-shadow: <s1>, <s2>` e
//! `background-image: <img1>, <img2>` (mais as três longhands de fundo que
//! seguem o número de camadas da imagem — `background-repeat`/`-position`/
//! `-size`).
//!
//! ## Porque um módulo próprio, e porque separado de `style::painting`
//!
//! `style::painting` já guarda a PRIMEIRA sombra (`text_shadow: BoxShadow`) e
//! já está em 481 linhas — perto do teto de 500. As três propriedades de fundo
//! vivem em `style::background`/`style::fmt::caixa_fluxo`, também sem folga.
//! Nenhum dos dois ganha uma segunda responsabilidade; esta entra ao lado, lendo
//! o valor CRU que cada um já guarda (`text_shadow_raw`/`bg_image_layers`/…) em
//! vez de reabrir o parser deles.
//!
//! ## O corte
//!
//! `fmt_bg_image_layer` normaliza a COR dentro de um `linear-gradient(...)`
//! (é o que os testes medidos exigem) e preserva o resto do token — um `deg`
//! ou uma posição de parada (`red 20%`) sai tal como a folha escreveu, e não
//! como o Blink os re-serializaria. Nenhuma fixture medida escreve ângulo ou
//! posição de parada; o dia que uma escrever, esta é a função a estender.

use super::color::parse_color;
use super::fmt_values::{fmt_color, fmt_dim, fmt_url};
use super::lengths::split_top;

/// `text-shadow: <s1>, <s2>, …` no formato computado — cada sombra com a cor à
/// FRENTE (como `style::painting::fmt_shadow` já fazia para uma só), juntas por
/// `, `. `None` = nada parseável (inclui `none`/vazio), e o chamador cai no
/// `BoxShadow` único que já tinha.
pub fn fmt_text_shadow_list(declared: &str) -> Option<String> {
    let v = declared.trim();
    if v.is_empty() || v.eq_ignore_ascii_case("none") {
        return None;
    }
    let layers = split_top(v, ',');
    let mut out = Vec::with_capacity(layers.len());
    for layer in &layers {
        let mut s = super::effects::BoxShadow::parse(layer)?;
        s.spread = 0.0; // `text-shadow` não tem spread — mesmo corte do único.
        out.push(super::painting::fmt_shadow(s));
    }
    if out.is_empty() {
        None
    } else {
        Some(out.join(", "))
    }
}

/// `background-image: <img1>, <img2>, …` computado — uma `url(...)` leva aspas
/// (`fmt_url`), um `linear-gradient(...)` tem as cores normalizadas para
/// `rgb()`/`rgba()`. Chamada só quando há mais de uma camada; uma só continua a
/// sair dos campos tipados (`gradient`/`bg_image`), que já a serializam certo.
pub fn fmt_bg_image_layers(declared: &str) -> String {
    split_top(declared, ',')
        .iter()
        .map(|layer| fmt_bg_image_layer(layer.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn fmt_bg_image_layer(layer: &str) -> String {
    let low = layer.to_ascii_lowercase();
    if low.starts_with("url(") {
        return fmt_url(layer);
    }
    let prefixo = ["linear-gradient(", "repeating-linear-gradient("]
        .into_iter()
        .find(|p| low.starts_with(p));
    if let Some(p) = prefixo {
        // ASCII: `low`/`layer` têm o mesmo comprimento de byte no prefixo, só
        // a caixa pode divergir — fatiar `layer` pelos bytes de `p` é seguro.
        let nome = &layer[..p.len() - 1];
        let resto = &layer[p.len() - 1..];
        if let Some(inner) = resto.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            let partes: Vec<String> = split_top(inner, ',')
                .iter()
                .map(|part| {
                    let pt = part.trim();
                    parse_color(pt).map(fmt_color).unwrap_or_else(|| pt.to_string())
                })
                .collect();
            return format!("{nome}({})", partes.join(", "));
        }
    }
    layer.to_string()
}

/// O número de camadas de `background-image` deste elemento — a IMAGEM é quem
/// decide quantas o Blink reporta para `-repeat`/`-position`/`-size`, mesmo
/// quando essas três não foram declaradas (cai no inicial repetido N vezes,
/// não numa única cópia). `1` sem imagem ou com uma só.
fn contagem_camadas(bg_image_layers: Option<&str>) -> usize {
    bg_image_layers
        .map(|s| split_top(s, ',').len().max(1))
        .unwrap_or(1)
}

/// `background-repeat` computado seguindo o nº de camadas da imagem. `unico` é
/// o que `background-repeat` reportaria hoje (uma só camada, campo tipado).
pub fn fmt_bg_repeat(
    bg_image_layers: Option<&str>,
    declarado: Option<&str>,
    unico: String,
) -> String {
    let n = contagem_camadas(bg_image_layers);
    if n <= 1 {
        return unico;
    }
    match declarado {
        Some(raw) => split_top(raw, ',')
            .iter()
            .map(|t| {
                super::BgRepeat::parse(t)
                    .map(|r| r.css().to_string())
                    .unwrap_or_else(|| t.trim().to_string())
            })
            .collect::<Vec<_>>()
            .join(", "),
        None => vec!["repeat"; n].join(", "),
    }
}

/// `background-position` computado seguindo o nº de camadas da imagem. Mesmo
/// esquema de `fmt_bg_repeat`.
pub fn fmt_bg_position(
    bg_image_layers: Option<&str>,
    declarado: Option<&str>,
    unico: String,
) -> String {
    let n = contagem_camadas(bg_image_layers);
    if n <= 1 {
        return unico;
    }
    match declarado {
        Some(raw) => split_top(raw, ',')
            .iter()
            .map(|t| {
                super::BgPosition::parse(t)
                    .map(|p| format!("{} {}", fmt_dim(p.x), fmt_dim(p.y)))
                    .unwrap_or_else(|| t.trim().to_string())
            })
            .collect::<Vec<_>>()
            .join(", "),
        None => vec!["0% 0%"; n].join(", "),
    }
}

/// `background-size` computado seguindo o nº de camadas da imagem. Mesmo
/// esquema dos dois acima.
pub fn fmt_bg_size(
    bg_image_layers: Option<&str>,
    declarado: Option<&str>,
    unico: String,
) -> String {
    let n = contagem_camadas(bg_image_layers);
    if n <= 1 {
        return unico;
    }
    match declarado {
        Some(raw) => split_top(raw, ',')
            .iter()
            .map(|t| match super::BgSize::parse(t) {
                Some(super::BgSize::Auto) | None => t.trim().to_string(),
                Some(super::BgSize::Cover) => "cover".to_string(),
                Some(super::BgSize::Contain) => "contain".to_string(),
                Some(super::BgSize::Len(w, h)) => format!("{} {}", fmt_dim(w), fmt_dim(h)),
            })
            .collect::<Vec<_>>()
            .join(", "),
        None => vec!["auto"; n].join(", "),
    }
}
