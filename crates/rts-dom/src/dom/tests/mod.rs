//! Os testes do `Dom`, movidos de `dom.rs` sem alteração de conteúdo.
//!
//! Aqui ficam apenas os HELPERS partilhados; cada área tem o seu submódulo.
//! Ver o cabeçalho de cada um sobre a indentação preservada.

mod animacao;
mod cascade;
mod consulta;
mod eventos;
mod invalidacao;
mod mutacao;
mod parser;

    use super::*;

    /// `true` se o nó tem estilo memoizado — o memo é um vetor esparso por
    /// índice da arena, então "tem entrada" é "o slot existe e está cheio".
    fn memoizado(dom: &Dom, idx: NodeIdx) -> bool {
        dom.computed_memo
            .borrow()
            .get(idx)
            .map(Option::is_some)
            .unwrap_or(false)
    }


    use super::*;

    /// Helper: nome de tag de um nó Element por índice cru (panica se não for
    /// elemento) — só para deixar os asserts curtos.
    fn tag(dom: &Dom, idx: NodeIdx) -> &str {
        match &dom.node(idx).kind {
            NodeKind::Element { tag } => tag,
            other => panic!("esperava Element, achei {other:?}"),
        }
    }


    /// Helper: resolve um `NodeId` versionado da API pública para o índice cru
    /// usado nos asserts de `children`/`parent`.
    fn idx(dom: &Dom, id: NodeId) -> NodeIdx {
        dom.resolve(id)
            .expect("NodeId deveria resolver nesta árvore")
    }


    /// Helper: o `<body>` IMPLÍCITO da árvore.
    ///
    /// O parser cria `<html>` e `<body>` quando o fonte não os escreve, que é o
    /// que qualquer browser faz. Não fazê-lo era um defeito real: sem `<body>`
    /// na árvore, uma regra `body{…}` não casava com elemento nenhum e TODA a
    /// propriedade herdada declarada aí — cor, fonte, `line-height`,
    /// alinhamento — desaparecia em silêncio. A herança funcionava; o ancestral
    /// é que não existia.
    ///
    /// Os testes abaixo continuam a pinar o mesmo que pinavam; só a NAVEGAÇÃO
    /// mudou, porque o que era filho do `#document` é hoje neto dele.
    fn body_idx(dom: &Dom) -> NodeIdx {
        let html = dom.node(dom.root).children[0];
        dom.node(html).children[0]
    }


    /// Helper: os filhos de topo do FLUXO — hoje filhos do `<body>` implícito,
    /// onde antes eram filhos do `#document`. Ver [`body_idx`].
    fn topo(dom: &Dom) -> &Vec<NodeIdx> {
        &dom.node(body_idx(dom)).children
    }
