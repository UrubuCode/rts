# CSS parity matrix — RTS versus Blink

Measurement date: 2026-08-27. Harness: `examples/claude-css-runner.ts`, expected values measured in Chrome at 1280x800, tolerance 1px.

## Baseline

The corpus contains 49 HTML fixtures. The initial RTS result was **36/49 fixtures passing**, with **38 deviations** in 13 fixtures. After the elliptical `border-radius`, resolved-grid-track, inline-baseline, and parent/child margin-collapse cuts, the current result is **41/49 fixtures passing**, with **23 deviations** in 8 fixtures. The `rts-dom` unit suite remains green at 713 passed, 0 failed and 1 ignored.

## Deviations grouped by mechanism

| Area | Fixtures | Deviations | Initial diagnosis |
|---|---:|---:|---|
| CSS computed-value serialization | — | 0 | Elliptical `border-radius` and explicit `grid-template-columns` used-track serialization now match the measured fixtures. |
| Block formatting / box model | box-model; margin-collapse | 0 | Parent/child margin escape, BFC barriers, and content-box height accounting now match the measured fixtures. |
| Float/clear | clear; float-clear | 6 | Float exclusion and clear placement/containing block calculations differ from Blink; this is the next layout priority. |
| Grid layout | grid-areas | 3 | Grid row/area placement remains incomplete; explicit column-track serialization now matches the measured fixture |
| Positioning | absolute; relative | 6 | Absolute stretch and relative offsets are not fully applied to used geometry |
| Inline formatting/text | text-align; vertical-align; white-space | 8 | Line box metrics, vertical alignment and preserved whitespace differ from Blink |

## Priority for implementation

1. Keep the computed/used boundary explicit. The elliptical `border-radius`, explicit grid-track, inline-baseline, and margin-collapse paths are implemented and covered by unit and corpus tests; the next priority is float/clear geometry.
2. Repair float exclusion and clear placement/containing-block used values. The parent box-model, BFC barriers, `display:none`, and inline dimension cases now match the corpus.
3. Continue with grid area sizing and relative/absolute geometry after float/clear semantics are stable.
4. Expand inline formatting and text metrics (`white-space`, `vertical-align`, `text-align`) after the box tree contracts are stable.

## Blink architecture rule applied

Blink separates rule matching, cascade/defaulting, computed value construction and layout used values. Therefore the RTS should not solve all differences by changing parser strings: computed serialization belongs in the style layer, while pixel positions/heights belong in the layout/used-value layer. Each fix must have one CSS fixture and one unit regression.
