//! `grid-area`, `grid-row` and `grid-column` all expand, by the one rule of
//! CSS Grid §8.4, into the four longhands placement reads. `grid-area` with
//! line numbers used to be dropped whole: only its single-name form was read,
//! so `grid-area: 2 / 2 / 3 / 3` auto-placed the item.

use super::*;
use crate::style::grid_lines::GridLine;

fn lines(s: &ComputedStyle) -> [Option<GridLine>; 4] {
    [
        s.grid_row_start.clone(),
        s.grid_column_start.clone(),
        s.grid_row_end.clone(),
        s.grid_column_end.clone(),
    ]
}

fn named(n: &str) -> Option<GridLine> {
    Some(GridLine::Named(n.into(), None))
}

#[test]
fn grid_area_with_four_line_numbers_sets_all_four_longhands() {
    // The form `css-grid/abspos/grid-abspos-staticpos-align-self-safe-001` writes.
    let s = parse_inline("grid-area: 2 / 2 / 3 / 3");
    use GridLine::Line;
    assert_eq!(lines(&s), [Some(Line(2)), Some(Line(2)), Some(Line(3)), Some(Line(3))]);
    assert_eq!(s.grid_area, None, "a value with `/` is not an area name");
}

#[test]
fn grid_area_single_name_copies_itself_into_every_edge() {
    let s = parse_inline("grid-area: header");
    assert_eq!(lines(&s), [named("header"), named("header"), named("header"), named("header")]);
    assert_eq!(s.grid_area.as_deref(), Some("header"));
}

#[test]
fn a_missing_edge_after_a_number_is_auto_not_a_copy() {
    let s = parse_inline("grid-area: 1 / span 2");
    assert_eq!(
        lines(&s),
        [Some(GridLine::Line(1)), Some(GridLine::Span(2)), Some(GridLine::Auto), Some(GridLine::Auto)]
    );
}

#[test]
fn two_value_shorthand_keeps_a_named_start_and_its_explicit_end() {
    let s = parse_inline("grid-row: a / b");
    assert_eq!((s.grid_row_start, s.grid_row_end), (named("a"), named("b")));
    // With one value only, a named start is ALSO the end (§8.4).
    let one = parse_inline("grid-column: a");
    assert_eq!((one.grid_column_start, one.grid_column_end), (named("a"), named("a")));
}

#[test]
fn named_lines_with_counts_negatives_and_named_spans_parse() {
    let s = parse_inline("grid-area: 2 a / -1 / span b / span 3 c");
    assert_eq!(
        lines(&s),
        [
            Some(GridLine::Named("a".into(), Some(2))),
            Some(GridLine::Line(-1)),
            Some(GridLine::SpanNamed("b".into(), 1)),
            Some(GridLine::SpanNamed("c".into(), 3)),
        ]
    );
    assert_eq!(s.get_property("grid-row-start"), "2 a");
    assert_eq!(s.get_property("grid-column-end"), "span 3 c");
}

#[test]
fn an_invalid_part_rejects_the_whole_declaration() {
    // Five parts, a zero line, and `span` alone: none may half-apply.
    for v in ["1 / 2 / 3 / 4 / 5", "0 / 1", "span / 2"] {
        let s = parse_inline(&format!("grid-area: {v}"));
        assert_eq!(lines(&s), [None, None, None, None], "{v}");
    }
}
