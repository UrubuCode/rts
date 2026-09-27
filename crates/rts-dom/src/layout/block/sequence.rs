//! WHERE THE CHILD SEQUENCE OF A BLOCK FLOW COMES FROM — the box tree.
//!
//! Until this module existed, `layout_children_vertical` walked
//! `dom.node(id).children` and asked the tree only "which box is this child's".
//! The tree entered as IDENTITY and the DOM kept the ORDER, so a box the DOM
//! does not have — an anonymous box, the whole point of the layer — could not be
//! reached by anyone descending, however well the build had constructed it.
//!
//! **The sequence is `tree.children(box_id)`, in the tree's order.** A TEXT box
//! is in it with its box, and an ANONYMOUS box is a STEP OF ITS OWN —
//! [`FlowStep::AnonymousBox`] — which the flow lays out through
//! [`super::block_box`], as the block box CSS 2.1 §9.2.1.1 says it is.
//!
//! **That replaced an `expande_anonima` that poured the box's children into the
//! sequence one by one.** It was the right thing while the flow had no path
//! that could lay a box with no `NodeIdx` out — each child reached the same arm
//! that had received it before the tree existed, and no answer moved. Now there
//! is such a path, and dissolving the box here would be throwing away the box
//! the split exists to produce.
//!
//! ## The one thing that still comes from the DOM, and why
//!
//! A node that generates NO box — a comment, and a whitespace-only run the
//! split declined to wrap — is spliced back in at its DOM position, with
//! `caixa: None`.
//!
//! That is not tidiness. A comment reaching `layout_children_vertical` falls
//! into the generic inline arm, and two things happen there that are not
//! nothing: it FLUSHES the run of consecutive inline-blocks, and it opens an
//! inline group whose flush resets `borda` and `strut` — so a comment between
//! two block siblings breaks their margin collapse. Dropping comments from the
//! sequence would therefore CHANGE answers, in the direction that looks like a
//! fix, on pages that have nothing to do with this lot. Whether a comment should
//! stop a margin collapse is its own lot, measured on its own.
//!
//! ## What replaced the `debug_assert!` of the mirror
//!
//! The old one compared two sequences — the DOM's children against the boxes the
//! follower handed out — and it earned its keep: it fired in 322 tests at once
//! when text gained a box. With the order coming from the tree there are no
//! longer two sequences to compare, so the check it made is not expressible.
//!
//! [`flow_sequence`] asserts what remains checkable and is just as silent
//! when it breaks: that the box being descended into belongs to the tree the
//! `DisplayList` carries (the GENERATION), that every box emitted is a child of
//! that box in that tree, and that no box of the tree is dropped on the way.

use super::*;
use crate::boxes::{BoxId, BoxTree};

/// One step of the block flow's descent over the box tree.
///
/// **An ENUM and not a struct with an `Option<NodeIdx>`**, which was the other
/// shape available. A box with no node is the whole point of this layer, and
/// every reader of the old struct went straight to `dom.node(passo.no)` — with
/// an optional field the compiler lets that read stay, one `unwrap_or` away from
/// answering about the SPLIT INLINE instead of about the anonymous box. That is
/// invariant I6 of `docs/ui/html-engine/box-tree.md` failing in silence. Here
/// the node is not reachable without matching the variant that has one.
#[derive(Debug)]
pub(in crate::layout) enum FlowStep {
    /// A step that names a node AND its box: an element box or a text box.
    Node { dom_node: NodeIdx, box_id: BoxId },
    /// A DOM child that generates NO box — a comment, a whitespace-only run the
    /// split declined to wrap, an element the cascade refused — spliced back at
    /// its DOM position (see the module header for why it is here at all).
    ///
    /// Its own variant and not `Node` with an `Option<BoxId>`: the flow has one
    /// arm for it, which reproduces what the box-less child did before — it
    /// opens an inline group and nothing else — and no layout function below
    /// that arm ever receives a "maybe box". BT-2a, `box-tree.md` §7 I1.
    NoBox(NodeIdx),
    /// An ANONYMOUS block box. It has no node, and nothing in this step may be
    /// translated back into one: what it needs — its style source, its children
    /// — it asks the tree for.
    AnonymousBox(BoxId),
}

/// The children of `box_id`, in the tree's order, with the no-box DOM children
/// spliced back at their own positions.
pub(in crate::layout) fn flow_sequence(
    dom: &Dom,
    tree: &BoxTree,
    id: NodeIdx,
    box_id: BoxId,
) -> Vec<FlowStep> {
    // The box being descended into must belong to the tree this list was laid
    // out against. It is the check the generation field exists for: an id kept
    // across a rebuild indexes an arena that has moved, and reading it answers
    // the geometry of an unrelated box in silence. `BoxTree::get` panics on the
    // same comparison; asserting here names the CALLER instead of the accessor.
    //
    // A PLAIN assert and not a `debug_assert`, for the same reason `BoxTree::get`
    // is one: the failure it catches is a wrong answer in silence, and a check
    // that evaporates in the profile the merge gate runs under is not a check.
    // It is one `u32` comparison per block container.
    assert_eq!(
        box_id.generation(),
        tree.generation(),
        "a caixa {box_id:?} de {id:?} nao e desta arvore (geracao {}): a lista carrega uma arvore de outra construcao",
        tree.generation()
    );
    // WITHOUT the generated boxes (`::before`/`::after`): `pseudo_bloco` lays a
    // block-level one out around this very loop, and a generated box here would
    // be a second, anonymous-looking step for the same box — laid out twice.
    // The equality below counts what the flow owns, not what the tree holds.
    let box_ids = tree.children_without_generated(box_id);
    // The tree's own order, first and on its own. ONE STEP PER BOX, anonymous
    // included: the flow has a path for a box with no node now.
    let mut da_arvore: Vec<FlowStep> = Vec::with_capacity(box_ids.len());
    for &b in box_ids {
        debug_assert_eq!(
            tree.parent(b),
            Some(box_id),
            "a caixa {b:?} nao e filha de {box_id:?}, e a descida chegou a ela na mesma"
        );
        match tree.node_of(b) {
            Some(dom_node) => da_arvore.push(FlowStep::Node { dom_node, box_id: b }),
            None => da_arvore.push(FlowStep::AnonymousBox(b)),
        }
    }
    // Every box of this container came out the other side, one for one. It is an
    // EQUALITY and not a `>=`: while an anonymous box was expanded, the count on
    // the left counted its GRANDCHILDREN and no arithmetic related the two sides.
    debug_assert_eq!(
        da_arvore.len(),
        box_ids.len(),
        "a sequencia de {box_id:?} perdeu caixas: {} passos para {} filhas",
        da_arvore.len(),
        box_ids.len()
    );
    // The whole child list belongs to the OWNER alone — see [`splice_boxless`]
    // for why an anonymous box gets a window over one run instead.
    let window = if tree.node_of(box_id) == Some(id) {
        RunWindow::WholeContainer
    } else {
        run_window(dom, tree, id, box_ids)
    };
    splice_boxless(dom, tree, id, da_arvore, window)
}

/// Which DOM children of `id` a sequence may have spliced into it.
#[derive(Clone, Copy)]
enum RunWindow {
    /// The box IS `id`'s: every child of it that generates no box belongs.
    WholeContainer,
    /// The box holds one RUN of `id`'s children, between these two DOM
    /// positions, both excluded.
    Run(usize, usize),
    /// The run could not be located in the child list. Nothing is spliced, which
    /// is the conservative half of the two.
    Empty,
}

/// The DOM positions an anonymous box's run COVERS, as an exclusive interval.
///
/// An anonymous box encloses a contiguous run of its container's children (see
/// `boxes/build.rs`), and which children those are is not recorded anywhere — it
/// is recoverable, and only recoverable, from where the nodes under it sit in
/// the DOM child list. [`RunWindow::Empty`] is the answer when that fails: a box
/// naming a node that is not a child of `id` at all, which the table fixups of
/// BT-4 will produce. It means "splice nothing", which is the conservative half
/// of the two — a comment that was in the sequence stops being in it, never the
/// reverse.
fn run_window(dom: &Dom, tree: &BoxTree, id: NodeIdx, box_ids: &[BoxId]) -> RunWindow {
    let mut lower = usize::MAX;
    let mut upper = 0usize;
    for &b in box_ids {
        let Some(pos) = position_in_container(dom, tree, id, b) else {
            return RunWindow::Empty;
        };
        lower = lower.min(pos);
        upper = upper.max(pos);
    }
    if lower == usize::MAX {
        return RunWindow::Empty;
    }
    RunWindow::Run(lower, upper)
}

/// Where the content under `b` sits among the DOM children of `id`.
///
/// A box that names a node answers directly. An ANONYMOUS box answers with the
/// FIRST position any box below it reaches, and that recursion is not decoration:
/// without it an anonymous step has no position, and `splice_boxless` then
/// lets every pending comment slide past it to the end of the container — where
/// it stops breaking the margin collapse it used to break. That is an answer
/// moving for a reason nothing in this lot measured.
fn position_in_container(dom: &Dom, tree: &BoxTree, id: NodeIdx, b: BoxId) -> Option<usize> {
    let child_list = &dom.node(id).children;
    if let Some(dom_node) = tree.node_of(b) {
        return child_list.iter().position(|&d| d == dom_node);
    }
    tree.children_without_generated(b)
        .iter()
        .filter_map(|&grandchild| position_in_container(dom, tree, id, grandchild))
        .min()
}

/// Splices the DOM children that generated NO box back into a sequence that came
/// from the tree, each at its own DOM position.
///
/// Today that is comments and the whitespace-only runs the split declined to
/// wrap, and the reason they are here rather than dropped is the module
/// header's.
///
/// The merge is by DOM POSITION and the tree still decides the order among the
/// boxes: a no-box child is emitted before the first step whose content sits
/// after it in the DOM. A box whose node is not a DOM child of `id` at all has
/// no position, sorts last, and therefore never drags a comment past anything.
///
/// **`window` is what an ANONYMOUS box gets instead of the whole child list.**
/// The children of `id` are not the children of one anonymous box: the box holds
/// ONE RUN of them. Without the window, every comment anywhere in the container
/// would be spliced into every anonymous box the split produced — the same
/// comment several times over, each one closing a run of inline-blocks it is
/// nowhere near. The window is the DOM span the run covers, and it is EXCLUSIVE
/// at both ends on purpose: a comment at the edge of a run sits next to the
/// block-level child that ended it, where the flush it would have caused happens
/// anyway.
fn splice_boxless(
    dom: &Dom,
    tree: &BoxTree,
    id: NodeIdx,
    da_arvore: Vec<FlowStep>,
    window: RunWindow,
) -> Vec<FlowStep> {
    let dom_children = &dom.node(id).children;
    let inside = |pos: usize| match window {
        RunWindow::WholeContainer => true,
        RunWindow::Run(lower, upper) => pos > lower && pos < upper,
        RunWindow::Empty => false,
    };
    // **An ELEMENT with no box is not one of those.** The cascade accepted it,
    // so the build gave it a box unless the split consumed it: an inline whose
    // block-level child left no inline content on either side materialises no
    // fragment at all, and its content is already in the tree as a sibling of
    // this sequence. Splicing it back made the flow lay the `<span>` out a
    // second time as an inline atom with no box, and the block inside it then
    // reached the fragment cache without an identity
    // (`CSS2/normal-flow/height-inherit-001.xht`).
    let absorbed = |d: NodeIdx| {
        matches!(dom.node(d).kind, NodeKind::Element { .. }) && dom.computed_style_idx(d).is_some()
    };
    let boxless: Vec<(usize, NodeIdx)> = dom_children
        .iter()
        .copied()
        .enumerate()
        .filter(|&(pos, d)| tree.boxes_of(d).is_empty() && !absorbed(d) && inside(pos))
        .collect();
    if boxless.is_empty() {
        return da_arvore;
    }
    let mut out = Vec::with_capacity(da_arvore.len() + boxless.len());
    let mut s = 0usize;
    for f in da_arvore {
        let index = match &f {
            FlowStep::Node { dom_node, .. } | FlowStep::NoBox(dom_node) => dom_children.iter().position(|d| d == dom_node),
            FlowStep::AnonymousBox(b) => position_in_container(dom, tree, id, *b),
        };
        while s < boxless.len() && Some(boxless[s].0) < index {
            out.push(FlowStep::NoBox(boxless[s].1));
            s += 1;
        }
        out.push(f);
    }
    for &(_, d) in &boxless[s..] {
        out.push(FlowStep::NoBox(d));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boxes::BoxKind;

    /// Finds the first element with a given tag.
    fn node_of_tag(dom: &Dom, wanted: &str) -> NodeIdx {
        (0..dom.node_count())
            .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == wanted))
            .unwrap_or_else(|| panic!("a fixture nao tem <{wanted}>"))
    }

    /// O nó de um passo, ou `None` quando o passo é uma caixa anónima — que é a
    /// resposta certa e não uma falha de conversão: essa caixa não tem nó.
    /// A caixa que um passo traz, anónima ou não. `None` só para um filho do DOM
    /// que não gera caixa nenhuma.
    fn box_of_step(p: &FlowStep) -> Option<BoxId> {
        match *p {
            FlowStep::Node { box_id, .. } | FlowStep::AnonymousBox(box_id) => Some(box_id),
            FlowStep::NoBox(_) => None,
        }
    }

    fn node_of_step(p: &FlowStep) -> Option<NodeIdx> {
        match *p {
            FlowStep::Node { dom_node, .. } | FlowStep::NoBox(dom_node) => Some(dom_node),
            FlowStep::AnonymousBox(_) => None,
        }
    }

    /// **O caso por que a camada existe, visto de quem DESCE.** As caixas
    /// anónimas e o `<div>` são filhos do `<section>`, e não do `<span>`: é a
    /// sequência do CONTENTOR que muda, porque é para ele que a partição sobe.
    #[test]
    fn a_particao_aparece_na_sequencia_do_contentor_e_nao_na_do_inline() {
        let dom = crate::parse_html_to_dom("<section><span>texto<div>bloco</div>texto</span></section>");
        let tree = dom.box_tree();
        let p = node_of_tag(&dom, "section");
        let div = node_of_tag(&dom, "div");

        let seq = flow_sequence(&dom, &tree, p, tree.boxes_of(p)[0]);
        assert_eq!(seq.len(), 3, "anonima, o bloco, anonima: {seq:?}");
        assert!(
            matches!(seq[0], FlowStep::AnonymousBox(_)),
            "a corrida da frente e uma caixa anonima do <section>, nao o texto la dentro"
        );
        assert_eq!(
            node_of_step(&seq[1]),
            Some(div),
            "o bloco e IRMAO das anonimas, na ordem da arvore"
        );
        assert!(matches!(seq[2], FlowStep::AnonymousBox(_)));
        assert!(seq.iter().all(|p| box_of_step(p).is_some()));
    }

    /// Dentro da caixa anónima está o FRAGMENTO do `<span>` — um caixa de
    /// ELEMENTO e não anónima, porque o CSS 2.1 §9.2.1.1 diz que cada metade
    /// guarda a borda e o fundo do elemento, e uma caixa anónima não tem
    /// declarações para os carregar.
    #[test]
    fn dentro_da_anonima_esta_um_fragmento_do_inline_com_o_estilo_dele() {
        let dom = crate::parse_html_to_dom(
            "<section><span style='background:red'>a<div>b</div>c</span></section>",
        );
        let tree = dom.box_tree();
        let p = node_of_tag(&dom, "section");
        let span = node_of_tag(&dom, "span");

        assert_eq!(
            tree.boxes_of(span).len(),
            2,
            "o inline partido tem DUAS caixas suas, uma por corrida"
        );
        let seq = flow_sequence(&dom, &tree, p, tree.boxes_of(p)[0]);
        let FlowStep::AnonymousBox(anon) = seq[0] else {
            panic!("o primeiro passo devia ser anonimo: {seq:?}");
        };
        assert!(matches!(tree.kind(anon), BoxKind::Anonymous { .. }));
        // A anónima herda do CONTENTOR, não do inline: §9.2.1.1 manda herdar da
        // caixa não-anónima que a envolve.
        assert_eq!(tree.style_source(anon), p);

        let inside = flow_sequence(&dom, &tree, p, anon);
        assert_eq!(inside.len(), 1, "a corrida da frente e o fragmento do span");
        assert_eq!(node_of_step(&inside[0]), Some(span));
        assert_eq!(
            box_of_step(&inside[0]),
            Some(tree.boxes_of(span)[0]),
            "e e o PRIMEIRO fragmento, nao o segundo"
        );
    }

    /// Um irmão inline do inline partido entra na MESMA caixa anónima. Sem isto
    /// o `x` e o `a` ficavam em linhas diferentes, que é o que nenhum browser
    /// faz.
    #[test]
    fn um_irmao_inline_entra_na_mesma_anonima_que_o_fragmento() {
        let dom = crate::parse_html_to_dom("<section>x<span>a<div>b</div>c</span>y</section>");
        let tree = dom.box_tree();
        let p = node_of_tag(&dom, "section");

        let seq = flow_sequence(&dom, &tree, p, tree.boxes_of(p)[0]);
        assert_eq!(seq.len(), 3, "anonima, bloco, anonima: {seq:?}");
        let FlowStep::AnonymousBox(ahead) = seq[0] else {
            panic!("{seq:?}");
        };
        let inside = flow_sequence(&dom, &tree, p, ahead);
        assert_eq!(inside.len(), 2, "o texto 'x' E o fragmento do span");
        assert!(
            matches!(&dom.node(node_of_step(&inside[0]).unwrap()).kind, NodeKind::Text(t) if t == "x")
        );
    }

    /// A nó de TEXTO entra na sequência COM a sua caixa.
    #[test]
    fn um_no_de_texto_entra_na_sequencia_com_a_sua_caixa() {
        let dom = crate::parse_html_to_dom("<div>ola</div>");
        let tree = dom.box_tree();
        let div = node_of_tag(&dom, "div");
        let seq = flow_sequence(&dom, &tree, div, tree.boxes_of(div)[0]);

        assert_eq!(seq.len(), 1);
        assert!(box_of_step(&seq[0]).is_some(), "o texto tem caixa desde que a arvore a da");
        let dom_node = node_of_step(&seq[0]).expect("um no de texto tem no");
        assert!(matches!(&dom.node(dom_node).kind, NodeKind::Text(_)));
    }

    /// Um COMENTÁRIO não gera caixa e continua na sequência, na posição do DOM.
    #[test]
    fn um_comentario_nao_tem_caixa_e_fica_na_sequencia_onde_o_dom_o_poe() {
        let dom = crate::parse_html_to_dom("<div><p>a</p><!--c--><p>b</p></div>");
        let tree = dom.box_tree();
        let div = node_of_tag(&dom, "div");
        let seq = flow_sequence(&dom, &tree, div, tree.boxes_of(div)[0]);

        assert_eq!(seq.len(), 3, "dois <p> e o comentario entre eles");
        assert!(box_of_step(&seq[1]).is_none(), "um comentario nao gera caixa");
        let dom_node = node_of_step(&seq[1]).expect("um comentario e um no, so nao tem caixa");
        assert!(matches!(&dom.node(dom_node).kind, NodeKind::Comment(_)));
    }

    /// **Um comentário não é emendado em TODAS as corridas do contentor.** Sem
    /// a janela, o `<!--c-->` daqui aparecia na corrida da frente E na de trás,
    /// cada uma a fechar uma corrida de inline-blocks onde não está.
    #[test]
    fn a_emenda_de_um_comentario_nao_se_repete_por_corrida() {
        let dom = crate::parse_html_to_dom("<div>a<!--c-->b<span>s<p>x</p>f</span></div>");
        let tree = dom.box_tree();
        let div = node_of_tag(&dom, "div");
        let seq = flow_sequence(&dom, &tree, div, tree.boxes_of(div)[0]);
        let anonymous_boxes: Vec<BoxId> = seq
            .iter()
            .filter_map(|p| match *p {
                FlowStep::AnonymousBox(b) => Some(b),
                _ => None,
            })
            .collect();
        assert_eq!(anonymous_boxes.len(), 2, "uma corrida de cada lado do <p>: {seq:?}");

        let comments = |b: BoxId| {
            flow_sequence(&dom, &tree, div, b)
                .iter()
                .filter(|p| {
                    node_of_step(p).is_some_and(|n| matches!(&dom.node(n).kind, NodeKind::Comment(_)))
                })
                .count()
        };
        assert_eq!(comments(anonymous_boxes[0]), 1, "o comentario esta nesta corrida");
        assert_eq!(comments(anonymous_boxes[1]), 0, "e nao na outra");
    }

    /// Uma `BoxId` de OUTRA construção da árvore é recusada.
    #[test]
    #[should_panic(expected = "nao e desta arvore")]
    fn uma_caixa_de_outra_arvore_e_recusada() {
        let dom = crate::parse_html_to_dom("<div><p>a</p></div>");
        let old = dom.box_tree();
        let div = node_of_tag(&dom, "div");
        let box_id = old.boxes_of(div)[0];
        let fresh = crate::boxes::build_mirror(&dom);

        let _ = flow_sequence(&dom, &fresh, div, box_id);
    }
}
