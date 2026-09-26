//! The TYPES of the fragment cache — [`ChildRef`] and [`Fragment`] — apart from
//! the algorithm that builds, stitches and reuses them (`fragment.rs`), which
//! was past the file ceiling and was about to gain the fragment's last-line
//! baseline (`line_baseline.rs`). Moved verbatim.

use super::*;

/// Uma subárvore emitida por referência dentro de uma lista ou de outro
/// fragmento.
#[derive(Clone, Debug)]
pub struct ChildRef {
    /// A caixa que esta subárvore desenha. O nó deixa de ser suficiente quando
    /// um inline partido tem dois fragmentos no mesmo container.
    pub caixa: crate::boxes::BoxId,
    /// Altura externa que ele ocupou e a margem de topo resolvida: se qualquer
    /// uma mudar ao refazê-lo, tudo abaixo desloca e a costura não serve.
    pub height: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    /// As CONSTRAINTS com que ele foi layoutado — as do CONTEÚDO do pai, não as
    /// do pai. Refazer um filho com a largura do container em vez da do conteúdo
    /// dá uma caixa larga demais pela soma do padding e da margem.
    pub avail_w: f32,
    pub avail_h: Option<f32>,
    /// As mesmas três constraints IMPOSTAS com que este filho foi posto — flex,
    /// grid e out-of-flow ditam largura/altura externa e shrink-to-fit; o fluxo
    /// de bloco normal deixa as duas primeiras em `None` e a terceira `false`.
    /// A costura precisa delas para relayoutar um filho sujo com a MESMA
    /// imposição, e não com a do fluxo normal — reusar `None`/`None`/`false`
    /// aqui daria a um item de flex a largura errada (a classe silenciosa que
    /// o lote L existe para fechar).
    pub forced_outer_w: Option<f32>,
    pub forced_outer_h: Option<f32>,
    pub shrink_to_fit: bool,
    // No position of its own: where this subtree paints, and where its hit
    // order enters, is where its `Piece::Child` stands in the sequence
    // (`pieces.rs`). The two indices it carried (`at` into the items, `hit_at`
    // into the hit order) grew apart for different reasons, and keeping them
    // aligned was the arithmetic of invariant I5.
    pub fragment: std::rc::Rc<Fragment>,
    pub dx: f32,
    pub dy: f32,
}

impl PartialEq for ChildRef {
    /// Compara CONTEÚDO — duas listas equivalentes podem ter chegado ao mesmo
    /// desenho por caminhos diferentes.
    fn eq(&self, other: &Self) -> bool {
        self.dx == other.dx && self.dy == other.dy && self.fragment.pieces == other.fragment.pieces
    }
}

/// O DESENHO de uma subárvore posta com certas constraints, guardado para ser
/// reusado numa posição diferente.
///
/// Coordenadas ABSOLUTAS, como saíram do layout: a origem em que foi calculado
/// fica registrada em `origin`, e reusar é somar a diferença. Guardar já
/// relativo daria na mesma e custaria uma passada extra na hora de gravar — o
/// caso comum é justamente reusar na MESMA posição (nada acima dele mudou de
/// altura), e aí a soma é zero e nem se percorre.
#[derive(Clone, Debug)]
pub struct Fragment {
    /// A caixa que este fragmento desenha.
    pub caixa: crate::boxes::BoxId,
    /// A árvore que emitiu todos os `BoxId`s deste fragmento. Um fragmento pode
    /// sobreviver à reconstrução da árvore; nesses casos ele é reidratado para
    /// a árvore nova antes de voltar à `DisplayList`.
    pub tree: std::rc::Rc<crate::boxes::BoxTree>,
    /// This subtree's output in paint order — own items, the subtrees it reused
    /// by reference, the geometry marks of its boxes (`pieces.rs`).
    ///
    /// `Rc`, like `rects`, so that emitting or re-emitting a cached fragment
    /// never copies it. A STITCH does copy it, where it used to copy only the
    /// subtree list and share the items: one sequence cannot be half-replaced.
    pub pieces: std::rc::Rc<Vec<super::Piece>>,
    /// Geometria por caixa. A fronteira pública agrega-a por nó somente ao
    /// construir `Geometry`.
    pub rects: std::rc::Rc<Vec<(crate::boxes::BoxId, Rect)>>,
    /// Tracks de coluna resolvidas desta subárvore, para `computedProperty`.
    pub grid_column_tracks: std::rc::Rc<Vec<(NodeIdx, Vec<f32>)>>,
    /// Regiões roláveis internas descobertas dentro da subárvore.
    pub scroll_regions: Vec<ScrollRegion>,
    /// The baseline of this subtree's lowest OWN line box, in the coordinates
    /// of `origin`: `linha_directa` from the flows that ran in this fragment's
    /// own list, `ultima_linha` counting its child fragments too. Re-announced
    /// on every emission (`crate::layout::inline::line_baseline::regista_do_fragmento`), because a
    /// cached fragment runs no flow; the split is what lets a stitch replace a
    /// child and recompute the total (`fragment::stitch`).
    pub linha_directa: Option<f32>,
    pub ultima_linha: Option<f32>,
    /// The static positions the inline flows of THIS fragment's own list
    /// recorded (`static_anchor.rs`), in the coordinates of `origin`. A cached
    /// fragment runs no flow, so they have to travel with it.
    pub ancoras_estaticas: std::rc::Rc<Vec<(crate::boxes::BoxId, f32, f32)>>,
    /// Onde este fragmento foi calculado.
    pub origin: (f32, f32),
    /// Tamanho externo devolvido pelo `layout_block` (o que o chamador usa para
    /// avançar o cursor).
    pub size: (f32, f32),
    /// A MARGEM DE TOPO resolvida deste bloco, para o colapso com o irmão
    /// anterior.
    ///
    /// Guardada junto porque o laço a calculava ANTES de descobrir que o
    /// fragmento servia: resolver a margem pede o estilo computado, o
    /// `font-size` do contexto e um `ResolveCtx` — por filho, mil vezes por
    /// frame, para um valor que não muda enquanto o epoch do nó não muda.
    pub margin_top: f32,
    /// A MARGEM DE BAIXO resolvida, para o colapso com o irmão SEGUINTE.
    ///
    /// Vive aqui pela mesma razão que a de topo — resolvê-la pede o estilo
    /// computado e um `ResolveCtx` por filho — e entrou depois dela porque o
    /// laço comparava a margem de CIMA do anterior com a de cima do seguinte:
    /// o caminho rápido só podia devolver o que guardava.
    pub margin_bottom: f32,
}

impl Fragment {
    /// Traduz os IDs privados deste fragmento para `tree`.
    ///
    /// A tradução só aceita uma correspondência um-a-um por `(NodeIdx,
    /// ordinal)`. Caixas anônimas, ou um nó que passou a gerar outra quantidade
    /// de caixas, fazem o cache recusar o acerto: escolher uma caixa parecida
    /// seria precisamente o reaproveitamento silenciosamente errado que a
    /// geração de `BoxId` existe para impedir.
    pub(in crate::layout) fn remapped_to(
        self: &std::rc::Rc<Self>,
        tree: &std::rc::Rc<crate::boxes::BoxTree>,
    ) -> Option<std::rc::Rc<Self>> {
        if std::rc::Rc::ptr_eq(&self.tree, tree) {
            return Some(std::rc::Rc::clone(self));
        }
        // `BoxTree::translate_from` (`boxes/generated.rs`): the address of a box
        // is the tree's knowledge since a generated box has no `(node, ordinal)`.
        let map_box = |old| tree.translate_from(&self.tree, old);
        let caixa = map_box(self.caixa)?;
        let rects = self
            .rects
            .iter()
            .map(|&(old, rect)| Some((map_box(old)?, rect)))
            .collect::<Option<Vec<_>>>()?;
        // Every piece is rebuilt, the items included, where the item vector
        // used to be shared: the marks and subtrees name boxes of the old tree,
        // and they live in the same sequence as the items now.
        let pieces = self
            .pieces
            .iter()
            .map(|piece| {
                Some(match piece {
                    super::Piece::Item(item) => super::Piece::Item(item.clone()),
                    super::Piece::Rect(old) => super::Piece::Rect(map_box(*old)?),
                    super::Piece::Child(child) => super::Piece::Child(ChildRef {
                        caixa: map_box(child.caixa)?,
                        fragment: child.fragment.remapped_to(tree)?,
                        ..child.clone()
                    }),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(std::rc::Rc::new(Fragment {
            caixa,
            tree: std::rc::Rc::clone(tree),
            pieces: std::rc::Rc::new(pieces),
            rects: std::rc::Rc::new(rects),
            grid_column_tracks: std::rc::Rc::clone(&self.grid_column_tracks),
            scroll_regions: self.scroll_regions.clone(),
            linha_directa: self.linha_directa,
            ultima_linha: self.ultima_linha,
            ancoras_estaticas: std::rc::Rc::new(
                self.ancoras_estaticas.iter().map(|&(b, x, y)| Some((map_box(b)?, x, y))).collect::<Option<Vec<_>>>()?,
            ),
            origin: self.origin,
            size: self.size,
            margin_top: self.margin_top,
            margin_bottom: self.margin_bottom,
        }))
    }

    /// Emite este fragmento numa `DisplayList`, deslocado para `(x, y)`.
    #[allow(clippy::too_many_arguments)]
    pub fn emit_at(
        self: &std::rc::Rc<Self>,
        list: &mut DisplayList,
        x: f32,
        y: f32,
        avail_w: f32,
        avail_h: Option<f32>,
        forced_outer_w: Option<f32>,
        forced_outer_h: Option<f32>,
        shrink_to_fit: bool,
    ) {
        let (dx, dy) = (x - self.origin.0, y - self.origin.1);
        if let (Some(baseline), Some(owner)) = (self.ultima_linha, self.tree.node_of(self.caixa)) {
            crate::layout::inline::line_baseline::regista_do_fragmento(owner, baseline + dy);
        }
        // APONTA, não copia: os itens desta subárvore já existem e não mudaram.
        // Os RETÂNGULOS abaixo continuam sendo materializados, porque a consulta
        // de geometria é por nó e precisa do valor pronto — são 16 bytes contra
        // os 48 de um item, numa quantidade menor.
        for (node, tracks) in self.grid_column_tracks.iter() {
            list.grid_column_tracks.insert(*node, tracks.clone());
        }
        list.pieces.push(super::Piece::Child(ChildRef {
            caixa: self.caixa,
            height: self.size.1,
            margin_top: self.margin_top,
            margin_bottom: self.margin_bottom,
            avail_w,
            avail_h,
            forced_outer_w,
            forced_outer_h,
            shrink_to_fit,
            fragment: std::rc::Rc::clone(self),
            dx,
            dy,
        }));
        // A GEOMETRIA da subárvore (retângulos, ordem de hit-test, regiões
        // roláveis) também fica na referência: materializá-la aqui era metade do
        // custo de um frame parado — três inserções em mapa por fragmento, mil
        // fragmentos. Quem precisa dela chama `geometry()`, que percorre a
        // árvore uma vez e guarda o resultado.
    }
}
