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
        assert_eq!(ra, rb, "item {i}");
    }
}

#[test]
fn test_line_names_007() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-007.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-007-ref.html",
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
fn test_grid_gap_008() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-008.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-008-ref.html",
        )
        .unwrap(),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let (st, sr) = (dom_t.query_all("span"), dom_r.query_all("span"));
    for (i, (&a, &b)) in st.iter().zip(&sr).enumerate() {
        let ra = list_t.rect_of(dom_t.resolve(a).unwrap());
        let rb = list_r.rect_of(dom_r.resolve(b).unwrap());
        assert_eq!(ra, rb, "span {i}");
    }
}

#[test]
fn test_grid_gap_009() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-009.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-009-ref.html",
        )
        .unwrap(),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let (st, sr) = (dom_t.query_all("span"), dom_r.query_all("span"));
    for (i, (&a, &b)) in st.iter().zip(&sr).enumerate() {
        let ra = list_t.rect_of(dom_t.resolve(a).unwrap());
        let rb = list_r.rect_of(dom_r.resolve(b).unwrap());
        assert_eq!(ra, rb, "span {i}");
    }
}


#[test]
fn test_orthogonal_writing_mode_006() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\orthogonal-writing-mode-006.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\orthogonal-writing-mode-006-ref.html",
        )
        .unwrap(),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let (divs_t, divs_r) = (dom_t.query_all("div"), dom_r.query_all("div"));
    for (i, (&a, &b)) in divs_t.iter().zip(&divs_r).enumerate() {
        let ra = list_t.rect_of(dom_t.resolve(a).unwrap());
        let rb = list_r.rect_of(dom_r.resolve(b).unwrap());
        assert_eq!(ra, rb, "div {i}");
    }
}

#[test]
fn test_orthogonal_writing_mode_005() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\orthogonal-writing-mode-005.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\orthogonal-writing-mode-005-ref.html",
        )
        .unwrap(),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let (divs_t, divs_r) = (dom_t.query_all("div"), dom_r.query_all("div"));
    for (i, (&a, &b)) in divs_t.iter().zip(&divs_r).enumerate() {
        let ra = list_t.rect_of(dom_t.resolve(a).unwrap());
        let rb = list_r.rect_of(dom_r.resolve(b).unwrap());
        assert_eq!(ra, rb, "div {i}");
    }
}

fn assert_rect_approx(a: Option<crate::paint::Rect>, b: Option<crate::paint::Rect>) {
    let (Some(a), Some(b)) = (a, b) else {
        assert_eq!(a, b);
        return;
    };
    assert!(
        (a.x - b.x).abs() < 0.05
            && (a.y - b.y).abs() < 0.05
            && (a.w - b.w).abs() < 0.05
            && (a.h - b.h).abs() < 0.05,
        "{a:?} != {b:?}"
    );
}

#[test]
fn test_subgrid_baseline_001() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\subgrid-baseline-001.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\subgrid-baseline-001-ref.html",
        )
        .unwrap(),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let yt = list_t.rect_of(dom_t.resolve(dom_t.query_all("y")[0]).unwrap());
    let yr = list_r.rect_of(dom_r.resolve(dom_r.query_all("y")[0]).unwrap());
    assert_rect_approx(yt, yr);

    let st = list_t.rect_of(dom_t.resolve(dom_t.query_all(".subgrid")[0]).unwrap());
    let sr = list_r.rect_of(dom_r.resolve(dom_r.query_all(".subgrid")[0]).unwrap());
    assert_rect_approx(st, sr);
}

#[test]
fn test_subgrid_baseline_004() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\subgrid-baseline-004.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\subgrid-baseline-004-ref.html",
        )
        .unwrap(),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    for sel in [".first", ".second"] {
        let (dt, dr) = (dom_t.query_all(sel), dom_r.query_all(sel));
        for (i, (&a, &b)) in dt.iter().zip(&dr).enumerate() {
            let ra = list_t.rect_of(dom_t.resolve(a).unwrap());
            let rb = list_r.rect_of(dom_r.resolve(b).unwrap());
            assert_rect_approx(ra, rb);
        }
    }
}

#[test]
fn test_subgrid_baseline_002() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\subgrid-baseline-002.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\subgrid-baseline-002-ref.html",
        )
        .unwrap(),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let yt = list_t.rect_of(dom_t.resolve(dom_t.query_all("y")[0]).unwrap());
    let yr = list_r.rect_of(dom_r.resolve(dom_r.query_all("y")[0]).unwrap());
    assert_rect_approx(yt, yr);

    let st = list_t.rect_of(dom_t.resolve(dom_t.query_all(".subgrid")[0]).unwrap());
    let sr = list_r.rect_of(dom_r.resolve(dom_r.query_all(".subgrid")[0]).unwrap());
    assert_rect_approx(st, sr);

    // The text item in test is x[0], and in ref is x[0]
    let xt = list_t.rect_of(dom_t.resolve(dom_t.query_all("x")[0]).unwrap());
    let xr = list_r.rect_of(dom_r.resolve(dom_r.query_all("x")[0]).unwrap());
    assert_rect_approx(xt, xr);
}

