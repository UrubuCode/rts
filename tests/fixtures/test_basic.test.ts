import { describe, test, expect } from "rts:test";

describe("math", () => {
    test("addition", () => {
        expect(`${1 + 1}`).toBe("2");
    });

    test("string equality", () => {
        expect("hello").toBe("hello");
    });
});
