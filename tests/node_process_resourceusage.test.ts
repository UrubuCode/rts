// node:process — resourceUsage.
import { describe, test, expect } from "rts:test";
import { resourceUsage } from "node:process";
const r = resourceUsage();
const cpuOk = r.userCPUTime >= 0 && r.systemCPUTime >= 0;
const rssOk = r.maxRSS > 0;
const shape = typeof r.minorPageFault === "number" && typeof r.fsRead === "number";
describe("node:process resourceUsage", () => {
    test("CPU times", () => expect(cpuOk).toBe(true));
    test("maxRSS positive", () => expect(rssOk).toBe(true));
    test("full struct shape", () => expect(shape).toBe(true));
});
