//! O caso do grupo inline SEM NENHUM átomo com corpo — só `Marker`s (e
//! whitespace entre eles). Extraído de `linha.rs` (§0 do `PLAN.md`: esse
//! ficheiro está no teto de 500 linhas e não cresce), para o lote S-inline
//! que fecha `#rotulo-com`/`#rotulo-sem` de `claude-sel-has.html`.

use super::*;

/// Um grupo assim não cria linha — a altura do bloco não muda, exatamente
/// como antes desta função existir — mas cada `Marker` ainda é um elemento
/// do documento, e o Blink dá-lhe um retângulo 0×0 na posição onde a linha
/// teria começado. Dois `<span></span>` seguidos, sem texto nem elemento
/// nenhum entre eles a abrir uma linha de verdade, é exatamente
/// `claude-sel-has.html`: sem esta chamada eles ficavam sem geometria
/// NENHUMA (nem a chave existia em `node_rects`), que é pior do que 0×0 para
/// hit-test e para `getBoundingClientRect`.
///
/// Todos os `Marker`s do grupo caem no MESMO ponto (`x`, `y`, o início do
/// que seria a linha): um marker não tem largura, então nenhum deles avança
/// o cursor — não há "posição seguinte" a calcular.
pub(in crate::layout) fn registar_markers_sem_linha(
    list: &mut DisplayList,
    x: f32,
    y: f32,
    runs: &[InlineRun],
) {
    for r in runs {
        if let Some((idx, AtomicKind::Marker)) = r.atomic {
            crate::inline_box::union_rect(list, idx, Rect::new(x, y, 0.0, 0.0));
        }
    }
}
