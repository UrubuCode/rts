import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
    __rtsCapturedOutput += value + "\n";
}

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
    const seen: string[] = [];
    outer: for await (const x of asyncRange(3)) {
        for await (const y of asyncRange(3)) {
            if (y === 1) continue outer;
            seen.push(`${x},${y}`);
        }
    }
    print(seen.join("|"));
}

describe("fixture:for_await_labelled", () => {
    test("a labelled `for await` reaches the right frame for continue", async () => {
        await main();
        expect(__rtsCapturedOutput).toBe("0,0|1,0|2,0\n");
    });
});
