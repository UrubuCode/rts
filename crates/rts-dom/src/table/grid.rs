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
        col_bg: Vec::new(),
    };
    // Cursor de coluna para os `<col>`/`<colgroup>` — só avança enquanto eles
    // aparecem, na ordem do documento (HTML só os permite ANTES das linhas).
    let mut col_cursor = 0usize;
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
        // Um filho que não é elemento é o whitespace entre `<tr>` — nada — OU
        // texto a sério (`<div style="display:table">abc</div>`), que a spec
        // embrulha numa célula anónima como a qualquer elemento solto; o nó da
        // célula é o próprio texto, como já é o próprio elemento mais abaixo.
        // É preciso perguntá-lo ANTES do display: `display_of` responde `None`
        // tanto para o whitespace como para um `<figcaption>` sem default de UA,
        // e tratar os dois como "nada" era o que apagava o segundo.
        match &dom.node(child).kind {
            crate::NodeKind::Element { .. } => {}
            crate::NodeKind::Text(t) if !t.trim().is_empty() => {
                soltas.push(child);
                continue;
            }
            _ => continue,
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
            // `table-column`/`table-column-group` não geram caixa (o `display`
            // computa `None` — `style/parse/mod.rs`), mas o `background` que
            // declaram pinta-se atrás das células da coluna que atravessam
            // (CSS 2.1 §17.5.1); um `display:none` de verdade não carrega
            // nenhum dos dois campos que os distingue.
            Some(DisplayKind::None) => match col_kind(dom, child) {
                ColKind::Group => col_cursor = coleta_grupo_de_colunas(dom, child, col_cursor, &mut g.col_bg),
                ColKind::Column => {
                    let span = atributo_span(dom, child, "span");
                    g.col_bg.push((col_cursor, col_cursor + span, child));
                    col_cursor += span;
                }
                ColKind::Nenhum => {}
            },
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
            //
            // Um FLUTUADO (`float:left/right`) sai do mesmo jeito que um
            // `position:absolute`/`fixed` já saía por `is_out_of_flow`: os
            // dois são fora do fluxo normal, e um filho fora do fluxo nunca
            // entra na fusão de células soltas em linha anónima (§17.2.1 só
            // fala de conteúdo NO FLUXO). Sem isto, `float-applies-to-001`
            // (WPT) — um `table-row-group` flutuado cujas PRÓPRIAS linhas são
            // filhas dele, não da tabela — virava a única célula de uma
            // tabela de 1 coluna, esticada à largura inteira: a mesma caixa
            // completa, só que dentro da grade errada em vez de fora dela.
            // Vai para `g.outros` — a mesma lista do `<caption>` — porque um
            // flutuado empilha-se ao lado da grade, não dentro dela; quem
            // decide onde exatamente é `layout_table` (vê o `float` antes de
            // chamar `layout_block`, e faz a grade DESCER até limpar a base
            // de qualquer flutuado — CSS 2.1 §9.5: a grade ocupa a largura
            // TODA, então nunca há banda livre ao lado de um flutuado que a
            // deixe ficar onde estava; medido contra o Blink em
            // `claude-table-outros-flutuante`).
            _ => {
                let flutua = dom
                    .computed_style_idx(child)
                    .and_then(|c| c.float_side)
                    .is_some_and(|f| f != crate::style::FloatSide::None);
                if crate::layout::is_out_of_flow(dom, child) {
                    // posicionado: nem outros, nem grade — a passada
                    // out-of-flow de `layout_document` trata dele.
                } else if flutua {
                    fechar_anonima!();
                    g.outros.push(child);
                } else {
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

/// `colspan`/`rowspan`/`span`: pelo menos 1, e com teto. O teto não é
/// decoração — um `rowspan="100000000"` (que existe em páginas reais e em
/// ataques de DoS de layout) alocaria a grade inteira em memória.
fn atributo_span(dom: &Dom, id: NodeIdx, nome: &str) -> usize {
    dom.node(id)
        .attr(nome)
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, 1000)
}

/// O que um nó de `display: None` computado realmente É: um `table-column`,
/// um `table-column-group`, ou um `display:none` de verdade. A diferença vive
/// nos dois campos que `style/parse/fluxo.rs` guarda ao lado do `display` —
/// `flow_root` já faz o mesmo truque para `flow-root` vs `block`.
enum ColKind {
    Group,
    Column,
    Nenhum,
}

fn col_kind(dom: &Dom, id: NodeIdx) -> ColKind {
    match dom.computed_style_idx(id) {
        Some(css) if css.table_column_group.unwrap_or(false) => ColKind::Group,
        Some(css) if css.table_column.unwrap_or(false) => ColKind::Column,
        _ => ColKind::Nenhum,
    }
}

/// Um `table-column-group` (`<colgroup>`): entra no fundo primeiro pelo seu
/// PRÓPRIO intervalo (a soma dos `<col>` filhos, ou o seu `span` se não tiver
/// nenhum — o mesmo cálculo que a spec faz para a LARGURA, aqui reusado para
/// a coluna que o fundo cobre), e cada `<col>` filho entra a seguir pelo seu
/// próprio intervalo mais estreito — a ordem em que `pinta_fundo_de_coluna`
/// os lê é a ordem em que os pinta, então o grupo fica ATRÁS da sua coluna.
/// Devolve a nova posição do cursor (`start` + colunas consumidas).
fn coleta_grupo_de_colunas(
    dom: &Dom,
    group: NodeIdx,
    start: usize,
    col_bg: &mut Vec<(usize, usize, NodeIdx)>,
) -> usize {
    let mut filhos: Vec<(usize, NodeIdx)> = Vec::new();
    for &c in &dom.node(group).children {
        if matches!(dom.node(c).kind, crate::NodeKind::Element { .. })
            && matches!(col_kind(dom, c), ColKind::Column)
        {
            filhos.push((atributo_span(dom, c, "span"), c));
        }
    }
    let total: usize = if filhos.is_empty() {
        atributo_span(dom, group, "span")
    } else {
        filhos.iter().map(|(s, _)| s).sum()
    };
    col_bg.push((start, start + total, group));
    let mut cursor = start;
    for (span, node) in filhos {
        col_bg.push((cursor, cursor + span, node));
        cursor += span;
    }
    start + total
}

