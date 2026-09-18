//! As CHAVES dos caches de layout: medida, fragmento e largura intrínseca, e o
//! endereço de caixa que as três partilham.
//!
//! Saíram de `dom/mod.rs` quando ele passou do teto de 500 linhas: são tipos
//! de dados puros, lidos pelo layout e escritos por `caches.rs`, e nada no
//! resto do `Dom` os constrói.

use super::NodeIdx;

/// Chave de uma medição de layout descartável. O cache guarda apenas `(outer_w,
/// outer_h)`, nunca itens de pintura; por isso a posição `(x,y)` não participa.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct LayoutMeasureKey {
    pub(crate) tree: u64,
    pub(crate) node_epoch: u64,
    pub(crate) style_epoch: u64,
    /// A caixa medida. Era um `enum` com um braço `No(NodeIdx)` para os
    /// chamadores que só sabiam o nó; `measure_block` passou a exigir a
    /// caixa, e o braço ficou sem quem o construísse.
    pub(crate) target: BoxCacheTarget,
    pub(crate) avail_w: u32,
    pub(crate) avail_h: Option<u32>,
    pub(crate) forced_outer_w: Option<u32>,
    pub(crate) forced_outer_h: Option<u32>,
    pub(crate) shrink_to_fit: bool,
    pub(crate) viewport_w: u32,
    pub(crate) viewport_h: u32,
    pub(crate) measurer: u64,
}

/// O endereço de uma caixa que SOBREVIVE à reconstrução da árvore de caixas:
/// o nó que a gera e a posição dela entre as caixas desse nó.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct BoxCacheTarget {
    /// O nó continua estável entre reconstruções da árvore.
    pub(crate) node: NodeIdx,
    /// A posição da caixa entre as caixas que esse nó gerou. Junto do nó, é a
    /// identidade semântica da caixa; o `BoxId` concreto só vale na árvore que
    /// o produziu.
    pub(crate) ordinal: u32,
}

/// A chave de um FRAGMENTO de layout: o desenho de uma subárvore posta com
/// certas constraints.
///
/// É a `LayoutMeasureKey` sem a posição — porque a posição é justamente o que se
/// corrige ao reusar (o desenho é o mesmo, deslocado). Os campos são os mesmos
/// pela mesma razão: cada um protege uma dependência do resultado. `node_epoch`
/// cobre mudanças na subárvore, `style_epoch` as globais de estilo, o viewport e
/// o medidor cobrem o resto do ambiente.
///
/// **Lote L**: `forced_outer_w`/`forced_outer_h`/`shrink_to_fit` entraram para
/// que flex, grid e out-of-flow pudessem participar do cache. Sem eles, um
/// item de flex cujo `flex-grow` mudasse o `main size` bateria na MESMA chave
/// que o desenho antigo (o `node_epoch` sozinho não vê essa mudança — ela vem
/// do IRMÃO, não do próprio nó) e devolveria a geometria de outra largura
/// imposta: a classe silenciosa que `CLAUDE.md` pede para nomear. A posição
/// continua de fora — é a costura/emissão que a desloca, não a chave.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct FragmentKey {
    pub(crate) tree: u64,
    pub(crate) node_epoch: u64,
    pub(crate) style_epoch: u64,
    pub(crate) anim_epoch: u64,
    pub(crate) target: BoxCacheTarget,
    pub(crate) avail_w: u32,
    pub(crate) avail_h: Option<u32>,
    pub(crate) forced_outer_w: Option<u32>,
    pub(crate) forced_outer_h: Option<u32>,
    pub(crate) shrink_to_fit: bool,
    pub(crate) viewport_w: u32,
    pub(crate) viewport_h: u32,
    pub(crate) measurer: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct IntrinsicWidthKey {
    pub(crate) tree: u64,
    pub(crate) node_epoch: u64,
    pub(crate) style_epoch: u64,
    pub(crate) node: NodeIdx,
    pub(crate) font_size: u32,
    pub(crate) viewport_w: u32,
    pub(crate) viewport_h: u32,
    pub(crate) measurer: u64,
}
