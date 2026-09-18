import { describe, test, expect } from "rts:test";
import { serialize, deserialize } from "rts:serde";

// A symbol in the pickle (docs/engine/pickle.md §2): a registered one comes
// back as THE symbol of that key, a well-known one as itself, and an
// unregistered one as a new symbol of the same description whose identity
// holds inside the graph. As a property key, the same three rules. Private
// fields share the `@@` space with symbols and must stay private fields.
// Pre-computed at top level, as the other pickle tests are.

const reg = Symbol.for("claude.pickle.key");
const uniq = Symbol("mine");
const bare = Symbol();

// ── values ───────────────────────────────────────────────────────────────
const values: any = deserialize(serialize({ reg, iter: Symbol.iterator, uniq, again: uniq, bare, list: [uniq] }));
const regSame = values.reg === reg;
const iterSame = values.iter === Symbol.iterator;
const uniqIsSymbol = typeof values.uniq === "symbol";
const uniqIsNew = values.uniq !== uniq;
const uniqDescription = values.uniq.description;
const uniqIdentityHeld = values.uniq === values.again && values.uniq === values.list[0];
const bareDescription = values.bare.description;
const keyForRevived = Symbol.keyFor(values.reg);
const keyForUniq = Symbol.keyFor(values.uniq);

// ── keys ─────────────────────────────────────────────────────────────────
const keyed: any = { plain: 1 };
keyed[reg] = "registered";
keyed[Symbol.toStringTag] = "Keyed";
keyed[uniq] = "unique";
keyed.value = uniq; // the same symbol as key and as value, in one graph
const keys: any = deserialize(serialize(keyed));
const ownSymbols = Object.getOwnPropertySymbols(keys);
const regByKey = keys[reg];
const tagByKey = keys[Symbol.toStringTag];
const tagShows = Object.prototype.toString.call(keys);
const uniqRevivedKey = ownSymbols.find((s: symbol) => s.description === "mine");
const uniqByRevivedKey = uniqRevivedKey === undefined ? undefined : keys[uniqRevivedKey];
const keyIsValue = uniqRevivedKey === keys.value;
const plainKeys = Object.keys(keys);

// ── private fields beside a symbol key ───────────────────────────────────
class Box {
  #secret = 7;
  [uniq] = "on the instance";
  get secret(): number {
    return this.#secret;
  }
}
const box: any = deserialize(serialize(new Box()));
const boxSecret = box.secret;
const boxSymbols = Object.getOwnPropertySymbols(box).length;

// ── a Map keyed by a symbol, a Set of symbols ────────────────────────────
const m = new Map<any, string>([[reg, "r"], [uniq, "u"]]);
const collections: any = deserialize(serialize({ m, s: new Set([uniq, uniq, reg]) }));
const mapReg = collections.m.get(reg);
const mapUniqEntries = [...collections.m.keys()].filter((k: any) => typeof k === "symbol" && k.description === "mine").length;
const setSize = collections.s.size;

describe("rts:serde symbols", () => {
  test("a registered symbol is the same symbol, a well-known one is itself", () => {
    expect(regSame).toBe(true);
    expect(iterSame).toBe(true);
    expect(keyForRevived).toBe("claude.pickle.key");
  });

  test("an unregistered symbol is a new symbol with the same description, one per graph", () => {
    expect(uniqIsSymbol).toBe(true);
    expect(uniqIsNew).toBe(true);
    expect(uniqDescription).toBe("mine");
    expect(uniqIdentityHeld).toBe(true);
    expect(bareDescription).toBe(undefined);
    expect(keyForUniq).toBe(undefined);
  });

  test("a symbol-keyed property round-trips under the symbol, not under text", () => {
    expect(regByKey).toBe("registered");
    expect(tagByKey).toBe("Keyed");
    expect(tagShows).toBe("[object Keyed]");
    expect(ownSymbols.length).toBe(3);
    expect(uniqByRevivedKey).toBe("unique");
    expect(keyIsValue).toBe(true);
    expect(plainKeys.length).toBe(2);
  });

  test("a private field stays private beside a symbol key", () => {
    expect(boxSecret).toBe(7);
    expect(boxSymbols).toBe(1);
    expect(box instanceof Box).toBe(true);
  });

  test("Map keys and Set members that are symbols", () => {
    expect(mapReg).toBe("r");
    expect(mapUniqEntries).toBe(1);
    expect(setSize).toBe(2);
  });
});
