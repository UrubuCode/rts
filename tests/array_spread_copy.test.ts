import { describe, test, expect } from "rts:test";

// `[...xs]` is a COPY, wherever it is written and whatever `xs` is. It was the
// array itself for one commit (2c47de0b1): a shortcut sound for the argument
// list of a call, which the door copies from, sat on the path an array literal
// also builds through, and a push on the copy grew the original. Nothing in the
// suite had that program. This does.

function copyOf(a: number[]): number[] { return [...a]; }

describe("an array literal that spreads another array", () => {
  test("is a fresh array, at the top level and inside a function", () => {
    const xs = [1, 2, 3];
    const ys = [...xs];
    expect(ys === xs).toBe(false);
    ys.push(9);
    ys[0] = 100;
    expect(xs.join()).toBe("1,2,3");
    expect(ys.join()).toBe("100,2,3,9");
    const zs = copyOf(xs);
    zs.pop();
    expect(xs.length).toBe(3);
    expect(zs.length).toBe(2);
    let total = 0;
    for (let i = 0; i < 1000; i++) { const c = [...xs]; c[1] = i; total += c[1] + xs[1]; }
    expect(total).toBe(499500 + 2000);
    const around = [0, ...xs, 4];
    expect(around.join()).toBe("0,1,2,3,4");
    const holes = [1, , 3] as number[];
    const filled = [...holes];
    expect(1 in filled).toBe(true);
    expect(filled[1]).toBe(undefined);
  });
  test("a call's spread still reaches the callee without a copy being seen", () => {
    const xs = [1, 2, 3];
    const sum = (...r: number[]) => { r.push(0); return r.reduce((a, b) => a + b, 0) + r.length * 100; };
    expect(sum(...xs)).toBe(406);
    expect(xs.length).toBe(3);
  });
});
