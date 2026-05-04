import { describe, test, expect } from "rts:test";

let out = "";

// (#220) Date setters — mutate em-place.
const d = new Date(0);
d.setFullYear(2024);
out += d.getUTCFullYear() + "\n";  // 2024

d.setMonth(5);
out += d.getUTCMonth() + "\n";     // 5

d.setDate(15);
out += d.getUTCDate() + "\n";      // 15

d.setHours(10);
out += d.getUTCHours() + "\n";     // 10

d.setMinutes(30);
out += d.getUTCMinutes() + "\n";   // 30

d.setSeconds(45);
out += d.getUTCSeconds() + "\n";   // 45

// setTime substitui tudo
d.setTime(0);
out += d.getUTCFullYear() + "\n";  // 1970

describe("date_setters", () => {
  test("setX mutate Entry::DateMs (#220)", () => expect(out).toBe(
    "2024\n5\n15\n10\n30\n45\n1970\n"
  ));
});
