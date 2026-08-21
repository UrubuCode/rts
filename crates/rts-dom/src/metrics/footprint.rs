//! PEGADA de memória — quanto uma árvore ocupa, e onde.
//!
//! Contadores dizem quanto trabalho houve; fases, quanto tempo levou. Nenhum dos
//! dois vê o terceiro consumo: uma página de 5000 nós que fica aberta paga
//! memória a cada frame de animação, a cada classe trocada e a cada nó removido
//! — e paga em lugares que não são a árvore. Metade dos campos do `Dom` é
//! estado DERIVADO (memos, caches, índices, estilos interpolados), e é
//! exatamente aí que uma invalidação preguiçosa aparece como bytes em vez de
//! milissegundos.
//!
//! ## É uma ESTIMATIVA, e diz o que estima
//!
//! Não há alocador instrumentado aqui: o número é somado do que as estruturas
//! declaram — `size_of` do elemento vezes a CAPACIDADE do `Vec`/`HashMap`, mais
//! o conteúdo de cada `String` alcançável. Isso ignora o overhead do alocador e
//! a fragmentação, e um `HashMap` reserva mais do que a capacidade lógica; o
//! valor é um piso comparável entre execuções, não o RSS do processo. Para o
//! que ele serve — comparar páginas, e ver o derivado crescer sem a árvore
//! crescer — um piso comparável é o suficiente, e um número que se diz exato
//! sem ser é pior do que nenhum.

use crate::dom::{Dom, NodeKind};

/// Bytes por área, e a contagem que os produziu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Footprint {
    /// A arena: os `Node` mais os `Vec` de filhos e de atributos.
    pub arena: usize,
    /// O TEXTO propriamente dito: tags, valores de atributo, nós de texto.
    pub strings: usize,
    /// Índices `#id`/`.classe`.
    pub indices: usize,
    /// Memos de estilo (`computed_memo` + `base_memo`).
    pub style_memos: usize,
    /// Caches de layout (medição de bloco + largura intrínseca).
    pub layout_caches: usize,
    /// O stylesheet de autor parseado + o CSS bruto guardado.
    pub stylesheet: usize,
    /// Estado por nó que não é a árvore: listeners, inputs, imagens, animação.
    pub derived: usize,
    /// Quantas ENTRADAS há em cada área derivada, para ler o byte ao lado de um
    /// "quantos": 3 MB de memo é muito ou pouco dependendo de quantos nós há.
    pub entries_style_memos: usize,
    pub entries_layout_caches: usize,
    pub entries_indices: usize,
    pub entries_derived: usize,
    pub nodes: usize,
}

impl Footprint {
    pub fn total(&self) -> usize {
        self.arena
            + self.strings
            + self.indices
            + self.style_memos
            + self.layout_caches
            + self.stylesheet
            + self.derived
    }

    /// Relatório legível, ordenado do maior para o menor: a primeira linha é
    /// onde a memória está, que é a única pergunta que um total não responde.
    pub fn report(&self) -> String {
        let mut areas: Vec<(&str, usize, String)> = vec![
            ("árvore (arena)", self.arena, format!("{} nós", self.nodes)),
            ("texto (tags/attrs/conteúdo)", self.strings, String::new()),
            (
                "índices #id/.classe",
                self.indices,
                format!("{} entradas", self.entries_indices),
            ),
            (
                "memos de estilo",
                self.style_memos,
                format!("{} entradas", self.entries_style_memos),
            ),
            (
                "caches de layout",
                self.layout_caches,
                format!("{} entradas", self.entries_layout_caches),
            ),
            ("stylesheet + CSS bruto", self.stylesheet, String::new()),
            (
                "estado derivado por nó",
                self.derived,
                format!("{} entradas", self.entries_derived),
            ),
        ];
        areas.sort_by_key(|(_, b, _)| std::cmp::Reverse(*b));
        let total = self.total().max(1);
        let mut out = format!(
            "    pegada estimada: {} ({:.0} B por nó)\n",
            human(self.total()),
            self.total() as f64 / self.nodes.max(1) as f64
        );
        for (name, bytes, note) in areas {
            if bytes == 0 {
                continue;
            }
            out.push_str(&format!(
                "      {:<30} {:>10}  {:>5.1}%  {}\n",
                name,
                human(bytes),
                bytes as f64 * 100.0 / total as f64,
                note
            ));
        }
        out
    }
}

/// O TAMANHO das estruturas-chave, em bytes. Não é sobre uma página: é sobre o
/// código, e é o número que explica os outros — um `ComputedStyle` de 1 KB
/// significa que cada hit de memo copia 1 KB, e que cada regra CSS carrega dois
/// deles. Um clone barato e um clone caro têm exatamente a mesma cara no código.
pub fn type_sizes() -> Vec<(&'static str, usize)> {
    use crate::dom::{Attr, Node};
    use crate::style::ComputedStyle;
    vec![
        ("Node", std::mem::size_of::<Node>()),
        ("Attr", std::mem::size_of::<Attr>()),
        ("ComputedStyle", std::mem::size_of::<ComputedStyle>()),
        ("Rule (CSS)", crate::style::stylesheet::rule_size()),
        (
            "DisplayItem",
            std::mem::size_of::<crate::layout::DisplayItem>(),
        ),
    ]
}

fn human(bytes: usize) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KiB", b as f64 / 1024.0),
        b => format!("{:.2} MiB", b as f64 / (1024.0 * 1024.0)),
    }
}

/// Soma a pegada de uma árvore. O(n) sobre a arena.
pub fn footprint(dom: &Dom) -> Footprint {
    use crate::dom::{Attr, Node, NodeIdx};
    use crate::style::ComputedStyle;

    let mut f = Footprint {
        nodes: dom.nodes.len(),
        ..Default::default()
    };
    f.arena = dom.nodes.capacity() * std::mem::size_of::<Node>()
        + dom.layout_epoch_len() * std::mem::size_of::<u64>();

    for node in &dom.nodes {
        f.arena += node.children.capacity() * std::mem::size_of::<NodeIdx>()
            + node.attrs.capacity() * std::mem::size_of::<Attr>();
        f.strings += match &node.kind {
            NodeKind::Element { tag } => tag.capacity(),
            NodeKind::Text(t) | NodeKind::Comment(t) => t.capacity(),
            NodeKind::Document => 0,
        };
        for a in &node.attrs {
            f.strings += a.name.capacity() + a.value.capacity();
        }
    }

    let (id_index, class_index) = dom.debug_indices();
    for (key, bucket) in id_index.iter().chain(class_index.iter()) {
        f.entries_indices += bucket.len();
        f.indices += key.capacity()
            + bucket.capacity() * std::mem::size_of::<NodeIdx>()
            + std::mem::size_of::<String>();
    }

    // Memos e caches: o `Dom` enumera os próprios (tamanho de entrada + quantas),
    // pela mesma razão do `derived_node_state` — um campo novo que não for
    // acrescentado lá não ganha contagem, mas um removido não deixa esta
    // varredura mentindo sobre um campo que já não existe.
    let (memo_entries, cache_entries) = dom.derived_cache_sizes();
    f.entries_style_memos = memo_entries;
    f.style_memos =
        memo_entries * (std::mem::size_of::<ComputedStyle>() + std::mem::size_of::<NodeIdx>());
    f.entries_layout_caches = cache_entries;
    f.layout_caches = dom.layout_cache_bytes();

    f.stylesheet = dom.stylesheet_bytes();

    let derived = dom.derived_node_state();
    f.entries_derived = derived.len();
    // Uma entrada de estado derivado guarda pelo menos a chave e um valor do
    // tamanho de um `ComputedStyle` no pior caso (anim_override/prev_computed,
    // que são os maiores). Estimar pelo maior evita um número que parece pequeno
    // justamente na área que cresce sozinha.
    f.derived =
        derived.len() * (std::mem::size_of::<NodeIdx>() + std::mem::size_of::<ComputedStyle>());

    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_html_to_dom;

    /// A pegada acompanha a árvore: uma página maior ocupa mais, e o texto
    /// aparece separado da arena (senão "o DOM ocupa X" não diz se o problema é
    /// a estrutura ou o conteúdo).
    #[test]
    fn a_pegada_cresce_com_a_arvore_e_separa_texto_de_estrutura() {
        let pequena = footprint(&parse_html_to_dom("<p>a</p>"));
        let grande = footprint(&parse_html_to_dom(
            &"<div class=\"c\"><p>texto um pouco maior</p></div>".repeat(50),
        ));
        assert!(grande.total() > pequena.total() * 10);
        assert!(grande.strings > 0 && grande.arena > 0);
        assert!(grande.report().contains("por nó"));
    }

    /// O estado DERIVADO é o que cresce sem a árvore crescer — é o ponto todo
    /// desta medição. Um layout preenche memos; a árvore não muda.
    #[test]
    fn o_derivado_cresce_sem_a_arvore_crescer() {
        let dom = parse_html_to_dom("<div><p>um</p><p>dois</p></div>");
        let antes = footprint(&dom);
        let ctx = crate::layout::LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &crate::layout::ApproxMeasurer,
        };
        let _ = crate::layout::layout_document(&dom, &ctx);
        let depois = footprint(&dom);
        assert_eq!(antes.arena, depois.arena, "a árvore não mudou");
        assert!(depois.style_memos > antes.style_memos, "os memos encheram");
    }
}
