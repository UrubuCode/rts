//! De onde os bytes de uma imagem vêm — perguntado, nunca sabido.
//!
//! O render pinta `DisplayItem::Image` a partir de um *handle* de buffer mais um
//! deslocamento. Quem sabe o que aquele handle significa é o MOTOR, e existem
//! dois: o antigo guarda um `Entry::Buffer` no `HandleTable`, o novo guarda uma
//! `View` sobre uma célula da região. Nomear qualquer um dos dois aqui prenderia
//! o crate de render a um deles — que é exatamente o acoplamento que este porte
//! existe para desfazer.
//!
//! Então o render pergunta, e quem instalou a fonte responde. `None` enquanto
//! ninguém instalou uma: uma imagem que não pinta é o resultado honesto de um
//! host que não disse de onde ler os pixels, e é preferível a um crate de render
//! que só compila junto com um motor específico.

use std::cell::Cell;

/// Como se lê `len` bytes a partir de `offset` dentro do buffer que `handle`
/// nomeia. `None` quando o handle não existe ou o intervalo não cabe.
pub type PixelSource = fn(handle: u64, offset: u64, len: usize) -> Option<Vec<u8>>;

thread_local! {
    /// A fonte instalada nesta thread — a mesma onde o `UiCtx` vive, porque o
    /// contexto do motor novo também é por thread e uma fonte global mentiria
    /// sobre qual heap está sendo lido.
    static SOURCE: Cell<Option<PixelSource>> = const { Cell::new(None) };
}

/// Instala a fonte de pixels desta thread.
pub fn set_source(source: PixelSource) {
    SOURCE.with(|slot| slot.set(Some(source)));
}

/// Os bytes que o render precisa, se alguém souber respondê-los.
pub fn fetch(handle: u64, offset: u64, len: usize) -> Option<Vec<u8>> {
    let source = SOURCE.with(|slot| slot.get())?;
    source(handle, offset, len)
}
