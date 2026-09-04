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
    if let Some(n) = low.strip_suffix("ex").and_then(num) {
        return Some(Dimension::Ex(n));
    }
    if let Some(n) = low.strip_suffix("ch").and_then(num) {
        return Some(Dimension::Ch(n));
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

/// Um offset de posicionamento (`top`/`left`/…): um comprimento COM SINAL, em
/// qualquer unidade. Deslocar para fora da caixa é o idioma de badges e
/// tooltips, e a folha real escreve-o tanto em `px` como em `em`/`rem`/`%`.
///
/// Era um caminho de sinal PRÓPRIO que só sabia ler `px`/número puro, e por isso
/// `bottom:-11px` passava enquanto `top:-1.65em` e `right:-.25rem` eram
/// descartados em silêncio. Acertar numa unidade e falhar nas outras é pior do
/// que recusar todas: quem lê a folha não tem como adivinhar a fronteira.
///
/// Agora é o [`parse_dimension_signed`] e mais nada — a decisão "esta
/// propriedade aceita negativo" fica no NOME de quem se chama, e a leitura da
/// unidade fica num sítio só.
pub(crate) fn parse_inset(v: &str) -> Option<Dimension> {
    parse_dimension_signed(v)
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
/// A [`Caixa`] diz qual das duas é, e com isso o que `auto` e o sinal valem.
///
/// Responde `None` quando a declaração é INVÁLIDA — nenhum token, mais de
/// quatro, ou qualquer um deles recusado. Um shorthand com um componente
/// inválido é inválido por inteiro (CSS Cascade 5 §3.2), e aqui isso não é
/// pedantismo: o shorthand escreve os quatro lados de uma vez, portanto
/// devolver os lados que deram apagaria os outros três. Ver
/// [`super::aplica::set_edges`].
pub(crate) fn parse_edges(val: &str, caixa: Caixa) -> Option<Edges> {
    // Separa os lados respeitando PARÊNTESES — um `calc(0.25rem * 4)` (todo o
    // espaçamento do Tailwind v4) tem espaços INTERNOS que o `split_whitespace` cru
    // quebraria em 3 tokens inválidos, zerando o padding/margin da página inteira.
    let toks: Vec<Side> = split_top_ws(val)
        .iter()
        .map(|t| parse_side(t, caixa))
        .collect();
    // Um componente recusado invalida o shorthand INTEIRO — a verificação vem
    // antes do mapeamento porque é sobre a declaração e não sobre um lado.
    if toks.iter().any(|s| *s == Side::Unset) {
        return None;
    }
    Some(match toks.as_slice() {
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
        _ => return None, // 0 ou >4 valores: a declaração é inválida.
    })
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

/// Qual das duas caixas de espaçamento está a ser parseada. As duas fazem as
/// MESMAS perguntas de unidade e respondem ao contrário em duas delas, e é por
/// isso que é um nome de propriedade e não dois booleanos: `auto` e o sinal
/// coincidem aqui por acaso — `margin` diz sim aos dois e `padding` não aos dois
/// — e dois `bool` que valem sempre o mesmo convidam o próximo a passar um sem o
/// outro.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Caixa {
    /// `margin` — aceita `auto` e aceita comprimento negativo (o gutter `.row`
    /// do Bootstrap é `margin-left:-15px`).
    Margem,
    /// `padding` — recusa `auto` e recusa negativo (CSS Box 3: o valor é
    /// `<length-percentage [0,∞]>`).
    Padding,
}

impl Caixa {
    fn aceita_auto(self) -> bool {
        self == Caixa::Margem
    }
    fn aceita_negativo(self) -> bool {
        self == Caixa::Margem
    }
}

/// Parseia UM lado de margin/padding: um COMPRIMENTO em qualquer unidade
/// (px/%/em/rem/vw/vh — resolve tarde, como width), `auto` (só margem), ou
/// `Unset` se inválido para aquela caixa.
///
/// O sinal é decidido AQUI, pela propriedade, e não lá em baixo pela unidade. Um
/// `padding:-4px` era aceite e clampado a zero no layout, e as duas coisas não
/// são a mesma: clampar CONSOME a declaração e apaga a que vinha antes, enquanto
/// recusá-la — que é o que o browser faz com uma declaração inválida — deixa a
/// anterior de pé.
pub(crate) fn parse_side(tok: &str, caixa: Caixa) -> Side {
    let t = tok.trim();
    if caixa.aceita_auto() && t.eq_ignore_ascii_case("auto") {
        return Side::Auto;
    }
    let d = if caixa.aceita_negativo() {
        parse_dimension_signed(t)
    } else {
        parse_dimension(t)
    };
    match d {
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
        Dimension::Ex(x) => Dimension::Ex(-x),
        Dimension::Ch(x) => Dimension::Ch(-x),
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
mod sinal_por_propriedade {
    use crate::style::parse::parse_inline;
    use crate::style::values::{Dimension, Side};

    /// Um offset negativo vale em QUALQUER unidade, e não só em `px`.
    ///
    /// O `parse_inset` tratava o sinal à parte, com um caminho próprio que só
    /// sabia ler `px`/número — por isso `top:-1.65em` e `right:-.25rem` eram
    /// descartados enquanto `bottom:-11px` passava. Um por asserção porque o
    /// defeito era exatamente esse: acertar numa unidade e falhar nas outras.
    #[test]
    fn offset_negativo_vale_em_qualquer_unidade() {
        assert_eq!(parse_inline("bottom:-11px").inset_bottom, Some(Dimension::Px(-11.0)));
        assert_eq!(parse_inline("top:-1.65em").inset_top, Some(Dimension::Em(-1.65)));
        assert_eq!(parse_inline("right:-.25rem").inset_right, Some(Dimension::Rem(-0.25)));
        assert_eq!(parse_inline("left:-50%").inset_left, Some(Dimension::Percent(-50.0)));
    }

    /// E a margem também — a forma relativa é a que a folha real escreve.
    #[test]
    fn margem_negativa_vale_em_qualquer_unidade() {
        assert_eq!(
            parse_inline("margin-top:-0.5em").margin.top,
            Side::Len(Dimension::Em(-0.5))
        );
        assert_eq!(
            parse_inline("margin-left:-3.2em").margin.left,
            Side::Len(Dimension::Em(-3.2))
        );
        assert_eq!(
            parse_inline("margin:-1px").margin.top,
            Side::Len(Dimension::Px(-1.0))
        );
    }

    /// O OUTRO lado da regra, e a metade sem a qual esta correção seria pior que
    /// o defeito: as propriedades que a spec proíbe de ser negativas continuam a
    /// recusar. A declaração cai, o que deixa a anterior a valer — que é o que o
    /// browser faz.
    #[test]
    fn comprimento_negativo_continua_recusado_onde_a_spec_o_proibe() {
        assert_eq!(parse_inline("width:-5px").width, None);
        assert_eq!(parse_inline("height:-2em").height, None);
        assert_eq!(parse_inline("min-width:-1rem").min_width, None);
        assert_eq!(parse_inline("font-size:-12px").font_size, None);
    }

    /// `padding` é o caso que estava do lado errado: aceitava o negativo e
    /// deixava o layout clampar a zero. Não é o mesmo — clampar CONSOME a
    /// declaração e apaga a anterior, enquanto recusá-la deixa a anterior de pé.
    #[test]
    fn padding_negativo_e_recusado_e_nao_clampado() {
        assert_eq!(parse_inline("padding-top:-4px").padding.top, Side::Unset);
        assert_eq!(parse_inline("padding:-1em").padding.left, Side::Unset);
        assert_eq!(
            parse_inline("padding-left:8px;padding-left:-4px").padding.left,
            Side::Len(Dimension::Px(8.0)),
            "a declaração inválida tem de deixar a anterior a valer"
        );
    }

    /// `word-spacing` aceita negativo (aperta o espaço entre palavras), como o
    /// `letter-spacing` ao lado dele já aceitava.
    #[test]
    fn word_spacing_negativo_e_aceite_como_o_letter_spacing() {
        assert_eq!(parse_inline("word-spacing:-2px").word_spacing, Some(-2.0));
        assert_eq!(parse_inline("letter-spacing:-1px").letter_spacing, Some(-1.0));
    }

    /// E o `auto` continua a ser só da margem: `padding:auto` não existe.
    #[test]
    fn auto_continua_a_ser_so_da_margem() {
        assert_eq!(parse_inline("margin-left:auto").margin.left, Side::Auto);
        assert_eq!(parse_inline("padding-left:auto").padding.left, Side::Unset);
    }
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
