//! Os LIMITES de um item flex no eixo principal: a base (`flex-basis` ou a
//! largura natural) e o tecto/piso de `max-width`/`min-width`, ambos como
//! OUTER — o que `FlexItem::base`/`main` são. Extraído de `flex.rs` (no teto
//! de 500) com o lote que deu ao item o `max-width` que lhe faltava
//! (`claude-flex-item-max-width`: o `.cover-container` do Bootstrap).

use super::*;

/// A BASE outer de um item flex no eixo principal: `flex-basis` explícita
/// (resolvida como o width — respeita box-sizing) + margens; `auto`/ausente →
/// width/conteúdo ([`child_outer_width`]). O `.col` do Bootstrap tem basis `0%`
/// → a base é só o frame (e o grow distribui o espaço).
pub(in crate::layout) fn flex_base_outer(
    dom: &Dom,
    id: NodeIdx,
    container_w: f32,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let font = font_px(&css, parent_font);
    let resolve = ResolveCtx {
        parent_content_w: container_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let basis = css.flex_basis.and_then(|d| match d {
        crate::style::Dimension::Auto => None,
        other => other.resolve(&resolve),
    });
    let Some(basis) = basis else {
        return child_outer_width(dom, id, container_w, parent_font, ctx);
    };
    let margin_h = css.margin.resolve_h(&resolve);
    if css.border_box.unwrap_or(false) {
        basis + margin_h // border-box: a basis JÁ é a caixa (pad+borda inclusos)
    } else {
        basis + margin_h + { let [_, r, _, l] = crate::style::borders::used_widths(&css); l + r } + css.padding.resolve_h(&resolve)
    }
}


/// `(max-width, min-width)` declarados do item, resolvidos e convertidos a
/// OUTER: em content-box somam o frame (padding + borda) e a margem; em
/// border-box só a margem. `None` quando não declarados.
///
/// `min-content` explícito (`Dimension::MinContent`, só possível em
/// `min-width`/`max-width` — `style/lengths.rs`) resolve para o min-content
/// REAL do item (`crate::table::min_content`, o mesmo que já serve o piso
/// automático do encolhimento) em vez de cair no `None` genérico que
/// `Dimension::resolve` dá a uma keyword que só a árvore resolve — sem isto
/// `min-width:min-content` empatava com "não declarado" e um `max-width`
/// menor vencia, ao contrário do CSS2 §10.4 (min sempre vence max em
/// conflito; `claude-flex-min-width-min-content`).
pub(in crate::layout) fn limites_do_item(
    dom: &Dom,
    id: NodeIdx,
    ccss: &ComputedStyle,
    content_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> (Option<f32>, Option<f32>) {
    let font = font_px(ccss, font_size);
    let rc = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let extra = if ccss.border_box.unwrap_or(false) {
        ccss.margin.resolve_h(&rc)
    } else {
        ccss.margin.resolve_h(&rc)
            + { let [_, r, _, l] = crate::style::borders::used_widths(ccss); l + r }
            + ccss.padding.resolve_h(&rc)
    };
    let resolve_lado = |d: Option<crate::style::Dimension>| -> Option<f32> {
        match d {
            Some(crate::style::Dimension::MinContent) => {
                Some(crate::table::min_content(dom, id, font_size, ctx))
            }
            Some(other) => other.resolve(&rc),
            None => None,
        }
    };
    (
        resolve_lado(ccss.max_width).map(|m| m + extra),
        resolve_lado(ccss.min_width).map(|m| m + extra),
    )
}

/// Shrink-to-fit (CSS2 §10.3.5): `width = min(max(pref-min, disponível),
/// pref)` — o PISO (min-content) e o TECTO (`disponível`/max-content), na
/// escala de CONTEÚDO (sem o frame de `id`, que o chamador já descontou de
/// `disponivel`). Extraído de `bloco.rs` (já acima do tecto de 500, não
/// cresce) com este lote: faltava o piso — um bloco/float sem `width` num
/// pai de largura zero (o truque que o WPT usa para simular
/// `width:min-content`) colapsava a 0 em vez de parar no min-content
/// (`claude-shrink-to-fit-sem-piso-min-content`).
pub(in crate::layout) fn largura_shrink_to_fit(
    dom: &Dom,
    id: NodeIdx,
    disponivel: f32,
    frame: f32,
    font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let piso = (crate::table::min_content(dom, id, font, ctx) - frame).max(0.0);
    disponivel.max(piso).min(content_natural_width(dom, id, font, ctx))
}

/// GROW/SHRINK de uma linha (Flexbox §9.7): espaço livre positivo distribui
/// ∝ `flex-grow` (o `.col { flex:1 0 0% }` divide igual); negativo encolhe ∝
/// `shrink×base` (itens maiores cedem mais), com PISO (`min_main`) e TECTO
/// (`max_main`). Extraído de `flex.rs` (no tecto de 500) com o lote que deu
/// ao encolhimento o tecto que só o piso tinha
/// (`claude-flex-base-size-max-width`).
pub(in crate::layout) fn resolve_grow_encolhe(
    line: &mut [super::flex::FlexItem],
    content_w: f32,
    total_gap: f32,
) {
    let sum_base: f32 = line.iter().map(|it| it.base).sum();
    let free_pre = content_w - sum_base - total_gap;
    let sum_grow: f32 = line.iter().map(|it| it.grow).sum();
    if free_pre > 0.0 && sum_grow > 0.0 {
        // O tecto (`max_main`) e o piso entram os dois em
        // `com_limites_finais`, chamada pelo caller para TODO item — inclui
        // o que nem cresce nem encolhe (`sum_grow==0`), que antes deste lote
        // saía sem tecto nenhum quando a base deixou de vir pré-capada no
        // construtor do item (`flex.rs`).
        for it in line.iter_mut() {
            it.main = it.base + free_pre * it.grow / sum_grow;
        }
    } else if free_pre < 0.0 {
        // A cada iteração repartimos o défice pelos itens ainda LIVRES
        // (`shrink*base` ponderado); um item que bateria fora do seu
        // intervalo [min_main,max_main] congela na fronteira violada e sai
        // da repartição — o défice por repartir volta para os itens que
        // sobraram, na iteração seguinte. O PISO evita que um item de texto
        // longo encolha até sobrepor-se ao próprio conteúdo (achado da
        // auditoria de 2026-09-04, `04-layout.md` finding 6). O TECTO é novo
        // neste lote: sem ele, um item com `max-width` cuja BASE já nasce
        // acima do seu próprio máximo (agora que a base deixou de vir
        // pré-capada) continuava a encolher a partir dela — um resultado
        // impossível por definição de "máximo" (`#capado` saía a 75 em vez
        // de congelar em 100).
        let n = line.len();
        let mut frozen = vec![false; n];
        let mut deficit = free_pre; // negativo
        loop {
            let weighted: f32 = line
                .iter()
                .zip(&frozen)
                .filter(|&(_, f)| !f)
                .map(|(it, _)| it.shrink * it.base)
                .sum();
            if weighted <= 0.0 || deficit >= -0.01 {
                break;
            }
            let mut novo_congelado = false;
            for (it, f) in line.iter_mut().zip(frozen.iter_mut()) {
                if *f {
                    continue;
                }
                let proposto = it.base + deficit * (it.shrink * it.base) / weighted;
                if proposto <= it.min_main {
                    it.main = it.min_main;
                    *f = true;
                    novo_congelado = true;
                } else if it.max_main.is_some_and(|m| proposto >= m) {
                    it.main = it.max_main.unwrap();
                    *f = true;
                    novo_congelado = true;
                } else {
                    it.main = proposto;
                }
            }
            if !novo_congelado {
                break; // convergiu sem ninguém bater numa fronteira: acabou.
            }
            // défice restante = o espaço por repartir pelos itens NÃO
            // congelados: o total da linha menos o que os congelados já
            // fixaram (`it.main`) menos as BASES de quem continua livre —
            // não a soma `main-base` de antes deste lote, que só conservava
            // o total enquanto ninguém congelava ACIMA da base (só o piso
            // fazia isso; o tecto agora congela ACIMA, e aquela soma perdia
            // o excedente reclamado).
            let congelado_soma: f32 = line
                .iter()
                .zip(&frozen)
                .filter(|&(_, f)| *f)
                .map(|(it, _)| it.main)
                .sum();
            let livre_base_soma: f32 = line
                .iter()
                .zip(&frozen)
                .filter(|&(_, f)| !f)
                .map(|(it, _)| it.base)
                .sum();
            deficit = (content_w - total_gap - congelado_soma - livre_base_soma).min(0.0);
            if deficit >= -0.01 {
                break;
            }
        }
    }
}

/// A HYPOTHETICAL main size FINAL de um item (Flexbox §9.7 passo 4, combinado
/// com o piso automático de min-content do §4.5): clampa `main` pelo TECTO
/// (`max_main`) e só depois pelo PISO (`min_main`) — a ordem de
/// `clamp_size`/CSS2 §10.4, onde o min vence se os dois entrarem em
/// conflito. Chamado para TODO item depois de resolvido o grow/shrink da
/// linha, incluindo o item que não cresceu nem encolheu (`free_pre>=0` sem
/// `flex-grow`): antes deste lote a BASE vinha pré-capada pelo `max-width`
/// no construtor do item (`flex.rs`), e por isso um item assim nunca
/// precisava de tecto aqui — sem a pré-capagem (ver o comentário na
/// construção do item, `flex.rs`), este é o ÚNICO sítio onde o tecto ainda
/// se aplica para esse caso (`claude-flex-base-size-max-width`). O piso
/// (`min_main`) já vivia aqui antes: um item `flex-grow:0` congela direto na
/// sua base sem nunca entrar no laço de grow/shrink, e o piso tem de valer
/// lá também. Sem redistribuir pelos outros itens o que o piso/tecto consome
/// aqui — o mesmo corte já aceite para o `max_main` do grow (`flex.rs`); o
/// encolhimento redistribui de verdade, no próprio laço. `grid_cols` fica de
/// fora: uma coluna de grid tem largura FIXA por desenho (a base já veio
/// zerada de grow/shrink em `flex.rs`), não pelo conteúdo.
pub(in crate::layout) fn com_limites_finais(
    main: f32,
    min_main: f32,
    max_main: Option<f32>,
    grid_cols: Option<i32>,
) -> f32 {
    if grid_cols.is_some() {
        main
    } else {
        main.min(max_main.unwrap_or(f32::INFINITY)).max(min_main)
    }
}

/// O piso AUTOMÁTICO de min-content (Flexbox §4.5), antes de `min-width`
/// DECLARADO entrar (esse, quando presente, substitui este resultado por
/// inteiro — a spec só liga o automático a `min-width:auto`). O automático é
/// o MENOR entre o min-content (`min_content`, já medido pelo chamador) e a
/// "specified size suggestion": o `width` do item, quando é um comprimento
/// DEFINIDO — a spec exclui a `flex-basis` desta conta de propósito, por
/// isso um `flex-basis:0` sem `width` fica no min-content puro, sem teto
/// (`claude-flex-basis-zero-min-content`, o piso do lote flex-basis-piso).
///
/// Sem este teto, um item `width:100%; aspect-ratio:1/1` cujo filho também
/// mede a `100%` tinha min-content GIGANTE (o filho mede-se à custa do pai
/// que ainda não tem largura) e o piso erguia o item bem acima do `width`
/// pedido — em `flex-aspect-ratio-resize-001` (WPT) isso encravava o
/// rasterizador tentando um canvas do tamanho desse min-content.
pub(in crate::layout) fn min_automatico(
    dom: &Dom,
    id: NodeIdx,
    min_content: f32,
    ccss: &ComputedStyle,
    content_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let font = font_px(ccss, font_size);
    let rc = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    match ccss.width.and_then(|d| d.resolve(&rc)) {
        Some(_) => min_content.min(child_outer_width(dom, id, content_w, font_size, ctx)),
        None => min_content,
    }
}
