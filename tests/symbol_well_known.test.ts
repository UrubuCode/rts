import { describe, test, expect } from "rts:test";

let out = "";

// (#216) Well-known symbols — handles cacheados.
const it1 = Symbol.iterator;
const it2 = Symbol.iterator;
out += (it1 === it2 ? "iterator-same" : "iterator-diff") + "\n";

const ai1 = Symbol.asyncIterator;
const ai2 = Symbol.asyncIterator;
out += (ai1 === ai2 ? "ai-same" : "ai-diff") + "\n";

const distinct: boolean = Symbol.iterator !== Symbol.hasInstance;
out += (distinct ? "distinct" : "merged") + "\n";

describe("symbol_well_known", () => {
  test("Symbol.iterator/asyncIterator/hasInstance well-known (#216)", () => expect(out).toBe(
    "iterator-same\nai-same\ndistinct\n"
  ));
});
