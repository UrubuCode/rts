//! A logical property (`padding-block-start`, `margin-inline-start`, …) maps
//! to a physical side through the writing mode and direction of the ELEMENT
//! that carries it, whichever rule, layer or origin declares them (CSS
//! Logical Properties §4). Each case compares the logical declaration against
//! a sibling that writes the physical side directly, so the assertion is
//! "same box as the physical spelling" rather than a number.
//!
//! Found by WPT `grid-item-content-baseline-001` (container 16): `.pbs`
//! precedes `.hl` in the sheet, and the padding was mapped with the
//! parent's `vertical-lr` because `.hl` had not been applied yet.

use super::*;

fn padding_of(dom: &Dom, sel: &str) -> crate::style::Edges {
    dom.computed_style(dom.query(sel).unwrap()).unwrap().padding
}

fn margin_of(dom: &Dom, sel: &str) -> crate::style::Edges {
    dom.computed_style(dom.query(sel).unwrap()).unwrap().margin
}

#[test]
fn writing_mode_from_a_later_rule_decides_the_block_side() {
    let dom = parse_html_to_dom(
        "<style>.v{writing-mode:vertical-lr} .pbs{padding-block-start:15px} \
         .hl{writing-mode:horizontal-tb} .ref{padding-top:15px}</style>\
         <div class='v'><span id='a' class='pbs hl'></span><span id='r' class='ref'></span></div>",
    );
    assert_eq!(padding_of(&dom, "#a"), padding_of(&dom, "#r"));
}

#[test]
fn important_writing_mode_beats_a_more_specific_normal_one() {
    let dom = parse_html_to_dom(
        "<style>.v{writing-mode:vertical-lr} .pbs{padding-block-start:15px} \
         .hl{writing-mode:horizontal-tb !important} div .x{writing-mode:vertical-lr} \
         .ref{padding-top:15px}</style>\
         <div class='v'><span id='a' class='x pbs hl'></span><span id='r' class='ref'></span></div>",
    );
    assert_eq!(padding_of(&dom, "#a"), padding_of(&dom, "#r"));
}

#[test]
fn inline_writing_mode_decides_an_author_logical_property() {
    let dom = parse_html_to_dom(
        "<style>.v{writing-mode:vertical-lr} .pbs{padding-block-start:15px} \
         .ref{padding-top:15px}</style>\
         <div class='v'><span id='a' class='pbs' style='writing-mode:horizontal-tb'></span>\
         <span id='r' class='ref'></span></div>",
    );
    assert_eq!(padding_of(&dom, "#a"), padding_of(&dom, "#r"));
}

#[test]
fn direction_from_a_later_rule_decides_the_inline_side() {
    let dom = parse_html_to_dom(
        "<style>.mis{margin-inline-start:7px} .r{direction:rtl} \
         .ref{margin-right:7px}</style>\
         <div><span id='a' class='mis r'></span><span id='r' class='ref'></span></div>",
    );
    assert_eq!(margin_of(&dom, "#a"), margin_of(&dom, "#r"));
}

#[test]
fn logical_and_physical_still_compete_by_rule_position() {
    // The fix moves WHICH axis a logical name maps to, never WHEN it is
    // applied: a later `padding-top` still overrides an earlier
    // `padding-block-start`, and the other way round.
    let dom = parse_html_to_dom(
        "<style>.v{writing-mode:vertical-lr} .pbs{padding-block-start:15px} \
         .pt{padding-top:3px} .hl{writing-mode:horizontal-tb} .pbs2{padding-block-start:9px} \
         .ref3{padding-top:3px} .ref9{padding-top:9px}</style>\
         <div class='v'><span id='a' class='pbs pt hl'></span><span id='b' class='pt hl pbs2'></span>\
         <span id='r3' class='ref3'></span><span id='r9' class='ref9'></span></div>",
    );
    assert_eq!(padding_of(&dom, "#a"), padding_of(&dom, "#r3"));
    assert_eq!(padding_of(&dom, "#b"), padding_of(&dom, "#r9"));
}
