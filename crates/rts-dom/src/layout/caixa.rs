//! **A pergunta "que caixa é esta?" mora AQUI.**
//!
//! A pergunta *"é de bloco?"* — escrita quase sempre ao contrário, como *"não é
//! inline?"* — está em CINCO sítios desta árvore, e cada correção só chegou a um
//! deles de cada vez: quatro lotes, quatro medições, e a quinta cópia foi
//! encontrada depois das outras quatro. Três dos cinco vivem agora aqui.
//!
//! | # | onde | forma |
//! |---|---|---|
//! | 1 | `is_block_level`, neste ficheiro | função — **aqui** |
//! | 2 | `is_inline_block`, neste ficheiro | função — **aqui** |
//! | 3 | `em_contexto_inline` e `whitespace_is_inline_separator`, neste ficheiro | `!is_block_level(..) && !is_inline_block(..)`, à mão — **aqui** |
//! | 4 | `inline_box::cria_caixa_apesar_de_inline` | função, **noutro ficheiro** — o `layout` chama-a de dois sítios |
//! | 5 | dentro do laço de `layout_children_vertical` | escrita à mão no corpo da função — **não é movível** sem extrair uma função, o que deixa de ser um `move` |
//!
//! **Os 4 e 5 não estão aqui, e é uma limitação e não um esquecimento.** Movê-los
//! é mudança de comportamento, não arrumação: o 4 muda a API entre dois ficheiros
//! que outros consomem, e o 5 exige extrair uma função de dentro de um laço.
//! Juntá-los é um lote medido, à parte.
//!
//! **A forma do defeito, que é o que interessa a quem acrescentar a sexta:**
//! perguntar *"não é inline?"* onde a pergunta é *"é de bloco?"* erra para o lado
//! ERRADO. Um elemento que não é inline nem de bloco — um `inline-block`, um
//! `table-cell`, um substituído — responde "sim" a uma e "não" à outra, e é por
//! isso que cada cópia falha sozinha e tem de ser corrigida sozinha.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

/// `true` quando `display:inline` torna `width` e `height` inoperantes. A
/// classificação de caixa e o fluxo inline partilham esta pergunta para não
/// transformar uma dimensão declarada num inline-block artificial.
pub(in crate::layout) fn ignores_inline_dimensions(css: Option<&ComputedStyle>, tag: &str) -> bool {
    // O display USADO é `inline` quando declarado (`display:inline`
    // explícito) OU quando nada o declara e a tag também não tem um default
    // de bloco (`block::lookup`) — que é o caso comum de um `<span>` sem
    // `display` nenhum. A versão anterior só via o primeiro: um `<span>{
    // height:20px }` sem `display` declarado tinha `effective_display() ==
    // None`, a condição respondia `false`, e a `height` era aplicada a um
    // inline puro — o desvio medido no Blink em `claude-sel-has.html`
    // (`#rotulo-com`/`#rotulo-sem`, esperado 0×0, obtido 20 de altura).
    let usa_inline = match css.and_then(|c| c.effective_display()) {
        Some(d) => d == crate::style::DisplayKind::Inline,
        None => crate::block::lookup(tag).is_none(),
    };
    usa_inline && css.is_some_and(|c| c.width.is_some() || c.height.is_some())
}

/// `true` se este estilo, isoladamente (sem a tag/`replaced`/`display`
/// declarado — os chamadores já os testaram antes), justifica `layout_block`
/// como FILHO DE BLOCO independente (linha própria, largura do contentor).
///
/// Quando `ignora_dimensoes` (ver [`ignores_inline_dimensions`]) a resposta é
/// SEMPRE `false` — e não `cria_caixa_apesar_de_inline(c)`, a tentativa
/// anterior: um inline por omissão com `background` NÃO ganha linha própria
/// só por ter fundo (essa era a regressão medida em `claude-sel-has.html` —
/// `#rotulo-com`/`#rotulo-sem`, que TÊM `background-color` além da `height`,
/// passaram a `1280×20` numa linha só sua). `layout_block` aplicaria a
/// `height` de qualquer forma — ignora-la aqui e não lá é a mesma regra que
/// `is_inline_block`/`is_block_level` já aplicam para o resto do fluxo, e é
/// esse desacordo entre os dois que produzia o valor errado.
pub(in crate::layout) fn cria_caixa_via_dimensoes(
    css: Option<&ComputedStyle>,
    ignora_dimensoes: bool,
) -> bool {
    !ignora_dimensoes && css.is_some_and(|c| c.has_box() || c.height.is_some())
}

/// `true` se um nó-elemento deve ser tratado como BLOCO no layout (entra em
/// `layout_block`, com sua própria caixa/eixo) — em vez de inline (texto corrido).
/// É bloco se: tem `display` no CSS (qualquer um define caixa própria), OU tem um
/// default de display registrado (`block::lookup` = defineBlock, alimentado pela
/// UA-stylesheet `ua.ts` para div/p/… e pelo autor). Tags inline puras (sem nada
/// disso) fluem como texto. O motor NÃO nomeia tags HTML — os defaults são dados
/// do prelude TS.
pub(in crate::layout) fn is_block_level(dom: &Dom, id: NodeIdx) -> bool {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            // `<img>` é um elemento REPLACED → precisa de layout_block p/ ter a
            // sua caixa registada e emitir o DisplayItem::Image, mesmo sem CSS de
            // caixa. A condição era ter PIXELS decodificados, e é a mesma
            // pergunta errada que `layout_image` fazia: uma imagem que declara
            // `width`/`height` ocupa espaço antes de chegar da rede, e sem rede
            // nunca chega. Quem sabe dizer se ela se dimensiona é
            // `replaced_inline_size`, o único sítio onde essa regra vive.
            // Não é `return`: um `<img>` sem atributos ainda pode ganhar caixa
            // pelo CSS, e quem responde isso são as regras no fim desta função.
            if tag == "img"
                && (dom.image_of(id).is_some()
                    || dom.node(id).attr("width").is_some()
                    || dom.node(id).attr("height").is_some())
            {
                return true;
            }
            // `<canvas>` é REPLACED como o `<img>`: a caixa vem dos atributos
            // `width`/`height` e o conteúdo são pixels. Sem esta linha ele cai no
            // fluxo inline, onde não há quem emita a superfície — e um canvas
            // pintado não aparecia na tela, com o resto da página intacto.
            if tag == "canvas" {
                return true;
            }
            // `<svg>` é replaced: layout_svg_placeholder reserva a caixa (logo/
            // ícones do google ocupam o espaço certo em vez de colapsar).
            if tag == "svg" {
                return true;
            }
            let css = dom.computed_style_idx(id);
            // `display:inline` DECLARADO vence a tag. Sem esta pergunta, um
            // `<h3 style="display:inline">` — a forma que o MediaWiki usa nos
            // cabeçalhos das secções colapsáveis — caía no caminho de bloco e
            // saía com os 752px do contentor em vez dos ~55px do seu texto.
            // A condição anterior era `effective_display().is_some()`, que
            // responde "há display declarado" e não "é de bloco": um display
            // inline-level entrava por ela como qualquer outro.
            // A alternativa rejeitada era filtrar por tag (tratar `h3`/`li` à
            // parte): a tag não é o que decide isto, o display é, e uma lista
            // de tags teria de crescer a cada página nova.
            // Um inline com fundo/padding ainda precisa de `layout_block` para
            // os pintar, e quem responde isso é
            // `cria_caixa_apesar_de_inline` — não `cria_caixa_de_bloco`, que
            // conta a margem que o `display:inline` acabou de tornar
            // inoperante e devolveria o `<h3>` ao caminho de onde ele saiu.
            if ignores_inline_dimensions(css.as_deref(), tag) {
                return false;
            }
            if css.as_ref().and_then(|c| c.effective_display())
                == Some(crate::style::DisplayKind::Inline)
            {
                return css
                    .as_ref()
                    .map(|c| crate::inline_box::cria_caixa_apesar_de_inline(c))
                    .unwrap_or(false);
            }
            css.as_ref().and_then(|c| c.effective_display()).is_some()
                || crate::block::lookup(tag).is_some()
                // INLINE-BLOCK de fato: um elemento inline (`<a>`/`<span>`/`<button>`)
                // que tem CAIXA própria (fundo/borda/padding/width/height) precisa de
                // layout_block p/ pintar essa caixa e respeitar o padding — senão o
                // botão fica sem fundo/borda. (`has_box` cobre bg/pad/margin/border/
                // radius/width; +height.)
                // Uma tag inline só vira bloco quando o estilo CRIA caixa — ver
                // `inline_box::cria_caixa_de_bloco` para porque não é `has_box`.
                || css.as_ref().map(|c| crate::inline_box::cria_caixa_de_bloco(c)).unwrap_or(false)
        }
        _ => false,
    }
}

/// `true` se o elemento é INLINE-BLOCK: tem caixa (vira bloco p/ pintar) MAS é inline
/// por natureza (`<a>`/`<span>`/`<button>`/etc., não uma tag block) e SEM width que
/// ocupe o pai → dimensiona pelo CONTEÚDO (shrink-to-fit), como o pill/botão. Tags
/// block conhecidas (div/p/section…) NÃO são inline-block (ocupam o pai).
pub(in crate::layout) fn is_inline_block(dom: &Dom, id: NodeIdx) -> bool {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            let css = dom.computed_style_idx(id);
            if ignores_inline_dimensions(css.as_deref(), tag) {
                return false;
            }
            // Um `display:inline-block` DECLARADO responde `true` e não chega às
            // perguntas seguintes — nem à tag, que é o que o tornava invisível.
            //
            // A pergunta abaixo é `d != Inline`, e por ela o `InlineBlock` contava
            // como display DE BLOCO: um `<li style="display:inline-block">` batia
            // no `block::lookup("li")` e saía `false`. O efeito dependia do pai e
            // por isso escondia-se: sob um pai de bloco a caixa saía com a largura
            // toda (760 onde o Chrome dá 22,2) e sob um pai inline o elemento
            // ficava SEM CAIXA NENHUMA, que é conteúdo invisível e não geometria
            // errada. São os `<li>` do menu principal da Wikipédia.
            //
            // É a mesma forma do defeito que `is_block_level` já tinha — perguntar
            // "não é inline?" quando a pergunta é "é de bloco?" — e o `InlineBlock`
            // é exatamente o valor que as duas leituras separam.
            if css.as_ref().and_then(|c| c.effective_display())
                == Some(crate::style::DisplayKind::InlineBlock)
            {
                return true;
            }
            let explicit_block = css
                .as_ref()
                .and_then(|c| c.effective_display())
                .map(|d| d != crate::style::DisplayKind::Inline)
                .unwrap_or(false);
            // `<input>`/`<button>`/`<select>`/`<textarea>` são inline-block por
            // DEFAULT do browser (fluem lado a lado — os botões do google), a não
            // ser que o CSS do autor force um display de bloco. É o que faz os 2
            // `<input type=submit>` irmãos ficarem na MESMA linha.
            if matches!(tag.as_str(), "input" | "button" | "select" | "textarea") {
                return !explicit_block;
            }
            // tag block conhecida OU display de bloco explícito → NÃO é inline-block.
            if crate::block::lookup(tag).is_some() || explicit_block {
                return false;
            }
            // é inline-com-box (tem caixa mas é tag inline) → inline-block.
            css.as_ref()
                .map(|c| crate::inline_box::cria_caixa_de_bloco(c))
                .unwrap_or(false)
        }
        _ => false,
    }
}

/// `true` se a tag NÃO é renderável — metadata do documento (`<head>` e o que vive
/// nele: `<title>`, `<meta>`, `<link>`, `<base>`) e os recursos `<style>`/`<script>`
/// (o CSS já virou stylesheet no parse; JS não executamos). Permite carregar um HTML
/// COMPLETO e pintar só o conteúdo visível (`<body>`). `<html>`/`<body>` SÃO
/// renderáveis (transparentes — fluxo block normal dos filhos).
pub(crate) fn is_non_rendered_tag(tag: &str) -> bool {
    matches!(
        tag,
        "head" | "title" | "meta" | "link" | "base" | "style" | "script"
    )
}

/// O código de `display` de um nó: o CSS (`display:` parseado) VENCE; se não
/// declarado, cai no default da tag (`block::lookup`, a UA-stylesheet via
/// defineBlock); senão vertical. É o eixo de empilhamento dos filhos.
/// Códigos: 0=vertical/block, 1=wrap, 2=horizontal/flex, -1=none.
pub(in crate::layout) fn css_display(dom: &Dom, id: NodeIdx) -> i64 {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            // 1) CSS explícito (display:flex/block/inline/none) tem prioridade.
            if let Some(css) = dom.computed_style_idx(id) {
                if let Some(kind) = css.effective_display() {
                    return kind.to_display_code();
                }
            }
            // 2) default da tag: defineBlock (UA-stylesheet do TS) tem prioridade;
            // senão as tags HTML block conhecidas (div/p/…) são vertical; o resto
            // também cai em vertical (default seguro para um container).
            crate::block::lookup(tag)
                .map(|d| d.display)
                .unwrap_or(crate::block::DISPLAY_VERTICAL)
        }
        _ => crate::block::DISPLAY_VERTICAL,
    }
}

/// O `display` USADO de um nó: o `display` computado (autor OU UA — a UA já
/// entra na cascade via `style/ua.css`, lote I), senão `None` — e `None` aqui
/// significa "o fluxo de bloco genérico decide", não "sem display".
///
/// Existe ao lado de [`css_display`] e não em vez dele porque as duas respondem
/// a perguntas diferentes: aquela dá o EIXO em que os filhos empilham (um `i64`
/// que o TS também escreve), esta dá o PAPEL da caixa. Um `<tr>` tem eixo
/// vertical e papel de linha de tabela, e só a segunda pergunta o distingue de
/// um `<div>`.
pub(crate) fn used_display(dom: &Dom, id: NodeIdx) -> Option<crate::style::DisplayKind> {
    let NodeKind::Element { .. } = &dom.node(id).kind else {
        return None;
    };
    // O `display` de papel (`list-item`/`table`/`table-row`/`table-cell`/…)
    // já chega pela CASCADE — `style/ua.css` declara-o para cada tag como
    // qualquer outra propriedade, na origem UA (lote I). Antes disto era
    // `crate::block::ua_display(tag)`, um `match` chamado AQUI, depois da
    // cascade, que uma regra de autor (`td { display: block }`) nunca
    // conseguia vencer — porque a cascade nunca a via.
    dom.computed_style_idx(id).and_then(|css| css.effective_display())
}

/// O `font-size` COMPUTADO em pontos: a CASCADE já resolveu a forma para `Px`
/// (dom.rs — base de em/% é o pai; rem/vw/vh contra root/viewport), então aqui é
/// só extrair; `fallback` cobre o não-declarado (herda) e formas não-resolvidas.
pub(crate) fn font_px(css: &ComputedStyle, fallback: f32) -> f32 {
    match css.font_size {
        Some(crate::style::Dimension::Px(v)) => v,
        _ => fallback,
    }
}

/// `true` se este filho tem TEXTO (ou um inline puro) por vizinho — isto é, se
/// está dentro de uma linha em vez de estar sozinho entre blocos.
///
/// Serve para decidir se um inline-block flui na linha ou abre corrida própria.
/// A pergunta é a mesma que o whitespace faz, com um vizinho a mais: o texto
/// pode estar antes OU depois, e um `<span>` com fundo no fim de um parágrafo
/// pertence à linha do texto que o antecede.
pub(in crate::layout) fn em_contexto_inline(dom: &Dom, parent: NodeIdx, child: NodeIdx) -> bool {
    let irmaos = &dom.node(parent).children;
    let Some(pos) = irmaos.iter().position(|&c| c == child) else {
        return false;
    };
    let e_inline = |idx: NodeIdx| match &dom.node(idx).kind {
        NodeKind::Text(t) => !t.trim().is_empty(),
        NodeKind::Element { tag } => {
            !is_non_rendered_tag(tag)
                && !is_out_of_flow(dom, idx)
                && !is_block_level(dom, idx)
                && !is_inline_block(dom, idx)
        }
        _ => false,
    };
    irmaos[..pos].iter().rev().any(|&c| e_inline(c))
        || irmaos[pos + 1..].iter().any(|&c| e_inline(c))
}

/// Retorna se um whitespace entre irmãos deve participar do contexto inline. O
/// parser preserva o nó de texto por fidelidade ao DOM, mas whitespace entre dois
/// blocos/floats não cria uma linha visual; whitespace adjacente a texto/inline sim.
pub(in crate::layout) fn whitespace_is_inline_separator(
    dom: &Dom,
    parent: NodeIdx,
    child: NodeIdx,
) -> bool {
    let children = &dom.node(parent).children;
    let Some(pos) = children.iter().position(|&c| c == child) else {
        return false;
    };
    let is_inline_candidate = |idx: NodeIdx| match &dom.node(idx).kind {
        NodeKind::Text(t) => !t.trim().is_empty(),
        NodeKind::Element { tag } => {
            !is_non_rendered_tag(tag)
                && !is_block_level(dom, idx)
                && !is_inline_block(dom, idx)
                && float_of(dom, idx) == crate::style::FloatSide::None
        }
        _ => false,
    };
    let previous = children[..pos]
        .iter()
        .rev()
        .copied()
        .find(|&idx| !matches!(&dom.node(idx).kind, NodeKind::Text(t) if t.trim().is_empty()));
    let next = children[pos + 1..]
        .iter()
        .copied()
        .find(|&idx| !matches!(&dom.node(idx).kind, NodeKind::Text(t) if t.trim().is_empty()));
    previous.is_some_and(is_inline_candidate) || next.is_some_and(is_inline_candidate)
}

/// Um elemento inline de texto não cria caixa própria no fluxo, mas ainda assim
/// deve receber um retângulo união dos fragmentos que seus descendentes pintam.
pub(in crate::layout) fn is_inline_text_container(dom: &Dom, id: NodeIdx) -> bool {
    match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            !is_non_rendered_tag(tag)
                && !is_out_of_flow(dom, id)
                && !is_block_level(dom, id)
                && !is_inline_block(dom, id)
        }
        _ => false,
    }
}
