// node:perf_hooks — performance timeline.
// NOTA: `now`, `mark`, `measure`, `clearMarks`, `clearMeasures`,
// `getEntries[ByName|ByType]` and `timeOrigin` are NOT top-level named
// exports of `node:perf_hooks` in real Node — they live on the `performance`
// object (`performance.now()`, `performance.timeOrigin` as a data property,
// not a function). This file imported them as bare named imports, which
// Node itself refuses (no such export); verified against Node's documented
// `perf_hooks` exports (`performance`, `PerformanceObserver`, `constants`,
// `monitorEventLoopDelay`, `createHistogram`, ...). Fixed to go through
// `performance`, which is the real surface.
import { describe, test, expect } from "rts:test";
import { performance } from "node:perf_hooks";

performance.clearMarks();
performance.clearMeasures();

const t0 = performance.now();
const t1 = performance.now();
const nowMonotonic = t1 >= t0;

const origin = performance.timeOrigin;
const originOk = origin > 0;

const m = performance.mark("start");
const markNameOk = m.name === "start" && m.entryType === "mark";

performance.mark("end");
const meas = performance.measure("span", "start", "end");
const measOk = meas.name === "span" && meas.entryType === "measure" && meas.duration >= 0;

const simple = performance.measure("simple");
const simpleOk = simple.entryType === "measure" && simple.duration >= 0;

const byName = performance.getEntriesByName("start");
const byNameOk = byName.length === 1 && byName[0].name === "start";

const marks = performance.getEntriesByType("mark");
const marksOk = marks.length === 2;

const all = performance.getEntries();
const allOk = all.length >= 4; // 2 marks + 2 measures

performance.clearMarks();
const afterClearMarks = performance.getEntriesByType("mark").length === 0;
const measuresRemain = performance.getEntriesByType("measure").length >= 2;

performance.clearMeasures();
const afterClearAll = performance.getEntries().length === 0;

describe("node:perf_hooks", () => {
    test("now monotonic", () => expect(nowMonotonic).toBe(true));
    test("timeOrigin positive", () => expect(originOk).toBe(true));
    test("mark returns entry", () => expect(markNameOk).toBe(true));
    test("measure between marks", () => expect(measOk).toBe(true));
    test("measure no marks", () => expect(simpleOk).toBe(true));
    test("getEntriesByName", () => expect(byNameOk).toBe(true));
    test("getEntriesByType mark", () => expect(marksOk).toBe(true));
    test("getEntries all", () => expect(allOk).toBe(true));
    test("clearMarks", () => expect(afterClearMarks).toBe(true));
    test("clearMarks keeps measures", () => expect(measuresRemain).toBe(true));
    test("clearMeasures empties", () => expect(afterClearAll).toBe(true));
});
