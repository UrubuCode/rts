//! O NOME de uma declaração CSS: os tokens entre o início do bloco e o `:`.
//!
//! Extraído de `syntax.rs` (no teto de linhas, "não cresce" — `PLAN.md` §1)
//! para trazer aqui a correcção que faltava: CSS Syntax §5.4.4 permite
//! espaço em branco de QUALQUER lado do `:` — `flex-direction : column` é
//! tão válido quanto `flex-direction: column`. O laço original só aceitava
//! `Ident`/`Delim('-')` no intervalo; um token de espaço aí caía no
//! `_ => return None` e descartava a declaração INTEIRA — as 4 regras do WPT
//! `flex-direction.html` escrevem exactamente `prop : valor`, e
//! `tests/css/claude-declaracao-espaco-antes-dois-pontos.html` fixa o caso.

use crate::style::syntax::{ComponentValue, Token, TokenKind};

/// `None` se algum token do intervalo não fizer parte de um nome válido —
/// identificador, hífen solto (`Delim('-')`, como o tokenizer separa um nome
/// composto) ou espaço em branco/comentário, que se ignora.
pub(crate) fn nome_da_declaracao(name_values: &[ComponentValue]) -> Option<String> {
    let mut name = String::new();
    for value in name_values {
        match value {
            ComponentValue::Token(Token {
                kind: TokenKind::Ident(part),
                ..
            }) => name.push_str(part),
            ComponentValue::Token(Token {
                kind: TokenKind::Delim(part),
                ..
            }) if *part == '-' => name.push('-'),
            value if value.is_trivia() => {}
            _ => return None,
        }
    }
    (!name.is_empty()).then_some(name)
}
