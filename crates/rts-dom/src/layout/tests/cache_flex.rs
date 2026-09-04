//! Lote L: o cache de fragmentos passou a cobrir flex, grid e coluna, não só
//! o fluxo de bloco. Estes testes são a mesma pergunta de `cache.rs`
//! (`o_layout_reusado_e_igual_ao_recalculado`) feita nos três caminhos que
//! antes chamavam `layout_block` direto — mais o caso perigoso do §4.L:
//! mudar a largura do CONTAINER não pode servir um item com a largura
//! imposta antiga.

    use super::*;

    /// Conta `fragment_hits` do bloco executado, isolado do resto do processo
    /// (os testes de layout correm em paralelo na mesma thread de `cargo test`
    /// só quando são `#[test]` de um mesmo `cargo test --test-threads=1`, o que
    /// não é o caso — por isso o contador é por THREAD, e cada teste tem a sua).
    /// Sem a feature `metrics` isto é sempre zero (ver `metrics::counters::
    /// snapshot`), e por isso o teste que lê hits está marcado com `#[cfg]`.
    #[cfg(feature = "metrics")]
    fn fragment_hits_em(f: impl FnOnce()) -> u64 {
        crate::metrics::counters::reset();
        f();
        crate::metrics::counters::snapshot().fragment_hits
    }

    /// `flex-direction: row`: mutar o texto de UM item não pode mudar a
    /// geometria de ninguém — nem a do item mutado (comparado ao cálculo do
    /// zero) nem a dos outros dois (que devem ser servidos do fragmento
    /// antigo, deslocado).
    #[test]
    fn flex_row_item_mutado_bate_com_calculo_do_zero() {
        let mut dom = parse_html_to_dom(
            "<div id='linha' style='display:flex;width:600px'>               <div class='it'>um</div>               <div class='it'>dois</div>               <div class='it'>tres</div>             </div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let alvo = dom.query(".it").unwrap();

        let conferir = |dom: &Dom| {
            let cacheado = layout_cached(dom, &ctx);
            dom.clear_fragment_cache();
            let recalculado = layout_document(dom, &ctx);
            assert_eq!(
                cacheado.materialized().len(),
                recalculado.materialized().len(),
                "nº de itens diverge (flex-row)"
            );
            for (i, (a, b)) in cacheado
                .materialized()
                .iter()
                .zip(&recalculado.materialized())
                .enumerate()
            {
                assert!(
                    itens_equivalentes(a, b),
                    "flex-row, item {i} diverge:\n  reuso: {a:?}\n  zero:  {b:?}"
                );
            }
        };

        conferir(&dom);
        dom.set_text(alvo, "um texto bem mais comprido do que \"um\"");
        conferir(&dom);
    }

    /// `flex-direction: column` (via `coluna.rs`), mesma pergunta.
    #[test]
    fn flex_column_item_mutado_bate_com_calculo_do_zero() {
        let mut dom = parse_html_to_dom(
            "<div id='coluna' style='display:flex;flex-direction:column;width:300px'>               <div class='it'>um</div>               <div class='it'>dois</div>               <div class='it'>tres</div>             </div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let alvo = dom.query(".it").unwrap();

        let conferir = |dom: &Dom| {
            let cacheado = layout_cached(dom, &ctx);
            dom.clear_fragment_cache();
            let recalculado = layout_document(dom, &ctx);
            assert_eq!(
                cacheado.materialized().len(),
                recalculado.materialized().len(),
                "nº de itens diverge (flex-column)"
            );
            for (i, (a, b)) in cacheado
                .materialized()
                .iter()
                .zip(&recalculado.materialized())
                .enumerate()
            {
                assert!(
                    itens_equivalentes(a, b),
                    "flex-column, item {i} diverge:\n  reuso: {a:?}\n  zero:  {b:?}"
                );
            }
        };

        conferir(&dom);
        dom.set_text(alvo, "um texto bem mais comprido do que \"um\"");
        conferir(&dom);
    }

    /// `display:grid`, três células, mesma pergunta.
    #[test]
    fn grid_item_mutado_bate_com_calculo_do_zero() {
        let mut dom = parse_html_to_dom(
            "<div id='grade' style='display:grid;grid-template-columns:100px 100px 100px;width:320px'>               <div class='it'>um</div>               <div class='it'>dois</div>               <div class='it'>tres</div>             </div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let alvo = dom.query(".it").unwrap();

        let conferir = |dom: &Dom| {
            let cacheado = layout_cached(dom, &ctx);
            dom.clear_fragment_cache();
            let recalculado = layout_document(dom, &ctx);
            assert_eq!(
                cacheado.materialized().len(),
                recalculado.materialized().len(),
                "nº de itens diverge (grid)"
            );
            for (i, (a, b)) in cacheado
                .materialized()
                .iter()
                .zip(&recalculado.materialized())
                .enumerate()
            {
                assert!(
                    itens_equivalentes(a, b),
                    "grid, item {i} diverge:\n  reuso: {a:?}\n  zero:  {b:?}"
                );
            }
        };

        conferir(&dom);
        dom.set_text(alvo, "um texto bem mais comprido do que \"um\"");
        conferir(&dom);
    }

    /// A prova positiva: dos três itens de um flex-row, só UM mudou — os
    /// outros dois têm de aparecer como HIT no cache de fragmentos. Antes do
    /// lote L, `fragment_hits` era sempre 0 neste caminho (flex chamava
    /// `layout_block` direto, nunca `layout_block_reusing`).
    #[test]
    #[cfg(feature = "metrics")]
    fn itens_de_flex_nao_mutados_batem_no_cache() {
        let mut dom = parse_html_to_dom(
            "<div id='linha' style='display:flex;width:600px'>               <div class='it'>um</div>               <div class='it'>dois</div>               <div class='it'>tres</div>             </div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let alvo = dom.query(".it").unwrap();

        // primeira passada: povoa o cache de fragmentos.
        let _ = layout_cached(&dom, &ctx);
        dom.set_text(alvo, "um texto bem mais comprido do que \"um\"");

        let hits = fragment_hits_em(|| {
            let _ = layout_cached(&dom, &ctx);
        });
        assert!(
            hits > 0,
            "os dois itens intactos deviam bater no cache de fragmentos; hits={hits}"
        );
    }

    /// O caso perigoso: mudar a largura do CONTAINER muda o `avail_w`/o
    /// `forced_outer_w` de todo item — nenhum pode ser servido pela chave
    /// antiga. Sem `forced_outer_w`/`forced_outer_h` na `FragmentKey` (o que
    /// este lote acrescentou), um item cujo epoch não mudou bateria na MESMA
    /// entrada de cache que a largura antiga escreveu, e devolveria a
    /// geometria de outra largura — a classe silenciosa que o `CLAUDE.md`
    /// pede para nomear.
    #[test]
    fn mudar_a_largura_do_container_recalcula_os_itens() {
        let mut dom = parse_html_to_dom(
            "<div id='linha' style='display:flex;width:600px'>               <div class='it'>um</div>               <div class='it'>dois</div>               <div class='it'>tres</div>             </div>",
        );
        let ctx = LayoutCtx {
            viewport_w: 800.0,
            viewport_h: 600.0,
            measurer: &ApproxMeasurer,
        };
        let linha = dom.query("#linha").unwrap();

        // primeira passada: 600px de largura, povoa o cache.
        let _ = layout_cached(&dom, &ctx);

        // encolhe o container — nenhum epoch de FILHO muda, só o do próprio
        // `#linha` (a folha de estilo dele).
        dom.set_attr(linha, "style", "display:flex;width:150px");

        let cacheado = layout_cached(&dom, &ctx);
        dom.clear_fragment_cache();
        let recalculado = layout_document(&dom, &ctx);
        assert_eq!(
            cacheado.materialized().len(),
            recalculado.materialized().len(),
            "nº de itens diverge após encolher o container"
        );
        for (i, (a, b)) in cacheado
            .materialized()
            .iter()
            .zip(&recalculado.materialized())
            .enumerate()
        {
            assert!(
                itens_equivalentes(a, b),
                "item {i} diverge após encolher o container:\n  reuso: {a:?}\n  zero:  {b:?}"
            );
        }
        // E a régua direta: nenhum item pode continuar largo os 600px de
        // antes — os retângulos da largura NOVA são o que prova que a chave
        // (e não só a coincidência do teste anterior) fez a diferença.
        let geo = cacheado.geometry();
        for it in dom.query_all(".it") {
            let idx = dom.resolve(it).unwrap();
            let rect = geo.rects[&idx];
            assert!(
                rect.w < 100.0,
                "item {idx:?} ainda largo como no container de 600px: {rect:?}"
            );
        }
    }
