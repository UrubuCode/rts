//! Thread-local line box stack for tracking baselines of inline and block flows.
//!
//! Extracted from `line_baseline.rs` to keep both files strictly below the 500-line ceiling.

use crate::Dom;
use crate::dom::NodeIdx;
use crate::style::ComputedStyle;

/// One recorded line: the flow's owner, the baseline of its last line box in
/// the coordinates of the list it was laid into, and whether a cached
/// FRAGMENT re-announced it (its subtree's answer) or a flow pushed it live.
#[derive(Clone, Copy)]
pub(super) struct LineRecord {
    pub(super) owner: NodeIdx,
    pub(super) baseline: f32,
    pub(super) from_fragment: bool,
}

thread_local! {
    /// The last line box of every inline flow laid out, in layout order —
    /// pushed by `layout_inline_flow` and the inline-block run
    /// ([`register_last_line`]) and by every emitted fragment
    /// ([`register_from_fragment`]), read by whoever [`collect_baselines`]s.
    static LINES: std::cell::RefCell<Vec<LineRecord>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Records the baseline of the last line of the inline flow owned by `owner`.
pub(in crate::layout) fn register_last_line(owner: NodeIdx, baseline: f32) {
    LINES.with(|v| {
        v.borrow_mut().push(LineRecord {
            owner,
            baseline,
            from_fragment: false,
        })
    });
}

/// Re-announces a cached fragment's last own line where it is emitted.
pub(in crate::layout) fn register_from_fragment(owner: NodeIdx, baseline: f32) {
    LINES.with(|v| {
        v.borrow_mut().push(LineRecord {
            owner,
            baseline,
            from_fragment: true,
        })
    });
}

/// Where the stack stands now — what a reader passes back to [`collect_baselines`].
pub(in crate::layout) fn mark() -> usize {
    LINES.with(|v| v.borrow().len())
}

/// Drops what was recorded since `marca` unread.
pub(in crate::layout) fn discard(marca: usize) {
    LINES.with(|v| v.borrow_mut().truncate(marca));
}

/// Empties the stack. Called where a document layout starts.
pub(in crate::layout) fn clear() {
    LINES.with(|v| v.borrow_mut().clear());
}

/// The LOWEST own line box of `id` among what was recorded since `marca`, as
/// `(direct, total)`.
pub(in crate::layout) fn collect_baselines(
    dom: &Dom,
    id: NodeIdx,
    marca: usize,
) -> (Option<f32>, Option<f32>) {
    LINES.with(|v| {
        let mut v = v.borrow_mut();
        let max_of = |acc: Option<f32>, b: f32| Some(acc.map_or(b, |m: f32| m.max(b)));
        let (mut direct, mut total) = (None, None);
        for l in v[marca.min(v.len())..]
            .iter()
            .filter(|l| own_flow(dom, id, l.owner, l.from_fragment))
        {
            total = max_of(total, l.baseline);
            if !l.from_fragment {
                direct = max_of(direct, l.baseline);
            }
        }
        v.truncate(marca);
        (direct, total)
    })
}

/// `overflow` other than `visible` on either axis.
pub(in crate::layout) fn clips_overflow(css: &ComputedStyle) -> bool {
    [css.overflow_x, css.overflow_y]
        .iter()
        .flatten()
        .any(|o| o.clips())
}

/// Is the line recorded by `owner` one of the atom `id`'s OWN line boxes?
pub(in crate::layout) fn own_flow(dom: &Dom, id: NodeIdx, owner: NodeIdx, from_fragment: bool) -> bool {
    let mut cur = owner;
    while cur != id {
        let Some(css) = dom.computed_style_idx(cur) else {
            return false;
        };
        use crate::style::DisplayKind as D;
        let atomic = matches!(
            css.effective_display(),
            Some(
                D::InlineBlock
                    | D::InlineFlex
                    | D::InlineFlexWrap
                    | D::InlineGrid
                    | D::InlineTable
                    | D::Flex
                    | D::FlexWrap
                    | D::Grid
                    | D::Table
            )
        );
        let outside = css
            .float_side
            .is_some_and(|f| f != crate::style::FloatSide::None)
            || css.position.is_some_and(|p| p.out_of_flow());
        if atomic || outside || (clips_overflow(&css) && !(cur == owner && from_fragment)) {
            return false;
        }
        match dom.node(cur).parent {
            Some(p) => cur = p,
            None => return false,
        }
    }
    true
}
