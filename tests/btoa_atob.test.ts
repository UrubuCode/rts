import { describe, test, expect } from "rts:test";

let out = "";

// (#372) btoa/atob roundtrip
const encoded = btoa("hello");
out += encoded + "\n";              // aGVsbG8=
out += atob(encoded) + "\n";        // hello

// String mais longa
const long = btoa("Hello, World!");
out += long + "\n";                 // SGVsbG8sIFdvcmxkIQ==
out += atob(long) + "\n";           // Hello, World!

describe("btoa_atob", () => {
  test("roundtrip + acceptance #372", () => expect(out).toBe(
    "aGVsbG8=\nhello\nSGVsbG8sIFdvcmxkIQ==\nHello, World!\n"
  ));
});
