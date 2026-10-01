// A regex literal is a RegExp wherever it is evaluated — #2836.
//
// The target stack says a construction is in progress, never that THIS
// allocation is that construction, so a literal evaluated anywhere inside a
// `new Thing()` was linked to `Thing.prototype`: `[object RegExp]` with `test`,
// `exec` and `source` all `undefined`. That is what `dayjs("2026-10-01")` died
// of, four frames below the `new`, and it needed no bundle and no module — a
// three-line program reproduces it.
//
// `RegExp.prototype.test.call(r, …)` answered correctly the whole time, which is
// why this pins the PROTOTYPE LINK and not just the method: the object was
// always a real RegExp, it simply could not be reached as one.
import { describe, test, expect } from "rts:test";

const seen: any[] = [];

function Legacy(this: any) {
    const r: any = /Z$/i;
    seen.push({ where: "function constructor", proto: Object.getPrototypeOf(r) === RegExp.prototype, test: typeof r.test, source: r.source, works: r.test("aZ") });
}
new (Legacy as any)();

function free(): any {
    const r: any = /Z$/i;
    return { proto: Object.getPrototypeOf(r) === RegExp.prototype, test: typeof r.test, source: r.source };
}

class Modern {
    constructor() { seen.push(Object.assign({ where: "class, via a prototype method" }, this.go())); }
    go(): any { return free(); }
}
new Modern();

// The dayjs shape exactly: a free function reached from a constructor, using the
// literal in the second operand of an `&&` inside an `if`.
function parseDate(date: string): string {
    if (typeof date === "string" && !/Z$/i.test(date)) { return "no-Z"; }
    return "other";
}
class Dayjs {
    public answer: string;
    constructor(date: string) { this.answer = parseDate(date); }
}

const outside: any = /Z$/i;
class Fancy extends RegExp {}

describe("a regex literal inside a constructor (#2836)", () => {
    test("function constructor: reaches RegExp.prototype", () => expect(seen[0].proto).toBe(true));
    test("function constructor: test is a function", () => expect(seen[0].test).toBe("function"));
    test("function constructor: source is the pattern", () => expect(seen[0].source).toBe("Z$"));
    test("function constructor: and it matches", () => expect(seen[0].works).toBe(true));
    test("class through a method: reaches RegExp.prototype", () => expect(seen[1].proto).toBe(true));
    test("class through a method: source is the pattern", () => expect(seen[1].source).toBe("Z$"));
    test("the dayjs shape answers", () => expect(new Dayjs("2026-10-01").answer).toBe("no-Z"));
    test("outside a constructor, unchanged", () => expect(typeof outside.test).toBe("function"));
    test("a real subclass still reaches its own prototype", () =>
        expect(new Fancy("a") instanceof Fancy).toBe(true));
    test("and a subclass instance is still a RegExp", () =>
        expect(new Fancy("a").test("a")).toBe(true));
});
