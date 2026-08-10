import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `new.target` itself is not yet emitted by this engine (a separate, unrelated
// gap: `Emit(Unsupported { construct: "new.target" })`), so this observes the
// SAME fact `new.target` would answer, through the mechanism that actually
// carries it: which prototype the constructed object inherits from. That is
// exactly what `context.new_targets` decides in `allocate_for_target`, and
// exactly what a spreading `super()` would corrupt if it were routed through
// `ConstructWithArgs` (which PUSHES a target of its own) instead of
// `SuperConstructWithArgs` (which must not).
class Base {
    a: number;
    constructor(...args: number[]) {
        this.a = args.reduce((x, y) => x + y, 0);
    }
}

class Mid extends Base {
    own(): string {
        return "mid-own";
    }
    constructor() {
        super(...[1, 2, 3]); // spread super() — three arguments
    }
}

class Leaf extends Mid {
    own2(): string {
        return "leaf-own";
    }
    constructor() {
        super(...[9, 8, 7, 6, 5]); // spread super() — five, over ARGUMENT_SLOTS
    }
}

const leaf = new Leaf();
const mid = new Mid();

// If a spreading `super()` had corrupted new.target to `Base`, `leaf` would
// inherit from `Base.prototype` and `leaf instanceof Leaf` would read false,
// and `leaf.own2()` (only on `Leaf.prototype`) would be missing.
print(`${leaf instanceof Leaf}`); // true
print(`${leaf instanceof Mid}`); // true
print(`${leaf instanceof Base}`); // true
print(leaf.own2()); // leaf-own — only reachable via Leaf.prototype
print(leaf.own()); // mid-own — only reachable via Mid.prototype
print(`${leaf.a}`); // 6 — Mid's own constructor ignores what Leaf passed it
print(`${mid instanceof Mid}`); // true
print(`${mid instanceof Leaf}`); // false — mid is not a Leaf
print(mid.own()); // mid-own
print(`${mid.a}`); // 6

describe("fixture:new_target_super_spread", () => {
    test("the prototype chain new.target controls survives a spreading super(), matching node --experimental-strip-types", () => {
        expect(__rtsCapturedOutput).toBe(
            "true\ntrue\ntrue\nleaf-own\nmid-own\n6\ntrue\nfalse\nmid-own\n6\n"
        );
    });
});
