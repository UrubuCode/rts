// Tests running subgrid WPT fixtures against references or expected positions.

#[test]
fn test_standalone_axis_size_006() {
    let html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\standalone-axis-size-006.html",
    )
    .unwrap();
    let dom = crate::parse_html_to_dom(&html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    list.walk(|item, x, y| {
        println!("item at ({}, {}): {:?}", x, y, item);
    });
    let test_s = crate::style::parse_inline("font-size: 0");
    println!("test_s.font_size: {:?}", test_s.font_size);
    let r_grid = list
        .rect_of(dom.resolve(dom.query_all(".grid")[0]).unwrap())
        .unwrap();
    let r_subgrid = list
        .rect_of(dom.resolve(dom.query_all(".subgrid")[0]).unwrap())
        .unwrap();
    println!("grid: {:?}", r_grid);
    println!("subgrid: {:?}", r_subgrid);
    assert_eq!(
        r_grid,
        crate::paint::Rect {
            x: 8.0,
            y: 50.0,
            w: 100.0,
            h: 100.0
        }
    );
    assert_eq!(
        r_subgrid,
        crate::paint::Rect {
            x: 8.0,
            y: 50.0,
            w: 100.0,
            h: 100.0
        }
    );
}

#[test]
fn test_line_names_010() {
    let html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-010.html",
    )
    .unwrap();
    let dom = crate::parse_html_to_dom(&html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let html_ref = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-010-ref.html",
    )
    .unwrap();
    let dom_ref = crate::parse_html_to_dom(&html_ref);
    let list_ref = crate::layout::layout_document(&dom_ref, &ctx);

    let xs_test = dom.query_all("x");
    let xs_ref = dom_ref.query_all("x");
    assert_eq!(xs_test.len(), 12);
    assert_eq!(xs_ref.len(), 12);
    for i in 0..12 {
        let n_t = dom.resolve(xs_test[i]).unwrap();
        let n_r = dom_ref.resolve(xs_ref[i]).unwrap();
        let r_t = list.rect_of(n_t);
        let r_r = list_ref.rect_of(n_r);
        assert_eq!(r_t, r_r, "Mismatch at item {i}");
    }
}

#[test]
fn test_orthogonal_writing_mode_001() {
    let html = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\orthogonal-writing-mode-001.html").unwrap();
    let dom = crate::parse_html_to_dom(&html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let html_ref = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\orthogonal-writing-mode-001-ref.html").unwrap();
    let dom_ref = crate::parse_html_to_dom(&html_ref);
    let list_ref = crate::layout::layout_document(&dom_ref, &ctx);

    for (i, elem) in dom.query_all(".grid, .subgrid").iter().enumerate() {
        let n_t = dom.resolve(*elem).unwrap();
        let r_t = list.rect_of(n_t);
        let s = dom.computed_style_idx(n_t);
        println!(
            "test grid[{i}]: {:?}, display={:?}, is_col_sub={:?}, is_row_sub={:?}, wm={:?}",
            r_t,
            s.as_ref().map(|c| c.display),
            s.as_ref().map(|c| matches!(
                c.grid_template_columns.as_deref(),
                Some(crate::style::GridTemplate::Subgrid(_))
            )),
            s.as_ref().map(|c| matches!(
                c.grid_template_rows.as_deref(),
                Some(crate::style::GridTemplate::Subgrid(_))
            )),
            s.as_ref().and_then(|c| c.writing_mode)
        );
    }
    for (i, elem) in dom_ref.query_all(".grid, .subgrid").iter().enumerate() {
        let n_r = dom_ref.resolve(*elem).unwrap();
        let r_r = list_ref.rect_of(n_r);
        println!("ref grid[{i}]: {:?}", r_r);
    }
    for (i, elem) in dom_ref.query_all("x").iter().enumerate() {
        let n_r = dom_ref.resolve(*elem).unwrap();
        let r_r = list_ref.rect_of(n_r);
        println!(
            "ref x[{i}]: {:?}, boxes: {:?}, display: {:?}",
            r_r,
            list_ref.tree.boxes_of(n_r),
            dom_ref.computed_style_idx(n_r).map(|c| c.display)
        );
    }
}

#[test]
fn test_independent_formatting_context() {
    let html = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\independent-formatting-context.html").unwrap();
    let dom = crate::parse_html_to_dom(&html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    for (i, elem) in dom.query_all(".grid").iter().enumerate() {
        let n_t = dom.resolve(*elem).unwrap();
        println!("test grid[{i}]: rect={:?}", list.rect_of(n_t));
    }
    let html_ref = std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\independent-formatting-context-ref.html").unwrap();
    let dom_ref = crate::parse_html_to_dom(&html_ref);
    let list_ref = crate::layout::layout_document(&dom_ref, &ctx);
    for (i, elem) in dom_ref.query_all(".grid").iter().enumerate() {
        let n_r = dom_ref.resolve(*elem).unwrap();
        println!("ref grid[{i}]: rect={:?}", list_ref.rect_of(n_r));
    }
    let b_t = dom.resolve(dom.query("body").unwrap()).unwrap();
    let b_r = dom_ref.resolve(dom_ref.query("body").unwrap()).unwrap();
    let box_t = list.tree.boxes_of(b_t)[0];
    let box_r = list_ref.tree.boxes_of(b_r)[0];
    println!(
        "test body kids: {:?}",
        list.tree
            .children(box_t)
            .iter()
            .map(|&cb| (cb, list.tree.node_of(cb)))
            .collect::<Vec<_>>()
    );
    println!("rect 81: {:?}", list.rect_of(81));
    println!("rect 83: {:?}", list.rect_of(83));
    println!("rect 84: {:?}", list.rect_of(84));
    for (i, elem) in dom.query_all(".first, .second").iter().enumerate() {
        let n_t = dom.resolve(*elem).unwrap();
        let r_t = list.rect_of(n_t);
        println!("item[{i}]: rect={:?}", r_t);
    }
}

#[test]
fn test_grid_gap_011() {
    let html = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-011.html",
    )
    .unwrap();
    let dom = crate::parse_html_to_dom(&html);
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let boxes = dom.query_all("div > div > div");
    assert_eq!(boxes.len(), 2);
    let r0 = list.rect_of(dom.resolve(boxes[0]).unwrap()).unwrap();
    let r1 = list.rect_of(dom.resolve(boxes[1]).unwrap()).unwrap();
    assert_eq!(
        r0,
        crate::paint::Rect {
            x: 183.0,
            y: 8.0,
            w: 100.0,
            h: 100.0
        }
    );
    assert_eq!(
        r1,
        crate::paint::Rect {
            x: 283.0,
            y: 8.0,
            w: 100.0,
            h: 100.0
        }
    );
}

#[test]
fn test_grid_gap_004() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let html_t = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-004.html",
    )
    .unwrap();
    let (dom_t, dom_r) = (
        crate::parse_html_to_dom(&html_t),
        crate::parse_html_to_dom(
            &std::fs::read_to_string(
                r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\grid-gap-004-ref.html",
            )
            .unwrap(),
        ),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let (g_t, g_r) = (
        dom_t.resolve(dom_t.query(".grid").unwrap()).unwrap(),
        dom_r.resolve(dom_r.query(".grid").unwrap()).unwrap(),
    );
    assert_eq!(list_t.rect_of(g_t), list_r.rect_of(g_r));
    assert_eq!(
        list_t.grid_column_tracks.get(&g_t),
        list_r.grid_column_tracks.get(&g_r)
    );
}

#[test]
fn test_placement_implicit_001() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\placement-implicit-001.html",
        )
        .unwrap(),
    );
    let list_t = crate::layout::layout_document(&dom_t, &ctx);
    let dom_r = crate::parse_html_to_dom(&std::fs::read_to_string(r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\placement-implicit-001-ref.html").unwrap());
    let list_r = crate::layout::layout_document(&dom_r, &ctx);

    for (i, (&gt, &gr)) in dom_t
        .query_all(".grid")
        .iter()
        .zip(&dom_r.query_all(".grid"))
        .enumerate()
    {
        assert_eq!(
            list_t.rect_of(dom_t.resolve(gt).unwrap()),
            list_r.rect_of(dom_r.resolve(gr).unwrap()),
            "grid {i}"
        );
    }
    for (i, (&st, &sr)) in dom_t
        .query_all(".subgrid")
        .iter()
        .zip(&dom_r.query_all(".subgrid"))
        .enumerate()
    {
        assert_eq!(
            list_t.rect_of(dom_t.resolve(st).unwrap()),
            list_r.rect_of(dom_r.resolve(sr).unwrap()),
            "subgrid {i}"
        );
    }
}

#[test]
fn test_line_names_006() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-006.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-006-ref.html",
        )
        .unwrap(),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let (xt, xr) = (dom_t.query_all("x"), dom_r.query_all("x"));
    assert_eq!(xt.len(), xr.len());
    for (i, (&a, &b)) in xt.iter().zip(&xr).enumerate() {
        assert_eq!(
            list_t.rect_of(dom_t.resolve(a).unwrap()),
            list_r.rect_of(dom_r.resolve(b).unwrap()),
            "x {i}"
        );
    }
}

#[test]
fn test_abs_pos_002() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\abs-pos-002.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\abs-pos-002-ref.html",
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
        println!("x {i}: test={ra:?} ref={rb:?}");
        assert_eq!(ra, rb, "x {i}");
    }
}

#[test]
fn test_item_percentage_height_001() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let html_t = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\item-percentage-height-001.html",
    )
    .unwrap();
    let html_r = std::fs::read_to_string(
        r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\item-percentage-height-001-ref.html",
    )
    .unwrap();
    let dom_t = crate::parse_html_to_dom(&html_t);
    let dom_r = crate::parse_html_to_dom(&html_r);
    let list_t = crate::layout::layout_document(&dom_t, &ctx);
    let list_r = crate::layout::layout_document(&dom_r, &ctx);

    for (i, &it) in dom_t.query_all(".item").iter().enumerate() {
        let n = dom_t.resolve(it).unwrap();
        println!("test item {i}: {:?}", list_t.rect_of(n));
    }
    for (i, &g) in dom_t.query_all(".grid, .subgrid").iter().enumerate() {
        let n = dom_t.resolve(g).unwrap();
        println!("test grid {i}: rect={:?}", list_t.rect_of(n));
    }
    for (i, &it) in dom_r.query_all(".item").iter().enumerate() {
        let n = dom_r.resolve(it).unwrap();
        println!("ref item {i}: {:?}", list_r.rect_of(n));
    }
    for (i, &g) in dom_r.query_all(".grid").iter().enumerate() {
        let n = dom_r.resolve(g).unwrap();
        println!("ref grid {i}: rect={:?}", list_r.rect_of(n));
    }
}

#[test]
fn test_line_names_008() {
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 1280.0,
        viewport_h: 800.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let dom_t = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-008.html",
        )
        .unwrap(),
    );
    let dom_r = crate::parse_html_to_dom(
        &std::fs::read_to_string(
            r"C:\Users\nexga\Documents\wpt-corpus\css\css-grid\subgrid\line-names-008-ref.html",
        )
        .unwrap(),
    );
    let (list_t, list_r) = (
        crate::layout::layout_document(&dom_t, &ctx),
        crate::layout::layout_document(&dom_r, &ctx),
    );
    let (xt, xr) = (dom_t.query_all("x"), dom_r.query_all("x"));
    assert_eq!(xt.len(), xr.len());
    for (i, (&a, &b)) in xt.iter().zip(&xr).enumerate() {
        let ra = list_t.rect_of(dom_t.resolve(a).unwrap());
        let rb = list_r.rect_of(dom_r.resolve(b).unwrap());
        println!("x {i}: test={ra:?} ref={rb:?}");
    }
}


