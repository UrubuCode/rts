//! O cross-size de uma linha flex (CSS Flexbox §9.4): a fronteira entre uma
//! ÚNICA linha (usa sempre a altura DEFINIDA do contentor, mesmo quando um
//! item a excede ou fica aquém dela) e VÁRIAS linhas (`align-content`
//! distribui o espaço sobrante entre elas — negativo incluído, porque a
//! CSS Box Alignment não dá `center`/`flex-end` um fallback `safe` por
//! omissão).
//!
//! Extraído de `flex.rs` (que já estava no tecto de 500 linhas do crate)
//! para o lote `flex-cross-size` (2026-09-04): as duas perguntas eram
//! parágrafos inline ali, cada um com um bug próprio —
//! `flexbox-overflow-horiz-001`/`flexbox-flex-wrap-horiz-001` (linha única)
//! e `flex-align-content-center` (linhas múltiplas). `flex.rs` mantém só o
//! gancho de uma linha em cada um dos dois sítios.

use crate::style::JustifyContent;

/// O cross-size de uma linha ÚNICA — com ou sem `wrap`, o resultado é o
/// mesmo: `flexbox-overflow-horiz-001` (sem wrap) e
/// `flexbox-flex-wrap-horiz-001` (com wrap, um único item por linha) pedem
/// exatamente a mesma resposta. Quando o contentor tem altura DEFINIDA
/// (`container_cross_h > 0.0`, a mesma convenção de "0 = indefinida" já
/// usada pelo resto de `flex.rs`), ela vence sempre — um item que a excede
/// transborda em vez de a redefinir, e um item mais pequeno estica contra
/// ela (`align-items: stretch`).
///
/// Antes disto, `flex.rs` só usava a altura do contentor quando ela era
/// MAIOR que a do maior item — o oposto do caso que o `overflow` existe
/// para testar, e o motivo de `#pequeno` (com `margin-bottom`, sem `height`
/// própria) esticar contra o item GRANDE do lado em vez de contra o
/// contentor.
///
/// `None` devolve a decisão ao chamador (contentor sem altura definida, ou
/// mais de uma linha — aí quem decide é `items_h` + o que o
/// `align-content` de [`distribuir_align_content`] tiver esticado).
pub(in crate::layout) fn cross_unica_linha(n_lines: usize, container_cross_h: f32) -> Option<f32> {
    (n_lines == 1 && container_cross_h > 0.0).then_some(container_cross_h)
}

/// `align-content` em multi-linha (CSS Flexbox §8.4 / Box Alignment §8.3):
/// devolve `(leading, between, stretch_extra)` para o chamador somar à
/// posição/altura de cada linha.
///
/// O espaço livre (`container_cross_h - estimativa`) pode ficar NEGATIVO
/// quando as linhas transbordam a altura do contentor — a spec não dá
/// `center`/`flex-end` um fallback `safe` por omissão, então o `leading`
/// fica negativo e o bloco de linhas transborda SIMETRICAMENTE para cima e
/// para baixo (`flex-align-content-center`: 2 linhas de 40 num contentor de
/// 64 dão `leading = (64-80)/2 = -8`, não `0`). `justify_offsets` já sabe
/// tratar `free<=0.0` — é a mesma função que o `justify-content` do eixo
/// principal usa para overflow — por isso não duplicamos essa lógica aqui.
///
/// Só o ramo SEM `align-content` declarado (stretch por omissão,
/// `distribuir_align_content` devolve `stretch_extra`) grampeia o livre a
/// `≥0`: aí um livre negativo significaria ENCOLHER uma linha abaixo do seu
/// próprio conteúdo, e nada na spec pede isso — o piso de conteúdo já é
/// aplicado antes (`items_h`), não é este cálculo que o violaria.
pub(in crate::layout) fn distribuir_align_content(
    declarado: Option<JustifyContent>,
    container_cross_h: f32,
    estimativa: f32,
    n_lines: usize,
) -> (f32, f32, f32) {
    match declarado {
        Some(v) => {
            let free = container_cross_h - estimativa;
            let (leading, between) = super::coluna::justify_offsets(v, free, n_lines);
            (leading, between, 0.0)
        }
        None => {
            let free = (container_cross_h - estimativa).max(0.0);
            (0.0, 0.0, free / n_lines as f32)
        }
    }
}
