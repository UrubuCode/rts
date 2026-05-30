import { describe, test, expect } from "rts:test";

// (#310) Override de console.* com callback REST (`...args`). Apos
// expand_rest_args a lifted fn espera 1 param (o array). O call site
// detecta variadic (LIFTED_VARIADIC) e empacota os args num unico array
// antes de INVOKE_AUTO. Antes o arrow recebia args individuais -> "null".
const ev: string[] = [];
const origGroup = console.group;
const origEnd = console.groupEnd;
(console as any).group = (...args: any[]) => { ev.push("g:" + args.join(",")); };
(console as any).groupEnd = () => { ev.push("end"); };
console.group("x");
console.group("a", "b", "c");
console.groupEnd();
(console as any).group = origGroup;
(console as any).groupEnd = origEnd;
const result = ev.join("|");

describe("console_override_variadic (#310)", () => {
  test("rest callback recebe args empacotados", () =>
    expect(result).toBe("g:x|g:a,b,c|end"));
});
