import { describe, test, expect } from "rts:test";

// (#311/#312) Reatribuir um metodo de console em runtime
// (`(console as any).table = fn`) agora dispara o handle custom em vez do
// builtin nativo. O codegen mantem um side-table runtime CONSOLE_OVERRIDES:
// o assignment grava; o call site checa antes do nativo. Callbacks de
// aridade FIXA (1-2 params) funcionam; rest-param (...args) ainda usa o
// nativo (precisa flag variadic no FunctionData — follow-up #310).

// table: 1 arg (espelha fixture cross-runtime 311_console_table)
const tcalls: string[] = [];
const origTable = console.table;
(console as any).table = (arg: any) => {
  tcalls.push(Array.isArray(arg) ? String(arg.length) : typeof arg);
};
console.table([{ a: 1 }, { a: 2 }]);
(console as any).table = origTable;
const tableResult = tcalls.join("|");

// dir: 2 args (espelha 312_console_dir)
const dcalls: string[] = [];
const origDir = console.dir;
(console as any).dir = (arg: any, opts: any) => {
  dcalls.push(typeof arg + ":" + String(opts?.depth));
};
console.dir({ a: { b: 1 } }, { depth: 1 });
(console as any).dir = origDir;
const dirResult = dcalls.join("|");

describe("console_method_override (#311/#312)", () => {
  test("override table dispara handle custom", () => expect(tableResult).toBe("2"));
  test("override dir recebe arg + opts", () => expect(dirResult).toBe("object:1"));
});
