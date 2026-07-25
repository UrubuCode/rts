//! SUÍTE DE PERFORMANCE — não roda por padrão.
//!
//! Cada teste aqui corresponde a UMA issue de performance aberta. Ele FALHA
//! enquanto a issue existe e PASSA quando ela for corrigida: é a especificação
//! executável do que "corrigido" significa, com o número medido junto do código.
//!
//! ## Como rodar
//!
//! ```text
//! cargo test --release --test performance -- --ignored           # todos
//! cargo test --release --test performance -- --ignored method    # um assunto
//! ```
//!
//! ## Por que `#[ignore]`
//!
//! Estes testes falham DE PROPÓSITO hoje — são o registro de um problema
//! conhecido, não uma regressão nova. Deixá-los no caminho padrão pintaria a
//! suíte de vermelho permanentemente e ensinaria a ignorar falha, que é o pior
//! resultado possível para um teste. Rodam quando alguém for trabalhar em
//! performance do runtime, que é quando a resposta importa.
//!
//! ## Por que `--release`
//!
//! Em debug o próprio código gerado não é representativo; uma razão medida ali
//! não diz nada sobre o binário que os usuários rodam.
//!
//! ## Por que binário separado
//!
//! Pelo mesmo motivo de `leak.rs`: as medições de tempo competem com os outros
//! testes por CPU quando rodam no binário paralelo da lib. Um binário próprio dá
//! a elas o processo inteiro.

mod performance {
    pub mod method_vs_free;
    pub mod now_resolution;
}
