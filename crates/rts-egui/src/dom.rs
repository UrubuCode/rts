//! `Dom` — árvore de elementos RETIDA (retained-mode), a fonte da verdade do
//! conteúdo de uma janela.
//!
//! Diferente do caminho immediate-mode de `html.rs` (que re-parseia a string a
//! cada frame para uma fila PLANA de `WidgetCmd`), aqui o HTML é parseado UMA vez
//! para uma **árvore de nós persistente**. Essa árvore:
//!
//! - tem hierarquia real (cada nó conhece pai e filhos), não pares Begin/End;
//! - dá a cada nó um `NodeId` ESTÁVEL — o que o JS vai usar depois para
//!   referenciar e MUTAR um elemento (`getElementById`/`setText`/`append`…);
//! - é o que o render passa a percorrer (Fatia 2), em vez da fila.
//!
//! ## Por que arena (`Vec<Node>` + índice como `NodeId`)
//!
//! Um DOM é um grafo mutável com referências cruzadas (pai↔filho). Em Rust isso
//! com `Rc<RefCell<…>>` vira um inferno de borrows/ciclos. A arena resolve:
//! cada nó vive num `Vec`, o `NodeId` é só o índice — `Copy`, estável, trivial
//! de guardar do lado do JS, e a mutação é um `self.nodes[id]` sem brigar com o
//! borrow-checker. É o padrão consagrado (`indextree`/o DOM do servo etc).
//!
//! ## Como SABER se a árvore está correta (o ponto desta fatia)
//!
//! `Node`/`NodeKind`/`Dom` derivam `Debug` + `PartialEq`, e `Dom::dump()`
//! serializa a árvore indentada (estilo devtools). Com isso a verificação é um
//! teste unitário determinístico (`cargo test -p rts-egui`), SEM abrir janela:
//! compara-se o `dump()` (ou a estrutura) com o esperado. Ver `#[cfg(test)]`.

use crate::html::{tokenize, Token};

/// Identificador estável de um nó na arena. É o índice em `Dom::nodes`.
///
/// Estável durante a vida da árvore (não há compactação): é exatamente o handle
/// que o lado JS guardará para mutar um elemento depois.
pub type NodeId = usize;

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
}

/// Um nó da árvore: seu tipo + os elos de parentesco (índices na arena).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    /// `None` apenas para a raiz `Document`.
    pub parent: Option<NodeId>,
    /// Filhos em ordem de documento.
    pub children: Vec<NodeId>,
}

/// A árvore inteira: arena de nós + a raiz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dom {
    /// Arena: `nodes[id]` é o nó de `NodeId == id`.
    pub nodes: Vec<Node>,
    /// A raiz sintética `#document`.
    pub root: NodeId,
}

impl Dom {
    /// Cria uma árvore vazia contendo só o `#document`.
    fn new() -> Dom {
        Dom {
            nodes: vec![Node {
                kind: NodeKind::Document,
                parent: None,
                children: Vec::new(),
            }],
            root: 0,
        }
    }

    /// Aloca um nó como filho de `parent` e devolve seu `NodeId`.
    fn push(&mut self, kind: NodeKind, parent: NodeId) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            parent: Some(parent),
            children: Vec::new(),
        });
        self.nodes[parent].children.push(id);
        id
    }

    /// Acesso por id (conveniência para o render e a futura API de mutação JS).
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    /// Serializa a árvore indentada (estilo devtools) — a forma legível de
    /// inspecionar/verificar o que foi gerado. Elemento vira `<tag>`; texto vira
    /// a string entre aspas; cada nível adiciona 2 espaços.
    ///
    /// Exemplo de saída:
    /// ```text
    /// #document
    ///   <h1>
    ///     "Titulo"
    ///   <p>
    ///     "antes "
    ///     <b>
    ///       "forte"
    /// ```
    pub fn dump(&self) -> String {
        let mut out = String::new();
        self.dump_node(self.root, 0, &mut out);
        out
    }

    fn dump_node(&self, id: NodeId, depth: usize, out: &mut String) {
        let node = &self.nodes[id];
        for _ in 0..depth {
            out.push_str("  ");
        }
        match &node.kind {
            NodeKind::Document => out.push_str("#document"),
            NodeKind::Element { tag } => {
                out.push('<');
                out.push_str(tag);
                out.push('>');
            }
            NodeKind::Text(t) => {
                out.push('"');
                out.push_str(t);
                out.push('"');
            }
        }
        out.push('\n');
        for &child in &node.children {
            self.dump_node(child, depth + 1, out);
        }
    }
}

/// Tags que são VAZIAS (void) — não têm fechamento nem filhos. Mínimo por ora.
fn is_void(tag: &str) -> bool {
    matches!(tag, "br" | "hr" | "img" | "input" | "meta" | "link")
}

/// Parseia HTML para uma árvore retida. Reusa o tokenizador de `html.rs`; a
/// diferença é a etapa sintática: aqui mantém-se uma PILHA de "elemento aberto"
/// e cada nó nasce filho do topo da pilha.
///
/// - Tag de abertura → cria `Element` filho do topo e empurra na pilha (salvo
///   void, que não empurra).
/// - Tag de fechamento → faz pop até casar o nome (tolerante a aninhamento
///   malformado; um `</x>` sem `<x>` aberto é ignorado).
/// - Texto → vira nó `Text` filho do topo (whitespace puro entre tags é
///   descartado, como no caminho immediate-mode, para a árvore não encher de
///   nós de espaço irrelevantes).
pub fn parse_html_to_dom(html: &str) -> Dom {
    let mut dom = Dom::new();
    // Pilha de (NodeId aberto, nome da tag). Começa na raiz Document.
    let mut open: Vec<(NodeId, String)> = vec![(dom.root, String::new())];

    for tok in tokenize(html) {
        match tok {
            Token::Tag { name, close } => {
                if close {
                    // Pop até encontrar a tag de nome igual (tolerante).
                    if let Some(pos) = open.iter().rposition(|(_, n)| *n == name) {
                        // Fecha esse nível e quaisquer filhos mal-fechados acima.
                        open.truncate(pos);
                    }
                    // `</x>` órfão (sem abertura): ignora, não mexe na pilha.
                } else {
                    let parent = open.last().unwrap().0;
                    let id = dom.push(NodeKind::Element { tag: name.clone() }, parent);
                    if !is_void(&name) {
                        open.push((id, name));
                    }
                }
            }
            Token::Text(text) => {
                if text.trim().is_empty() {
                    continue; // whitespace puro entre tags — descarta.
                }
                let parent = open.last().unwrap().0;
                dom.push(NodeKind::Text(text), parent);
            }
        }
    }
    dom
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: nome de tag de um nó Element (panica se não for elemento) — só
    /// para deixar os asserts curtos.
    fn tag(dom: &Dom, id: NodeId) -> &str {
        match &dom.node(id).kind {
            NodeKind::Element { tag } => tag,
            other => panic!("esperava Element, achei {other:?}"),
        }
    }

    #[test]
    fn arvore_simples_heading_e_paragrafo() {
        let dom = parse_html_to_dom("<h1>Titulo</h1><p>Corpo</p>");
        // Document tem 2 filhos de topo: h1 e p.
        let top = &dom.node(dom.root).children;
        assert_eq!(top.len(), 2);
        assert_eq!(tag(&dom, top[0]), "h1");
        assert_eq!(tag(&dom, top[1]), "p");
        // h1 tem um único filho de texto "Titulo".
        let h1_kids = &dom.node(top[0]).children;
        assert_eq!(h1_kids.len(), 1);
        assert_eq!(dom.node(h1_kids[0]).kind, NodeKind::Text("Titulo".into()));
    }

    #[test]
    fn inline_aninhado_vira_subarvore() {
        // <b> com <i> dentro precisa virar b → i → texto (aninhamento real).
        let dom = parse_html_to_dom("<p>a <b>forte <i>e it</i></b> z</p>");
        let p = dom.node(dom.root).children[0];
        assert_eq!(tag(&dom, p), "p");
        let pk = &dom.node(p).children;
        // p: "a ", <b>, " z"
        assert_eq!(pk.len(), 3);
        assert_eq!(dom.node(pk[0]).kind, NodeKind::Text("a ".into()));
        assert_eq!(tag(&dom, pk[1]), "b");
        assert_eq!(dom.node(pk[2]).kind, NodeKind::Text(" z".into()));
        // <b>: "forte ", <i>
        let bk = &dom.node(pk[1]).children;
        assert_eq!(bk.len(), 2);
        assert_eq!(dom.node(bk[0]).kind, NodeKind::Text("forte ".into()));
        assert_eq!(tag(&dom, bk[1]), "i");
        // <i>: "e it"
        assert_eq!(dom.node(bk[1]).children.len(), 1);
    }

    #[test]
    fn cada_no_conhece_o_pai() {
        let dom = parse_html_to_dom("<p><b>x</b></p>");
        let p = dom.node(dom.root).children[0];
        let b = dom.node(p).children[0];
        let x = dom.node(b).children[0];
        assert_eq!(dom.node(p).parent, Some(dom.root));
        assert_eq!(dom.node(b).parent, Some(p));
        assert_eq!(dom.node(x).parent, Some(b));
    }

    #[test]
    fn tag_desconhecida_e_preservada_como_no() {
        // No caminho de fila <span> some; na árvore ele PERSISTE como elemento.
        let dom = parse_html_to_dom("<p>oi <span>spn</span> tchau</p>");
        let p = dom.node(dom.root).children[0];
        let pk = &dom.node(p).children;
        assert_eq!(pk.len(), 3);
        assert_eq!(tag(&dom, pk[1]), "span");
        assert_eq!(dom.node(pk[1]).children.len(), 1);
    }

    #[test]
    fn entidades_decodificadas() {
        let dom = parse_html_to_dom("<p>a &lt; b &amp; c &gt; d</p>");
        let p = dom.node(dom.root).children[0];
        let txt = dom.node(dom.node(p).children[0]).kind.clone();
        assert_eq!(txt, NodeKind::Text("a < b & c > d".into()));
    }

    #[test]
    fn fechamento_orfao_nao_quebra() {
        // </div> sem abertura é ignorado; texto ao redor preservado.
        let dom = parse_html_to_dom("</div><p>ok</p>");
        let top = &dom.node(dom.root).children;
        assert_eq!(top.len(), 1);
        assert_eq!(tag(&dom, top[0]), "p");
    }

    #[test]
    fn void_tag_nao_empilha() {
        // <br> não tem fechamento; o <p> seguinte deve ser irmão, não filho.
        let dom = parse_html_to_dom("<br><p>depois</p>");
        let top = &dom.node(dom.root).children;
        assert_eq!(top.len(), 2);
        assert_eq!(tag(&dom, top[0]), "br");
        assert_eq!(tag(&dom, top[1]), "p");
        assert!(dom.node(top[0]).children.is_empty());
    }

    #[test]
    fn dump_legivel_para_inspecao() {
        let dom = parse_html_to_dom("<h1>Oi</h1><p>antes <b>forte</b></p>");
        let esperado = "\
#document
  <h1>
    \"Oi\"
  <p>
    \"antes \"
    <b>
      \"forte\"
";
        assert_eq!(dom.dump(), esperado);
    }
}
