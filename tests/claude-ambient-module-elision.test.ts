import { describe, test, expect } from "rts:test";

// An AMBIENT module or namespace declaration is type-only, and TypeScript
// erases it whole — body included. These four forms were refused at compile
// time ("a string-named TypeScript module", "a `declare global`
// augmentation") until #2901, which is why no fixture in this suite wrote
// one: zero of the ~990 `tests/*.ts` mentioned `declare module` before this
// file.
//
// What each case pins is ERASURE and not merely acceptance: a body lowered
// instead of erased would bind its members at run time, which is the shape of
// a partial elision that looks like it works.
//
// The reference is bun 1.4.0, measured 2026-10-03: it prints
// `ReferenceError ReferenceError undefined undefined object 7` for exactly
// this file's bindings, which is what the tests below assert. Node 22.23.2
// does NOT agree and cannot: its strip-only TypeScript refuses `namespace`
// and the `module` keyword outright (`ERR_UNSUPPORTED_TYPESCRIPT_SYNTAX`),
// so it rejects the file rather than answering differently. Node agrees on
// the string-named and `declare global` forms taken alone.

declare module "qualquer" {
  interface I {
    n: number;
  }
}

// The real-world shape: a package augmenting an interface it does not own,
// which is how `kire`'s four plugin packages extend the `Kire` class.
declare module "./base" {
  interface Alvo {
    b: string;
  }
}

declare module "mod" {
  export function f(): void;
  export const n: number;
}

// Both bodiless forms: a module declared opaque, and one declared empty.
declare module "opaco";
declare module "vazio" {}

declare global {
  interface Window {
    meu: string;
  }
}

declare namespace AmbienteN {
  const n: number;
}

declare module AmbienteLegado {
  export const n: number;
}

// A plain `namespace` carries no `declare`, so it is NOT ambient and still
// builds its object. Pinned here beside the ambient forms because one rule
// now decides between them.
namespace Vivo {
  export const n: number = 7;
}

function unbound(name: string): string {
  try {
    eval(name);
    return "bound";
  } catch (error) {
    return (error as Error).name;
  }
}

describe("fixture:claude-ambient-module-elision", () => {
  test("a declared module's members are not bound at run time", () => {
    // `export const n` inside `declare module "mod"` declares a type and
    // nothing else: reading `n` throws exactly as it does in node.
    expect(unbound("n")).toBe("ReferenceError");
    expect(unbound("f")).toBe("ReferenceError");
  });

  test("`declare global` creates nothing", () => {
    expect((globalThis as any).meu).toBe(undefined);
  });

  test("an ambient namespace is erased, object and all", () => {
    expect(typeof AmbienteN).toBe("undefined");
    expect(typeof AmbienteLegado).toBe("undefined");
  });

  test("a non-ambient namespace still runs", () => {
    expect(typeof Vivo).toBe("object");
    expect(Vivo.n).toBe(7);
  });
});
