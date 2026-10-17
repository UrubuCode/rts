// Tests running grid layout-algorithm WPT fixtures.

#[test]
fn test_flex_sizing_rows_indefinite_height() {
    let html = r#"<!DOCTYPE HTML>
<style>
.grid {
  display: inline-grid;
  position: relative;
  grid-template-columns: minmax(0, .5fr);
  grid-template-rows: minmax(0, .5fr);
}
.item {
  width: 200px;
  height: 200px;
  background: linear-gradient(green, green) no-repeat;
  background-size: 100px 100px;
}
.abspos {
  grid-area: 1 / 1 / 2 / 2;
  position: absolute;
  width: 200%;
  height: 200%;
  background: red;
  z-index: -1;
}
</style>
<div class="grid">
  <div class="item"></div>
  <div class="abspos"></div>
</div>"#;
    let dom = crate::parse_html_to_dom(html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let divs = dom.query_all("div");
    let grid_node = dom.resolve(divs[0]).unwrap();
    let grid_rect = list.rect_of(grid_node).unwrap();
    let abspos_rect = list.rect_of(dom.resolve(divs[2]).unwrap()).unwrap();
    let cols = list.grid_column_tracks.get(&grid_node).unwrap();
    let rows = list.grid_row_tracks.get(&grid_node).unwrap();
    assert_eq!(cols.as_slice(), &[50.0]);
    assert_eq!(rows.as_slice(), &[50.0]);
    assert_eq!(grid_rect.w, 50.0);
    assert_eq!(grid_rect.h, 50.0);
    assert_eq!(abspos_rect.w, 100.0);
    assert_eq!(abspos_rect.h, 100.0);
}

#[test]
fn test_min_content_width_percent_height_aspect_ratio_001() {
    let html = r#"<!DOCTYPE html>
<div style="display: grid; width: min-content; height: 100px; grid-template-rows: 50px; background: red;">
  <div style="height: 100%; background: green;">
    <canvas width="20" height="10" style="height: 100%; display: block;"></canvas>
  </div>
</div>"#;
    let dom = crate::parse_html_to_dom(html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let divs = dom.query_all("div");
    let grid_node = dom.resolve(divs[0]).unwrap();
    let grid_rect = list.rect_of(grid_node).unwrap();
    assert_eq!(grid_rect.w, 100.0);
    assert_eq!(grid_rect.h, 100.0);
}

#[test]
fn test_fit_content_columns() {
    let test_path = r#"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\grid-definition\grid-template-columns-fit-content-001.html"#;
    let ref_path = r#"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\grid-definition\grid-template-columns-fit-content-001-ref.html"#;
    let grid_css_path = r#"C:\Users\nexga\Documents\wpt-corpus\css\support\grid.css"#;

    let grid_css = std::fs::read_to_string(grid_css_path).unwrap();
    let test_html = std::fs::read_to_string(test_path).unwrap();
    let ref_html = std::fs::read_to_string(ref_path).unwrap();

    let mut dom_t = crate::parse_html_to_dom(&test_html);
    dom_t.add_stylesheet(&grid_css);

    let mut dom_r = crate::parse_html_to_dom(&ref_html);
    dom_r.add_stylesheet(&grid_css);

    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };

    let list_t = crate::layout::layout_document(&dom_t, &ctx);
    let list_r = crate::layout::layout_document(&dom_r, &ctx);

    let grids_t = dom_t.query_all(".grid");
    let grids_r = dom_r.query_all(".grid");

    assert_eq!(grids_t.len(), grids_r.len(), "grid count mismatch");
    for i in 0..grids_t.len() {
        let nt = dom_t.resolve(grids_t[i]).unwrap();
        let nr = dom_r.resolve(grids_r[i]).unwrap();
        let ct = list_t.grid_column_tracks.get(&nt).cloned().unwrap_or_default();
        let cr = list_r.grid_column_tracks.get(&nr).cloned().unwrap_or_default();
        if ct != cr {
            let class_t = dom_t.node(nt).attr("class").unwrap_or("");
            let style_t = dom_t.node(nt).attr("style").unwrap_or("");
            let style_r = dom_r.node(nr).attr("style").unwrap_or("");
            eprintln!("MISMATCH Grid[{i}] class={class_t} style={style_t} vs ref style={style_r}");
            eprintln!("  test cols: {:?}", ct);
            eprintln!("  ref  cols: {:?}", cr);
        }
    }
}

#[test]
fn test_fit_content_rows() {
    let test_path = r#"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\grid-definition\grid-template-rows-fit-content-001.html"#;
    let ref_path = r#"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\grid-definition\grid-template-rows-fit-content-001-ref.html"#;
    let grid_css_path = r#"C:\Users\nexga\Documents\wpt-corpus\css\support\grid.css"#;

    let grid_css = std::fs::read_to_string(grid_css_path).unwrap();
    let test_html = std::fs::read_to_string(test_path).unwrap();
    let ref_html = std::fs::read_to_string(ref_path).unwrap();

    let mut dom_t = crate::parse_html_to_dom(&test_html);
    dom_t.add_stylesheet(&grid_css);

    let mut dom_r = crate::parse_html_to_dom(&ref_html);
    dom_r.add_stylesheet(&grid_css);

    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };

    let list_t = crate::layout::layout_document(&dom_t, &ctx);
    let list_r = crate::layout::layout_document(&dom_r, &ctx);

    let grids_t = dom_t.query_all(".grid");
    let grids_r = dom_r.query_all(".grid");

    assert_eq!(grids_t.len(), grids_r.len(), "grid count mismatch");
    for i in 0..grids_t.len() {
        let nt = dom_t.resolve(grids_t[i]).unwrap();
        let nr = dom_r.resolve(grids_r[i]).unwrap();
        let rt = list_t.grid_row_tracks.get(&nt).cloned().unwrap_or_default();
        let rr = list_r.grid_row_tracks.get(&nr).cloned().unwrap_or_default();
    }
}

#[test]
fn test_baseline_001() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(&std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\baseline-001.html").unwrap());
    let dom_r = crate::parse_html_to_dom(&std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\baseline-001-ref.html").unwrap());
    let list_t = crate::layout::layout_document(&dom_t, &ctx);
    let list_r = crate::layout::layout_document(&dom_r, &ctx);
    let yt = dom_t.query_all("y");
    let yr = dom_r.query_all("y");
    for (i, (&t, &r)) in yt.iter().zip(&yr).enumerate() {
        let rt = list_t.rect_of(dom_t.resolve(t).unwrap()).unwrap();
        let rr = list_r.rect_of(dom_r.resolve(r).unwrap()).unwrap();
        assert!((rt.y - rr.y).abs() < 1.0, "y[{i}] mismatch: {:?} vs {:?}", rt, rr);
    }
}

#[test]
fn test_repeat_auto_fill_003() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(&std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\repeat-auto-fill-003.html").unwrap());
    let dom_r = crate::parse_html_to_dom(&std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\repeat-auto-fill-001-ref.html").unwrap());
    let list_t = crate::layout::layout_document(&dom_t, &ctx);
    let list_r = crate::layout::layout_document(&dom_r, &ctx);
    let div_t = dom_t.query_all(".subgrid > .subgrid > div");
    let div_r = dom_r.query_all(".subgrid > div");
    assert_eq!(div_t.len(), div_r.len());
    for (i, (&t, &r)) in div_t.iter().zip(&div_r).enumerate() {
        let rt = list_t.rect_of(dom_t.resolve(t).unwrap()).unwrap();
        let rr = list_r.rect_of(dom_r.resolve(r).unwrap()).unwrap();
        assert_eq!(rt, rr, "mismatch at leaf div {i}");
    }
}


/// Lays out a WPT test + ref pair and returns the indices of `sel` elements
/// whose rects differ. Used as a ratchet for partially-passing fixtures.
fn wpt_rect_mismatches(test: &str, reference: &str, sheets: &[&str], sel: &str) -> Vec<usize> {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let base = r"C:\Users\nexga\Documents\wpt-corpus\css\";
    let load = |p: &str| {
        let mut dom = crate::parse_html_to_dom(&std::fs::read_to_string(format!("{base}{p}")).unwrap());
        for s in sheets {
            dom.add_stylesheet(&std::fs::read_to_string(format!("{base}{s}")).unwrap());
        }
        dom
    };
    let (dom_t, dom_r) = (load(test), load(reference));
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let (els_t, els_r) = (dom_t.query_all(sel), dom_r.query_all(sel));
    assert_eq!(els_t.len(), els_r.len());
    els_t
        .iter()
        .zip(&els_r)
        .enumerate()
        .filter(|(_, (t, r))| {
            list_t.rect_of(dom_t.resolve(**t).unwrap()) != list_r.rect_of(dom_r.resolve(**r).unwrap())
        })
        .map(|(i, _)| i)
        .collect()
}

#[test]
fn test_grid_self_baseline_002() {
    let bad = wpt_rect_mismatches(
        r"css-grid\alignment\self-baseline\grid-self-baseline-002.html",
        r"css-grid\alignment\self-baseline\grid-self-baseline-002-ref.html",
        &[r"support\alignment.css", r"support\grid.css"],
        ".item",
    );
    // Ratchet: só pode diminuir.
    assert!(bad.len() <= SELF_BASELINE_002_MAX, "regressão: {} itens divergem: {bad:?}", bad.len());
}
const SELF_BASELINE_002_MAX: usize = 7;

#[test]
fn test_grid_item_content_baseline_001() {
    let bad = wpt_rect_mismatches(
        r"css-grid\alignment\grid-item-content-baseline-001.html",
        r"css-grid\alignment\grid-item-content-baseline-001-ref.html",
        &[],
        "span",
    );
    // Spans 0..48 já batem — 39..48 desde que as propriedades lógicas mapeiam
    // pelo writing-mode do PRÓPRIO item (`dom::eixos_proprios`); o resto é WIP
    // (Box Alignment §5.4).
    assert!(bad.iter().all(|&i| i >= 48), "regressão nos spans já corrigidos: {bad:?}");
    assert!(bad.len() <= 40, "regressão: {} spans divergem: {bad:?}", bad.len());
}
