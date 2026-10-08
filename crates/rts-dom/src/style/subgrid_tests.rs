use crate::style::{GridTemplate, GridTrack, SubgridLine, Dimension};
use crate::style::fmt_values::fmt_grid_template;

#[test]
fn test_parse_grid_template_none() {
    assert_eq!(GridTemplate::parse("none"), Some(GridTemplate::None));
    assert_eq!(fmt_grid_template(Some(&GridTemplate::None)), "none");
}

#[test]
fn test_parse_grid_template_tracks_with_names() {
    let t = GridTemplate::parse("[first] 100px [main] 200px [last]").unwrap();
    if let GridTemplate::Tracks(tl) = &t {
        assert_eq!(tl.entries.len(), 2);
        assert_eq!(tl.entries[0].0, vec!["first"]);
        assert_eq!(tl.entries[0].1, GridTrack::Fixed(Dimension::Px(100.0)));
        assert_eq!(tl.entries[1].0, vec!["main"]);
        assert_eq!(tl.entries[1].1, GridTrack::Fixed(Dimension::Px(200.0)));
        assert_eq!(tl.trailing, vec!["last"]);
    } else {
        panic!("expected Tracks");
    }
    assert_eq!(fmt_grid_template(Some(&t)), "[first] 100px [main] 200px [last]");
}

#[test]
fn test_parse_grid_template_subgrid() {
    let s = GridTemplate::parse("subgrid").unwrap();
    assert_eq!(s, GridTemplate::Subgrid(Vec::new()));
    assert_eq!(fmt_grid_template(Some(&s)), "subgrid");

    let s2 = GridTemplate::parse("subgrid [a] [b c]").unwrap();
    assert_eq!(
        s2,
        GridTemplate::Subgrid(vec![
            SubgridLine::Line(vec!["a".to_string()]),
            SubgridLine::Line(vec!["b".to_string(), "c".to_string()]),
        ])
    );
    assert_eq!(fmt_grid_template(Some(&s2)), "subgrid [a] [b c]");
}

#[test]
fn test_parse_grid_template_subgrid_repeat() {
    let s = GridTemplate::parse("subgrid repeat(auto-fill, [x])").unwrap();
    assert_eq!(
        s,
        GridTemplate::Subgrid(vec![
            SubgridLine::AutoFill(vec![vec!["x".to_string()]])
        ])
    );
    assert_eq!(fmt_grid_template(Some(&s)), "subgrid repeat(auto-fill, [x])");
}

#[test]
fn test_invalid_subgrid_idents() {
    assert!(GridTemplate::parse("subgrid [none]").is_none());
    assert!(GridTemplate::parse("subgrid [subgrid]").is_none());
    assert!(GridTemplate::parse("subgrid [auto]").is_none());
    assert!(GridTemplate::parse("subgrid [inherit]").is_none());
    assert!(GridTemplate::parse("subgrid [1a]").is_none());
}

#[test]
fn test_auto_repeat_line_merging() {
    let t = GridTemplate::parse("[x] repeat(2, [a] 50px [b]) [z]").unwrap();
    if let GridTemplate::Tracks(tl) = t {
        let (tracks, _collapsible, lines) = crate::layout::grid::tracks::expand_auto_repeats(&tl, 200.0, 10.0);
        assert_eq!(tracks.len(), 2);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], vec!["x", "a"]);
        assert_eq!(lines[1], vec!["b", "a"]);
        assert_eq!(lines[2], vec!["b", "z"]);
    } else {
        panic!("expected Tracks");
    }
}

#[test]
fn test_wpt_grid_placement_002() {
    let html = r#"<!DOCTYPE html>
<html>
<head>
<style>
.grid {
  display: grid;
  width: 100px;
  height: 100px;
  background: red;
  grid-template-columns: [area-start] repeat(auto-fill, 10px) [area-end];
  grid-template-rows: [area-start] repeat(auto-fill, 10px [area-start]) [area-end];
}
.grid > div {
  grid-area: area;
  background: green;
}
</style>
</head>
<body>
<div class="grid">
  <div id="target"></div>
</div>
</body>
</html>"#;
    let (dom, list) = crate::table::tests::geometria(html, 800.0);
    let r = crate::table::tests::rect(&dom, &list, "#target", 0);
    eprintln!("TARGET RECT: {:?}", r);
    assert_eq!(r.w, 100.0);
    assert_eq!(r.h, 100.0);
}

#[test]
fn test_computed_subgrid_on_block() {
    let html = r#"<!DOCTYPE html>
<html>
<head>
<style>
#target {
  display: block;
  grid-template-columns: subgrid [a] [b];
}
</style>
</head>
<body>
  <div id="target"></div>
</body>
</html>"#;
    let (dom, _list) = crate::table::tests::geometria(html, 800.0);
    let target = dom.query("#target").unwrap();
    let val = dom.computed_property(target, "grid-template-columns");
    assert_eq!(val, "subgrid [a] [b]");
}

#[test]
fn test_computed_subgrid_invalid_on_grid_without_grid_parent() {
    let html = r#"<!DOCTYPE html>
<html>
<head>
<style>
#parent {
  display: block;
}
#target {
  display: grid;
  grid-template-columns: subgrid [a] [b];
}
</style>
</head>
<body>
  <div id="parent">
    <div id="target"></div>
  </div>
</body>
</html>"#;
    let (dom, _list) = crate::table::tests::geometria(html, 800.0);
    let target = dom.query("#target").unwrap();
    let val = dom.computed_property(target, "grid-template-columns");
    assert_eq!(val, "none");
}



