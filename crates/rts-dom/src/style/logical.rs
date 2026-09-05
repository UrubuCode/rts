//! As propriedades LÓGICAS que faltavam: `inset*` e as bordas `-inline-`/`-block-`.
//!
//! `inset-inline-start` e `border-inline-start-color` caíam no contador de
//! ignoradas. Não é uma cauda: numa varredura das folhas reais o WhatsApp Web
//! escreve `border-inline-start-color` 522 vezes e `inset-inline-start` 216 —
//! o CSS moderno gerado por ferramenta já não escreve `left`.
//!
//! `padding-inline-start`/`margin-inline-start` (e os `-end`) JÁ tinham
//! tradução — mas em `style::parse::caixa`, um braço próprio por nome, que
//! respondia PRIMEIRO na cadeia de `aplica_declaracao` e por isso escondia
//! este ficheiro por completo para essas duas famílias: `logical::try_apply`
//! nunca chegava a ser chamado com elas. Duas traduções do MESMO nome —
//! achado pelo WPT `gap-007-rtl` (lote `flex-reverse-order`), quando tornar
//! esta função direction-aware não mudou nada porque o braço de `caixa.rs`,
//! sempre LTR, respondia antes. Removido de lá: as duas famílias reentregam
//! aqui como as outras duas já faziam.
//!
//! ## Uma tradução de NOME, e não um segundo modelo de bordas
//!
//! Todo o trabalho aqui é mapear o eixo lógico no lado físico e reentregar o
//! nome traduzido a quem já sabe aplicá-lo (`style::borders`, os campos `inset_*`).
//! A alternativa — campos lógicos próprios em `ComputedStyle`, resolvidos no
//! layout — foi recusada porque duplicaria o modelo de bordas inteiro (doze
//! campos) por uma propriedade de sentido; o que faltava não era essa
//! duplicação, era resolver no MOMENTO certo (ver "Quando isto resolve",
//! abaixo) — o layout já inverte em RTL em alguns sítios (`coluna_rtl`,
//! `rtl_bloco`), só não em todos (`dom::direction_herdada` documenta o que
//! ainda falta: uma LINHA de flex).
//!
//! ## O corte, dito por extenso: o eixo BLOCO continua horizontal-topo
//!
//! `block-start`/`block-end` continuam fixos em topo/fundo — o motor não faz
//! layout vertical (`writing-mode` aceite e serializado, nunca disposto), e
//! ninguém pediu essa metade. O eixo INLINE (`start`/`end`) já não é: desde o
//! lote `flex-reverse-order` resolve contra `direction` nos "três sítios"
//! que este cabeçalho reservava — `margin-inline-*`, `padding-inline-*` e
//! `border-inline-*` (mais `inset-inline-*`, de graça, pela MESMA função) —
//! `ltr` continua `start`=esquerda, e `rtl` inverte para `start`=direita
//! (CSS Logical Properties §3, `direction` como o eixo herdável já lido por
//! `coluna_rtl`/`rtl_bloco`). O fluxo de TEXTO/inline em si continua LTR —
//! sem bidi — o que muda é só qual LADO físico um `start`/`end` aponta.
//!
//! Achado pelo WPT `gap-007-rtl` (retrabalho do lote `flex-reverse-order`):
//! a ordem de colunas de `flex-direction:column`+`direction:rtl` ficou
//! correta (`coluna_wrap.rs`) e destapou que `margin-inline-end:20px` sob
//! `direction:rtl` continuava a resolver como `margin-right` fixo — o lado
//! ERRADO (devia ser `margin-left`, o lado que fecha o vão ENTRE colunas em
//! RTL) — porque a ordem de colunas trocada por engano escondia a margem no
//! lado errado por coincidência.
//!
//! ## Quando isto resolve: por ELEMENTO, não por REGRA
//!
//! `direction` é herdável e uma regra CSS é compilada UMA vez, partilhada por
//! todo elemento que a casar — casar `section > div{margin-inline-end:20px}`
//! não sabe, nesse momento, qual vai ser o `direction` do elemento que a vai
//! usar (pode nem existir DOM ainda). Resolver aqui, contra o `css.direction`
//! do momento em que a regra é parseada, resolveria sempre contra o INICIAL
//! (`ltr`) — o mesmo bug, só que invisível. Por isso as declarações destas
//! famílias não resolvem aqui: ficam PENDENTES (`style::parse::
//! apply_specified_declaration` reusa a fila que já existia para `var()`,
//! `style::stylesheet::apply_resolved_decl` ganhou um `direction` a mais) e
//! só resolvem por elemento em `dom::cascade`, contra o `direction` já
//! herdado desse elemento (ou já declarado por ele mesmo, na mesma regra —
//! ver o comentário lá) — com UM corte: `dom::direction_herdada` nega esse
//! `direction` quando o pai é uma LINHA de flex, porque a ORDEM dos itens da
//! linha ainda não inverte por `direction` (só por `row-reverse`), e margem
//! certa com ordem errada é pior do que as duas erradas (achado pelo WPT
//! `gap-003-rtl`/`gap-006-rtl`, retrabalho deste lote). `to_physical`
//! continua pura: a pergunta "que lado físico" não muda, só QUANDO se faz.
//!
//! **Lote `flex-writing-mode`**: `to_physical` já não assume o inline
//! sempre-horizontal que o parágrafo acima descrevia — lê `writing_mode`
//! também (`style::text::eixo_x_forward`/`eixo_y_forward`, a MESMA pergunta
//! que `layout::eixos_flex` faz para o flex): em `writing-mode` vertical o
//! eixo inline é Y (`inline-start`/`inline-end` viram `top`/`bottom`) e o de
//! bloco é X (`block-start`/`block-end` viram `left`/`right`, e PODEM
//! inverter — `vertical-rl`/`sideways-rl` sozinhos, sem `direction` nenhum).
//! Por isso `apply_resolved_decl` passou a injectar `writing_mode` herdado
//! no clone, ao lado do `direction` que já injectava — a MESMA fila de
//! pendentes, um segundo motivo (achado pelo WPT `gap-001-lr`/`gap-007-lr`:
//! um `margin-inline-start` sob `vertical-lr` é a margem de CIMA, não a da
//! esquerda).

use super::lengths::{parse_inset, split_top_ws};
use super::props::ComputedStyle;
use super::values::Dimension;
use crate::style::text::{eixo_x_forward, eixo_y_forward};
use crate::style::{Direction, WritingMode};

/// `true` para as propriedades cujo lado físico depende de `direction` —
/// `margin`/`padding`/`border`(-width/-style/-color)/`inset`, eixo INLINE,
/// mais os três nomes antigos do WebKit que se reentregam a elas. É a mesma
/// lista de nomes que `to_physical`/o alias `antigo` reconhecem, aqui à
/// parte para quem PARSEIA decidir se adia a resolução (ver o cabeçalho).
/// `inline-size`/`min-inline-size`/etc. não entram: são dimensões, não têm
/// lado.
pub(crate) fn e_direction_dependente(prop: &str) -> bool {
    prop.contains("inline-start")
        || prop.contains("inline-end")
        || matches!(
            prop,
            "margin-start" | "margin-end" | "padding-start" | "padding-end" | "border-start" | "border-end"
        )
}

/// `true` para `margin`/`padding`/`border`(-width/-style/-color)/`inset`
/// no eixo de BLOCO (`block-start`/`block-end`) — o lado físico não
/// depende de `direction` nenhum (o eixo de bloco nunca lê `direction`,
/// `to_physical` já dizia isso), mas depende de QUAL eixo físico é o de
/// bloco, e isso é `writing-mode`. Ficaram de fora de
/// [`e_direction_dependente`] porque nunca precisaram de adiar ATÉ este
/// lote — em `horizontal-tb` o bloco é sempre Y, fixo, sem pergunta
/// nenhuma para fazer; passou a ter uma só quando o bloco pode ser X
/// (achado pelo WPT `gap-002-lr`: uma `section` com `flex-direction:
/// column` simula o `gap` principal com `margin-block-start` numa
/// referência, e essa margem resolvia sempre como `margin-top` — physico
/// ERRADO sob `vertical-lr`, onde o eixo de bloco é X).
pub(crate) fn e_bloco_writing_mode_dependente(prop: &str) -> bool {
    prop.contains("block-start") || prop.contains("block-end")
}

/// `true` para as DIMENSÕES lógicas (`inline-size`/`block-size` e os
/// `min-`/`max-` das duas) — o lado FÍSICO de `-start`/`-end` depende de
/// `direction`, mas qual EIXO físico (largura ou altura) `inline-size`/
/// `block-size` apontam depende de `writing-mode`, herdado do mesmo jeito
/// (`dom::cascade`, "o writing-mode herdado" ao lado do `direction`
/// herdado) — a razão de precisar de uma lista à parte de
/// `e_direction_dependente` em vez de reusar a mesma pendência.
pub(crate) fn e_writing_mode_dependente(prop: &str) -> bool {
    matches!(
        prop,
        "inline-size" | "block-size" | "min-inline-size" | "min-block-size" | "max-inline-size" | "max-block-size"
    )
}

/// Traduz o eixo lógico de um nome de propriedade para o lado físico.
/// `"inset-inline-start"` → `"inset-left"` (`rtl=false`) ou `"inset-right"`
/// (`rtl=true`); `"border-block-end-width"` → sempre `"border-bottom-width"`
/// (o eixo bloco não lê `direction`). `None` quando o nome não tem eixo
/// lógico nenhum.
fn to_physical(prop: &str, wm: WritingMode, dir: Direction) -> Option<String> {
    // Horizontal: inline=X (`eixo_x_forward` decide left/right), bloco=Y
    // fixo (top/bottom, nunca invertido — o motor não roda o fluxo normal).
    // Vertical: TROCADOS — inline=Y (`eixo_y_forward`), bloco=X
    // (`eixo_x_forward`, que já ignora `direction` nesse caso).
    let (inline_start, inline_end) = if wm.is_horizontal() {
        if eixo_x_forward(wm, dir) { ("left", "right") } else { ("right", "left") }
    } else if eixo_y_forward(wm, dir) {
        ("top", "bottom")
    } else {
        ("bottom", "top")
    };
    let (block_start, block_end) = if wm.is_horizontal() {
        ("top", "bottom")
    } else if eixo_x_forward(wm, dir) {
        ("left", "right")
    } else {
        ("right", "left")
    };
    for (logico, fisico) in [
        ("inline-start", inline_start),
        ("inline-end", inline_end),
        ("block-start", block_start),
        ("block-end", block_end),
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

    // Em `writing-mode` vertical os dois eixos TROCAM: `inline-size` é a
    // altura (o eixo inline correu para Y) e `block-size` a largura — a
    // MESMA pergunta que `layout::eixos_flex` faz para o flex, aqui para a
    // dimensão declarada (achado pelo WPT `gap-001-lr`/`gap-002-lr`: um
    // `block-size` no contentor flex de um `writing-mode:vertical-lr`
    // media a LARGURA física no Chrome, não a altura).
    let vertical = !css.writing_mode.unwrap_or_default().is_horizontal();
    let dimensao = match prop {
        "inline-size" => Some(if vertical { "height" } else { "width" }),
        "block-size" => Some(if vertical { "width" } else { "height" }),
        "min-inline-size" => Some(if vertical { "min-height" } else { "min-width" }),
        "min-block-size" => Some(if vertical { "min-width" } else { "min-height" }),
        "max-inline-size" => Some(if vertical { "max-height" } else { "max-width" }),
        "max-block-size" => Some(if vertical { "max-width" } else { "max-height" }),
        _ => None,
    };
    if let Some(fisico) = dimensao {
        return super::parse::aplica_declaracao(css, fisico, val);
    }

    let wm = css.writing_mode.unwrap_or_default();
    let dir = css.direction.unwrap_or_default();
    let Some(fisico) = to_physical(prop, wm, dir) else {
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
