//! Shaping (kerning), UAX #14 opportunities, grapheme clusters, the shaping
//! cache, and Ahem's coverage.

use rts_text::{FontStore, Style, cache_stats, grapheme_boundaries, line_break_opportunities, rasterise, shape, shaped_width};

const AHEM: &str = "C:/Users/nexga/Documents/wpt-corpus/fonts/Ahem.ttf";

#[test]
fn kerning_makes_avatar_narrower_than_the_sum_of_its_advances_in_times() {
    let store = FontStore::new();
    let Some(face) = store.resolve("serif", 400, Style::Normal) else {
        eprintln!("SKIPPED: no Times New Roman on this machine");
        return;
    };
    let text = "AVATAR Toy To.";
    let kerned = shaped_width(&face, text, 16.0, true);
    let plain = shaped_width(&face, text, 16.0, false);
    let sum: f32 = text.chars().map(|c| f32::from(face.nominal_advance(c).unwrap())).sum::<f32>() * face.units_to_px(16.0);
    eprintln!("AVATAR Toy To. @16px Times: kerned {kerned:.4}px, kern off {plain:.4}px, sum of hmtx {sum:.4}px");
    assert_eq!(plain, sum, "with kerning off, shaping is the sum of advances");
    // Edge 153 (headless, 2026-09-26), `<span style="font:16px serif">`:
    // getBoundingClientRect().width = 112.21875 kerned, 122.21875 with
    // `font-kerning: none` — 10px of kerning, not the 14 the plan quoted.
    // Blink snaps widths to 1/64 px (LayoutUnit), hence the tolerance.
    assert!((kerned - 112.21875).abs() < 1.0 / 64.0, "kerned {kerned} vs Edge 112.21875");
    assert!((plain - 122.21875).abs() < 1.0 / 64.0, "unkerned {plain} vs Edge 122.21875");
}

#[test]
fn opportunities_follow_uax14_and_a_no_break_space_holds() {
    // After the space (2) and after the hyphen (4); never around U+00A0.
    assert_eq!(line_break_opportunities("a b-c\u{a0}d"), vec![2, 4]);
    // A newline is a mandatory break and is reported like the others.
    assert_eq!(line_break_opportunities("a\nb"), vec![2]);
}

#[test]
fn a_grapheme_cluster_is_never_split() {
    // e + combining acute: one cluster of three bytes.
    assert_eq!(grapheme_boundaries("e\u{301}x"), vec![0, 3, 4]);
    // Family: man ZWJ woman ZWJ girl — one cluster.
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    assert_eq!(grapheme_boundaries(family), vec![0, family.len()]);
    // No opportunity falls inside it either.
    assert!(line_break_opportunities(family).is_empty());
}

#[test]
fn a_repeated_run_is_served_from_the_cache() {
    let store = FontStore::new();
    let Some(face) = store.resolve("sans-serif", 400, Style::Normal) else {
        eprintln!("SKIPPED: no Arial on this machine");
        return;
    };
    let text = "cache probe \u{2014} unique to this test";
    let first = shape(&face, text, 13.0, true);
    let before = cache_stats();
    let second = shape(&face, text, 13.0, true);
    let after = cache_stats();
    assert_eq!(first, second);
    assert!(after.hits > before.hits, "{before:?} -> {after:?}");
    // Clusters are byte indices into the text, in order for LTR.
    assert!(first.windows(2).all(|w| w[0].cluster <= w[1].cluster));
}

#[test]
fn an_ahem_x_at_20px_is_a_filled_20_by_20_square_with_ascent_16() {
    let Ok(bytes) = std::fs::read(AHEM) else {
        eprintln!("SKIPPED: {AHEM} is absent (the WPT checkout's fonts/)");
        return;
    };
    let store = FontStore::with_system_dir(None);
    assert_eq!(store.register(bytes, "Ahem"), 1);
    let face = store.resolve("Ahem", 400, Style::Normal).unwrap();
    assert_eq!(face.ascent_px(20.0), 16.0);
    assert_eq!(face.descent_px(20.0), 4.0);
    let glyphs = shape(&face, "X", 20.0, true);
    assert_eq!(glyphs.len(), 1);
    assert_eq!(glyphs[0].x_advance, 20.0);
    let bmp = rasterise(&face, glyphs[0].id, 20.0).unwrap();
    assert_eq!((bmp.w, bmp.h, bmp.left, bmp.top), (20, 20, 0, 16));
    assert!(bmp.alpha.iter().all(|&a| a == 255), "every pixel fully covered");
    // A space has no outline: an empty bitmap, not None.
    let space = shape(&face, " ", 20.0, true)[0].id;
    assert_eq!(rasterise(&face, space, 20.0).unwrap().w, 0);
}
