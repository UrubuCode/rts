//! Ler um argumento que atravessou a fronteira, e devolver um valor para ela.
//!
//! Mesma forma que o `rts-ui` usa, pela mesma razão: um nativo do motor novo
//! recebe `(env, this, a0, a1, a2, a3)` e devolve um `u64` opaco, e cada
//! conversão precisa decidir o que fazer com um argumento que não é do tipo
//! esperado. Aqui a resposta é sempre um DEFAULT, nunca um pânico — a fronteira
//! de um nativo é `extern "C"` e um pânico dentro dela não desenrola.
//!
//! ## O que este módulo NÃO duplica
//!
//! O `rts-ui::value` existe e tem funções de nome igual. Não são reusadas
//! porque isto teria de depender do `rts-ui`, e o `rts-ui` depende de janela,
//! GPU e winit — o DOM headless perderia a razão de ser. As três funções aqui
//! são as três que este crate usa, e não a superfície inteira daquele: `fields`
//! (o objeto de opções de treze parâmetros do `drawMesh`) não tem análogo aqui,
//! porque nenhum primitivo do DOM passa de quatro argumentos.

use rts_core::entry;

/// O número que um valor carrega, ou o default quando não carrega nenhum.
pub fn number(value: u64, default: f64) -> f64 {
    match entry::number_of(value) {
        Some(number) if number.is_finite() => number,
        _ => default,
    }
}

/// O mesmo, truncado. Contagens, índices e o `NodeId` versionado cruzam como
/// `i64` — e `-1` é a sentinela "nó nenhum" da ABI, que precisa sobreviver ao
/// arredondamento (por isso truncar e não envolver).
pub fn integer(value: u64, default: i64) -> i64 {
    number(value, default as f64) as i64
}

/// Um handle de documento. Contador pequeno, cabe exato num double.
pub fn handle(value: u64) -> u64 {
    number(value, 0.0).max(0.0) as u64
}

/// O texto de um valor que É uma string, vazio para qualquer outro.
///
/// `string_in` e não `text_in` pela mesma razão do `rts-ui`: o segundo
/// responderia `"42"` para o número 42, transformando um argumento trocado num
/// nome de tag plausível em vez de num vazio visível.
pub fn text(value: u64) -> String {
    entry::with_runtime(|context| entry::string_in(context, value)).unwrap_or_default()
}

/// Um número como valor de retorno.
pub fn num(v: f64) -> u64 {
    entry::make_number(v)
}

/// Um inteiro como valor de retorno (contagens, ids, booleanos-como-0/1).
pub fn int(v: i64) -> u64 {
    entry::make_number(v as f64)
}

/// Uma string como valor de retorno.
pub fn string(s: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, s))
}

/// `undefined` — o retorno de quem não devolve nada.
pub fn nothing() -> u64 {
    entry::undefined_value()
}
