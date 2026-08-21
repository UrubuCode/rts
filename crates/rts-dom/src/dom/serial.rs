//! Serialização: `innerHTML`/`outerHTML` (o inverso do parser) e o `dump()`
//! indentado estilo devtools.
//!
//! Movido de `dom.rs` na modularização; nenhuma linha de lógica foi alterada.

use super::*;

impl Dom {

    /// `element.innerHTML` (GET) — serializa os FILHOS do nó como string HTML
    /// válida (o inverso do parser: `<tag attrs>filhos</tag>`, texto com entidades
    /// re-encodadas, `<!-- -->` para comentário, void tags sem fechar). `None` se o
    /// id não resolve. Round-trip com `set_inner_html` é estável para o subset.
    pub fn inner_html(&self, id: NodeId) -> Option<String> {
        let idx = self.resolve(id)?;
        let mut out = String::new();
        for &child in &self.nodes[idx].children {
            self.serialize_node(child, &mut out);
        }
        Some(out)
    }

    /// `element.outerHTML` (GET) — como [`inner_html`](Dom::inner_html) mas inclui o
    /// PRÓPRIO elemento (a tag de abertura+fechamento ao redor dos filhos).
    pub fn outer_html(&self, id: NodeId) -> Option<String> {
        let idx = self.resolve(id)?;
        let mut out = String::new();
        self.serialize_node(idx, &mut out);
        Some(out)
    }

    /// Serializa UM nó como HTML (recursivo). Element → `<tag a="v">filhos</tag>`
    /// (void → `<tag>` sem fechar); Text → texto com entidades re-encodadas;
    /// Comment → `<!-- ... -->`; Document → só os filhos.
    fn serialize_node(&self, idx: NodeIdx, out: &mut String) {
        match &self.nodes[idx].kind {
            NodeKind::Document => {
                for &c in &self.nodes[idx].children {
                    self.serialize_node(c, out);
                }
            }
            NodeKind::Text(t) => out.push_str(&crate::html::encode_text_entities(t)),
            NodeKind::Comment(c) => {
                out.push_str("<!--");
                out.push_str(c);
                out.push_str("-->");
            }
            NodeKind::Element { tag } => {
                out.push('<');
                out.push_str(tag);
                for a in &self.nodes[idx].attrs {
                    out.push(' ');
                    out.push_str(&a.name);
                    out.push_str("=\"");
                    out.push_str(&crate::html::encode_attr_entities(&a.value));
                    out.push('"');
                }
                out.push('>');
                if is_void(tag) {
                    return; // void: sem filhos, sem fechamento.
                }
                // RAW-TEXT elements (`<script>`/`<style>`): o conteúdo é CRU —
                // re-encodar entidades corrompia o código (`&&` → `&amp;&amp;`,
                // que o reparse raw NÃO decodifica → syntax error no JS).
                if self.is_raw_text_element(idx) {
                    for &c in &self.nodes[idx].children {
                        if let NodeKind::Text(t) = &self.nodes[c].kind {
                            out.push_str(t);
                        }
                    }
                    out.push_str("</");
                    out.push_str(tag);
                    out.push('>');
                    return;
                }
                for &c in &self.nodes[idx].children {
                    self.serialize_node(c, out);
                }
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
        }
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

    fn dump_node(&self, idx: NodeIdx, depth: usize, out: &mut String) {
        let node = &self.nodes[idx];
        for _ in 0..depth {
            out.push_str("  ");
        }
        match &node.kind {
            NodeKind::Document => out.push_str("#document"),
            NodeKind::Element { tag } => {
                out.push('<');
                out.push_str(tag);
                for a in &node.attrs {
                    out.push(' ');
                    out.push_str(&a.name);
                    out.push_str("=\"");
                    out.push_str(&a.value);
                    out.push('"');
                }
                out.push('>');
            }
            NodeKind::Text(t) => {
                out.push('"');
                out.push_str(t);
                out.push('"');
            }
            NodeKind::Comment(c) => {
                out.push_str("<!--");
                out.push_str(c);
                out.push_str("-->");
            }
        }
        out.push('\n');
        for &child in &node.children {
            self.dump_node(child, depth + 1, out);
        }
    }
}
