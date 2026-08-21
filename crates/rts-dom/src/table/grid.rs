use super::*;

/// O `display` de um nó já com o default da UA aplicado, ou `None` para nós que
/// não são elementos.
fn display_of(dom: &Dom, id: NodeIdx) -> Option<DisplayKind> {
    crate::layout::used_display(dom, id)
}

/// Constrói a grade a partir da árvore. Um `<tbody>` implícito não existe no
/// nosso DOM (o parser não o cria), por isso as linhas podem estar tanto dentro
/// de um grupo como soltas — os dois casos caem no mesmo laço.
pub(in crate::table) fn collect(dom: &Dom, table: NodeIdx) -> Grid {
    let mut g = Grid {
        rows: Vec::new(),
        groups: Vec::new(),
        outros: Vec::new(),
        cols: 0,
    };
    // `ocupado[coluna]` = quantas linhas ainda faltam para o `rowspan` que passa
    // por ali terminar. É o que faz uma célula da linha de baixo saltar a coluna
    // que a de cima ainda ocupa — sem isto, `rowspan` desalinha tudo o que vem
    // depois, que é o modo mais visível de uma tabela estar errada.
    let mut ocupado: Vec<usize> = Vec::new();

    // Células que apareceram sem `table-row` por pai e ainda esperam pela linha
    // ANÓNIMA que as vai conter. Só células CONSECUTIVAS entram na mesma linha:
    // é o que a spec manda, e é o que faz `célula, linha, célula` dar três
    // linhas em vez de duas células na mesma.
    let mut soltas: Vec<NodeIdx> = Vec::new();
    macro_rules! fechar_anonima {
        () => {
            if !soltas.is_empty() {
                add_row(dom, None, &soltas, &mut g, &mut ocupado);
                soltas.clear();
            }
        };
    }

    for &child in &dom.node(table).children {
        // Um filho que não é elemento (o whitespace entre `<tr>`) não é nada. É
        // preciso perguntá-lo ANTES do display: `display_of` responde `None`
        // tanto para o whitespace como para um `<figcaption>` sem default de UA,
        // e tratar os dois como "nada" era o que apagava o segundo.
        if !matches!(dom.node(child).kind, crate::NodeKind::Element { .. }) {
            continue;
        }
        match display_of(dom, child) {
            Some(DisplayKind::TableCell) => soltas.push(child),
            Some(DisplayKind::TableCaption) => {
                fechar_anonima!();
                g.outros.push(child);
            }
            Some(DisplayKind::TableRow) => {
                fechar_anonima!();
                add_row(
                    dom,
                    Some(child),
                    &celulas_de(dom, child),
                    &mut g,
                    &mut ocupado,
                );
            }
            Some(DisplayKind::TableRowGroup) => {
                fechar_anonima!();
                let inicio = g.rows.len();
                // O grupo também pode trazer células soltas — mesma regra, e o
                // mesmo motivo de a resolver aqui dentro em vez de achatar a
                // árvore antes: achatar perderia a fronteira do grupo, que é o
                // que dá a caixa ao `<tbody>`.
                let mut soltas_g: Vec<NodeIdx> = Vec::new();
                for &r in &dom.node(child).children {
                    if !matches!(dom.node(r).kind, crate::NodeKind::Element { .. }) {
                        continue;
                    }
                    match display_of(dom, r) {
                        Some(DisplayKind::TableCell) => soltas_g.push(r),
                        Some(DisplayKind::TableRow) => {
                            if !soltas_g.is_empty() {
                                add_row(dom, None, &soltas_g, &mut g, &mut ocupado);
                                soltas_g.clear();
                            }
                            add_row(dom, Some(r), &celulas_de(dom, r), &mut g, &mut ocupado);
                        }
                        _ => {}
                    }
                }
                if !soltas_g.is_empty() {
                    add_row(dom, None, &soltas_g, &mut g, &mut ocupado);
                }
                g.groups.push((child, inicio, g.rows.len() - inicio));
            }
            Some(DisplayKind::None) => {}
            // QUALQUER outro filho — um `<div>` solto, a `<a><img></a>` de uma
            // miniatura — é envolvido numa CÉLULA anónima (§17.2.1), e não
            // empilhado por cima da grade como se fosse uma legenda.
            //
            // É o que dá largura a `figure { display: table }`, o padrão das
            // miniaturas da Wikipédia: sem célula anónima a tabela não tinha
            // coluna nenhuma, a soma das colunas era zero, e o *shrink-to-fit*
            // dava-lhe 0px de largura. Foram 3 tabelas a desaparecer da página e
            // as 24 `<figcaption>` com elas.
            //
            // O nó da célula anónima é o PRÓPRIO filho: uma célula anónima não
            // é um elemento, não tem estilo e não pinta nada, portanto inventar
            // um nó para ela só acrescentaria uma caixa que o documento não tem.
            _ => {
                if !crate::layout::is_out_of_flow(dom, child) {
                    soltas.push(child);
                }
            }
        }
    }
    fechar_anonima!();
    g
}

/// Acrescenta uma linha à grade a partir das CÉLULAS dela. Recebe as células e
/// não o nó da linha porque uma linha anónima não tem nó — e porque o que a
/// grade precisa de saber de uma linha são as suas células.
fn add_row(
    dom: &Dom,
    node: Option<NodeIdx>,
    celulas: &[NodeIdx],
    g: &mut Grid,
    ocupado: &mut Vec<usize>,
) {
    // Consome uma linha de cada `rowspan` pendente antes de colocar as células.
    for o in ocupado.iter_mut() {
        *o = o.saturating_sub(1);
    }
    let mut cells = Vec::new();
    let mut col = 0usize;
    for &c in celulas {
        while ocupado.get(col).copied().unwrap_or(0) > 0 {
            col += 1;
        }
        let colspan = atributo_span(dom, c, "colspan");
        let rowspan = atributo_span(dom, c, "rowspan");
        if ocupado.len() < col + colspan {
            ocupado.resize(col + colspan, 0);
        }
        for o in &mut ocupado[col..col + colspan] {
            *o = rowspan;
        }
        cells.push(Cell {
            node: c,
            col,
            colspan,
            rowspan,
        });
        col += colspan;
    }
    g.cols = g.cols.max(col);
    g.rows.push(Row { node, cells });
}

/// As células FILHAS de um nó, na ordem do documento.
fn celulas_de(dom: &Dom, pai: NodeIdx) -> Vec<NodeIdx> {
    dom.node(pai)
        .children
        .iter()
        .copied()
        .filter(|&c| display_of(dom, c) == Some(DisplayKind::TableCell))
        .collect()
}

/// `colspan`/`rowspan`: pelo menos 1, e com teto. O teto não é decoração — um
/// `rowspan="100000000"` (que existe em páginas reais e em ataques de DoS de
/// layout) alocaria a grade inteira em memória.
fn atributo_span(dom: &Dom, id: NodeIdx, nome: &str) -> usize {
    dom.node(id)
        .attr(nome)
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, 1000)
}

