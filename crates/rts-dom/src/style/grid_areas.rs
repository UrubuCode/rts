//! ÁREAS NOMEADAS do grid (`grid-template-areas` + `grid-area: <nome>`).
//!
//! Por que um módulo próprio e não mais um braço em `values.rs`: o valor de
//! `grid-template-areas` não é um valor escalar como as trilhas — é uma MATRIZ de
//! nomes que só serve depois de reduzida ao retângulo de cada nome, e essa redução
//! (com a validação de que o nome forma mesmo um retângulo) é o grosso do código.
//! Guardar a matriz crua no `ComputedStyle` e reduzi-la no layout foi rejeitado:
//! o layout roda por frame e o parse roda uma vez por regra, então a redução
//! pertence ao parse.

use std::collections::HashMap;

/// O retângulo que UM nome ocupa na grade, em índices de célula 0-based e com o
/// fim EXCLUSIVO (`r0..r1`, `c0..c1`), que é a forma que o layout consome direto
/// para somar as trilhas do span.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GridArea {
    pub r0: usize,
    pub c0: usize,
    pub r1: usize,
    pub c1: usize,
}

impl GridArea {
    pub fn rows(&self) -> usize {
        self.r1 - self.r0
    }
    pub fn cols(&self) -> usize {
        self.c1 - self.c0
    }
}

/// `grid-template-areas` já REDUZIDO: quantas linhas/colunas a matriz declara e
/// onde cada nome vive. O `.` (célula vazia) não vira entrada nenhuma — ele só
/// existe para reservar espaço, e reservar espaço é o que o contador de
/// linhas/colunas já faz.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct GridAreas {
    pub rows: usize,
    pub cols: usize,
    named: HashMap<String, GridArea>,
}

impl GridAreas {
    /// O retângulo de um nome, ou `None` se a matriz não o declara — que é o caso
    /// em que o item volta para a colocação automática em vez de ir para uma
    /// célula inventada.
    pub fn area(&self, name: &str) -> Option<GridArea> {
        self.named.get(name).copied()
    }

    /// O nome que cobre a célula `(r, c)`, ou `None` se ela está vazia. Só serve à
    /// re-serialização (`getComputedStyle`) — o layout pergunta pelo nome, não pela
    /// célula, e é por isso que a estrutura guarda o retângulo e não a matriz.
    pub fn name_at(&self, r: usize, c: usize) -> Option<&str> {
        self.named
            .iter()
            .find(|(_, a)| r >= a.r0 && r < a.r1 && c >= a.c0 && c < a.c1)
            .map(|(n, _)| n.as_str())
    }

    /// Parseia `'a b' 'c d'` (ou com aspas duplas). Cada string é uma LINHA; os
    /// tokens separados por espaço são as colunas.
    ///
    /// Duas coisas que o browser faz e aqui são deliberadamente frouxas: uma linha
    /// com número de colunas diferente das outras é ERRO na spec (a declaração
    /// inteira cai) e aqui é tolerada, com a grade tomando o maior número de
    /// colunas; e um nome cujas células não formam um retângulo também é erro na
    /// spec, enquanto aqui vira o BOUNDING BOX das células. As duas são a mesma
    /// escolha: o motor está a renderizar páginas reais, e recusar a declaração
    /// inteira por um CSS torto perde o layout todo em vez de aproximá-lo.
    pub fn parse(v: &str) -> Option<GridAreas> {
        let rows_text = quoted_strings(v);
        if rows_text.is_empty() {
            return None;
        }
        let mut named: HashMap<String, GridArea> = HashMap::new();
        let mut cols = 0usize;
        for (r, line) in rows_text.iter().enumerate() {
            for (c, name) in line.split_whitespace().enumerate() {
                cols = cols.max(c + 1);
                // `.` e a sequência `...` são células explicitamente VAZIAS.
                if name.chars().all(|ch| ch == '.') {
                    continue;
                }
                match named.get_mut(name) {
                    None => {
                        named.insert(name.to_string(), GridArea { r0: r, c0: c, r1: r + 1, c1: c + 1 });
                    }
                    Some(a) => {
                        a.r0 = a.r0.min(r);
                        a.c0 = a.c0.min(c);
                        a.r1 = a.r1.max(r + 1);
                        a.c1 = a.c1.max(c + 1);
                    }
                }
            }
        }
        if named.is_empty() {
            return None;
        }
        Some(GridAreas { rows: rows_text.len(), cols: cols.max(1), named })
    }
}

/// Extrai as strings entre aspas (simples ou duplas) de um valor CSS. Serve os
/// dois consumidores: `grid-template-areas` puro e o shorthand `grid-template`,
/// onde as linhas de área vêm INTERCALADAS com os tamanhos de trilha.
pub fn quoted_strings(v: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<(char, String)> = None;
    for ch in v.chars() {
        match &mut cur {
            Some((q, buf)) => {
                if ch == *q {
                    out.push(std::mem::take(buf));
                    cur = None;
                } else {
                    buf.push(ch);
                }
            }
            None if ch == '\'' || ch == '"' => cur = Some((ch, String::new())),
            None => {}
        }
    }
    out
}

/// O valor COM as strings entre aspas REMOVIDAS. É o que torna o shorthand
/// `grid-template: "a b" 40px "c d" 1fr / 100px 1fr` parseável pelo mesmo código
/// que já lê `rows / cols`: tirar as áreas deixa exatamente `40px 1fr / 100px 1fr`.
pub fn strip_quoted(v: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    for ch in v.chars() {
        match quote {
            Some(q) if ch == q => {
                quote = None;
                out.push(' '); // separa os tamanhos que ladeavam a string
            }
            Some(_) => {}
            None if ch == '\'' || ch == '"' => quote = Some(ch),
            None => out.push(ch),
        }
    }
    out
}

/// `grid-area: <nome>` — só a forma de NOME ÚNICO.
///
/// A forma numérica (`grid-area: 1 / 2 / 3 / 4`, e as variantes com `span`) é
/// IGNORADA de propósito: aceitá-la sem a colocação por índice no layout daria um
/// nome que o layout procuraria na matriz de áreas e nunca acharia — silenciosamente
/// caindo na colocação automática, que é o que já acontece. Um valor com `/` ou que
/// comece por dígito devolve `None` aqui para que a intenção fique legível no parse
/// e não pareça um nome esquisito mais abaixo.
pub fn parse_grid_area_name(v: &str) -> Option<String> {
    let v = v.trim();
    if v.is_empty() || v.contains('/') || v.eq_ignore_ascii_case("auto") {
        return None;
    }
    let first = v.chars().next()?;
    if !(first.is_alphabetic() || first == '_' || first == '-') {
        return None;
    }
    if v.split_whitespace().count() != 1 {
        return None;
    }
    Some(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_repetido_em_celulas_adjacentes_forma_um_span() {
        let a = GridAreas::parse("'topo topo' 'lado conteudo' 'rodape rodape'").unwrap();
        assert_eq!((a.rows, a.cols), (3, 2));
        assert_eq!(a.area("topo").unwrap(), GridArea { r0: 0, c0: 0, r1: 1, c1: 2 });
        assert_eq!(a.area("lado").unwrap(), GridArea { r0: 1, c0: 0, r1: 2, c1: 1 });
        assert_eq!(a.area("conteudo").unwrap(), GridArea { r0: 1, c0: 1, r1: 2, c1: 2 });
        assert_eq!(a.area("rodape").unwrap().rows(), 1);
        assert_eq!(a.area("rodape").unwrap().cols(), 2);
        assert!(a.area("inexistente").is_none());
    }

    #[test]
    fn ponto_reserva_celula_sem_criar_nome() {
        let a = GridAreas::parse("\"a . b\"").unwrap();
        assert_eq!(a.cols, 3);
        assert!(a.area(".").is_none());
        assert_eq!(a.area("b").unwrap().c0, 2);
    }

    #[test]
    fn shorthand_sem_as_areas_ainda_e_rows_barra_cols() {
        assert_eq!(strip_quoted("\"a b\" 40px \"c d\" 1fr / 100px 1fr").split_whitespace().collect::<Vec<_>>(),
                   vec!["40px", "1fr", "/", "100px", "1fr"]);
    }

    #[test]
    fn grid_area_numerica_nao_vira_nome() {
        assert_eq!(parse_grid_area_name("pageContent").as_deref(), Some("pageContent"));
        assert!(parse_grid_area_name("1 / 2 / 3 / 4").is_none());
        assert!(parse_grid_area_name("auto").is_none());
    }
}
