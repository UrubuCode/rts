import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
    __rtsCapturedOutput += value + "\n";
}

// A hand-authored async iterable — no async generator, no Node stream,
// exactly the general protocol `for await` is specified over: an object with
// `[Symbol.asyncIterator]()` returning something with `.next()` answering a
// Promise of `{ value, done }`.
function asyncRange(n: number) {
    return {
        [Symbol.asyncIterator]() {
            let i = 0;
            return {
                next(): Promise<{ value: number; done: boolean }> {
                    if (i < n) {
                        const value = i;
                        i++;
                        return Promise.resolve({ value, done: false });
                    }
                    return Promise.resolve({ value: undefined as any, done: true });
                },
            };
        },
    };
}

async function main() {
    const seen: number[] = [];
    for await (const x of asyncRange(5)) {
        seen.push(x * 10);
    }
    print(seen.join(","));

    const partial: number[] = [];
    for await (const x of asyncRange(10)) {
        if (x === 3) break;
        partial.push(x);
    }
    print(partial.join(","));

    // `for await (existingName of xs)` — assigning to an existing binding
    // rather than declaring a fresh one — is a stated, separate gap: see
    // `emit/for_await.rs`'s refusal and its comment. Not exercised here.
}

describe("fixture:for_await_protocol", () => {
    test("steps a hand-authored [Symbol.asyncIterator], matching node --experimental-strip-types", async () => {
        await main();
        expect(__rtsCapturedOutput).toBe("0,10,20,30,40\n0,1,2\n");
    });
});
