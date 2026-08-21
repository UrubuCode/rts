//! COLOCAÇÃO POR LINHA de grid: `grid-column`, `grid-row` e as quatro longhands
//! `grid-{column,row}-{start,end}`.
//!
//! ## Guardadas, SEM geometria — e porquê não é um defeito visível
//!
//! Este motor coloca os itens de grid por ordem de documento (colocação
//! automática). Dizer `grid-column-start: 7` não move o item hoje: o valor fica
//! no `ComputedStyle` e o `getComputedStyle` responde-o certo, mas a caixa sai
//! onde saía. Isso é "não faz nada", que é diferente do caso do `clip` —
//! nenhuma destas seis esconde ou revela conteúdo, portanto não há defeito
//! visível a decidir antes.
//!
//! **O ponto de enxerto, para quem tiver o layout de grid na mão:** os quatro
//! campos `grid_column_start`/`_end`/`grid_row_start`/`_end` de
//! `style::props::ComputedStyle`, lidos onde hoje se atribui a célula seguinte
//! por ordem. É `crate::layout` e não é deste módulo.
//!
//! ## Um SEGUNDO sistema de colocação, dito por extenso
//!
//! O `ComputedStyle` já tem `grid_area` — colocação por NOME de área, de
//! `style::grid_areas`. Esta é a colocação por NÚMERO DE LINHA, e a spec define
//! as duas: `grid-area` com um nome resolve para as quatro linhas da área, e uma
//! longhand escrita a seguir sobrepõe-se ao lado dela. Não reconciliei as duas
//! porque reconciliar é a decisão de quem colocar os itens — o que este módulo
//! garante é que os dois valores chegam lá inteiros e distinguíveis, em vez de
//! um deles se perder no caminho.
//!
//! ## O que a gramática NÃO tem, e porquê
//!
//! A spec aceita também `<custom-ident>` e `<ident> <integer>` (linhas com
//! nome). **Nenhuma folha do corpus escreve uma linha com nome** — as 13 folhas,
//! juntas, escrevem exatamente quatro formas: `auto`, `<inteiro>`,
//! `-<inteiro>` e `span <inteiro>`. Um nome de linha cairia como não-declarado,
//! que é a mesma resposta que dar hoje ao valor errado — e inventar um
//! `GridLine::Named` sem nada que resolva nomes seria um campo que ninguém lê.

use super::props::ComputedStyle;

/// Uma extremidade de colocação. `Line(-1)` é a última linha do eixo (contagem
/// a partir do fim), que é como `grid-column: 1 / -1` diz "todas as colunas".
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GridLine {
    Auto,
    Line(i32),
    Span(u32),
}

impl GridLine {
    /// `auto | <inteiro> | span <inteiro>`. `None` quando o valor não é nenhuma
    /// das formas — incluindo `0`, que a spec proíbe explicitamente (as linhas
    /// são numeradas a partir de 1, e -1 conta do fim).
    pub fn parse(v: &str) -> Option<GridLine> {
        let low = v.trim().to_ascii_lowercase();
        if low == "auto" {
            return Some(GridLine::Auto);
        }
        if let Some(n) = low.strip_prefix("span") {
            // `span 3` e `span3` não são o mesmo: sem separador não é um span.
            let n = n.strip_prefix(|c: char| c.is_whitespace())?;
            let n = n.trim().parse::<u32>().ok()?;
            return (n >= 1).then_some(GridLine::Span(n));
        }
        let n = low.parse::<i32>().ok()?;
        (n != 0).then_some(GridLine::Line(n))
    }

    /// O que `getComputedStyle` responde para uma LONGHAND. As três formas são
    /// as da spec e não têm variação de serialização — ao contrário do
    /// shorthand, ver a nota em [`shorthand_css`].
    pub fn css(self) -> String {
        match self {
            GridLine::Auto => "auto".to_string(),
            GridLine::Line(n) => n.to_string(),
            GridLine::Span(n) => format!("span {n}"),
        }
    }
}

/// O computado do SHORTHAND (`grid-column`/`grid-row`), a partir das duas pontas.
///
/// **Esta é a única serialização deste módulo que não foi medida contra o
/// Chrome**, e fica escrito em vez de silenciado: o dump de referência
/// (`tests/css/claude-computed-valor-inicial.esperado.json`) tem
/// `grid-template-columns` mas não tem `grid-column`, portanto não há aqui de
/// onde transcrever. Escolhi a forma `<start> / <end>` sempre, que é
/// auto-consistente e faz round-trip com o parser. O que decide é
/// `scripts/parity/chrome_extract.mjs` sobre um fixture em `tests/css/`, e se
/// discordar é ESTA função que muda — as longhands acima não dependem dela.
fn shorthand_css(start: Option<GridLine>, end: Option<GridLine>) -> String {
    let l = |v: Option<GridLine>| v.unwrap_or(GridLine::Auto).css();
    format!("{} / {}", l(start), l(end))
}

/// O shorthand `grid-column: <start> [/ <end>]`.
///
/// Com UM valor só, a spec diz que o `end` copia o `start` **apenas** se o valor
/// for um `<custom-ident>`; para um inteiro ou um `span`, o `end` fica `auto`.
/// Como este módulo não tem idents, a regra reduz-se a "um valor = start, end
/// auto" — que é o que `grid-column: 5` significa em qualquer folha do corpus.
fn parse_shorthand(val: &str) -> (Option<GridLine>, Option<GridLine>) {
    let mut it = val.splitn(2, '/');
    let start = it.next().and_then(GridLine::parse);
    let end = it.next().and_then(GridLine::parse);
    (start, end)
}

/// `grid-auto-flow` — a direção em que a colocação automática preenche a grelha,
/// e se ela volta atrás para tapar buracos (`dense`).
///
/// GUARDADA, SEM GEOMETRIA, e é a que descreve LITERALMENTE o que este motor já
/// faz: colocar os itens por ordem, numa direção. Só que a direção é fixa e esta
/// propriedade não a muda — quem colocar os itens é que a lê. O `dense` é o
/// mesmo mecanismo com uma segunda passada a tapar buracos.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GridAutoFlow {
    /// `column` = preenche coluna a coluna. `row` (o inicial) = linha a linha.
    pub coluna: bool,
    pub dense: bool,
}

impl GridAutoFlow {
    /// `row | column | dense | row dense | column dense`, em qualquer ordem — a
    /// spec não a fixa, e as folhas escrevem `row dense` e `column dense`.
    pub fn parse(v: &str) -> Option<GridAutoFlow> {
        let low = v.trim().to_ascii_lowercase();
        let mut f = GridAutoFlow { coluna: false, dense: false };
        let mut viu_eixo = false;
        for t in low.split_whitespace() {
            match t {
                "row" => viu_eixo = true,
                "column" => {
                    f.coluna = true;
                    viu_eixo = true;
                }
                "dense" => f.dense = true,
                // Um token que não é da gramática invalida a declaração inteira,
                // em vez de dar um `row` que o autor não escreveu.
                _ => return None,
            }
        }
        (viu_eixo || f.dense).then_some(f)
    }

    /// O Chrome imprime `row` mesmo quando o autor o omitiu (`dense` → `row dense`).
    pub fn css(self) -> String {
        let eixo = if self.coluna { "column" } else { "row" };
        if self.dense { format!("{eixo} dense") } else { eixo.to_string() }
    }
}

/// Tenta aplicar uma das seis. `false` = o nome não é de nenhuma delas.
pub fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    match prop {
        "grid-column-start" => css.grid_column_start = GridLine::parse(val),
        "grid-column-end" => css.grid_column_end = GridLine::parse(val),
        "grid-row-start" => css.grid_row_start = GridLine::parse(val),
        "grid-row-end" => css.grid_row_end = GridLine::parse(val),
        "grid-column" => {
            let (s, e) = parse_shorthand(val);
            css.grid_column_start = s;
            css.grid_column_end = e;
        }
        "grid-row" => {
            let (s, e) = parse_shorthand(val);
            css.grid_row_start = s;
            css.grid_row_end = e;
        }
        "grid-auto-flow" => css.grid_auto_flow = GridAutoFlow::parse(val),
        // `grid-auto-columns` — o tamanho das colunas IMPLÍCITAS. `grid-auto-rows`
        // não está aqui porque já tem braço próprio no `parse` e é CONSUMIDA
        // pelo layout; esta é a metade que faltava, com o mesmo tipo.
        "grid-auto-columns" => css.grid_auto_columns = super::GridTrack::parse_one(val),
        // `grid-gap` é o nome ANTIGO de `gap` — alias puro, e a folha que o
        // escreve escreve-o sozinho. Reentrega ao `parse`, que já sabe expandir
        // o par; uma segunda expansão aqui divergia da primeira.
        "grid-gap" => return super::parse::aplica_declaracao(css, "gap", val),
        "grid-column-gap" => return super::parse::aplica_declaracao(css, "column-gap", val),
        "grid-row-gap" => return super::parse::aplica_declaracao(css, "row-gap", val),
        _ => return false,
    }
    true
}

/// O valor tal como o elemento o DECLAROU (`el.style.x`), ou `""`. `None` = o
/// nome não é deste módulo. A distinção entre isto e o computado está no
/// cabeçalho de `style::initial`.
pub fn get_property(css: &ComputedStyle, name: &str) -> Option<String> {
    let s = match name {
        "grid-column-start" => css.grid_column_start.map(|v| v.css()).unwrap_or_default(),
        "grid-column-end" => css.grid_column_end.map(|v| v.css()).unwrap_or_default(),
        "grid-row-start" => css.grid_row_start.map(|v| v.css()).unwrap_or_default(),
        "grid-row-end" => css.grid_row_end.map(|v| v.css()).unwrap_or_default(),
        // O shorthand só responde se ALGUMA das pontas foi declarada — senão
        // `el.style.gridColumn` responderia `auto / auto` em todo o elemento do
        // documento, que é o erro que o cabeçalho de `style::initial` descreve.
        "grid-column" => match (css.grid_column_start, css.grid_column_end) {
            (None, None) => String::new(),
            (s, e) => shorthand_css(s, e),
        },
        "grid-auto-flow" => css.grid_auto_flow.map(|v| v.css()).unwrap_or_default(),
        "grid-row" => match (css.grid_row_start, css.grid_row_end) {
            (None, None) => String::new(),
            (s, e) => shorthand_css(s, e),
        },
        _ => return None,
    };
    Some(s)
}
