//! A REPARTIÇÃO propriamente dita: dada uma lista de colunas já medidas e a
//! largura disponível, quanto leva cada uma.
//!
//! Está separada da medição (o módulo pai) porque são duas perguntas com fontes
//! diferentes: a medição lê a árvore e o estilo, esta não lê nada — recebe
//! números e devolve números. É o que a torna testável com uma lista escrita à
//! mão, e é onde a repartição por CLASSE vai crescer.

use super::Coluna;

/// A largura USADA de cada coluna, dada a largura de conteúdo disponível para as
/// colunas (já sem os `border-spacing`).
///
/// Três regimes, e é aqui que vive a regra que abre este ficheiro:
/// - não cabe nem o mínimo → cada coluna fica com o seu mínimo e a tabela
///   transborda (é o que o browser faz: uma tabela não encolhe o conteúdo
///   abaixo do indivisível);
/// - cabe o máximo → o que sobra vai proporcionalmente ao máximo, porque uma
///   tabela sem `width` que já está satisfeita distribui a sobra pelas colunas
///   que mais conteúdo têm;
/// - entre os dois → cada coluna recebe o seu mínimo mais a sua fatia da FOLGA.
pub(crate) fn resolve_colunas(cols: &[Coluna], disponivel: f32) -> Vec<f32> {
    let total_min: f32 = cols.iter().map(|c| c.min).sum();
    let total_max: f32 = cols.iter().map(|c| c.max).sum();
    if cols.is_empty() {
        return Vec::new();
    }
    if disponivel <= total_min || total_max <= total_min {
        // Sem folga nenhuma (todas as colunas fixas) a repartição por folga
        // dividiria por zero; o mínimo já é a resposta.
        if total_max <= total_min && disponivel > total_min && total_min > 0.0 {
            let k = disponivel / total_min;
            return cols.iter().map(|c| c.min * k).collect();
        }
        return cols.iter().map(|c| c.min).collect();
    }
    if disponivel >= total_max {
        if total_max <= 0.0 {
            let q = disponivel / cols.len() as f32;
            return vec![q; cols.len()];
        }
        let sobra = disponivel - total_max;
        return cols
            .iter()
            .map(|c| c.max + sobra * (c.max / total_max))
            .collect();
    }
    let folga_total = total_max - total_min;
    let a_dividir = disponivel - total_min;
    cols.iter()
        .map(|c| c.min + a_dividir * ((c.max - c.min) / folga_total))
        .collect()
}

/// `table-layout: fixed` — as larguras vêm da PRIMEIRA linha (e dos `<col>`), o
/// conteúdo não é consultado, e o que sobra divide-se igualmente pelas colunas
/// sem largura declarada. É o algoritmo que existe para ser rápido: nenhuma
/// célula é medida.
///
/// `declaradas[i] == None` = coluna sem largura declarada.
pub(crate) fn resolve_fixo(declaradas: &[Option<f32>], disponivel: f32) -> Vec<f32> {
    let soma: f32 = declaradas.iter().flatten().sum();
    let livres = declaradas.iter().filter(|d| d.is_none()).count();
    if livres == 0 {
        // Todas declaradas: escalam para encher a largura da tabela (a spec manda
        // a tabela ficar com a largura pedida, não com a soma das colunas).
        if soma <= 0.0 {
            return vec![0.0; declaradas.len()];
        }
        let k = disponivel / soma;
        return declaradas.iter().map(|d| d.unwrap_or(0.0) * k).collect();
    }
    let quota = ((disponivel - soma) / livres as f32).max(0.0);
    declaradas.iter().map(|d| d.unwrap_or(quota)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::widths::colunas_min_max;

    fn mm(min: f32, max: f32) -> Coluna {
        Coluna {
            min,
            max,
            ..Coluna::default()
        }
    }

    #[test]
    fn a_folga_e_repartida_e_nao_o_maximo() {
        // Uma coluna estreita e satisfeita (10..12) ao lado de uma esfomeada
        // (10..110): 100px de folga total, 40 para dividir.
        let w = resolve_colunas(&[mm(10.0, 12.0), mm(10.0, 110.0)], 60.0);
        // A estreita leva 2% dos 40 (a sua folga é 2 de 102), não 10%.
        assert!((w[0] - 10.78).abs() < 0.05, "coluna estreita = {}", w[0]);
        assert!((w[1] - 49.22).abs() < 0.05, "coluna larga = {}", w[1]);
        assert!((w[0] + w[1] - 60.0).abs() < 0.01);
    }

    #[test]
    fn abaixo_do_minimo_a_tabela_transborda_em_vez_de_esmagar() {
        let w = resolve_colunas(&[mm(50.0, 80.0), mm(50.0, 80.0)], 40.0);
        assert_eq!(w, vec![50.0, 50.0]);
    }

    #[test]
    fn colspan_so_levanta_colunas_nunca_as_baixa() {
        // Duas colunas já com 100 cada, e um cabeçalho colspan=2 que só quer 50.
        let cols = colunas_min_max(
            &[
                (0, 1, mm(100.0, 100.0)),
                (1, 1, mm(100.0, 100.0)),
                (0, 2, mm(50.0, 50.0)),
            ],
            2,
            0.0,
        );
        assert_eq!(cols[0].min, 100.0);
        assert_eq!(cols[1].min, 100.0);
    }

    #[test]
    fn colspan_maior_que_as_colunas_reparte_o_que_falta_por_igual() {
        let cols = colunas_min_max(&[(0, 2, mm(100.0, 100.0))], 2, 0.0);
        assert_eq!(cols[0].min, 50.0);
        assert_eq!(cols[1].min, 50.0);
    }

    #[test]
    fn fixo_divide_o_resto_pelas_colunas_sem_largura() {
        let w = resolve_fixo(&[Some(100.0), None, None], 300.0);
        assert_eq!(w, vec![100.0, 100.0, 100.0]);
    }

    #[test]
    fn fixo_com_todas_declaradas_escala_para_a_largura_da_tabela() {
        let w = resolve_fixo(&[Some(50.0), Some(50.0)], 200.0);
        assert_eq!(w, vec![100.0, 100.0]);
    }
}
