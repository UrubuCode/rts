//! As propriedades LÓGICAS que faltavam: `inset*` e as bordas `-inline-`/`-block-`.
//!
//! O `parse` já traduzia `padding-inline-start` e `margin-inline-start` para o
//! lado físico; `inset-inline-start` e `border-inline-start-color` caíam no
//! contador de ignoradas. Não é uma cauda: numa varredura das folhas reais o
//! WhatsApp Web escreve `border-inline-start-color` 522 vezes e
//! `inset-inline-start` 216 — o CSS moderno gerado por ferramenta já não escreve
//! `left`.
//!
//! ## Uma tradução de NOME, e não um segundo modelo de bordas
//!
//! Todo o trabalho aqui é mapear o eixo lógico no lado físico e reentregar o
//! nome traduzido a quem já sabe aplicá-lo (`style::borders`, os campos `inset_*`).
//! A alternativa — campos lógicos próprios em `ComputedStyle`, resolvidos no
//! layout — daria `direction: rtl` de graça, e foi recusada porque duplicaria o
//! modelo de bordas inteiro (doze campos) por uma propriedade de sentido, e
//! porque o layout não inverte em RTL de qualquer maneira.
//!
//! ## O corte, dito por extenso: assume-se LTR horizontal
//!
//! `start` = esquerda/topo, `end` = direita/fundo. É o MESMO corte que
//! `padding-inline-start` e `margin-inline-start` já faziam — mantê-lo é ter uma
//! resposta só para a pergunta. Numa página `direction: rtl` os lados saem
//! trocados, e é isso que o dia do RTL vai ter de resolver nos três sítios ao
//! mesmo tempo (`style::text` guarda `direction`; o layout ainda não o lê).

use super::lengths::{parse_inset, split_top_ws};
use super::props::ComputedStyle;
use super::values::Dimension;

/// Traduz o eixo lógico de um nome de propriedade para o lado físico, em LTR.
/// `"inset-inline-start"` → `"inset-left"`, `"border-block-end-width"` →
/// `"border-bottom-width"`. `None` quando o nome não tem eixo lógico nenhum.
fn to_physical(prop: &str) -> Option<String> {
    for (logico, fisico) in [
        ("inline-start", "left"),
        ("inline-end", "right"),
        ("block-start", "top"),
        ("block-end", "bottom"),
    ] {
        if let Some(i) = prop.find(logico) {
            let mut out = String::with_capacity(prop.len());
            out.push_str(&prop[..i]);
            out.push_str(fisico);
            out.push_str(&prop[i + logico.len()..]);
            return Some(out);
        }
    }
    None
}

/// Escreve um dos quatro offsets pelo NOME físico do lado.
fn set_inset(css: &mut ComputedStyle, side: &str, v: Option<Dimension>) {
    match side {
        "top" => css.inset_top = v,
        "right" => css.inset_right = v,
        "bottom" => css.inset_bottom = v,
        "left" => css.inset_left = v,
        _ => {}
    }
}

/// Tenta aplicar `prop` como propriedade lógica (ou como o shorthand `inset`).
/// Devolve `false` se o nome não é uma delas — o `parse` usa isso para decidir se
/// conta a declaração como ignorada.
pub fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    // `inset: <1 a 4 valores>` — a mesma ordem de qualquer shorthand de caixa
    // (top right bottom left, com os omitidos a copiar o lado oposto). Não reusa
    // `parse_edges` porque os offsets aceitam NEGATIVO e `auto`, que a caixa de
    // margem/padding trata de outra maneira.
    if prop == "inset" {
        let toks = split_top_ws(val);
        let g = |i: usize| parse_inset(&toks[i]);
        let (t, r, b, l) = match toks.len() {
            1 => (g(0), g(0), g(0), g(0)),
            2 => (g(0), g(1), g(0), g(1)),
            3 => (g(0), g(1), g(2), g(1)),
            4 => (g(0), g(1), g(2), g(3)),
            _ => return true, // valor malformado: reconhecido, sem efeito
        };
        css.inset_top = t;
        css.inset_right = r;
        css.inset_bottom = b;
        css.inset_left = l;
        return true;
    }
    // `inset-inline` / `inset-block` — os dois lados de um eixo de uma vez.
    if let Some(eixo) = prop.strip_prefix("inset-") {
        if eixo == "inline" || eixo == "block" {
            let toks = split_top_ws(val);
            if toks.is_empty() {
                return true;
            }
            let a = parse_inset(&toks[0]);
            let b = if toks.len() > 1 {
                parse_inset(&toks[1])
            } else {
                a
            };
            if eixo == "inline" {
                css.inset_left = a;
                css.inset_right = b;
            } else {
                css.inset_top = a;
                css.inset_bottom = b;
            }
            return true;
        }
    }

    // As DIMENSÕES lógicas: `inline-size` é a largura e `block-size` a altura,
    // em escrita horizontal — o mesmo corte LTR-horizontal que o resto do módulo
    // assume e que o cabeçalho diz por extenso.
    //
    // Reentrega ao `parse` com o nome FÍSICO em vez de escrever o campo aqui: a
    // largura tem keywords, percentagens e `calc()` que aquele braço já sabe
    // ler, e uma segunda leitura divergia dele à primeira correção.
    // Os nomes ANTIGOS do WebKit para a caixa lógica: `-webkit-margin-end` é o
    // que hoje se chama `margin-inline-end`. Chegam aqui já sem o prefixo (o
    // `parse` corta-o na última tentativa), e sem esta linha `margin-end` não
    // tem eixo lógico nenhum para traduzir e cai como desconhecida.
    let antigo = match prop {
        "margin-start" => Some("margin-inline-start"),
        "margin-end" => Some("margin-inline-end"),
        "padding-start" => Some("padding-inline-start"),
        "padding-end" => Some("padding-inline-end"),
        "border-start" => Some("border-inline-start"),
        "border-end" => Some("border-inline-end"),
        _ => None,
    };
    if let Some(moderno) = antigo {
        return try_apply(css, moderno, val);
    }

    let dimensao = match prop {
        "inline-size" => Some("width"),
        "block-size" => Some("height"),
        "min-inline-size" => Some("min-width"),
        "min-block-size" => Some("min-height"),
        "max-inline-size" => Some("max-width"),
        "max-block-size" => Some("max-height"),
        _ => None,
    };
    if let Some(fisico) = dimensao {
        return super::parse::aplica_declaracao(css, fisico, val);
    }

    let Some(fisico) = to_physical(prop) else {
        return false;
    };

    // O resto da CAIXA lógica, pela mesma reentrega. O `parse` tinha
    // `padding-inline-start/end` e `margin-block-start/end` por literal mas não
    // as outras metades das mesmas famílias: `padding-block-end` caía como
    // desconhecida ao lado de uma `margin-block-end` que funcionava. Traduzir o
    // eixo e reentregar fecha as quatro famílias sem um braço por nome — e sem
    // a assimetria poder voltar.
    if fisico.starts_with("padding-") || fisico.starts_with("margin-") {
        return super::parse::aplica_declaracao(css, &fisico, val);
    }

    // `inset-inline-start` → o offset do lado físico.
    if let Some(side) = fisico.strip_prefix("inset-") {
        set_inset(css, side, parse_inset(val));
        return true;
    }
    // As bordas lógicas: o nome traduzido é EXATAMENTE o que `style::borders` já
    // reconhece, longhand (`border-left-color`) ou shorthand de lado
    // (`border-left`). Reentregar em vez de reimplementar é o ponto do módulo.
    if let Some(resto) = fisico.strip_prefix("border-") {
        if super::borders::is_longhand(&fisico) {
            super::borders::apply_longhand(css, &fisico, val);
            return true;
        }
        if let Some(side) = super::SideName::parse(resto) {
            super::borders::apply_side_shorthand(css, side, val);
            return true;
        }
    }
    false
}
