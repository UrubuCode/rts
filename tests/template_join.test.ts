import { describe, test, expect } from "rts:test";

// A template of up to six substitutions is joined in one crossing, whatever
// the holes hold; wider ones fold through `+`. What this pins is the LANGUAGE's
// answer either way: the string hint (toString before valueOf), source order of
// the conversions, a symbol refused, a string hole answered as itself, and the
// text of numbers a join must spell exactly as `String()` does.

describe("template literals joined in one crossing", () => {
  test("numbers, strings, booleans and nullish holes spell as String() would", () => {
    const n = 42, f = 0.1 + 0.2, s = "abc", b = true, u = undefined, z = null;
    expect(`${n}`).toBe("42");
    expect(`${f}`).toBe(String(f));
    expect(`x${s}y`).toBe("xabcy");
    expect(`${s}`).toBe(s);
    expect(`${b}|${u}|${z}`).toBe("true|undefined|null");
    expect(`${-0}|${NaN}|${Infinity}|${1e21}|${2 ** 53}`).toBe("0|NaN|Infinity|1e+21|9007199254740992");
    expect(`${1n}${2n}`).toBe("12");
  });
  test("four, five and six holes join; seven still answer the same text", () => {
    const a = 1, b = "two", c = 3, d = "four", e = 5, f = "six", g = 7;
    expect(`${a}a${b}b${c}c${d}`).toBe("1atwob3cfour");
    expect(`${a}${b}${c}${d}${e}`).toBe("1two3four5");
    expect(`<${a}|${b}|${c}|${d}|${e}|${f}>`).toBe("<1|two|3|four|5|six>");
    expect(`${a}${b}${c}${d}${e}${f}${g}`).toBe("1two3four5six7");
    let acc = 0;
    for (let i = 0; i < 300; i++) acc += `${i}a${i}b${i}c${i}d${i}e${i}`.length;
    expect(acc).toBe(300 * 5 + 6 * (10 * 1 + 90 * 2 + 200 * 3));
  });
  test("an object converts with the STRING hint and in source order", () => {
    const log: string[] = [];
    const o = {
      toString() { log.push("toString"); return "T"; },
      valueOf() { log.push("valueOf"); return 42; },
    };
    expect(`${o}`).toBe("T");
    expect(`a${o}b${o}c`).toBe("aTbTc");
    expect(log.join()).toBe("toString,toString,toString");
    log.length = 0;
    const first = { toString() { log.push("first"); return "1"; } };
    const second = { toString() { log.push("second"); return "2"; } };
    expect(`${first}-${second}-${first}`).toBe("1-2-1");
    expect(log.join()).toBe("first,second,first");
    const arr = [1, 2, 3];
    expect(`${arr}`).toBe("1,2,3");
    expect(`${{}}`).toBe("[object Object]");
    const sym = { [Symbol.toPrimitive](hint: string) { return hint; } };
    expect(`${sym}`).toBe("string");
  });
  test("a hole that cannot become text raises, and a throwing toString propagates", () => {
    let message = "";
    try { `${Symbol("s")}`; } catch (e: any) { message = e.message; }
    expect(message.includes("Symbol")).toBe(true);
    const n = 1;
    let mixed = "";
    try { `${n}${Symbol("s")}${n}`; } catch (e: any) { mixed = e.message; }
    expect(mixed.includes("Symbol")).toBe(true);
    const boom = { toString() { throw new Error("nope"); } };
    let caught = "";
    try { `${n}${boom}`; } catch (e: any) { caught = e.message; }
    expect(caught).toBe("nope");
  });
  test("long answers and wide text join too", () => {
    const wide = "héllo → ∑";
    const long = "abcdefghijklmnopqrstuvwxyz";
    expect(`${wide}|${long}|${long}`).toBe(wide + "|" + long + "|" + long);
    expect(`${long}${long}`.length).toBe(52);
    expect(`${wide}${1}`).toBe(wide + "1");
    expect(`\u{1F600}${wide}`.length).toBe(2 + wide.length);
  });
  test("String() over a string is the same string, and over others a fresh one", () => {
    const s = "same";
    expect(String(s)).toBe(s);
    expect(String(12)).toBe("12");
    expect(String({ toString: () => "o" })).toBe("o");
    expect(`${s}` === s).toBe(true);
  });
});
