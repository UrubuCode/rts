# Blink style calculation notes

Reference: Chromium, `third_party/blink/renderer/core/css/style-calculation.md`, consulted 2026-08-27.

Blink describes style calculation in three phases: gathering/partitioning/indexing style rules; visiting each element and finding all rules that match; and combining matching rules plus other context into the final computed style.

The central path is `Element::StyleForLayoutObject` -> `StyleResolver::ResolveStyle`. Rule matching is indexed through `RuleSet` and `RuleData` maps by selector keys such as class names, so irrelevant selectors are avoided. Matching is performed through `ElementRuleCollector`, `SelectorCheckingContext`, `SelectorChecker`, and recursive `MatchSelector`/`CheckOne` operations.

The cascade considers user-agent, user and author origins. Author matching includes host, slotted, element-scope and pseudo-part rules. Blink keeps explicit resolution context (`ElementResolveContext`, `StyleResolverState`, `ElementRuleCollector`) during style calculation instead of reducing the input immediately to an unordered property map.

Implications for RTS: preserve the ordered specified declarations and rule provenance; keep indexed candidate selection separate from selector matching; keep cascade priority separate from property parsing; and resolve the final computed style only after all matching rules, inheritance context, pseudo state, animations and other conditions are known. The current RTS already has an AST, a rule index and stateful lowering, but needs a more explicit per-element resolve context and property-level cascade records to approach this model.

The current `style_resolver.cc` includes dedicated components for cascade layers (`CascadeLayerMap`, `CascadeLayered`), value conversion (`StyleBuilderConverter`), custom properties (`CSSVariableData`), animation integration (`CSSAnimations`, `ElementAnimations`) and a dedicated `StyleCascade`/`StyleResolverState`. This confirms that Blink treats layer precedence, custom-property resolution, conversion to computed values and animation overlays as separate concerns rather than one property-map merge.

The next RTS cut should therefore add observable provenance/cascade records at the per-element boundary and avoid folding all author rules into a single `DeclBlock` before layer/origin/importance decisions are complete. The existing RTS rule index remains a compatible optimisation for the first matching phase; it should feed a richer cascade record rather than replace it.

CSS Cascade Level 5 findings (W3C Editor's Draft, consulted 2026-08-27): the cascade/defaulting process takes declarations as input and outputs a specified value for each property on each element. Value processing is staged as declared, cascaded, specified, computed, used and actual values.

An unlayered author declaration takes precedence over layered author declarations in the normal cascade, even when the layered selector has higher specificity and appears later. For important declarations, layer precedence is reversed. At-rules such as `@keyframes` defined inside layers also participate in layer ordering.

`unset` acts as `inherit` for inherited properties and `initial` for non-inherited properties. `revert` rolls back to the previous cascade origin; for author origin that means ignoring author-origin rules and returning to the user/UA result. `revert-layer` rolls back declarations from the current cascade layer and can fall back to an earlier layer or origin. These are not equivalent to clearing a single field in a final style map.

Implications for RTS: the current implementation has normal/important layers and `initial`/`unset`, but `revert` and `revert-layer` require retaining per-declaration provenance or applying cascade in origin/layer passes. A property-level cascade record is the next necessary abstraction before implementing those keywords correctly.

Blink `CSSComputedStyleDeclaration` finding: `getPropertyValue` first obtains a property-specific CSS value from the computed style mapping. `getPropertyCSSValue` updates the style/layout tree, obtains the node's `ComputedStyle`, and passes the `LayoutObject` when needed to `ComputedStyleCSSValueMapping::get`; the final CSS text is the mapped value's `cssText()`. This is why a computed-style probe may expose resolved geometry-dependent values (for example grid track sizes) while the style object still retains computed declarations separately.
