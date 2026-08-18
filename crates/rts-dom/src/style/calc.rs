//! O parser de `calc()` — uma expressão LINEAR de comprimentos, reduzida a uma
//! [`CalcLen`] no momento do parse.
//!
//! Módulo próprio porque `parse.rs` é o dispatch nome→campo e isto é um parser
//! de expressões completo (descida recursiva, três níveis de precedência): duas
//! coisas com ritmos de mudança diferentes, e juntas punham o ficheiro do
//! dispatch acima do teto de 500 linhas do repositório.
//!
//! A gramática e as regras da spec que ele respeita estão no comentário abaixo,
//! que veio com o código.

use super::values::Dimension;

/// `calc(...)` de um valor já reconhecido como tal pelo chamador: recebe o
/// MIOLO (sem o `calc(` e o `)`).
pub(crate) fn parse_calc_dim(inner: &str) -> Option<Dimension> {
    parse_calc(inner).map(Dimension::Calc)
}

// ── calc() — expressão LINEAR de comprimentos, reduzida no parse ────────────────
// `calc(1.375rem + 1.5vw)` / `calc(100% - 2rem)` / `calc(2 * (1rem + 4px))`.
// Gramática (recursive descent): expr := term (('+'|'-') term)* ;
// term := atom (('*'|'/') atom)* ; atom := comprimento | número | '(' expr ')'.
// Multiplicação/divisão só com ESCALAR de um dos lados (regra da spec). O
// resultado é um [`CalcLen`] (combinação das 6 bases) que resolve TARDE.

/// Um valor intermediário do parser de calc: comprimento (combinação linear) ou
/// número puro (escalar de multiplicação).
enum CalcVal {
    Len(crate::style::CalcLen),
    Num(f32),
}

/// Parseia o MIOLO de um `calc(...)` (sem o `calc(` e `)`), ou `None` se inválido.
fn parse_calc(inner: &str) -> Option<crate::style::CalcLen> {
    let toks: Vec<char> = inner.chars().collect();
    let mut pos = 0usize;
    let v = calc_expr(&toks, &mut pos)?;
    // sobrou lixo → inválido.
    while pos < toks.len() {
        if !toks[pos].is_whitespace() {
            return None;
        }
        pos += 1;
    }
    match v {
        CalcVal::Len(l) => Some(l),
        CalcVal::Num(_) => None, // calc(2) não é um comprimento
    }
}

fn calc_ws(t: &[char], p: &mut usize) {
    while *p < t.len() && t[*p].is_whitespace() {
        *p += 1;
    }
}

fn calc_expr(t: &[char], p: &mut usize) -> Option<CalcVal> {
    let mut acc = calc_term(t, p)?;
    loop {
        calc_ws(t, p);
        let op = match t.get(*p) {
            Some('+') => 1.0f32,
            Some('-') => -1.0f32,
            _ => return Some(acc),
        };
        *p += 1;
        let rhs = calc_term(t, p)?;
        acc = match (acc, rhs) {
            (CalcVal::Len(a), CalcVal::Len(b)) => CalcVal::Len(a.add(b.scale(op))),
            (CalcVal::Num(a), CalcVal::Num(b)) => CalcVal::Num(a + op * b),
            _ => return None, // comprimento ± número é inválido na spec
        };
    }
}

fn calc_term(t: &[char], p: &mut usize) -> Option<CalcVal> {
    let mut acc = calc_atom(t, p)?;
    loop {
        calc_ws(t, p);
        let mul = match t.get(*p) {
            Some('*') => true,
            Some('/') => false,
            _ => return Some(acc),
        };
        *p += 1;
        let rhs = calc_atom(t, p)?;
        acc = match (acc, rhs, mul) {
            (CalcVal::Len(a), CalcVal::Num(k), true) => CalcVal::Len(a.scale(k)),
            (CalcVal::Num(k), CalcVal::Len(a), true) => CalcVal::Len(a.scale(k)),
            (CalcVal::Num(a), CalcVal::Num(b), true) => CalcVal::Num(a * b),
            (CalcVal::Len(a), CalcVal::Num(k), false) if k != 0.0 => CalcVal::Len(a.scale(1.0 / k)),
            (CalcVal::Num(a), CalcVal::Num(b), false) if b != 0.0 => CalcVal::Num(a / b),
            _ => return None, // len*len, num/len, divisão por zero: inválidos
        };
    }
}

fn calc_atom(t: &[char], p: &mut usize) -> Option<CalcVal> {
    calc_ws(t, p);
    if t.get(*p) == Some(&'(') {
        *p += 1;
        let v = calc_expr(t, p)?;
        calc_ws(t, p);
        if t.get(*p) != Some(&')') {
            return None;
        }
        *p += 1;
        return Some(v);
    }
    // número (com sinal) + unidade opcional.
    let start = *p;
    if matches!(t.get(*p), Some('-') | Some('+')) {
        *p += 1;
    }
    while matches!(t.get(*p), Some(c) if c.is_ascii_digit() || *c == '.') {
        *p += 1;
    }
    if *p == start {
        return None;
    }
    let num: f32 = t[start..*p].iter().collect::<String>().parse().ok()?;
    // unidade (letras/%).
    let ustart = *p;
    while matches!(t.get(*p), Some(c) if c.is_ascii_alphabetic() || *c == '%') {
        *p += 1;
    }
    let unit: String = t[ustart..*p].iter().collect::<String>().to_ascii_lowercase();
    use crate::style::CalcLen;
    Some(match unit.as_str() {
        "" => CalcVal::Num(num),
        "px" => CalcVal::Len(CalcLen { px: num, ..Default::default() }),
        "%" => CalcVal::Len(CalcLen { pct: num, ..Default::default() }),
        "em" => CalcVal::Len(CalcLen { em: num, ..Default::default() }),
        "rem" => CalcVal::Len(CalcLen { rem: num, ..Default::default() }),
        "vw" => CalcVal::Len(CalcLen { vw: num, ..Default::default() }),
        "vh" => CalcVal::Len(CalcLen { vh: num, ..Default::default() }),
        _ => return None, // unidade desconhecida (ch/vmin/…): calc inválido
    })
}
