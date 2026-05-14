import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// Promise.all com promises ja resolvidas (sync path).
const p1 = Promise.resolve(10);
const p2 = Promise.resolve(20);

const allH = Promise.all([p1, p2]);
const allWaited = promise.wait(allH as unknown as number);
print("all_handle=" + (allWaited !== 0));

// Promise.race retorna o valor da primeira.
const raceH = Promise.race([p1, p2]);
const raceVal = promise.wait(raceH as unknown as number);
print("race=" + raceVal);

// Promise.any tambem.
const anyH = Promise.any([p1, p2]);
const anyVal = promise.wait(anyH as unknown as number);
print("any=" + anyVal);

// Promise.allSettled — retorna Vec de descriptors com {status, value/reason}.
const settledH = Promise.allSettled([p1, p2]);
const settledHandle = promise.wait(settledH as unknown as number);
print("settled_handle=" + (settledHandle !== 0));

describe("Promise.all/race/any/allSettled static methods (#779 / #806)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "all_handle=true\n" +
      "race=10\n" +
      "any=10\n" +
      "settled_handle=true\n"
    )
  );
});
