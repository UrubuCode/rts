//! As trilhas de `grid-template-*`
//!
//! Extraído de `values.rs` sem alterar uma linha.

use super::*;

/// Um dos dois lados de `minmax()`/`fit-content()` quando pelo menos um NÃO é
/// um comprimento fixo puro — `Bounded` (abaixo) já serve o par fixo/fixo, que
/// é a maioria dos casos do corpus, e continua a existir por isso.
///
/// Um enum PRÓPRIO em vez de `Box<GridTrack>` recursivo: um lado de `minmax()`
/// nunca é ele próprio `minmax()`/`repeat()` — a spec não o permite — e um
/// enum plano deixa o resto de `GridTrack` como estava. Só `AutoRepeat`
/// (abaixo) tira `GridTrack` de `Copy`; sem ele este tipo bastava.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TrackBound {
    /// `100px`, `2rem`, `0`…
    Fixed(Dimension),
    /// `min-content` — a palavra mais larga do conteúdo da trilha (o piso que
    /// `table::min_content` já calcula para o `flex-shrink`, reusado aqui).
    MinContent,
    /// `max-content`, ou `auto` do lado do MÁXIMO — a largura sem quebra
    /// nenhuma (`layout::medida::intrinsic_outer_width`).
    MaxContent,
    /// `fit-content(<len>)` do lado do MÁXIMO: `min(<len>, max-content)` —
    /// cresce com o conteúdo até `<len>`, e não além disso mesmo que o
    /// conteúdo pedisse mais.
    FitContent(Dimension),
}

/// Uma TRILHA de grid (`grid-template-columns`/`-rows`, `grid-auto-rows`): o
/// tamanho de uma coluna/linha. `Px` fixo, `Fr` fração do espaço livre, `Auto`
/// dimensiona pelo conteúdo, `Percent` do container. A resolução (px → fr → auto)
/// vive no layout (algoritmo de track sizing). Egui-free.
#[derive(Clone, PartialEq, Debug)]
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
    /// `minmax(x, 1fr)` NÃO passa por aqui: o máximo já é uma trilha flexível,
    /// e o parse devolve essa (`GridTrack::Fr`) — aproximação mantida do v1,
    /// coberta por `minmax_com_maximo_flexivel_e_a_trilha_flexivel`.
    Bounded { min: Dimension, max: Dimension },
    /// `minmax(<bound>, <bound>)` com pelo menos um lado intrínseco (não os
    /// dois fixos — isso é `Bounded`), e `fit-content(<len>)` — que É
    /// `minmax(min-content, fit-content(<len>))` na letra da spec (CSS Grid 1
    /// §7.2.1/§7.2.3), por isso reusa a mesma trilha em vez de um variant à
    /// parte. A BASE é `min` avaliado contra o conteúdo da trilha; o TECTO é
    /// `max` — e se a base exceder o tecto (a palavra mais larga não cabe no
    /// máximo declarado), a base vence (spec §11.1: "if the growth limit is
    /// less than the base size, increase the growth limit to match").
    Intrinsic { min: TrackBound, max: TrackBound },
    /// `repeat(auto-fill|auto-fit, <tracks>)` — quantas vezes repetir é uma
    /// pergunta de LAYOUT (depende do espaço disponível), ao contrário de
    /// `repeat(N, …)`, que `parse_list` já expande aqui mesmo. Fica por
    /// expandir até `layout::grid_tracks::expand_auto_repeat`, que o resolve
    /// contra `content_w`. `fit` distingue `auto-fit` (colapsa trilhas vazias
    /// a 0) de `auto-fill` (mantém-nas, mesmo vazias).
    ///
    /// É o único variant que tira `GridTrack` de `Copy` — carrega um `Vec`.
    /// A alternativa considerada foi guardar o `content_w` de contagem já
    /// resolvido no VALOR CSS (ao estilo de `grid_column_tracks` da display
    /// list); rejeitada porque o valor CSS é por REGRA e o `content_w` é por
    /// CAIXA — duas instâncias do mesmo elemento com containers diferentes
    /// partilhariam o mesmo `ComputedStyle` e uma delas mentiria.
    ///
    /// `count_unit` é a contribuição do padrão para a CONTAGEM de repetições
    /// (§7.2.3.3), pré-calculada AQUI — no parse — porque é a única altura em
    /// que o texto de `minmax(150px, 1fr)` ainda tem os dois lados: depois de
    /// `tracks` estar construído, um `minmax` de máximo `fr` já colapsou à
    /// trilha flexível pura (a aproximação certa para o SIZING, ver o
    /// variant `Bounded`), e o mínimo — a única coisa que a contagem
    /// pergunta — já não estaria lá para reler.
    AutoRepeat {
        tracks: Vec<GridTrack>,
        fit: bool,
        count_unit: f32,
    },
}

impl GridTrack {
    /// Parseia UMA trilha: `100px`/`50%`/`1fr`/`auto`/`minmax(a,b)`/
    /// `fit-content(x)`. `None` se não reconhece.
    pub fn parse_one(v: &str) -> Option<GridTrack> {
        let v = v.trim();
        let low = v.to_ascii_lowercase();
        if low == "auto" || low == "min-content" || low == "max-content" {
            return Some(GridTrack::Auto);
        }
        if let Some(n) = low.strip_suffix("fr") {
            return n.trim().parse::<f32>().ok().map(GridTrack::Fr);
        }
        // `minmax(min, max)`. Quando o MÁXIMO é `fr`, a trilha É essa — um
        // `minmax(0,1fr)` é uma trilha `1fr` cujo mínimo é zero, e o mínimo
        // zero é o que ela já faria (o `resolve_tracks` dá tudo o livre ao
        // `fr` quando existe algum na lista — a base do `minmax` nunca chega
        // a ser consultada). Fora disso: os dois lados fixos → `Bounded`
        // (o caso mais comum, caminho antigo intacto); qualquer lado
        // intrínseco (`min-content`/`max-content`/`auto`/`fit-content()`) →
        // `Intrinsic`, resolvida contra o conteúdo em `layout::grid_tracks`.
        if let Some(inner) = low
            .strip_prefix("minmax(")
            .and_then(|s| s.strip_suffix(')'))
        {
            let parts: Vec<&str> = inner.splitn(2, ',').collect();
            if parts.len() == 2 {
                let min_s = parts[0].trim();
                let max_s = parts[1].trim();
                if let Some(n) = max_s
                    .strip_suffix("fr")
                    .and_then(|n| n.trim().parse::<f32>().ok())
                {
                    return Some(GridTrack::Fr(n));
                }
                let min_b = parse_track_bound(min_s)?;
                let max_b = parse_track_bound(max_s)?;
                return Some(match (min_b, max_b) {
                    (TrackBound::Fixed(mn), TrackBound::Fixed(mx)) => {
                        GridTrack::Bounded { min: mn, max: mx }
                    }
                    (min, max) => GridTrack::Intrinsic { min, max },
                });
            }
        }
        // `fit-content(<len>)` É `minmax(min-content, fit-content(<len>))` na
        // letra da spec (§7.2.1) — cresce com o conteúdo até `<len>` e não
        // além. A aproximação v1 (`x` fixo) dava a MESMA largura a uma célula
        // vazia e a uma com texto que a enche — errado nas duas direções.
        if let Some(inner) = low
            .strip_prefix("fit-content(")
            .and_then(|s| s.strip_suffix(')'))
        {
            let d = super::lengths::parse_dimension_pub(inner.trim())?;
            return Some(GridTrack::Intrinsic {
                min: TrackBound::MinContent,
                max: TrackBound::FitContent(d),
            });
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
                let mut parts = inner.splitn(2, ',');
                let count = parts.next().unwrap_or("").trim();
                let tracks = parts.next().unwrap_or("").trim();
                if count == "auto-fill" || count == "auto-fit" {
                    // Quantas vezes repetir é uma pergunta de LAYOUT (depende
                    // do espaço disponível) — não se expande aqui; fica um
                    // único `AutoRepeat` que `layout::grid_tracks` resolve
                    // contra `content_w`.
                    if let Some(inner_tracks) = GridTrack::parse_list(tracks) {
                        let count_unit: f32 =
                            split_top_level(tracks).iter().map(|t| track_count_unit(t)).sum();
                        out.push(GridTrack::AutoRepeat {
                            tracks: inner_tracks,
                            fit: count == "auto-fit",
                            count_unit,
                        });
                    }
                } else {
                    // repeat(N, tracks) — N vezes as trilhas internas.
                    let n: usize = count.parse().unwrap_or(1);
                    if let Some(inner_tracks) = GridTrack::parse_list(tracks) {
                        for _ in 0..n.max(1) {
                            out.extend(inner_tracks.iter().cloned());
                        }
                    }
                }
            } else if let Some(track) = GridTrack::parse_one(t) {
                out.push(track);
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

/// Parseia UM lado de `minmax()`: comprimento, ou um dos dois extremos
/// intrínsecos. Função à parte de `GridTrack::parse_one` porque um lado de
/// `minmax()` precisa de DISTINGUIR `min-content` de `auto`/`max-content` —
/// a pergunta que `GridTrack::Auto` (fora de `minmax()`) responde sem
/// distinguir, por não precisar (ver o comentário do variant).
fn parse_track_bound(s: &str) -> Option<TrackBound> {
    let s = s.trim();
    let low = s.to_ascii_lowercase();
    match low.as_str() {
        "min-content" => Some(TrackBound::MinContent),
        "max-content" | "auto" => Some(TrackBound::MaxContent),
        _ => {
            if let Some(inner) = low
                .strip_prefix("fit-content(")
                .and_then(|s| s.strip_suffix(')'))
            {
                return super::lengths::parse_dimension_pub(inner.trim())
                    .map(TrackBound::FitContent);
            }
            super::lengths::parse_dimension_pub(s).map(TrackBound::Fixed)
        }
    }
}

/// A contribuição de UMA trilha do padrão de `repeat(auto-fill|auto-fit, …)`
/// para a CONTAGEM de repetições (CSS Grid 1 §7.2.3.3): um comprimento fixo
/// conta o seu valor em px; `minmax(<fixo>, …)` conta o MÍNIMO — mesmo quando
/// o máximo é `fr`, que fora daqui já colapsa à trilha flexível pura e
/// perderia essa informação (ver `GridTrack::Bounded`). Qualquer trilha
/// intrínseca ou `fr` sem mínimo fixo conta 0 — a mesma aproximação que
/// `GridTrack::Auto` já faz no resto do ficheiro (a base real dependeria de
/// itens ainda não colocados). Só `px`: `%`/`em`/`rem`/`vw` dependeriam do
/// container ou da fonte, que o parse ainda não tem — ficam a contar 0, o que
/// SUBESTIMA repetições em vez de as sobrestimar (nunca corta um item).
fn track_count_unit(tok: &str) -> f32 {
    let low = tok.trim().to_ascii_lowercase();
    let px_de = |s: &str| match super::lengths::parse_dimension_pub(s.trim()) {
        Some(Dimension::Px(p)) => p,
        _ => 0.0,
    };
    if let Some(inner) = low
        .strip_prefix("minmax(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return px_de(inner.splitn(2, ',').next().unwrap_or(""));
    }
    px_de(&low)
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
