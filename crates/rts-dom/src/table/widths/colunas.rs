//! Juntar as restrições de célula em colunas — a metade de `mod.rs` que não
//! lê a árvore (nem de nós nem de caixas), extraída para o manter abaixo do
//! tecto de 500 linhas depois de 02bc7088d somar a travessia pela árvore de
//! caixas à medição de célula.

use super::Coluna;

/// Junta as células numa lista de colunas: cada coluna fica com o maior mínimo e
/// o maior máximo das células que a ocupam SOZINHAS (colspan 1).
///
/// As células que atravessam colunas entram depois, e só para LEVANTAR o que já
/// existe — nunca para o baixar. É a regra que evita o efeito mais visível de um
/// algoritmo ingénuo: um cabeçalho com `colspan=3` a ditar a largura de três
/// colunas que o corpo da tabela já tinha dimensionado pelos seus dados.
pub(crate) fn colunas_min_max(
    cells: &[(usize, usize, Coluna)], // (coluna inicial, colspan, restrição da célula)
    cols: usize,
    spacing: f32,
) -> Vec<Coluna> {
    let mut out = vec![Coluna::default(); cols];
    for &(col, _span, mm) in cells.iter().filter(|c| c.1 <= 1) {
        if col < cols {
            out[col].absorve(mm);
        }
    }
    // Segunda passada: as que atravessam. O espaço que a célula precisa é o dela
    // MENOS o `border-spacing` que já existe entre as colunas atravessadas — esse
    // espaço é dela também.
    //
    // A CLASSE de uma célula que atravessa não passa para as colunas, e é de
    // propósito: no Blink a percentagem de uma célula com `colspan` é repartida
    // pelas colunas não-percentuais em proporção ao máximo delas, o que é uma
    // regra à parte e não uma absorção. Marcar as colunas como restringidas aqui
    // seria mais barato e daria a classe errada a todas.
    for &(col, span, mm) in cells.iter().filter(|c| c.1 > 1) {
        let fim = (col + span).min(cols);
        if col >= cols {
            continue;
        }
        let n = fim - col;
        let vaos = (n.saturating_sub(1)) as f32 * spacing;
        let atual_min: f32 = out[col..fim].iter().map(|c| c.min).sum::<f32>() + vaos;
        let atual_max: f32 = out[col..fim].iter().map(|c| c.max).sum::<f32>() + vaos;
        distribui_excedente(&mut out[col..fim], mm.min - atual_min, |c| &mut c.min);
        distribui_excedente(&mut out[col..fim], mm.max - atual_max, |c| &mut c.max);
        // O máximo nunca fica abaixo do mínimo depois de levantado.
        for c in &mut out[col..fim] {
            c.max = c.max.max(c.min);
        }
    }
    out
}

/// Espalha `extra` (se positivo) igualmente pelas colunas. Igualmente e não em
/// proporção porque, no ponto em que isto corre, a proporção seria contra
/// larguras que podem ser todas zero (uma linha inteira de células vazias sob um
/// cabeçalho com colspan), e uma proporção contra zero não distribui nada.
fn distribui_excedente(cols: &mut [Coluna], extra: f32, campo: impl Fn(&mut Coluna) -> &mut f32) {
    if extra <= 0.0 || cols.is_empty() {
        return;
    }
    // Uma célula com colspan não pode elevar uma coluna que já tem uma largura
    // declarada: essa coluna já foi classificada como restringida pelo conteúdo
    // da própria coluna e deve ficar congelada neste degrau. O excedente pertence
    // às colunas automáticas do intervalo. Quando todas são declaradas, não há
    // uma classe livre; nesse caso mantemos o fallback igualitário para preservar
    // o comportamento de uma tabela composta só por restrições.
    let livres: Vec<usize> = cols
        .iter()
        .enumerate()
        .filter_map(|(i, c)| (!c.restringida).then_some(i))
        .collect();
    let alvos: Vec<usize> = if livres.is_empty() {
        (0..cols.len()).collect()
    } else {
        livres
    };
    let quota = extra / alvos.len() as f32;
    for i in alvos {
        *campo(&mut cols[i]) += quota;
    }
}
