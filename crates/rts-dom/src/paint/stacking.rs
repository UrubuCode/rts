//! ORDEM DE PINTURA do out-of-flow: um `z-index` NEGATIVO pinta-se ANTES do
//! fluxo normal — atrás dele —, nunca depois (CSS 2.1 Apêndice E, passo 3: só
//! o que tem z-index ≥ 0/`auto` fica por cima). `layout_document` sempre
//! pintou a passada out-of-flow inteira DEPOIS do fluxo inteiro, e o
//! comentário que ali estava dizia isso mesmo ("sem z-index real") — o
//! `sort_by_key` já ordenava os posicionados ENTRE SI, nunca contra o fluxo.
//! Medido (`claude-z-index-negativo-atras-do-fluxo`): um `#atras` vermelho
//! `z-index:-1` e um `#fundo` verde em fluxo, mesmo retângulo 100×100 — a
//! ordem saía [verde, vermelho] (vermelho visível, por cima) e devia ser o
//! oposto.
//!
//! Layout e pintura andam juntos nesta engine — uma só passada, e o veredito
//! da auditoria estrutural lista isso como "o que NÃO é problema" enquanto
//! houver um backend imediato só. Por isso "pintar antes" não é reordenar uma
//! lista: é montar os itens negativos numa lista À PARTE e PREPENDER essa
//! lista à frente da que já existe. Since BT-2b that is a splice of pieces at
//! the front (`pieces.rs`); it used to be followed by a correction of every
//! index that pointed by POSITION — the `at`/`hit_at` of each emitted subtree
//! and the subtree counts of each clip marker — and none of those exists now.
//!
//! Um contexto que `opacity<1`, `transform` ou `z-index` explícito abre não
//! deixa o `z-index` de um filho escapar para o contexto raiz. A passada de
//! fora do fluxo ainda monta fragmentos separadamente, por isso ela usa a
//! chave léxica de TODOS os contextos ancestrais: `[0, 100]` (filho 100 dentro
//! do contexto 0) pinta antes de `[1]` (irmão no contexto raiz). Vários
//! negativos continuam na ordem ascendente dentro do mesmo contexto.
//!
//! **A camada 8 do Apêndice E junta `z-index:0` e `z-index:auto`** — dentro
//! de UM contexto, os dois pintam na ordem do documento, sem se separar por
//! terem ou não um `z-index` explícito (só a camada 3, negativos, e a camada
//! 9, positivos, se separam por número). `4c1d08132` só empurrava um
//! componente para a chave quando o PRÓPRIO nó abria contexto — um
//! `position:absolute` com `z-index:0` (abre contexto, chave `[0]`) ficava
//! mais LONGO que um irmão `z-index:auto` (não abre nada, chave `[]`), e
//! `Vec<i32>::cmp` ordena um prefixo antes do vector que o estende — `[] <
//! [0]` sempre, TREE ORDER à parte. Medido: `#atras{z-index:0}` antes de
//! `#fundo{z-index:auto}` no documento pintava `#fundo` por cima (a chave
//! vazia "vencia"), o oposto do Chrome e do binário pré-codex. A chave agora
//! sempre fecha com o `z-index` do PRÓPRIO nó (`auto` lido como `0`, a mesma
//! leitura de [`z_index_of`]) — os ancestrais que abrem contexto continuam a
//! prefixar, então `[0, 100]` ainda fica preso atrás de `[1]`, mas dois
//! irmãos de camada 8 (`0`/`auto`) empatam e o sort ESTÁVEL decide pela
//! árvore, como a camada pede.
//!
//! Moved from `layout/empilhamento.rs` on 2026-09-25 (PQ-A1); nothing in it changed.

use crate::boxes::BoxTree;
use crate::dom::{Dom, NodeIdx};
use crate::paint::list::DisplayList;
use crate::paint::pieces::Piece;
use crate::paint::pieces;

/// O `z-index` computado de um out-of-flow, `0` para `auto`/sem estilo — a
/// mesma leitura que `layout_document` já fazia inline no `sort_by_key`.
///
/// **I6** (`docs/ui/html-engine/box-tree.md` §7): esta função lê o estilo do
/// NÓ, e uma caixa anónima não tem nó nem estilo próprio. Hoje isso não é
/// alcançável — este lote (BT-1) não produz caixas anónimas —, mas o dia em
/// que produzir, esta função responde `0` (a mesma coisa que "auto") em vez
/// de ter um ramo para "esta caixa não tem nó". Deixado como achado para o
/// lote das caixas anónimas, e não corrigido aqui.
pub(crate) fn z_index_of(dom: &Dom, id: NodeIdx) -> i32 {
    dom.computed_style_idx(id)
        .and_then(|c| c.z_index)
        .unwrap_or(0)
}

/// Chave de pintura de `id`: um componente por contexto ANCESTRAL que o
/// isola (o z-index desse contexto), seguido SEMPRE de um último componente
/// — o `z-index` do próprio `id` (`auto` como `0`) — que é o que mantém
/// `z-index:0` e `z-index:auto` na mesma camada 8 do Apêndice E: dois nós do
/// mesmo contexto, um com `0` explícito e outro `auto`, chegam à mesma chave
/// e o sort ESTÁVEL de `layout_document` desempata pela árvore. Sem esse
/// último componente, o nó que NÃO abre contexto próprio (`auto`) ficava com
/// uma chave mais curta que o irmão que abre (`0`), e um vector mais curto
/// ordena antes do que o estende — sempre, tree order à parte.
///
/// A ordenação léxica mantém uma subárvore inteira contida no lugar do seu
/// ancestral: um filho `z-index:100` de um pai `z-index:0` (`[0, 100]`) não
/// ultrapassa o irmão raiz `z-index:1` (`[1]`), porque o PRIMEIRO componente
/// já decide.
pub(crate) fn stacking_key(dom: &Dom, id: NodeIdx) -> Vec<i32> {
    let mut ancestors = Vec::new();
    let mut current = dom.node(id).parent;
    while let Some(node) = current {
        ancestors.push(node);
        current = dom.node(node).parent;
    }
    ancestors.reverse();

    let mut key = Vec::new();
    for node in ancestors {
        if creates_context(dom, node) {
            key.push(z_index_of(dom, node));
        }
    }
    key.push(z_index_of(dom, id));
    key
}

/// `id` isola um `z-index` filho num contexto próprio: `opacity<1`,
/// `transform`, um `position` não-`static` com `z-index` explícito (CSS 2.1
/// Apêndice E), ou — Flexbox §4.3 — um ITEM de contentor flex/grid com
/// `z-index` não-`auto`, mesmo `position:static` (um filho direto de
/// `display:flex`/`grid` participa do empilhamento do contentor sem precisar
/// de `position`). `isolation`/`filter`/`will-change` ficam de fora: nenhum
/// dos três tem campo no `ComputedStyle` — `style/inert.rs` os declara
/// INERTES de propósito — e inventar o parse deles não é o corte deste lote.
fn creates_context(dom: &Dom, node: NodeIdx) -> bool {
    let Some(css) = dom.computed_style_idx(node) else {
        return false;
    };
    if css.opacity.is_some_and(|opacity| opacity < 1.0) || css.transform.is_some() {
        return true;
    }
    if css.z_index.is_none() {
        return false;
    }
    let positioned = css
        .position
        .is_some_and(|position| position != crate::style::Position::Static);
    if positioned {
        return true;
    }
    dom.node(node)
        .parent
        .and_then(|parent| dom.computed_style_idx(parent))
        .and_then(|parent_css| parent_css.effective_display())
        .is_some_and(|display| {
            matches!(
                display,
                crate::style::DisplayKind::Flex
                    | crate::style::DisplayKind::FlexWrap
                    | crate::style::DisplayKind::Grid
            )
        })
}

/// Prepende `antes` a `target`: what `antes` paints — its items, subtrees and
/// geometry marks, in its own order — comes FIRST, further back than
/// everything `target` already had. A splice: no clip already in `target` can come
/// to "contain" the negative subtrees, because a clip contains what lies
/// between its markers and they now lie before both.
pub(crate) fn merge_before(target: &mut DisplayList, antes: DisplayList) {
    if antes.pieces.is_empty() && antes.box_rects.is_empty() {
        return;
    }
    target.pieces.splice(0..0, antes.pieces);
    // `box_rects` é a geometria por CAIXA (era `node_rects`, por nó); a
    // fusão continua sendo uma simples união de mapas — as chaves de `antes`
    // e `target` não colidem, porque vêm de subárvores disjuntas.
    target.box_rects.extend(antes.box_rects);
    target.grid_column_tracks.extend(antes.grid_column_tracks);
    target.scroll_regions.splice(0..0, antes.scroll_regions);
}

/// Appends a positioned fragment after the current list.
///
/// It used to translate the appended subtrees' item and hit indices and NOT
/// the subtree counts its clip markers carried, so an `EndClip` of an
/// `overflow:hidden` positioned box counted subtrees of `target` and let its own
/// children be drawn after it — outside the clip. An append of pieces has no
/// count to forget.
pub(crate) fn merge_after(target: &mut DisplayList, mut depois: DisplayList) {
    if depois.pieces.is_empty() && depois.box_rects.is_empty() {
        return;
    }
    target.pieces.append(&mut depois.pieces);
    target.box_rects.extend(depois.box_rects);
    target.grid_column_tracks.extend(depois.grid_column_tracks);
    target.scroll_regions.append(&mut depois.scroll_regions);
}

/// Is `a` before `b` in DOCUMENT (preorder) order? Walks both ancestor
/// chains to the root and compares at the first level they diverge, by the
/// SIBLING position under their common ancestor — general rather than
/// assuming `a`/`b` are siblings, because a layer 8 out-of-flow box and the
/// `position:relative` mark it is compared against can sit at different
/// depths (CLAUDE.md repro: both direct children of one container, but a
/// nested case is not excluded). An ancestor sorts before its own
/// descendant, matching preorder.
pub(crate) fn is_before_in_tree(dom: &Dom, a: NodeIdx, b: NodeIdx) -> bool {
    if a == b {
        return false;
    }
    let chain_of = |mut n: NodeIdx| {
        let mut chain = vec![n];
        while let Some(p) = dom.node(n).parent {
            chain.push(p);
            n = p;
        }
        chain.reverse();
        chain
    };
    let (ca, cb) = (chain_of(a), chain_of(b));
    let mut i = 0;
    while i < ca.len() && i < cb.len() && ca[i] == cb[i] {
        i += 1;
    }
    if i == ca.len() || i == cb.len() {
        // One is an ancestor of the other: the ancestor's content starts
        // first.
        return i == ca.len();
    }
    let parent = ca[i - 1];
    let siblings = &dom.node(parent).children;
    let pos_a = siblings.iter().position(|&c| c == ca[i]);
    let pos_b = siblings.iter().position(|&c| c == cb[i]);
    pos_a < pos_b
}

/// Is `node` a layer 8 box — CSS 2.1 Appendix E: `position:relative` with
/// `z-index: auto`/`0` — that an out-of-flow sibling of the same layer must
/// paint BEFORE if it precedes `node` in the document? Read straight from the
/// style at splice time, computed on demand rather than recorded during
/// layout: a stored index into `pieces` is exactly the bookkeeping BT-2b
/// deleted (this module's header), and every `Piece::Child` this walks
/// through would need its own index space anyway, since it is a REUSED
/// subtree potentially built by an earlier frame or shared with another box
/// entirely (`layout/fragment/fragment.rs`'s cache).
fn is_layer8_relative(dom: &Dom, node: NodeIdx) -> bool {
    dom.computed_style_idx(node)
        .is_some_and(|css| css.position == Some(crate::style::Position::Relative) && z_index_of(dom, node) == 0)
}

/// Splices `insert` into `pieces` right before the EARLIEST layer 8
/// `position:relative` box that follows `target` in document order, searched
/// depth-first in paint order — descending into a `Piece::Child`'s own
/// subtree when its root box is not itself the match, since the relative box
/// this out-of-flow sibling has to land before can be nested inside a cached
/// container (`layout/fragment/fragment.rs` wraps EVERY ordinary block child as one, so the
/// two are direct DOM siblings far more often than they are direct
/// `pieces`-array neighbours).
///
/// A matched `Piece::Child`'s cached [`Fragment`] is never mutated in place —
/// it may be shared with another frame or another box entirely — so the path
/// from `pieces` down to the match is copy-on-written: only the fragments
/// actually on that path get a fresh `Rc`, and every sibling subtree the walk
/// does not enter keeps pointing at the exact `Rc` it already had.
///
/// `(dx, dy)` is the offset the caller's OWN pieces already carry at this
/// depth — the sum of every `ChildRef::{dx,dy}` walked through to reach
/// `pieces` (zero at the top-level call from `layout.rs`). `insert`'s items
/// hold PAGE-ABSOLUTE coordinates, as `layout_out_of_flow` computed them
/// against `flow_rects` (itself page-absolute) — never coordinates relative
/// to some fragment's own build-time origin. `Fragment::emit_at`'s own doc
/// comment states the reverse for a REUSED subtree: its pieces are recorded
/// absolute AS OF BUILD TIME, and everything under a `Piece::Child` is
/// walked with that child's `(dx, dy)` ADDED on top (`pieces::walk`). Splicing
/// `insert` straight into a `Piece::Child`'s own subtree — one level below the
/// top of `pieces` — used to leave its absolute coordinates as they were, so
/// that child's own `(dx, dy)` landed on them a SECOND time: a `<tbody>` cell
/// reused 8px away from where it was first built shifted its absolute
/// sibling `.indicator` by 8px in both axes when the two landed side by side
/// (measured against `position-relative-table-tbody-left-absolute-child.html`:
/// the indicator painted at `(116, 16)` against the tbody's own `(108, 8)`,
/// both meant to coincide). Subtracting the accumulated `(dx, dy)` before a
/// splice at THIS depth cancels exactly the amount the walk will re-add.
///
/// `Err(insert)` unchanged when no match exists anywhere in `pieces` — the
/// caller appends it at the end instead, same as before this fix (Appendix E
/// layer 8 paints a box with nothing after it last among ties).
pub(crate) fn splice_layer8(
    dom: &Dom,
    tree: &BoxTree,
    pieces: &mut Vec<Piece>,
    target: NodeIdx,
    mut insert: Vec<Piece>,
    dx: f32,
    dy: f32,
) -> Result<(), Vec<Piece>> {
    for i in 0..pieces.len() {
        // `Piece::Rect(box_id)` is the mark EVERY box leaves at its own paint
        // position, reserved before its own content and descendants
        // (`layout/fragment/items.rs::reserve_box_order`) — unlike `Piece::Child`, it exists
        // whether or not this box went through the fragment cache, which is
        // what a table's internals (`table/mod.rs` calls `layout_block`
        // straight, never `layout_block_reusing`) need: a `<tbody>` never
        // gets its own `Piece::Child`, only this mark.
        if let Piece::Rect(box_id) = &pieces[i] {
            if tree
                .node_of(*box_id)
                .is_some_and(|n| is_layer8_relative(dom, n) && is_before_in_tree(dom, target, n))
            {
                // `Piece::Rect(box_id)` is NOT this box's earliest paint
                // position: `layout/block/block.rs` reserves it at `box_start`, lays out
                // the children (appended after), and only THEN inserts the
                // box's own background/border AT `box_start` — pushing the
                // `Rect` one slot later than where the box's OWN paint
                // actually starts (`layout/block/block.rs` around `record_box_rect`,
                // comment "o fundo... insert no box_start"). Walking
                // backward over plain `Item`s is safe: the previous sibling
                // finished its ENTIRE insert cycle before this box's
                // `box_start` was even captured, so nothing between the end
                // of that sibling and this `Rect` can belong to anyone else.
                let mut splice_at = i;
                while splice_at > 0 && matches!(pieces[splice_at - 1], Piece::Item(_)) {
                    splice_at -= 1;
                }
                pieces::shift_from(&mut insert, 0, -dx, -dy);
                pieces.splice(splice_at..splice_at, insert);
                return Ok(());
            }
            continue;
        }
        let Piece::Child(c) = &pieces[i] else { continue };
        if tree
            .node_of(c.caixa)
            .is_some_and(|n| is_layer8_relative(dom, n) && is_before_in_tree(dom, target, n))
        {
            pieces::shift_from(&mut insert, 0, -dx, -dy);
            pieces.splice(i..i, insert);
            return Ok(());
        }
        let mut subtree = (*c.fragment.pieces).clone();
        match splice_layer8(dom, &c.fragment.tree, &mut subtree, target, insert, dx + c.dx, dy + c.dy) {
            Ok(()) => {
                let mut novo = (*c.fragment).clone();
                novo.pieces = std::rc::Rc::new(subtree);
                let Piece::Child(c) = &mut pieces[i] else { unreachable!() };
                c.fragment = std::rc::Rc::new(novo);
                return Ok(());
            }
            Err(devolvido) => insert = devolvido,
        }
    }
    Err(insert)
}
