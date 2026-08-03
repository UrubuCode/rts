# RTS HTML+CSS render engine — study and plan

Folder with **the entire study** of RTS's own HTML+CSS render engine, on top of
`rts-egui`. Long-term goal: our "DOOM" — a render engine from scratch,
mastering every layer (on the way to a browser engine).

> **Status (2026-06-23):** DECIDED. The lightweight HTML engine already retained
> on main (retained DOM tree + data-driven block allocator in TS + mutation by
> NodeId, in `rts-egui`) is the **official direction**, evolved IN-PLACE in
> phases. The 5-tree `rts-html` crate **will not be created**. The living
> operational plan is **[rts-html-roadmap.md](rts-html-roadmap.md)** (F0-F5). The
> old 5-tree plan was **demoted to a frozen north-star** ([rts-html-north-star.md](rts-html-north-star.md)),
> a conceptual reference that does not dictate phases. Decision made after
> multi-agent analysis (4 approaches × 3 adversarial lenses + completeness
> critique).

## How to read

1. **[rts-html-roadmap.md](rts-html-roadmap.md)** — **the living operational
   document. START HERE.** Strategy, the 10 resolved decision points, the 6
   hard invariants, the F0-F5 roadmap (pixel-early, verifiable kill-gates), and
   the first concrete slice of ≤1 day.
2. **[rts-html-north-star.md](rts-html-north-star.md)** — the old 5-tree
   `PLANO.md` (DOM→Style→Layout→DisplayList→Paint), FROZEN as a theoretical
   ceiling. Conceptual reference; does NOT dictate phases. It only "wakes up" if
   the F4 ceiling criterion proves that egui is not enough beyond the rich
   paragraph.
3. **[arquitetura.md](arquitetura.md)** — the detailed architectural synthesis of
   the canonical pipeline (foundation of the north-star), with the Rust structs
   of each phase.
4. **[critica-adversarial.md](critica-adversarial.md)** — the skeptical review
   that cut the scope down to the realistic (flexbox/grid/position/modern-CSS5
   out) and pointed out the real risks (text layout, hit-testing, "5 trees
   without a pixel"). Its corrections fed both the north-star and the roadmap.
5. **[analises/](analises/)** — the 4 base research documents that support
   everything:
   - `analise-browser-pipeline.md` — how real engines (Servo/Blink/robinson)
     structure the pipeline; why the DOM is a tree, not a list.
   - `analise-css-subset.md` — pragmatic CSS subset by priority (phase 1/2/3
     table); what never gets in.
   - `analise-egui-as-paint.md` — egui as an absolute PAINT backend (Painter,
     galley, text measurement, ScrollArea), not as layout.
   - `analise-rts-constraints.md` — fit within RTS: doctrine (Rust=infra,
     TS=high level), TS engine limits, decision on the new `rts-html` crate.

## Key decisions (summary — strategy DECIDED, see roadmap)

> The decisions below are the CURRENT ones (roadmap). The old decisions (new
> `rts-html` crate, universal absolute paint) became the frozen north-star — the
> study in `analise-*` that grounded them remains valid as a conceptual base.

- **Evolve the lightweight engine IN-PLACE in `rts-egui`** — do not create
  `rts-html`. The retained tree DOM + attributes + O(1) indices + mutation by
  NodeId already exist and are reused; only the internal functions of
  `frame.rs::render_*` evolve.
- **egui is the layout engine BY DEFAULT** (the 4 displays); absolute paint
  (`allocate_painter` + `LayoutJob`/`galley.rows`) enters as a surgical
  exception in a single `render_*` (F4), only where egui provably does not
  compose (rich inline paragraph with a link). The north-star's "universal
  absolute paint" rule was cut.
- **CSS arrives early via an opaque numeric slot** (`defineStyle`/`setStyle`):
  Rust never matches on a CSS string; TS maps name→index. `BlockDef` (display)
  gains a `ComputedStyle` per NodeId with cascade in Rust.
- **Events via polling** (`pollEvent(h) → NodeId`), no reactive listener
  (blocked by #195/mutable capture). Programmatic DOM mutation (which the
  north-star did not even anticipate) remains.
- **Honest scope:** "rich text + styled boxes + click" usable in ~6.5-9.5
  weeks (F0-F3); rich inline paragraph + links in +2-3 (F4). Flexbox, grid,
  absolute positioning, animations, modern CSS5, font-family — **out** (cuts
  inherited from the north-star).
- **Pixel in the first week (P1):** thin vertical end-to-end path before
  building the full 5 trees.
