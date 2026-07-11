// node:perf_hooks — performance timeline.
import { describe, test, expect } from "rts:test";
import {
    now,
    timeOrigin,
    mark,
    measure,
    getEntries,
    getEntriesByName,
    getEntriesByType,
    clearMarks,
    clearMeasures,
} from "node:perf_hooks";

clearMarks();
clearMeasures();

const t0 = now();
const t1 = now();
const nowMonotonic = t1 >= t0;

const origin = timeOrigin();
const originOk = origin > 0;

const m = mark("start");
const markNameOk = m.name === "start" && m.entryType === "mark";

mark("end");
const meas = measure("span", "start", "end");
const measOk = meas.name === "span" && meas.entryType === "measure" && meas.duration >= 0;

const simple = measure("simple");
const simpleOk = simple.entryType === "measure" && simple.duration >= 0;

const byName = getEntriesByName("start");
const byNameOk = byName.length === 1 && byName[0].name === "start";

const marks = getEntriesByType("mark");
const marksOk = marks.length === 2;

const all = getEntries();
const allOk = all.length >= 4; // 2 marks + 2 measures

clearMarks();
const afterClearMarks = getEntriesByType("mark").length === 0;
const measuresRemain = getEntriesByType("measure").length >= 2;

clearMeasures();
const afterClearAll = getEntries().length === 0;

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
