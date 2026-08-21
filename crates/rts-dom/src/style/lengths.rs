//! COMPRIMENTOS: as unidades do CSS (`px`/`%`/`em`/`rem`/`vw`/`vh`/`pt`/`pc`),
//! os lados de uma caixa e os atalhos que os shorthands usam para os separar.
//!
//! Vive fora de `parse.rs` porque aquele ficheiro é o DISPATCH nome→campo — um
//! match com uma linha por propriedade — e isto é o vocabulário de valores que
//! ele consome. Juntos passavam o teto de 500 linhas do repositório, e o critério
//! do corte é o mesmo do `fmt`/`fmt_values`: quem acrescenta uma PROPRIEDADE toca
//! no dispatch, quem acrescenta uma UNIDADE toca aqui.

use super::values::{Dimension, Edges, Side};

/// `font-size` em px (aceita "18px" ou "18"). Ignora unidades não-px por ora
/// (em/%/rem chegam na fase de unidades). Só valores > 0.
pub(crate) fn parse_px(v: &str) -> Option<f32> {
    let v = v.trim();
    let num = v.strip_suffix("px").unwrap_or(v);
    num.trim().parse::<f32>().ok().filter(|n| *n > 0.0)
}

/// `width` como [`Dimension`], cobrindo as unidades de comprimento usuais:
/// `auto`; `60%` → Percent; `1.5em` → Em; `2rem` → Rem; `50vw`/`80vh` → Vw/Vh;
/// `280`/`280px` → Px. Unidades relativas resolvem TARDE no render (risco 5).
/// Número inválido / unidade desconhecida → `None`. Ordem do match importa: testa
/// sufixos de 3/2 letras (`rem`) ANTES dos de 1 (`%`) e do px implícito.
/// Público p/ o parse de trilhas de grid (`GridTrack::parse_one`).
pub(crate) fn parse_dimension_pub(v: &str) -> Option<Dimension> {
    parse_dimension(v)
}

pub(crate) fn parse_dimension(v: &str) -> Option<Dimension> {
    let v = v.trim();
    if v.eq_ignore_ascii_case("auto") {
        return Some(Dimension::Auto);
    }
    // `max-content` — palavra-chave intrínseca, resolvida pelo layout.
    //
    // `min-content` e `fit-content` continuam a cair no `None` do fim, que é o
    // comportamento de hoje: a máquina de medida que temos calcula o MÁXIMO do
    // conteúdo e mais nada. Mapeá-las para `MaxContent` daria a um `min-content`
    // a resposta oposta à que o nome promete, em silêncio.
    if v.eq_ignore_ascii_case("max-content") {
        return Some(Dimension::MaxContent);
    }
    // `calc(...)` — expressão linear reduzida no parse (resolve tarde).
    let low_full = v.to_ascii_lowercase();
    if let Some(inner) = low_full
        .strip_prefix("calc(")
        .and_then(|r| r.strip_suffix(')'))
    {
        return super::calc::parse_calc_dim(inner);
    }
    // (sufixo, construtor, clamp_max) — `%`/`vw`/`vh` em 0..=100; resto sem teto.
    let num = |s: &str| s.trim().parse::<f32>().ok().filter(|n| *n >= 0.0);
    let low = v.to_ascii_lowercase();
    // sufixos de 2+ letras primeiro (rem antes de em; px por último implícito).
    if let Some(n) = low.strip_suffix("rem").and_then(num) {
        return Some(Dimension::Rem(n));
    }
    if let Some(n) = low.strip_suffix("em").and_then(num) {
        return Some(Dimension::Em(n));
    }
    if let Some(n) = low.strip_suffix("vw").and_then(num) {
        return Some(Dimension::Vw(n.clamp(0.0, 100.0)));
    }
    if let Some(n) = low.strip_suffix("vh").and_then(num) {
        return Some(Dimension::Vh(n.clamp(0.0, 100.0)));
    }
    if let Some(n) = low.strip_suffix('%').and_then(num) {
        // SEM teto de 100. Uma percentagem maior é CSS legal e comum:
        // `font-size: 150%` (medido no corpus: 150% de 20px = 30px, e o clamp
        // dava 20px), `width: 200%` numa faixa que transborda de propósito.
        // O clamp vinha de tratar `%` como uma fração do container, que é uma
        // das leituras, não a definição.
        return Some(Dimension::Percent(n));
    }
    // `pt` (pontos de impressão) e `pc` (paica) — absolutos, comuns em páginas
    // legadas (o rodapé do google usa `font-size:10pt`). 1pt = 4/3 px; 1pc = 16px.
    if let Some(n) = low.strip_suffix("pt").and_then(num) {
        return Some(Dimension::Px(n * 4.0 / 3.0));
    }
    if let Some(n) = low.strip_suffix("pc").and_then(num) {
        return Some(Dimension::Px(n * 16.0));
    }
    // px explícito ou número puro.
    num(low.strip_suffix("px").unwrap_or(&low)).map(Dimension::Px)
}

/// Um offset de posicionamento (`top`/`left`/…): como [`parse_dimension`], MAS
/// aceita valores NEGATIVOS em px (deslocar para fora é comum em badges/tooltips);
/// as unidades relativas continuam ≥ 0 via parse_dimension.
pub(crate) fn parse_inset(v: &str) -> Option<Dimension> {
    let t = v.trim();
    // negativo: só a forma px/número (o parse_dimension rejeita <0).
    if t.starts_with('-') {
        let num = t.strip_suffix("px").unwrap_or(t);
        return num.trim().parse::<f32>().ok().map(Dimension::Px);
    }
    parse_dimension(t)
}

/// Parseia o shorthand `gap: <row-gap> <column-gap>` → `(row_gap, column_gap)`.
/// 1 valor = ambos iguais; 2 valores = row primeiro (ordem CSS). Reusa parse_dimension.
pub(crate) fn parse_gap_pair(val: &str) -> (Option<Dimension>, Option<Dimension>) {
    let parts: Vec<&str> = val.split_whitespace().collect();
    match parts.as_slice() {
        [a] => {
            let d = parse_dimension(a);
            (d, d)
        }
        [r, c] => (parse_dimension(r), parse_dimension(c)),
        _ => (None, None),
    }
}

/// Parseia o shorthand de margin/padding (1/2/3/4 valores) para [`Edges`], com o
/// mapeamento exato do CSS:
/// - 1: todos os lados
/// - 2: `top/bottom` | `left/right` (vertical | horizontal)
/// - 3: `top` | `left/right` | `bottom`
/// - 4: `top` | `right` | `bottom` | `left` (horário)
/// `allow_auto` habilita o keyword `auto` (margin). Tokens inválidos → Unset.
pub(crate) fn parse_edges(val: &str, allow_auto: bool) -> Edges {
    // Separa os lados respeitando PARÊNTESES — um `calc(0.25rem * 4)` (todo o
    // espaçamento do Tailwind v4) tem espaços INTERNOS que o `split_whitespace` cru
    // quebraria em 3 tokens inválidos, zerando o padding/margin da página inteira.
    let toks: Vec<Side> = split_top_ws(val)
        .iter()
        .map(|t| parse_side(t, allow_auto))
        .collect();
    match toks.as_slice() {
        [a] => Edges::all(*a),
        [v, h] => Edges {
            top: *v,
            right: *h,
            bottom: *v,
            left: *h,
        },
        [t, h, b] => Edges {
            top: *t,
            right: *h,
            bottom: *b,
            left: *h,
        },
        [t, r, b, l] => Edges {
            top: *t,
            right: *r,
            bottom: *b,
            left: *l,
        },
        _ => Edges::default(), // 0 ou >4: ignora (robustez).
    }
}

/// [`split_top_ws`] para os módulos de shorthand (`background`), que precisam da
/// mesma regra de "espaço de topo" para não partir um `url(a b.png)`.
pub(crate) fn split_top_ws_pub(v: &str) -> Vec<String> {
    split_top_ws(v)
}

/// [`parse_len`] (comprimento absoluto em pontos) para os módulos de shorthand.
pub(crate) fn parse_len_pub(v: &str) -> Option<f32> {
    parse_len(v)
}

/// Separa por um SEPARADOR de topo (fora de parênteses) — a vírgula que separa
/// camadas de `background`, a barra que separa `position / size`. A versão que
/// ignora os parênteses é obrigatória: a vírgula de `rgba(0,0,0,.5)` e a barra de
/// `url(a/b.png)` não separam nada.
pub(crate) fn split_top(v: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in v.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            c if c == sep && depth == 0 => out.push(std::mem::take(&mut cur).trim().to_string()),
            _ => cur.push(c),
        }
    }
    let last = cur.trim().to_string();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

/// Um comprimento em pontos que PODE ser negativo (`outline-offset`, que afunda o
/// anel para dentro da caixa). `parse_len` recusa negativos por servir larguras.
pub(crate) fn parse_signed_px(v: &str) -> Option<f32> {
    let t = v.trim();
    match t.strip_prefix('-') {
        Some(rest) => parse_len(rest).map(|x| -x),
        None => parse_len(t),
    }
}

/// Separa uma lista de valores por ESPAÇO de TOPO (fora de parênteses), para o
/// shorthand de edges não quebrar `calc(a * b)` (espaços internos). Vazio → [].
pub(crate) fn split_top_ws(v: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in v.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Parseia UM lado de margin/padding: um COMPRIMENTO em qualquer unidade
/// (px/%/em/rem/vw/vh — resolve tarde, como width), `auto` (se `allow_auto`), ou
/// `Unset` se inválido. NEGATIVO permitido (margem negativa é o gutter `.row` do
/// Bootstrap; padding negativo é clampado ≥ 0 no consumo do layout).
pub(crate) fn parse_side(tok: &str, allow_auto: bool) -> Side {
    let t = tok.trim();
    if allow_auto && t.eq_ignore_ascii_case("auto") {
        return Side::Auto;
    }
    match parse_dimension_signed(t) {
        // `auto` já foi tratado acima (só margin); um Auto aqui é inválido
        // (padding: auto não existe) — vira Unset, nunca `Len(Auto)`.
        Some(Dimension::Auto) | None => Side::Unset,
        Some(d) => Side::Len(d),
    }
}

/// Como [`parse_dimension`], mas aceita valores NEGATIVOS (margens/offsets). O
/// `%`/`vw`/`vh` não são clampados a 0..=100 aqui (o sinal importa).
pub(crate) fn parse_dimension_signed(v: &str) -> Option<Dimension> {
    let v = v.trim();
    let (neg, abs) = match v.strip_prefix('-') {
        Some(rest) => (true, rest.trim()),
        None => (false, v),
    };
    let d = parse_dimension(abs)?;
    if !neg {
        return Some(d);
    }
    Some(match d {
        Dimension::Auto => Dimension::Auto,
        // `-max-content` não existe em CSS; um sinal antes de uma palavra-chave
        // é a declaração inválida, e devolvê-la sem sinal é o que o `auto` já faz.
        Dimension::MaxContent => Dimension::MaxContent,
        Dimension::Px(x) => Dimension::Px(-x),
        Dimension::Percent(x) => Dimension::Percent(-x),
        Dimension::Em(x) => Dimension::Em(-x),
        Dimension::Rem(x) => Dimension::Rem(-x),
        Dimension::Vw(x) => Dimension::Vw(-x),
        Dimension::Vh(x) => Dimension::Vh(-x),
        Dimension::Calc(c) => Dimension::Calc(c.scale(-1.0)),
    })
}

/// Um comprimento de TEXTO/BORDA (`font-size`/`border-width`/`border-radius`, que
/// são `f32` px no modelo): aceita `px`/número E `rem` (× o root FIXO de 16px —
/// igual ao browser sem `html{font-size}` custom; o Bootstrap define TODA a
/// tipografia em rem). ⚠️ CORTE documentado: `em`/`%` de font-size dependem do
/// font do PAI (só a cascade sabe) — ficam de fora até o campo virar `Dimension`.
pub(crate) fn parse_len(v: &str) -> Option<f32> {
    let low = v.trim().to_ascii_lowercase();
    if let Some(n) = low.strip_suffix("rem") {
        return n
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|x| *x > 0.0)
            .map(|x| x * 16.0);
    }
    parse_px(&low)
}


#[cfg(test)]
mod palavras_chave_intrinsecas {
    use crate::style::{Dimension, parse::parse_inline};
    use crate::table::tests::{geometria, rect};

    /// `width: max-content` deixa de ser DESCARTADO no parse.
    ///
    /// Respondia `None` — indistinguível de "não declarado" — e o elemento
    /// tomava a largura do pai.
    #[test]
    fn max_content_e_parseado_e_nao_descartado() {
        assert_eq!(parse_inline("width:max-content").width, Some(Dimension::MaxContent));
    }

    /// `min-content` e `fit-content` continuam DESCARTADOS, deliberadamente.
    ///
    /// A máquina de medida que temos calcula o máximo do conteúdo e mais nada.
    /// Mapeá-las para `MaxContent` daria a um `min-content` a resposta oposta à
    /// que o nome promete, em silêncio — e um valor que erra ao contrário é pior
    /// do que um valor ausente. Este teste existe para que a ausência seja uma
    /// decisão visível e não um esquecimento.
    #[test]
    fn min_content_e_fit_content_ficam_de_fora_de_proposito() {
        assert_eq!(parse_inline("width:min-content").width, None);
        assert_eq!(parse_inline("width:fit-content").width, None);
    }

    /// A largura passa a ser a do CONTEÚDO, e transborda o pai em vez de ceder.
    ///
    /// É a diferença face ao shrink-to-fit, que é a mesma medição com um
    /// `.min(disponível)` por cima: um contentor de 60px não encolhe um
    /// `max-content` de 164. É a forma do painel do menu da Wikipédia, onde o
    /// Chrome dá 198,6 e nós dávamos 56,2.
    #[test]
    fn max_content_mede_o_conteudo_e_transborda_o_pai() {
        let com = "<div style='width:60px'><div style='width:max-content'>                   <ul><li>Pagina principal</li><li>Conteudo destacado</li></ul></div></div>";
        let sem = "<div style='width:60px'><div>                   <ul><li>Pagina principal</li><li>Conteudo destacado</li></ul></div></div>";
        let (d1, l1) = geometria(com, 1280.0);
        let (d2, l2) = geometria(sem, 1280.0);
        let a = rect(&d1, &l1, "div", 1);
        let b = rect(&d2, &l2, "div", 1);
        assert!(a.w > 100.0, "max-content tem de medir o conteúdo: {a:?}");
        assert!((b.w - 60.0).abs() < 0.5, "sem ele, a largura do pai: {b:?}");
    }

    /// E o `max-width` continua a morder POR CIMA, como manda a spec.
    ///
    /// É também como se distingue um `max-content` que calcula a mais: se o
    /// elemento real da página passar a bater no seu teto de 200px, é a medição
    /// que cresceu, não a folha que mudou.
    #[test]
    fn o_teto_de_max_width_continua_a_morder_sobre_o_max_content() {
        let (dom, list) = geometria(
            "<div style='width:60px'><div style='width:max-content;max-width:50px'>             <ul><li>Pagina principal muito comprida aqui</li></ul></div></div>",
            1280.0,
        );
        let d = rect(&dom, &list, "div", 1);
        assert!((d.w - 50.0).abs() < 0.5, "o teto tem de vencer: {d:?}");
    }
}
