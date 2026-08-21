//! As trilhas de `grid-template-*`
//!
//! Extraído de `values.rs` sem alterar uma linha.

use super::*;

/// Uma TRILHA de grid (`grid-template-columns`/`-rows`, `grid-auto-rows`): o
/// tamanho de uma coluna/linha. `Px` fixo, `Fr` fração do espaço livre, `Auto`
/// dimensiona pelo conteúdo, `Percent` do container. A resolução (px → fr → auto)
/// vive no layout (algoritmo de track sizing). Egui-free.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GridTrack {
    /// `100px`, `2rem`… — tamanho absoluto (já resolvido a px no parse quando
    /// possível; `em`/`rem`/`vw` resolvem no layout via a Dimension).
    Fixed(Dimension),
    /// `1fr`, `2fr` — fração do espaço livre (após px/auto). O nº é o peso.
    Fr(f32),
    /// `auto`/`min-content`/`max-content` — dimensiona pelo CONTEÚDO dos itens
    /// da trilha (sem distinção entre min e max: a diferença entre os três é o
    /// que fazem com o espaço que SOBRA, e isso decide-se na repartição).
    Auto,
    /// `minmax(<len>, <len>)` — uma trilha que parte do mínimo e CRESCE até ao
    /// máximo com o espaço que sobrar.
    ///
    /// Existe porque tratá-la como o seu MÁXIMO — que era a aproximação v1 — não
    /// é uma aproximação, é a resposta errada sempre que há outra trilha ao lado:
    /// a trilha come o máximo, não sobra nada para as outras, e a grade
    /// transborda. Medido na Wikipédia, cujo `<main>` é
    /// `minmax(0,59.25rem) min-content`: dávamos 948px (59.25rem) à coluna de
    /// conteúdo onde o Chrome dá 752, e a barra lateral saía fora da janela.
    /// Foram 196px de erro herdados por tudo o que está dentro do artigo,
    /// incluindo 46 das 49 tabelas da página.
    ///
    /// `minmax(x, 1fr)` e `minmax(x, min-content)` NÃO passam por aqui: o máximo
    /// deles já é uma trilha flexível ou intrínseca, e o parse devolve essa.
    Bounded { min: Dimension, max: Dimension },
}

impl GridTrack {
    /// Parseia UMA trilha: `100px`/`50%`/`1fr`/`auto`/`minmax(a,b)` (minmax → o
    /// MÁXIMO, aproximação v1). `None` se não reconhece.
    pub fn parse_one(v: &str) -> Option<GridTrack> {
        let v = v.trim();
        let low = v.to_ascii_lowercase();
        if low == "auto" || low == "min-content" || low == "max-content" {
            return Some(GridTrack::Auto);
        }
        if let Some(n) = low.strip_suffix("fr") {
            return n.trim().parse::<f32>().ok().map(GridTrack::Fr);
        }
        // `minmax(min, max)`. Quando o MÁXIMO é `fr` ou intrínseco, a trilha É
        // essa — um `minmax(0,1fr)` é uma trilha `1fr` cujo mínimo é zero, e o
        // mínimo zero é o que ela já faria. Só quando os dois lados são
        // comprimentos é que o par importa, e aí a trilha é limitada.
        if let Some(inner) = low
            .strip_prefix("minmax(")
            .and_then(|s| s.strip_suffix(')'))
        {
            let parts: Vec<&str> = inner.splitn(2, ',').collect();
            if parts.len() == 2 {
                let max = GridTrack::parse_one(parts[1])?;
                return Some(match (GridTrack::parse_one(parts[0]), max) {
                    (Some(GridTrack::Fixed(mn)), GridTrack::Fixed(mx)) => {
                        GridTrack::Bounded { min: mn, max: mx }
                    }
                    (_, outro) => outro,
                });
            }
        }
        // fit-content(x) → x fixo (aproximação).
        if let Some(inner) = low
            .strip_prefix("fit-content(")
            .and_then(|s| s.strip_suffix(')'))
        {
            return GridTrack::parse_one(inner);
        }
        super::lengths::parse_dimension_pub(v).map(GridTrack::Fixed)
    }

    /// Parseia uma LISTA de trilhas (`grid-template-columns`), expandindo
    /// `repeat(N, tracks…)`. Devolve o Vec de trilhas na ordem. Vazio → None.
    pub fn parse_list(v: &str) -> Option<Vec<GridTrack>> {
        let v = v.trim();
        if v.is_empty() || v.eq_ignore_ascii_case("none") {
            return None;
        }
        let mut out = Vec::new();
        // tokeniza respeitando parênteses (repeat/minmax têm vírgulas internas).
        for tok in split_top_level(v) {
            let t = tok.trim();
            let low = t.to_ascii_lowercase();
            if let Some(inner) = low
                .strip_prefix("repeat(")
                .and_then(|s| s.strip_suffix(')'))
            {
                // repeat(N, tracks) — N vezes as trilhas internas.
                let mut parts = inner.splitn(2, ',');
                let count = parts.next().unwrap_or("").trim();
                let tracks = parts.next().unwrap_or("").trim();
                // `auto-fill`/`auto-fit`: v1 usa 1 repetição (sem cálculo de quantas
                // cabem — aproximação; a maioria das páginas usa repeat(N,...) fixo).
                let n: usize = count.parse().unwrap_or(1);
                if let Some(inner_tracks) = GridTrack::parse_list(tracks) {
                    for _ in 0..n.max(1) {
                        out.extend(inner_tracks.iter().copied());
                    }
                }
            } else if let Some(track) = GridTrack::parse_one(t) {
                out.push(track);
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

/// Tokeniza uma lista separada por espaços RESPEITANDO parênteses (para não
/// quebrar `repeat(3, 1fr)` / `minmax(0, 1fr)` na vírgula/espaço internos).
fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' => {
                depth -= 1;
                cur.push(ch);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
