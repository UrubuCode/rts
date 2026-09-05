//! `flex-basis` e `flex-shrink` no eixo de COLUNA — espelha o que `flex.rs`
//! já tem no eixo horizontal (base/min-content/shrink iterativo,
//! `flex.rs:180-224` e `308-370`), só que a base é a ALTURA e o `%` do
//! `flex-basis` resolve contra a altura do container, não a largura.
//!
//! Extraído de `coluna.rs` (que só tinha o ramo de `flex-grow`) para não
//! passar o teto de 500 linhas com a lógica nova — `layout_children_column`
//! ganharia ~90 linhas só com isto. Lote `flex-coluna-shrink` (2026-09-04):
//! antes, `coluna.rs` nunca lia `flex-basis`/`flex-shrink`, e um item de
//! coluna com conteúdo maior do que o espaço principal transbordava em vez de
//! encolher (achado da auditoria de 2026-09-04, `04-layout.md`).

use super::*;

/// A BASE outer de um item de coluna no eixo principal (a ALTURA): `flex-basis`
/// explícito (resolvido contra a altura do container — `%` fica `None`/`auto`
/// quando ela é indefinida, como `resolve_height` já faz para `height`) mais
/// margem-v e frame; `auto`/ausente cai no `natural_h` já medido em PASSO 1
/// (que já reflete `height`/conteúdo — o que `flex-basis:auto` pede, CSS
/// Flexbox §4.5: "use the specified size suggestion if it exists").
pub(in crate::layout) fn base_outer(
    ccss: &ComputedStyle,
    natural_h: f32,
    container_h: Option<f32>,
    container_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let resolve = ResolveCtx {
        parent_content_w: container_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let basis = ccss.flex_basis.and_then(|d| match d {
        crate::style::Dimension::Auto => None,
        other => resolve_height(Some(other), container_h, &resolve),
    });
    let Some(basis) = basis else {
        return natural_h;
    };
    let margin_v = ccss.margin.resolve_v(&resolve);
    if ccss.border_box.unwrap_or(false) {
        basis + margin_v
    } else {
        let [t, _, b, _] = crate::style::borders::used_widths(ccss);
        basis + margin_v + t + b + ccss.padding.resolve_v(&resolve)
    }
}

/// O piso AUTOMÁTICO de `min-height` no eixo principal de coluna (CSS Flexbox
/// §4.5, "automatic minimum size"): zero quando o overflow NÃO é visível no
/// eixo vertical (a exceção que o próprio spec cita — um item com
/// `overflow-y:scroll` pode encolher até nada, o conteúdo rola por dentro);
/// senão, o conteúdo NÃO comprime no eixo de bloco como o texto comprime no
/// eixo inline (não há "min-content height" por quebra de linha), então o
/// piso é a própria altura natural — MAS só quando essa altura veio do
/// CONTEÚDO (sem `height` declarado): um `height:50px` some do numerador,
/// mas o item continua sem conteúdo próprio nenhum a proteger.
///
/// CORTE dito: um item com `height` declarado E conteúdo que por si só
/// exigisse mais do que o encolhimento permite (ex.: texto que não cabe)
/// devia manter esse piso — não temos uma medição de "altura mínima do
/// conteúdo, ignorando `height`" sem uma segunda passada de layout, e
/// nenhuma das fixtures deste lote precisa dela. `min-height` DECLARADO
/// (lido pelo chamador, `resolve_height(ccss.min_height, ...)`) continua a
/// vencer este automático, como no eixo horizontal.
pub(in crate::layout) fn min_main_auto(ccss: &ComputedStyle, natural_h: f32) -> f32 {
    let overflow_visible =
        ccss.overflow_y.unwrap_or(crate::scrollbar::Overflow::Visible)
            == crate::scrollbar::Overflow::Visible;
    if !overflow_visible {
        0.0
    } else if ccss.height.is_none() {
        natural_h
    } else {
        0.0
    }
}

/// O piso de `min-height` no eixo principal de coluna: DECLARADO vence
/// sempre o automático. `min-content` é o caso à parte — resolve para
/// `natural_h` (o motor não distingue min-content de max-content no eixo de
/// bloco, CSS Sizing 3 §2.1) e, ao contrário do automático, NÃO some sob
/// overflow não-visível: é um número que o autor escreveu, não uma
/// inferência (achado ao medir `flex-item-min-height-min-content-overflow`
/// — a régua contra a fixture anterior, onde `overflow:auto` zera o
/// automático, tinha zerado este também).
pub(in crate::layout) fn min_main(
    ccss: &ComputedStyle,
    natural_h: f32,
    container_h: Option<f32>,
    resolve: &ResolveCtx,
) -> f32 {
    if ccss.min_height == Some(crate::style::Dimension::MinContent) {
        return natural_h;
    }
    resolve_height(ccss.min_height, container_h, resolve).unwrap_or_else(|| min_main_auto(ccss, natural_h))
}


/// ENCOLHIMENTO com piso de `min_main` (CSS Flexbox §9.7) — a mesma iteração
/// de congelamento de `flex.rs:319-370`, extraída para slices paralelas em
/// vez de reusar `FlexItem` (que carrega campos do eixo horizontal, como
/// `max_main`/`auto_esq`, que a coluna não tem ainda — ver o corte no
/// cabeçalho de `coluna.rs`). Devolve o `main` final de cada item, na mesma
/// ordem de `bases`. `free_pre >= 0.0` devolve `bases` sem tocar (sem
/// défice: quem cresce é o `flex-grow`, tratado à parte em `coluna.rs`).
pub(in crate::layout) fn shrink(bases: &[f32], shrinks: &[f32], mins: &[f32], free_pre: f32) -> Vec<f32> {
    let n = bases.len();
    let mut main: Vec<f32> = bases.to_vec();
    if free_pre >= 0.0 {
        return main;
    }
    let mut frozen = vec![false; n];
    let mut deficit = free_pre; // negativo
    loop {
        let weighted: f32 = (0..n)
            .filter(|&i| !frozen[i])
            .map(|i| shrinks[i] * bases[i])
            .sum();
        if weighted <= 0.0 || deficit >= -0.01 {
            break;
        }
        let mut novo_congelado = false;
        for i in 0..n {
            if frozen[i] {
                continue;
            }
            let proposto = bases[i] + deficit * (shrinks[i] * bases[i]) / weighted;
            if proposto <= mins[i] {
                main[i] = mins[i];
                frozen[i] = true;
                novo_congelado = true;
            } else {
                main[i] = proposto;
            }
        }
        if !novo_congelado {
            break; // convergiu sem ninguém bater no piso: acabou.
        }
        deficit = (0..n)
            .filter(|&i| !frozen[i])
            .map(|i| main[i] - bases[i])
            .sum::<f32>()
            .min(0.0);
        if deficit >= -0.01 {
            break;
        }
    }
    main
}
