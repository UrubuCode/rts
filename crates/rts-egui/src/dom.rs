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

use std::collections::HashMap;

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
    /// `None` apenas para a raiz `Document`.
    pub parent: Option<NodeId>,
    /// Filhos em ordem de documento.
    pub children: Vec<NodeId>,
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

/// A árvore inteira: arena de nós + a raiz + índices de busca O(1).
///
/// **DOM otimizado:** como somos donos da arena, mantemos índices `id → NodeId`
/// e `classe → [NodeId]` atualizados na construção e na mutação. Assim
/// `query("#alvo")`/`query(".card")` é O(1) em vez de varrer a árvore (o que um
/// `querySelector` genérico não consegue). Query por tag segue pré-ordem O(n)
/// (pra respeitar a ordem de documento; um índice por tag viria depois, se valer).
#[derive(Debug, Clone)]
pub struct Dom {
    /// Arena: `nodes[id]` é o nó de `NodeId == id`.
    pub nodes: Vec<Node>,
    /// A raiz sintética `#document`.
    pub root: NodeId,
    /// Índice `valor-de-id → NodeId` (último a registrar vence, como no browser).
    id_index: HashMap<String, NodeId>,
    /// Índice `classe → nós que a têm` (em ordem de inserção).
    class_index: HashMap<String, Vec<NodeId>>,
}

// Igualdade estrutural: compara só a árvore (nodes+root). Os índices são estado
// DERIVADO — duas árvores com os mesmos nós têm os mesmos índices.
impl PartialEq for Dom {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root && self.nodes == other.nodes
    }
}
impl Eq for Dom {}

impl Dom {
    /// Cria uma árvore vazia contendo só o `#document`.
    fn new() -> Dom {
        Dom {
            nodes: vec![Node {
                kind: NodeKind::Document,
                attrs: Vec::new(),
                parent: None,
                children: Vec::new(),
            }],
            root: 0,
            id_index: HashMap::new(),
            class_index: HashMap::new(),
        }
    }

    /// Registra um nó nos índices a partir de seus atributos `id`/`class`.
    fn index_node(&mut self, id: NodeId) {
        // Coleta antes para não emprestar `self.nodes` e os índices juntos.
        let id_attr = self.nodes[id].attr("id").map(str::to_string);
        let classes: Vec<String> = self.nodes[id]
            .attr("class")
            .map(|c| c.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();
        if let Some(k) = id_attr {
            self.id_index.insert(k, id);
        }
        for c in classes {
            self.class_index.entry(c).or_default().push(id);
        }
    }

    /// Aloca um nó (com seus atributos) como filho de `parent`; devolve o id.
    fn push(&mut self, kind: NodeKind, attrs: Vec<Attr>, parent: NodeId) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            attrs,
            parent: Some(parent),
            children: Vec::new(),
        });
        self.index_node(id);
        self.nodes[parent].children.push(id);
        id
    }

    /// Acesso por id (conveniência para o render e a futura API de mutação JS).
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    /// `true` se `id` é um índice válido na arena (defesa contra handles velhos
    /// vindos do JS após um re-parse que encolheu a árvore).
    pub fn is_valid(&self, id: NodeId) -> bool {
        id < self.nodes.len()
    }

    // ── Query (base do querySelector) ───────────────────────────────────────

    /// Primeiro nó que casa com um seletor SIMPLES: `tag` (`"h1"`), `#id`
    /// (`"#alvo"`) ou `.classe` (`".card"`). `None` se nada casar. É o
    /// `querySelector` de um seletor só.
    ///
    /// `#id`/`.classe` usam os índices O(1); `tag` varre em pré-ordem (ordem de
    /// documento). Valida que o hit do índice ainda está vivo (anexado à raiz),
    /// já que mutações podem ter desligado o nó sem limpar o índice.
    pub fn query(&self, selector: &str) -> Option<NodeId> {
        let sel = selector.trim();
        if let Some(key) = sel.strip_prefix('#') {
            // Valida valor + alcançabilidade (o índice pode ter entrada stale).
            return self
                .id_index
                .get(key)
                .copied()
                .filter(|&id| self.is_attached(id) && self.nodes[id].attr("id") == Some(key));
        }
        if let Some(cls) = sel.strip_prefix('.') {
            return self.class_index.get(cls)?.iter().copied().find(|&id| {
                self.is_attached(id)
                    && self.nodes[id]
                        .attr("class")
                        .map(|c| c.split_whitespace().any(|x| x == cls))
                        .unwrap_or(false)
            });
        }
        let tag = sel.to_ascii_lowercase();
        let m = move |n: &Node| matches!(&n.kind, NodeKind::Element { tag: t } if *t == tag);
        self.find_pre_order(self.root, &m)
    }

    /// `true` se `id` está conectado à raiz (não foi desligado por uma mutação).
    /// Os índices não são limpos no `remove`/`append`, então uma busca por
    /// índice valida a alcançabilidade aqui (barato: sobe pelos pais).
    fn is_attached(&self, id: NodeId) -> bool {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if c == self.root {
                return true;
            }
            cur = self.nodes[c].parent;
        }
        false
    }

    fn find_pre_order(&self, id: NodeId, m: &dyn Fn(&Node) -> bool) -> Option<NodeId> {
        let node = &self.nodes[id];
        if id != self.root && m(node) {
            return Some(id);
        }
        for &child in &node.children {
            if let Some(hit) = self.find_pre_order(child, m) {
                return Some(hit);
            }
        }
        None
    }

    // ── Mutação (base da API DOM do JS) ─────────────────────────────────────

    /// Substitui TODO o conteúdo de um elemento por um único nó de texto (o
    /// equivalente a `element.textContent = txt`). Não faz nada num nó de texto.
    pub fn set_text(&mut self, id: NodeId, text: &str) {
        if !self.is_valid(id) || !matches!(self.nodes[id].kind, NodeKind::Element { .. }) {
            return;
        }
        // Descarta os filhos atuais (arena não compacta; vira lixo inacessível —
        // ok para o uso atual, a árvore é reconstruída a cada `html()`).
        self.nodes[id].children.clear();
        let child = self.push_detached(NodeKind::Text(text.to_string()));
        self.nodes[child].parent = Some(id);
        self.nodes[id].children.push(child);
    }

    /// Define/atualiza um atributo (`element.setAttribute`). Cria se não existir.
    /// Mantém os índices `id`/`class` em dia (adiciona a nova entrada; entradas
    /// antigas viram stale mas a busca valida alcançabilidade/valor).
    pub fn set_attr(&mut self, id: NodeId, name: &str, value: &str) {
        if !self.is_valid(id) {
            return;
        }
        let name_lc = name.to_ascii_lowercase();
        let node = &mut self.nodes[id];
        if let Some(a) = node.attrs.iter_mut().find(|a| a.name == name_lc) {
            a.value = value.to_string();
        } else {
            node.attrs.push(Attr { name: name_lc.clone(), value: value.to_string() });
        }
        // Atualiza índices se o atributo afeta busca.
        match name_lc.as_str() {
            "id" => {
                self.id_index.insert(value.to_string(), id);
            }
            "class" => {
                for c in value.split_whitespace() {
                    let v = self.class_index.entry(c.to_string()).or_default();
                    if !v.contains(&id) {
                        v.push(id);
                    }
                }
            }
            _ => {}
        }
    }

    /// Cria um elemento SOLTO (sem pai) e devolve seu id; ligue-o com
    /// `append_child` (`document.createElement`).
    pub fn create_element(&mut self, tag: &str) -> NodeId {
        self.push_detached(NodeKind::Element { tag: tag.to_ascii_lowercase() })
    }

    /// Move `child` para o fim dos filhos de `parent` (`parent.appendChild`).
    /// Remove `child` do pai antigo, se tiver. Ignora ids inválidos ou ciclos.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        if !self.is_valid(parent) || !self.is_valid(child) || parent == child {
            return;
        }
        if self.is_ancestor(child, parent) {
            return; // evita criar ciclo (child seria ancestral de parent)
        }
        self.detach(child);
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.push(child);
    }

    /// Desliga um nó do pai (`element.remove`). O nó continua na arena (lixo).
    pub fn remove_node(&mut self, id: NodeId) {
        if self.is_valid(id) && id != self.root {
            self.detach(id);
        }
    }

    /// Aloca um nó sem pai (usado por create_element / set_text).
    fn push_detached(&mut self, kind: NodeKind) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node { kind, attrs: Vec::new(), parent: None, children: Vec::new() });
        id
    }

    /// Remove `id` da lista de filhos do seu pai atual (se houver).
    fn detach(&mut self, id: NodeId) {
        if let Some(p) = self.nodes[id].parent.take() {
            self.nodes[p].children.retain(|&c| c != id);
        }
    }

    /// `true` se `a` é ancestral de (ou igual a) `b` — guarda contra ciclos.
    fn is_ancestor(&self, a: NodeId, b: NodeId) -> bool {
        let mut cur = Some(b);
        while let Some(c) = cur {
            if c == a {
                return true;
            }
            cur = self.nodes[c].parent;
        }
        false
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

/// Parseia a parte crua de atributos de uma tag (`class='card' id="x" checked`)
/// em pares `Attr`. Tolerante: aceita aspas simples/duplas ou sem aspas, e
/// atributo sem valor (`checked` → value vazio). Nomes em minúsculas; valores
/// com entidades decodificadas. Não é conforme à spec — cobre o uso comum.
fn parse_attrs(raw: &str) -> Vec<Attr> {
    let mut attrs = Vec::new();
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Pula espaços entre atributos.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        // Lê o nome até `=`, espaço ou fim.
        let name_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        if i == name_start {
            break; // nada de nome — acabou.
        }
        let name = raw[name_start..i].to_ascii_lowercase();
        // Pula espaços antes de um possível `=`.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let value = if i < bytes.len() && bytes[i] == b'=' {
            i += 1; // consome `=`
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                // Valor entre aspas: lê até a aspa de fechamento igual.
                let quote = bytes[i];
                i += 1;
                let v_start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                let v = raw[v_start..i].to_string();
                if i < bytes.len() {
                    i += 1; // consome a aspa de fechamento
                }
                v
            } else {
                // Valor sem aspas: lê até o próximo espaço.
                let v_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                raw[v_start..i].to_string()
            }
        } else {
            String::new() // atributo booleano (sem `=valor`).
        };
        attrs.push(Attr {
            name,
            value: crate::html::decode_entities(&value),
        });
    }
    attrs
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
            Token::Tag { name, attrs_raw, close } => {
                if close {
                    // Pop até encontrar a tag de nome igual (tolerante).
                    if let Some(pos) = open.iter().rposition(|(_, n)| *n == name) {
                        // Fecha esse nível e quaisquer filhos mal-fechados acima.
                        open.truncate(pos);
                    }
                    // `</x>` órfão (sem abertura): ignora, não mexe na pilha.
                } else {
                    let parent = open.last().unwrap().0;
                    let attrs = parse_attrs(&attrs_raw);
                    let id = dom.push(NodeKind::Element { tag: name.clone() }, attrs, parent);
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
                dom.push(NodeKind::Text(text), Vec::new(), parent);
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
    fn query_por_tag_id_classe() {
        let dom = parse_html_to_dom(
            "<div class='card'><span id='alvo'>x</span><b class='hl a'>y</b></div>",
        );
        // tag
        let span = dom.query("span").unwrap();
        assert_eq!(tag(&dom, span), "span");
        // #id
        assert_eq!(dom.query("#alvo"), Some(span));
        // .classe (mesmo dentro de class multi-valor "hl a")
        let b = dom.query(".hl").unwrap();
        assert_eq!(tag(&dom, b), "b");
        assert_eq!(dom.query(".a"), Some(b));
        // sem match
        assert_eq!(dom.query("#naoexiste"), None);
        assert_eq!(dom.query(".naoexiste"), None);
    }

    #[test]
    fn set_text_substitui_conteudo() {
        let mut dom = parse_html_to_dom("<p>antes <b>x</b></p>");
        let p = dom.query("p").unwrap();
        dom.set_text(p, "depois");
        assert_eq!(dom.node(p).children.len(), 1);
        assert_eq!(dom.node(dom.node(p).children[0]).kind, NodeKind::Text("depois".into()));
    }

    #[test]
    fn set_attr_cria_e_atualiza() {
        let mut dom = parse_html_to_dom("<div>x</div>");
        let div = dom.query("div").unwrap();
        dom.set_attr(div, "class", "card");
        assert_eq!(dom.node(div).attr("class"), Some("card"));
        dom.set_attr(div, "class", "card ativo"); // atualiza, não duplica
        assert_eq!(dom.node(div).attr("class"), Some("card ativo"));
        assert_eq!(dom.node(div).attrs.len(), 1);
    }

    #[test]
    fn create_e_append_child() {
        let mut dom = parse_html_to_dom("<ul></ul>");
        let ul = dom.query("ul").unwrap();
        let li = dom.create_element("li");
        dom.set_text(li, "novo item");
        dom.append_child(ul, li);
        assert_eq!(dom.node(ul).children, vec![li]);
        assert_eq!(dom.node(li).parent, Some(ul));
        assert_eq!(tag(&dom, li), "li");
    }

    #[test]
    fn append_move_de_pai_e_remove() {
        let mut dom = parse_html_to_dom("<div><span>x</span></div><section></section>");
        let div = dom.query("div").unwrap();
        let span = dom.query("span").unwrap();
        let section = dom.query("section").unwrap();
        // move o span do div para o section
        dom.append_child(section, span);
        assert!(dom.node(div).children.is_empty());
        assert_eq!(dom.node(section).children, vec![span]);
        assert_eq!(dom.node(span).parent, Some(section));
        // remove o span de vez
        dom.remove_node(span);
        assert!(dom.node(section).children.is_empty());
        assert_eq!(dom.node(span).parent, None);
    }

    #[test]
    fn append_nao_cria_ciclo() {
        let mut dom = parse_html_to_dom("<div><span>x</span></div>");
        let div = dom.query("div").unwrap();
        let span = dom.query("span").unwrap();
        // tentar pôr o div (ancestral) dentro do span deve ser ignorado.
        dom.append_child(span, div);
        assert_eq!(dom.node(div).parent, Some(dom.root)); // intacto
        assert!(dom.node(span).children.contains(&div) == false);
    }

    #[test]
    fn atributos_class_id_href_preservados() {
        let dom = parse_html_to_dom(
            "<div class='card' id=\"alvo\"><a href='https://x'>l</a></div>",
        );
        let div = dom.node(dom.root).children[0];
        assert_eq!(dom.node(div).attr("class"), Some("card"));
        assert_eq!(dom.node(div).attr("id"), Some("alvo"));
        assert_eq!(dom.node(div).attr("naoexiste"), None);
        let a = dom.node(div).children[0];
        assert_eq!(tag(&dom, a), "a");
        assert_eq!(dom.node(a).attr("href"), Some("https://x"));
    }

    #[test]
    fn atributos_variantes_aspas_e_booleano() {
        // aspas duplas, simples, sem aspas, e atributo sem valor.
        let dom = parse_html_to_dom("<input type=text value='oi' disabled checked=\"x\">");
        let inp = dom.node(dom.root).children[0];
        assert_eq!(dom.node(inp).attr("type"), Some("text"));   // sem aspas
        assert_eq!(dom.node(inp).attr("value"), Some("oi"));    // aspas simples
        assert_eq!(dom.node(inp).attr("disabled"), Some(""));   // booleano
        assert_eq!(dom.node(inp).attr("checked"), Some("x"));   // aspas duplas
        // `input` é void: não empilha, não tem filhos.
        assert!(dom.node(inp).children.is_empty());
    }

    #[test]
    fn valor_de_atributo_decodifica_entidades() {
        let dom = parse_html_to_dom("<a title='Tom &amp; Jerry'>x</a>");
        let a = dom.node(dom.root).children[0];
        assert_eq!(dom.node(a).attr("title"), Some("Tom & Jerry"));
    }

    #[test]
    fn dump_mostra_atributos() {
        let dom = parse_html_to_dom("<div class='card' id='x'>oi</div>");
        let esperado = "\
#document
  <div class=\"card\" id=\"x\">
    \"oi\"
";
        assert_eq!(dom.dump(), esperado);
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
