//! `NodeId`, `NodeKind`, `Attr` e `Node` — o nó e o handle versionado que
//! atravessa a fronteira TS/ABI.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

/// Identificador VERSIONADO e estável de um nó: `{ generation, idx }` (invariante 2 do
/// roadmap — sem `generation`, um índice reciclado após re-parse aplica estado a um nó
/// vivo errado, um bug de SEGURANÇA DE MEMÓRIA). É o handle que o lado JS guarda.
///
/// `generation` é a geração da ÁRVORE dona do nó; `idx` é a posição na arena. Um acesso
/// só é válido se a `generation` do id casa com a `generation` da árvore atual (`Dom::generation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeId {
    pub generation: u32,
    pub idx: u32,
}

impl NodeId {
    /// Empacota num `i64` opaco para a ABI: `(generation << 32) | idx`. Sempre ≥ 0
    /// (generation começa em 1, então o bit de sinal nunca acende). `-1` é a sentinela
    /// de "nó nenhum" (invariante 3), distinta de qualquer id real.
    pub fn to_abi(self) -> i64 {
        (((self.generation as u64) << 32) | (self.idx as u64)) as i64
    }

    /// Desempacota o `i64` da ABI. `None` para a sentinela `-1` ou valores
    /// negativos (id inválido vindo do TS).
    pub fn from_abi(v: i64) -> Option<NodeId> {
        if v < 0 {
            return None;
        }
        let u = v as u64;
        Some(NodeId {
            generation: (u >> 32) as u32,
            idx: (u & 0xFFFF_FFFF) as u32,
        })
    }
}

/// O tipo de um nó da árvore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// A raiz sintética (`#document`) que contém os nós de topo.
    Document,
    /// Um elemento com nome de tag em minúsculas (`h1`, `p`, `div`, `b`, `i`,
    /// e também tags desconhecidas como `span`/`code` — preservadas como nós,
    /// não descartadas: um DOM fiel mantém o elemento).
    Element { tag: String },
    /// Um nó de texto (folha). Entidades já vêm decodificadas.
    Text(String),
    /// Um nó de COMENTÁRIO (`<!-- ... -->`). Um DOM fiel preserva comentários como
    /// nós (nodeType 8); o render os ignora. O conteúdo é o texto entre os
    /// delimitadores. (Antes eram descartados no parse.)
    Comment(String),
}

/// Um par atributo→valor de um elemento (`class="card"`). Lista ordenada (não
/// mapa) para preservar a ordem do HTML — importante para `style` e para a
/// futura cascata de CSS, onde a ordem de declaração desempata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    /// Nome em minúsculas (`class`, `id`, `href`, `style`…).
    pub name: String,
    /// Valor já com entidades decodificadas; `""` para atributo sem valor.
    pub value: String,
}

/// Um nó da árvore: seu tipo + atributos + os elos de parentesco (índices na
/// arena).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    /// Atributos do elemento (vazio para Document/Text). É a base de qualquer
    /// seletor além da tag (`.classe`, `#id`) e de `<a href>` — o pré-requisito
    /// de um motor de CSS.
    pub attrs: Vec<Attr>,
    /// `None` apenas para a raiz `Document`. Índice cru (interno à arena).
    pub parent: Option<NodeIdx>,
    /// Filhos em ordem de documento. Índices crus (internos à arena).
    pub children: Vec<NodeIdx>,
}

impl Node {
    /// Valor do atributo `name` (case-insensitive), se presente.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .map(|a| a.value.as_str())
    }
}
