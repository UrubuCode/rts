//! LINE PLACEMENT of grid items: `grid-area`, `grid-column`, `grid-row` and the
//! four longhands `grid-{column,row}-{start,end}`, which `layout::grid::lines`
//! resolves into cells (a name against `grid-template-areas` included).
//!
//! ## Named lines, and ONE expansion for the three shorthands
//!
//! `GridLine` carries the whole CSS Grid §8.3 grammar: `auto`, `<integer>`,
//! `<custom-ident>` (optionally with an integer), `span <integer>` and
//! `span <custom-ident>`. `grid-area`, `grid-row` and `grid-column` all go
//! through [`expand_shorthand`] — the §8.4 rule that a missing `-end` (or a
//! missing `grid-column-start`) copies a bare `<custom-ident>` and is `auto`
//! otherwise — into the four longhands, which are the only thing placement
//! (`layout::grid::lines`) reads. `grid-area: 2 / 2 / 3 / 3` used to be
//! dropped whole, because only its single-name form had a reader.

use std::sync::Arc;

use super::props::ComputedStyle;
use super::aplica::set_if;

/// One placement edge. `Line(-1)` is the last line of the axis, which is how
/// `grid-column: 1 / -1` says "every column". `Named(name, None)` is a BARE
/// ident — the only form a missing shorthand edge copies — and `Some(n)` is
/// `<integer> <ident>`.
#[derive(Clone, PartialEq, Debug)]
pub enum GridLine {
    Auto,
    Line(i32),
    Span(u32),
    Named(Arc<str>, Option<i32>),
    SpanNamed(Arc<str>, u32),
}

/// A `<custom-ident>` as grid placement admits it: not `span`/`auto` or a
/// CSS-wide keyword, and not starting with a digit.
fn is_ident(t: &str) -> bool {
    let mut chars = t.chars();
    let starts_ok = match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => true,
        Some('-') => chars.next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '-'),
        _ => false,
    };
    let low = t.to_ascii_lowercase();
    starts_ok
        && t.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        && !matches!(low.as_str(), "span" | "auto" | "inherit" | "initial" | "unset" | "default" | "revert")
}

impl GridLine {
    /// One edge per §8.3, its tokens in any order. `None` for anything outside
    /// the grammar — including `0` (lines count from 1, and -1 from the end),
    /// `span 0` and `span3` (no separator is not a span).
    pub fn parse(v: &str) -> Option<GridLine> {
        let v = v.trim();
        if v.eq_ignore_ascii_case("auto") {
            return Some(GridLine::Auto);
        }
        let (mut span, mut int, mut name): (bool, Option<i32>, Option<&str>) = (false, None, None);
        for t in v.split_whitespace() {
            if t.eq_ignore_ascii_case("span") && !span {
                span = true;
            } else if let (Ok(n), None) = (t.parse::<i32>(), int) {
                int = Some(n);
            } else if is_ident(t) && name.is_none() {
                name = Some(t);
            } else {
                return None;
            }
        }
        match (span, int, name) {
            (true, Some(n), None) => (n >= 1).then_some(GridLine::Span(n as u32)),
            (true, n, Some(id)) => {
                let n = n.unwrap_or(1);
                (n >= 1).then(|| GridLine::SpanNamed(id.into(), n as u32))
            }
            (false, Some(n), None) => (n != 0).then_some(GridLine::Line(n)),
            (false, n, Some(id)) => (n != Some(0)).then(|| GridLine::Named(id.into(), n)),
            _ => None,
        }
    }

    /// What `getComputedStyle` answers for a LONGHAND.
    pub fn css(&self) -> String {
        match self {
            GridLine::Auto => "auto".to_string(),
            GridLine::Line(n) => n.to_string(),
            GridLine::Span(n) => format!("span {n}"),
            GridLine::Named(id, None) => id.to_string(),
            GridLine::Named(id, Some(n)) => format!("{n} {id}"),
            GridLine::SpanNamed(id, 1) => format!("span {id}"),
            GridLine::SpanNamed(id, n) => format!("span {n} {id}"),
        }
    }

    fn is_bare_ident(&self) -> bool {
        matches!(self, GridLine::Named(_, None))
    }
}

/// The computed SHORTHAND (`grid-column`/`grid-row`), from the two edges.
///
/// **Not measured against Chrome**: the reference dump
/// (`tests/css/claude-computed-valor-inicial.esperado.json`) has no
/// `grid-column`. `<start> / <end>` was chosen for being self-consistent and
/// round-tripping through the parser; `scripts/parity/chrome_extract.mjs` on a
/// fixture decides, and if it disagrees it is THIS function that changes.
fn shorthand_css(start: &Option<GridLine>, end: &Option<GridLine>) -> String {
    let l = |v: &Option<GridLine>| v.clone().unwrap_or(GridLine::Auto).css();
    format!("{} / {}", l(start), l(end))
}

/// The ONE expansion of `grid-area` (`max = 4`: row-start / column-start /
/// row-end / column-end) and of `grid-row`/`grid-column` (`max = 2`: start /
/// end), CSS Grid §8.4. A missing edge copies the edge it pairs with when that
/// is a bare `<custom-ident>`, and is `auto` otherwise — so `grid-area: a`
/// gives `a` four times and `grid-row: 2` gives `2 / auto`. `None` when a part
/// is outside the grammar or there are too many parts: the declaration is
/// invalid whole, never half-applied.
pub fn expand_shorthand(val: &str, max: usize) -> Option<Vec<GridLine>> {
    let parts: Vec<&str> = val.split('/').collect();
    if parts.len() > max {
        return None;
    }
    let mut out: Vec<GridLine> = parts.iter().map(|p| GridLine::parse(p)).collect::<Option<_>>()?;
    while out.len() < max {
        // The edge a missing one pairs with: two places back in the 4-value
        // form (row-end ← row-start, column-end ← column-start), row-start for
        // column-start itself, and the start in the 2-value form.
        let i = out.len();
        let pair = if max == 4 && i == 1 { 0 } else { i - max / 2 };
        let copied = if out[pair].is_bare_ident() { out[pair].clone() } else { GridLine::Auto };
        out.push(copied);
    }
    Some(out)
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
    pub column: bool,
    pub dense: bool,
}

impl GridAutoFlow {
    /// `row | column | dense | row dense | column dense`, em qualquer ordem — a
    /// spec não a fixa, e as folhas escrevem `row dense` e `column dense`.
    pub fn parse(v: &str) -> Option<GridAutoFlow> {
        let low = v.trim().to_ascii_lowercase();
        let mut f = GridAutoFlow { column: false, dense: false };
        let mut viu_eixo = false;
        for t in low.split_whitespace() {
            match t {
                "row" => viu_eixo = true,
                "column" => {
                    f.column = true;
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
        let eixo = if self.column { "column" } else { "row" };
        if self.dense { format!("{eixo} dense") } else { eixo.to_string() }
    }
}

/// Tenta aplicar uma das seis. `false` = o nome não é de nenhuma delas.
pub fn try_apply(css: &mut ComputedStyle, prop: &str, val: &str) -> bool {
    match prop {
        "grid-column-start" => set_if(&mut css.grid_column_start, GridLine::parse(val)),
        "grid-column-end" => set_if(&mut css.grid_column_end, GridLine::parse(val)),
        "grid-row-start" => set_if(&mut css.grid_row_start, GridLine::parse(val)),
        "grid-row-end" => set_if(&mut css.grid_row_end, GridLine::parse(val)),
        "grid-column" | "grid-row" => {
            if let Some(v) = expand_shorthand(val, 2) {
                let mut v = v.into_iter().map(Some);
                let (s, e) = (v.next().flatten(), v.next().flatten());
                if prop == "grid-column" {
                    (css.grid_column_start, css.grid_column_end) = (s, e);
                } else {
                    (css.grid_row_start, css.grid_row_end) = (s, e);
                }
            }
        }
        // All four longhands, as the spec expands it; `grid_area` keeps the
        // single-name spelling only for `getComputedStyle("grid-area")`.
        "grid-area" => {
            if let Some(v) = expand_shorthand(val, 4) {
                let mut v = v.into_iter().map(Some);
                css.grid_area = super::grid_areas::parse_grid_area_name(val);
                css.grid_row_start = v.next().flatten();
                css.grid_column_start = v.next().flatten();
                css.grid_row_end = v.next().flatten();
                css.grid_column_end = v.next().flatten();
            }
        }
        "grid-auto-flow" => set_if(&mut css.grid_auto_flow, GridAutoFlow::parse(val)),
        // `grid-auto-columns` — o tamanho das colunas IMPLÍCITAS. `grid-auto-rows`
        // não está aqui porque já tem braço próprio no `parse` e é CONSUMIDA
        // pelo layout; esta é a metade que faltava, com o mesmo tipo.
        "grid-auto-columns" => set_if(&mut css.grid_auto_columns, super::GridTrack::parse_one(val)),
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
        "grid-column-start" => css.grid_column_start.as_ref().map(|v| v.css()).unwrap_or_default(),
        "grid-column-end" => css.grid_column_end.as_ref().map(|v| v.css()).unwrap_or_default(),
        "grid-row-start" => css.grid_row_start.as_ref().map(|v| v.css()).unwrap_or_default(),
        "grid-row-end" => css.grid_row_end.as_ref().map(|v| v.css()).unwrap_or_default(),
        // O shorthand só responde se ALGUMA das pontas foi declarada — senão
        // `el.style.gridColumn` responderia `auto / auto` em todo o elemento do
        // documento, que é o erro que o cabeçalho de `style::initial` descreve.
        "grid-column" => match (&css.grid_column_start, &css.grid_column_end) {
            (None, None) => String::new(),
            (s, e) => shorthand_css(s, e),
        },
        "grid-auto-flow" => css.grid_auto_flow.map(|v| v.css()).unwrap_or_default(),
        "grid-row" => match (&css.grid_row_start, &css.grid_row_end) {
            (None, None) => String::new(),
            (s, e) => shorthand_css(s, e),
        },
        _ => return None,
    };
    Some(s)
}
