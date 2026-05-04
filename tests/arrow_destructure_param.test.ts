import { describe, test, expect } from "rts:test";

// (#568) Arrow callback com destructure no param: forEach/map.
// reduce com destructure no 2o param tem partial coverage — issue separada.

const pairs: [number, number][] = [[1, 10], [2, 20], [3, 30]];

const sums = pairs.map(([a, b]) => a + b);

let collected: number = 0;
function add(n: number): void { collected += n; }
pairs.forEach(([a, b]) => add(a + b));

describe("arrow_destructure_param", () => {
  test("map com destructure (#568)", () => expect(sums.join(",")).toBe("11,22,33"));
  test("forEach com destructure (#568)", () => expect(collected).toBe(66));
});
