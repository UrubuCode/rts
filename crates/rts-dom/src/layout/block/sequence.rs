//! WHERE THE CHILD SEQUENCE OF A BLOCK FLOW COMES FROM — the box tree.
//!
//! Until this module existed, `layout_children_vertical` walked
//! `dom.node(id).children` and asked the tree only "which box is this child's".
//! The tree entered as IDENTITY and the DOM kept the ORDER, so a box the DOM
//! does not have — an anonymous box, the whole point of the layer — could not be
//! reached by anyone descending, however well the build had constructed it.
//!
//! **The sequence is `tree.children(caixa)`, in the tree's order.** A TEXT box
//! is in it with its box, and an ANONYMOUS box is a STEP OF ITS OWN —
//! [`PassoDoFluxo::Anonima`] — which the flow lays out through
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
//! [`sequencia_do_fluxo`] asserts what remains checkable and is just as silent
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
pub(in crate::layout) enum PassoDoFluxo {
    /// A step that names a node AND its box: an element box or a text box.
    No { no: NodeIdx, caixa: BoxId },
    /// A DOM child that generates NO box — a comment, a whitespace-only run the
    /// split declined to wrap, an element the cascade refused — spliced back at
    /// its DOM position (see the module header for why it is here at all).
    ///
    /// Its own variant and not `No` with an `Option<BoxId>`: the flow has one
    /// arm for it, which reproduces what the box-less child did before — it
    /// opens an inline group and nothing else — and no layout function below
    /// that arm ever receives a "maybe box". BT-2a, `box-tree.md` §7 I1.
    NoBox(NodeIdx),
    /// An ANONYMOUS block box. It has no node, and nothing in this step may be
    /// translated back into one: what it needs — its style source, its children
    /// — it asks the tree for.
    Anonima(BoxId),
}

/// The children of `caixa`, in the tree's order, with the no-box DOM children
/// spliced back at their own positions.
pub(in crate::layout) fn sequencia_do_fluxo(
    dom: &Dom,
    tree: &BoxTree,
    id: NodeIdx,
    caixa: BoxId,
) -> Vec<PassoDoFluxo> {
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
        caixa.generation(),
        tree.generation(),
        "a caixa {caixa:?} de {id:?} nao e desta arvore (geracao {}): a lista carrega uma arvore de outra construcao",
        tree.generation()
    );
    // WITHOUT the generated boxes (`::before`/`::after`): `pseudo_bloco` lays a
    // block-level one out around this very loop, and a generated box here would
    // be a second, anonymous-looking step for the same box — laid out twice.
    // The equality below counts what the flow owns, not what the tree holds.
    let caixas = tree.children_without_generated(caixa);
    // The tree's own order, first and on its own. ONE STEP PER BOX, anonymous
    // included: the flow has a path for a box with no node now.
    let mut da_arvore: Vec<PassoDoFluxo> = Vec::with_capacity(caixas.len());
    for &b in caixas {
        debug_assert_eq!(
            tree.parent(b),
            Some(caixa),
            "a caixa {b:?} nao e filha de {caixa:?}, e a descida chegou a ela na mesma"
        );
        match tree.node_of(b) {
            Some(no) => da_arvore.push(PassoDoFluxo::No { no, caixa: b }),
            None => da_arvore.push(PassoDoFluxo::Anonima(b)),
        }
    }
    // Every box of this container came out the other side, one for one. It is an
    // EQUALITY and not a `>=`: while an anonymous box was expanded, the count on
    // the left counted its GRANDCHILDREN and no arithmetic related the two sides.
    debug_assert_eq!(
        da_arvore.len(),
        caixas.len(),
        "a sequencia de {caixa:?} perdeu caixas: {} passos para {} filhas",
        da_arvore.len(),
        caixas.len()
    );
    // The whole child list belongs to the OWNER alone — see [`emenda_os_sem_caixa`]
    // for why an anonymous box gets a window over one run instead.
    let janela = if tree.node_of(caixa) == Some(id) {
        Janela::TodoOContentor
    } else {
        janela_do_run(dom, tree, id, caixas)
    };
    emenda_os_sem_caixa(dom, tree, id, da_arvore, janela)
}

/// Which DOM children of `id` a sequence may have spliced into it.
#[derive(Clone, Copy)]
enum Janela {
    /// The box IS `id`'s: every child of it that generates no box belongs.
    TodoOContentor,
    /// The box holds one RUN of `id`'s children, between these two DOM
    /// positions, both excluded.
    Run(usize, usize),
    /// The run could not be located in the child list. Nothing is spliced, which
    /// is the conservative half of the two.
    Nenhuma,
}

/// The DOM positions an anonymous box's run COVERS, as an exclusive interval.
///
/// An anonymous box encloses a contiguous run of its container's children (see
/// `boxes/build.rs`), and which children those are is not recorded anywhere — it
/// is recoverable, and only recoverable, from where the nodes under it sit in
/// the DOM child list. [`Janela::Nenhuma`] is the answer when that fails: a box
/// naming a node that is not a child of `id` at all, which the table fixups of
/// BT-4 will produce. It means "splice nothing", which is the conservative half
/// of the two — a comment that was in the sequence stops being in it, never the
/// reverse.
fn janela_do_run(dom: &Dom, tree: &BoxTree, id: NodeIdx, caixas: &[BoxId]) -> Janela {
    let mut menor = usize::MAX;
    let mut maior = 0usize;
    for &b in caixas {
        let Some(pos) = posicao_no_contentor(dom, tree, id, b) else {
            return Janela::Nenhuma;
        };
        menor = menor.min(pos);
        maior = maior.max(pos);
    }
    if menor == usize::MAX {
        return Janela::Nenhuma;
    }
    Janela::Run(menor, maior)
}

/// Where the content under `b` sits among the DOM children of `id`.
///
/// A box that names a node answers directly. An ANONYMOUS box answers with the
/// FIRST position any box below it reaches, and that recursion is not decoration:
/// without it an anonymous step has no position, and `emenda_os_sem_caixa` then
/// lets every pending comment slide past it to the end of the container — where
/// it stops breaking the margin collapse it used to break. That is an answer
/// moving for a reason nothing in this lot measured.
fn posicao_no_contentor(dom: &Dom, tree: &BoxTree, id: NodeIdx, b: BoxId) -> Option<usize> {
    let filhos = &dom.node(id).children;
    if let Some(no) = tree.node_of(b) {
        return filhos.iter().position(|&d| d == no);
    }
    tree.children_without_generated(b)
        .iter()
        .filter_map(|&neto| posicao_no_contentor(dom, tree, id, neto))
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
/// **`janela` is what an ANONYMOUS box gets instead of the whole child list.**
/// The children of `id` are not the children of one anonymous box: the box holds
/// ONE RUN of them. Without the window, every comment anywhere in the container
/// would be spliced into every anonymous box the split produced — the same
/// comment several times over, each one closing a run of inline-blocks it is
/// nowhere near. The window is the DOM span the run covers, and it is EXCLUSIVE
/// at both ends on purpose: a comment at the edge of a run sits next to the
/// block-level child that ended it, where the flush it would have caused happens
/// anyway.
fn emenda_os_sem_caixa(
    dom: &Dom,
    tree: &BoxTree,
    id: NodeIdx,
    da_arvore: Vec<PassoDoFluxo>,
    janela: Janela,
) -> Vec<PassoDoFluxo> {
    let filhos_dom = &dom.node(id).children;
    let dentro = |pos: usize| match janela {
        Janela::TodoOContentor => true,
        Janela::Run(menor, maior) => pos > menor && pos < maior,
        Janela::Nenhuma => false,
    };
    // **An ELEMENT with no box is not one of those.** The cascade accepted it,
    // so the build gave it a box unless the split consumed it: an inline whose
    // block-level child left no inline content on either side materialises no
    // fragment at all, and its content is already in the tree as a sibling of
    // this sequence. Splicing it back made the flow lay the `<span>` out a
    // second time as an inline atom with no box, and the block inside it then
    // reached the fragment cache without an identity
    // (`CSS2/normal-flow/height-inherit-001.xht`).
    let absorvido = |d: NodeIdx| {
        matches!(dom.node(d).kind, NodeKind::Element { .. }) && dom.computed_style_idx(d).is_some()
    };
    let sem_caixa: Vec<(usize, NodeIdx)> = filhos_dom
        .iter()
        .copied()
        .enumerate()
        .filter(|&(pos, d)| tree.boxes_of(d).is_empty() && !absorvido(d) && dentro(pos))
        .collect();
    if sem_caixa.is_empty() {
        return da_arvore;
    }
    let mut out = Vec::with_capacity(da_arvore.len() + sem_caixa.len());
    let mut s = 0usize;
    for f in da_arvore {
        let posicao = match &f {
            PassoDoFluxo::No { no, .. } | PassoDoFluxo::NoBox(no) => filhos_dom.iter().position(|d| d == no),
            PassoDoFluxo::Anonima(b) => posicao_no_contentor(dom, tree, id, *b),
        };
        while s < sem_caixa.len() && Some(sem_caixa[s].0) < posicao {
            out.push(PassoDoFluxo::NoBox(sem_caixa[s].1));
            s += 1;
        }
        out.push(f);
    }
    for &(_, d) in &sem_caixa[s..] {
        out.push(PassoDoFluxo::NoBox(d));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boxes::BoxKind;

    /// Finds the first element with a given tag.
    fn no_da_tag(dom: &Dom, procurada: &str) -> NodeIdx {
        (0..dom.node_count())
            .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == procurada))
            .unwrap_or_else(|| panic!("a fixture nao tem <{procurada}>"))
    }

    /// O nó de um passo, ou `None` quando o passo é uma caixa anónima — que é a
    /// resposta certa e não uma falha de conversão: essa caixa não tem nó.
    /// A caixa que um passo traz, anónima ou não. `None` só para um filho do DOM
    /// que não gera caixa nenhuma.
    fn caixa_do_passo(p: &PassoDoFluxo) -> Option<BoxId> {
        match *p {
            PassoDoFluxo::No { caixa, .. } | PassoDoFluxo::Anonima(caixa) => Some(caixa),
            PassoDoFluxo::NoBox(_) => None,
        }
    }

    fn no_do_passo(p: &PassoDoFluxo) -> Option<NodeIdx> {
        match *p {
            PassoDoFluxo::No { no, .. } | PassoDoFluxo::NoBox(no) => Some(no),
            PassoDoFluxo::Anonima(_) => None,
        }
    }

    /// **O caso por que a camada existe, visto de quem DESCE.** As caixas
    /// anónimas e o `<div>` são filhos do `<section>`, e não do `<span>`: é a
    /// sequência do CONTENTOR que muda, porque é para ele que a partição sobe.
    #[test]
    fn a_particao_aparece_na_sequencia_do_contentor_e_nao_na_do_inline() {
        let dom = crate::parse_html_to_dom("<section><span>texto<div>bloco</div>texto</span></section>");
        let tree = dom.box_tree();
        let p = no_da_tag(&dom, "section");
        let div = no_da_tag(&dom, "div");

        let seq = sequencia_do_fluxo(&dom, &tree, p, tree.boxes_of(p)[0]);
        assert_eq!(seq.len(), 3, "anonima, o bloco, anonima: {seq:?}");
        assert!(
            matches!(seq[0], PassoDoFluxo::Anonima(_)),
            "a corrida da frente e uma caixa anonima do <section>, nao o texto la dentro"
        );
        assert_eq!(
            no_do_passo(&seq[1]),
            Some(div),
            "o bloco e IRMAO das anonimas, na ordem da arvore"
        );
        assert!(matches!(seq[2], PassoDoFluxo::Anonima(_)));
        assert!(seq.iter().all(|p| caixa_do_passo(p).is_some()));
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
        let p = no_da_tag(&dom, "section");
        let span = no_da_tag(&dom, "span");

        assert_eq!(
            tree.boxes_of(span).len(),
            2,
            "o inline partido tem DUAS caixas suas, uma por corrida"
        );
        let seq = sequencia_do_fluxo(&dom, &tree, p, tree.boxes_of(p)[0]);
        let PassoDoFluxo::Anonima(anon) = seq[0] else {
            panic!("o primeiro passo devia ser anonimo: {seq:?}");
        };
        assert!(matches!(tree.kind(anon), BoxKind::Anonymous { .. }));
        // A anónima herda do CONTENTOR, não do inline: §9.2.1.1 manda herdar da
        // caixa não-anónima que a envolve.
        assert_eq!(tree.style_source(anon), p);

        let dentro = sequencia_do_fluxo(&dom, &tree, p, anon);
        assert_eq!(dentro.len(), 1, "a corrida da frente e o fragmento do span");
        assert_eq!(no_do_passo(&dentro[0]), Some(span));
        assert_eq!(
            caixa_do_passo(&dentro[0]),
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
        let p = no_da_tag(&dom, "section");

        let seq = sequencia_do_fluxo(&dom, &tree, p, tree.boxes_of(p)[0]);
        assert_eq!(seq.len(), 3, "anonima, bloco, anonima: {seq:?}");
        let PassoDoFluxo::Anonima(frente) = seq[0] else {
            panic!("{seq:?}");
        };
        let dentro = sequencia_do_fluxo(&dom, &tree, p, frente);
        assert_eq!(dentro.len(), 2, "o texto 'x' E o fragmento do span");
        assert!(
            matches!(&dom.node(no_do_passo(&dentro[0]).unwrap()).kind, NodeKind::Text(t) if t == "x")
        );
    }

    /// A nó de TEXTO entra na sequência COM a sua caixa.
    #[test]
    fn um_no_de_texto_entra_na_sequencia_com_a_sua_caixa() {
        let dom = crate::parse_html_to_dom("<div>ola</div>");
        let tree = dom.box_tree();
        let div = no_da_tag(&dom, "div");
        let seq = sequencia_do_fluxo(&dom, &tree, div, tree.boxes_of(div)[0]);

        assert_eq!(seq.len(), 1);
        assert!(caixa_do_passo(&seq[0]).is_some(), "o texto tem caixa desde que a arvore a da");
        let no = no_do_passo(&seq[0]).expect("um no de texto tem no");
        assert!(matches!(&dom.node(no).kind, NodeKind::Text(_)));
    }

    /// Um COMENTÁRIO não gera caixa e continua na sequência, na posição do DOM.
    #[test]
    fn um_comentario_nao_tem_caixa_e_fica_na_sequencia_onde_o_dom_o_poe() {
        let dom = crate::parse_html_to_dom("<div><p>a</p><!--c--><p>b</p></div>");
        let tree = dom.box_tree();
        let div = no_da_tag(&dom, "div");
        let seq = sequencia_do_fluxo(&dom, &tree, div, tree.boxes_of(div)[0]);

        assert_eq!(seq.len(), 3, "dois <p> e o comentario entre eles");
        assert!(caixa_do_passo(&seq[1]).is_none(), "um comentario nao gera caixa");
        let no = no_do_passo(&seq[1]).expect("um comentario e um no, so nao tem caixa");
        assert!(matches!(&dom.node(no).kind, NodeKind::Comment(_)));
    }

    /// **Um comentário não é emendado em TODAS as corridas do contentor.** Sem
    /// a janela, o `<!--c-->` daqui aparecia na corrida da frente E na de trás,
    /// cada uma a fechar uma corrida de inline-blocks onde não está.
    #[test]
    fn a_emenda_de_um_comentario_nao_se_repete_por_corrida() {
        let dom = crate::parse_html_to_dom("<div>a<!--c-->b<span>s<p>x</p>f</span></div>");
        let tree = dom.box_tree();
        let div = no_da_tag(&dom, "div");
        let seq = sequencia_do_fluxo(&dom, &tree, div, tree.boxes_of(div)[0]);
        let anonimas: Vec<BoxId> = seq
            .iter()
            .filter_map(|p| match *p {
                PassoDoFluxo::Anonima(b) => Some(b),
                _ => None,
            })
            .collect();
        assert_eq!(anonimas.len(), 2, "uma corrida de cada lado do <p>: {seq:?}");

        let comentarios = |b: BoxId| {
            sequencia_do_fluxo(&dom, &tree, div, b)
                .iter()
                .filter(|p| {
                    no_do_passo(p).is_some_and(|n| matches!(&dom.node(n).kind, NodeKind::Comment(_)))
                })
                .count()
        };
        assert_eq!(comentarios(anonimas[0]), 1, "o comentario esta nesta corrida");
        assert_eq!(comentarios(anonimas[1]), 0, "e nao na outra");
    }

    /// Uma `BoxId` de OUTRA construção da árvore é recusada.
    #[test]
    #[should_panic(expected = "nao e desta arvore")]
    fn uma_caixa_de_outra_arvore_e_recusada() {
        let dom = crate::parse_html_to_dom("<div><p>a</p></div>");
        let antiga = dom.box_tree();
        let div = no_da_tag(&dom, "div");
        let caixa = antiga.boxes_of(div)[0];
        let nova = crate::boxes::build_mirror(&dom);

        let _ = sequencia_do_fluxo(&dom, &nova, div, caixa);
    }
}
