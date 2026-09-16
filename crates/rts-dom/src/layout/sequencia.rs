//! WHERE THE CHILD SEQUENCE OF A BLOCK FLOW COMES FROM — the box tree.
//!
//! Until this module existed, `layout_children_vertical` walked
//! `dom.node(id).children` and asked the tree only "which box is this child's".
//! The tree entered as IDENTITY and the DOM kept the ORDER, which is the line
//! §10 of `docs/ui/html-engine/box-tree.md` names as the first thing the base
//! does not do. While it held, a box the DOM does not have — an anonymous box,
//! the whole point of the layer — could not be reached by anyone descending,
//! however well the build had constructed it.
//!
//! **Now the sequence is `tree.children(caixa)`, in the tree's order.** A TEXT
//! box is in it with its box, and an ANONYMOUS box is entered rather than walked
//! past — see [`expande_anonima`] for what "entered" means in this lot and what
//! it does not yet mean.
//!
//! **The cursor this replaced STOPPED at the first anonymous box.** It matched a
//! box to a DOM child by `node_of`, an anonymous box answers `None`, so no child
//! matched it and the cursor never advanced again: every remaining child of that
//! container came out with no box, and with it no style — a `<div>` inside a
//! `<span style="background:red">` lost its margin, its `float` and its `clear`.
//! That shape is not hypothetical; it is exactly the one the split produces, and
//! `caixa::is_block_level` routes such a `<span>` through `layout_block`.
//!
//! ## The one thing that still comes from the DOM, and why
//!
//! A node that generates NO box — today only a comment — is spliced back in at
//! its DOM position, with `caixa: None`.
//!
//! That is not tidiness; it is the lot's ruler. A comment reaching
//! `layout_children_vertical` today falls into the generic inline arm, and two
//! things happen there that are not nothing: it FLUSHES the run of consecutive
//! inline-blocks, and it opens an inline group whose flush resets `borda` and
//! `strut` — so a comment between two block siblings breaks their margin
//! collapse. Dropping comments from the sequence would therefore CHANGE answers,
//! in the direction that looks like a fix. BT-1's ruler is **zero lost and zero
//! gained**, and a gain here is a thing to explain, not to bank. Whether a
//! comment should stop a margin collapse is its own lot, measured on its own.
//!
//! ## What replaced the `debug_assert!` of the mirror
//!
//! The old one compared two sequences — the DOM's children against the boxes
//! the follower handed out — and it earned its keep: it fired in 322 tests at
//! once when text gained a box. With the order coming from the tree there are
//! no longer two sequences to compare, so the check it made is not expressible.
//!
//! [`sequencia_do_fluxo`] asserts what remains checkable and is just as silent
//! when it breaks: that the box being descended into belongs to the tree the
//! `DisplayList` carries (the GENERATION), that every box emitted is a child of
//! that box in that tree, and that no box of the tree is dropped on the way.
//! A stale `BoxId` is the failure this catches, and it is the one the box
//! generation exists for — see `boxes/mod.rs`.

use super::*;
use crate::boxes::{BoxId, BoxTree};

/// One step of the block flow's descent over the box tree.
///
/// `caixa` is `None` for a DOM child that generates no box at all — a comment —
/// and for every child when the list carries no tree. The flow asks
/// `caixa.is_some()` where it used to ask "is this an element".
pub(in crate::layout) struct FilhoDoFluxo {
    pub no: NodeIdx,
    pub caixa: Option<BoxId>,
}

/// The children of `caixa`, in the tree's order, with the no-box DOM children
/// spliced back at their own positions.
///
/// `caixa` is `None` when the list carries no tree (`DisplayList::default()`,
/// five call sites) — the sequence is then the DOM's children with no box each,
/// which is exactly what the loop did before this module existed.
pub(in crate::layout) fn sequencia_do_fluxo(
    dom: &Dom,
    tree: &BoxTree,
    id: NodeIdx,
    caixa: Option<BoxId>,
) -> Vec<FilhoDoFluxo> {
    let filhos_dom = &dom.node(id).children;
    let Some(caixa) = caixa else {
        return filhos_dom
            .iter()
            .map(|&no| FilhoDoFluxo { no, caixa: None })
            .collect();
    };
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
    let caixas = tree.children(caixa);
    // The tree's own order, first and on its own. An ANONYMOUS box is EXPANDED
    // into its children here and does not itself become a step of the flow —
    // see [`expande_anonima`].
    let mut da_arvore: Vec<FilhoDoFluxo> = Vec::with_capacity(caixas.len());
    for &b in caixas {
        debug_assert_eq!(
            tree.parent(b),
            Some(caixa),
            "a caixa {b:?} nao e filha de {caixa:?}, e a descida chegou a ela na mesma"
        );
        match tree.node_of(b) {
            Some(no) => da_arvore.push(FilhoDoFluxo { no, caixa: Some(b) }),
            None => expande_anonima(tree, b, &mut da_arvore),
        }
    }
    // Every box that names a node came out the other side. This is the half of
    // the mirror's `debug_assert!` that survives the order moving: there is no
    // second sequence to compare against any more, but there is still a tree
    // whose boxes all have to be visited.
    //
    // `>=` and over the boxes that NAME a node, not over all of them: an
    // anonymous box contributes its children, and an anonymous box can be EMPTY
    // — `<span><!--c--><div>x</div></span>` builds one around a run whose only
    // member is a comment, which generates nothing.
    debug_assert!(
        da_arvore.len() >= caixas.iter().filter(|&&b| tree.node_of(b).is_some()).count(),
        "a sequencia de {caixa:?} perdeu caixas: {} passos para {} filhas",
        da_arvore.len(),
        caixas.len()
    );
    emenda_os_sem_caixa(dom, tree, id, da_arvore)
}

/// Pours an anonymous box's own children into the sequence, recursively.
///
/// **An anonymous box really does reach this module**, which the first version
/// of it denied. A `<span style="background:red">` with a `<div>` inside is
/// split by `boxes::build` — inline-level, flow, not independent — and is ALSO
/// routed to `layout_block` by `caixa::is_block_level`, which answers "does this
/// go through `layout_block`" and says yes for an inline with a box to paint.
///
/// **Expanding is what keeps this lot's ruler.** The flow has no path that lays
/// an anonymous box out as the block box it is, and the one thing it could do
/// instead — pour the content straight into the inline group — would skip the
/// per-node dispatch of `layout_children_vertical`: collapsing whitespace
/// between two blocks would stop being skipped and would open a line, resetting
/// the margin collapse. Expansion hands each child to the same arm that received
/// it before the tree existed, so no answer moves.
///
/// **Laying the anonymous box out as the block it is — the 148 CSS2 reftests of
/// §9.2.1.1 — is the next lot, and this is the function it replaces.**
///
/// Recursive because nothing forbids an anonymous box inside another once the
/// table fixups land (BT-4): a non-recursive version would drop that subtree in
/// silence, which is the class this layer was built to stop.
fn expande_anonima(tree: &BoxTree, anonima: BoxId, seq: &mut Vec<FilhoDoFluxo>) {
    for &neto in tree.children(anonima) {
        match tree.node_of(neto) {
            Some(no) => seq.push(FilhoDoFluxo {
                no,
                caixa: Some(neto),
            }),
            None => expande_anonima(tree, neto, seq),
        }
    }
}

/// Splices the DOM children that generated NO box back into a sequence that
/// came from the tree, each at its own DOM position.
///
/// Today that is comments and nothing else, and the reason they are here rather
/// than dropped is the module header's: a comment currently closes the run of
/// inline-blocks and opens an inline group, which resets the margin collapse
/// between two block siblings. Dropping them would move answers, and this lot's
/// ruler is zero lost AND zero gained.
///
/// The merge is by DOM POSITION and the tree still decides the order among the
/// boxes: a no-box child is emitted before the first box whose node sits after
/// it in the DOM. A box whose node is not a DOM child of `id` at all — which a
/// future fixup may well produce — has no position, sorts last, and therefore
/// never drags a comment past anything.
fn emenda_os_sem_caixa(
    dom: &Dom,
    tree: &BoxTree,
    id: NodeIdx,
    da_arvore: Vec<FilhoDoFluxo>,
) -> Vec<FilhoDoFluxo> {
    let filhos_dom = &dom.node(id).children;
    let sem_caixa: Vec<(usize, NodeIdx)> = filhos_dom
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, d)| tree.boxes_of(d).is_empty())
        .collect();
    if sem_caixa.is_empty() {
        return da_arvore;
    }
    let mut out = Vec::with_capacity(da_arvore.len() + sem_caixa.len());
    let mut s = 0usize;
    for f in da_arvore {
        let posicao = filhos_dom.iter().position(|&d| d == f.no);
        while s < sem_caixa.len() && Some(sem_caixa[s].0) < posicao {
            out.push(FilhoDoFluxo {
                no: sem_caixa[s].1,
                caixa: None,
            });
            s += 1;
        }
        out.push(f);
    }
    for &(_, d) in &sem_caixa[s..] {
        out.push(FilhoDoFluxo { no: d, caixa: None });
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

    /// **O caso por que a camada existe, visto de quem DESCE e não de quem
    /// constrói.** `<span>texto<div>bloco</div>texto</span>`: a árvore parte o
    /// inline numa anónima, no `<div>` e noutra anónima, e o `<div>` é IRMÃO
    /// das anónimas em vez de descendente do inline.
    ///
    /// **É o teste que não passa enquanto a ordem vier do DOM.** Com o cursor
    /// que este módulo substituiu, `tree.children(span)[0]` é uma caixa
    /// ANÓNIMA, `node_of` responde `None`, nenhum filho do DOM casa com ela e o
    /// cursor PARA — o `<div>` e os dois textos saíam todos com `caixa: None`,
    /// e com eles o estilo do `<div>` (a margem, o `float`, o `clear`), que é
    /// lido de `caixa_do_filho`. Aqui os três saem com a sua caixa.
    #[test]
    fn a_ordem_de_um_inline_partido_vem_da_arvore_e_nao_do_dom() {
        let dom = crate::parse_html_to_dom("<p><span>texto<div>bloco</div>texto</span></p>");
        let tree = dom.box_tree();
        let span = no_da_tag(&dom, "span");
        let div = no_da_tag(&dom, "div");
        let caixa_span = tree.boxes_of(span)[0];

        // A arvore parte mesmo: anonima, div, anonima.
        let filhas = tree.children(caixa_span);
        assert_eq!(filhas.len(), 3, "anon, o div, anon: {filhas:?}");
        assert!(matches!(tree.kind(filhas[0]), BoxKind::Anonymous { .. }));
        assert_eq!(tree.node_of(filhas[1]), Some(div), "o bloco e irmao das anonimas");
        assert!(matches!(tree.kind(filhas[2]), BoxKind::Anonymous { .. }));

        let seq = sequencia_do_fluxo(&dom, &tree, span, Some(caixa_span));
        assert_eq!(seq.len(), 3, "texto, div, texto — as anonimas expandidas");
        assert!(
            seq.iter().all(|f| f.caixa.is_some()),
            "os tres saem COM caixa; com o cursor antigo os tres saiam sem nenhuma"
        );
        assert_eq!(seq[1].no, div, "o bloco fica no meio, na ordem da arvore");
        assert_eq!(
            tree.parent(seq[0].caixa.unwrap()),
            Some(filhas[0]),
            "o texto da frente pertence a anonima, e e por ela que foi alcancado"
        );
        assert!(matches!(&dom.node(seq[0].no).kind, NodeKind::Text(t) if t == "texto"));
        assert_eq!(tree.parent(seq[2].caixa.unwrap()), Some(filhas[2]));
    }

    /// A nó de TEXTO entra na sequência COM a sua caixa. Antes deste módulo o
    /// texto aparecia no laço por vir do DOM; agora aparece por estar na
    /// árvore, e é essa a diferença que faz uma caixa anónima ser alcançável.
    #[test]
    fn um_no_de_texto_entra_na_sequencia_com_a_sua_caixa() {
        let dom = crate::parse_html_to_dom("<div>ola</div>");
        let tree = dom.box_tree();
        let div = no_da_tag(&dom, "div");
        let seq = sequencia_do_fluxo(&dom, &tree, div, Some(tree.boxes_of(div)[0]));

        assert_eq!(seq.len(), 1);
        assert!(
            seq[0].caixa.is_some(),
            "o texto tem caixa desde que a arvore a da"
        );
        assert!(matches!(&dom.node(seq[0].no).kind, NodeKind::Text(_)));
    }

    /// Um COMENTÁRIO não gera caixa e continua na sequência, na posição do DOM.
    ///
    /// É a recusa que o cabeçalho deste módulo explica: hoje um comentário
    /// fecha a corrida de inline-blocks e quebra o colapso de margens entre dois
    /// blocos. Deixá-lo cair mudava respostas, e a régua deste lote é zero
    /// perdidos **e zero ganhos**.
    #[test]
    fn um_comentario_nao_tem_caixa_e_fica_na_sequencia_onde_o_dom_o_poe() {
        let dom = crate::parse_html_to_dom("<div><p>a</p><!--c--><p>b</p></div>");
        let tree = dom.box_tree();
        let div = no_da_tag(&dom, "div");
        let seq = sequencia_do_fluxo(&dom, &tree, div, Some(tree.boxes_of(div)[0]));

        assert_eq!(seq.len(), 3, "dois <p> e o comentario entre eles");
        assert!(seq[1].caixa.is_none(), "um comentario nao gera caixa");
        assert!(matches!(&dom.node(seq[1].no).kind, NodeKind::Comment(_)));
    }

    /// Sem árvore (`DisplayList::default()`), a sequência é a do DOM e nenhuma
    /// caixa é prometida — o caminho que mantém os cinco sítios que constroem
    /// uma lista vazia a compilar e a responder como sempre responderam.
    #[test]
    fn sem_arvore_a_sequencia_e_a_do_dom() {
        let dom = crate::parse_html_to_dom("<div><p>a</p><!--c--><p>b</p></div>");
        let div = no_da_tag(&dom, "div");
        let seq = sequencia_do_fluxo(&dom, &BoxTree::default(), div, None);

        assert_eq!(seq.len(), dom.node(div).children.len());
        assert!(seq.iter().all(|f| f.caixa.is_none()));
    }

    /// Uma `BoxId` de OUTRA construção da árvore é recusada, e é esta a
    /// asserção que substituiu a equivalência do espelho: já não há duas
    /// sequências para comparar, mas há uma árvore a que a caixa tem de
    /// pertencer.
    #[test]
    #[should_panic(expected = "nao e desta arvore")]
    fn uma_caixa_de_outra_arvore_e_recusada() {
        let dom = crate::parse_html_to_dom("<div><p>a</p></div>");
        let antiga = dom.box_tree();
        let div = no_da_tag(&dom, "div");
        let caixa = antiga.boxes_of(div)[0];
        let nova = crate::boxes::build_mirror(&dom);

        let _ = sequencia_do_fluxo(&dom, &nova, div, Some(caixa));
    }
}
