#[cfg(test)]
mod palavras_chave_intrinsecas {
    use crate::style::{Dimension, parse::parse_inline};
    use crate::table::tests::{geometria, rect};

    /// `width: max-content` deixa de ser DESCARTADO no parse.
    ///
    /// Respondia `None` — indistinguível de "não declarado" — e o elemento
    /// tomava a largura do pai.
    #[test]
    fn max_content_e_parseado_e_nao_descartado() {
        assert_eq!(
            parse_inline("width:max-content").width,
            Some(Dimension::MaxContent)
        );
    }

    /// `min-content` é parseado e suportado; `fit-content` continua descartado.
    #[test]
    fn min_content_e_parseado_e_fit_content_e_parseado() {
        assert_eq!(
            parse_inline("width:min-content").width,
            Some(Dimension::MinContent)
        );
        assert_eq!(
            parse_inline("width:fit-content").width,
            Some(Dimension::FitContent)
        );
    }

    /// `fit-content(<length-percentage>)` — a forma FUNCIONAL — é parseada para
    /// `FitContentLength`, distinta da keyword `fit-content`, com o argumento
    /// (px ou %) preservado no `CalcLen`.
    #[test]
    fn fit_content_funcional_e_parseado_com_o_argumento() {
        match parse_inline("width:fit-content(100px)").width {
            Some(Dimension::FitContentLength(c)) => {
                assert_eq!(c.px, 100.0, "o px do argumento tem de viajar");
                assert_eq!(c.pct, 0.0);
            }
            outro => panic!("esperava FitContentLength, veio {outro:?}"),
        }
        match parse_inline("width:fit-content(40%)").width {
            Some(Dimension::FitContentLength(c)) => assert_eq!(c.pct, 40.0),
            outro => panic!("esperava FitContentLength(%), veio {outro:?}"),
        }
    }

    /// `fit-content(L)` = `max(min-content, min(L, max-content))`: com dois
    /// blocos de 60px (min-content 60, max-content 120), `fit-content(100px)`
    /// assenta nos 100 — nem encolhe ao min nem transborda ao max (WPT
    /// `fit-content-length-percentage-001`).
    #[test]
    fn fit_content_funcional_clampa_entre_min_e_max() {
        let fonte = "<div style='width:fit-content(100px);height:50px' id='alvo'>\
            <div style='display:inline-block;width:60px'></div>\
            <div style='display:inline-block;width:60px'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert!(
            (a.w - 100.0).abs() < 1.0,
            "fit-content(100px) entre 60 e 120 tem de dar 100: {a:?}"
        );
    }

    /// Um bloco não-replaced com altura definida e `aspect-ratio` contribui
    /// `altura×razão` para o min-content do pai: `width:min-content` num pai de
    /// `<div height:100px; aspect-ratio:1/1>` dá 100, não 0 (WPT
    /// `aspect-ratio/intrinsic-size-001`).
    #[test]
    fn aspect_ratio_com_altura_contribui_para_o_min_content() {
        let fonte = "<div style='width:min-content;height:100px' id='alvo'>\
            <div style='height:100px;aspect-ratio:1/1'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert!(
            (a.w - 100.0).abs() < 1.0,
            "a largura transferida pela razão tem de ser o min-content: {a:?}"
        );
    }

    /// Um `max-width` DEFINIDO vence o mínimo TRANSFERIDO pela razão a partir de
    /// `min-height` (CSS Sizing 4 §5.1): `height:50; aspect-ratio:2/1;
    /// min-height:100; max-width:100` dá 100×100 — a altura usada é 100, a razão
    /// pede 200 de largura, e o max-width corta para 100 (WPT
    /// `block-aspect-ratio-045`). Antes o mínimo transferido (200) vencia.
    #[test]
    fn max_width_definido_vence_o_minimo_transferido_pela_razao() {
        let fonte = "<div style='background:green;height:50px;aspect-ratio:2/1;min-height:100px;max-width:100px' id='alvo'></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert_eq!((a.w, a.h), (100.0, 100.0), "{a:?}");
    }

    /// Uma `width` DEFINIDA vence o mínimo TRANSFERIDO pela razão a partir de
    /// `min-height` (CSS Sizing 4 §5.1): `width:100; aspect-ratio:2/1;
    /// max-width:150; min-height:100` fica 100×100 — a altura usada é 100, a
    /// razão pede 200 de largura de volta, mas a `width:100` declarada vence
    /// (WPT `block-aspect-ratio-048`). Antes o transferido (150) ganhava.
    #[test]
    fn width_definida_vence_o_minimo_transferido_pela_razao() {
        let fonte = "<div style='background:green;width:100px;aspect-ratio:2/1;max-width:150px;min-height:100px' id='alvo'></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert_eq!((a.w, a.h), (100.0, 100.0), "{a:?}");
    }

    /// Um `max-height` DEFINIDO transfere-se pela razão a um MÁXIMO de largura
    /// (CSS Sizing 4 §5.1): um bloco de largura auto com `aspect-ratio:1/1;
    /// max-height:100%` (=100px) NÃO estica ao contentor — a largura é capada em
    /// 100 (WPT `block-aspect-ratio-020`). Antes esticava a 1280.
    #[test]
    fn max_height_transfere_um_maximo_de_largura_pela_razao() {
        let fonte = "<div style='height:100px'><div style='aspect-ratio:1/1;max-height:100%;background:green' id='alvo'>\
            <div style='width:20px;height:10px;display:inline-block'></div>\
            <div style='width:20px;height:10px;display:inline-block'></div>\
            <div style='width:20px;height:10px;display:inline-block'></div></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 1);
        assert_eq!((a.w, a.h), (100.0, 100.0), "{a:?}");
    }

    /// O máximo transferido de `max-height` NÃO corta abaixo do mínimo de
    /// conteúdo (WPT `block-aspect-ratio-043`): `height:200; aspect-ratio:1/2;
    /// max-height:100` (máx transferido 50) com filho `width:100` fica 100 de
    /// largura, não 50 — o min-content vence o máximo transferido.
    #[test]
    fn maximo_transferido_nao_corta_o_minimo_de_conteudo() {
        let fonte = "<div style='background:green;height:200px;aspect-ratio:1/2;max-height:100px' id='alvo'>\
            <div style='width:100px'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert!((a.w - 100.0).abs() < 1.0, "043: {a:?}");
    }

    /// Uma `width` definida não é capada pelo máximo transferido de `max-height`
    /// (WPT `block-aspect-ratio-039`): `width:100; aspect-ratio:1/2;
    /// max-height:100` fica 100 de largura (o máx transferido 50 não vence).
    #[test]
    fn width_definida_nao_e_capada_pelo_maximo_transferido() {
        let fonte = "<div style='background:green;width:100px;aspect-ratio:1/2;max-height:100px' id='alvo'>\
            <div style='height:200px'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert!((a.w - 100.0).abs() < 1.0, "039: {a:?}");
    }

    /// `max-width:max-content` com `box-sizing:border-box` NÃO desconta a
    /// moldura duas vezes: a palavra-chave já resolve para conteúdo, então o
    /// padding soma-se por cima — `padding:0 20px` sobre um conteúdo de 50 dá
    /// 90, não 10 (WPT `border-box-and-max-content-001`).
    #[test]
    fn max_content_border_box_nao_desconta_moldura_em_duplicado() {
        let fonte = "<div style='border:1px solid red;width:max-content'>\
            <div style='max-width:max-content;box-sizing:border-box;padding:10px 20px' id='item'>\
            <div style='width:50px;height:50px'></div></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let item = rect(&d, &l, "div", 1);
        assert_eq!(item.w, 90.0, "{item:?}");
    }

    /// O max-content PRÓPRIO de uma caixa com `aspect-ratio` e altura definida é
    /// a largura transferida, mesmo vazia: `max-width:max-content; height:500;
    /// aspect-ratio:1; box-sizing:border-box` dá 500×500, não 40 (WPT
    /// `border-box-and-max-content-002`).
    #[test]
    fn max_content_proprio_inclui_a_largura_transferida_pela_razao() {
        let fonte = "<div style='border:1px solid red;width:max-content'>\
            <div style='max-width:max-content;height:500px;aspect-ratio:1;padding:10px 20px;box-sizing:border-box' id='item'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let item = rect(&d, &l, "div", 1);
        assert_eq!((item.w, item.h), (500.0, 500.0), "{item:?}");
    }

    /// Um item flex não-replaced com `aspect-ratio` e sem tamanho, esticado numa
    /// row de altura definida, deriva o main (largura) do cross pela razão:
    /// `display:flex;height:100` com filho `aspect-ratio:1/1` dá 100×100, não
    /// 0×100 (WPT `flex-aspect-ratio-009`).
    #[test]
    fn item_flex_esticado_deriva_main_do_cross_pela_razao() {
        let fonte = "<div style='display:flex;height:100px'>\
            <div style='background:green;aspect-ratio:1/1;min-width:0' id='item'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let item = rect(&d, &l, "div", 1);
        assert_eq!((item.w, item.h), (100.0, 100.0), "{item:?}");
    }

    /// O min-content do conteúdo é piso do main transferido pela razão num item
    /// flex não-replaced: `aspect-ratio:1/2` (main pela razão = 50) com filho
    /// `width:100` fica 100, não 50 (WPT `flex-aspect-ratio-049`).
    #[test]
    fn min_content_e_piso_do_main_transferido_no_flex() {
        let fonte = "<div style='display:flex;width:0px;height:100px'>\
            <div style='background:green;aspect-ratio:1/2' id='item'>\
            <div style='width:100px'></div></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let item = rect(&d, &l, "div", 1);
        assert!((item.w - 100.0).abs() < 1.0, "{item:?}");
    }

    /// Num item de COLUMN flex com `aspect-ratio` e sem height, o min-content do
    /// conteúdo é piso da altura (main) sobre a razão: `width:100;
    /// aspect-ratio:2/1; flex-basis:0` com filho `height:100` fica 100 de alto,
    /// não os 50 da razão (WPT `flex-aspect-ratio-004`). O caminho de bloco
    /// aplica isto mas exclui itens de flex, daí o piso no `column_shrink`.
    #[test]
    fn min_content_e_piso_da_altura_na_coluna_flex_com_razao() {
        let fonte = "<div style='display:flex;flex-direction:column'>\
            <div style='background:green;width:100px;aspect-ratio:2/1;flex-basis:0' id='item'>\
            <div style='height:100px'></div></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let item = rect(&d, &l, "div", 1);
        assert_eq!((item.w, item.h), (100.0, 100.0), "{item:?}");
    }

    /// No sizing de CONTEÚDO (min-content) a % de uma margem resolve contra ZERO
    /// (CSS Sizing 3): `width:min-content` com filho `margin-left:calc(10% +
    /// 100px)` dá 100, não 10%×viewport + 100 (WPT `calc-margins-block`). O
    /// termo absoluto do calc fica; o percentual anula-se.
    #[test]
    fn margem_percentual_resolve_contra_zero_no_min_content() {
        let fonte = "<div style='width:min-content;height:100px' id='p'>\
            <div style='margin-left:calc(10% + 100px)'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let p = rect(&d, &l, "div", 0);
        assert_eq!(p.w, 100.0, "{p:?}");
    }

    /// `height:stretch` num abspos com UM só inset vertical estica entre esse
    /// inset e a posição estática do bordo em falta: `height:stretch;
    /// bottom:100` num CB de 200 fica 100 de alto, ancorado ao topo (y=0), não
    /// 200 deslocado para y=-100 (WPT `css-sizing/stretch/abspos-2`).
    #[test]
    fn stretch_abspos_com_um_inset_estica_ate_o_static() {
        let fonte = "<div style='width:100px;height:200px;position:relative'>\
            <div style='width:100%;height:stretch;position:absolute;bottom:100px;background:green' id='a'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 1);
        assert_eq!((a.y, a.w, a.h), (0.0, 100.0, 100.0), "{a:?}");
    }

    /// Um replaced com `height:100%` + `aspect-ratio` converte a altura
    /// percentual (contra a altura DEFINIDA do pai) em largura intrínseca, e
    /// isso chega à medição de largura shrink-to-fit do pai: um `#float{height:
    /// 100px}` à volta de um `<canvas height:100%>` 1:1 fica 100 de largura, não
    /// 0 (WPT `intrinsic-percent-replaced-001`). É o threading de altura pela
    /// recursão intrínseca sem cache.
    #[test]
    fn altura_percentual_de_replaced_vira_largura_intrinseca_do_pai() {
        let fonte = "<div style='float:left;height:100px' id='f'>\
            <canvas width='10' height='10' style='height:100%'></canvas></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let f = rect(&d, &l, "div", 0);
        assert!((f.w - 100.0).abs() < 1.5, "{f:?}");
    }

    /// Um item de flex-ROW esticado propaga o cross DEFINIDO do container ao
    /// `height:%` dos seus filhos na medição intrínseca: um `<canvas height:
    /// 100%>` 1:1 dentro de um item esticado de `inline-flex; height:100` mede
    /// 100 de largura (altura 100 × razão), não a natural 10 (WPT
    /// `css-flexbox/aspect-ratio-intrinsic-size-001`).
    #[test]
    fn item_flex_row_esticado_propaga_altura_ao_height_percent_dos_filhos() {
        let fonte = "<div style='display:inline-flex;height:100px' id='fl'>\
            <div><canvas width='10' height='10' style='height:100%'></canvas></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let fl = rect(&d, &l, "div", 0);
        assert!((fl.w - 100.0).abs() < 1.5, "{fl:?}");
    }

    /// Um item flex-row esticado com `aspect-ratio` + margem/borda/padding: o
    /// cross esticado é a MARGIN-box (= altura do container), então o conteúdo é
    /// `cross - margem - frame`, e a razão dá a largura daí. `inline-flex;
    /// height:100` com item `margin:10;border:7;padding:3;aspect-ratio:1/1` dá
    /// item 80×80 e container 100 (WPT `aspect-ratio-intrinsic-size-008`).
    #[test]
    fn item_flex_esticado_com_razao_subtrai_margem_e_frame() {
        let fonte = "<div style='display:inline-flex;height:100px' id='fl'>\
            <div style='padding:3px;border:7px solid green;margin:10px;aspect-ratio:1/1' id='it'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let fl = rect(&d, &l, "div", 0);
        let it = rect(&d, &l, "div", 1);
        assert_eq!((fl.w, it.w, it.h), (100.0, 80.0, 80.0), "fl={fl:?} it={it:?}");
    }

    /// `height:%` de um item num column flex de altura CONTENT-based (sem altura
    /// própria, sem razão/flex-basis/min-height, e container sem altura) resolve
    /// para `auto` (Flexbox §4.1): um `height:50%` numa coluna sem altura dá 0,
    /// não metade do conteúdo (WPT `css-flexbox/percentage-heights-008`).
    #[test]
    fn height_percent_em_coluna_content_based_e_auto() {
        let fonte = "<div style='display:flex;flex-direction:column'>\
            <div style='display:flex;flex-direction:column'>\
            <div style='height:100px;width:100px'></div>\
            <div style='height:50%;background:red' id='pct'></div></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let pct = rect(&d, &l, "div", 3);
        assert_eq!(pct.h, 0.0, "{pct:?}");
    }

    /// Uma `aspect-ratio` CSS declarada vence a razão natural do `viewBox` de um
    /// `<svg>` (CSS Sizing 4 §5.1): `<svg viewBox='0 0 5 1' style='width:100;
    /// aspect-ratio:1/1'>` é 100×100, não 100×20 (WPT
    /// `aspect-ratio/replaced-element-007`).
    #[test]
    fn aspect_ratio_css_vence_o_viewbox_do_svg() {
        let fonte = "<svg viewBox='0 0 5 1' style='width:100px;aspect-ratio:1/1;background:green' id='s'></svg>";
        let (d, l) = geometria(fonte, 1280.0);
        let s = rect(&d, &l, "svg", 0);
        assert_eq!((s.w, s.h), (100.0, 100.0), "{s:?}");
    }

    /// Com `overflow:hidden` o piso de conteúdo NÃO se aplica (o mínimo
    /// automático é 0): `aspect-ratio:1/1; overflow:hidden; height:100` com um
    /// filho de 500px fica na razão (100), cortando o filho (WPT
    /// `block-aspect-ratio-018`). Guarda contra o piso do teste `-015`.
    #[test]
    fn overflow_hidden_anula_o_piso_de_conteudo_na_razao() {
        let fonte = "<div style='background:green;height:100px;aspect-ratio:1/1;overflow:hidden' id='alvo'>\
            <div style='width:100px;display:inline-block'></div>\
            <div style='width:500px;display:inline-block'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert_eq!((a.w, a.h), (100.0, 100.0), "{a:?}");
    }

    /// O min-content é o piso (min-width:auto) da largura derivada da razão num
    /// bloco de largura auto (CSS Sizing 4 §5.1): `height:100; aspect-ratio:1/2`
    /// com filho `width:100` fica 100 de largura, não os 50 que a razão pede —
    /// o conteúdo não cabe em 50 (WPT `block-aspect-ratio-015`).
    #[test]
    fn min_content_e_piso_da_largura_derivada_da_razao() {
        let fonte = "<div style='background:green;height:100px;aspect-ratio:1/2' id='alvo'>\
            <div style='width:100px'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert_eq!((a.w, a.h), (100.0, 100.0), "{a:?}");
    }

    /// `aspect-ratio: auto <ratio>` (em qualquer ordem) parseia a razão e marca
    /// o flag `auto`; `auto` sozinho não declara razão.
    #[test]
    fn aspect_ratio_auto_e_parseado_com_o_flag() {
        let a = parse_inline("aspect-ratio:auto 1/1");
        assert_eq!(a.aspect_ratio, Some(1.0));
        assert_eq!(a.aspect_ratio_auto, Some(true));
        let b = parse_inline("aspect-ratio:2/1 auto");
        assert_eq!(b.aspect_ratio, Some(2.0));
        assert_eq!(b.aspect_ratio_auto, Some(true));
        let c = parse_inline("aspect-ratio:3/2");
        assert_eq!(c.aspect_ratio, Some(1.5));
        assert_eq!(c.aspect_ratio_auto, Some(false));
        // `auto` sozinho não declara razão nenhuma.
        assert_eq!(parse_inline("aspect-ratio:auto").aspect_ratio, None);
    }

    /// O espelho em `max-content`: `width:max-content` num pai de
    /// `<div height:100px; aspect-ratio:1/1>` também é 100 (WPT
    /// `aspect-ratio/intrinsic-size-002`).
    #[test]
    fn aspect_ratio_com_altura_contribui_para_o_max_content() {
        let fonte = "<div style='width:max-content;height:100px' id='alvo'>\
            <div style='height:100px;aspect-ratio:1/1'></div></div>";
        let (d, l) = geometria(fonte, 1280.0);
        let a = rect(&d, &l, "div", 0);
        assert!(
            (a.w - 100.0).abs() < 1.0,
            "max-content tem de transferir a razão: {a:?}"
        );
    }

    /// A largura passa a ser a do CONTEÚDO, e transborda o pai em vez de ceder.
    #[test]
    fn max_content_mede_o_conteudo_e_transborda_o_pai() {
        let com = "<div style='width:60px'><div style='width:max-content'>                   <ul><li>Pagina principal</li><li>Conteudo destacado</li></ul></div></div>";
        let sem = "<div style='width:60px'><div>                   <ul><li>Pagina principal</li><li>Conteudo destacado</li></ul></div></div>";
        let (d1, l1) = geometria(com, 1280.0);
        let (d2, l2) = geometria(sem, 1280.0);
        let a = rect(&d1, &l1, "div", 1);
        let b = rect(&d2, &l2, "div", 1);
        assert!(a.w > 100.0, "max-content tem de medir o conteúdo: {a:?}");
        assert!((b.w - 60.0).abs() < 0.5, "sem ele, a largura do pai: {b:?}");
    }

    /// E o `max-width` continua a morder POR CIMA, como manda a spec.
    #[test]
    fn o_teto_de_max_width_continua_a_morder_sobre_o_max_content() {
        let (dom, list) = geometria(
            "<div style='width:60px'><div style='width:max-content;max-width:50px'>             <ul><li>Pagina principal muito comprida aqui</li></ul></div></div>",
            1280.0,
        );
        let d = rect(&dom, &list, "div", 1);
        assert!((d.w - 50.0).abs() < 0.5, "o teto tem de vencer: {d:?}");
    }

    #[test]
    fn test_writing_directions_002() {
        let test_html = std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\writing-directions-002.html",
        )
        .unwrap();
        let ref_html = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\writing-directions-002-ref.html").unwrap();
        let (d_t, l_t) = geometria(&test_html, 800.0);
        let (d_r, l_r) = geometria(&ref_html, 800.0);
        for (i, node) in d_t.query_all("x").iter().enumerate() {
            let r_t = l_t.rect_of(d_t.resolve(*node).unwrap()).unwrap();
            let ref_node = d_r.query_all("x")[i];
            let r_r = l_r.rect_of(d_r.resolve(ref_node).unwrap()).unwrap();
            assert!(
                (r_t.x - r_r.x).abs() < 0.5,
                "x mismatch for item {i}: {r_t:?} vs {r_r:?}"
            );
            assert!(
                (r_t.w - r_r.w).abs() < 0.5,
                "w mismatch for item {i}: {r_t:?} vs {r_r:?}"
            );
        }
    }

    #[test]
    fn test_csft() {
        let test_html = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\contribution-size-flex-tracks-001.html").unwrap();
        let (d_t, l_t) = geometria(&test_html, 800.0);
        let g = d_t.resolve(d_t.query(".grid").unwrap()).unwrap();
        let s = d_t.resolve(d_t.query(".subgrid").unwrap()).unwrap();
        let it = d_t.resolve(d_t.query(".item").unwrap()).unwrap();
        assert_eq!(
            l_t.rect_of(g),
            Some(crate::paint::Rect {
                x: 0.0,
                y: 50.0,
                w: 100.0,
                h: 100.0
            })
        );
        assert_eq!(
            l_t.rect_of(s),
            Some(crate::paint::Rect {
                x: 0.0,
                y: 50.0,
                w: 100.0,
                h: 100.0
            })
        );
        assert_eq!(
            l_t.rect_of(it),
            Some(crate::paint::Rect {
                x: 0.0,
                y: 50.0,
                w: 10.0,
                h: 25.0
            })
        );
    }

    #[test]
    fn test_wd003() {
        let test_html = std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\writing-directions-003.html",
        )
        .unwrap();
        let ref_html = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\writing-directions-003-ref.html").unwrap();
        let (d_t, l_t) = geometria(&test_html, 800.0);
        let (d_r, l_r) = geometria(&ref_html, 800.0);
        let items_t = d_t.query_all(".grid-item");
        let items_r = d_r.query_all(".grid-item");
        assert_eq!(items_t.len(), items_r.len());
        for (i, (&it, &ir)) in items_t.iter().zip(&items_r).enumerate() {
            let rt = l_t.rect_of(d_t.resolve(it).unwrap()).unwrap();
            let rr = l_r.rect_of(d_r.resolve(ir).unwrap()).unwrap();
            assert!(
                (rt.x - rr.x).abs() < 0.5,
                "item {i} x mismatch: {rt:?} vs {rr:?}"
            );
            assert!(
                (rt.y - rr.y).abs() < 0.5,
                "item {i} y mismatch: {rt:?} vs {rr:?}"
            );
        }
    }

    #[test]
    fn test_ln11() {
        let test_html = std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-011.html",
        )
        .unwrap();
        let ref_html = std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-011-ref.html",
        )
        .unwrap();
        let (d_t, l_t) = geometria(&test_html, 800.0);
        let (d_r, l_r) = geometria(&ref_html, 800.0);
        let xt = d_t.query_all("x");
        let xr = d_r.query_all("x");
        for (i, (&nt, &nr)) in xt.iter().zip(&xr).enumerate() {
            let rt = l_t.rect_of(d_t.resolve(nt).unwrap());
            let rr = l_r.rect_of(d_r.resolve(nr).unwrap());
            assert_eq!(rt, rr, "x {i} mismatch");
        }
    }
}
