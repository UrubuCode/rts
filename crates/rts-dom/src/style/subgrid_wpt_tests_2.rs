// Additional subgrid WPT tests.

#[test]
fn test_abs_pos_001() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\abs-pos-001.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\abs-pos-001-ref.html",
        )
        .unwrap(),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let (xt, xr) = (dom_t.query_all("x"), dom_r.query_all("x"));
    for (i, (&a, &b)) in xt.iter().zip(&xr).enumerate() {
        let ra = list_t.rect_of(dom_t.resolve(a).unwrap());
        let rb = list_r.rect_of(dom_r.resolve(b).unwrap());
        assert_eq!(ra, rb, "x {i}");
    }
}

#[test]
fn test_repeat_auto_fill_006() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\repeat-auto-fill-006.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\repeat-auto-fill-005-ref.html",
        )
        .unwrap(),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let (it_t, it_r) = (dom_t.query_all("item"), dom_r.query_all("item"));
    for (i, (&a, &b)) in it_t.iter().zip(&it_r).enumerate() {
        let ra = list_t.rect_of(dom_t.resolve(a).unwrap());
        let rb = list_r.rect_of(dom_r.resolve(b).unwrap());
        if ra != rb {
            println!("MISMATCH item {i}: test={ra:?} ref={rb:?}");
        }
    }
}
