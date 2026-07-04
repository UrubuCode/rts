// (#377) Cobertura formal de setTimeout/setInterval/setImmediate +
// clearTimeout/clearInterval/clearImmediate.

import { describe, test, expect } from "rts:test";
import { time } from "rts";

// 1. setTimeout dispara apos delay
let fired1 = 0;
setTimeout(() => { fired1 = fired1 + 1; }, 10);
time.sleep_ms(50);
const fired1_end = fired1;

// 2. clearTimeout cancela o callback
let fired2 = 0;
const h2 = setTimeout(() => { fired2 = fired2 + 1; }, 30);
clearTimeout(h2);
time.sleep_ms(50);
const fired2_end = fired2;

// 3. setInterval dispara periodicamente. Janela LARGA (100ms / 15ms ~ 6
// fires esperados) com bounds folgados nos DOIS lados: um runner de CI
// lento (macos) pausa a VM e derruba a contagem — `> 2` em 70ms flakava.
let count3 = 0;
const h3 = setInterval(() => { count3 = count3 + 1; }, 15);
time.sleep_ms(100);
clearInterval(h3);
time.sleep_ms(30);
const count3_end = count3;

// 4. setImmediate (mesmo que setTimeout 0)
let fired4 = 0;
setImmediate(() => { fired4 = fired4 + 1; });
time.sleep_ms(20);
const fired4_end = fired4;

// 5. setTimeout retorna handle nao-zero
const h5 = setTimeout(() => {}, 1000);
const h5_nonzero = h5 !== 0;
clearTimeout(h5);

// 6. clearTimeout em handle invalido nao crasha
clearTimeout(0);
clearInterval(0);

describe("set_timeout_interval", () => {
    test("setTimeout fires after delay", () => expect(fired1_end).toBe(1));
    test("clearTimeout prevents fire", () => expect(fired2_end).toBe(0));
    test("setInterval fires periodically", () =>
        expect(count3_end > 1).toBe(true));
    test("clearInterval stops interval", () =>
        expect(count3_end < 12).toBe(true));
    test("setImmediate fires", () => expect(fired4_end).toBe(1));
    test("setTimeout returns non-zero handle", () =>
        expect(h5_nonzero).toBe(true));
});
