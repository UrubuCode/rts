//! A FONTE DA RAIZ — a base do `rem`.
//!
//! `rem` é a única unidade que não depende do pai: conta sempre da fonte
//! computada do `<html>`. Era uma CONSTANTE de 16px espalhada por 17 sítios, e
//! isso torna errado o idioma mais comum do CSS moderno — `html { font-size:
//! 62.5% }`, que faz `1rem` valer 10px para a aritmética ficar redonda. Numa
//! página assim, TODOS os nossos valores em `rem` saíam 60% grandes demais.
//!
//! ## Porque é um thread-local e não um campo
//!
//! Porque é isso que os 17 sítios permitem tocar: eles constroem um
//! [`super::values::ResolveCtx`] a partir do que têm à mão (o nó, o container, o
//! viewport) e nenhum deles tem o `<html>`. Passar a raiz por parâmetro obrigava
//! a atravessá-la por toda a cadeia do layout, que é a mudança grande que esta
//! não é. O crate já resolve o mesmo problema da mesma forma para o estilo por
//! tag (`props::STYLES`) e para o epoch — é o padrão da casa, não uma exceção.
//!
//! Quem escreve é a cascade, ao computar o `<html>`; quem lê são os sítios que
//! resolvem uma dimensão. Uma mudança BUMPA o epoch de estilo, porque todo o
//! layout em cache foi medido contra a base antiga.

use std::cell::Cell;

thread_local! {
    /// A fonte computada do `<html>` em px. 16 é o default de todo o browser, e
    /// é o valor certo até a cascade dizer outro (uma página sem `html {
    /// font-size }` — a maioria — nunca muda isto).
    static ROOT_FONT: Cell<f32> = const { Cell::new(16.0) };
}

/// A base do `rem`: a fonte computada do `<html>`.
pub fn root_font_size() -> f32 {
    ROOT_FONT.with(|c| c.get())
}

/// Regista a fonte computada do `<html>`. Chamado pela cascade; ignora valores
/// não-positivos (uma folha que peça `font-size: 0` no root não pode zerar a
/// base de toda a página).
pub fn set_root_font_size(px: f32) {
    if px <= 0.0 {
        return;
    }
    ROOT_FONT.with(|c| {
        if c.get() != px {
            c.set(px);
            // O layout em cache foi medido contra a base antiga: todo `rem` da
            // árvore muda de valor com esta linha.
            super::props::bump_style_epoch();
        }
    });
}
