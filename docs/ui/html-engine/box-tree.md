# The box tree

The layer between the DOM and the paint list, which this engine does not have
and every other CSS engine does. Gecko calls it the frame tree, WebKit the
render tree, Blink the layout object tree, Servo the box tree. The names differ
and the reason does not: **CSS produces boxes that no element owns, and elements
that own several boxes**, and neither can be expressed while layout runs
directly over the DOM.

This document is the decision and its shape. `crates/rts-dom/PLAN.md` carries
the lots that build it, which is where a plan belongs (rule 2 of
`docs/README.md`: a plan goes stale the moment work starts, and the person who
fixes it is the person editing the crate).

---

## 1. What is missing, measured

Everything in this section was measured on 2026-09-16, not inferred.

**The central case does not fail — it is absent.** A `<div>` inside a `<span>`
is not laid out wrongly; it is **ignored as a box**. `layout/runs.rs` walks the
children of an inline container and tests, in order, for non-rendered metadata,
`display:none`, form widgets, `<br>`, replaced elements and inline-blocks. It
never tests for block level. The `<div>` matches none of them, falls into the
generic inline arm, and the loop descends into *its* children as though the
`<div>` were a transparent `<span>`. Its `width`, `height`, `background`,
`padding` and `margin` are discarded. CSS 2.1 §9.2.1.1 asks for the inline to be
split into three boxes, two of them anonymous. **148 reftests in `CSS2` fail on
that family**, and there is nowhere to put them.

**Generated content is written three times.** `::before` and `::after` have no
`NodeIdx` — a deliberate decision recorded in `pseudo/mod.rs`, taken so the
arena would not gain and lose a node on every re-cascade. The cost is that
`Dom::pseudo_box` returns a `PseudoBox` that each consumer then re-integrates by
hand: `layout/pseudo_bloco.rs` as a block with its own margin-collapse
machinery, `layout/flex_pseudo.rs` as a counterfeit `FlexItem`, `layout/runs.rs`
as an inline run, `layout/clearfix.rs` for the `clear` effect only. All three of
the first record the same cuts — no `border-radius`, no `flex-basis`, text does
not wrap — because each would have to implement them again.

**Five distinct CSS values collapse into one `DisplayKind` at parse time**, and
where the distinction matters it survives in a boolean beside the enum:
`flow_root` beside `Block`, `table_column` and `table_column_group` beside
`None`. `table` and `inline-table` collapse with no flag at all, and so do
`grid` and `inline-grid` — while `flex` and `inline-flex` keep separate
variants. `table-header-group`, `table-row-group` and `table-footer-group` are
one value, so the render order the table spec asks for cannot be expressed.

**Anonymous table rows and cells exist; the anonymous table does not.**
`table/grid.rs` closes loose cells into a row and wraps a stray child as a cell.
But a `<div style="display:table-cell">` with no table above it becomes an
ordinary block: `layout/bloco.rs` only reaches `layout_table` when the node
itself is `DisplayKind::Table`. CSS 2.1 §17.2.1 asks for the table to be
generated around it.

**One element with several boxes already happens, and is flattened on the
spot.** An inline that wraps across lines contributes one rectangle per line,
and `union_rect` grows a single stored rectangle to their envelope. That is the
right answer for `getBoundingClientRect`, and the wrong shape for layout: the
boundary between fragments is lost at the moment it is computed.

---

## 2. The decision: two trees

**A persistent box tree as input, an immutable fragment tree as output.** This
is the Blink-after-LayoutNG and Servo shape, and it is chosen over the two
alternatives for reasons that are specific to this engine.

**Not one tree that layout mutates.** That is the Gecko and old-WebKit shape,
and it is smaller. It was rejected because layout stops being a function:
`measure_block` and `layout_block_reusing` already cache on a key of
`(node, constraints, epochs)` and already, in effect, hold two box instances of
the same node distinguished only by constraints. Writing results back into the
input makes that cache something to invalidate by hand rather than something
that is correct by construction.

**Not the existing paint list as output.** It is tempting — `DisplayList` is
already a tree, with `children: Vec<ChildRef>` holding subtrees by reference for
incremental reuse, so half the shape exists. It was rejected because it fuses
layout with painting. The paint list answers *what pixels*, the fragment tree
answers *what geometry*, and a fragment has to exist for a box that paints
nothing. Chrome separated exactly these two and the cost of undoing that shows
up later, not now.

---

## 3. What a box is

Three kinds, and three because the evidence above asks for three:

| kind | owns a `NodeIdx` | where it comes from |
|---|---|---|
| element box | yes | an element that generates a box |
| anonymous box | no | the CSS rules that require one: block-in-inline, table fixups, text in a block container |
| generated box | no, but names its originator | `::before`, `::after`, `::marker` |

**Storage is a contiguous arena indexed by `BoxId`**, the same shape the DOM
already uses for nodes. Not `Rc<RefCell<…>>`: the tree is built in one pass,
read many times, and dropped whole.

**A box does not STORE a style — it stores the node whose style is its own.**
An element box names its element; an anonymous box names the element whose box
the CSS rules split to produce it, which is exactly the rule that an anonymous
box has no declarations of its own and takes the inherited properties of its
generator.

**This was not the first design, and the reason it changed is worth keeping.**
The box used to hold an `Rc<ComputedStyle>` captured when the tree was built —
cheap, shared, and wrong. The tree is memoised by `(revision, style_epoch)` and
deliberately NOT by `anim_epoch`, because rebuilding it every frame of every
transition would undo the reason the style memo separates those two. But the
style accessor DOES include `anim_epoch`. A captured style is therefore one
frame behind for the whole of an animation, and every reader would be wrong
with nothing to say so — the silent class this repository exists to refuse.

It was found by an agent migrating the layout, who reached for the captured
style, saw the mismatch, and stopped. Asking the document each time costs a memo
hit and an `Rc` clone. It is also smaller: no `Rc` per box.
**A text box carries the resolved inline properties, not a style pointer.**
`computed_style_idx` returns `None` for a text node — it matches only
`NodeKind::Element` — and `collect_runs` already threads colour, weight, italic,
decoration, text transform and white-space down as parameters. The box tree
keeps that: a text box is built with those values already resolved, which is
also what makes it shareable across the fragments of one run.

---

## 4. How it is built

One downward pass over the DOM, producing boxes as it goes. The pass is where
every rule that is today spread across consumers gets to live exactly once:

- **blockification** (CSS 2.1 §9.7), today in `effective_display`, consulted
  from 16 files at 30 call sites with one known bypass;
- **the two-value `display` form**, today rewritten inside the parser;
- **anonymous block boxes** around text and around block-in-inline;
- **anonymous table boxes**, the whole fixup chain rather than the two thirds
  that exist;
- **generated boxes** for `::before` and `::after`, once instead of three times.

The pass reads the DOM and the style and writes only boxes. It does not measure,
does not position and does not paint.

---

## 5. Invalidation

The engine has four independent signals and the box tree must listen to the same
ones rather than invent a fifth:

| signal | scope | what it means |
|---|---|---|
| `style_epoch` | global, thread-local | per-tag styles changed |
| `revision` | per document | structure changed (`touch`, `touch_subtree`, `touch_structural`, `touch_attr`) |
| `anim_epoch` | per document | an animation frame, deliberately without touching structure |
| `layout_epochs[node]` | per node | this subtree changed |

The scoping already implemented is worth preserving rather than rebuilding: a
structural change with no position-sensitive selector invalidates only the moved
subtree; with `:nth-child` or a sibling combinator it invalidates the parent's
subtree; with `:has()` it falls back to global. A box tree that listened only to
`revision` would throw that away.

**The cache key changes shape.** `LayoutMeasureKey` and `FragmentKey` are keyed
on a single `NodeIdx` plus the constraints. With boxes, the key is a `BoxId`
plus the constraints — which is strictly simpler, because the constraints are
today doing part of the job of distinguishing box instances of one node.

---

## 6. What does not change

The bridge to TypeScript promises that a `NodeId` yields exactly four numbers,
through `boundingRect` and `boundingRectAll`. `rts-egui` asks for the whole
paint list through `layout_cached` and hit-tests through it. The public surface
is `layout_document`, `layout_cached` and `bounding_rect`; everything else in
`crate::layout` is crate-private, which is what makes this migration possible at
all.

**Internally there may be N boxes per element; at that boundary they aggregate**,
exactly as `union_rect` aggregates the line fragments of an inline today.

---

## 7. The invariants that break silently

Surveyed on 2026-09-16 by reading the layout, the fragment cache, the stacking
code and both consumers. **Seven of these compile and lie. Two stop the
compiler, and those are the cheap ones.** Ordered by how invisible the failure
is.

**I1 — `NodeIdx` *is* the box identity, and there is no other.** Geometry
(`node_rects`), hit order (`hit_order`), scroll state (`ScrollRegion.node_idx`),
clips, grid tracks and all three cache keys are indexed by node. `record_node_rect`
inserts, so two boxes of one node silently keep the last. The decision is
written down in `pseudo/mod.rs`, which rejects Blink's approach with a reason
that was true then: *"faz sentido lá, onde a árvore de layout é uma estrutura
separada da árvore de nós; aqui o layout é indexado por `NodeIdx`"*. It stops
being true the moment this layer exists. **I1 is the root of I4, I7 and I9.**

**I2 — a box's parent is the node's parent.** `relativo.rs` and
`transformacao.rs` find what to translate by walking `dom.node(id).children`,
because — in the file's own words — *"a única forma de saber quais entradas são
desta subárvore é andar o DOM a partir de `id`"*. With anonymous boxes they
still find rectangles, just not all of them: half an element moves.

**I3 — the child sequence of a container is the DOM's child list.** The
incremental stitch validates a cached fragment by comparing its children against
`dom.node(id).children` filtered to elements. With anonymous wrappers that
comparison never matches, which merely disables stitching — a performance loss.
**The danger is fixing it wrongly:** mapping box back to node to make it match
again would stop it detecting changes in box structure that leave node structure
alone, and the stitch would then repaint last frame's layout, internally
consistent and wrong.

**I4 — one node, one rectangle; several fragments collapse to their union.**
`union_rect` is deliberate and correct for `getBoundingClientRect`. It is also
why hit-testing a link that wraps across two lines hits the gap at the end of
the first line. This is the one invariant whose breaking *improves* the answer.

**I5 — paint order is a position in a flat vector.** Subtree references point at
an index, and "is this child mine" is answered by a counter captured at creation
time. That arithmetic has already cost three real defects, all recorded in the
comments. A box tree is the natural place to do stacking by traversal — and
while both models coexist, any box emitting out of creation order misaligns the
indices with no way for the list to notice.

**I6 — the style read mid-layout is the box's style.** About 175 sites ask the
DOM for the computed style of the node they are laying out. An anonymous box has
no node style: the spec says it inherits the inherited properties and resets the
rest. Every one of these sites would answer with the parent element's style, and
none of them has a `None` branch for "this box has no node".

**I7 — invalidation rises through DOM ancestors, and that is enough.** It is
already documented as not being enough: the fragment key carries the imposed
constraints because *"o `node_epoch` sozinho não vê essa mudança — ela vem do
IRMÃO, não do próprio nó"*, and the comment calls it the silent class by name.
Anonymous boxes multiply exactly that, and they have no node whose epoch could
rise. **This is the piece that cannot be deferred**, because the failure is
serving last frame's drawing.

**I8 and I9 — 85 signatures say `(dom, NodeIdx)`, and the fragment types carry a
node.** Both are compile errors. The compiler enumerates every site. This is the
cheap part, and the metric "85 functions" overstates the work while understating
the risk.

**The consequence for the order of work.** The first move is not the mirror by
itself: it is **replacing the geometry and cache key with a `BoxId`**, with a
single module owning a `NodeIdx → Vec<BoxId>` map. That makes I1, I4, I7 and
I9 fall together, and turns I2 and I3 from silent lies into compile errors.

---

## 8. Phases

Each phase has a ruler that must pass before the next begins.

**Phase 1 — the identity swap.** A `BoxId` becomes the key of geometry, hit
order, scroll state and all three caches, with one module owning the
`NodeIdx → Vec<BoxId>` map. The tree is built as an exact 1:1 copy of the
DOM — one element box per element, no anonymous boxes, no generated boxes — so
every entry has length one and behaviour is identical by
construction. I1, I4, I7 and I9 fall here; I2 and I3 become compile errors.

> Ruler: **zero lost and zero gained**, per file, on the corpus and on the five
> WPT folders. A gain here is not good news — it means something changed that
> was not meant to.

**Phase 2 — fragments.** Layout stops writing into a shared display list and
starts returning fragments. The cache moves from node keys to box keys.

> Ruler: zero lost. Gains are possible here and must be explained one by one.

**Phase 3 — anonymous boxes, one family per lot.** Block-in-inline first,
because it is 148 reftests and the case the layer exists for. Then the table
fixups, then generated content moving from three implementations to one.

> Ruler: per family, per file, against a kept binary — the process this
> repository already runs.

---

## 9. Out of scope

This layer does not paint glyphs, and painting glyphs is the single largest
blocker measured on this corpus (issue #2729: around 1 369 of the passing
reftests draw nothing on either side, and around 541 of the failures have one
side drawing and the other not). It does not implement subgrid. It does not fix
the two font metric constants that were calibrated separately, nor the
containing block that does not know which axis it is on — both are smaller,
both are independent, and both are worth doing first.

---

## 10. The base, as it stands — the contract to build on

Written 2026-09-16, after the three holes found while building it were closed.
Anything below is what a lot may ASSUME; anything not below is not there yet.
The tests that pin each line are in `crates/rts-dom/src/boxes/tests.rs`.

### What the tree holds

One box per ELEMENT the cascade accepts, one per TEXT node, and — where CSS 2.1
§9.2.1.1 applies — **several boxes for the split inline plus one anonymous block
box per inline run**. No box for a comment, none for `display: none`, none for a
run of collapsible whitespace the split declined to wrap, and no table fixups or
generated content yet — those are BT-4 and BT-5.

**The shape of the split, because it is the one place the tree is not a mirror.**
For `<p><span>a<div>b</div>c</span></p>` the `<p>` box has three children: an
anonymous block enclosing a FRAGMENT of the span (an element box, which is what
carries the span's own border and background), the `<div>` as their SIBLING, and
a second anonymous block with the second fragment. The anonymous boxes rise to
the CONTAINER — a sibling of boxes nested inside the inline would not be a
sibling — and they inherit from the container, which is the enclosing
non-anonymous box. `boxes/build.rs` quotes the rule and draws the tree.

**Only an IN-FLOW block splits.** §9.2.1.1 says "an in-flow block-level box",
and a float or an absolutely positioned box is block-level (it is blockified)
but out of flow. Asking only the outer display split
`<span>a<div style="float:left"></div>b</span>` in three and put `b` on a line
of its own. Such a child now stays in the inline run: a float becomes an
ANCHOR there (`AtomicKind::Float`), and `layout/float_na_linha.rs` places it at
the top of the line it appears in when it fits — CSS 2.1 §9.5.1 — which is
also what happens to a float that is a DIRECT child in the middle of text,
since the block flow stopped closing the inline group on it. What an
absolutely positioned child still lacks is its static position after the
line box: there is no line box kept to ask (the IFC lot).

**Two consequences a reader must not assume away.** `boxes_of(node)` may return
MORE THAN ONE box, so `.first()` is one fragment of several; and an anonymous
box's children are not its style source's children — they are one RUN of them.

### The three rules, and the hole each one closed

**A `BoxId` names a build, not just a slot.** It carries the generation of the
tree that issued it, and every accessor refuses an id from another build with a
message naming both. The hole: the tree is memoised by `(revision,
style_epoch)`, so a style-only change yields a NEW tree at the SAME revision —
a generation taken from the revision would have let a stale id pass the check
and read the wrong arena. `Dom::next_box_generation` counts BUILDS for that
reason. Never store a `BoxId` across a rebuild; if a cache must survive one,
key it by node and translate on the way in.

**A box stores the SOURCE of its style, never a copy.** `style_source(id)`
answers the node whose computed style applies — the element itself, or, for an
anonymous or text box, the element it inherits from — and `style(dom, id)` asks
the DOM fresh. The hole: a copy taken at build time goes stale the moment a
style epoch bumps without a DOM revision, which is precisely the case the memo
key exists to catch. **Any derived value follows the same rule**: compute it on
demand, do not cache it on the box.

**A box knows what it is by the two-value display model.**
`formatting_context(dom, id)` answers `outer` (block-level or inline-level to
its siblings), `inner` (flow, flex, grid or table for its children) and
`independent` (does it contain its own floats and margins). The hole:
`layout::caixa::is_block_level` looks like the outer-display question and is
not — it answers "does this element go through `layout_block`", so an
`inline-block` or `inline-flex` answers `true` there while being inline-LEVEL.
**Do not use `is_block_level` to mean outer display.** The two coexist, they
mean different things, and `boxes/context.rs` carries the divergence in full.

### The one question that needs the tree

`runs_inline_formatting_context(dom, id)` — whether a flow container lays its
children out as lines or as a stack. CSS decides it by looking at the CHILDREN,
and after the block-in-inline split a box's children are not its node's
children. Asking the DOM answers about a shape that no longer exists. Any lot
that wants to know "is this an inline formatting context" asks the tree.

### What the base does NOT do yet, and must not be assumed

- **Layout takes the child ORDER from the tree, and this line replaces one that
  said the opposite.** The block flow walks `tree.children(box)`: a text box
  arrives with its box, and an anonymous box is ENTERED rather than skipped.
  What still comes from the DOM is a node that generates NO box — today only a
  comment — spliced back at its DOM position, because letting it fall through
  closes an inline-block run and breaks margin collapsing between the two
  blocks around it. That is a refusal with a measured reason, not an omission.
- **The mirror-equivalence assert is gone, and what replaced it is weaker in
  one dimension.** Once the order comes from the tree there are no longer two
  sequences to compare, so the old assertion is not expressible. In its place:
  a plain `assert_eq!` on the GENERATION, a `debug_assert` that every visited
  box is a child of the one descended through, and one that no box naming a
  node was dropped. None of them compares the tree against the DOM child by
  child. Said plainly rather than dressed up as equivalent.
- **An anonymous box IS laid out as the block box it is**, and this line
  replaces one that said it was expanded into its children instead.
  `layout/bloco_caixa.rs` is the block path that accepts a box with no node: it
  takes the container's content box and stacks the run in it, which is all an
  anonymous box needs — no width to resolve, no margin, no border, no
  background, no `float`, no `clear`, no generated content. What it does NOT do
  is named in its own header: no fragment cache of its own and no stacking
  context. It DOES get a geometry entry since `b867facb5`:
  `record_box_rect` stores its rect by `BoxId`, and `DisplayList::rect_of_box`
  finds it even inside a reused fragment. The DOM-facing geometry still only
  answers for nodes, because an anonymous box has no `NodeIdx` to be asked by.
- **The inline flow walks the tree, and this line replaces one that said it
  did so only inside the split.** `runs::collect_runs` takes the box every
  inline-group member got from the flow sequence and walks `tree.children`
  instead of the DOM's. Inside the split that is what stops it descending into
  the `<div>` that split the inline. Everywhere else it is what hands an ATOM —
  an `inline-flex`, an `inline-block`, a widget — its exact box. Walking the DOM
  there gave the atom `None`, and `layout_block` then had no box for the flex
  container's `expect` (11 `flexbox-baseline-*` reftests panicked) nor for
  reserving the atom's hit order before its children (a link inside an
  inline-block lost the click to the inline-block). Outside the split the two
  walks visit the same nodes: a comment has no box and was already ignored.
- **Fragments are keyed by a box ADDRESS that survives a rebuild, and this
  line replaces one that said they were keyed by `NodeIdx`.** Since
  `053ed4f67` the key target is `BoxCacheTarget { node, ordinal }`: the node
  that generates the box and its position among that node's boxes
  (`dom/chaves_cache.rs`). A `BoxId` is still never stored across a rebuild.
  A fragment taken from the cache has its `BoxId`s remapped into the current
  tree on the way in (`Fragment::remapped_to`), and the hit is refused when
  the node's box count changed rather than guessing which box it was. The
  incremental seam compares a box's children in the OLD tree against the NEW
  one, box against box. It never translates boxes back to nodes, which is the
  "named care" of BT-1. Two things follow and must not be assumed away. A
  global `touch()` bumps no epoch, so it must clear the measure and
  intrinsic-width caches: removing that clear served stale widths after a new
  `<style>`. And recycling a node forgets only that node's `last_fragment`
  entries, not the whole map.
- **A measurement names its box; there is no fallback from a node.**
  `measure_block` takes a `BoxId`, not an `Option`, and the "sole box of this
  node" bridge that stood in for callers who only knew the node is gone. It
  was wrong in both directions the split creates: a node with several boxes
  panicked by design, and a node with NONE (a `<span>` that only wrapped a
  block, whose boxes rose to the container) ran `layout_block` with no box
  over a document that has a tree, and the fragment cache's fast path
  panicked on the first child (`css-flexbox/percentage-heights-023`). A
  caller that starts from a node walks `tree.children` to the box instead —
  `coluna_shrink::altura_conteudo_sem_height` does, entering anonymous boxes
  rather than skipping them. The measure cache key is therefore a
  `BoxCacheTarget` and nothing else.
- **The DOM rect of a split inline includes the blocks that split it; the
  hit-test rect does not.** `DisplayList::rect_of` (what `boundingRect` and
  `boundingRectAll` read) unions the node's own boxes with
  `BoxTree::blocks_splitting(node)` — the in-flow blocks the split moved out of
  it — because Blink's client rects of a split inline include them (four
  `claude-bloco-*` fixtures, Edge 153). `Geometry::rects` keeps the boxes
  alone: it is also the hit-test table, and there the second fragment, later
  in hit order, would steal every click on the block. `layout/rect_cliente.rs`
  carries the reason; the paint of the inline is untouched.
- **No formatting context is IMPLEMENTED here.** `inner` says which algorithm
  applies; running it is still `layout`'s.
- **Whitespace is not decided here.** Which whitespace survives is a question
  about `white-space` and about the neighbours in a line, and `quebra.rs` owns
  it. Deciding it twice is the second-truth failure this module exists to avoid.
