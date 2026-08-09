//! Ler um argumento, e a decisão de aridade que este crate teve de tomar.
//!
//! # O problema
//!
//! Um nativo do motor novo tem a forma que uma função compilada tem:
//! `(env, this, a0, a1, a2, a3) -> valor`. Quatro argumentos. O que passa disso
//! vai para um vetor que o runtime guarda e que `rest_arguments` lê — mas essa
//! leitura não está exposta na superfície de host, então um nativo simplesmente
//! não enxerga o quinto argumento.
//!
//! A superfície do egui não cabe nisso: `drawMesh` tinha treze parâmetros,
//! `setCameraLookAt` onze, `drawRect` nove.
//!
//! # O que foi escolhido, e o que foi rejeitado
//!
//! **Um objeto**: `drawMesh(win, { mesh, x, y, z, … })`. Dois argumentos, e o
//! motor novo já sabe ler propriedade por nome (`get_member`), então nada
//! precisa mudar abaixo deste crate.
//!
//! Rejeitado: expandir a convenção, ou expor o vetor de rest para hosts. Ambos
//! são mudanças no motor para acomodar UMA superfície, e a chamada de treze
//! posicionais que sairia delas é pior de ler do que o objeto — `drawMesh(w, m,
//! 0, 0, 0, 0, 0, 1, 1, 1, 0xFF00FF, 0, 0)` não diz qual zero é a rotação.
//!
//! A regra que este crate segue: **até quatro, posicional; mais que isso, um
//! objeto de opções na segunda posição.** Uma superfície com duas convenções
//! misturadas ao acaso seria pior que qualquer uma delas.
//!
//! # Por que um campo ausente vale o default e não é um erro
//!
//! Este parágrafo dizia "este motor ainda não lança", e **deixou de ser
//! verdade** enquanto o porte era escrito: `af0c5a03` fez com que um nativo
//! possa levantar um erro capturável, sob a regra 8 do
//! `crates/rts-core-rwk/README.md` — todo nativo que chama código de usuário
//! pergunta se ele deixou um throw pendente antes de olhar a resposta.
//!
//! O que continua verdade é mais estreito e é uma LACUNA, não uma propriedade:
//! `throw::type_error` é `pub(in crate::entry)`, então a fábrica de erro não
//! está na superfície de host. O que um host alcança é `entry::throw(tag,
//! payload)`, que exige construir o valor do erro por fora. Enquanto for assim,
//! um argumento faltando aqui só pode virar silêncio ou um valor.
//!
//! **E um valor é o que se quer nesta superfície de qualquer modo.** `drawRect(w,
//! { x, y, w, h })` deve funcionar sem repetir borda e raio; o default é a API,
//! não o contorno da lacuna. O que a lacuna de fato custa é o caso oposto — um
//! `drawMesh(w, "texto")` desenha na origem em vez de dizer que o argumento está
//! errado. Esse é o dia de expor a fábrica, e é um pedido ao `rts-core-rwk` do
//! mesmo formato que os outros oito pares que a `modules.rs` já registra.

use rts_core_rwk::entry;

/// O número que um valor carrega, ou o default quando não carrega nenhum.
///
/// `undefined` e um objeto valem o mesmo aqui: nenhum dos dois É um número, e
/// `NaN` propagado até uma coordenada de vértice é mais difícil de rastrear que
/// um retângulo desenhado na origem.
pub fn number(value: u64, default: f64) -> f64 {
    match entry::number_of(value) {
        Some(number) if number.is_finite() => number,
        _ => default,
    }
}

/// O mesmo, truncado — cores, flags e contagens cruzam como `i64`.
///
/// # Reuse-check: o mais próximo é `rts_core_rwk::value::to_int32`, e não serve
///
/// Aquela é a aritmética de OPERADOR BITWISE: reduz módulo 2^32 e reinterpreta,
/// porque é isso que `x | 0` faz. Aqui o valor é uma largura, uma contagem ou
/// uma fase de tecla, e envolver transformaria um número absurdamente grande num
/// negativo plausível — que é pior do que o absurdo, porque um `-1` de largura
/// parece um código de erro.
///
/// Para as cores as duas coincidem: o consumidor faz `as u32`, e `to_int32` de
/// `0xFFFFFFFF` dá `-1`, cujo padrão de bits É `0xFFFFFFFF`.
///
/// Isto trunca em vez de envolver, deliberadamente, e é o comportamento que a
/// ABI antiga já tinha ao declarar o parâmetro como `I64`.
pub fn integer(value: u64, default: i64) -> i64 {
    number(value, default as f64) as i64
}

/// Um handle de janela ou de malha.
///
/// Um handle é um contador pequeno, então cabe exato num double — o que o
/// torna seguro de carregar como `number` no TS, ao contrário de um ponteiro.
pub fn handle(value: u64) -> u64 {
    number(value, 0.0).max(0.0) as u64
}

/// O texto de um valor que É uma string, vazio para qualquer outro.
///
/// `string_in` e não `text_in`: o segundo é `ToString` e responde `"42"` para o
/// número 42, o que transforma um argumento trocado num rótulo plausível em vez
/// de num vazio visível.
pub fn text(value: u64) -> String {
    entry::with_runtime(|context| entry::string_in(context, value)).unwrap_or_default()
}

/// Os campos de um objeto de opções, lidos num único empréstimo do contexto.
///
/// Um empréstimo por campo funcionaria e custaria treze num `drawMesh` por
/// draw. Mais importante que o custo: `with_runtime` segura o `RefCell` durante
/// o corpo, e o hábito de aninhá-lo é o que vira um empréstimo reentrante — que
/// num quadro `extern "C"` não desenrola e portanto **aborta o processo**.
/// Uma leitura, uma vez, é também a forma que não tem como errar isso.
pub fn fields(object: u64, names: &[&str]) -> Vec<u64> {
    entry::with_runtime(|context| {
        names
            .iter()
            .map(|name| entry::get_member(context, object, name))
            .collect()
    })
}

/// Os bytes de um `Uint8Array`/`Float32Array`/`DataView`, ou nada.
///
/// # Por que uma view tipada e não um ponteiro
///
/// A superfície antiga recebia `vertsPtr: number` — o endereço cru de um buffer,
/// que o TS obtinha e o Rust desreferenciava. Isso é uma leitura arbitrária de
/// memória a partir de um número que um programa pode calcular, e no motor novo
/// seria pior: o coletor move células, então o endereço que o TS leu pode não
/// ser mais o buffer quando o Rust o usa.
///
/// `bytes_of` responde uma CÓPIA da janela que a view descreve. A cópia é o que
/// torna a fronteira segura, e é paga por upload — não por frame.
pub fn bytes(value: u64) -> Option<Vec<u8>> {
    entry::with_runtime(|context| entry::bytes_of(context, value))
}

/// `undefined`, para um nativo que não responde nada.
pub fn nothing() -> u64 {
    entry::undefined_value()
}

/// Um número como valor.
pub fn from_number(value: f64) -> u64 {
    entry::make_number(value)
}

/// Um booleano como valor — `true`/`false`, não 1/0.
///
/// A superfície antiga respondia `i64` porque a ABI não tinha booleano, e o TS
/// comparava com `1`. Aqui existe um booleano de verdade, então `isOpen()` é
/// usável direto num `while` sem que o programa saiba disso.
pub fn from_bool(value: bool) -> u64 {
    entry::boolean_value(value)
}

/// Uma string como valor.
pub fn from_text(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}
