// (#602) Optional chaining em obj literals aninhados — 3+ niveis.

import { describe, test, expect } from "rts:test";

// 1. 2 niveis (regressao do que ja' funcionava)
const a2 = { server: { host: "localhost" } };
const r2: string = a2?.server?.host;

// 2. 3 niveis
const a3 = { db: { conn: { url: "postgres://x" } } };
const r3: string = a3?.db?.conn?.url;

// 3. 3 niveis com numero na folha
const n3 = { app: { srv: { port: 8080 } } };
const rn3: i64 = n3?.app?.srv?.port;

// 4. Mix opt + regular
const m4 = { a: { b: { c: 42 } } };
const rm4: i64 = m4.a?.b.c;

// 5. Acesso via var intermediaria (caso ja' funcionava)
const a5 = { srv: { hostname: "alpha" } };
const s5 = a5?.srv;
const rs5: string = s5?.hostname;

// 6. Encadeamento sem optional
const r6: string = a3.db.conn.url;

// 7. 4 niveis
const a7 = { l1: { l2: { l3: { v: "deep" } } } };
const r7: string = a7?.l1?.l2?.l3?.v;

// 8. Nivel intermediario inexistente — fallback 0
const a8: any = { x: 1 };
const r8 = a8?.notExist?.deep;
const r8IsZero = (r8 as any) === 0;

describe("optchain_nested", () => {
    test("2 levels", () => expect(r2).toBe("localhost"));
    test("3 levels string", () => expect(r3).toBe("postgres://x"));
    test("3 levels number", () => expect(rn3).toBe(8080));
    test("mix opt + regular", () => expect(rm4).toBe(42));
    test("via intermediate var", () => expect(rs5).toBe("alpha"));
    test("non-opt 3 levels", () => expect(r6).toBe("postgres://x"));
    test("4 levels", () => expect(r7).toBe("deep"));
    test("missing intermediate → 0", () => expect(r8IsZero).toBe(true));
});
