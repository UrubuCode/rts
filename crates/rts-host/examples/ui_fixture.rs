//! Roda UM arquivo `.ts` com a UI instalada, na thread PRINCIPAL.
//!
//! ```text
//! cargo run --release -p rts-host --features ui --example ui_fixture -- prog.ts
//! ```
//!
//! # Por que este exemplo faltava, e o que a falta impedia
//!
//! `run_fixture` roda um arquivo mas o processo `rts run` compila e executa numa
//! thread SECUNDÁRIA, e o winit entra em pânico ao criar o event loop fora da
//! principal — então qualquer programa que abra janela morre antes do primeiro
//! frame. `janela` roda na principal mas com o fonte EMBUTIDO no próprio Rust,
//! então medir uma variante nova custava recompilar o crate.
//!
//! Nenhum dos dois serve para o que mede um custo de UI: variantes de um mesmo
//! programa, editadas em TypeScript, rodadas sem recompilar nada. Este é o par
//! que faltava — `run_fixture` com a janela, ou `janela` com o arquivo por
//! argumento, conforme de que lado se olhe.
//!
//! # Isto não mede uma taxa de frames
//!
//! Pela razão que `janela` já dá: o `PresentMode` default é `Fifo`, então o
//! `endFrame` espera o monitor e um número de fps daqui é sobre o vsync. O que
//! um programa PODE medir aqui é o tempo do próprio trabalho — o laço que
//! enfileira, contra N crescente — que é o que a inclinação responde e o vsync
//! não mascara.

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("uso: ui_fixture <programa.ts>");
    let mut program = match rts_host::compile_graph(std::path::Path::new(&path)) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("RTS-COMPILE-ERROR: {error:?}");
            std::process::exit(2);
        }
    };
    program.run();
}
