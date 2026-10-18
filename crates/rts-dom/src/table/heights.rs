//! Table row height measurement and excess space distribution (CSS 2.1 §17.5.3).
use super::*;

pub(super) fn medir_e_distribuir_alturas(
    dom: &Dom,
    g: &Grid,
    ts: &TableStyle,
    definite_h: Option<f32>,
    largura_de: impl Fn(usize, usize) -> f32,
    ctx: &LayoutCtx,
) -> Vec<f32> {
    let mut alturas = vec![0.0f32; g.rows.len()];
    let mut pendentes: Vec<(usize, usize, f32)> = Vec::new();
    for (ri, row) in g.rows.iter().enumerate() {
        for c in &row.cells {
            if c.col >= g.cols {
                continue;
            }
            let w = largura_de(c.col, c.colspan);
            let (_, h) = crate::layout::measure_block(
                dom,
                c.node,
                c.caixa,
                w,
                None,
                Some(w),
                None,
                false,
                ctx,
            );
            if c.rowspan <= 1 {
                alturas[ri] = alturas[ri].max(h);
            } else {
                let ultima = (ri + c.rowspan - 1).min(g.rows.len() - 1);
                pendentes.push((ultima, ultima - ri + 1, h));
            }
        }
    }
    for (ultima, n, h) in pendentes {
        let inicio = ultima + 1 - n;
        let ja: f32 = alturas[inicio..=ultima].iter().sum::<f32>() + (n - 1) as f32 * ts.spacing_v;
        if h > ja {
            alturas[ultima] += h - ja;
        }
    }

    // CSS 2.1 §17.5.3: Se a altura da tabela for maior que a soma das linhas,
    // o excesso é distribuído entre as linhas.
    if let Some(dh) = definite_h {
        let vaos_v = (g.rows.len() + 1) as f32 * ts.spacing_v;
        let soma: f32 = alturas.iter().sum::<f32>() + vaos_v;
        if dh > soma && !g.rows.is_empty() {
            let extra = dh - soma;
            let mut auto_rows = Vec::new();
            for (ri, row) in g.rows.iter().enumerate() {
                let has_fixed = row.cells.iter().any(|c| {
                    dom.computed_style_idx(c.node).and_then(|cs| cs.height).is_some()
                });
                if !has_fixed {
                    auto_rows.push(ri);
                }
            }
            let targets = if auto_rows.is_empty() {
                (0..g.rows.len()).collect::<Vec<_>>()
            } else {
                auto_rows
            };
            let per_row = extra / targets.len() as f32;
            for ri in targets {
                alturas[ri] += per_row;
            }
        }
    }

    alturas
}
