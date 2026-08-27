# CSS parity matrix — RTS versus Blink

Measurement date: 2026-08-27. Harness: `examples/claude-css-runner.ts`, expected values measured in Chrome at 1280x800, tolerance 1px.

## Baseline

The corpus contains 49 HTML fixtures. The initial RTS result was **36/49 fixtures passing**, with **38 deviations** in 13 fixtures. After the elliptical `border-radius`, resolved-grid-track, and inline-baseline cuts, the current result is **39/49 fixtures passing**, with **27 deviations** in 10 fixtures. The `rts-dom` unit suite remains green at 709 passed, 0 failed and 5 ignored.

## Deviations grouped by mechanism

| Area | Fixtures | Deviations | Initial diagnosis |
|---|---:|---:|---|
| CSS computed-value serialization | — | 0 | Elliptical `border-radius` and explicit `grid-template-columns` used-track serialization now match the measured fixtures. |
| Block formatting / box model | box-model; margin-collapse | 4 | Layout used values diverge; includes parent margins and height accounting |
| Float/clear | clear; float-clear | 6 | Float exclusion and clear placement/containing block calculations differ from Blink |
| Grid layout | grid-areas | 3 | Grid row/area placement remains incomplete; explicit column-track serialization now matches the measured fixture |
| Positioning | absolute; relative | 6 | Absolute stretch and relative offsets are not fully applied to used geometry |
| Inline formatting/text | text-align; vertical-align; white-space | 8 | Line box metrics, vertical alignment and preserved whitespace differ from Blink |

## Priority for implementation

1. Keep the computed/used boundary explicit. The elliptical `border-radius`, explicit grid-track, and inline-baseline paths are implemented and covered by unit and corpus tests; the next priority is block formatting geometry.
2. Fix parent box heights and margin accounting in the block formatting contract. The `display:none` and inline dimension cases now match the corpus; the remaining block gaps are isolated in box-model and margin-collapse fixtures.
3. Repair float/clear and margin-collapse used values, then position relative/absolute geometry.
4. Expand inline formatting and text metrics (`white-space`, `vertical-align`, `text-align`) after the box tree contracts are stable.

## Blink architecture rule applied

Blink separates rule matching, cascade/defaulting, computed value construction and layout used values. Therefore the RTS should not solve all differences by changing parser strings: computed serialization belongs in the style layer, while pixel positions/heights belong in the layout/used-value layer. Each fix must have one CSS fixture and one unit regression.
