//! F5 of the plan: the real faces on this machine reproduce the two tables
//! Blink was measured into — `rts-dom`'s `layout/measure/font_metrics.rs`
//! (its test `BLINK`, copied here row for row because it is private to that
//! crate) and a sample of the generated `font_advances.rs`. When the two
//! disagree, the table is what Edge measured and the loader is wrong.
//!
//! A machine without the Windows fonts (CI on Linux) skips with the reason
//! printed; it does not pass by asserting nothing silently.

use rts_text::{FontStore, Style};

const BLINK: [(&str, [(f32, f32, f32, f32); 7]); 4] = [
    ("serif", [(10.0, 9.0, 2.0, 11.0), (12.0, 11.0, 3.0, 15.0), (14.0, 12.0, 3.0, 16.0), (16.0, 14.0, 3.0, 18.0), (20.0, 18.0, 4.0, 23.0), (24.0, 21.0, 5.0, 27.0), (32.0, 29.0, 7.0, 37.0)]),
    ("sans-serif", [(10.0, 9.0, 2.0, 11.0), (12.0, 11.0, 3.0, 14.0), (14.0, 13.0, 3.0, 16.0), (16.0, 14.0, 3.0, 18.0), (20.0, 18.0, 4.0, 23.0), (24.0, 22.0, 5.0, 28.0), (32.0, 29.0, 7.0, 37.0)]),
    ("monospace", [(10.0, 9.0, 3.0, 12.0), (12.0, 11.0, 3.0, 14.0), (14.0, 13.0, 4.0, 17.0), (16.0, 15.0, 4.0, 19.0), (20.0, 18.0, 5.0, 23.0), (24.0, 22.0, 6.0, 28.0), (32.0, 29.0, 8.0, 37.0)]),
    ("system-ui", [(10.0, 11.0, 3.0, 14.0), (12.0, 13.0, 3.0, 16.0), (14.0, 15.0, 4.0, 19.0), (16.0, 17.0, 4.0, 21.0), (20.0, 22.0, 5.0, 27.0), (24.0, 26.0, 6.0, 32.0), (32.0, 35.0, 8.0, 43.0)]),
];

/// `(char, regular, bold)` in units of a 2048 em, from `font_advances.rs`.
const ADVANCES: [(&str, [(char, u16, u16); 8]); 4] = [
    ("serif", [('A', 1479, 1479), ('V', 1479, 1479), ('a', 909, 1024), ('g', 1024, 1024), ('W', 1933, 2048), ('0', 1024, 1024), (' ', 512, 512), ('é', 909, 909)]),
    ("sans-serif", [('A', 1366, 1479), ('V', 1366, 1366), ('a', 1139, 1139), ('g', 1139, 1251), ('W', 1933, 1933), ('0', 1139, 1139), (' ', 569, 569), ('é', 1139, 1139)]),
    ("monospace", [('A', 1126, 1126), ('V', 1126, 1126), ('a', 1126, 1126), ('g', 1126, 1126), ('W', 1126, 1126), ('0', 1126, 1126), (' ', 1126, 1126), ('é', 1126, 1126)]),
    ("system-ui", [('A', 1321, 1440), ('V', 1272, 1366), ('a', 1042, 1102), ('g', 1206, 1268), ('W', 1913, 2058), ('0', 1104, 1178), (' ', 561, 565), ('é', 1071, 1108)]),
];

/// The store, or `None` with the reason printed when this machine lacks the
/// four Windows faces.
fn windows_store() -> Option<FontStore> {
    let store = FontStore::new();
    for generic in ["serif", "sans-serif", "monospace", "system-ui"] {
        if store.resolve(generic, 400, Style::Normal).is_none() {
            eprintln!("SKIPPED: no face for `{generic}` on this machine (the Windows fonts are not in the repo)");
            return None;
        }
    }
    Some(store)
}

#[test]
fn every_row_of_blinks_metric_table_is_reproduced_from_the_font_files() {
    let Some(store) = windows_store() else { return };
    let mut rows = 0;
    for (family, table) in BLINK {
        let face = store.resolve(family, 400, Style::Normal).unwrap();
        for (size, ascent, descent, line) in table {
            let got = (face.ascent_px(size), face.descent_px(size), face.normal_line_height(size));
            assert_eq!(got, (ascent, descent, line), "{family} ({}) {size}px", face.family());
            rows += 1;
        }
    }
    assert_eq!(rows, 28);
}

#[test]
fn generic_families_resolve_to_blinks_windows_defaults() {
    let Some(store) = windows_store() else { return };
    for (generic, family) in [("serif", "Times New Roman"), ("sans-serif", "Arial"), ("monospace", "Consolas"), ("system-ui", "Segoe UI")] {
        assert_eq!(store.resolve(generic, 400, Style::Normal).unwrap().family(), family);
    }
    let bold_italic = store.resolve("serif", 700, Style::Italic).unwrap();
    assert!(bold_italic.weight() >= 700 && bold_italic.italic());
    // An unknown name is skipped, not guessed; a list of only unknowns is None.
    assert_eq!(store.resolve("NoSuchFont, monospace", 400, Style::Normal).unwrap().family(), "Consolas");
    assert!(store.resolve("NoSuchFont", 400, Style::Normal).is_none());
}

#[test]
fn the_hmtx_advances_match_the_table_blink_measured_regular_and_bold() {
    let Some(store) = windows_store() else { return };
    for (family, sample) in ADVANCES {
        for (bold, weight) in [(false, 400), (true, 700)] {
            let face = store.resolve(family, weight, Style::Normal).unwrap();
            for (c, regular, heavy) in sample {
                let want = if bold { heavy } else { regular };
                let units = face.nominal_advance(c).unwrap();
                let got = u32::from(units) * 2048 / u32::from(face.units_per_em());
                assert_eq!(got, u32::from(want), "{family} bold={bold} {c:?}");
            }
        }
    }
}

/// Also true of a family found only through the `name` table scan.
#[test]
fn a_family_outside_the_four_defaults_is_found_by_its_name_table() {
    let Some(store) = windows_store() else { return };
    match store.resolve("Georgia", 400, Style::Normal) {
        Some(face) => assert_eq!(face.family(), "Georgia"),
        None => eprintln!("SKIPPED: Georgia is not installed"),
    }
}
