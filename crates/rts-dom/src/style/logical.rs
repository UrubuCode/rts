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
//! sempre LTR, respondia antes. Removido de lá — as quatro famílias
//! (`margin`/`padding`/`border`-inline-\*, e `margin-block-*` desde o lote
//! `flex-writing-mode`) reentregam aqui.
//!
//! ## Uma tradução de NOME, e não um segundo modelo de bordas
//!
//! Todo o trabalho aqui é mapear o eixo lógico no lado físico e reentregar o
//! nome traduzido a quem já sabe aplicá-lo (`style::borders`, os campos `inset_*`).
//! A alternativa — campos lógicos próprios em `ComputedStyle`, resolvidos no
//! layout — foi recusada porque duplicaria o modelo de bordas inteiro (doze
//! campos) por uma propriedade de sentido; o que faltava não era essa
//! duplicação, era resolver no MOMENTO certo (ver "Quando isto resolve",
//! abaixo).
//!
//! ## Os dois eixos lógicos seguem `direction` E `writing-mode` de verdade
//!
//! `inline-start`/`inline-end`/`block-start`/`block-end` (e `inline-size`/
//! `block-size`) já não são sinónimos fixos de esquerda/topo. Em DUAS ondas:
//! `flex-reverse-order` tornou o eixo INLINE `direction`-aware (`ltr`
//! `start`=esquerda, `rtl` `start`=direita — CSS Logical Properties §3), com
//! o eixo de BLOCO ainda fixo topo/fundo ("o motor não faz layout vertical").
//! `flex-writing-mode` generalizou os DOIS: `to_physical` pergunta a
//! `style::text::eixo_x_forward`/`eixo_y_forward` (a MESMA resposta que
//! `layout::eixos_flex` usa para trocar o eixo do FLEX) qual eixo físico
//! (X ou Y) é o inline e qual é o de bloco, e em que sentido cada um corre —
//! em escrita vertical o INLINE é que fica fixo a top/bottom e o de BLOCO
//! passa a left/right. Sem isto, `margin-inline-start` de um
//! `gap-*-{rtl,lr,rl}` do WPT continuava a virar `margin-left`/`margin-top`
//! sempre, e a referência (que usa a propriedade lógica para simular o
//! `gap`) divergia do motor assim que este deixou de fingir que RTL/vertical
//! não existem.
//!
//! Achado pelo WPT `gap-007-rtl` (retrabalho de `flex-reverse-order`): a
//! ordem de colunas de `flex-direction:column`+`direction:rtl` ficou correta
//! (`coluna_wrap.rs`) e destapou que `margin-inline-end:20px` sob
//! `direction:rtl` continuava a resolver como `margin-right` fixo — o lado
//! ERRADO — porque a ordem de colunas trocada por engano escondia a margem
//! no lado errado por coincidência.
//!
//! ## Quando isto resolve: por ELEMENTO, não por REGRA
//!
//! `direction`/`writing-mode` são herdáveis e uma regra CSS é compilada UMA
//! vez, partilhada por todo elemento que a casar — casar
//! `section > div{margin-inline-end:20px}` não sabe, nesse momento, qual vai
//! ser o `direction`/`writing-mode` do elemento que a vai usar (pode nem
//! existir DOM ainda). Resolver aqui, contra o `css` do momento em que a
//! regra é parseada, resolveria sempre contra o INICIAL (`ltr`,
//! `horizontal-tb`) — o mesmo bug, só que invisível. Por isso as declarações
//! de [`e_eixo_dependente`] não resolvem aqui: ficam PENDENTES
//! (`style::parse::apply_specified_declaration` reusa a fila que já existia
//! para `var()`; `style::stylesheet::apply_resolved_decl` ganhou
//! `direction`/`writing_mode` a mais) e só resolvem por elemento em
//! `dom::cascade`, contra os dois já herdados desse elemento (ou já
//! declarados por ele mesmo, na mesma regra — ver o comentário lá) — com UM
//! corte: `dom::direction_herdada` nega o `direction` herdado quando o pai é
//! uma LINHA de flex, porque a ORDEM dos itens da linha ainda não inverte
//! por `direction` (só por `row-reverse`), e margem certa com ordem errada é
//! pior do que as duas erradas (achado pelo WPT `gap-003-rtl`/
//! `gap-006-rtl`). `eixo_x_forward`/`eixo_y_forward`/`to_physical` continuam
//! puras: a pergunta "que lado físico" não muda, só QUANDO se faz.

use super::lengths::{Caixa, parse_inset, parse_side, split_top_ws};
use super::props::ComputedStyle;
use super::text::{Direction, WritingMode, eixo_x_forward, eixo_y_forward};
use super::values::Dimension;

/// `true` para as propriedades cujo lado físico depende do CONTEXTO herdado
/// (`direction` e/ou `writing-mode`) — precisam de ficar PENDENTES até ao
/// elemento (ver "Quando isto resolve" acima), nunca resolvem na regra.
/// `margin`/`padding`/`border`(-width/-style/-color)/`inset`, os DOIS eixos
/// (`-inline-`/`-block-`, shorthand OU longhand), mais `inline-size`/
/// `block-size` (e os `min-`/`max-`, que trocam de EIXO sob `writing-mode`
/// mesmo sem lado a inverter) e os três nomes antigos do WebKit que se
/// reentregam às formas modernas.
pub(crate) fn e_eixo_dependente(prop: &str) -> bool {
    prop.contains("inline-start")
        || prop.contains("inline-end")
        || prop.contains("block-start")
        || prop.contains("block-end")
        || matches!(
            prop,
            "margin-start"
                | "margin-end"
                | "padding-start"
                | "padding-end"
                | "border-start"
                | "border-end"
                | "inset-inline"
                | "inset-block"
                | "margin-inline"
                | "margin-block"
                | "padding-inline"
                | "padding-block"
                | "inline-size"
                | "block-size"
                | "min-inline-size"
                | "min-block-size"
                | "max-inline-size"
                | "max-block-size"
        )
}

/// Traduz o eixo lógico de um nome de propriedade para o lado físico, sob
/// `(wm, dir)`. `"inset-inline-start"` → o lado do eixo INLINE (`left`/
/// `right` em escrita horizontal, `top`/`bottom` em vertical) que
/// `eixo_x_forward`/`eixo_y_forward` disserem ser o início; `block-start`
/// segue o eixo OPOSTO. `None` quando o nome não tem eixo lógico nenhum.
fn to_physical(prop: &str, wm: WritingMode, dir: Direction) -> Option<String> {
    let (inline_par, inline_fwd): ((&str, &str), bool) = if wm.is_horizontal() {
        (("left", "right"), eixo_x_forward(wm, dir))
    } else {
        (("top", "bottom"), eixo_y_forward(wm, dir))
    };
    let (block_par, block_fwd): ((&str, &str), bool) = if wm.is_horizontal() {
        (("top", "bottom"), eixo_y_forward(wm, dir))
    } else {
        (("left", "right"), eixo_x_forward(wm, dir))
    };
    let (inicio_inline, fim_inline) = if inline_fwd { inline_par } else { (inline_par.1, inline_par.0) };
    let (inicio_block, fim_block) = if block_fwd { block_par } else { (block_par.1, block_par.0) };
    for (logico, fisico) in [
        ("inline-start", inicio_inline),
        ("inline-end", fim_inline),
        ("block-start", inicio_block),
        ("block-end", fim_block),
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
            // mesma troca de `to_physical`: o eixo INLINE é X (left/right)
            // em escrita horizontal, Y (top/bottom) em vertical — e vice-versa
            // para o de BLOCO; o sentido vem de `eixo_x_forward`/`eixo_y_forward`.
            let wm = css.writing_mode.unwrap_or_default();
            let dir = css.direction.unwrap_or_default();
            let e_x = (eixo == "inline") == wm.is_horizontal();
            let forward = if e_x { eixo_x_forward(wm, dir) } else { eixo_y_forward(wm, dir) };
            let (inicio, fim) = if forward { (a, b) } else { (b, a) };
            if e_x {
                css.inset_left = inicio;
                css.inset_right = fim;
            } else {
                css.inset_top = inicio;
                css.inset_bottom = fim;
            }
            return true;
        }
    }
    // `margin-inline`/`-block`, `padding-inline`/`-block` — o shorthand de
    // DOIS valores, mesma troca de eixo que `inset-inline`/`-block` acima
    // (sem lado a inverter — os dois valores já vão um para cada ponta — mas
    // COM eixo a trocar: `margin-block:20px` sob `writing-mode` vertical é
    // `left`/`right`, não `top`/`bottom`; achado no WPT `gap-007-lr`, onde
    // este shorthand ainda respondia sempre físico antes desta função).
    if let Some((eixo, caixa)) = match prop {
        "margin-inline" => Some(("inline", Caixa::Margem)),
        "margin-block" => Some(("block", Caixa::Margem)),
        "padding-inline" => Some(("inline", Caixa::Padding)),
        "padding-block" => Some(("block", Caixa::Padding)),
        _ => None,
    } {
        let toks = split_top_ws(val);
        if toks.is_empty() {
            return true;
        }
        let a = parse_side(&toks[0], caixa);
        let b = if toks.len() > 1 { parse_side(&toks[1], caixa) } else { a };
        let wm = css.writing_mode.unwrap_or_default();
        let e_x = (eixo == "inline") == wm.is_horizontal();
        let edges = if caixa == Caixa::Margem { &mut css.margin } else { &mut css.padding };
        if e_x {
            edges.left = a;
            edges.right = b;
        } else {
            edges.top = a;
            edges.bottom = b;
        }
        return true;
    }

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

    // `inline-size` é `width` em escrita horizontal, `height` em vertical
    // (e vice-versa para `block-size`) — mesma troca de `to_physical`, mas
    // sem `direction`: uma DIMENSÃO não tem lado a inverter, só eixo.
    let wm = css.writing_mode.unwrap_or_default();
    let (largura, altura) = if wm.is_horizontal() { ("width", "height") } else { ("height", "width") };
    let dimensao = match prop {
        "inline-size" => Some(largura),
        "block-size" => Some(altura),
        "min-inline-size" => Some(if largura == "width" { "min-width" } else { "min-height" }),
        "min-block-size" => Some(if altura == "height" { "min-height" } else { "min-width" }),
        "max-inline-size" => Some(if largura == "width" { "max-width" } else { "max-height" }),
        "max-block-size" => Some(if altura == "height" { "max-height" } else { "max-width" }),
        _ => None,
    };
    if let Some(fisico) = dimensao {
        return super::parse::aplica_declaracao(css, fisico, val);
    }

    let Some(fisico) = to_physical(prop, wm, css.direction.unwrap_or_default()) else {
        return false;
    };

    // O resto da CAIXA lógica, pela mesma reentrega. O `parse` tinha
    // `padding-inline-start/end` por literal mas não as outras metades das
    // mesmas famílias: `padding-block-end` caía como desconhecida ao lado de
    // uma `margin-block-end` que funcionava. Traduzir o eixo e reentregar
    // fecha as quatro famílias sem um braço por nome — e sem a assimetria
    // poder voltar.
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
