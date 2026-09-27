//! What the inspector asks of a document that nothing else asks: the rules
//! that match one element, in the terms a person reads them in.
//!
//! The cascade already answers "which rules match, in which order"
//! (`Stylesheet::matched_for_node`); it answers it as indices, because the
//! cascade only needs to apply them. This turns the same answer into text —
//! selector, origin, specificity, declarations — without a second matcher: the
//! call below is the one `cascade.rs` makes, with the same `matches_complex`.

use super::*;

/// One rule that matches an element, as DevTools shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchedRuleView {
    /// The selector, rebuilt from the parsed form (`ComplexSelector::to_css_text`).
    pub selector: String,
    /// `(ids, classes, types)`.
    pub specificity: (u32, u32, u32),
    /// A rule of the user-agent sheet rather than the page's.
    pub user_agent: bool,
    /// `(name, value, important)`, in source order.
    pub declarations: Vec<(String, String, bool)>,
}

impl Dom {
    /// The rules matching `id`, in cascade order — weakest first, which is the
    /// order CDP's `matchedCSSRules` uses. Empty for a non-element or a stale id.
    pub fn matched_rules_view(&self, id: NodeId) -> Vec<MatchedRuleView> {
        let Some(idx) = self.resolve(id) else {
            return Vec::new();
        };
        let NodeKind::Element { tag } = &self.nodes[idx].kind else {
            return Vec::new();
        };
        if self.stylesheet.is_empty() {
            return Vec::new();
        }
        let classes: Vec<&str> = self.nodes[idx]
            .attr("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default();
        let id_attr = self.nodes[idx].attr("id");
        let matched = self.stylesheet.matched_for_node(
            &self.media_context(),
            tag,
            id_attr,
            &classes,
            |sel| self.matches_complex(idx, sel),
        );
        matched
            .rules
            .iter()
            .map(|&(_, _, _, _, i)| {
                let rule = &self.stylesheet.rules[i];
                MatchedRuleView {
                    selector: rule.selector.to_css_text(),
                    specificity: rule.selector.specificity_triple(),
                    user_agent: rule.is_ua,
                    declarations: rule
                        .source_declarations
                        .iter()
                        .map(|d| (d.name.clone(), d.value_css(), d.important))
                        .collect(),
                }
            })
            .collect()
    }

    /// The node under `(x, y)` in VIEWPORT coordinates — `DOM.getNodeForLocation`.
    ///
    /// The same list and measurer `bounding_component` reads (`layout_cached`
    /// with the thread's active measurer), so the node answered is the one
    /// painted there; the page scroll turns the viewport point into content.
    pub fn node_at(&self, x: f32, y: f32) -> Option<NodeId> {
        let (scroll_x, scroll_y) = self.page_scroll();
        let (vw, vh) = self.viewport.get();
        let idx = crate::layout::measure::active_measurer::with_active(|measurer| {
            let ctx = crate::layout::LayoutCtx { viewport_w: vw, viewport_h: vh, measurer };
            crate::layout::layout_cached(self, &ctx).hit_test(x + scroll_x, y + scroll_y)
        })?;
        Some(self.id_of_idx(idx))
    }

    /// The border box of `id` as `getBoundingClientRect` answers it:
    /// `[x, y, width, height]`. `None` for a node the layout gave no box.
    pub fn border_box(&self, id: NodeId) -> Option<[f32; 4]> {
        let idx = self.resolve(id)?;
        let (vw, vh) = self.viewport.get();
        crate::layout::measure::active_measurer::with_active(|measurer| {
            let ctx = crate::layout::LayoutCtx { viewport_w: vw, viewport_h: vh, measurer };
            let list = crate::layout::layout_cached(self, &ctx);
            let geometry = self.geometry_cached(&ctx);
            list.rect_of_in(&geometry, idx).map(|r| [r.x, r.y, r.w, r.h])
        })
    }
}
