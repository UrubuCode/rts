//! Travessia da árvore por elemento e por nó: texto, atributos, filhos,
//! irmãos, pai, e os `getElementsBy*`.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {

    /// Concatena o texto de TODOS os descendentes de `id`, em ordem de documento
    /// (`element.textContent` getter). `None` se o id não resolve nesta árvore.
    /// Num nó de texto, retorna o próprio texto.
    pub fn text_content(&self, id: NodeId) -> Option<String> {
        let idx = self.resolve(id)?;
        let mut out = String::new();
        self.collect_text_into(idx, &mut out);
        Some(out)
    }

    fn collect_text_into(&self, idx: NodeIdx, out: &mut String) {
        match &self.nodes[idx].kind {
            NodeKind::Text(t) => out.push_str(t),
            _ => {
                for &child in &self.nodes[idx].children {
                    self.collect_text_into(child, out);
                }
            }
        }
    }

    /// O elemento `<html>` top-level do documento, separado da raiz sintética
    /// `#document`. `None` para um fragmento sem `<html>`.
    pub fn document_element(&self) -> Option<NodeId> {
        self.nodes[self.root].children.iter().copied().find_map(|idx| {
            matches!(&self.nodes[idx].kind, NodeKind::Element { tag } if tag == "html")
                .then(|| self.make_id(idx))
        })
    }

    /// Nome da tag de um elemento em minúsculas (`element.tagName`, mas o browser
    /// devolve em CAIXA ALTA para HTML — a fachada TS faz o upper). `None` se não
    /// resolve ou não é elemento.
    pub fn tag_name(&self, id: NodeId) -> Option<&str> {
        let idx = self.resolve(id)?;
        match &self.nodes[idx].kind {
            NodeKind::Element { tag } => Some(tag.as_str()),
            _ => None,
        }
    }

    /// Valor de um atributo (`element.getAttribute`). `None` se o id não resolve
    /// ou o atributo não existe.
    pub fn get_attr(&self, id: NodeId, name: &str) -> Option<&str> {
        let idx = self.resolve(id)?;
        self.nodes[idx].attr(name)
    }

    /// Os filhos ELEMENTO de um nó (`element.children` — exclui nós de texto), em
    /// ordem. Vazio se o id não resolve.
    pub fn child_elements(&self, id: NodeId) -> Vec<NodeId> {
        let Some(idx) = self.resolve(id) else {
            return Vec::new();
        };
        self.nodes[idx]
            .children
            .iter()
            .filter(|&&c| matches!(self.nodes[c].kind, NodeKind::Element { .. }))
            .map(|&c| self.make_id(c))
            .collect()
    }

    /// TODOS os filhos de um nó (`node.childNodes` — inclui nós de TEXTO), em
    /// ordem de documento. Vazio se o id não resolve. (`child_elements` filtra só
    /// elementos; este é o `childNodes` cru do DOM.)
    pub fn child_nodes(&self, id: NodeId) -> Vec<NodeId> {
        let Some(idx) = self.resolve(id) else {
            return Vec::new();
        };
        self.nodes[idx]
            .children
            .iter()
            .map(|&c| self.make_id(c))
            .collect()
    }

    // ── Traversal POR ELEMENTO (pula nós de texto/comentário) — #1757 ────────────
    // `*ElementChild`/`*ElementSibling`/`parentElement` são as variantes "só
    // elemento" das de cima. O JS usa muito mais estas (ignora whitespace/texto).

    /// `element.firstElementChild`: o 1º filho que é ELEMENTO (pula Text/Comment).
    pub fn first_element_child(&self, id: NodeId) -> Option<NodeId> {
        self.child_elements(id).first().copied()
    }

    /// `element.lastElementChild`: o último filho-elemento.
    pub fn last_element_child(&self, id: NodeId) -> Option<NodeId> {
        self.child_elements(id).last().copied()
    }

    /// `element.nextElementSibling`: o próximo irmão que é ELEMENTO (pula texto).
    pub fn next_element_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.element_sibling(id, 1)
    }

    /// `element.previousElementSibling`: o irmão-elemento anterior.
    pub fn previous_element_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.element_sibling(id, -1)
    }

    /// Caminha irmão-a-irmão na direção `delta` até achar um ELEMENTO (ou acabar).
    fn element_sibling(&self, id: NodeId, delta: isize) -> Option<NodeId> {
        let mut cur = id;
        loop {
            cur = self.sibling(cur, delta)?;
            let idx = self.resolve(cur)?;
            if matches!(self.nodes[idx].kind, NodeKind::Element { .. }) {
                return Some(cur);
            }
        }
    }

    /// `element.parentElement`: o pai SE for um elemento; `None` se o pai é o
    /// `#document` (a raiz não é um elemento) ou não há pai.
    pub fn parent_element(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.parent_of(id)?;
        let pidx = self.resolve(parent)?;
        matches!(self.nodes[pidx].kind, NodeKind::Element { .. }).then_some(parent)
    }

    /// `element.matches(sel)`: o nó casa o seletor SIMPLES (tag/`#id`/`.classe`)?
    /// Reusa o matcher de `querySelector` (mesma sintaxe). Combinadores → #1752.
    pub fn matches_selector(&self, id: NodeId, sel: &str) -> bool {
        self.resolve(id)
            .map(|i| self.matches(i, sel.trim()))
            .unwrap_or(false)
    }

    /// `element.closest(sel)`: sobe pela cadeia de ancestrais (incluindo o próprio
    /// nó) e devolve o PRIMEIRO que casa o seletor; `None` se nenhum casa.
    pub fn closest(&self, id: NodeId, sel: &str) -> Option<NodeId> {
        let sel = sel.trim();
        let mut cur = Some(id);
        while let Some(node) = cur {
            let idx = self.resolve(node)?;
            if matches!(self.nodes[idx].kind, NodeKind::Element { .. }) && self.matches(idx, sel) {
                return Some(node);
            }
            cur = self.parent_of(node);
        }
        None
    }


    /// `getElementsByTagName(tag)`: todos os descendentes da árvore com a tag.
    /// (`"*"` casa qualquer elemento.) Reusa o matcher de `query_all`.
    pub fn get_elements_by_tag_name(&self, tag: &str) -> Vec<NodeId> {
        crate::bump!(tag_scans);
        let tag = tag.trim();
        if tag == "*" {
            // todos os elementos em ordem de documento.
            let mut out = Vec::new();
            self.collect_all_elements(self.root, &mut out);
            return out;
        }
        self.query_all(tag)
    }

    fn collect_all_elements(&self, idx: NodeIdx, out: &mut Vec<NodeId>) {
        if idx != self.root && matches!(self.nodes[idx].kind, NodeKind::Element { .. }) {
            out.push(self.make_id(idx));
        }
        for &child in &self.nodes[idx].children {
            self.collect_all_elements(child, out);
        }
    }

    /// `getElementsByClassName(names)`: todos os elementos que têm TODAS as classes
    /// dadas (separadas por espaço — semântica AND da MDN). Um único token reusa o
    /// caminho de `.classe`; múltiplos filtram por interseção.
    pub fn get_elements_by_class_name(&self, names: &str) -> Vec<NodeId> {
        let wanted: Vec<&str> = names.split_whitespace().collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        // varre todos os elementos, mantendo os que têm TODAS as classes pedidas.
        let mut out = Vec::new();
        self.collect_by_classes(self.root, &wanted, &mut out);
        out
    }

    fn collect_by_classes(&self, idx: NodeIdx, wanted: &[&str], out: &mut Vec<NodeId>) {
        if idx != self.root {
            if let Some(class_attr) = self.nodes[idx].attr("class") {
                if wanted.iter().all(|wanted_class| {
                    class_attr
                        .split_whitespace()
                        .any(|class| class == *wanted_class)
                }) {
                    out.push(self.make_id(idx));
                }
            }
        }
        for &child in &self.nodes[idx].children {
            self.collect_by_classes(child, wanted, out);
        }
    }

    /// `getElementsByName(name)`: todos os elementos cujo atributo `name` é igual.
    /// Nome vazio → lista vazia (consistente com getElementsByClassName).
    pub fn get_elements_by_name(&self, name: &str) -> Vec<NodeId> {
        if name.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        self.collect_by_name(self.root, name, &mut out);
        out
    }

    fn collect_by_name(&self, idx: NodeIdx, name: &str, out: &mut Vec<NodeId>) {
        if idx != self.root && self.nodes[idx].attr("name") == Some(name) {
            out.push(self.make_id(idx));
        }
        for &child in &self.nodes[idx].children {
            self.collect_by_name(child, name, out);
        }
    }


    // ── Node utils — #1762 ───────────────────────────────────────────────────────

    /// `node.contains(other)`: `other` é o próprio nó OU um descendente dele?
    /// (Reusa a guarda de ciclo `is_ancestor`, que é exatamente esta relação.)
    pub fn contains(&self, node: NodeId, other: NodeId) -> bool {
        let (Some(a), Some(b)) = (self.resolve(node), self.resolve(other)) else {
            return false;
        };
        a == b || self.is_ancestor(a, b)
    }

    /// `node.hasChildNodes()`: tem ao menos um filho (de qualquer tipo)?
    pub fn has_child_nodes(&self, id: NodeId) -> bool {
        self.resolve(id)
            .map(|i| !self.nodes[i].children.is_empty())
            .unwrap_or(false)
    }


    // ── Navegação do DOM (parentNode / first|lastChild / next|previousSibling) ───
    // O `parent`/`children` da arena já têm tudo; aqui só expomos no vocabulário do
    // DOM. `None`/`-1` na fronteira ABI quando não há (raiz não tem pai; primeiro
    // filho não tem irmão anterior; etc.).

    /// `node.parentNode`: o pai, ou `None` para a raiz `#document` (ou id inválido).
    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        let idx = self.resolve(id)?;
        self.nodes[idx].parent.map(|p| self.make_id(p))
    }

    /// `node.firstChild`: o PRIMEIRO filho (qualquer tipo, inclui Text), ou `None`.
    pub fn first_child(&self, id: NodeId) -> Option<NodeId> {
        let idx = self.resolve(id)?;
        self.nodes[idx].children.first().map(|&c| self.make_id(c))
    }

    /// `node.lastChild`: o ÚLTIMO filho (qualquer tipo), ou `None`.
    pub fn last_child(&self, id: NodeId) -> Option<NodeId> {
        let idx = self.resolve(id)?;
        self.nodes[idx].children.last().map(|&c| self.make_id(c))
    }

    /// `node.nextSibling`: o próximo irmão na lista de filhos do pai, ou `None` se
    /// é o último (ou não tem pai / id inválido).
    pub fn next_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.sibling(id, 1)
    }

    /// `node.previousSibling`: o irmão anterior, ou `None` se é o primeiro.
    pub fn previous_sibling(&self, id: NodeId) -> Option<NodeId> {
        self.sibling(id, -1)
    }

    /// Irmão a `delta` posições (`+1` próximo, `-1` anterior). Acha a posição do nó
    /// na lista de filhos do pai e desloca; `None` se sai dos limites.
    fn sibling(&self, id: NodeId, delta: isize) -> Option<NodeId> {
        let idx = self.resolve(id)?;
        let parent = self.nodes[idx].parent?;
        let sibs = &self.nodes[parent].children;
        let pos = sibs.iter().position(|&c| c == idx)?;
        let target = pos as isize + delta;
        if target < 0 || target as usize >= sibs.len() {
            return None;
        }
        Some(self.make_id(sibs[target as usize]))
    }
}
