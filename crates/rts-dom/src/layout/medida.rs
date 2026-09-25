//! As larguras intrínsecas e as alturas externas que os pré-passos pedem.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.
//!
//! O `TextMeasurer`/`ApproxMeasurer` MUDOU-SE para `medidor_texto.rs` no lote
//! `medidor-ahem` (este ficheiro já estava no teto de 500 linhas do resto do
//! workspace e os métodos `_family` novos não cabiam sem passar o teto).
//!
//! A travessia pela ÁRVORE DE CAIXAS (a caixa concreta de um filho, o corpo
//! "geral" bloco/flex, a caixa anónima) MUDOU-SE para `medida_arvore.rs` no
//! lote que corrigiu 147cb3e53/02bc7088d — este ficheiro já estava perto do
//! teto de 500 linhas do resto do workspace e o fix (uma caixa anónima
//! deixar de ser saltada, e um fragmento deixar de se misturar com os
//! outros do mesmo nó) não cabia sem o passar.

use super::*;
pub use super::medidor_texto::{ApproxMeasurer, TextMeasurer};
use super::medida_arvore::{intrinsic_content_width_sem_cache, intrinsic_outer_width_de};

/// Largura NATURAL do conteúdo de um nó (sem `width` explícito): a maior largura
/// de uma linha de texto entre os descendentes. É o "preferred width" do
/// shrink-to-fit (item flex / inline-block). Para um filho-bloco com `width`, usa
/// esse width (+ frame); para texto, a largura medida. Aproximação do max-content
/// (o inline-flow exato — palavras quebrando — vem na fatia de inline).
pub(in crate::layout) fn content_natural_width(
    dom: &Dom,
    id: NodeIdx,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    intrinsic_content_width(dom, id, font, ctx)
}

/// LARGURA INTRÍNSECA do CONTEÚDO de um elemento (max-content): quanto o conteúdo
/// QUER de largura sem quebrar. É a BASE de toda medição (shrink-to-fit, item flex,
/// inline-block, container flex). CONSCIENTE DO DISPLAY dos filhos:
/// - flex-ROW (horizontal/wrap): SOMA as larguras outer dos filhos + os gaps (eles
///   ficam lado a lado). Era o bug do navbar: `.logo`/`.links` (flex) mediam pelo
///   MAX, dando ~0.
/// - block (vertical): MAX das larguras dos filhos (empilham).
/// - texto: a largura do texto concatenado.
/// Recursivo: a largura de um filho é a SUA intrínseca + frame (ou seu `width` fixo).
///
/// O corpo que percorre a árvore de caixas vive em `medida_arvore.rs`
/// (`intrinsic_content_width_sem_cache`); esta função só resolve a CACHE, que
/// é chaveada por `id` e por isso só pode responder pela pergunta "todas as
/// caixas deste nó, dobradas pelo máximo" — nunca por UM fragmento
/// específico (ver o comentário lá).
pub(in crate::layout) fn intrinsic_content_width(
    dom: &Dom,
    id: NodeIdx,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let key = IntrinsicWidthKey {
        tree: dom.cache_identity(),
        node_epoch: dom.layout_epoch(id),
        style_epoch: crate::style::props::style_epoch(),
        node: id,
        font_size: font.to_bits(),
        viewport_w: ctx.viewport_w.to_bits(),
        viewport_h: ctx.viewport_h.to_bits(),
        measurer: ctx.measurer.identity(),
    };
    crate::bump!(intrinsic_calls);
    if let Some(hit) = dom.intrinsic_width_get(key) {
        crate::bump!(intrinsic_hits);
        return hit;
    }
    // `caixa: None` = "sem caixa concreta em mãos": dobra sobre TODAS as
    // caixas de `id` (ver o comentário em `intrinsic_content_width_sem_cache`).
    // A CACHE fica só aqui, chaveada por `id` — nunca dentro da função sem
    // cache, que também é chamada com uma caixa ESPECÍFICA por
    // `intrinsic_outer_width_de` (um fragmento entre vários de `id`), e
    // cachear por `id` misturaria a resposta de um fragmento com a do outro.
    let tree = dom.box_tree();
    let width = intrinsic_content_width_sem_cache(dom, &tree, id, None, font, ctx);
    dom.intrinsic_width_put(key, width);
    width
}

/// A largura OUTER intrínseca de UM filho (max-content): seu `width` fixo (+ frame),
/// senão a intrínseca do seu conteúdo (+ frame). Texto → largura do texto.
pub(crate) fn intrinsic_outer_width(
    dom: &Dom,
    id: NodeIdx,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    intrinsic_outer_width_de(dom, &dom.box_tree(), id, None, parent_font, ctx)
}

/// Altura OUTER que um filho QUER, para o align-items/cross-axis. Para nós-bloco,
/// MEDE chamando o `layout_block` real numa `DisplayList` DESCARTÁVEL — assim a
/// altura medida é EXATAMENTE a que será pintada (inclui height explícito, frame,
/// recursão nos filhos, %). Sem aproximação: a verificação adversarial pegou que a
/// estimativa por "nº de linhas × line-height" divergia da pintura quando o filho
/// tinha frame próprio ou múltiplas linhas, errando a centralização cross-axis.
///
/// `caixa` é a caixa de `id` que o chamador encontrou ao andar a árvore — a
/// que se mede, e não uma redescoberta pelo nó (ver `measure_block`).
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn child_outer_height(
    dom: &Dom,
    id: NodeIdx,
    caixa: crate::boxes::BoxId,
    container_w: f32,
    container_h: Option<f32>,
    parent_css: &ComputedStyle,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    match &dom.node(id).kind {
        // Como no eixo horizontal: qualquer elemento renderável mede-se pela sua
        // caixa real, porque um inline blockificado (item de flex) tem uma.
        NodeKind::Element { tag } if !is_non_rendered_tag(tag) => {
            // layout de teste numa lista descartável: o (_, outer_h) é a altura real.
            let (_, outer_h) =
                measure_block(dom, id, caixa, container_w, container_h, None, None, true, ctx);
            outer_h
        }
        // A MESMA altura que o fluxo dará a esta linha — medir com o default do
        // medidor enquanto o pai declara `line-height` fazia a medida do
        // cross-axis discordar da pintura, que é o erro que este comentário
        // acima diz que a verificação adversarial já apanhou uma vez.
        NodeKind::Text(_) => {
            crate::inline_box::altura_da_linha(parent_css, parent_font, ctx.measurer)
        }
        _ => 0.0,
    }
}

/// Largura OUTER que um filho QUER (sem pintar), para decidir a quebra de linha no
/// modo wrap. Bloco com `width`: esse width (+ frame); sem width: largura natural
/// do conteúdo (+ frame); texto solto: a largura do texto.
pub(in crate::layout) fn child_outer_width(
    dom: &Dom,
    id: NodeIdx,
    container_w: f32,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    match &dom.node(id).kind {
        // QUALQUER elemento renderável, e não só os de nível bloco: um `<span>`
        // BLOCKIFICADO (item de flex, float) tem largura natural como qualquer
        // outra caixa. Com o guard antigo caía no `_ => 0.0` e era medido como
        // tendo largura ZERO — a caixa existia e não tinha tamanho.
        NodeKind::Element { tag } if !is_non_rendered_tag(tag) => {
            let css = dom.computed_style_idx(id).unwrap_or_default();
            let font = font_px(&css, parent_font);
            let resolve = ResolveCtx {
                parent_content_w: container_w,
                node_font_size: font,
                root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            // frame horizontal = margin_h + 2*border + padding_h (cada já é o eixo;
            // unidades relativas resolvidas contra o container).
            let frame = css.margin.resolve_h(&resolve)
                + { let [_, r, _, l] = crate::style::borders::used_widths(&css); l + r }
                + css.padding.resolve_h(&resolve);
            // Em border-box, o `width` declarado JÁ é a caixa (outer sem margin) —
            // não soma pad/border de novo; só a margin. Em content-box, soma o frame.
            //
            // SEM clamp de `max-width`/`min-width` aqui de propósito: esta
            // função também dá a BASE de um item flex (`flex_base_outer`), e
            // Flexbox §9.2 passo 3 é explícito — a flex base size NÃO é
            // grampeada por max-width; só a hypothetical main size (§9.7) o
            // faz, ao CONGELAR o encolhimento nela — capar aqui pré-cortava
            // o orçamento dos OUTROS itens da linha antes de tempo
            // (`claude-flex-base-size-max-width`, `#capado{max-width:100}`
            // com conteúdo 300 tinha de entrar na conta como 300, não 100).
            // O clamp para um FLOAT sem `width` (que precisa dele) vive no
            // chamador em `vertical.rs`, não aqui.
            match css.width.and_then(|d| d.resolve(&resolve)) {
                Some(w) if css.border_box.unwrap_or(false) => w + css.margin.resolve_h(&resolve),
                Some(w) => w + frame,
                None => content_natural_width(dom, id, font, ctx) + frame,
            }
        }
        // A loose text node measures as `intrinsic_outer_width` measures it:
        // its lines under its parent's `white-space` and font (`text_measure`).
        NodeKind::Text(_) => super::text_measure::intrinsic_text_width(dom, id, parent_font, false, ctx),
        _ => 0.0,
    }
}

/// Concatena o texto de todos os descendentes de `id` (ordem de documento).
pub(in crate::layout) fn collect_text(dom: &Dom, id: NodeIdx) -> String {
    let _phase = crate::metrics::phases::scope("collect-text");
    let mut out = String::new();
    collect_into(dom, id, &mut out);
    return out;

    fn collect_into(dom: &Dom, id: NodeIdx, out: &mut String) {
        match &dom.node(id).kind {
            NodeKind::Text(t) => out.push_str(t),
            // `<script>`/`<style>` não são conteúdo renderável — o texto cru
            // deles não entra no texto pintado (mesmo skip do collect_runs).
            NodeKind::Element { tag } if is_non_rendered_tag(tag) => {}
            _ => {
                for &c in &dom.node(id).children {
                    collect_into(dom, c, out);
                }
            }
        }
    }
}
